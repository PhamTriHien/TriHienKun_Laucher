use hickory_resolver::proto::rr::RData;
use hickory_resolver::Resolver;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::time::{Duration, Instant};
use thk_core::{ThkError, ThkResult};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

const DEFAULT_MC_PORT: u16 = 25565;
const PING_PROTOCOL_VERSION: i32 = 767;
const MAX_STATUS_PACKET_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotdSpan {
    pub text: String,
    pub color_hex: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
    pub strikethrough: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatusPing {
    pub address: String,
    pub resolved_host: String,
    pub resolved_port: u16,
    pub online: bool,
    pub latency_ms: Option<u64>,
    pub version_name: String,
    pub protocol: i32,
    pub online_players: u32,
    pub max_players: u32,
    pub sample_players: Vec<String>,
    pub motd_spans: Vec<MotdSpan>,
    pub motd_plain: String,
    pub favicon_data_uri: Option<String>,
}

pub fn parse_host_and_port(address: &str) -> (String, Option<u16>) {
    let trimmed = address.trim();
    if let Some((host, port_str)) = trimmed.rsplit_once(':') {
        if !host.contains(':') {
            if let Ok(port) = port_str.parse::<u16>() {
                return (host.trim().to_string(), Some(port));
            }
        }
    }
    (trimmed.to_string(), None)
}

pub async fn resolve_minecraft_srv(host: &str, explicit_port: Option<u16>) -> (String, u16) {
    if let Some(port) = explicit_port {
        return (host.to_string(), port);
    }
    if host.parse::<IpAddr>().is_ok() || host.eq_ignore_ascii_case("localhost") {
        return (host.to_string(), DEFAULT_MC_PORT);
    }

    let srv_query = format!("_minecraft._tcp.{}.", host.trim_end_matches('.'));
    if let Ok(builder) = Resolver::builder_tokio() {
        if let Ok(resolver) = builder.build() {
            if let Ok(Ok(lookup)) =
                timeout(Duration::from_secs(3), resolver.srv_lookup(srv_query)).await
            {
                if let Some(record) = lookup
                    .answers()
                    .iter()
                    .filter_map(|rec| match &rec.data {
                        RData::SRV(srv) => Some(srv),
                        _ => None,
                    })
                    .min_by_key(|r| (r.priority, u16::MAX - r.weight))
                {
                    let target = record.target.to_utf8();
                    let clean_target = target.trim_end_matches('.').to_string();
                    if !clean_target.is_empty() {
                        return (clean_target, record.port);
                    }
                }
            }
        }
    }

    (host.to_string(), DEFAULT_MC_PORT)
}

pub async fn ping_minecraft_server(
    address: &str,
    connect_timeout: Duration,
) -> ThkResult<ServerStatusPing> {
    let (host, explicit_port) = parse_host_and_port(address);
    if host.is_empty() {
        return Err(ThkError::InvalidInput("Server address cannot be empty".into()));
    }

    let (resolved_host, resolved_port) = resolve_minecraft_srv(&host, explicit_port).await;
    let connect_target = format!("{resolved_host}:{resolved_port}");

    let connect_started = Instant::now();
    let mut stream = timeout(connect_timeout, TcpStream::connect(&connect_target))
        .await
        .map_err(|_| ThkError::Process(format!("Connection timed out: {connect_target}")))?
        .map_err(ThkError::Io)?;
    let tcp_rtt_ms = connect_started.elapsed().as_millis() as u64;

    let handshake_pkt = build_handshake_packet(&resolved_host, resolved_port, PING_PROTOCOL_VERSION);
    let status_req_pkt = build_packet(0x00, &[]);

    timeout(connect_timeout, async {
        stream.write_all(&handshake_pkt).await?;
        stream.write_all(&status_req_pkt).await?;
        stream.flush().await
    })
    .await
    .map_err(|_| ThkError::Process("Timed out sending Minecraft status handshake".into()))??;

    let status_json_str = timeout(connect_timeout, read_status_response_json(&mut stream))
        .await
        .map_err(|_| ThkError::Process("Timed out reading Minecraft status packet".into()))??;

    let ping_ms = measure_ping_pong(&mut stream, tcp_rtt_ms).await;

    let root: Value = serde_json::from_str(&status_json_str)?;
    let version_name = root
        .pointer("/version/name")
        .and_then(Value::as_str)
        .unwrap_or("Unknown")
        .to_string();
    let protocol = root
        .pointer("/version/protocol")
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let online_players = root
        .pointer("/players/online")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let max_players = root
        .pointer("/players/max")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;

    let sample_players = root
        .pointer("/players/sample")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("name").and_then(Value::as_str))
                .map(strip_section_codes)
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    let motd_spans = root
        .get("description")
        .map(parse_chat_component_to_spans)
        .unwrap_or_default();
    let motd_plain = motd_spans
        .iter()
        .map(|s| s.text.as_str())
        .collect::<String>()
        .trim()
        .to_string();

    let favicon_data_uri = root
        .get("favicon")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| s.starts_with("data:image/png;base64,"));

    Ok(ServerStatusPing {
        address: address.trim().to_string(),
        resolved_host,
        resolved_port,
        online: true,
        latency_ms: Some(ping_ms),
        version_name,
        protocol,
        online_players,
        max_players,
        sample_players,
        motd_spans,
        motd_plain,
        favicon_data_uri,
    })
}

