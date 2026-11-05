use crate::crash::{analyze_crash_logs, CrashAnalysisReport};
use crate::loaders::prepare_loader_patch;
use crate::vanilla::{
    evaluate_rules, library_dedup_key, prepare_vanilla_files, resolve_vanilla_version_json,
    MojangRule, VanillaVersionJson,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use thk_auth::{
    ensure_authlib_injector, ensure_valid_minecraft_token, start_local_yggdrasil,
    LocalYggdrasilHandle,
};
use thk_core::models::{Account, AccountType, DownloadProgress, GameProfile, LoaderType};
use thk_core::{AppPaths, Database, DownloadManager, ThkError, ThkResult};
use thk_java::{ensure_mojang_java, probe_java_binary, RequiredJavaSpec};
use thk_mods::ensure_fabric_performance_mods;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const MAX_LOG_LINES_PER_INSTANCE: usize = 4_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstanceKey {
    pub profile_id: String,
    pub account_id: String,
}

impl InstanceKey {
    pub fn new(profile_id: impl Into<String>, account_id: impl Into<String>) -> Self {
        Self {
            profile_id: profile_id.into(),
            account_id: account_id.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub profile: GameProfile,
    pub account: Account,
    pub quick_play_server: Option<String>,
    pub custom_ms_client_id: Option<String>,
    pub high_process_priority: bool,
}

#[derive(Debug, Clone)]
pub enum LauncherEvent {
    Progress {
        key: InstanceKey,
        progress: DownloadProgress,
    },
    LogLine {
        key: InstanceKey,
        line: String,
    },
    Started {
        key: InstanceKey,
        pid: u32,
    },
    Exited {
        key: InstanceKey,
        exit_code: i32,
        elapsed_secs: u64,
        crash_report: Option<CrashAnalysisReport>,
    },
    LaunchFailed {
        key: InstanceKey,
        error: String,
    },
}

#[derive(Debug, Clone)]
pub struct RunningInstanceSnapshot {
    pub key: InstanceKey,
    pub pid: Option<u32>,
    pub uptime_secs: u64,
}

struct RunningInstanceEntry {
    pub pid: Option<u32>,
    pub started_at: Instant,
    pub stop_signal: CancellationToken,
    pub logs: Arc<Mutex<VecDeque<String>>>,
    pub _yggdrasil: Option<LocalYggdrasilHandle>,
}

#[derive(Clone)]
pub struct InstanceSupervisor {
    paths: AppPaths,
    db: Arc<Database>,
    downloader: DownloadManager,
    running: Arc<RwLock<HashMap<InstanceKey, RunningInstanceEntry>>>,
    events_tx: mpsc::UnboundedSender<LauncherEvent>,
}

impl InstanceSupervisor {
    pub fn new(
        paths: AppPaths,
        db: Arc<Database>,
        downloader: DownloadManager,
    ) -> (Self, mpsc::UnboundedReceiver<LauncherEvent>) {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let sup = Self {
            paths,
            db,
            downloader,
            running: Arc::new(RwLock::new(HashMap::new())),
            events_tx,
        };
        (sup, events_rx)
    }

    pub fn is_running(&self, key: &InstanceKey) -> bool {
        self.running
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(key)
    }

    pub fn running_keys(&self) -> Vec<InstanceKey> {
        self.running
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect()
    }

    pub fn running_snapshots(&self) -> Vec<RunningInstanceSnapshot> {
        let now = Instant::now();
        self.running
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(k, entry)| RunningInstanceSnapshot {
                key: k.clone(),
                pid: entry.pid,
                uptime_secs: now.duration_since(entry.started_at).as_secs(),
            })
            .collect()
    }

    pub fn get_logs(&self, key: &InstanceKey) -> Vec<String> {
        let guard = self.running.read().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = guard.get(key) else {
            return Vec::new();
        };
        let logs = entry.logs.lock().unwrap_or_else(|e| e.into_inner());
        logs.iter().cloned().collect()
    }

    pub fn stop_instance(&self, key: &InstanceKey) -> bool {
        let guard = self.running.read().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = guard.get(key) {
            entry.stop_signal.cancel();
            return true;
        }
        false
    }

    pub fn launch_async(&self, req: LaunchRequest) -> ThkResult<CancellationToken> {
        let key = InstanceKey::new(req.profile.id.clone(), req.account.id.clone());
        if self.is_running(&key) {
            return Err(ThkError::Process(format!(
                "Profile '{}' đang chạy dưới tài khoản '{}' rồi.",
                req.profile.name, req.account.username
            )));
        }

        let cancel = CancellationToken::new();
        let sup = self.clone();
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            if let Err(err) = sup.execute_launch_pipeline(req, key.clone(), cancel_clone).await {
                let _ = sup.events_tx.send(LauncherEvent::LaunchFailed {
                    key,
                    error: err.to_string(),
                });
            }
        });

        Ok(cancel)
    }

    async fn execute_launch_pipeline(
        &self,
        mut req: LaunchRequest,
        key: InstanceKey,
        cancel: CancellationToken,
    ) -> ThkResult<()> {
        tokio::fs::create_dir_all(&req.profile.instance_path).await?;

        let emit_progress = {
            let tx = self.events_tx.clone();
            let k = key.clone();
            move |progress: DownloadProgress| {
                let _ = tx.send(LauncherEvent::Progress {
                    key: k.clone(),
                    progress,
                });
            }
        };

        let mc_access_token = ensure_valid_minecraft_token(
            &mut req.account,
            req.custom_ms_client_id.as_deref(),
        )
        .await?;
        if req.account.account_type == AccountType::Microsoft {
            let _ = self.db.upsert_account(&req.account);
        }

        let version_json = resolve_vanilla_version_json(
            &self.downloader,
            &self.paths,
            &req.profile.game_version,
        )
        .await?;

        let java_spec = RequiredJavaSpec::for_minecraft_version(
            &req.profile.game_version,
            version_json
                .java_version
                .as_ref()
                .and_then(|j| j.component.as_deref()),
            version_json
                .java_version
                .as_ref()
                .and_then(|j| j.major_version),
        );

        let java_install = match req.profile.java_path.as_deref().filter(|p| p.is_file()) {
            Some(custom_bin) => probe_java_binary(custom_bin, false)?,
            None => {
                ensure_mojang_java(
                    &self.downloader,
                    &self.paths.runtimes_dir,
                    &java_spec,
                    Some(cancel.clone()),
                    emit_progress.clone(),
                )
                .await?
            }
        };

        let mut vanilla = prepare_vanilla_files(
            &self.downloader,
            &self.paths,
            &version_json,
            Some(cancel.clone()),
            emit_progress.clone(),
        )
        .await?;

        let loader_patch = prepare_loader_patch(
            &self.downloader,
            &self.paths,
            req.profile.loader,
            &req.profile.game_version,
            req.profile.loader_version.as_deref(),
            &vanilla.client_jar,
            &java_install.path,
            Some(cancel.clone()),
            emit_progress.clone(),
        )
        .await?;

        if req.profile.loader == LoaderType::Fabric && req.profile.auto_perf_mods {
            let _ = ensure_fabric_performance_mods(
                &self.downloader,
                &self.db,
                &req.profile,
                Some(cancel.clone()),
                emit_progress.clone(),
            )
            .await;
        }

        let mut yggdrasil_handle = None;
        let mut authlib_jvm_arg = None;
        let has_offline_custom_textures = req.account.account_type == AccountType::Offline
            && (req.account.custom_skin_path.as_ref().is_some_and(|p| p.is_file())
                || req.account.custom_cape_path.as_ref().is_some_and(|p| p.is_file())
                || req.account.custom_elytra_path.as_ref().is_some_and(|p| p.is_file()));

        if has_offline_custom_textures {
            if let Ok(agent_jar) = ensure_authlib_injector(&self.downloader, &self.paths).await {
                if let Ok(handle) = start_local_yggdrasil(&req.account).await {
                    authlib_jvm_arg = Some(handle.jvm_agent_arg(&agent_jar));
                    yggdrasil_handle = Some(handle);
                }
            }
        }

        apply_windows_discrete_gpu_preference(&java_install.path).await;

        let mut include_client_jar = true;
        let mut main_class = vanilla.version_json.main_class.clone();
        let mut loader_jvm_args = Vec::new();
        let mut loader_game_args = Vec::new();
        let mut legacy_mc_args_override = None;

        if let Some(patch) = loader_patch {
            main_class = patch.main_class;
            include_client_jar = patch.include_client_jar_on_cp;
            loader_jvm_args = patch.jvm_args;
            loader_game_args = patch.game_args;
            legacy_mc_args_override = patch.legacy_minecraft_arguments;

            let loader_keys: HashSet<String> = patch
                .extra_libraries
                .iter()
                .map(|p| library_dedup_key(p))
                .collect();
            let filtered_vanilla: Vec<PathBuf> = vanilla
                .libraries
                .into_iter()
                .filter(|p| !loader_keys.contains(&library_dedup_key(p)))
                .collect();
            let mut merged = patch.extra_libraries;
            merged.extend(filtered_vanilla);
            vanilla.libraries = merged;
        }

        let full_args = build_full_jvm_and_game_args(
            &req.profile,
            &req.account,
            &mc_access_token,
            &vanilla,
            &main_class,
            include_client_jar,
            &loader_jvm_args,
            &loader_game_args,
            legacy_mc_args_override.as_deref(),
            authlib_jvm_arg.as_deref(),
            req.quick_play_server.as_deref(),
        );

        let mut cmd = Command::new(&java_install.path);
        cmd.args(&full_args)
            .current_dir(&req.profile.instance_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;
            let flags = if req.high_process_priority {
                CREATE_NO_WINDOW | ABOVE_NORMAL_PRIORITY_CLASS
            } else {
                CREATE_NO_WINDOW
            };
            cmd.creation_flags(flags);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| ThkError::Process(format!("Không thể khởi chạy JVM Minecraft: {e}")))?;

        let pid = child.id().unwrap_or(0);
        let logs_buf = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES_PER_INSTANCE)));
        let started_at = Instant::now();

        {
            let mut guard = self.running.write().unwrap_or_else(|e| e.into_inner());
            guard.insert(
                key.clone(),
                RunningInstanceEntry {
                    pid: child.id(),
                    started_at,
                    stop_signal: cancel.clone(),
                    logs: Arc::clone(&logs_buf),
                    _yggdrasil: yggdrasil_handle,
                },
            );
        }

        let _ = self
            .events_tx
            .send(LauncherEvent::Started { key: key.clone(), pid });

        if let Some(stdout) = child.stdout.take() {
            spawn_log_reader(stdout, Arc::clone(&logs_buf), self.events_tx.clone(), key.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_log_reader(stderr, Arc::clone(&logs_buf), self.events_tx.clone(), key.clone());
        }

        let exit_code = tokio::select! {
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                -1
            }
            status_res = child.wait() => {
                match status_res {
                    Ok(st) => st.code().unwrap_or(-1),
                    Err(_) => -1,
                }
            }
        };

        let elapsed_secs = started_at.elapsed().as_secs();
        let collected_logs: Vec<String> = {
            let mut guard = self.running.write().unwrap_or_else(|e| e.into_inner());
            guard.remove(&key);
            let l = logs_buf.lock().unwrap_or_else(|e| e.into_inner());
            l.iter().cloned().collect()
        };

        let crashed = exit_code != 0 && !cancel.is_cancelled();
        let _ = self
            .db
            .record_play_session(&req.profile.id, elapsed_secs, crashed);

        let crash_report = if crashed {
            Some(analyze_crash_logs(&collected_logs, exit_code))
        } else {
            None
        };

        let _ = self.events_tx.send(LauncherEvent::Exited {
            key,
            exit_code,
            elapsed_secs,
            crash_report,
        });

        Ok(())
    }
}

