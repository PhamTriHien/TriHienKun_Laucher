use crate::lan_tunnel::{MC_LAN_MULTICAST_GROUP, MC_LAN_MULTICAST_PORT};
use crate::ping::strip_section_codes;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashSet;
use std::net::{
    IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, UdpSocket as StdUdpSocket,
};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use sysinfo::Networks;
use thk_core::downloader::{configure_reqwest_client_builder, USER_AGENT};
use thk_core::{ThkError, ThkResult};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const LAN_BROADCAST_INTERVAL: Duration = Duration::from_millis(1500);
const SSDP_MULTICAST_ADDR: SocketAddrV4 =
    SocketAddrV4::new(Ipv4Addr::new(239, 255, 255, 250), 1900);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkInterfaceEntry {
    pub name: String,
    pub ipv4: Ipv4Addr,
    pub is_primary: bool,
    pub is_virtual_hypervisor: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectedNetworkSnapshot {
    pub primary_ipv4: Option<Ipv4Addr>,
    pub interfaces: Vec<NetworkInterfaceEntry>,
    pub global_ipv6: Option<Ipv6Addr>,
    pub public_ipv4: Option<Ipv4Addr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpnpPortStatus {
    Disabled,
    Mapping,
    Mapped {
        external_ip: Ipv4Addr,
        external_port: u16,
        gateway_label: String,
    },
    CarrierGradeNat {
        wan_ip: Ipv4Addr,
        external_port: u16,
    },
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectHostActiveInfo {
    pub local_port: u16,
    pub host_port: u16,
    pub lan_endpoints: Vec<(String, String)>,
    pub ipv6_endpoint: Option<String>,
    pub public_ipv4_endpoint: Option<String>,
    pub upnp_status: UpnpPortStatus,
    pub announcing_lan: bool,
    pub bridging_tcp: bool,
    pub active_clients: usize,
}

impl DirectHostActiveInfo {
    pub fn primary_lan_endpoint(&self) -> Option<&str> {
        self.lan_endpoints.first().map(|(_, ep)| ep.as_str())
    }

    pub fn preferred_online_endpoint(&self) -> Option<String> {
        if let UpnpPortStatus::Mapped {
            external_ip,
            external_port,
            ..
        } = &self.upnp_status
        {
            return Some(format!("{external_ip}:{external_port}"));
        }
        self.ipv6_endpoint.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectHostStatus {
    Idle,
    Starting { local_port: u16, host_port: u16 },
    Running(DirectHostActiveInfo),
    Stopped,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct DirectHostConfig {
    pub local_target_port: u16,
    pub host_port: u16,
    pub bind_ipv4: Option<Ipv4Addr>,
    pub motd: String,
    pub announce_on_lan: bool,
    pub try_upnp: bool,
}

#[derive(Clone)]
pub struct DirectPortHostHandle {
    cancel: CancellationToken,
    status: Arc<RwLock<DirectHostStatus>>,
}

impl DirectPortHostHandle {
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    pub fn current_status(&self) -> DirectHostStatus {
        self.status
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

pub fn is_global_unicast_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return false;
    }
    let segs = ip.segments();
    if (segs[0] & 0xe000) != 0x2000 {
        return false;
    }
    if segs[0] == 0x2001 && (segs[1] == 0x0db8 || segs[1] == 0x0000) {
        return false;
    }
    true
}

pub fn is_cgnat_or_non_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || (octets[0] == 100 && (octets[1] & 0b1100_0000) == 64)
}

pub fn is_usable_lan_ipv4(ip: Ipv4Addr) -> bool {
    !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_unspecified()
        && !ip.is_broadcast()
        && !ip.is_multicast()
}

pub fn detect_primary_routed_ipv4() -> Option<Ipv4Addr> {
    for probe in ["1.1.1.1:80", "8.8.8.8:53"] {
        if let Ok(sock) = StdUdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
            if sock.connect(probe).is_ok() {
                if let Ok(local_addr) = sock.local_addr() {
                    if let IpAddr::V4(v4) = local_addr.ip() {
                        if is_usable_lan_ipv4(v4) {
                            return Some(v4);
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn detect_primary_global_ipv6() -> Option<Ipv6Addr> {
    for probe in ["[2606:4700:4700::1111]:80", "[2001:4860:4860::8888]:53"] {
        if let Ok(sock) = StdUdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)) {
            if sock.connect(probe).is_ok() {
                if let Ok(local_addr) = sock.local_addr() {
                    if let IpAddr::V6(v6) = local_addr.ip() {
                        if is_global_unicast_ipv6(v6) {
                            return Some(v6);
                        }
                    }
                }
            }
        }
    }
    None
}

fn is_hypervisor_adapter_name(name: &str) -> bool {
    let low = name.to_ascii_lowercase();
    low.contains("vmware")
        || low.contains("vmnet")
        || low.contains("vboxnet")
        || low.contains("virtualbox")
        || low.contains("vethernet (wsl")
        || low.contains("vethernet (default switch")
        || low.starts_with("docker")
        || low.starts_with("virbr")
}

fn friendly_interface_label(raw_name: &str) -> String {
    let trimmed = raw_name.trim();
    if trimmed.is_empty() {
        return "LAN".to_string();
    }
    let low = trimmed.to_ascii_lowercase();
    if low == "wlan0" {
        "Wi-Fi".to_string()
    } else if matches!(low.as_str(), "ap0" | "swlan0" | "wlan1" | "softap0") {
        "Hotspot".to_string()
    } else if low.starts_with("rndis") {
        "USB LAN".to_string()
    } else if low.starts_with("tun") {
        format!("VPN ({trimmed})")
    } else {
        trimmed.to_string()
    }
}

#[derive(Deserialize)]
struct AndroidBridgeNetworkJson {
    #[serde(default)]
    interfaces: Vec<AndroidBridgeIfaceItem>,
    #[serde(default)]
    ipv6: Vec<String>,
}

#[derive(Deserialize)]
struct AndroidBridgeIfaceItem {
    name: String,
    ipv4: String,
}

fn read_android_bridge_networks(
    bridge_dir: Option<&Path>,
) -> (Vec<(String, Ipv4Addr)>, Option<Ipv6Addr>) {
    let resolved_dir = bridge_dir
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("THK_ANDROID_BRIDGE_DIR").map(std::path::PathBuf::from));
    let Some(dir) = resolved_dir else {
        return (Vec::new(), None);
    };
    let Ok(raw) = std::fs::read_to_string(dir.join("networks.json")) else {
        return (Vec::new(), None);
    };
    let Ok(parsed) = serde_json::from_str::<AndroidBridgeNetworkJson>(&raw) else {
        return (Vec::new(), None);
    };
    let mut v4_list = Vec::new();
    for item in parsed.interfaces {
        if let Ok(v4) = item.ipv4.trim().parse::<Ipv4Addr>() {
            if is_usable_lan_ipv4(v4) {
                v4_list.push((friendly_interface_label(&item.name), v4));
            }
        }
    }
    let v6_first = parsed
        .ipv6
        .into_iter()
        .filter_map(|s| s.trim().parse::<Ipv6Addr>().ok())
        .find(|&ip| is_global_unicast_ipv6(ip));
    (v4_list, v6_first)
}

pub fn detect_local_network_snapshot() -> ConnectedNetworkSnapshot {
    detect_local_network_snapshot_with_bridge(None)
}

pub fn detect_local_network_snapshot_with_bridge(
    android_bridge_dir: Option<&Path>,
) -> ConnectedNetworkSnapshot {
    let primary_ipv4 = detect_primary_routed_ipv4();
    let mut global_ipv6 = detect_primary_global_ipv6();
    let mut seen_v4 = HashSet::new();
    let mut interfaces = Vec::new();

    let (bridge_v4, bridge_v6) = read_android_bridge_networks(android_bridge_dir);
    if global_ipv6.is_none() {
        global_ipv6 = bridge_v6;
    }
    for (label, v4) in bridge_v4 {
        if seen_v4.insert(v4) {
            interfaces.push(NetworkInterfaceEntry {
                is_primary: Some(v4) == primary_ipv4,
                is_virtual_hypervisor: is_hypervisor_adapter_name(&label),
                name: label,
                ipv4: v4,
            });
        }
    }

    if let Ok(networks) = Networks::new_with_refreshed_list() {
        for (iface_os, data) in &networks {
            let raw_name = iface_os.to_string_lossy();
            let label = friendly_interface_label(&raw_name);
            let is_virt = is_hypervisor_adapter_name(&raw_name);
            for ip_net in data.ip_networks() {
                match ip_net.addr {
                    IpAddr::V4(v4) if is_usable_lan_ipv4(v4) && seen_v4.insert(v4) => {
                        interfaces.push(NetworkInterfaceEntry {
                            name: label.clone(),
                            ipv4: v4,
                            is_primary: Some(v4) == primary_ipv4,
                            is_virtual_hypervisor: is_virt,
                        });
                    }
                    IpAddr::V6(v6) if global_ipv6.is_none() && is_global_unicast_ipv6(v6) => {
                        global_ipv6 = Some(v6);
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(primary) = primary_ipv4 {
        if seen_v4.insert(primary) {
            interfaces.push(NetworkInterfaceEntry {
                name: "LAN / Wi-Fi".to_string(),
                ipv4: primary,
                is_primary: true,
                is_virtual_hypervisor: false,
            });
        }
    }

    interfaces.sort_by(|a, b| {
        b.is_primary
            .cmp(&a.is_primary)
            .then_with(|| a.is_virtual_hypervisor.cmp(&b.is_virtual_hypervisor))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.ipv4.cmp(&b.ipv4))
    });

    let effective_primary = primary_ipv4.or_else(|| {
        interfaces
            .iter()
            .find(|i| !i.is_virtual_hypervisor)
            .or_else(|| interfaces.first())
            .map(|i| i.ipv4)
    });

    ConnectedNetworkSnapshot {
        primary_ipv4: effective_primary,
        interfaces,
        global_ipv6,
        public_ipv4: None,
    }
}

pub async fn fetch_public_ipv4_address() -> Option<Ipv4Addr> {
    let client = configure_reqwest_client_builder(
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_millis(2200)),
    )
    .build()
    .ok()?;

    for endpoint in [
        "https://api4.ipify.org",
        "https://checkip.amazonaws.com",
        "https://ipv4.icanhazip.com",
    ] {
        if let Ok(resp) = client.get(endpoint).send().await {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    if let Ok(ip) = text.trim().parse::<Ipv4Addr>() {
                        if !is_cgnat_or_non_public_ipv4(ip) {
                            return Some(ip);
                        }
                    }
                }
            }
        }
    }
    None
}

pub async fn probe_connected_network_snapshot() -> ConnectedNetworkSnapshot {
    let mut snap = tokio::task::spawn_blocking(detect_local_network_snapshot)
        .await
        .unwrap_or_default();
    snap.public_ipv4 = fetch_public_ipv4_address().await;
    snap
}

pub fn format_lan_announcement(motd: &str, port: u16) -> String {
    let stripped = strip_section_codes(motd);
    let sanitized: String = stripped
        .replace("[/MOTD]", "")
        .replace("[MOTD]", "")
        .replace("[/AD]", "")
        .replace("[AD]", "")
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    let final_motd = if sanitized.trim().is_empty() {
        "TriHienKun LAN Server"
    } else {
        sanitized.trim()
    };
    format!("[MOTD]{final_motd}[/MOTD][AD]{port}[/AD]")
}

#[derive(Clone)]
struct ActiveUpnpLease {
    control_url: String,
    service_type: String,
    external_port: u16,
}

fn is_private_lan_gateway_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(_) => false,
    }
}

fn extract_ssdp_location(packet: &str) -> Option<String> {
    for line in packet.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("location") {
                let loc = v.trim();
                if loc.starts_with("http://") {
                    return Some(loc.to_string());
                }
            }
        }
    }
    None
}

fn resolve_control_url(location_url: &str, control_path: &str) -> Option<String> {
    let trimmed = control_path.trim();
    if trimmed.starts_with("http://") {
        return Some(trimmed.to_string());
    }
    let after_scheme = location_url.strip_prefix("http://")?;
    let origin_end = after_scheme.find('/').unwrap_or(after_scheme.len());
    let origin = &after_scheme[..origin_end];
    if trimmed.starts_with('/') {
        Some(format!("http://{origin}{trimmed}"))
    } else {
        let base_dir = match after_scheme.rfind('/') {
            Some(idx) => &after_scheme[..=idx],
            None => return Some(format!("http://{origin}/{trimmed}")),
        };
        Some(format!("http://{base_dir}{trimmed}"))
    }
}

fn parse_upnp_igd_description(xml: &str, location_url: &str) -> Option<(String, String, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut current_tag = String::new();
    let mut in_service = false;
    let mut friendly_name = String::new();
    let mut svc_type = String::new();
    let mut ctrl_url = String::new();
    let mut best_match: Option<(u8, String, String)> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let lname = e.local_name();
                let local = AsRef::<str>::as_ref(&lname).to_ascii_lowercase();
                if local == "service" {
                    in_service = true;
                    svc_type.clear();
                    ctrl_url.clear();
                }
                current_tag = local;
            }
            Ok(Event::Text(t)) => {
                let val = t.html_content();
                let text = val.trim();
                if text.is_empty() {
                    continue;
                }
                if current_tag == "friendlyname" && friendly_name.is_empty() {
                    friendly_name = text.to_string();
                } else if in_service && current_tag == "servicetype" {
                    svc_type = text.to_string();
                } else if in_service && current_tag == "controlurl" {
                    ctrl_url = text.to_string();
                }
            }
            Ok(Event::End(e)) => {
                let lname = e.local_name();
                let local = AsRef::<str>::as_ref(&lname).to_ascii_lowercase();
                if local == "service" && in_service {
                    in_service = false;
                    let priority = if svc_type.contains("WANIPConnection:2") {
                        Some(3u8)
                    } else if svc_type.contains("WANIPConnection:1") {
                        Some(2u8)
                    } else if svc_type.contains("WANPPPConnection:1") {
                        Some(1u8)
                    } else {
                        None
                    };
                    if let (Some(prio), Some(resolved)) =
                        (priority, resolve_control_url(location_url, &ctrl_url))
                    {
                        if best_match.as_ref().is_none_or(|(p, _, _)| prio > *p) {
                            best_match = Some((prio, svc_type.clone(), resolved));
                        }
                    }
                }
                current_tag.clear();
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    let (_, matched_service, matched_url) = best_match?;
    let gw = if friendly_name.is_empty() {
        "IGD Router".to_string()
    } else {
        friendly_name
    };
    Some((gw, matched_service, matched_url))
}

fn extract_xml_tag_value(xml: &str, target_local_tag: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut current_match = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let lname = e.local_name();
                current_match = AsRef::<str>::as_ref(&lname).eq_ignore_ascii_case(target_local_tag);
            }
            Ok(Event::Text(t)) if current_match => {
                let decoded = t.html_content();
                let trimmed = decoded.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            Ok(Event::End(_)) => {
                current_match = false;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    None
}

async fn send_soap_action(
    client: &reqwest::Client,
    control_url: &str,
    service_type: &str,
    action: &str,
    inner_body: &str,
) -> Result<String, String> {
    let envelope = format!(
        r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:{action} xmlns:u="{service_type}">
      {inner_body}
    </u:{action}>
  </s:Body>
</s:Envelope>"#
    );
    let soap_action_hdr = format!("\"{service_type}#{action}\"");
    let resp = client
        .post(control_url)
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("SOAPAction", soap_action_hdr)
        .body(envelope)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        let fault = extract_xml_tag_value(&text, "errorDescription")
            .or_else(|| extract_xml_tag_value(&text, "faultstring"))
            .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
        return Err(fault);
    }
    Ok(text)
}

async fn discover_and_map_upnp_port(
    local_lan_ip: Ipv4Addr,
    internal_port: u16,
    external_port: u16,
) -> (UpnpPortStatus, Option<ActiveUpnpLease>) {
    let sock = match UdpSocket::bind(SocketAddrV4::new(local_lan_ip, 0)).await {
        Ok(s) => s,
        Err(e) => {
            return (
                UpnpPortStatus::Unavailable(format!("UDP bind failed: {e}")),
                None,
            );
        }
    };

    for st in [
        "urn:schemas-upnp-org:service:WANIPConnection:2",
        "urn:schemas-upnp-org:service:WANIPConnection:1",
        "urn:schemas-upnp-org:service:WANPPPConnection:1",
        "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
    ] {
        let query = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: {st}\r\n\r\n"
        );
        let _ = sock.send_to(query.as_bytes(), SSDP_MULTICAST_ADDR).await;
    }

    let mut locations = Vec::new();
    let mut buf = [0u8; 2048];
    let deadline = tokio::time::Instant::now() + Duration::from_millis(1400);
    while tokio::time::Instant::now() < deadline && locations.len() < 4 {
        let rem = deadline.saturating_duration_since(tokio::time::Instant::now());
        let Ok(Ok((len, src_addr))) = timeout(rem, sock.recv_from(&mut buf)).await else {
            break;
        };
        if !is_private_lan_gateway_ip(src_addr.ip()) {
            continue;
        }
        let pkt = String::from_utf8_lossy(&buf[..len]);
        if let Some(loc) = extract_ssdp_location(&pkt) {
            if !locations.contains(&loc) {
                locations.push(loc);
            }
        }
    }

    if locations.is_empty() {
        return (
            UpnpPortStatus::Unavailable("Modem tắt UPnP hoặc không phản hồi SSDP".to_string()),
            None,
        );
    }

    let Ok(http) = configure_reqwest_client_builder(
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_millis(2000)),
    )
    .build() else {
        return (
            UpnpPortStatus::Unavailable("HTTP client init failed".to_string()),
            None,
        );
    };

    let mut last_err = "Không tìm thấy dịch vụ WAN trên Modem".to_string();
    for loc in locations {
        let Ok(desc_resp) = http.get(&loc).send().await else {
            continue;
        };
        if !desc_resp.status().is_success() {
            continue;
        }
        let Ok(xml_body) = desc_resp.text().await else {
            continue;
        };
        let Some((gateway_label, service_type, control_url)) =
            parse_upnp_igd_description(&xml_body, &loc)
        else {
            continue;
        };

        let map_body = format!(
            "<NewRemoteHost></NewRemoteHost>\
             <NewExternalPort>{external_port}</NewExternalPort>\
             <NewProtocol>TCP</NewProtocol>\
             <NewInternalPort>{internal_port}</NewInternalPort>\
             <NewInternalClient>{local_lan_ip}</NewInternalClient>\
             <NewEnabled>1</NewEnabled>\
             <NewPortMappingDescription>TriHienKun Minecraft</NewPortMappingDescription>\
             <NewLeaseDuration>0</NewLeaseDuration>"
        );

        match send_soap_action(
            &http,
            &control_url,
            &service_type,
            "AddPortMapping",
            &map_body,
        )
        .await
        {
            Ok(_) => {
                let lease = ActiveUpnpLease {
                    control_url: control_url.clone(),
                    service_type: service_type.clone(),
                    external_port,
                };
                let wan_ip = send_soap_action(
                    &http,
                    &control_url,
                    &service_type,
                    "GetExternalIPAddress",
                    "",
                )
                .await
                .ok()
                .and_then(|resp_xml| extract_xml_tag_value(&resp_xml, "NewExternalIPAddress"))
                .and_then(|s| s.parse::<Ipv4Addr>().ok());

                if let Some(ip) = wan_ip {
                    if is_cgnat_or_non_public_ipv4(ip) {
                        return (
                            UpnpPortStatus::CarrierGradeNat {
                                wan_ip: ip,
                                external_port,
                            },
                            Some(lease),
                        );
                    }
                    return (
                        UpnpPortStatus::Mapped {
                            external_ip: ip,
                            external_port,
                            gateway_label,
                        },
                        Some(lease),
                    );
                }

                if let Some(pub_ip) = fetch_public_ipv4_address().await {
                    return (
                        UpnpPortStatus::Mapped {
                            external_ip: pub_ip,
                            external_port,
                            gateway_label,
                        },
                        Some(lease),
                    );
                }

                return (
                    UpnpPortStatus::Unavailable(
                        "Đã gửi lệnh mở cổng nhưng không đọc được IP WAN".to_string(),
                    ),
                    Some(lease),
                );
            }
            Err(err) => {
                last_err = err;
            }
        }
    }

    (UpnpPortStatus::Unavailable(last_err), None)
}

async fn release_upnp_lease(lease: ActiveUpnpLease) {
    let Ok(http) = configure_reqwest_client_builder(
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_millis(1500)),
    )
    .build() else {
        return;
    };
    let body = format!(
        "<NewRemoteHost></NewRemoteHost>\
         <NewExternalPort>{}</NewExternalPort>\
         <NewProtocol>TCP</NewProtocol>",
        lease.external_port
    );
    let _ = send_soap_action(
        &http,
        &lease.control_url,
        &lease.service_type,
        "DeletePortMapping",
        &body,
    )
    .await;
}

fn bind_tcp_v4_listener(bind_ip: Ipv4Addr, port: u16) -> std::io::Result<TcpListener> {
    let sock = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
    #[cfg(not(windows))]
    let _ = sock.set_reuse_address(true);
    sock.bind(&SocketAddr::V4(SocketAddrV4::new(bind_ip, port)).into())?;
    sock.listen(128)?;
    sock.set_nonblocking(true)?;
    TcpListener::from_std(sock.into())
}

fn bind_tcp_v6_only_listener(port: u16) -> std::io::Result<TcpListener> {
    let sock = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
    sock.set_only_v6(true)?;
    #[cfg(not(windows))]
    let _ = sock.set_reuse_address(true);
    sock.bind(&SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0)).into())?;
    sock.listen(128)?;
    sock.set_nonblocking(true)?;
    TcpListener::from_std(sock.into())
}