async fn measure_ping_pong(stream: &mut TcpStream, fallback_ms: u64) -> u64 {
    let payload_ts = 0x5448_4B4C_4155_4348i64;
    let pkt = build_packet(0x01, &payload_ts.to_be_bytes());
    let started = Instant::now();
    let res: Result<u64, ThkError> = timeout(Duration::from_millis(2000), async {
        stream.write_all(&pkt).await?;
        stream.flush().await?;
        let pkt_len = read_varint_async(stream).await? as usize;
        if !(9..=32).contains(&pkt_len) {
            return Err(ThkError::Process("Unexpected pong length".into()));
        }
        let mut buf = vec![0u8; pkt_len];
        stream.read_exact(&mut buf).await?;
        Ok(started.elapsed().as_millis() as u64)
    })
    .await
    .unwrap_or(Ok(fallback_ms));

    res.unwrap_or(fallback_ms)
}

async fn read_status_response_json(stream: &mut TcpStream) -> ThkResult<String> {
    let packet_len = read_varint_async(stream).await?;
    if packet_len <= 0 || (packet_len as usize) > MAX_STATUS_PACKET_BYTES {
        return Err(ThkError::Process(format!(
            "Invalid status packet length: {packet_len}"
        )));
    }
    let mut packet_buf = vec![0u8; packet_len as usize];
    stream.read_exact(&mut packet_buf).await?;

    let (packet_id, consumed_id) = decode_varint_slice(&packet_buf)?;
    if packet_id != 0x00 {
        return Err(ThkError::Process(format!(
            "Unexpected status packet id: {packet_id}"
        )));
    }
    let (json_len, consumed_len) = decode_varint_slice(&packet_buf[consumed_id..])?;
    let start = consumed_id + consumed_len;
    let end = start.saturating_add(json_len.max(0) as usize);
    if end > packet_buf.len() {
        return Err(ThkError::Process("Truncated status JSON payload".into()));
    }

    Ok(String::from_utf8_lossy(&packet_buf[start..end]).to_string())
}

fn build_handshake_packet(host: &str, port: u16, protocol_version: i32) -> Vec<u8> {
    let mut body = Vec::with_capacity(32 + host.len());
    encode_varint(protocol_version, &mut body);
    let host_bytes = host.as_bytes();
    encode_varint(host_bytes.len() as i32, &mut body);
    body.extend_from_slice(host_bytes);
    body.extend_from_slice(&port.to_be_bytes());
    encode_varint(1, &mut body);
    build_packet(0x00, &body)
}

