use crate::config_files::{is_eula_accepted, read_server_properties, write_eula_accepted};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use thk_core::models::{DownloadProgress, ManagedServer};
use thk_core::{AppPaths, DownloadManager, SafePath, ThkError, ThkResult};
use thk_java::{ensure_mojang_java, ensure_optimal_java, probe_java_binary, RequiredJavaSpec};
use thk_network::{
    start_bore_tunnel, start_direct_port_host, BoreTunnelHandle, DirectHostConfig,
    DirectHostStatus, DirectPortHostHandle, LanTunnelEvent, TunnelStatus,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::sync::{mpsc, Notify};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const MAX_SERVER_CONSOLE_LINES: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerRuntimeState {
    Offline,
    Starting,
    Online,
    Stopping,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerResourceStats {
    pub cpu_percent: f32,
    #[serde(default)]
    pub used_cores: f32,
    #[serde(default)]
    pub logical_cores: u32,
    #[serde(default)]
    pub allowed_cores: u32,
    #[serde(default)]
    pub cpu_model: String,
    pub ram_mb: u64,
    pub uptime_secs: u64,
    pub online_players: u32,
    pub max_players: u32,
    #[serde(default)]
    pub local_ping_ms: Option<u32>,
    #[serde(default)]
    pub public_ping_ms: Option<u32>,
    #[serde(default)]
    pub tps: Option<f32>,
    #[serde(default)]
    pub mspt: Option<f32>,
    #[serde(default)]
    pub online_player_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum ServerSupervisorEvent {
    StatusChanged {
        server_id: String,
        state: ServerRuntimeState,
    },
    LogLine {
        server_id: String,
        line: String,
    },
    StatsUpdated {
        server_id: String,
        stats: ServerResourceStats,
    },
    TunnelChanged {
        server_id: String,
        status: TunnelStatus,
    },
    DirectHostChanged {
        server_id: String,
        status: DirectHostStatus,
    },
    InstallProgress {
        server_id: String,
        progress: DownloadProgress,
    },
}

struct RunningServerHandle {
    state: Arc<RwLock<ServerRuntimeState>>,
    stats: Arc<RwLock<ServerResourceStats>>,
    logs: Arc<Mutex<VecDeque<String>>>,
    stdin_tx: mpsc::UnboundedSender<String>,
    silent_probe_until_ms: Arc<AtomicU64>,
    force_kill: CancellationToken,
    exited_notify: Arc<Notify>,
}

pub fn sanitize_console_command(raw: &str) -> String {
    let cleaned = raw.chars().filter(|c| !c.is_control()).collect::<String>();
    let trimmed = cleaned.trim();
    if let Some(without_slash) = trimmed.strip_prefix('/') {
        if !without_slash.starts_with('/') {
            return without_slash.trim_start().to_string();
        }
    }
    trimmed.to_string()
}

pub fn strip_console_ansi_and_noise(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\0' || c == '\r' {
            continue;
        }
        if c == '\x1b' {
            match chars.peek().copied() {
                Some('[') => {
                    let _ = chars.next();
                    while let Some(&ch) = chars.peek() {
                        if matches!(ch, '0'..='?' | ' '..='/') {
                            let _ = chars.next();
                        } else {
                            break;
                        }
                    }
                    if let Some(&final_byte) = chars.peek() {
                        if matches!(final_byte, '@'..='~') {
                            let _ = chars.next();
                        }
                    }
                }
                Some(']') => {
                    let _ = chars.next();
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' && chars.peek() == Some(&'\\') {
                            let _ = chars.next();
                            break;
                        }
                    }
                }
                Some(_) => {
                    let _ = chars.next();
                }
                None => {}
            }
            continue;
        }
        if !c.is_control() || c == '\t' {
            out.push(c);
        }
    }
    out
}

pub fn resolve_effective_server_port(server: &ManagedServer) -> u16 {
    read_server_properties(&server.server_dir)
        .ok()
        .and_then(|props| {
            props
                .get("server-port")
                .or_else(|| props.get("server.port"))
                .and_then(|v| v.trim().parse::<u16>().ok())
                .filter(|&p| p > 0)
        })
        .unwrap_or(server.port.max(1))
}