async fn is_tcp_addr_reachable(addr: SocketAddr, wait: Duration) -> bool {
    timeout(wait, TcpStream::connect(addr))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
}

struct ClientGuard {
    counter: Arc<AtomicUsize>,
    status: Arc<RwLock<DirectHostStatus>>,
    events_tx: Option<mpsc::UnboundedSender<DirectHostStatus>>,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        let next = self
            .counter
            .fetch_sub(1, Ordering::SeqCst)
            .saturating_sub(1);
        update_active_client_count(&self.status, &self.events_tx, next);
    }
}

fn update_active_client_count(
    status: &Arc<RwLock<DirectHostStatus>>,
    events_tx: &Option<mpsc::UnboundedSender<DirectHostStatus>>,
    count: usize,
) {
    let mut maybe_updated = None;
    {
        let mut guard = status.write().unwrap_or_else(|e| e.into_inner());
        if let DirectHostStatus::Running(ref mut info) = *guard {
            if info.active_clients != count {
                info.active_clients = count;
                maybe_updated = Some(guard.clone());
            }
        }
    }
    if let (Some(st), Some(tx)) = (maybe_updated, events_tx) {
        let _ = tx.send(st);
    }
}

fn spawn_tcp_bridge_worker(
    listener: TcpListener,
    local_target_port: u16,
    cancel: CancellationToken,
    active_clients: Arc<AtomicUsize>,
    status: Arc<RwLock<DirectHostStatus>>,
    events_tx: Option<mpsc::UnboundedSender<DirectHostStatus>>,
) {
    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                _ = cancel.cancelled() => break,
                res = listener.accept() => res,
            };
            let Ok((mut inbound, _)) = accepted else {
                continue;
            };
            let _ = inbound.set_nodelay(true);
            let child_cancel = cancel.clone();
            let counter = Arc::clone(&active_clients);
            let st_ref = Arc::clone(&status);
            let tx_ref = events_tx.clone();

            tokio::spawn(async move {
                let target_v4 =
                    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, local_target_port));
                let Ok(Ok(mut outbound)) =
                    timeout(Duration::from_secs(3), TcpStream::connect(target_v4)).await
                else {
                    return;
                };
                let _ = outbound.set_nodelay(true);

                let now_count = counter.fetch_add(1, Ordering::SeqCst) + 1;
                update_active_client_count(&st_ref, &tx_ref, now_count);
                let _guard = ClientGuard {
                    counter,
                    status: st_ref,
                    events_tx: tx_ref,
                };

                tokio::select! {
                    _ = child_cancel.cancelled() => {}
                    _ = copy_bidirectional(&mut inbound, &mut outbound) => {}
                }
            });
        }
    });
}

