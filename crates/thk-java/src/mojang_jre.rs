use crate::scanner::{find_java_executable_in_dir, probe_java_binary, JavaInstallation};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use thk_core::models::DownloadProgress;
use thk_core::paths::SafePath;
use thk_core::{DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;

pub const MOJANG_JRE_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Debug, Clone)]
pub struct RequiredJavaSpec {
    pub component: String,
    pub major_version: u32,
}

impl RequiredJavaSpec {
    pub fn for_minecraft_version(
        mc_version: &str,
        manifest_component: Option<&str>,
        manifest_major: Option<u32>,
    ) -> Self {
        if let (Some(comp), Some(maj)) = (manifest_component, manifest_major) {
            if !comp.trim().is_empty() && maj > 0 {
                return Self {
                    component: comp.to_string(),
                    major_version: maj,
                };
            }
        }

        let major_ver = infer_java_major_from_mc_version(mc_version);
        let component = match major_ver {
            0..=8 => "jre-legacy",
            9..=17 => "java-runtime-gamma",
            _ => "java-runtime-delta",
        }
        .to_string();

        Self {
            component,
            major_version: major_ver,
        }
    }
}

pub fn infer_java_major_from_mc_version(mc_version: &str) -> u32 {
    let clean = mc_version.split('-').next().unwrap_or(mc_version).trim();
    let parts: Vec<u32> = clean
        .split('.')
        .filter_map(|p| p.parse::<u32>().ok())
        .collect();

    match parts.as_slice() {
        // New Mojang calendar-style versions (e.g. 26.1+) require Java 21+
        [first, ..] if *first >= 25 => 21,
        [1, minor, patch, ..] => {
            if *minor > 20 || (*minor == 20 && *patch >= 5) {
                21
            } else if *minor >= 18 {
                17
            } else if *minor == 17 {
                16
            } else {
                8
            }
        }
        [1, minor] => {
            if *minor > 20 {
                21
            } else if *minor >= 18 {
                17
            } else if *minor == 17 {
                16
            } else {
                8
            }
        }
        _ => 21,
    }
}

#[derive(Deserialize)]
struct MojangRuntimeComponentEntry {
    manifest: MojangManifestRef,
}

#[derive(Deserialize)]
struct MojangManifestRef {
    url: String,
}

#[derive(Deserialize)]
struct MojangFilesManifest {
    files: HashMap<String, MojangRuntimeFile>,
}

#[derive(Deserialize)]
struct MojangRuntimeFile {
    #[serde(rename = "type")]
    entry_type: String,
    #[serde(default)]
    executable: bool,
    downloads: Option<MojangFileDownloads>,
}

#[derive(Deserialize)]
struct MojangFileDownloads {
    raw: Option<MojangRawDownload>,
}

#[derive(Deserialize)]
struct MojangRawDownload {
    url: String,
    sha1: String,
    size: u64,
}

pub async fn ensure_mojang_java<F>(
    downloader: &DownloadManager,
    runtimes_dir: &Path,
    spec: &RequiredJavaSpec,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<JavaInstallation>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let target_dir = SafePath::resolve_child(runtimes_dir, &spec.component)?;
    if let Some(existing_bin) = find_java_executable_in_dir(&target_dir, 4) {
        if let Ok(info) = probe_java_binary(&existing_bin, true) {
            if info.major_version >= spec.major_version {
                return Ok(info);
            }
        }
    }

    let all_platforms: HashMap<String, HashMap<String, Vec<MojangRuntimeComponentEntry>>> =
        downloader.get_json(MOJANG_JRE_MANIFEST_URL).await?;

    let platform_key = mojang_platform_key();
    let comp_entries = all_platforms
        .get(platform_key)
        .and_then(|m| m.get(&spec.component))
        .filter(|v| !v.is_empty())
        .or_else(|| {
            // Fallback on arm64 to x64 Rosetta/emulation if Mojang lacks native arm64 legacy JRE
            mojang_fallback_platform_key()
                .and_then(|fb| all_platforms.get(fb))
                .and_then(|m| m.get(&spec.component))
                .filter(|v| !v.is_empty())
        })
        .ok_or_else(|| {
            ThkError::Java(format!(
                "Mojang runtime '{}' is unavailable on platform '{}'",
                spec.component, platform_key
            ))
        })?;

    let files_manifest: MojangFilesManifest =
        downloader.get_json(&comp_entries[0].manifest.url).await?;

    tokio::fs::create_dir_all(&target_dir).await?;
    let mut tasks = Vec::new();
    let mut exec_paths = Vec::new();

    for (rel_path, entry) in files_manifest.files {
        let dest = SafePath::resolve_child(&target_dir, &rel_path)?;
        match entry.entry_type.as_str() {
            "directory" => {
                tokio::fs::create_dir_all(&dest).await?;
            }
            "file" => {
                if let Some(raw) = entry.downloads.and_then(|d| d.raw) {
                    let mut task = DownloadTask::new(raw.url, dest.clone());
                    task.expected_sha1 = Some(raw.sha1);
                    task.expected_size = Some(raw.size);
                    task.label = format!("JRE {}", spec.component);
                    tasks.push(task);
                    if entry.executable {
                        exec_paths.push(dest);
                    }
                }
            }
            _ => {}
        }
    }

    downloader
        .download_batch(
            &format!("Installing Mojang Java {}", spec.major_version),
            tasks,
            12,
            cancel,
            on_progress,
        )
        .await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for exe in exec_paths {
            if let Ok(meta) = std::fs::metadata(&exe) {
                let mut perms = meta.permissions();
                perms.set_mode(perms.mode() | 0o755);
                let _ = std::fs::set_permissions(&exe, perms);
            }
        }
    }
    #[cfg(not(unix))]
    let _ = exec_paths;

    let java_bin = find_java_executable_in_dir(&target_dir, 4).ok_or_else(|| {
        ThkError::Java(format!(
            "Java binary not found after downloading Mojang component '{}'",
            spec.component
        ))
    })?;

    probe_java_binary(&java_bin, true)
}

fn mojang_platform_key() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "windows-arm64"
    }
    #[cfg(all(target_os = "windows", not(target_arch = "aarch64")))]
    {
        "windows-x64"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "mac-os-arm64"
    }
    #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
    {
        "mac-os"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86"))]
    {
        "linux-i386"
    }
    #[cfg(all(target_os = "linux", not(target_arch = "x86")))]
    {
        "linux"
    }
}

fn mojang_fallback_platform_key() -> Option<&'static str> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Some("mac-os")
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        Some("windows-x64")
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "aarch64")
    )))]
    {
        None
    }
}
