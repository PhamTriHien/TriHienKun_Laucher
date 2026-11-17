use crate::direct_host::{
    detect_local_network_snapshot, format_lan_announcement, is_tcp_addr_reachable,
    send_lan_multicast_frame,
};
use crate::lan_tunnel::LanTunnelEvent;
use crate::ping::strip_section_codes;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use iroh::endpoint::{presets, Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, EndpointId};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use thk_core::{ThkError, ThkResult};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

pub const VIRTUAL_PORT_ALPN: &[u8] = b"thk-mc-vport/1";
const ROOM_TICKET_PREFIX: &str = "thk1_";
const ROOM_TICKET_VERSION: u8 = 1;
const MAX_DECOMPRESSED_TICKET_BYTES: u64 = 16 * 1024;
const STREAM_OPEN_MAGIC: u8 = 0x01;
const LAN_BROADCAST_INTERVAL: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VirtualRoomTicket {
    pub v: u8,
    pub addr: EndpointAddr,
    pub motd: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualPortHostInfo {
    pub local_port: u16,
    pub room_code: String,
    pub node_id: String,
    pub relay_connected: bool,
    pub active_streams: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VirtualPortHostStatus {
    Idle,
    Starting { local_port: u16 },
    Running(VirtualPortHostInfo),
    Stopped,
    Error(String),
}

#[derive(Clone)]
pub struct VirtualPortHostHandle {
    cancel: CancellationToken,
    status: Arc<RwLock<VirtualPortHostStatus>>,
}

impl VirtualPortHostHandle {
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    pub fn current_status(&self) -> VirtualPortHostStatus {
        self.status
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualPortClientInfo {
    pub bound_port: u16,
    pub local_endpoint: String,
    pub motd: String,
    pub remote_node_id: String,
    pub announcing_lan: bool,
    pub active_streams: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VirtualPortClientStatus {
    Idle,
    Connecting { preferred_port: u16 },
    Running(VirtualPortClientInfo),
    Stopped,
    Error(String),
}

#[derive(Clone)]
pub struct VirtualPortClientHandle {
    cancel: CancellationToken,
    status: Arc<RwLock<VirtualPortClientStatus>>,
}

impl VirtualPortClientHandle {
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    pub fn current_status(&self) -> VirtualPortClientStatus {
        self.status
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

fn sanitize_room_motd(raw: &str) -> String {
    let stripped = strip_section_codes(raw);
    let cleaned: String = stripped
        .replace("[/MOTD]", "")
        .replace("[MOTD]", "")
        .replace("[/AD]", "")
        .replace("[AD]", "")
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "Minecraft World".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn encode_room_ticket(addr: &EndpointAddr, motd: &str) -> ThkResult<String> {
    let ticket = VirtualRoomTicket {
        v: ROOM_TICKET_VERSION,
        addr: addr.clone(),
        motd: sanitize_room_motd(motd),
    };
    let json = serde_json::to_vec(&ticket)
        .map_err(|e| ThkError::InvalidInput(format!("Failed to serialize room code: {e}")))?;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder
        .write_all(&json)
        .map_err(|e| ThkError::InvalidInput(format!("Failed to compress room code: {e}")))?;
    let compressed = encoder
        .finish()
        .map_err(|e| ThkError::InvalidInput(format!("Failed to finalize room code: {e}")))?;
    Ok(format!(
        "{ROOM_TICKET_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(&compressed)
    ))
}

pub fn decode_room_ticket(raw: &str) -> ThkResult<VirtualRoomTicket> {
    let mut trimmed = raw.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        trimmed = trimmed[1..trimmed.len() - 1].trim();
    }
    if trimmed.is_empty() {
        return Err(ThkError::InvalidInput(
            "Mã phòng không được để trống.".into(),
        ));
    }

    if let Some(encoded) = trimmed.strip_prefix(ROOM_TICKET_PREFIX) {
        let compressed = URL_SAFE_NO_PAD
            .decode(encoded)
            .or_else(|_| URL_SAFE.decode(encoded))
            .map_err(|e| ThkError::InvalidInput(format!("Mã phòng không hợp lệ: {e}")))?;
        let mut decoder =
            ZlibDecoder::new(compressed.as_slice()).take(MAX_DECOMPRESSED_TICKET_BYTES + 1);
        let mut json_bytes = Vec::new();
        decoder
            .read_to_end(&mut json_bytes)
            .map_err(|e| ThkError::InvalidInput(format!("Không thể giải mã mã phòng: {e}")))?;
        if json_bytes.len() as u64 > MAX_DECOMPRESSED_TICKET_BYTES {
            return Err(ThkError::InvalidInput(
                "Mã phòng vượt quá kích thước cho phép.".into(),
            ));
        }
        let mut ticket: VirtualRoomTicket = serde_json::from_slice(&json_bytes)
            .map_err(|e| ThkError::InvalidInput(format!("Dữ liệu mã phòng bị hỏng: {e}")))?;
        if ticket.v != ROOM_TICKET_VERSION {
            return Err(ThkError::InvalidInput(format!(
                "Phiên bản mã phòng không được hỗ trợ: v{}",
                ticket.v
            )));
        }
        ticket.motd = sanitize_room_motd(&ticket.motd);
        return Ok(ticket);
    }

    if let Ok(node_id) = trimmed.parse::<EndpointId>() {
        return Ok(VirtualRoomTicket {
            v: ROOM_TICKET_VERSION,
            addr: EndpointAddr::from(node_id),
            motd: "Minecraft World".to_string(),
        });
    }

    Err(ThkError::InvalidInput(
        "Mã phòng không đúng định dạng (cần mã dạng thk1_... hoặc mã nút 64 ký tự).".into(),
    ))
}

fn emit_host_status(
    events_tx: &Option<mpsc::UnboundedSender<LanTunnelEvent>>,
    status: &Arc<RwLock<VirtualPortHostStatus>>,
    next: VirtualPortHostStatus,
) {
    {
        let mut guard = status.write().unwrap_or_else(|e| e.into_inner());
        *guard = next.clone();
    }
    if let Some(tx) = events_tx {
        let _ = tx.send(LanTunnelEvent::VirtualHostStatusChanged(next));
    }
}

fn emit_client_status(
    events_tx: &Option<mpsc::UnboundedSender<LanTunnelEvent>>,
    status: &Arc<RwLock<VirtualPortClientStatus>>,
    next: VirtualPortClientStatus,
) {
    {
        let mut guard = status.write().unwrap_or_else(|e| e.into_inner());
        *guard = next.clone();
    }
    if let Some(tx) = events_tx {
        let _ = tx.send(LanTunnelEvent::VirtualClientStatusChanged(next));
    }
}

fn update_host_active_streams(
    status: &Arc<RwLock<VirtualPortHostStatus>>,
    events_tx: &Option<mpsc::UnboundedSender<LanTunnelEvent>>,
    count: usize,
) {
    let mut updated = None;
    {
        let mut guard = status.write().unwrap_or_else(|e| e.into_inner());
        if let VirtualPortHostStatus::Running(ref mut info) = *guard {
            if info.active_streams != count {
                info.active_streams = count;
                updated = Some(guard.clone());
            }
        }
    }
    if let (Some(st), Some(tx)) = (updated, events_tx) {
        let _ = tx.send(LanTunnelEvent::VirtualHostStatusChanged(st));
    }
}

fn update_client_active_streams(
    status: &Arc<RwLock<VirtualPortClientStatus>>,
    events_tx: &Option<mpsc::UnboundedSender<LanTunnelEvent>>,
    count: usize,
) {
    let mut updated = None;
    {
        let mut guard = status.write().unwrap_or_else(|e| e.into_inner());
        if let VirtualPortClientStatus::Running(ref mut info) = *guard {
            if info.active_streams != count {
                info.active_streams = count;
                updated = Some(guard.clone());
            }
        }
    }
    if let (Some(st), Some(tx)) = (updated, events_tx) {
        let _ = tx.send(LanTunnelEvent::VirtualClientStatusChanged(st));
    }
}

trait StreamCountSink: Send + Sync + 'static {
    fn set_count(&self, count: usize);
}

struct HostStreamSink {
    status: Arc<RwLock<VirtualPortHostStatus>>,
    events_tx: Option<mpsc::UnboundedSender<LanTunnelEvent>>,
}

impl StreamCountSink for HostStreamSink {
    fn set_count(&self, count: usize) {
        update_host_active_streams(&self.status, &self.events_tx, count);
    }
}

struct ClientStreamSink {
    status: Arc<RwLock<VirtualPortClientStatus>>,
    events_tx: Option<mpsc::UnboundedSender<LanTunnelEvent>>,
}

impl StreamCountSink for ClientStreamSink {
    fn set_count(&self, count: usize) {
        update_client_active_streams(&self.status, &self.events_tx, count);
    }
}

struct ActiveStreamGuard<S: StreamCountSink> {
    counter: Arc<AtomicUsize>,
    sink: S,
}

impl<S: StreamCountSink> ActiveStreamGuard<S> {
    fn enter(counter: Arc<AtomicUsize>, sink: S) -> Self {
        let next = counter.fetch_add(1, Ordering::SeqCst) + 1;
        sink.set_count(next);
        Self { counter, sink }
    }
}

impl<S: StreamCountSink> Drop for ActiveStreamGuard<S> {
    fn drop(&mut self) {
        let next = self
            .counter
            .fetch_sub(1, Ordering::SeqCst)
            .saturating_sub(1);
        self.sink.set_count(next);
    }
}

async fn bridge_tcp_and_quic(
    tcp: TcpStream,
    mut quic_send: SendStream,
    mut quic_recv: RecvStream,
    cancel: CancellationToken,
) {
    let _ = tcp.set_nodelay(true);
    let (mut tcp_read, mut tcp_write) = tcp.into_split();

    let to_tcp = async {
        let res = tokio::io::copy(&mut quic_recv, &mut tcp_write).await;
        let _ = tcp_write.shutdown().await;
        res
    };
    let to_quic = async {
        let res = tokio::io::copy(&mut tcp_read, &mut quic_send).await;
        let _ = quic_send.finish();
        res
    };

    tokio::select! {
        _ = cancel.cancelled() => {}
        _ = async { tokio::join!(to_tcp, to_quic) } => {}
    }
}

async fn build_host_endpoint(offline_local_only: bool) -> ThkResult<Endpoint> {
    let builder = if offline_local_only {
        Endpoint::builder(presets::Minimal)
    } else {
        Endpoint::builder(presets::N0)
    };
    builder
        .alpns(vec![VIRTUAL_PORT_ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| ThkError::Process(format!("Không thể khởi tạo cổng ảo mạng: {e}")))
}

async fn build_client_endpoint(offline_local_only: bool) -> ThkResult<Endpoint> {
    let res = if offline_local_only {
        Endpoint::bind(presets::Minimal).await
    } else {
        Endpoint::bind(presets::N0).await
    };
    res.map_err(|e| ThkError::Process(format!("Không thể khởi tạo kết nối cổng ảo: {e}")))
}

pub async fn start_virtual_port_host(
    local_port: u16,
    motd: String,
    events_tx: Option<mpsc::UnboundedSender<LanTunnelEvent>>,
) -> ThkResult<VirtualPortHostHandle> {
    start_virtual_port_host_inner(local_port, motd, false, events_tx).await
}

pub(crate) async fn start_virtual_port_host_inner(
    local_port: u16,
    motd: String,
    offline_local_only: bool,
    events_tx: Option<mpsc::UnboundedSender<LanTunnelEvent>>,
) -> ThkResult<VirtualPortHostHandle> {
    let status = Arc::new(RwLock::new(VirtualPortHostStatus::Idle));
    if local_port == 0 {
        let msg = "Cổng nội bộ phải nằm trong khoảng 1..=65535.".to_string();
        emit_host_status(
            &events_tx,
            &status,
            VirtualPortHostStatus::Error(msg.clone()),
        );
        return Err(ThkError::InvalidInput(msg));
    }

    let local_v4 = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, local_port));
    if !is_tcp_addr_reachable(local_v4, Duration::from_millis(650)).await {
        let msg = format!(
            "Chưa có thế giới hoặc máy chủ nào đang mở tại 127.0.0.1:{local_port} (hãy khởi động Server hoặc bật Open to LAN trước)."
        );
        emit_host_status(
            &events_tx,
            &status,
            VirtualPortHostStatus::Error(msg.clone()),
        );
        return Err(ThkError::Process(msg));
    }

    emit_host_status(
        &events_tx,
        &status,
        VirtualPortHostStatus::Starting { local_port },
    );

    let endpoint = match build_host_endpoint(offline_local_only).await {
        Ok(ep) => ep,
        Err(err) => {
            emit_host_status(
                &events_tx,
                &status,
                VirtualPortHostStatus::Error(err.to_string()),
            );
            return Err(err);
        }
    };

    let cancel = CancellationToken::new();
    let relay_connected = if offline_local_only {
        false
    } else {
        tokio::select! {
            _ = cancel.cancelled() => {
                endpoint.close().await;
                emit_host_status(&events_tx, &status, VirtualPortHostStatus::Stopped);
                return Err(ThkError::Cancelled);
            }
            res = timeout(Duration::from_secs(7), endpoint.online()) => res.is_ok(),
        }
    };

    let room_code = match encode_room_ticket(&endpoint.addr(), &motd) {
        Ok(code) => code,
        Err(err) => {
            endpoint.close().await;
            emit_host_status(
                &events_tx,
                &status,
                VirtualPortHostStatus::Error(err.to_string()),
            );
            return Err(err);
        }
    };

    let info = VirtualPortHostInfo {
        local_port,
        room_code,
        node_id: endpoint.id().to_string(),
        relay_connected,
        active_streams: 0,
    };
    emit_host_status(&events_tx, &status, VirtualPortHostStatus::Running(info));

    let active_streams = Arc::new(AtomicUsize::new(0));
    let cancel_task = cancel.clone();
    let status_task = Arc::clone(&status);
    let tx_task = events_tx.clone();

    tokio::spawn(async move {
        loop {
            let incoming_opt = tokio::select! {
                _ = cancel_task.cancelled() => break,
                inc = endpoint.accept() => inc,
            };
            let Some(incoming) = incoming_opt else {
                break;
            };
            let conn_cancel = cancel_task.clone();
            let counter = Arc::clone(&active_streams);
            let st_ref = Arc::clone(&status_task);
            let tx_ref = tx_task.clone();

            tokio::spawn(async move {
                let conn = tokio::select! {
                    _ = conn_cancel.cancelled() => return,
                    res = timeout(Duration::from_secs(6), incoming) => {
                        match res {
                            Ok(Ok(c)) => c,
                            _ => return,
                        }
                    }
                };

                loop {
                    let bi = tokio::select! {
                        _ = conn_cancel.cancelled() => break,
                        res = conn.accept_bi() => res,
                    };
                    let Ok((send, mut recv)) = bi else {
                        break;
                    };

                    let stream_cancel = conn_cancel.clone();
                    let counter_s = Arc::clone(&counter);
                    let st_s = Arc::clone(&st_ref);
                    let tx_s = tx_ref.clone();

                    tokio::spawn(async move {
                        let mut magic = [0u8; 1];
                        let ok_magic = tokio::select! {
                            _ = stream_cancel.cancelled() => false,
                            res = timeout(Duration::from_secs(4), recv.read_exact(&mut magic)) => {
                                matches!(res, Ok(Ok(_)) if magic[0] == STREAM_OPEN_MAGIC)
                            }
                        };
                        if !ok_magic {
                            return;
                        }

                        let target_v4 =
                            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, local_port));
                        let Ok(Ok(tcp_stream)) =
                            timeout(Duration::from_secs(3), TcpStream::connect(target_v4)).await
                        else {
                            return;
                        };

                        let _guard = ActiveStreamGuard::enter(
                            counter_s,
                            HostStreamSink {
                                status: st_s,
                                events_tx: tx_s,
                            },
                        );
                        bridge_tcp_and_quic(tcp_stream, send, recv, stream_cancel).await;
                    });
                }
            });
        }

        endpoint.close().await;
        emit_host_status(&tx_task, &status_task, VirtualPortHostStatus::Stopped);
    });

    Ok(VirtualPortHostHandle { cancel, status })
}

async fn bind_local_virtual_listener(preferred_port: u16) -> ThkResult<TcpListener> {
    if preferred_port > 0 {
        if let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, preferred_port)).await {
            return Ok(listener);
        }
    }
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|e| ThkError::Process(format!("Không thể mở cổng ảo trên máy này: {e}")))
}