fn build_packet(packet_id: i32, payload: &[u8]) -> Vec<u8> {
    let mut id_bytes = Vec::with_capacity(5);
    encode_varint(packet_id, &mut id_bytes);
    let total_len = id_bytes.len() + payload.len();

    let mut out = Vec::with_capacity(5 + total_len);
    encode_varint(total_len as i32, &mut out);
    out.extend_from_slice(&id_bytes);
    out.extend_from_slice(payload);
    out
}

pub fn encode_varint(mut value: i32, out: &mut Vec<u8>) {
    let mut uval = value as u32;
    loop {
        if (uval & !0x7F) == 0 {
            out.push(uval as u8);
            break;
        }
        out.push(((uval & 0x7F) | 0x80) as u8);
        uval >>= 7;
        let _ = &mut value;
    }
}

pub fn decode_varint_slice(bytes: &[u8]) -> ThkResult<(i32, usize)> {
    let mut num_read = 0usize;
    let mut result = 0i32;
    loop {
        if num_read >= bytes.len() {
            return Err(ThkError::Process("Unexpected EOF decoding VarInt".into()));
        }
        if num_read >= 5 {
            return Err(ThkError::Process("VarInt exceeds 5 bytes".into()));
        }
        let byte = bytes[num_read];
        let value = (byte & 0x7F) as i32;
        result |= value << (7 * num_read);
        num_read += 1;
        if (byte & 0x80) == 0 {
            return Ok((result, num_read));
        }
    }
}

async fn read_varint_async(stream: &mut TcpStream) -> ThkResult<i32> {
    let mut num_read = 0usize;
    let mut result = 0i32;
    let mut buf = [0u8; 1];
    loop {
        if num_read >= 5 {
            return Err(ThkError::Process("VarInt exceeds 5 bytes".into()));
        }
        stream.read_exact(&mut buf).await?;
        let byte = buf[0];
        let value = (byte & 0x7F) as i32;
        result |= value << (7 * num_read);
        num_read += 1;
        if (byte & 0x80) == 0 {
            return Ok(result);
        }
    }
}

#[derive(Default, Clone)]
struct StyleState {
    color_hex: Option<String>,
    bold: bool,
    italic: bool,
    underlined: bool,
    strikethrough: bool,
}

pub fn parse_chat_component_to_spans(value: &Value) -> Vec<MotdSpan> {
    let mut out = Vec::new();
    collect_component_spans(value, &StyleState::default(), &mut out);
    merge_adjacent_spans(out)
}

fn collect_component_spans(value: &Value, inherited: &StyleState, out: &mut Vec<MotdSpan>) {
    match value {
        Value::String(s) => {
            parse_legacy_section_text(s, inherited, out);
        }
        Value::Array(arr) => {
            for item in arr {
                collect_component_spans(item, inherited, out);
            }
        }
        Value::Object(map) => {
            let mut style = inherited.clone();
            if let Some(color_str) = map.get("color").and_then(Value::as_str) {
                if let Some(hex) = named_or_hex_mc_color(color_str) {
                    style.color_hex = Some(hex);
                }
            }
            if let Some(b) = map.get("bold").and_then(Value::as_bool) {
                style.bold = b;
            }
            if let Some(i) = map.get("italic").and_then(Value::as_bool) {
                style.italic = i;
            }
            if let Some(u) = map.get("underlined").and_then(Value::as_bool) {
                style.underlined = u;
            }
            if let Some(s) = map.get("strikethrough").and_then(Value::as_bool) {
                style.strikethrough = s;
            }

            if let Some(text) = map.get("text").and_then(Value::as_str) {
                parse_legacy_section_text(text, &style, out);
            }
            if let Some(extra) = map.get("extra").and_then(Value::as_array) {
                for child in extra {
                    collect_component_spans(child, &style, out);
                }
            }
        }
        _ => {}
    }
}

pub fn parse_legacy_section_str(input: &str) -> Vec<MotdSpan> {
    let mut out = Vec::new();
    parse_legacy_section_text(input, &StyleState::default(), &mut out);
    merge_adjacent_spans(out)
}