async fn is_local_port_open(port: u16) -> bool {
    timeout(
        Duration::from_millis(250),
        TcpStream::connect(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
    )
    .await
    .map(|res| res.is_ok())
    .unwrap_or(false)
}

#[derive(Clone)]
pub struct LocalServerSupervisor {
    paths: AppPaths,
    downloader: DownloadManager,
    running: Arc<RwLock<HashMap<String, RunningServerHandle>>>,
    tunnels: Arc<RwLock<HashMap<String, BoreTunnelHandle>>>,
    tunnel_generations: Arc<RwLock<HashMap<String, u64>>>,
    direct_hosts: Arc<RwLock<HashMap<String, DirectPortHostHandle>>>,
    direct_host_generations: Arc<RwLock<HashMap<String, u64>>>,
    prefer_optimal_java: Arc<AtomicBool>,
    events_tx: mpsc::UnboundedSender<ServerSupervisorEvent>,
}

impl LocalServerSupervisor {
    pub fn new(
        paths: AppPaths,
        downloader: DownloadManager,
    ) -> (Self, mpsc::UnboundedReceiver<ServerSupervisorEvent>) {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let sup = Self {
            paths,
            downloader,
            running: Arc::new(RwLock::new(HashMap::new())),
            tunnels: Arc::new(RwLock::new(HashMap::new())),
            tunnel_generations: Arc::new(RwLock::new(HashMap::new())),
            direct_hosts: Arc::new(RwLock::new(HashMap::new())),
            direct_host_generations: Arc::new(RwLock::new(HashMap::new())),
            prefer_optimal_java: Arc::new(AtomicBool::new(false)),
            events_tx,
        };
        (sup, events_rx)
    }

    pub fn set_prefer_optimal_java(&self, enabled: bool) {
        self.prefer_optimal_java.store(enabled, Ordering::Relaxed);
    }

    pub fn events_sender(&self) -> mpsc::UnboundedSender<ServerSupervisorEvent> {
        self.events_tx.clone()
    }

    pub fn runtime_state(&self, server_id: &str) -> ServerRuntimeState {
        self.running
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(server_id)
            .map(|h| *h.state.read().unwrap_or_else(|e| e.into_inner()))
            .unwrap_or(ServerRuntimeState::Offline)
    }

    pub fn latest_stats(&self, server_id: &str) -> Option<ServerResourceStats> {
        self.running
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(server_id)
            .map(|h| h.stats.read().unwrap_or_else(|e| e.into_inner()).clone())
    }

    pub fn console_logs(&self, server_id: &str) -> Vec<String> {
        let guard = self.running.read().unwrap_or_else(|e| e.into_inner());
        let Some(handle) = guard.get(server_id) else {
            return Vec::new();
        };
        let logs = handle.logs.lock().unwrap_or_else(|e| e.into_inner());
        logs.iter().cloned().collect()
    }

    pub async fn resolve_java_for_server(&self, server: &ManagedServer) -> ThkResult<PathBuf> {
        let spec = RequiredJavaSpec::for_minecraft_version(&server.game_version, None, None);
        if let Some(ref custom) = server.java_path {
            let installation = probe_java_binary(custom, false)?;
            if installation.major_version < spec.major_version {
                return Err(ThkError::Java(format!(
                    "Server '{}' requires Java {} or newer; selected Java is {}",
                    server.name, spec.major_version, installation.major_version
                )));
            }
            return Ok(custom.clone());
        }
        let emit_progress = {
            let tx = self.events_tx.clone();
            let sid = server.id.clone();
            move |progress: DownloadProgress| {
                let _ = tx.send(ServerSupervisorEvent::InstallProgress {
                    server_id: sid.clone(),
                    progress,
                });
            }
        };
        let inst = if self.prefer_optimal_java.load(Ordering::Relaxed) {
            ensure_optimal_java(
                &self.downloader,
                &self.paths.runtimes_dir,
                &spec,
                None,
                emit_progress,
            )
            .await?
        } else {
            ensure_mojang_java(
                &self.downloader,
                &self.paths.runtimes_dir,
                &spec,
                None,
                emit_progress,
            )
            .await?
        };
        Ok(inst.path)
    }

    pub async fn start_server(
        &self,
        server: &ManagedServer,
        auto_accept_eula: bool,
    ) -> ThkResult<()> {
        if self.runtime_state(&server.id) != ServerRuntimeState::Offline {
            return Err(ThkError::Process(format!(
                "Server '{}' is already running.",
                server.name
            )));
        }
        if server.jar_file.trim().is_empty() {
            return Err(ThkError::InvalidInput(
                "Server JAR has not been installed yet.".into(),
            ));
        }

        tokio::fs::create_dir_all(&server.server_dir).await?;
        if auto_accept_eula && !is_eula_accepted(&server.server_dir) {
            write_eula_accepted(&server.server_dir)?;
        }
        if !is_eula_accepted(&server.server_dir) {
            return Err(ThkError::InvalidInput(
                "Accept the Minecraft EULA before starting this server.".into(),
            ));
        }

        let effective_port = resolve_effective_server_port(server);
        if is_local_port_open(effective_port).await {
            return Err(ThkError::Process(format!(
                "Port {effective_port} is already in use on 127.0.0.1 (stop the existing server process or change server-port)."
            )));
        }

        let java_bin = self.resolve_java_for_server(server).await?;
        let launch_args = build_server_jvm_and_target_args(server)?;

        let mut cmd = Command::new(&java_bin);
        thk_java::configure_tokio_java_command_env(&mut cmd, &java_bin, Some(&server.server_dir));
        cmd.current_dir(&server.server_dir)
            .args(&launch_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);

        let mut child = cmd.spawn().map_err(|e| {
            ThkError::Process(format!(
                "Failed spawning server '{}' with {}: {e}",
                server.name,
                java_bin.display()
            ))
        })?;

        let child_pid = child.id();
        let child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| ThkError::Process("Failed acquiring server stdin pipe".into()))?;
        let child_stdout = child.stdout.take();
        let child_stderr = child.stderr.take();

        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();
        let state = Arc::new(RwLock::new(ServerRuntimeState::Starting));
        let host_cpu = crate::cpu::CpuHardwareInfo::host();
        let initial_allowed_cores = child_pid
            .map(|p| crate::cpu::read_process_allowed_cores(p, host_cpu.logical_cores))
            .unwrap_or(host_cpu.logical_cores);
        let initial_stats = ServerResourceStats {
            cpu_percent: 0.0,
            used_cores: 0.0,
            logical_cores: host_cpu.logical_cores,
            allowed_cores: initial_allowed_cores,
            cpu_model: host_cpu.short_model.clone(),
            ram_mb: 0,
            uptime_secs: 0,
            online_players: 0,
            max_players: 20,
            local_ping_ms: None,
            public_ping_ms: None,
            tps: None,
            mspt: None,
            online_player_names: Vec::new(),
        };
        let stats = Arc::new(RwLock::new(initial_stats.clone()));
        let logs = Arc::new(Mutex::new(VecDeque::with_capacity(
            MAX_SERVER_CONSOLE_LINES,
        )));
        let started_at = Instant::now();
        let silent_probe_until_ms = Arc::new(AtomicU64::new(0));
        let probe_cmd_state = Arc::new(Mutex::new(crate::telemetry::tps_probe_command_for_server(
            server.engine,
            &server.game_version,
        )));
        let probe_stdin_tx = stdin_tx.clone();
        let force_kill = CancellationToken::new();
        let exited_notify = Arc::new(Notify::new());

        {
            let mut guard = self.running.write().unwrap_or_else(|e| e.into_inner());
            guard.insert(
                server.id.clone(),
                RunningServerHandle {
                    state: Arc::clone(&state),
                    stats: Arc::clone(&stats),
                    logs: Arc::clone(&logs),
                    stdin_tx,
                    silent_probe_until_ms: Arc::clone(&silent_probe_until_ms),
                    force_kill: force_kill.clone(),
                    exited_notify: Arc::clone(&exited_notify),
                },
            );
        }

        let _ = self.events_tx.send(ServerSupervisorEvent::StatusChanged {
            server_id: server.id.clone(),
            state: ServerRuntimeState::Starting,
        });
        let _ = self.events_tx.send(ServerSupervisorEvent::StatsUpdated {
            server_id: server.id.clone(),
            stats: initial_stats,
        });

        tokio::spawn(async move {
            let mut writer = child_stdin;
            while let Some(cmd_line) = stdin_rx.recv().await {
                let payload = format!("{cmd_line}\n");
                if writer.write_all(payload.as_bytes()).await.is_err() {
                    break;
                }
                if writer.flush().await.is_err() {
                    break;
                }
            }
        });

        let metrics_cancel = CancellationToken::new();
        if let Some(pid_u32) = child_pid {
            let metrics_stop = metrics_cancel.clone();
            let stats_ref = Arc::clone(&stats);
            let tx_metrics = self.events_tx.clone();
            let srv_id = server.id.clone();

            tokio::spawn(async move {
                let Ok(mut sys) = System::new() else {
                    return;
                };
                let pid = Pid::from_u32(pid_u32);
                let refresh_kind = ProcessRefreshKind::nothing().with_cpu().with_memory();
                sys.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&[pid]),
                    true,
                    refresh_kind,
                );
                let mut prev_sample_at = Instant::now();
                let mut prev_cpu_ms = sys
                    .process(pid)
                    .map(|p| p.accumulated_cpu_time())
                    .unwrap_or(0);
                let mut first_tick = true;

                loop {
                    let delay = if first_tick {
                        first_tick = false;
                        Duration::from_millis(500)
                    } else {
                        Duration::from_secs(1)
                    };
                    tokio::select! {
                        _ = metrics_stop.cancelled() => break,
                        _ = tokio::time::sleep(delay) => {}
                    }

                    sys.refresh_processes_specifics(
                        ProcessesToUpdate::Some(&[pid]),
                        true,
                        refresh_kind,
                    );
                    let now = Instant::now();
                    let elapsed_ms = now.duration_since(prev_sample_at).as_secs_f64() * 1000.0;

                    if let Some(proc_info) = sys.process(pid) {
                        let current_cpu_ms = proc_info.accumulated_cpu_time();
                        let delta_cpu_ms = current_cpu_ms.saturating_sub(prev_cpu_ms);
                        prev_sample_at = now;
                        prev_cpu_ms = current_cpu_ms;

                        let (cpu_percent, used_cores) = crate::cpu::compute_process_cpu_sample(
                            delta_cpu_ms,
                            elapsed_ms,
                            host_cpu.logical_cores,
                        );
                        let allowed_cores =
                            crate::cpu::read_process_allowed_cores(pid_u32, host_cpu.logical_cores);
                        let rss_mb = proc_info.memory() / (1024 * 1024);
                        let snapshot = {
                            let mut guard = stats_ref.write().unwrap_or_else(|e| e.into_inner());
                            guard.cpu_percent = cpu_percent;
                            guard.used_cores = used_cores;
                            guard.logical_cores = host_cpu.logical_cores;
                            guard.allowed_cores = allowed_cores;
                            if guard.cpu_model.is_empty() {
                                guard.cpu_model = host_cpu.short_model.clone();
                            }
                            guard.ram_mb = rss_mb;
                            guard.uptime_secs = started_at.elapsed().as_secs();
                            guard.clone()
                        };
                        let _ = tx_metrics.send(ServerSupervisorEvent::StatsUpdated {
                            server_id: srv_id.clone(),
                            stats: snapshot,
                        });
                    }
                }
            });
        }

        {
            let ping_stop = metrics_cancel.clone();
            let state_ref = Arc::clone(&state);
            let stats_ref = Arc::clone(&stats);
            let tunnels_ref = Arc::clone(&self.tunnels);
            let direct_hosts_ref = Arc::clone(&self.direct_hosts);
            let probe_cmd_ref = Arc::clone(&probe_cmd_state);
            let silent_until_ref = Arc::clone(&silent_probe_until_ms);
            let tx_ping = self.events_tx.clone();
            let srv_id = server.id.clone();

            tokio::spawn(async move {
                let mut tick_idx = 0u64;
                loop {
                    let delay = if tick_idx == 0 {
                        Duration::from_millis(1200)
                    } else {
                        Duration::from_secs(3)
                    };
                    tokio::select! {
                        _ = ping_stop.cancelled() => break,
                        _ = tokio::time::sleep(delay) => {}
                    }

                    let is_online = *state_ref.read().unwrap_or_else(|e| e.into_inner())
                        == ServerRuntimeState::Online;
                    if !is_online {
                        continue;
                    }

                    if tick_idx.is_multiple_of(3) {
                        let cmd_opt = *probe_cmd_ref.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(cmd_str) = cmd_opt {
                            let until_ms =
                                (started_at.elapsed().as_millis() as u64).saturating_add(1500);
                            silent_until_ref.store(until_ms, Ordering::Relaxed);
                            let _ = probe_stdin_tx.send(cmd_str.to_string());
                        }
                    }

                    let local_ep = format!("127.0.0.1:{effective_port}");
                    let local_ping_res =
                        thk_network::ping_minecraft_server(&local_ep, Duration::from_millis(1200))
                            .await;

                    let public_endpoint = tunnels_ref
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .get(&srv_id)
                        .map(BoreTunnelHandle::current_status)
                        .and_then(|st| match st {
                            TunnelStatus::Running { .. } => st.public_endpoint(),
                            _ => None,
                        })
                        .or_else(|| {
                            direct_hosts_ref
                                .read()
                                .unwrap_or_else(|e| e.into_inner())
                                .get(&srv_id)
                                .map(DirectPortHostHandle::current_status)
                                .and_then(|st| match st {
                                    DirectHostStatus::Running(info) => {
                                        info.preferred_online_endpoint().or_else(|| {
                                            info.primary_lan_endpoint().map(str::to_string)
                                        })
                                    }
                                    _ => None,
                                })
                        });

                    let prev_pub_ping = stats_ref
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .public_ping_ms;
                    let measured_pub_ping = if let Some(ref pub_ep) = public_endpoint {
                        if tick_idx.is_multiple_of(2) || prev_pub_ping.is_none() {
                            Some(
                                thk_network::ping_minecraft_server(
                                    pub_ep,
                                    Duration::from_millis(2500),
                                )
                                .await
                                .ok()
                                .and_then(|p| p.latency_ms)
                                .map(|v| v.min(u32::MAX as u64) as u32),
                            )
                        } else {
                            None
                        }
                    } else {
                        Some(None)
                    };

                    let updated_snapshot = {
                        let mut guard = stats_ref.write().unwrap_or_else(|e| e.into_inner());
                        if let Ok(ping) = local_ping_res {
                            guard.local_ping_ms =
                                ping.latency_ms.map(|v| v.min(u32::MAX as u64) as u32);
                            guard.online_players = ping.online_players;
                            guard.max_players = ping.max_players.max(1);
                            if ping.online_players == 0 {
                                guard.online_player_names.clear();
                            } else {
                                let valid_samples: Vec<String> = ping
                                    .sample_players
                                    .into_iter()
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| {
                                        !s.is_empty() && !s.eq_ignore_ascii_case("Anonymous Player")
                                    })
                                    .collect();
                                if valid_samples.len() == ping.online_players as usize {
                                    guard.online_player_names = valid_samples;
                                } else {
                                    for sample in valid_samples {
                                        if !guard
                                            .online_player_names
                                            .iter()
                                            .any(|n| n.eq_ignore_ascii_case(&sample))
                                        {
                                            guard.online_player_names.push(sample);
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(next_pub) = measured_pub_ping {
                            guard.public_ping_ms = next_pub;
                        }
                        guard.clone()
                    };

                    let _ = tx_ping.send(ServerSupervisorEvent::StatsUpdated {
                        server_id: srv_id.clone(),
                        stats: updated_snapshot,
                    });
                    tick_idx = tick_idx.wrapping_add(1);
                }
            });
        }

        let reader_ctx = OutputReaderShared {
            started_at,
            silent_probe_until_ms: Arc::clone(&silent_probe_until_ms),
            probe_cmd_state: Arc::clone(&probe_cmd_state),
            game_version: server.game_version.clone(),
        };
        if let Some(out) = child_stdout {
            spawn_server_output_reader(
                out,
                server.id.clone(),
                Arc::clone(&logs),
                Arc::clone(&state),
                Arc::clone(&stats),
                reader_ctx.clone(),
                force_kill.clone(),
                self.events_tx.clone(),
            );
        }
        if let Some(err) = child_stderr {
            spawn_server_output_reader(
                err,
                server.id.clone(),
                Arc::clone(&logs),
                Arc::clone(&state),
                Arc::clone(&stats),
                reader_ctx,
                force_kill.clone(),
                self.events_tx.clone(),
            );
        }

        let running_map = Arc::clone(&self.running);
        let sup_exit = self.clone();
        let srv_id_wait = server.id.clone();
        let tx_wait = self.events_tx.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = force_kill.cancelled() => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                }
                _ = child.wait() => {}
            }

            metrics_cancel.cancel();
            sup_exit.stop_server_direct_host(&srv_id_wait);
            running_map
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&srv_id_wait);
            let _ = tx_wait.send(ServerSupervisorEvent::StatusChanged {
                server_id: srv_id_wait,
                state: ServerRuntimeState::Offline,
            });
            exited_notify.notify_waiters();
        });

        Ok(())
    }

    pub fn send_command(&self, server_id: &str, raw_command: &str) -> ThkResult<()> {
        let cleaned = sanitize_console_command(raw_command);
        if cleaned.is_empty() {
            return Ok(());
        }
        let guard = self.running.read().unwrap_or_else(|e| e.into_inner());
        let handle = guard
            .get(server_id)
            .ok_or_else(|| ThkError::Process("Server is not currently running".into()))?;
        handle.silent_probe_until_ms.store(0, Ordering::Relaxed);
        handle
            .stdin_tx
            .send(cleaned)
            .map_err(|_| ThkError::Process("Server stdin stream is closed".into()))
    }

    pub async fn stop_server(&self, server_id: &str) -> ThkResult<()> {
        let (prev_state, stdin_tx, force_kill, exited_notify) = {
            let guard = self.running.read().unwrap_or_else(|e| e.into_inner());
            let Some(handle) = guard.get(server_id) else {
                return Ok(());
            };
            let prev = {
                let mut st = handle.state.write().unwrap_or_else(|e| e.into_inner());
                let prev = *st;
                *st = ServerRuntimeState::Stopping;
                prev
            };
            (
                prev,
                handle.stdin_tx.clone(),
                handle.force_kill.clone(),
                Arc::clone(&handle.exited_notify),
            )
        };

        let _ = self.events_tx.send(ServerSupervisorEvent::StatusChanged {
            server_id: server_id.to_string(),
            state: ServerRuntimeState::Stopping,
        });

        let notified = exited_notify.notified();
        let _ = stdin_tx.send("stop".to_string());

        let grace = match prev_state {
            ServerRuntimeState::Stopping => Duration::from_millis(100),
            ServerRuntimeState::Starting => Duration::from_secs(2),
            _ => Duration::from_secs(10),
        };
        if timeout(grace, notified).await.is_err() {
            force_kill.cancel();
            let _ = timeout(Duration::from_secs(3), exited_notify.notified()).await;
        }
        Ok(())
    }

    pub async fn restart_server(
        &self,
        server: &ManagedServer,
        auto_accept_eula: bool,
    ) -> ThkResult<()> {
        self.stop_server(&server.id).await?;
        self.start_server(server, auto_accept_eula).await
    }

    pub async fn start_server_tunnel(
        &self,
        server_id: &str,
        local_port: u16,
    ) -> ThkResult<BoreTunnelHandle> {
        let session_gen = {
            let mut gens = self
                .tunnel_generations
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let entry = gens.entry(server_id.to_string()).or_insert(0);
            *entry += 1;
            *entry
        };
        if let Some(old) = self
            .tunnels
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(server_id)
        {
            old.stop();
        }

        let (lan_tx, mut lan_rx) = mpsc::unbounded_channel::<LanTunnelEvent>();
        let srv_id = server_id.to_string();
        let out_tx = self.events_tx.clone();
        let gens_ref = Arc::clone(&self.tunnel_generations);
        tokio::spawn(async move {
            while let Some(ev) = lan_rx.recv().await {
                let current_gen = gens_ref
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&srv_id)
                    .copied()
                    .unwrap_or(0);
                if current_gen != session_gen {
                    break;
                }
                if let LanTunnelEvent::TunnelStatusChanged(status) = ev {
                    let _ = out_tx.send(ServerSupervisorEvent::TunnelChanged {
                        server_id: srv_id.clone(),
                        status,
                    });
                }
            }
        });

        let handle = start_bore_tunnel(local_port, None, None, Some(lan_tx)).await?;
        let still_current = self
            .tunnel_generations
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(server_id)
            .copied()
            .unwrap_or(0)
            == session_gen;
        if still_current {
            self.tunnels
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .insert(server_id.to_string(), handle.clone());
        } else {
            handle.stop();
        }
        Ok(handle)
    }

    pub fn stop_server_tunnel(&self, server_id: &str) {
        {
            let mut gens = self
                .tunnel_generations
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let entry = gens.entry(server_id.to_string()).or_insert(0);
            *entry += 1;
        }
        if let Some(handle) = self
            .tunnels
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(server_id)
        {
            handle.stop();
            let direct_active = matches!(
                self.server_direct_host_status(server_id),
                DirectHostStatus::Running(_)
            );
            if !direct_active {
                if let Some(srv_handle) = self
                    .running
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(server_id)
                {
                    let snap = {
                        let mut st = srv_handle.stats.write().unwrap_or_else(|e| e.into_inner());
                        st.public_ping_ms = None;
                        st.clone()
                    };
                    let _ = self.events_tx.send(ServerSupervisorEvent::StatsUpdated {
                        server_id: server_id.to_string(),
                        stats: snap,
                    });
                }
            }
            let _ = self.events_tx.send(ServerSupervisorEvent::TunnelChanged {
                server_id: server_id.to_string(),
                status: TunnelStatus::Stopped,
            });
        }
    }

    pub fn server_tunnel_status(&self, server_id: &str) -> TunnelStatus {
        self.tunnels
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(server_id)
            .map(BoreTunnelHandle::current_status)
            .unwrap_or(TunnelStatus::Idle)
    }

    pub async fn start_server_direct_host(
        &self,
        server_id: &str,
        config: DirectHostConfig,
    ) -> ThkResult<DirectPortHostHandle> {
        let session_gen = {
            let mut gens = self
                .direct_host_generations
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let entry = gens.entry(server_id.to_string()).or_insert(0);
            *entry += 1;
            *entry
        };
        if let Some(old) = self
            .direct_hosts
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(server_id)
        {
            old.stop();
        }

        let (host_tx, mut host_rx) = mpsc::unbounded_channel::<DirectHostStatus>();
        let srv_id = server_id.to_string();
        let out_tx = self.events_tx.clone();
        let gens_ref = Arc::clone(&self.direct_host_generations);
        tokio::spawn(async move {
            while let Some(status) = host_rx.recv().await {
                let current_gen = gens_ref
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&srv_id)
                    .copied()
                    .unwrap_or(0);
                if current_gen != session_gen {
                    break;
                }
                let _ = out_tx.send(ServerSupervisorEvent::DirectHostChanged {
                    server_id: srv_id.clone(),
                    status,
                });
            }
        });

        let handle = start_direct_port_host(config, Some(host_tx)).await?;
        let still_current = self
            .direct_host_generations
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(server_id)
            .copied()
            .unwrap_or(0)
            == session_gen;
        if still_current {
            self.direct_hosts
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .insert(server_id.to_string(), handle.clone());
        } else {
            handle.stop();
        }
        Ok(handle)
    }

    pub fn stop_server_direct_host(&self, server_id: &str) {
        {
            let mut gens = self
                .direct_host_generations
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let entry = gens.entry(server_id.to_string()).or_insert(0);
            *entry += 1;
        }
        if let Some(handle) = self
            .direct_hosts
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(server_id)
        {
            handle.stop();
            let tunnel_active = matches!(
                self.server_tunnel_status(server_id),
                TunnelStatus::Running { .. }
            );
            if !tunnel_active {
                if let Some(srv_handle) = self
                    .running
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(server_id)
                {
                    let snap = {
                        let mut st = srv_handle.stats.write().unwrap_or_else(|e| e.into_inner());
                        st.public_ping_ms = None;
                        st.clone()
                    };
                    let _ = self.events_tx.send(ServerSupervisorEvent::StatsUpdated {
                        server_id: server_id.to_string(),
                        stats: snap,
                    });
                }
            }
            let _ = self
                .events_tx
                .send(ServerSupervisorEvent::DirectHostChanged {
                    server_id: server_id.to_string(),
                    status: DirectHostStatus::Stopped,
                });
        }
    }

    pub fn server_direct_host_status(&self, server_id: &str) -> DirectHostStatus {
        self.direct_hosts
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(server_id)
            .map(DirectPortHostHandle::current_status)
            .unwrap_or(DirectHostStatus::Idle)
    }

    pub async fn shutdown_all(&self) {
        let active_ids: Vec<String> = self
            .running
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect();

        for srv_id in active_ids {
            let _ = self.stop_server(&srv_id).await;
        }

        let mut tunnels_guard = self.tunnels.write().unwrap_or_else(|e| e.into_inner());
        for (_, handle) in tunnels_guard.drain() {
            handle.stop();
        }

        let mut direct_guard = self.direct_hosts.write().unwrap_or_else(|e| e.into_inner());
        for (_, handle) in direct_guard.drain() {
            handle.stop();
        }
    }
}