async fn open_bridged_client_stream(
    endpoint: &Endpoint,
    target: &EndpointAddr,
    cached_conn: &Mutex<Option<Connection>>,
) -> Result<(SendStream, RecvStream), String> {
    let existing = { cached_conn.lock().await.clone() };
    if let Some(conn) = existing {
        if conn.close_reason().is_none() {
            if let Ok((mut send, recv)) = conn.open_bi().await {
                if send.write_all(&[STREAM_OPEN_MAGIC]).await.is_ok() {
                    return Ok((send, recv));
                }
            }
        }
    }

    let mut guard = cached_conn.lock().await;
    if let Some(ref conn) = *guard {
        if conn.close_reason().is_none() {
            if let Ok((mut send, recv)) = conn.open_bi().await {
                if send.write_all(&[STREAM_OPEN_MAGIC]).await.is_ok() {
                    return Ok((send, recv));
                }
            }
        }
    }

    let new_conn = timeout(
        Duration::from_secs(10),
        endpoint.connect(target.clone(), VIRTUAL_PORT_ALPN),
    )
    .await
    .map_err(|_| "Quá thời gian kết nối tới phòng ảo.".to_string())?
    .map_err(|e| format!("Không thể kết nối tới phòng ảo: {e}"))?;

    let (mut send, recv) = new_conn
        .open_bi()
        .await
        .map_err(|e| format!("Không thể mở luồng dữ liệu tới phòng ảo: {e}"))?;
    send.write_all(&[STREAM_OPEN_MAGIC])
        .await
        .map_err(|e| format!("Không thể gửi tín hiệu mở luồng: {e}"))?;
    *guard = Some(new_conn);
    Ok((send, recv))
}