fn spawn_log_reader<R: tokio::io::AsyncRead + Unpin + Send + 'static>(
    reader: R,
    logs_buf: Arc<Mutex<VecDeque<String>>>,
    tx: mpsc::UnboundedSender<LauncherEvent>,
    key: InstanceKey,
) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            {
                let mut guard = logs_buf.lock().unwrap_or_else(|e| e.into_inner());
                if guard.len() >= MAX_LOG_LINES_PER_INSTANCE {
                    guard.pop_front();
                }
                guard.push_back(line.clone());
            }
            let _ = tx.send(LauncherEvent::LogLine {
                key: key.clone(),
                line,
            });
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_full_jvm_and_game_args(
    profile: &GameProfile,
    account: &Account,
    access_token: &str,
    vanilla: &crate::vanilla::PreparedVanillaAssets,
    main_class: &str,
    include_client_jar: bool,
    loader_jvm_args: &[String],
    loader_game_args: &[String],
    legacy_mc_args_override: Option<&str>,
    authlib_jvm_arg: Option<&str>,
    quick_play_server: Option<&str>,
) -> Vec<String> {
    let cp_sep = if cfg!(windows) { ";" } else { ":" };
    let mut cp_entries: Vec<String> = vanilla
        .libraries
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    if include_client_jar {
        cp_entries.push(vanilla.client_jar.display().to_string());
    }
    let classpath_str = cp_entries.join(cp_sep);

    let ram_mb = profile.ram_mb.max(1024);
    let mut args = vec![
        format!("-Xmx{ram_mb}M"),
        format!("-Xms{}M", (ram_mb / 2).clamp(512, 4096)),
    ];

    // Preserve BOTH user-configured JVM arguments AND loader JVM arguments!
    for token in profile.jvm_args.split_whitespace() {
        if !token.starts_with("-Xmx") && !token.starts_with("-Xms") {
            args.push(token.to_string());
        }
    }

    if let Some(agent_flag) = authlib_jvm_arg {
        args.push(agent_flag.to_string());
    }

    let mut vars = HashMap::new();
    vars.insert("natives_directory", vanilla.natives_dir.display().to_string());
    vars.insert("launcher_name", "TriHienKun-Laucher".to_string());
    vars.insert("launcher_version", "1.0.0".to_string());
    vars.insert("classpath", classpath_str.clone());
    vars.insert("classpath_separator", cp_sep.to_string());
    vars.insert("auth_player_name", account.username.clone());
    vars.insert("version_name", profile.game_version.clone());
    vars.insert("game_directory", profile.instance_path.display().to_string());
    vars.insert("assets_root", vanilla.assets_dir.display().to_string());
    vars.insert("game_assets", vanilla.assets_dir.display().to_string());
    vars.insert("assets_index_name", vanilla.asset_index_id.clone());
    vars.insert("auth_uuid", account.uuid.replace('-', ""));
    vars.insert("auth_access_token", access_token.to_string());
    vars.insert("clientid", "0".to_string());
    vars.insert("auth_xuid", "0".to_string());
    vars.insert(
        "user_type",
        if account.account_type == AccountType::Microsoft {
            "msa".to_string()
        } else {
            "legacy".to_string()
        },
    );
    vars.insert("version_type", "TriHienKun".to_string());
    vars.insert("user_properties", "{}".to_string());

    let custom_res = profile
        .window_width
        .filter(|&w| w >= 320)
        .zip(profile.window_height.filter(|&h| h >= 240));

    let mut features = HashMap::new();
    if let Some((w, h)) = custom_res {
        vars.insert("resolution_width", w.to_string());
        vars.insert("resolution_height", h.to_string());
        features.insert("has_custom_resolution".to_string(), true);
    }

    if let Some(ref v_args) = vanilla.version_json.arguments {
        for raw in &v_args.jvm {
            append_evaluated_arg(raw, &vars, &features, &mut args);
        }
    } else {
        #[cfg(target_os = "macos")]
        args.push("-XstartOnFirstThread".into());
        args.push(format!(
            "-Djava.library.path={}",
            vanilla.natives_dir.display()
        ));
        args.push("-cp".into());
        args.push(classpath_str.clone());
    }

    if !args.iter().any(|a| a == "-cp" || a == "-classpath") {
        args.push("-cp".into());
        args.push(classpath_str);
    }

    for l_jvm in loader_jvm_args {
        args.push(substitute_placeholders(l_jvm, &vars));
    }

    args.push(main_class.to_string());

    let legacy_tpl = legacy_mc_args_override
        .or(vanilla.version_json.minecraft_arguments.as_deref());

    if let Some(tpl) = legacy_tpl {
        for tok in tpl.split_whitespace() {
            args.push(substitute_placeholders(tok, &vars));
        }
    } else if let Some(ref v_args) = vanilla.version_json.arguments {
        for raw in &v_args.game {
            append_evaluated_arg(raw, &vars, &features, &mut args);
        }
    }

    for l_game in loader_game_args {
        args.push(substitute_placeholders(l_game, &vars));
    }

    if let Some((w, h)) = custom_res {
        if !args.iter().any(|a| a == "--width") {
            args.push("--width".into());
            args.push(w.to_string());
            args.push("--height".into());
            args.push(h.to_string());
        }
    }

    if profile.fullscreen && !args.iter().any(|a| a == "--fullscreen") {
        args.push("--fullscreen".into());
    }

    if let Some(addr) = quick_play_server.map(str::trim).filter(|s| !s.is_empty()) {
        if supports_quick_play(&vanilla.version_json) {
            args.push("--quickPlayMultiplayer".into());
            args.push(addr.to_string());
        } else {
            let (host, port) = match addr.rsplit_once(':') {
                Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h, p),
                _ => (addr, "25565"),
            };
            args.push("--server".into());
            args.push(host.to_string());
            args.push("--port".into());
            args.push(port.to_string());
        }
    }

    args
}