pub fn build_server_jvm_and_target_args(server: &ManagedServer) -> ThkResult<Vec<String>> {
    let mut args = Vec::new();
    let custom_flags: Vec<String> = server
        .jvm_flags
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let has_mem_override = custom_flags
        .iter()
        .any(|f| f.starts_with("-Xmx") || f.starts_with("-Xms"));

    if !has_mem_override {
        let max_mb = server.ram_max_mb.max(512);
        let min_mb = server.ram_min_mb.clamp(256, max_mb);
        args.push(format!("-Xms{min_mb}M"));
        args.push(format!("-Xmx{max_mb}M"));
    }
    if !custom_flags
        .iter()
        .any(|f| f.starts_with("-Dterminal.jline="))
    {
        args.push("-Dterminal.jline=false".into());
    }
    if !custom_flags
        .iter()
        .any(|f| f.starts_with("-Dterminal.ansi="))
    {
        args.push("-Dterminal.ansi=false".into());
    }
    if !custom_flags
        .iter()
        .any(|f| f.starts_with("-Djline.terminal="))
    {
        args.push("-Djline.terminal=jline.UnsupportedTerminal".into());
    }
    args.extend(custom_flags);
    args.extend(thk_java::android_jvm_system_property_args(
        &server.server_dir,
    ));
    args.push("-Dfile.encoding=UTF-8".into());
    args.push("-Dstdout.encoding=UTF-8".into());
    args.push("-Dstderr.encoding=UTF-8".into());

    let trimmed_target = server.jar_file.trim();
    if let Some(argfile_rel) = trimmed_target.strip_prefix('@') {
        let effective_rel = if cfg!(unix) && argfile_rel.ends_with("/win_args.txt") {
            let unix_rel = format!(
                "{}unix_args.txt",
                &argfile_rel[..argfile_rel.len() - "win_args.txt".len()]
            );
            if SafePath::resolve_child(&server.server_dir, &unix_rel).is_ok_and(|p| p.is_file()) {
                unix_rel
            } else {
                argfile_rel.to_string()
            }
        } else {
            argfile_rel.to_string()
        };
        let resolved = SafePath::resolve_child(&server.server_dir, &effective_rel)?;
        if !resolved.is_file() {
            return Err(ThkError::InvalidInput(format!(
                "Forge/NeoForge launch argfile missing: {}",
                resolved.display()
            )));
        }
        args.push(format!("@{effective_rel}"));
    } else {
        let resolved = SafePath::resolve_child(&server.server_dir, trimmed_target)?;
        if !resolved.is_file() {
            return Err(ThkError::InvalidInput(format!(
                "Server JAR file not found: {}",
                resolved.display()
            )));
        }
        args.push("-jar".into());
        args.push(resolved.to_string_lossy().to_string());
    }

    args.push("nogui".into());
    Ok(args)
}