pub async fn start_virtual_port_client(
    room_code: &str,
    preferred_port: u16,
    announce_on_lan: bool,
    events_tx: Option<mpsc::UnboundedSender<LanTunnelEvent>>,
) -> ThkResult<VirtualPortClientHandle> {
    start_virtual_port_client_inner(room_code, preferred_port, announce_on_lan, false, events_tx)
        .await
}

pub(crate) async fn start_virtual_port_client_inner(
    room_code: &str,
    preferred_port: u16,
    announce_on_lan: bool,
    offline_local_only: bool,
    events_tx: Option<mpsc::UnboundedSender<LanTunnelEvent>>,
) -> ThkResult<VirtualPortClientHandle> {
    let status = Arc::new(RwLock::new(VirtualPortClientStatus::Idle));
    let ticket = match decode_room_ticket(room_code) {
        Ok(t) => t,
        Err(err) => {
            emit_client_status(
                &events_tx,
                &status,
                VirtualPortClientStatus::Error(err.to_string()),
            );
            return Err(err);
        }
    };

    emit_client_status(
        &events_tx,
        &status,
        VirtualPortClientStatus::Connecting { preferred_port },
    );

    let listener = match bind_local_virtual_listener(preferred_port).await {
        Ok(l) => l,
        Err(err) => {
            emit_client_status(
                &events_tx,
                &status,
                VirtualPortClientStatus::Error(err.to_string()),
            );
            return Err(err);
        }
    };
    let bound_port = listener
        .local_addr()
        .map_err(|e| ThkError::Process(format!("Không đọc được cổng ảo cục bộ: {e}")))?
        .port();
    let local_endpoint = format!("127.0.0.1:{bound_port}");

    let endpoint = match build_client_endpoint(offline_local_only).await {
        Ok(ep) => ep,
        Err(err) => {
            emit_client_status(
                &events_tx,
                &status,
                VirtualPortClientStatus::Error(err.to_string()),
            );
            return Err(err);
        }
    };

    let cancel = CancellationToken::new();
    let initial_conn = tokio::select! {
        _ = cancel.cancelled() => {
            endpoint.close().await;
            emit_client_status(&events_tx, &status, VirtualPortClientStatus::Stopped);
            return Err(ThkError::Cancelled);
        }
        res = timeout(Duration::from_secs(12), endpoint.connect(ticket.addr.clone(), VIRTUAL_PORT_ALPN)) => {
            match res {
                Ok(Ok(conn)) => conn,
                Ok(Err(e)) => {
                    let msg = format!("Không thể kết nối tới phòng của bạn bè: {e}");
                    endpoint.close().await;
                    emit_client_status(&events_tx, &status, VirtualPortClientStatus::Error(msg.clone()));
                    return Err(ThkError::Process(msg));
                }
                Err(_) => {
                    let msg = "Quá thời gian kết nối tới phòng (chủ phòng có thể đã đóng hoặc mạng chặn hoàn toàn).".to_string();
                    endpoint.close().await;
                    emit_client_status(&events_tx, &status, VirtualPortClientStatus::Error(msg.clone()));
                    return Err(ThkError::Process(msg));
                }
            }
        }
    };

    let cached_conn = Arc::new(Mutex::new(Some(initial_conn)));
    let info = VirtualPortClientInfo {
        bound_port,
        local_endpoint,
        motd: ticket.motd.clone(),
        remote_node_id: ticket.addr.id.to_string(),
        announcing_lan: announce_on_lan,
        active_streams: 0,
    };
    emit_client_status(&events_tx, &status, VirtualPortClientStatus::Running(info));

    if announce_on_lan {
        let cancel_bcast = cancel.clone();
        let payload = format_lan_announcement(&ticket.motd, bound_port).into_bytes();
        let snap = tokio::task::spawn_blocking(detect_local_network_snapshot)
            .await
            .unwrap_or_default();
        let bcast_ifaces: Vec<Ipv4Addr> = snap
            .interfaces
            .into_iter()
            .filter(|i| !i.is_virtual_hypervisor)
            .map(|i| i.ipv4)
            .collect();
        tokio::spawn(async move {
            send_lan_multicast_frame(&payload, &bcast_ifaces);
            loop {
                tokio::select! {
                    _ = cancel_bcast.cancelled() => break,
                    _ = tokio::time::sleep(LAN_BROADCAST_INTERVAL) => {
                        send_lan_multicast_frame(&payload, &bcast_ifaces);
                    }
                }
            }
        });
    }

    let active_streams = Arc::new(AtomicUsize::new(0));
    let cancel_task = cancel.clone();
    let status_task = Arc::clone(&status);
    let tx_task = events_tx.clone();
    let target_addr = ticket.addr;

    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                _ = cancel_task.cancelled() => break,
                res = listener.accept() => res,
            };
            let Ok((tcp_stream, _)) = accepted else {
                continue;
            };
            let ep_ref = endpoint.clone();
            let addr_ref = target_addr.clone();
            let conn_ref = Arc::clone(&cached_conn);
            let stream_cancel = cancel_task.clone();
            let counter_s = Arc::clone(&active_streams);
            let st_s = Arc::clone(&status_task);
            let tx_s = tx_task.clone();

            tokio::spawn(async move {
                let (send, recv) =
                    match open_bridged_client_stream(&ep_ref, &addr_ref, &conn_ref).await {
                        Ok(pair) => pair,
                        Err(_) => return,
                    };
                let _guard = ActiveStreamGuard::enter(
                    counter_s,
                    ClientStreamSink {
                        status: st_s,
                        events_tx: tx_s,
                    },
                );
                bridge_tcp_and_quic(tcp_stream, send, recv, stream_cancel).await;
            });
        }

        if let Some(conn) = cached_conn.lock().await.take() {
            conn.close(0u32.into(), b"virtual port closed");
        }
        endpoint.close().await;
        emit_client_status(&tx_task, &status_task, VirtualPortClientStatus::Stopped);
    });

    Ok(VirtualPortClientHandle { cancel, status })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn ticket_codec_roundtrip_node_id_fallback_and_invalid_inputs() {
        let secret = iroh::SecretKey::generate();
        let id = secret.public();
        let addr = EndpointAddr::from(id).with_ip_addr(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            41234,
        )));

        let encoded = encode_room_ticket(&addr, "§aMy §lCool [MOTD]Server").unwrap();
        assert!(encoded.starts_with(ROOM_TICKET_PREFIX));
        let decoded = decode_room_ticket(&format!("  \"{encoded}\" \n")).unwrap();
        assert_eq!(decoded.v, ROOM_TICKET_VERSION);
        assert_eq!(decoded.addr.id, id);
        assert_eq!(decoded.motd, "My Cool Server");
        assert_eq!(decoded.addr.ip_addrs().count(), 1);

        let raw_id_ticket = decode_room_ticket(&id.to_string()).unwrap();
        assert_eq!(raw_id_ticket.addr.id, id);
        assert_eq!(raw_id_ticket.motd, "Minecraft World");

        assert!(decode_room_ticket("").is_err());
        assert!(decode_room_ticket("not-a-valid-code").is_err());
        assert!(decode_room_ticket("thk1_!!!invalid_base64!!!").is_err());

        let mut oversized_enc = ZlibEncoder::new(Vec::new(), Compression::default());
        oversized_enc.write_all(&vec![b'a'; 32 * 1024]).unwrap();
        let oversized = format!(
            "{ROOM_TICKET_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(oversized_enc.finish().unwrap())
        );
        assert!(decode_room_ticket(&oversized).is_err());
    }

    #[test]
    fn offline_virtual_port_bridges_concurrent_bidirectional_streams_and_tracks_counts() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            assert!(
                start_virtual_port_host_inner(65431, "NoListener".into(), true, None)
                    .await
                    .is_err()
            );

            let mock_server = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
            let target_port = mock_server.local_addr().unwrap().port();
            tokio::spawn(async move {
                while let Ok((mut sock, _)) = mock_server.accept().await {
                    tokio::spawn(async move {
                        let mut buf = [0u8; 256];
                        while let Ok(n) = sock.read(&mut buf).await {
                            if n == 0 {
                                break;
                            }
                            if sock.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                    });
                }
            });

            let (tx, _rx) = mpsc::unbounded_channel();
            let host = start_virtual_port_host_inner(
                target_port,
                "§bThế giới Test".into(),
                true,
                Some(tx.clone()),
            )
            .await
            .unwrap();
            let room_code = match host.current_status() {
                VirtualPortHostStatus::Running(info) => {
                    assert_eq!(info.local_port, target_port);
                    assert!(!info.relay_connected);
                    info.room_code
                }
                other => panic!("unexpected host status: {other:?}"),
            };

            let client = start_virtual_port_client_inner(&room_code, 0, false, true, Some(tx))
                .await
                .unwrap();
            let client_ep = match client.current_status() {
                VirtualPortClientStatus::Running(info) => {
                    assert_eq!(info.motd, "Thế giới Test");
                    assert!(!info.announcing_lan);
                    info.local_endpoint
                }
                other => panic!("unexpected client status: {other:?}"),
            };

            let mut conns = Vec::new();
            for i in 0..3 {
                let mut stream = TcpStream::connect(&client_ep).await.unwrap();
                let payload = format!("mc-handshake-packet-{i}");
                stream.write_all(payload.as_bytes()).await.unwrap();
                let mut reply = vec![0u8; payload.len()];
                timeout(Duration::from_secs(5), stream.read_exact(&mut reply))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(reply, payload.as_bytes());
                conns.push(stream);
            }

            tokio::time::sleep(Duration::from_millis(80)).await;
            if let VirtualPortHostStatus::Running(info) = host.current_status() {
                assert_eq!(info.active_streams, 3);
            } else {
                panic!("host not running while streams active");
            }
            if let VirtualPortClientStatus::Running(info) = client.current_status() {
                assert_eq!(info.active_streams, 3);
            } else {
                panic!("client not running while streams active");
            }

            drop(conns);
            let mut returned_to_zero = false;
            for _ in 0..40 {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let h_zero = matches!(
                    host.current_status(),
                    VirtualPortHostStatus::Running(info) if info.active_streams == 0
                );
                let c_zero = matches!(
                    client.current_status(),
                    VirtualPortClientStatus::Running(info) if info.active_streams == 0
                );
                if h_zero && c_zero {
                    returned_to_zero = true;
                    break;
                }
            }
            assert!(returned_to_zero, "active_streams did not drop back to 0");

            client.stop();
            host.stop();
            tokio::time::sleep(Duration::from_millis(120)).await;
            assert!(matches!(
                client.current_status(),
                VirtualPortClientStatus::Stopped
            ));
            assert!(matches!(
                host.current_status(),
                VirtualPortHostStatus::Stopped
            ));
        });
    }
}