fn supports_quick_play(vj: &VanillaVersionJson) -> bool {
    let parts: Vec<u32> = vj
        .id
        .split(['.', '-'])
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();
    match parts.as_slice() {
        [first, ..] if *first >= 25 => true,
        [1, minor, ..] => *minor >= 20,
        _ => false,
    }
}

fn append_evaluated_arg(
    val: &Value,
    vars: &HashMap<&str, String>,
    features: &HashMap<String, bool>,
    out: &mut Vec<String>,
) {
    match val {
        Value::String(s) => out.push(substitute_placeholders(s, vars)),
        Value::Object(obj) => {
            if let Some(rules_val) = obj.get("rules") {
                if let Ok(rules) = serde_json::from_value::<Vec<MojangRule>>(rules_val.clone()) {
                    if !evaluate_rules(Some(&rules), features) {
                        return;
                    }
                }
            }
            if let Some(v) = obj.get("value") {
                match v {
                    Value::String(s) => out.push(substitute_placeholders(s, vars)),
                    Value::Array(arr) => {
                        for item in arr {
                            if let Some(s) = item.as_str() {
                                out.push(substitute_placeholders(s, vars));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn substitute_placeholders(input: &str, vars: &HashMap<&str, String>) -> String {
    let mut result = input.to_string();
    for (k, v) in vars {
        let needle = format!("${{{k}}}");
        if result.contains(&needle) {
            result = result.replace(&needle, v);
        }
    }
    result
}

#[cfg(windows)]
async fn apply_windows_discrete_gpu_preference(java_bin: &Path) {
    let java_str = java_bin.display().to_string().replace('/', "\\");
    let mut cmd = Command::new("reg");
    cmd.args([
        "add",
        r"HKCU\Software\Microsoft\DirectX\UserGpuPreferences",
        "/v",
        &java_str,
        "/t",
        "REG_SZ",
        "/d",
        "GpuPreference=2;",
        "/f",
    ]);
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.output().await;
}

#[cfg(not(windows))]
async fn apply_windows_discrete_gpu_preference(_java_bin: &Path) {}
