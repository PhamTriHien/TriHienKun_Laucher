use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thk_core::models::DownloadProgress;
use thk_core::{DownloadManager, DownloadTask, SafePath, ThkError, ThkResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateReleaseInfo {
    pub version: String,
    pub tag_name: String,
    pub html_url: String,
    pub release_notes: String,
    pub asset_name: String,
    pub asset_url: String,
    pub asset_size: u64,
    pub sha256: Option<String>,
    pub sha256sums_url: Option<String>,
}

#[derive(Deserialize)]
struct GithubLatestRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

pub async fn check_latest_release(
    downloader: &DownloadManager,
    github_repo: &str,
    current_version: &str,
) -> ThkResult<Option<UpdateReleaseInfo>> {
    let repo = github_repo.trim().trim_matches('/');
    if repo.is_empty() || !repo.contains('/') {
        return Err(ThkError::InvalidInput(
            "Invalid GitHub repository slug (expected owner/repo)".into(),
        ));
    }

    let api_url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let rel: GithubLatestRelease = downloader.get_json(&api_url).await?;
    let clean_remote = rel.tag_name.trim().trim_start_matches(['v', 'V']);
    if !is_newer_semver(clean_remote, current_version) {
        return Ok(None);
    }

    let sums_url = rel
        .assets
        .iter()
        .find(|a| {
            let low = a.name.to_ascii_lowercase();
            low == "sha256sums.txt" || low == "sha256sums" || low.ends_with(".sha256")
        })
        .map(|a| a.browser_download_url.clone());

    let Some(target_asset) = select_platform_asset(&rel.assets) else {
        return Ok(None);
    };

    let inline_sha256 = target_asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_string);

    Ok(Some(UpdateReleaseInfo {
        version: clean_remote.to_string(),
        tag_name: rel.tag_name,
        html_url: rel.html_url,
        release_notes: rel.body.unwrap_or_default(),
        asset_name: target_asset.name.clone(),
        asset_url: target_asset.browser_download_url.clone(),
        asset_size: target_asset.size,
        sha256: inline_sha256,
        sha256sums_url: sums_url,
    }))
}

pub async fn download_verified_update<F>(
    downloader: &DownloadManager,
    release: &UpdateReleaseInfo,
    updates_dir: &Path,
    on_progress: F,
) -> ThkResult<PathBuf>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let expected_sha256 = if let Some(ref h) = release.sha256 {
        h.clone()
    } else if let Some(ref sums_url) = release.sha256sums_url {
        let bytes = downloader.get_bytes(sums_url).await?;
        let text = String::from_utf8_lossy(&bytes);
        extract_sha256_for_asset(&text, &release.asset_name).ok_or_else(|| {
            ThkError::InvalidInput(format!(
                "Asset '{}' not listed in SHA256SUMS.txt",
                release.asset_name
            ))
        })?
    } else {
        return Err(ThkError::InvalidInput(
            "Refusing unverified update: release provides neither asset SHA-256 digest nor SHA256SUMS.txt"
                .into(),
        ));
    };

    tokio::fs::create_dir_all(updates_dir).await?;
    let clean_name = SafePath::validate_single_name(&release.asset_name)?;
    let dest_path = SafePath::resolve_child(updates_dir, clean_name)?;

    let mut task = DownloadTask::new(release.asset_url.clone(), dest_path.clone());
    task.expected_sha256 = Some(expected_sha256);
    if release.asset_size > 0 {
        task.expected_size = Some(release.asset_size);
    }

    downloader
        .download_batch("app-update", vec![task], 1, None, on_progress)
        .await?;
    Ok(dest_path)
}

pub fn extract_sha256_for_asset(sums_content: &str, asset_name: &str) -> Option<String> {
    for line in sums_content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let hash = parts.next()?;
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        if let Some(file_part) = parts.next() {
            let clean_file = file_part.trim_start_matches('*');
            if clean_file.eq_ignore_ascii_case(asset_name) {
                return Some(hash.to_ascii_lowercase());
            }
        }
    }
    None
}

pub fn is_newer_semver(remote: &str, local: &str) -> bool {
    let parse_tuple = |v: &str| -> (u32, u32, u32) {
        let core = v.split('-').next().unwrap_or(v);
        let mut nums = core
            .split('.')
            .map(|p| p.trim().parse::<u32>().unwrap_or(0));
        (
            nums.next().unwrap_or(0),
            nums.next().unwrap_or(0),
            nums.next().unwrap_or(0),
        )
    };
    parse_tuple(remote) > parse_tuple(local)
}

fn select_platform_asset(assets: &[GithubReleaseAsset]) -> Option<&GithubReleaseAsset> {
    let os_kw = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch_kw = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };

    assets
        .iter()
        .find(|a| {
            let low = a.name.to_ascii_lowercase();
            !low.ends_with(".sha256")
                && !low.contains("sha256sums")
                && low.contains(os_kw)
                && (low.contains(arch_kw) || low.contains("x86_64") || low.contains("aarch64"))
        })
        .or_else(|| {
            assets.iter().find(|a| {
                let low = a.name.to_ascii_lowercase();
                if cfg!(target_os = "windows") {
                    low.ends_with(".exe") || low.ends_with(".msi")
                } else if cfg!(target_os = "macos") {
                    low.ends_with(".dmg") || low.contains("darwin")
                } else {
                    low.ends_with(".appimage") || low.contains("linux")
                }
            })
        })
}

pub fn apply_downloaded_update(update_file: &Path) -> ThkResult<bool> {
    if !update_file.is_file() {
        return Err(ThkError::InvalidInput(
            "Verified update file is missing on disk.".into(),
        ));
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let file_name_low = update_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let is_portable_exe = file_name_low.ends_with(".exe")
            && !file_name_low.contains("setup")
            && !file_name_low.contains("installer");

        if is_portable_exe {
            let current_exe = std::env::current_exe()?;
            if current_exe != update_file {
                let script_dir = update_file.parent().unwrap_or_else(|| Path::new("."));
                let script_path = script_dir.join("apply_update.ps1");
                let pid = std::process::id();
                let src_lit = update_file.display().to_string().replace('\'', "''");
                let dst_lit = current_exe.display().to_string().replace('\'', "''");
                let ps_script = format!(
                    "$ErrorActionPreference = 'SilentlyContinue'\r\n\
                     Wait-Process -Id {pid} -Timeout 20\r\n\
                     Start-Sleep -Milliseconds 400\r\n\
                     $src = '{src_lit}'\r\n\
                     $dst = '{dst_lit}'\r\n\
                     $bak = \"$dst.bak\"\r\n\
                     if (Test-Path -LiteralPath $dst) {{\r\n\
                         Copy-Item -LiteralPath $dst -Destination $bak -Force\r\n\
                     }}\r\n\
                     Copy-Item -LiteralPath $src -Destination $dst -Force\r\n\
                     Start-Process -FilePath $dst\r\n"
                );
                std::fs::write(&script_path, ps_script)?;

                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                std::process::Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-WindowStyle",
                        "Hidden",
                        "-File",
                    ])
                    .arg(&script_path)
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()
                    .map_err(|e| {
                        ThkError::Process(format!("Failed to launch update helper: {e}"))
                    })?;
                return Ok(true);
            }
        }
    }

    open::that(update_file)
        .map_err(|e| ThkError::Process(format!("Failed to launch update package: {e}")))?;
    Ok(false)
}