#[derive(Clone)]
struct OutputReaderShared {
    started_at: Instant,
    silent_probe_until_ms: Arc<AtomicU64>,
    probe_cmd_state: Arc<Mutex<Option<&'static str>>>,
    game_version: String,
}

#[allow(clippy::too_many_arguments)]
fn spawn_server_output_reader<R>(
    reader: R,
    server_id: String,
    logs: Arc<Mutex<VecDeque<String>>>,
    state: Arc<RwLock<ServerRuntimeState>>,
    stats: Arc<RwLock<ServerResourceStats>>,
    shared: OutputReaderShared,
    force_kill: CancellationToken,
    events_tx: mpsc::UnboundedSender<ServerSupervisorEvent>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let list_re = Regex::new(r"There are (\d+) of a max of (\d+) players online(?::\s*(.*))?")
            .expect("valid regex");
        let mut lines = BufReader::new(reader).lines();

        while let Ok(Some(raw_line)) = lines.next_line().await {
            let line = strip_console_ansi_and_noise(&raw_line);
            if line.is_empty() && !raw_line.is_empty() {
                continue;
            }

            let elapsed_ms = shared.started_at.elapsed().as_millis() as u64;
            let in_silent_probe =
                elapsed_ms <= shared.silent_probe_until_ms.load(Ordering::Relaxed);

            let mut stats_changed = None;
            if let Some((new_tps, new_mspt)) = crate::telemetry::parse_tps_or_mspt_line(&line) {
                let mut g = stats.write().unwrap_or_else(|e| e.into_inner());
                if let Some(t) = new_tps {
                    g.tps = Some(t);
                }
                if let Some(m) = new_mspt {
                    g.mspt = Some(m);
                }
                stats_changed = Some(g.clone());
            }

            if in_silent_probe {
                if crate::telemetry::is_unknown_command_line(&line) {
                    {
                        let mut cmd_guard = shared
                            .probe_cmd_state
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        if let Some(cur) = *cmd_guard {
                            *cmd_guard = crate::telemetry::fallback_tps_probe_command(
                                cur,
                                &shared.game_version,
                            );
                        }
                    }
                    shared.silent_probe_until_ms.store(0, Ordering::Relaxed);
                    continue;
                }
                if crate::telemetry::is_automated_tps_probe_noise(&line) {
                    if let Some(updated) = stats_changed {
                        let _ = events_tx.send(ServerSupervisorEvent::StatsUpdated {
                            server_id: server_id.clone(),
                            stats: updated,
                        });
                    }
                    continue;
                }
            }

            {
                let mut guard = logs.lock().unwrap_or_else(|e| e.into_inner());
                if guard.len() >= MAX_SERVER_CONSOLE_LINES {
                    guard.pop_front();
                }
                guard.push_back(line.clone());
            }

            let is_ready_log = (line.contains("Done (")
                && (line.contains("For help") || line.contains(")!")))
                || line.contains("Listening on /");
            if is_ready_log {
                let transitioned = {
                    let mut st = state.write().unwrap_or_else(|e| e.into_inner());
                    if *st == ServerRuntimeState::Starting {
                        *st = ServerRuntimeState::Online;
                        true
                    } else {
                        false
                    }
                };
                if transitioned {
                    let _ = events_tx.send(ServerSupervisorEvent::StatusChanged {
                        server_id: server_id.clone(),
                        state: ServerRuntimeState::Online,
                    });
                }
            }

            let is_fatal_or_halted = line.contains("Failed to start the minecraft server")
                || line.contains("FAILED TO BIND TO PORT")
                || line.contains("All dimensions are saved");
            if is_fatal_or_halted {
                let kill_tok = force_kill.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(4)).await;
                    kill_tok.cancel();
                });
            }

            if let Some(caps) = list_re.captures(&line) {
                if let (Ok(cur), Ok(max_p)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>()) {
                    let mut g = stats.write().unwrap_or_else(|e| e.into_inner());
                    g.online_players = cur;
                    g.max_players = max_p;
                    if cur == 0 {
                        g.online_player_names.clear();
                    } else if let Some(names_raw) = caps.get(3).map(|m| m.as_str().trim()) {
                        if !names_raw.is_empty() {
                            let parsed_names: Vec<String> = names_raw
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            if !parsed_names.is_empty() {
                                g.online_player_names = parsed_names;
                            }
                        }
                    }
                    stats_changed = Some(g.clone());
                }
            } else if let Some((pname, joined)) =
                crate::telemetry::extract_player_join_or_leave(&line)
            {
                let mut g = stats.write().unwrap_or_else(|e| e.into_inner());
                if joined {
                    g.online_players = g.online_players.saturating_add(1);
                    if !g
                        .online_player_names
                        .iter()
                        .any(|n| n.eq_ignore_ascii_case(&pname))
                    {
                        g.online_player_names.push(pname);
                    }
                } else {
                    g.online_players = g.online_players.saturating_sub(1);
                    g.online_player_names
                        .retain(|n| !n.eq_ignore_ascii_case(&pname));
                }
                stats_changed = Some(g.clone());
            } else if line.contains("joined the game") {
                let mut g = stats.write().unwrap_or_else(|e| e.into_inner());
                g.online_players = g.online_players.saturating_add(1);
                stats_changed = Some(g.clone());
            } else if line.contains("left the game") {
                let mut g = stats.write().unwrap_or_else(|e| e.into_inner());
                g.online_players = g.online_players.saturating_sub(1);
                stats_changed = Some(g.clone());
            }

            if let Some(updated) = stats_changed {
                let _ = events_tx.send(ServerSupervisorEvent::StatsUpdated {
                    server_id: server_id.clone(),
                    stats: updated,
                });
            }

            let _ = events_tx.send(ServerSupervisorEvent::LogLine {
                server_id: server_id.clone(),
                line,
            });
        }
    });
}