fn parse_legacy_section_text(input: &str, initial: &StyleState, out: &mut Vec<MotdSpan>) {
    let mut style = initial.clone();
    let mut current = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '§' {
            if let Some(&code) = chars.peek() {
                chars.next();
                if !current.is_empty() {
                    out.push(MotdSpan {
                        text: std::mem::take(&mut current),
                        color_hex: style.color_hex.clone(),
                        bold: style.bold,
                        italic: style.italic,
                        underlined: style.underlined,
                        strikethrough: style.strikethrough,
                    });
                }
                apply_section_code(code, initial, &mut style);
                continue;
            }
        }
        current.push(ch);
    }

    if !current.is_empty() {
        out.push(MotdSpan {
            text: current,
            color_hex: style.color_hex,
            bold: style.bold,
            italic: style.italic,
            underlined: style.underlined,
            strikethrough: style.strikethrough,
        });
    }
}

fn apply_section_code(code: char, initial: &StyleState, style: &mut StyleState) {
    let c = code.to_ascii_lowercase();
    if let Some(hex) = section_code_color(c) {
        style.color_hex = Some(hex.to_string());
        style.bold = false;
        style.italic = false;
        style.underlined = false;
        style.strikethrough = false;
        return;
    }
    match c {
        'l' => style.bold = true,
        'o' => style.italic = true,
        'n' => style.underlined = true,
        'm' => style.strikethrough = true,
        'r' => *style = initial.clone(),
        _ => {}
    }
}

fn merge_adjacent_spans(spans: Vec<MotdSpan>) -> Vec<MotdSpan> {
    let mut merged: Vec<MotdSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if last.color_hex == span.color_hex
                && last.bold == span.bold
                && last.italic == span.italic
                && last.underlined == span.underlined
                && last.strikethrough == span.strikethrough
            {
                last.text.push_str(&span.text);
                continue;
            }
        }
        merged.push(span);
    }
    merged
}

pub fn strip_section_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            let _ = chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

fn section_code_color(c: char) -> Option<&'static str> {
    match c {
        '0' => Some("#000000"),
        '1' => Some("#0000AA"),
        '2' => Some("#00AA00"),
        '3' => Some("#00AAAA"),
        '4' => Some("#AA0000"),
        '5' => Some("#AA00AA"),
        '6' => Some("#FFAA00"),
        '7' => Some("#AAAAAA"),
        '8' => Some("#555555"),
        '9' => Some("#5555FF"),
        'a' => Some("#55FF55"),
        'b' => Some("#55FFFF"),
        'c' => Some("#FF5555"),
        'd' => Some("#FF55FF"),
        'e' => Some("#FFFF55"),
        'f' => Some("#FFFFFF"),
        _ => None,
    }
}

fn named_or_hex_mc_color(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if let Some(hex_digits) = trimmed.strip_prefix('#') {
        if hex_digits.len() == 6 && hex_digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(format!("#{}", hex_digits.to_ascii_uppercase()));
        }
    }
    let hex = match trimmed.to_ascii_lowercase().as_str() {
        "black" => "#000000",
        "dark_blue" => "#0000AA",
        "dark_green" => "#00AA00",
        "dark_aqua" => "#00AAAA",
        "dark_red" => "#AA0000",
        "dark_purple" => "#AA00AA",
        "gold" => "#FFAA00",
        "gray" => "#AAAAAA",
        "dark_gray" => "#555555",
        "blue" => "#5555FF",
        "green" => "#55FF55",
        "aqua" => "#55FFFF",
        "red" => "#FF5555",
        "light_purple" => "#FF55FF",
        "yellow" => "#FFFF55",
        "white" => "#FFFFFF",
        _ => return None,
    };
    Some(hex.to_string())
}

pub fn detect_primary_lan_ipv4() -> Ipv4Addr {
    if let Ok(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
        if sock.connect("1.1.1.1:80").is_ok() {
            if let Ok(local_addr) = sock.local_addr() {
                if let IpAddr::V4(v4) = local_addr.ip() {
                    if !v4.is_unspecified() {
                        return v4;
                    }
                }
            }
        }
    }
    Ipv4Addr::LOCALHOST
}