fn send_lan_multicast_frame(payload: &[u8], interfaces: &[Ipv4Addr]) {
    let dest = SocketAddrV4::new(MC_LAN_MULTICAST_GROUP, MC_LAN_MULTICAST_PORT);
    if interfaces.is_empty() {
        if let Ok(sock) = StdUdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
            let _ = sock.set_multicast_ttl_v4(4);
            let _ = sock.send_to(payload, dest);
        }
        return;
    }

    for &iface_ip in interfaces {
        if let Ok(sock) = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
            let _ = sock.set_multicast_ttl_v4(4);
            let _ = sock.set_multicast_if_v4(&iface_ip);
            if sock
                .bind(&SocketAddr::V4(SocketAddrV4::new(iface_ip, 0)).into())
                .is_ok()
            {
                let std_sock: StdUdpSocket = sock.into();
                let _ = std_sock.send_to(payload, dest);
            }
        }
    }
}

pub async fn start_direct_port_host(
    config: DirectHostConfig,
    events_tx: Option<mpsc::UnboundedSender<DirectHostStatus>>,
) -> ThkResult<DirectPortHostHandle> {
    if config.local_target_port == 0 || config.host_port == 0 {
        return Err(ThkError::InvalidInput(
            "Port must be between 1 and 65535.".into(),
        ));
    }

    let local_v4 = SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::LOCALHOST,
        config.local_target_port,
    ));
    if !is_tcp_addr_reachable(local_v4, Duration::from_millis(650)).await {
        return Err(ThkError::Process(format!(
            "Chưa có thế giới hoặc máy chủ nào đang mở tại 127.0.0.1:{} (hãy khởi động Server hoặc bật Open to LAN trước).",
            config.local_target_port
        )));
    }

    let starting = DirectHostStatus::Starting {
        local_port: config.local_target_port,
        host_port: config.host_port,
    };
    let status = Arc::new(RwLock::new(starting.clone()));
    if let Some(ref tx) = events_tx {
        let _ = tx.send(starting);
    }

    let snap = tokio::task::spawn_blocking(detect_local_network_snapshot)
        .await
        .unwrap_or_default();

    let bind_ip = config.bind_ipv4.unwrap_or(Ipv4Addr::UNSPECIFIED);
    let cancel = CancellationToken::new();
    let active_clients = Arc::new(AtomicUsize::new(0));

    let mut bridging_tcp = false;
    let mut ipv6_ready = false;

    if config.host_port != config.local_target_port {
        let v4_listener = bind_tcp_v4_listener(bind_ip, config.host_port).map_err(|e| {
            ThkError::Process(format!(
                "Không thể mở cổng {} trên mạng hiện tại (cổng có thể đang bận): {e}",
                config.host_port
            ))
        })?;
        bridging_tcp = true;
        spawn_tcp_bridge_worker(
            v4_listener,
            config.local_target_port,
            cancel.clone(),
            Arc::clone(&active_clients),
            Arc::clone(&status),
            events_tx.clone(),
        );

        if config.bind_ipv4.is_none() {
            if let Ok(v6_listener) = bind_tcp_v6_only_listener(config.host_port) {
                ipv6_ready = true;
                spawn_tcp_bridge_worker(
                    v6_listener,
                    config.local_target_port,
                    cancel.clone(),
                    Arc::clone(&active_clients),
                    Arc::clone(&status),
                    events_tx.clone(),
                );
            }
        }
    } else {
        if let Ok(v4_listener) = bind_tcp_v4_listener(bind_ip, config.host_port) {
            bridging_tcp = true;
            spawn_tcp_bridge_worker(
                v4_listener,
                config.local_target_port,
                cancel.clone(),
                Arc::clone(&active_clients),
                Arc::clone(&status),
                events_tx.clone(),
            );
        } else if let Some(check_ip) = config.bind_ipv4.or(snap.primary_ipv4) {
            let lan_sock = SocketAddr::V4(SocketAddrV4::new(check_ip, config.host_port));
            if !is_tcp_addr_reachable(lan_sock, Duration::from_millis(500)).await {
                let candidate_ips: Vec<Ipv4Addr> = match config.bind_ipv4 {
                    Some(ip) => vec![ip],
                    None => snap
                        .interfaces
                        .iter()
                        .filter(|i| !i.is_virtual_hypervisor || snap.interfaces.len() == 1)
                        .map(|i| i.ipv4)
                        .collect(),
                };
                let mut bound_any = false;
                for nic_ip in candidate_ips {
                    if let Ok(nic_listener) = bind_tcp_v4_listener(nic_ip, config.host_port) {
                        bound_any = true;
                        bridging_tcp = true;
                        spawn_tcp_bridge_worker(
                            nic_listener,
                            config.local_target_port,
                            cancel.clone(),
                            Arc::clone(&active_clients),
                            Arc::clone(&status),
                            events_tx.clone(),
                        );
                    }
                }
                if !bound_any {
                    return Err(ThkError::Process(format!(
                        "Cổng {} trên mạng hiện tại đang bận hoặc bị tường lửa chặn.",
                        config.host_port
                    )));
                }
            }
        }

        if config.bind_ipv4.is_none() {
            match bind_tcp_v6_only_listener(config.host_port) {
                Ok(v6_listener) => {
                    bridging_tcp = true;
                    ipv6_ready = true;
                    spawn_tcp_bridge_worker(
                        v6_listener,
                        config.local_target_port,
                        cancel.clone(),
                        Arc::clone(&active_clients),
                        Arc::clone(&status),
                        events_tx.clone(),
                    );
                }
                Err(_) => {
                    let lo_v6 = SocketAddr::V6(SocketAddrV6::new(
                        Ipv6Addr::LOCALHOST,
                        config.host_port,
                        0,
                        0,
                    ));
                    ipv6_ready = is_tcp_addr_reachable(lo_v6, Duration::from_millis(350)).await;
                }
            }
        }
    }

    let mut lan_endpoints = Vec::new();
    let mut multicast_ifaces = Vec::new();
    if let Some(pinned) = config.bind_ipv4 {
        let name = snap
            .interfaces
            .iter()
            .find(|i| i.ipv4 == pinned)
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "LAN".to_string());
        lan_endpoints.push((name, format!("{pinned}:{}", config.host_port)));
        multicast_ifaces.push(pinned);
    } else {
        for iface in &snap.interfaces {
            if !iface.is_virtual_hypervisor || snap.interfaces.len() == 1 {
                lan_endpoints.push((
                    iface.name.clone(),
                    format!("{}:{}", iface.ipv4, config.host_port),
                ));
                multicast_ifaces.push(iface.ipv4);
            }
        }
        if lan_endpoints.is_empty() {
            lan_endpoints.push((
                "Local".to_string(),
                format!("127.0.0.1:{}", config.host_port),
            ));
        }
    }

    let ipv6_endpoint = if ipv6_ready {
        snap.global_ipv6
            .map(|v6| format!("[{v6}]:{}", config.host_port))
    } else {
        None
    };

    if config.announce_on_lan {
        let cancel_bcast = cancel.clone();
        let payload = format_lan_announcement(&config.motd, config.host_port).into_bytes();
        let bcast_ifaces = multicast_ifaces.clone();
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

    let initial_upnp = if config.try_upnp {
        UpnpPortStatus::Mapping
    } else {
        UpnpPortStatus::Disabled
    };

    let running_info = DirectHostActiveInfo {
        local_port: config.local_target_port,
        host_port: config.host_port,
        lan_endpoints,
        ipv6_endpoint,
        public_ipv4_endpoint: None,
        upnp_status: initial_upnp,
        announcing_lan: config.announce_on_lan,
        bridging_tcp,
        active_clients: 0,
    };

    {
        let mut guard = status.write().unwrap_or_else(|e| e.into_inner());
        *guard = DirectHostStatus::Running(running_info.clone());
    }
    if let Some(ref tx) = events_tx {
        let _ = tx.send(DirectHostStatus::Running(running_info));
    }

    let primary_for_upnp = config.bind_ipv4.or(snap.primary_ipv4);
    let cancel_lifecycle = cancel.clone();
    let status_lifecycle = Arc::clone(&status);
    let tx_lifecycle = events_tx.clone();
    let host_port = config.host_port;
    let try_upnp = config.try_upnp;

    tokio::spawn(async move {
        let mut active_lease: Option<ActiveUpnpLease> = None;

        let pub_v4_fut = fetch_public_ipv4_address();
        let upnp_fut = async {
            if !try_upnp {
                return (UpnpPortStatus::Disabled, None);
            }
            let Some(lan_ip) = primary_for_upnp else {
                return (
                    UpnpPortStatus::Unavailable("Chưa có IPv4 mạng LAN nội bộ".to_string()),
                    None,
                );
            };
            discover_and_map_upnp_port(lan_ip, host_port, host_port).await
        };

        let probe_done = tokio::select! {
            _ = cancel_lifecycle.cancelled() => None,
            pair = async { tokio::join!(pub_v4_fut, upnp_fut) } => Some(pair),
        };

        if let Some((pub_ip, (upnp_res, lease))) = probe_done {
            active_lease = lease;
            let resolved_pub_ep = match &upnp_res {
                UpnpPortStatus::Mapped {
                    external_ip,
                    external_port,
                    ..
                } => Some(format!("{external_ip}:{external_port}")),
                _ => pub_ip.map(|ip| format!("{ip}:{host_port}")),
            };

            let mut updated_state = None;
            {
                let mut guard = status_lifecycle.write().unwrap_or_else(|e| e.into_inner());
                if let DirectHostStatus::Running(ref mut info) = *guard {
                    info.upnp_status = upnp_res;
                    info.public_ipv4_endpoint = resolved_pub_ep;
                    updated_state = Some(guard.clone());
                }
            }
            if let (Some(st), Some(ref tx)) = (updated_state, &tx_lifecycle) {
                let _ = tx.send(st);
            }

            cancel_lifecycle.cancelled().await;
        }

        if let Some(lease) = active_lease {
            release_upnp_lease(lease).await;
        }

        {
            let mut guard = status_lifecycle.write().unwrap_or_else(|e| e.into_inner());
            *guard = DirectHostStatus::Stopped;
        }
        if let Some(tx) = tx_lifecycle {
            let _ = tx.send(DirectHostStatus::Stopped);
        }
    });

    Ok(DirectPortHostHandle { cancel, status })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upnp_igd_xml_and_soap_parsers_handle_wan_services_and_relative_urls() {
        let igd_xml = r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <device>
    <friendlyName>Viettel GPON Home Gateway</friendlyName>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:WANPPPConnection:1</serviceType>
        <controlURL>/upnp/control/wanpppcpp1</controlURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:WANIPConnection:2</serviceType>
        <controlURL>ctl/IPConn</controlURL>
      </service>
    </serviceList>
  </device>
</root>"#;

        let (gw, svc, url) =
            parse_upnp_igd_description(igd_xml, "http://192.168.1.1:5000/desc/root.xml")
                .expect("must extract WANIPConnection:2");
        assert_eq!(gw, "Viettel GPON Home Gateway");
        assert_eq!(svc, "urn:schemas-upnp-org:service:WANIPConnection:2");
        assert_eq!(url, "http://192.168.1.1:5000/desc/ctl/IPConn");

        let soap_resp = r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
            <s:Body>
                <u:GetExternalIPAddressResponse xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:2">
                    <NewExternalIPAddress>14.234.190.58</NewExternalIPAddress>
                </u:GetExternalIPAddressResponse>
            </s:Body>
        </s:Envelope>"#;
        assert_eq!(
            extract_xml_tag_value(soap_resp, "NewExternalIPAddress").as_deref(),
            Some("14.234.190.58")
        );
    }
}
