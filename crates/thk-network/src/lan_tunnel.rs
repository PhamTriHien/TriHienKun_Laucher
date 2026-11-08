use crate::direct_host::{
    start_direct_port_host, DirectHostConfig, DirectHostStatus, DirectPortHostHandle,
};
use crate::ping::strip_section_codes;
use bore_cli::client::Client as BoreClient;
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket as StdUdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use thk_core::{ThkError, ThkResult};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

pub const MC_LAN_MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 2, 60);
pub const MC_LAN_MULTICAST_PORT: u16 = 4445;
pub const DEFAULT_BORE_SERVER: &str = "bore.pub";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanWorldAnnouncement {
    pub motd: String,
    pub port: u16,
    pub sender_ip: Ipv4Addr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TunnelStatus {
    Idle,
    Connecting {
        local_port: u16,
    },
    Running {
        local_port: u16,
        remote_addr: String,
        remote_port: u16,
    },
    Stopped,
    Error(String),
}

impl TunnelStatus {
    pub fn public_endpoint(&self) -> Option<String> {
        match self {
            Self::Running {
                remote_addr,
                remote_port,
                ..
            } => {
                let trimmed = remote_addr.trim();
                let suffix = format!(":{remote_port}");
                let host = trimmed.strip_suffix(&suffix).unwrap_or(trimmed);
                Some(format!("{host}:{remote_port}"))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum LanTunnelEvent {
    ScannerStateChanged { active: bool },
    LocalWorldDetected(LanWorldAnnouncement),
    IgnoredForeignAnnouncement { sender_ip: Ipv4Addr, port: u16 },
    TunnelStatusChanged(TunnelStatus),
    DirectHostStatusChanged(DirectHostStatus),
}

pub fn parse_lan_announcement(raw: &str, sender_ip: Ipv4Addr) -> Option<LanWorldAnnouncement> {
    let ad_start = raw.find("[AD]")? + 4;
    let ad_end = raw[ad_start..].find("[/AD]")? + ad_start;
    let port_str = raw[ad_start..ad_end].trim();
    let port = port_str.parse::<u16>().ok()?;
    if port == 0 {
        return None;
    }

    let motd = if let Some(m_start_idx) = raw.find("[MOTD]") {
        let content_start = m_start_idx + 6;
        if let Some(rel_end) = raw[content_start..].find("[/MOTD]") {
            let cleaned = strip_section_codes(&raw[content_start..content_start + rel_end])
                .trim()
                .to_string();
            if cleaned.is_empty() {
                "LAN World".to_string()
            } else {
                cleaned
            }
        } else {
            "LAN World".to_string()
        }
    } else {
        "LAN World".to_string()
    };

    Some(LanWorldAnnouncement {
        motd,
        port,
        sender_ip,
    })
}

pub fn is_local_interface_ipv4(ip: Ipv4Addr) -> bool {
    if ip.is_loopback() {
        return true;
    }
    if ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast() {
        return false;
    }
    StdUdpSocket::bind(SocketAddrV4::new(ip, 0)).is_ok()
}

async fn is_local_tcp_port_reachable(port: u16) -> bool {
    timeout(
        Duration::from_millis(450),
        TcpStream::connect(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}

#[derive(Clone)]
pub struct BoreTunnelHandle {
    cancel: CancellationToken,
    status: Arc<RwLock<TunnelStatus>>,
}

impl BoreTunnelHandle {
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    pub fn current_status(&self) -> TunnelStatus {
        self.status
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

pub async fn start_bore_tunnel(
    local_port: u16,
    bore_server: Option<&str>,
    secret: Option<&str>,
    events_tx: Option<mpsc::UnboundedSender<LanTunnelEvent>>,
) -> ThkResult<BoreTunnelHandle> {
    if local_port == 0 {
        return Err(ThkError::InvalidInput(
            "Tunnel local port must be > 0".into(),
        ));
    }

    let server_host = bore_server
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_BORE_SERVER)
        .to_string();
    let secret_owned = secret
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let status = Arc::new(RwLock::new(TunnelStatus::Connecting { local_port }));
    if let Some(ref tx) = events_tx {
        let _ = tx.send(LanTunnelEvent::TunnelStatusChanged(
            TunnelStatus::Connecting { local_port },
        ));
    }

    let client = BoreClient::new(
        "127.0.0.1",
        local_port,
        &server_host,
        0,
        secret_owned.as_deref(),
    )
    .await
    .map_err(|e| {
        ThkError::Process(format!(
            "Failed to initialize bore tunnel to {server_host}: {e}"
        ))
    })?;

    let remote_port = client.remote_port();
    let remote_addr = server_host;
    let running_state = TunnelStatus::Running {
        local_port,
        remote_addr,
        remote_port,
    };

    {
        let mut guard = status.write().unwrap_or_else(|e| e.into_inner());
        *guard = running_state.clone();
    }
    if let Some(ref tx) = events_tx {
        let _ = tx.send(LanTunnelEvent::TunnelStatusChanged(running_state));
    }

    let cancel = CancellationToken::new();
    let cancel_task = cancel.clone();
    let status_task = Arc::clone(&status);

    tokio::spawn(async move {
        let final_state = tokio::select! {
            _ = cancel_task.cancelled() => TunnelStatus::Stopped,
            res = client.listen() => {
                match res {
                    Ok(()) => TunnelStatus::Stopped,
                    Err(err) => TunnelStatus::Error(format!("Bore tunnel disconnected: {err}")),
                }
            }
        };

        {
            let mut guard = status_task.write().unwrap_or_else(|e| e.into_inner());
            *guard = final_state.clone();
        }
        if let Some(tx) = events_tx {
            let _ = tx.send(LanTunnelEvent::TunnelStatusChanged(final_state));
        }
    });

    Ok(BoreTunnelHandle { cancel, status })
}

#[derive(Clone)]
pub struct LanShareCoordinator {
    scan_cancel: Arc<RwLock<Option<CancellationToken>>>,
    active_tunnel: Arc<RwLock<Option<BoreTunnelHandle>>>,
    active_direct_host: Arc<RwLock<Option<DirectPortHostHandle>>>,
    auto_direct_host_template: Arc<RwLock<Option<DirectHostConfig>>>,
    self_hosted_lan_port: Arc<RwLock<Option<u16>>>,
    last_detected: Arc<RwLock<Option<LanWorldAnnouncement>>>,
    last_announced_port: Arc<RwLock<Option<u16>>>,
    tunnel_generation: Arc<AtomicU64>,
    direct_host_generation: Arc<AtomicU64>,
    events_tx: mpsc::UnboundedSender<LanTunnelEvent>,
}

impl LanShareCoordinator {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<LanTunnelEvent>) {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let coord = Self {
            scan_cancel: Arc::new(RwLock::new(None)),
            active_tunnel: Arc::new(RwLock::new(None)),
            active_direct_host: Arc::new(RwLock::new(None)),
            auto_direct_host_template: Arc::new(RwLock::new(None)),
            self_hosted_lan_port: Arc::new(RwLock::new(None)),
            last_detected: Arc::new(RwLock::new(None)),
            last_announced_port: Arc::new(RwLock::new(None)),
            tunnel_generation: Arc::new(AtomicU64::new(0)),
            direct_host_generation: Arc::new(AtomicU64::new(0)),
            events_tx,
        };
        (coord, events_rx)
    }

    pub fn is_scanning(&self) -> bool {
        self.scan_cancel
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    pub fn last_detected_world(&self) -> Option<LanWorldAnnouncement> {
        self.last_detected
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn tunnel_status(&self) -> TunnelStatus {
        self.active_tunnel
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(BoreTunnelHandle::current_status)
            .unwrap_or(TunnelStatus::Idle)
    }

    pub fn direct_host_status(&self) -> DirectHostStatus {
        self.active_direct_host
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(DirectPortHostHandle::current_status)
            .unwrap_or(DirectHostStatus::Idle)
    }

    pub fn set_auto_direct_host_template(&self, template: Option<DirectHostConfig>) {
        *self
            .auto_direct_host_template
            .write()
            .unwrap_or_else(|e| e.into_inner()) = template;
    }

    pub fn stop_direct_host(&self) {
        self.direct_host_generation.fetch_add(1, Ordering::SeqCst);
        *self
            .self_hosted_lan_port
            .write()
            .unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(handle) = self
            .active_direct_host
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            handle.stop();
            let _ = self.events_tx.send(LanTunnelEvent::DirectHostStatusChanged(
                DirectHostStatus::Stopped,
            ));
        }
    }

    pub async fn start_direct_host(
        &self,
        config: DirectHostConfig,
    ) -> ThkResult<DirectPortHostHandle> {
        let session_gen = self.direct_host_generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(old) = self
            .active_direct_host
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            old.stop();
        }
        *self
            .self_hosted_lan_port
            .write()
            .unwrap_or_else(|e| e.into_inner()) =
            config.announce_on_lan.then_some(config.host_port);

        let (inner_tx, mut inner_rx) = mpsc::unbounded_channel::<DirectHostStatus>();
        let gen_ref = Arc::clone(&self.direct_host_generation);
        let self_port_ref = Arc::clone(&self.self_hosted_lan_port);
        let out_tx = self.events_tx.clone();

        tokio::spawn(async move {
            while let Some(st) = inner_rx.recv().await {
                if gen_ref.load(Ordering::SeqCst) != session_gen {
                    break;
                }
                if matches!(st, DirectHostStatus::Stopped | DirectHostStatus::Error(_)) {
                    *self_port_ref.write().unwrap_or_else(|e| e.into_inner()) = None;
                }
                let _ = out_tx.send(LanTunnelEvent::DirectHostStatusChanged(st));
            }
        });

        let res = start_direct_port_host(config, Some(inner_tx)).await;
        match res {
            Ok(handle) => {
                if self.direct_host_generation.load(Ordering::SeqCst) == session_gen {
                    *self
                        .active_direct_host
                        .write()
                        .unwrap_or_else(|e| e.into_inner()) = Some(handle.clone());
                } else {
                    handle.stop();
                }
                Ok(handle)
            }
            Err(err) => {
                if self.direct_host_generation.load(Ordering::SeqCst) == session_gen {
                    *self
                        .self_hosted_lan_port
                        .write()
                        .unwrap_or_else(|e| e.into_inner()) = None;
                }
                Err(err)
            }
        }
    }

    pub fn stop_tunnel(&self) {
        self.tunnel_generation.fetch_add(1, Ordering::SeqCst);
        *self
            .last_announced_port
            .write()
            .unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(handle) = self
            .active_tunnel
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            handle.stop();
            let _ = self
                .events_tx
                .send(LanTunnelEvent::TunnelStatusChanged(TunnelStatus::Stopped));
        }
    }

    pub fn stop_scan(&self) {
        if let Some(tok) = self
            .scan_cancel
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            tok.cancel();
        }
        self.stop_tunnel();
        let _ = self
            .events_tx
            .send(LanTunnelEvent::ScannerStateChanged { active: false });
    }

    pub fn shutdown_all(&self) {
        self.stop_scan();
        self.stop_direct_host();
    }

    pub async fn start_manual_tunnel(&self, local_port: u16) -> ThkResult<BoreTunnelHandle> {
        let session_gen = self.tunnel_generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(old) = self
            .active_tunnel
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            old.stop();
        }

        let (inner_tx, mut inner_rx) = mpsc::unbounded_channel::<LanTunnelEvent>();
        let gen_ref = Arc::clone(&self.tunnel_generation);
        let port_ref = Arc::clone(&self.last_announced_port);
        let out_tx = self.events_tx.clone();

        tokio::spawn(async move {
            while let Some(ev) = inner_rx.recv().await {
                if gen_ref.load(Ordering::SeqCst) != session_gen {
                    break;
                }
                if matches!(
                    &ev,
                    LanTunnelEvent::TunnelStatusChanged(
                        TunnelStatus::Stopped | TunnelStatus::Error(_)
                    )
                ) {
                    *port_ref.write().unwrap_or_else(|e| e.into_inner()) = None;
                }
                let _ = out_tx.send(ev);
            }
        });

        let handle = start_bore_tunnel(local_port, None, None, Some(inner_tx)).await?;
        if self.tunnel_generation.load(Ordering::SeqCst) == session_gen {
            *self
                .active_tunnel
                .write()
                .unwrap_or_else(|e| e.into_inner()) = Some(handle.clone());
        } else {
            handle.stop();
        }
        Ok(handle)
    }

    pub fn start_lan_scan(
        &self,
        runtime: &tokio::runtime::Handle,
        auto_open_tunnel: bool,
    ) -> ThkResult<()> {
        if self.is_scanning() {
            return Ok(());
        }

        let std_sock = bind_multicast_lan_socket()?;
        let sock = {
            let _guard = runtime.enter();
            UdpSocket::from_std(std_sock)?
        };
        let cancel = CancellationToken::new();
        *self.scan_cancel.write().unwrap_or_else(|e| e.into_inner()) = Some(cancel.clone());
        *self
            .last_announced_port
            .write()
            .unwrap_or_else(|e| e.into_inner()) = None;

        let _ = self
            .events_tx
            .send(LanTunnelEvent::ScannerStateChanged { active: true });

        let coord = self.clone();
        runtime.spawn(async move {
            let mut buf = [0u8; 2048];

            loop {
                let recv_res = tokio::select! {
                    _ = cancel.cancelled() => break,
                    r = sock.recv_from(&mut buf) => r,
                };

                let Ok((len, SocketAddr::V4(src_v4))) = recv_res else {
                    continue;
                };
                let src_ip = *src_v4.ip();
                let text = String::from_utf8_lossy(&buf[..len]);
                let Some(ann) = parse_lan_announcement(&text, src_ip) else {
                    continue;
                };

                if !is_local_interface_ipv4(src_ip) {
                    let _ = coord
                        .events_tx
                        .send(LanTunnelEvent::IgnoredForeignAnnouncement {
                            sender_ip: src_ip,
                            port: ann.port,
                        });
                    continue;
                }

                let is_self_hosted_broadcast = *coord
                    .self_hosted_lan_port
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    == Some(ann.port);
                if is_self_hosted_broadcast {
                    continue;
                }

                let already_tracked = *coord
                    .last_announced_port
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    == Some(ann.port);
                if already_tracked {
                    continue;
                }

                if !is_local_tcp_port_reachable(ann.port).await {
                    continue;
                }

                *coord
                    .last_announced_port
                    .write()
                    .unwrap_or_else(|e| e.into_inner()) = Some(ann.port);
                *coord
                    .last_detected
                    .write()
                    .unwrap_or_else(|e| e.into_inner()) = Some(ann.clone());
                let _ = coord
                    .events_tx
                    .send(LanTunnelEvent::LocalWorldDetected(ann.clone()));

                let auto_host_cfg = coord
                    .auto_direct_host_template
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                if let Some(mut cfg) = auto_host_cfg {
                    cfg.local_target_port = ann.port;
                    if cfg.motd.trim().is_empty() {
                        cfg.motd = ann.motd.clone();
                    }
                    if let Err(err) = coord.start_direct_host(cfg).await {
                        let _ = coord
                            .events_tx
                            .send(LanTunnelEvent::DirectHostStatusChanged(
                                DirectHostStatus::Error(err.to_string()),
                            ));
                    }
                }

                if auto_open_tunnel {
                    if let Err(err) = coord.start_manual_tunnel(ann.port).await {
                        *coord
                            .last_announced_port
                            .write()
                            .unwrap_or_else(|e| e.into_inner()) = None;
                        let _ = coord.events_tx.send(LanTunnelEvent::TunnelStatusChanged(
                            TunnelStatus::Error(err.to_string()),
                        ));
                    }
                }
            }

            let _ = coord
                .events_tx
                .send(LanTunnelEvent::ScannerStateChanged { active: false });
        });

        Ok(())
    }
}

fn bind_multicast_lan_socket() -> ThkResult<StdUdpSocket> {
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), MC_LAN_MULTICAST_PORT);
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|e| ThkError::Process(format!("Cannot create UDP multicast socket: {e}")))?;
    let _ = sock.set_reuse_address(true);
    sock.bind(&bind_addr.into()).map_err(|e| {
        ThkError::Process(format!(
            "Cannot bind UDP multicast port {MC_LAN_MULTICAST_PORT}: {e}"
        ))
    })?;
    sock.set_nonblocking(true)?;
    sock.join_multicast_v4(&MC_LAN_MULTICAST_GROUP, &Ipv4Addr::UNSPECIFIED)
        .map_err(|e| ThkError::Process(format!("Failed joining multicast 224.0.2.60: {e}")))?;
    Ok(sock.into())
}
