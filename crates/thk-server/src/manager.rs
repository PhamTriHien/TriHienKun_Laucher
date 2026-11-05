use crate::config_files::{is_eula_accepted, write_eula_accepted};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use thk_core::models::{DownloadProgress, ManagedServer};
use thk_core::{AppPaths, DownloadManager, SafePath, ThkError, ThkResult};
use thk_java::{ensure_mojang_java, probe_java_binary, RequiredJavaSpec};
use thk_network::{start_bore_tunnel, BoreTunnelHandle, LanTunnelEvent, TunnelStatus};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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
    pub ram_mb: u64,
    pub uptime_secs: u64,
    pub online_players: u32,
    pub max_players: u32,
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
    force_kill: CancellationToken,
    exited_notify: Arc<Notify>,
}

pub fn sanitize_console_command(raw: &str) -> String {
    raw.chars()
        .filter(|&c| c != '\r' && c != '\n' && c >= ' ')
        .collect::<String>()
        .trim()
        .to_string()
}

#[derive(Clone)]
pub struct LocalServerSupervisor {
    paths: AppPaths,
    downloader: DownloadManager,
    running: Arc<RwLock<HashMap<String, RunningServerHandle>>>,
    tunnels: Arc<RwLock<HashMap<String, BoreTunnelHandle>>>,
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
            events_tx,
        };
        (sup, events_rx)
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
        if let Some(ref custom) = server.java_path {
            if custom.is_file() && probe_java_binary(custom, false).is_ok() {
                return Ok(custom.clone());
            }
        }
        let spec = RequiredJavaSpec::for_minecraft_version(&server.game_version, None, None);
        let inst = ensure_mojang_java(
            &self.downloader,
            &self.paths.runtimes_dir,
            &spec,
            None,
            |_| {},
        )
        .await?;
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

        let java_bin = self.resolve_java_for_server(server).await?;
        let launch_args = build_server_jvm_and_target_args(server)?;

        let mut cmd = Command::new(&java_bin);
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
        let stats = Arc::new(RwLock::new(ServerResourceStats {
            cpu_percent: 0.0,
            ram_mb: 0,
            uptime_secs: 0,
            online_players: 0,
            max_players: 20,
        }));
        let logs = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_SERVER_CONSOLE_LINES)));
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
                    force_kill: force_kill.clone(),
                    exited_notify: Arc::clone(&exited_notify),
                },
            );
        }

        let _ = self.events_tx.send(ServerSupervisorEvent::StatusChanged {
            server_id: server.id.clone(),
            state: ServerRuntimeState::Starting,
        });

        tokio::spawn(async move {
            let mut writer = child_stdin;
            while let Some(cmd_line) = stdin_rx.recv().await {
                let payload = format!("{cmd_line}\n");
                if writer.write_all(payload.as_bytes()).await.is_err() {
                    break;
                }
                let _ = writer.flush().await;
            }
        });

        let metrics_cancel = CancellationToken::new();
        if let Some(pid_u32) = child_pid {
            let metrics_stop = metrics_cancel.clone();
            let stats_ref = Arc::clone(&stats);
            let tx_metrics = self.events_tx.clone();
            let srv_id = server.id.clone();
            let started_at = Instant::now();

            tokio::spawn(async move {
                let Ok(mut sys) = System::new() else {
                    return;
                };
                let pid = Pid::from_u32(pid_u32);
                let refresh_kind = ProcessRefreshKind::nothing().with_cpu().with_memory();
                let num_cpus = std::thread::available_parallelism()
                    .map(|n| n.get() as f32)
                    .unwrap_or(1.0)
                    .max(1.0);

                loop {
                    tokio::select! {
                        _ = metrics_stop.cancelled() => break,
                        _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                    }

                    sys.refresh_processes_specifics(
                        ProcessesToUpdate::Some(&[pid]),
                        true,
                        refresh_kind,
                    );

                    if let Some(proc_info) = sys.process(pid) {
                        let normalized_cpu = (proc_info.cpu_usage() / num_cpus).clamp(0.0, 100.0);
                        let rss_mb = proc_info.memory() / (1024 * 1024);
                        let snapshot = {
                            let mut guard = stats_ref.write().unwrap_or_else(|e| e.into_inner());
                            guard.cpu_percent = (normalized_cpu * 10.0).round() / 10.0;
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

        if let Some(out) = child_stdout {
            spawn_server_output_reader(
                out,
                server.id.clone(),
                Arc::clone(&logs),
                Arc::clone(&state),
                Arc::clone(&stats),
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
                self.events_tx.clone(),
            );
        }

        let running_map = Arc::clone(&self.running);
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
        handle
            .stdin_tx
            .send(cleaned)
            .map_err(|_| ThkError::Process("Server stdin stream is closed".into()))
    }

    pub async fn stop_server(&self, server_id: &str) -> ThkResult<()> {
        let (stdin_tx, force_kill, exited_notify) = {
            let guard = self.running.read().unwrap_or_else(|e| e.into_inner());
            let Some(handle) = guard.get(server_id) else {
                return Ok(());
            };
            {
                let mut st = handle.state.write().unwrap_or_else(|e| e.into_inner());
                *st = ServerRuntimeState::Stopping;
            }
            (
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

        if timeout(Duration::from_secs(10), notified).await.is_err() {
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
        self.stop_server_tunnel(server_id);
        let (lan_tx, mut lan_rx) = mpsc::unbounded_channel::<LanTunnelEvent>();
        let srv_id = server_id.to_string();
        let out_tx = self.events_tx.clone();
        tokio::spawn(async move {
            while let Some(ev) = lan_rx.recv().await {
                if let LanTunnelEvent::TunnelStatusChanged(status) = ev {
                    let _ = out_tx.send(ServerSupervisorEvent::TunnelChanged {
                        server_id: srv_id.clone(),
                        status,
                    });
                }
            }
        });

        let handle = start_bore_tunnel(local_port, None, None, Some(lan_tx)).await?;
        self.tunnels
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(server_id.to_string(), handle.clone());
        Ok(handle)
    }

    pub fn stop_server_tunnel(&self, server_id: &str) {
        if let Some(handle) = self
            .tunnels
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(server_id)
        {
            handle.stop();
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
    }
}

fn build_server_jvm_and_target_args(server: &ManagedServer) -> ThkResult<Vec<String>> {
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
    args.extend(custom_flags);
    args.push("-Dfile.encoding=UTF-8".into());
    args.push("-Dstdout.encoding=UTF-8".into());
    args.push("-Dstderr.encoding=UTF-8".into());

    let trimmed_target = server.jar_file.trim();
    if let Some(argfile_rel) = trimmed_target.strip_prefix('@') {
        let resolved = SafePath::resolve_child(&server.server_dir, argfile_rel)?;
        if !resolved.is_file() {
            return Err(ThkError::InvalidInput(format!(
                "Forge/NeoForge launch argfile missing: {}",
                resolved.display()
            )));
        }
        args.push(format!("@{argfile_rel}"));
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

fn spawn_server_output_reader<R>(
    reader: R,
    server_id: String,
    logs: Arc<Mutex<VecDeque<String>>>,
    state: Arc<RwLock<ServerRuntimeState>>,
    stats: Arc<RwLock<ServerResourceStats>>,
    events_tx: mpsc::UnboundedSender<ServerSupervisorEvent>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let list_re =
            Regex::new(r"There are (\d+) of a max of (\d+) players online").expect("valid regex");
        let mut lines = BufReader::new(reader).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            {
                let mut guard = logs.lock().unwrap_or_else(|e| e.into_inner());
                if guard.len() >= MAX_SERVER_CONSOLE_LINES {
                    guard.pop_front();
                }
                guard.push_back(line.clone());
            }

            if line.contains("Done (") && line.contains("For help") {
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

            let mut stats_changed = None;
            if let Some(caps) = list_re.captures(&line) {
                if let (Ok(cur), Ok(max_p)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>()) {
                    let mut g = stats.write().unwrap_or_else(|e| e.into_inner());
                    g.online_players = cur;
                    g.max_players = max_p;
                    stats_changed = Some(g.clone());
                }
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
