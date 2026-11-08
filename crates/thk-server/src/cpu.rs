use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use sysinfo::{CpuRefreshKind, RefreshKind, System};

static HOST_CPU_INFO: OnceLock<CpuHardwareInfo> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuHardwareInfo {
    pub model_name: String,
    pub short_model: String,
    pub logical_cores: u32,
    pub physical_cores: Option<u32>,
}

impl CpuHardwareInfo {
    pub fn host() -> &'static Self {
        HOST_CPU_INFO.get_or_init(Self::detect_current)
    }

    fn detect_current() -> Self {
        let refreshes = RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing());
        let sys_opt = System::new_with_specifics(refreshes).ok();

        let brands: Vec<String> = sys_opt
            .as_ref()
            .map(|sys| sys.cpus().iter().map(|c| c.brand().to_string()).collect())
            .unwrap_or_default();
        let brand_refs: Vec<&str> = brands.iter().map(String::as_str).collect();

        let soc_hint = detect_platform_soc_hint();
        let (short_model, model_name) = summarize_cpu_brands(&brand_refs, soc_hint.as_deref());

        let sys_cpus = sys_opt
            .as_ref()
            .map(|sys| sys.cpus().len() as u32)
            .unwrap_or(0);
        let std_cpus = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1);
        let present_cpus = read_linux_present_cores().unwrap_or(0);
        let logical_cores = sys_cpus.max(std_cpus).max(present_cpus).max(1);

        let physical_cores = System::physical_core_count()
            .ok()
            .map(|n| n as u32)
            .filter(|&n| n > 0 && n <= logical_cores);

        Self {
            model_name,
            short_model,
            logical_cores,
            physical_cores,
        }
    }
}

pub fn compute_process_cpu_sample(
    delta_cpu_ms: u64,
    elapsed_ms: f64,
    logical_cores: u32,
) -> (f32, f32) {
    if elapsed_ms <= 0.0 || !elapsed_ms.is_finite() || logical_cores == 0 {
        return (0.0, 0.0);
    }

    let max_cores = f64::from(logical_cores);
    let raw_used_cores = (delta_cpu_ms as f64 / elapsed_ms).clamp(0.0, max_cores);
    if !raw_used_cores.is_finite() {
        return (0.0, 0.0);
    }

    let raw_percent = (raw_used_cores / max_cores * 100.0).clamp(0.0, 100.0);
    let cpu_percent = ((raw_percent * 10.0).round() / 10.0) as f32;
    let used_cores = ((raw_used_cores * 100.0).round() / 100.0) as f32;
    (cpu_percent, used_cores)
}

pub fn parse_cpu_list_count(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut total: u32 = 0;
    for part in trimmed.split(',') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        if let Some((start_s, end_s)) = token.split_once('-') {
            let start: u32 = start_s.trim().parse().ok()?;
            let end: u32 = end_s.trim().parse().ok()?;
            if end < start {
                return None;
            }
            total = total.checked_add(end - start + 1)?;
        } else {
            let _: u32 = token.parse().ok()?;
            total = total.checked_add(1)?;
        }
    }

    (total > 0).then_some(total)
}

pub fn parse_proc_status_allowed_cores(status_text: &str) -> Option<u32> {
    for line in status_text.lines() {
        if let Some(rest) = line.strip_prefix("Cpus_allowed_list:") {
            return parse_cpu_list_count(rest);
        }
    }
    None
}

