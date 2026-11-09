use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;

pub const GLFW_MOD_SHIFT: i32 = 0x1;
pub const GLFW_MOD_CONTROL: i32 = 0x2;
pub const GLFW_MOD_ALT: i32 = 0x4;
pub const GLFW_MOD_CAPS_LOCK: i32 = 0x10;
pub const GLFW_MOD_NUM_LOCK: i32 = 0x20;

const GLFW_KEY_CAPS_LOCK: i32 = 280;
const GLFW_KEY_NUM_LOCK: i32 = 282;
const GLFW_KEY_LEFT_SHIFT: i32 = 340;
const GLFW_KEY_LEFT_CONTROL: i32 = 341;
const GLFW_KEY_LEFT_ALT: i32 = 342;
const GLFW_KEY_RIGHT_SHIFT: i32 = 344;
const GLFW_KEY_RIGHT_CONTROL: i32 = 345;
const GLFW_KEY_RIGHT_ALT: i32 = 346;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AndroidPermissionSnapshot {
    pub ignore_battery_optimizations: bool,
    pub can_draw_overlays: bool,
    pub notifications_enabled: bool,
    pub keep_alive_running: bool,
    pub overlay_showing: bool,
    pub surface_ready: bool,
    pub surface_width: u32,
    pub surface_height: u32,
    pub game_process_running: bool,
    pub safe_inset_left_px: u32,
    pub safe_inset_top_px: u32,
    pub safe_inset_right_px: u32,
    pub safe_inset_bottom_px: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkIfaceItem {
    pub name: String,
    pub ipv4: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSnapshotJson {
    pub interfaces: Vec<NetworkIfaceItem>,
    pub ipv6: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BridgeCommandJson {
    pub op: String,
    pub enabled: bool,
    pub overlay: bool,
    pub status: String,
    pub session_id: String,
    pub spec_path: String,
    pub window_width: i32,
    pub window_height: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PickRequestJson {
    pub id: String,
    pub multiple: bool,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PickResponseJson {
    pub paths: Vec<String>,
}

fn default_window_width() -> i32 {
    1280
}

fn default_window_height() -> i32 {
    720
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobileLaunchSpecJson {
    pub input_port: u16,
    pub jre_home: String,
    pub runtime_root: String,
    pub lwjgl_natives_dir: String,
    pub instance_dir: String,
    pub log_file: String,
    #[serde(default = "default_window_width")]
    pub window_width: i32,
    #[serde(default = "default_window_height")]
    pub window_height: i32,
    #[serde(default)]
    pub physical_width: Option<i32>,
    #[serde(default)]
    pub physical_height: Option<i32>,
    #[serde(default)]
    pub renderer_lib: String,
    #[serde(default)]
    pub egl_lib: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedInputCmd {
    Quit,
    Key { keycode: i32, down: bool },
    Char { codepoint: u32, mods: i32 },
    MouseButton { button: i32, down: bool },
    CursorPos { x: f32, y: f32 },
    Scroll { dx: f64, dy: f64 },
    WindowSize { width: i32, height: i32 },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GlfwModifierState {
    bits: i32,
}

impl GlfwModifierState {
    pub const fn new() -> Self {
        Self { bits: 0 }
    }

    pub const fn bits(&self) -> i32 {
        self.bits
    }

    pub fn update_key(&mut self, keycode: i32, down: bool) -> i32 {
        let mask = match keycode {
            GLFW_KEY_LEFT_SHIFT | GLFW_KEY_RIGHT_SHIFT => GLFW_MOD_SHIFT,
            GLFW_KEY_LEFT_CONTROL | GLFW_KEY_RIGHT_CONTROL => GLFW_MOD_CONTROL,
            GLFW_KEY_LEFT_ALT | GLFW_KEY_RIGHT_ALT => GLFW_MOD_ALT,
            GLFW_KEY_CAPS_LOCK => GLFW_MOD_CAPS_LOCK,
            GLFW_KEY_NUM_LOCK => GLFW_MOD_NUM_LOCK,
            _ => 0,
        };
        if mask != 0 {
            if down {
                self.bits |= mask;
            } else {
                self.bits &= !mask;
            }
        }
        self.bits
    }
}

pub fn strip_zone_index(addr: &str) -> &str {
    addr.split('%').next().unwrap_or("").trim()
}

pub fn is_valid_lan_ipv4(ip: Ipv4Addr) -> bool {
    !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_unspecified()
        && !ip.is_broadcast()
        && !ip.is_multicast()
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

pub fn build_network_snapshot(
    java_iface_pairs: &[String],
    java_ipv6: &[String],
    native_ifaces: &[(String, String)],
    native_ipv6: &[String],
) -> NetworkSnapshotJson {
    let mut seen_v4 = HashSet::new();
    let mut interfaces = Vec::new();

    let mut push_v4 = |raw_name: &str, raw_ip: &str| {
        let clean_ip = strip_zone_index(raw_ip);
        let Ok(ip) = clean_ip.parse::<Ipv4Addr>() else {
            return;
        };
        if !is_valid_lan_ipv4(ip) {
            return;
        }
        let canonical = ip.to_string();
        if seen_v4.insert(canonical.clone()) {
            let trimmed_name = raw_name.trim();
            let name = if trimmed_name.is_empty() {
                "net".to_string()
            } else {
                trimmed_name.to_string()
            };
            interfaces.push(NetworkIfaceItem {
                name,
                ipv4: canonical,
            });
        }
    };

    for pair in java_iface_pairs {
        if let Some((iface, ip)) = pair.split_once('=') {
            push_v4(iface, ip);
        }
    }
    for (iface, ip) in native_ifaces {
        push_v4(iface, ip);
    }

    let mut seen_v6 = HashSet::new();
    let mut ipv6 = Vec::new();
    for raw in java_ipv6.iter().chain(native_ipv6.iter()) {
        let clean = strip_zone_index(raw);
        let Ok(ip) = clean.parse::<Ipv6Addr>() else {
            continue;
        };
        if !is_global_unicast_ipv6(ip) {
            continue;
        }
        let canonical = ip.to_string();
        if seen_v6.insert(canonical.clone()) {
            ipv6.push(canonical);
        }
    }

    NetworkSnapshotJson { interfaces, ipv6 }
}

pub fn resolve_pick_mime_type(extensions: &[String]) -> &'static str {
    if extensions.len() == 1 && extensions[0].trim().eq_ignore_ascii_case("png") {
        "image/png"
    } else {
        "*/*"
    }
}

pub fn sanitize_picked_filename(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' | '\r' | '\n' => '_',
            _ => c,
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        "picked_file.bin".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn read_saved_orientation(files_dir: &Path) -> String {
    let saved_path = files_dir.join(".thk_bridge").join("saved_orientation.txt");
    let Ok(raw) = fs::read_to_string(&saved_path) else {
        return "full_sensor".to_string();
    };
    let trimmed = raw.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        "full_sensor".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn resolve_launch_orientation(saved: &str, win_w: i32, win_h: i32) -> String {
    let clean = saved.trim();
    let saved_mode = if clean.is_empty() {
        "full_sensor"
    } else {
        clean
    };
    let is_landscape_family = saved_mode.eq_ignore_ascii_case("landscape")
        || saved_mode.eq_ignore_ascii_case("reverse_landscape")
        || saved_mode.eq_ignore_ascii_case("sensor_landscape");

    if win_w > 0 && win_h > 0 && win_w > win_h {
        if is_landscape_family {
            saved_mode.to_string()
        } else {
            "sensor_landscape".to_string()
        }
    } else if win_w > 0 && win_h > 0 && win_h > win_w {
        if saved_mode.eq_ignore_ascii_case("portrait") {
            "portrait".to_string()
        } else {
            saved_mode.to_string()
        }
    } else if saved_mode.eq_ignore_ascii_case("portrait") {
        "portrait".to_string()
    } else if is_landscape_family {
        saved_mode.to_string()
    } else {
        "sensor_landscape".to_string()
    }
}

pub fn write_atomic_bytes(
    dir: &Path,
    tmp_name: &str,
    final_name: &str,
    bytes: &[u8],
) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp_path = dir.join(tmp_name);
    let final_path = dir.join(final_name);
    {
        let mut out = File::create(&tmp_path)?;
        out.write_all(bytes)?;
        out.flush()?;
    }
    if let Err(err) = fs::rename(&tmp_path, &final_path) {
        #[cfg(not(unix))]
        if final_path.exists() {
            let _ = fs::remove_file(&final_path);
            if fs::rename(&tmp_path, &final_path).is_ok() {
                return Ok(());
            }
        }
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

pub fn write_focus_mode_atomic(files_dir: &Path, mode: &str) {
    let clean = mode.trim();
    if clean.is_empty() {
        return;
    }
    let bridge_dir = files_dir.join(".thk_bridge");
    let _ = write_atomic_bytes(
        &bridge_dir,
        "focus_mode.tmp",
        "focus_mode.txt",
        clean.as_bytes(),
    );
}

pub fn parse_input_wire_line(line: &str) -> Option<ParsedInputCmd> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "Q" {
        return Some(ParsedInputCmd::Quit);
    }
    let mut parts = trimmed.split_whitespace();
    match parts.next()? {
        "K" => {
            let keycode = parts.next()?.parse::<i32>().ok()?;
            let down = parts.next()? == "1";
            Some(ParsedInputCmd::Key { keycode, down })
        }
        "C" => {
            let codepoint = parts.next()?.parse::<u32>().ok()?;
            char::from_u32(codepoint)?;
            let mods = parts.next()?.parse::<i32>().ok()?;
            Some(ParsedInputCmd::Char { codepoint, mods })
        }
        "M" => {
            let button = parts.next()?.parse::<i32>().ok()?;
            let down = parts.next()? == "1";
            Some(ParsedInputCmd::MouseButton { button, down })
        }
        "P" => {
            let x = parts.next()?.parse::<f32>().ok()?;
            let y = parts.next()?.parse::<f32>().ok()?;
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            Some(ParsedInputCmd::CursorPos { x, y })
        }
        "S" => {
            let dx = parts.next()?.parse::<f64>().ok()?;
            let dy = parts.next()?.parse::<f64>().ok()?;
            if !dx.is_finite() || !dy.is_finite() {
                return None;
            }
            Some(ParsedInputCmd::Scroll { dx, dy })
        }
        "W" => {
            let width = parts.next()?.parse::<i32>().ok()?;
            let height = parts.next()?.parse::<i32>().ok()?;
            Some(ParsedInputCmd::WindowSize { width, height })
        }
        _ => None,
    }
}

pub fn compute_scaled_window_size(
    old_win_w: i32,
    old_phys_w: i32,
    new_phys_w: i32,
    new_phys_h: i32,
) -> (i32, i32) {
    let mut scale = if old_phys_w > 0 {
        (old_win_w as f64) / (old_phys_w as f64)
    } else {
        1.0
    };
    if !(scale > 0.1 && scale <= 2.0) {
        scale = 1.0;
    }
    let w = (((new_phys_w as f64) * scale).round() as i32) & !1;
    let h = (((new_phys_h as f64) * scale).round() as i32) & !1;
    (w.max(320), h.max(240))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(1);
        let id = SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("thk-ffi-proto-{label}-{}-{id}", std::process::id()))
    }

    #[test]
    fn permission_snapshot_serializes_exact_thirteen_fields() {
        let snap = AndroidPermissionSnapshot {
            ignore_battery_optimizations: true,
            can_draw_overlays: false,
            notifications_enabled: true,
            keep_alive_running: true,
            overlay_showing: false,
            surface_ready: true,
            surface_width: 2772,
            surface_height: 1280,
            game_process_running: true,
            safe_inset_left_px: 152,
            safe_inset_top_px: 24,
            safe_inset_right_px: 53,
            safe_inset_bottom_px: 36,
        };
        let val = serde_json::to_value(&snap).unwrap();
        let obj = val.as_object().unwrap();
        assert_eq!(obj.len(), 13);
        assert_eq!(obj["surface_width"], 2772);
        assert_eq!(obj["safe_inset_left_px"], 152);
        let roundtrip: AndroidPermissionSnapshot = serde_json::from_value(val).unwrap();
        assert_eq!(roundtrip, snap);
    }

    #[test]
    fn network_snapshot_filters_non_lan_v4_and_non_global_v6() {
        let java_v4 = vec![
            "wlan0=192.168.1.42".to_string(),
            "lo=127.0.0.1".to_string(),
            "wlan0=169.254.10.20".to_string(),
            "bad=224.0.2.60".to_string(),
            "zero=0.0.0.0".to_string(),
        ];
        let java_v6 = vec![
            "2405:4802:1d40::88%wlan0".to_string(),
            "fe80::1%wlan0".to_string(),
            "2001:db8::42".to_string(),
            "2001:0000:4136::1".to_string(),
            "::1".to_string(),
        ];
        let native_v4 = vec![
            ("wlan0".to_string(), "192.168.1.42".to_string()),
            ("tun0".to_string(), "100.84.12.9".to_string()),
        ];
        let native_v6 = vec![
            "2405:4802:1d40::88".to_string(),
            "2606:4700:4700::1111".to_string(),
        ];

        let snap = build_network_snapshot(&java_v4, &java_v6, &native_v4, &native_v6);
        assert_eq!(
            snap.interfaces,
            vec![
                NetworkIfaceItem {
                    name: "wlan0".into(),
                    ipv4: "192.168.1.42".into(),
                },
                NetworkIfaceItem {
                    name: "tun0".into(),
                    ipv4: "100.84.12.9".into(),
                },
            ]
        );
        assert_eq!(
            snap.ipv6,
            vec![
                "2405:4802:1d40::88".to_string(),
                "2606:4700:4700::1111".to_string(),
            ]
        );
    }

    #[test]
    fn pick_mime_and_filename_sanitization_match_saf_contract() {
        assert_eq!(resolve_pick_mime_type(&["png".into()]), "image/png");
        assert_eq!(resolve_pick_mime_type(&["PNG".into()]), "image/png");
        assert_eq!(resolve_pick_mime_type(&["png".into(), "zip".into()]), "*/*");
        assert_eq!(resolve_pick_mime_type(&["mrpack".into()]), "*/*");
        assert_eq!(resolve_pick_mime_type(&[]), "*/*");

        assert_eq!(sanitize_picked_filename(".."), "picked_file.bin");
        assert_eq!(sanitize_picked_filename("   "), "picked_file.bin");
        assert_eq!(
            sanitize_picked_filename("my:pack/v1\\test*?.mrpack"),
            "my_pack_v1_test__.mrpack"
        );
    }

    #[test]
    fn orientation_and_focus_mode_roundtrip_match_main_activity_rules() {
        let dir = unique_temp_dir("orient");
        assert_eq!(read_saved_orientation(&dir), "full_sensor");

        let bridge = dir.join(".thk_bridge");
        fs::create_dir_all(&bridge).unwrap();
        fs::write(bridge.join("saved_orientation.txt"), "reverse_landscape\n").unwrap();
        assert_eq!(read_saved_orientation(&dir), "reverse_landscape");

        assert_eq!(
            resolve_launch_orientation("full_sensor", 1920, 1080),
            "sensor_landscape"
        );
        assert_eq!(
            resolve_launch_orientation("reverse_landscape", 1920, 1080),
            "reverse_landscape"
        );
        assert_eq!(
            resolve_launch_orientation("portrait", 1080, 1920),
            "portrait"
        );
        assert_eq!(
            resolve_launch_orientation("full_sensor", 1080, 1920),
            "full_sensor"
        );
        assert_eq!(resolve_launch_orientation("portrait", 0, 0), "portrait");
        assert_eq!(resolve_launch_orientation("auto", 0, 0), "sensor_landscape");

        write_focus_mode_atomic(&dir, "hub");
        assert_eq!(
            fs::read_to_string(bridge.join("focus_mode.txt")).unwrap(),
            "hub"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_spec_deserializes_defaults_and_full_payload() {
        let raw = r#"{
            "input_port": 49152,
            "jre_home": "/data/user/0/com.trihienkun.laucher/files/.TriHienKunLaucher/runtimes/openjdk-17",
            "runtime_root": "/data/user/0/com.trihienkun.laucher/files/.TriHienKunLaucher/runtimes/mobile_runtime",
            "lwjgl_natives_dir": "/runtime/lwjgl-3.3.3-natives/arm64-v8a",
            "instance_dir": "/instances/forge",
            "log_file": "/bridge/mobile_game.log",
            "renderer_lib": "libmobileglues.so",
            "egl_lib": "libmobileglues.so",
            "env": {"JAVA_HOME": "/jre"},
            "argv": ["java", "-version"]
        }"#;
        let parsed: MobileLaunchSpecJson = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.input_port, 49152);
        assert_eq!(parsed.window_width, 1280);
        assert_eq!(parsed.window_height, 720);
        assert_eq!(parsed.physical_width, None);
        assert_eq!(parsed.egl_lib.as_deref(), Some("libmobileglues.so"));
        assert_eq!(parsed.argv, vec!["java", "-version"]);
    }

    #[test]
    fn input_wire_parser_modifiers_and_scaled_window_size_are_exact() {
        assert_eq!(parse_input_wire_line("Q\n"), Some(ParsedInputCmd::Quit));
        assert_eq!(
            parse_input_wire_line("K 87 1\n"),
            Some(ParsedInputCmd::Key {
                keycode: 87,
                down: true
            })
        );
        assert_eq!(
            parse_input_wire_line("C 7879 1\n"),
            Some(ParsedInputCmd::Char {
                codepoint: 7879,
                mods: 1
            })
        );
        assert_eq!(
            parse_input_wire_line("M 0 0\n"),
            Some(ParsedInputCmd::MouseButton {
                button: 0,
                down: false
            })
        );
        assert_eq!(
            parse_input_wire_line("P 640.50 360.25\n"),
            Some(ParsedInputCmd::CursorPos {
                x: 640.5,
                y: 360.25
            })
        );
        assert_eq!(
            parse_input_wire_line("S 0.000 -1.500\n"),
            Some(ParsedInputCmd::Scroll { dx: 0.0, dy: -1.5 })
        );
        assert_eq!(
            parse_input_wire_line("W 1920 1080\n"),
            Some(ParsedInputCmd::WindowSize {
                width: 1920,
                height: 1080
            })
        );
        assert_eq!(parse_input_wire_line("   "), None);
        assert_eq!(parse_input_wire_line("C 55296 0"), None);

        let mut mods = GlfwModifierState::new();
        assert_eq!(mods.update_key(340, true), GLFW_MOD_SHIFT);
        assert_eq!(
            mods.update_key(345, true),
            GLFW_MOD_SHIFT | GLFW_MOD_CONTROL
        );
        assert_eq!(
            mods.update_key(342, true),
            GLFW_MOD_SHIFT | GLFW_MOD_CONTROL | GLFW_MOD_ALT
        );
        assert_eq!(
            mods.update_key(280, true),
            GLFW_MOD_SHIFT | GLFW_MOD_CONTROL | GLFW_MOD_ALT | GLFW_MOD_CAPS_LOCK
        );
        assert_eq!(
            mods.update_key(282, true),
            GLFW_MOD_SHIFT
                | GLFW_MOD_CONTROL
                | GLFW_MOD_ALT
                | GLFW_MOD_CAPS_LOCK
                | GLFW_MOD_NUM_LOCK
        );
        assert_eq!(
            mods.update_key(344, false),
            GLFW_MOD_CONTROL | GLFW_MOD_ALT | GLFW_MOD_CAPS_LOCK | GLFW_MOD_NUM_LOCK
        );

        let (w, h) = compute_scaled_window_size(1386, 2772, 2400, 1080);
        assert_eq!((w, h), (1200, 540));
        let (min_w, min_h) = compute_scaled_window_size(320, 2000, 400, 300);
        assert_eq!((min_w, min_h), (320, 240));
        let (even_w, even_h) = compute_scaled_window_size(1500, 2000, 1921, 1081);
        assert_eq!(even_w % 2, 0);
        assert_eq!(even_h % 2, 0);
    }
}