pub fn read_process_allowed_cores(pid: u32, fallback_logical: u32) -> u32 {
    let fallback = fallback_logical.max(1);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let status_path = format!("/proc/{pid}/status");
        if let Ok(content) = std::fs::read_to_string(status_path) {
            if let Some(allowed) = parse_proc_status_allowed_cores(&content) {
                return allowed.clamp(1, fallback);
            }
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let _ = pid;

    fallback
}

pub fn summarize_cpu_brands(brands: &[&str], soc_hint: Option<&str>) -> (String, String) {
    let normalized_soc = soc_hint.map(normalize_soc_label).filter(|s| !s.is_empty());

    let mut clusters: Vec<(String, usize)> = Vec::new();
    for &raw in brands {
        let cleaned = collapse_whitespace(raw);
        if cleaned.is_empty() {
            continue;
        }
        if let Some(existing) = clusters.iter_mut().find(|(name, _)| name == &cleaned) {
            existing.1 += 1;
        } else {
            clusters.push((cleaned, 1));
        }
    }

    if clusters.is_empty() {
        let fallback = normalized_soc.unwrap_or_else(|| std::env::consts::ARCH.to_uppercase());
        return (fallback.clone(), fallback);
    }

    if clusters.len() == 1 {
        let full_brand = clusters.remove(0).0;
        let short_brand = trim_desktop_cpu_suffix(&full_brand);
        if let Some(soc) = normalized_soc {
            if !full_brand
                .to_ascii_lowercase()
                .contains(&soc.to_ascii_lowercase())
            {
                let count = brands.len().max(1);
                let full = format!("{soc} ({count}×{full_brand})");
                return (soc, full);
            }
        }
        return (short_brand, full_brand);
    }

    let cluster_summary = clusters
        .iter()
        .rev()
        .map(|(name, count)| format!("{count}×{name}"))
        .collect::<Vec<_>>()
        .join(" + ");

    if let Some(soc) = normalized_soc {
        let full = format!("{soc} ({cluster_summary})");
        (soc, full)
    } else {
        (cluster_summary.clone(), cluster_summary)
    }
}

fn collapse_whitespace(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trim_desktop_cpu_suffix(brand: &str) -> String {
    let mut base = brand;
    for marker in [" w/ ", " with ", " @ "] {
        if let Some((head, _)) = base.split_once(marker) {
            let trimmed = head.trim();
            if !trimmed.is_empty() {
                base = trimmed;
            }
        }
    }
    let compact = base
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace("(tm)", "");
    let cleaned = collapse_whitespace(&compact);
    if cleaned.is_empty() {
        brand.to_string()
    } else {
        cleaned
    }
}

fn normalize_soc_label(raw: &str) -> String {
    let cleaned = collapse_whitespace(raw);
    if cleaned.is_empty() || cleaned.eq_ignore_ascii_case("unknown") {
        return String::new();
    }

    let mut tokens: Vec<String> = cleaned
        .split(' ')
        .map(|tok| match tok.to_ascii_lowercase().as_str() {
            "mediatek" => "MediaTek".to_string(),
            "qualcomm" | "qti" => "Qualcomm".to_string(),
            "samsung" => "Samsung".to_string(),
            "google" => "Google".to_string(),
            "hisilicon" => "HiSilicon".to_string(),
            "unisoc" | "spreadtrum" => "Unisoc".to_string(),
            _ => tok.to_string(),
        })
        .collect();

    if tokens.len() >= 2
        && tokens[1]
            .to_ascii_lowercase()
            .starts_with(&tokens[0].to_ascii_lowercase())
    {
        tokens.remove(0);
    }
    tokens.join(" ")
}

fn read_linux_present_cores() -> Option<u32> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        for path in [
            "/sys/devices/system/cpu/present",
            "/sys/devices/system/cpu/online",
        ] {
            if let Ok(raw) = std::fs::read_to_string(path) {
                if let Some(count) = parse_cpu_list_count(&raw) {
                    return Some(count);
                }
            }
        }
    }
    None
}

fn detect_platform_soc_hint() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        if let Ok(env_soc) = std::env::var("THK_ANDROID_SOC") {
            let normalized = normalize_soc_label(&env_soc);
            if !normalized.is_empty() {
                return Some(normalized);
            }
        }

        let manufacturer = read_android_prop("ro.soc.manufacturer").unwrap_or_default();
        let model = read_android_prop("ro.soc.model")
            .or_else(|| read_android_prop("ro.hardware.chipname"))
            .or_else(|| read_android_prop("ro.board.platform"))
            .unwrap_or_default();
        let combined = normalize_soc_label(&format!("{manufacturer} {model}"));
        if !combined.is_empty() {
            return Some(combined);
        }
    }
    None
}

#[cfg(target_os = "android")]
fn read_android_prop(prop: &str) -> Option<String> {
    let output = std::process::Command::new("/system/bin/getprop")
        .arg(prop)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if val.is_empty() || val.eq_ignore_ascii_case("unknown") {
        None
    } else {
        Some(val)
    }
}
