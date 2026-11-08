use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thk_core::models::DownloadProgress;
use thk_core::{DownloadManager, DownloadTask, SafePath, ThkError, ThkResult};

#[derive(Debug, Clone)]
pub struct VerifiedUpdate {
    pub path: PathBuf,
    sha256: String,
}

#[cfg(test)]
impl VerifiedUpdate {
    pub(crate) fn for_test(path: PathBuf, sha256: impl Into<String>) -> Self {
        Self {
            path,
            sha256: sha256.into(),
        }
    }
}

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

#[derive(Debug, Clone)]
pub enum UpdateCheckOutcome {
    Available(UpdateReleaseInfo),
    UpToDate,
    NoPublishedReleases,
}

#[derive(Deserialize)]
struct GithubLatestRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
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
) -> ThkResult<UpdateCheckOutcome> {
    let repo = github_repo.trim().trim_matches('/');
    if repo.is_empty() || !repo.contains('/') {
        return Err(ThkError::InvalidInput(
            "Invalid GitHub repository slug (expected owner/repo)".into(),
        ));
    }

    let api_url = format!("https://api.github.com/repos/{repo}/releases?per_page=20");
    let releases: Vec<GithubLatestRelease> = downloader.get_json(&api_url).await?;
    parse_releases_list(releases, current_version)
}

fn parse_releases_list(
    releases: Vec<GithubLatestRelease>,
    current_version: &str,
) -> ThkResult<UpdateCheckOutcome> {
    semver::Version::parse(current_version).map_err(|error| {
        ThkError::InvalidInput(format!(
            "Invalid update version '{current_version}': {error}"
        ))
    })?;
    let Some(latest) = releases
        .into_iter()
        .find(|rel| !rel.draft && !rel.prerelease)
    else {
        return Ok(UpdateCheckOutcome::NoPublishedReleases);
    };
    match parse_latest_release(latest, current_version)? {
        Some(info) => Ok(UpdateCheckOutcome::Available(info)),
        None => Ok(UpdateCheckOutcome::UpToDate),
    }
}

fn parse_latest_release(
    rel: GithubLatestRelease,
    current_version: &str,
) -> ThkResult<Option<UpdateReleaseInfo>> {
    let clean_remote = rel.tag_name.trim().trim_start_matches(['v', 'V']);
    for version in [clean_remote, current_version] {
        semver::Version::parse(version).map_err(|error| {
            ThkError::InvalidInput(format!("Invalid update version '{version}': {error}"))
        })?;
    }
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

    let target_asset = select_platform_asset(&rel.assets).ok_or_else(|| {
        ThkError::InvalidInput(format!(
            "Release {} has no update artifact for {} / {}",
            rel.tag_name,
            std::env::consts::OS,
            std::env::consts::ARCH,
        ))
    })?;

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
) -> ThkResult<VerifiedUpdate>
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
    task.expected_sha256 = Some(expected_sha256.clone());
    if release.asset_size > 0 {
        task.expected_size = Some(release.asset_size);
    }

    downloader
        .download_batch("app-update", vec![task], 1, None, on_progress)
        .await?;
    Ok(VerifiedUpdate {
        path: dest_path,
        sha256: expected_sha256,
    })
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
    match (
        semver::Version::parse(remote),
        semver::Version::parse(local),
    ) {
        (Ok(remote), Ok(local)) => remote.cmp_precedence(&local).is_gt(),
        _ => false,
    }
}

fn expected_asset_name_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("trihienkun-laucher-windows-x64.exe"),
        ("linux", "x86_64") => Some("trihienkun-laucher-linux-x64"),
        ("linux", "aarch64") => Some("trihienkun-laucher-linux-arm64"),
        ("macos", "x86_64") => Some("trihienkun-laucher-macos-x64"),
        ("macos", "aarch64") => Some("trihienkun-laucher-macos-arm64"),
        ("android", "aarch64") => Some("trihienkun-laucher-android-arm64.apk"),
        _ => None,
    }
}

fn select_platform_asset(assets: &[GithubReleaseAsset]) -> Option<&GithubReleaseAsset> {
    let expected = expected_asset_name_for(std::env::consts::OS, std::env::consts::ARCH)?;
    assets.iter().find(|asset| asset.name == expected)
}

pub fn apply_downloaded_update(update: &VerifiedUpdate) -> ThkResult<bool> {
    let update_file = &update.path;
    if !update_file.is_file() {
        return Err(ThkError::InvalidInput(
            "Verified update file is missing on disk.".into(),
        ));
    }
    if !thk_core::hash::sha256_file(update_file)?.eq_ignore_ascii_case(&update.sha256) {
        return Err(ThkError::InvalidInput(
            "Update SHA-256 changed after download; download it again.".into(),
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
            if current_exe.as_path() != update_file.as_path() {
                let script_dir = update_file.parent().unwrap_or_else(|| Path::new("."));
                let (script_path, config_path) =
                    prepare_windows_update(update, &current_exe, script_dir, std::process::id())?;
                let error_log = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(config_path.with_extension("stderr.log"))?;

                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                std::process::Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-WindowStyle",
                        "Hidden",
                        "-File",
                    ])
                    .arg(&script_path)
                    .arg("-ConfigPath")
                    .arg(&config_path)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(error_log)
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()
                    .map_err(|e| {
                        ThkError::Process(format!("Failed to launch update helper: {e}"))
                    })?;
                return Ok(true);
            }
        }
    }

    #[cfg(all(unix, not(target_os = "android")))]
    {
        use std::os::unix::fs::PermissionsExt;

        let file_name_low = update_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let is_portable_bin = !file_name_low.ends_with(".dmg")
            && !file_name_low.ends_with(".pkg")
            && !file_name_low.ends_with(".deb")
            && !file_name_low.ends_with(".rpm")
            && !file_name_low.ends_with(".tar.gz");

        if is_portable_bin {
            std::fs::set_permissions(update_file, std::fs::Permissions::from_mode(0o755))?;
            if let Ok(current_exe) = std::env::current_exe().and_then(|p| p.canonicalize()) {
                let source_canon = update_file
                    .canonicalize()
                    .unwrap_or_else(|_| update_file.to_path_buf());
                if current_exe != source_canon {
                    if let Some(parent) = current_exe.parent() {
                        let id = uuid::Uuid::new_v4();
                        let name = current_exe
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        let staged = parent.join(format!(".{name}.{id}.staged"));
                        if std::fs::copy(&source_canon, &staged).is_ok() {
                            let _ = std::fs::set_permissions(
                                &staged,
                                std::fs::Permissions::from_mode(0o755),
                            );
                            let staged_ok = thk_core::hash::sha256_file(&staged)
                                .map(|h| h.eq_ignore_ascii_case(&update.sha256))
                                .unwrap_or(false);
                            if staged_ok && std::fs::rename(&staged, &current_exe).is_ok() {
                                let _ = std::fs::remove_file(update_file);
                                std::process::Command::new(&current_exe)
                                    .stdin(std::process::Stdio::null())
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .spawn()
                                    .map_err(|e| {
                                        ThkError::Process(format!(
                                            "Updated binary installed, but relaunch failed: {e}"
                                        ))
                                    })?;
                                return Ok(true);
                            }
                            let _ = std::fs::remove_file(&staged);
                        }
                    }
                }
            }

            std::process::Command::new(update_file)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| ThkError::Process(format!("Failed to launch updated binary: {e}")))?;
            return Ok(true);
        }
    }

    #[cfg(not(target_os = "android"))]
    {
        open::that(update_file)
            .map_err(|e| ThkError::Process(format!("Failed to launch update package: {e}")))?;
        Ok(false)
    }
    #[cfg(target_os = "android")]
    {
        Err(ThkError::Process(format!(
            "Gói APK đã xác thực SHA-256 tại: {}",
            update_file.display()
        )))
    }
}

#[cfg(windows)]
#[derive(Serialize, Deserialize)]
struct WindowsUpdateConfig {
    parent_id: u32,
    source: PathBuf,
    destination: PathBuf,
    staged: PathBuf,
    backup: PathBuf,
    failed: PathBuf,
    lock: PathBuf,
    sha256: String,
}

#[cfg(windows)]
fn prepare_windows_update(
    update: &VerifiedUpdate,
    destination: &Path,
    script_dir: &Path,
    parent_id: u32,
) -> ThkResult<(PathBuf, PathBuf)> {
    let destination = destination.canonicalize()?;
    let source = update.path.canonicalize()?;
    if source == destination {
        return Err(ThkError::InvalidInput(
            "Update source is the running executable.".into(),
        ));
    }
    let id = uuid::Uuid::new_v4();
    let parent = destination
        .parent()
        .ok_or_else(|| ThkError::InvalidInput("Executable directory is missing.".into()))?;
    let name = destination
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let config = WindowsUpdateConfig {
        parent_id,
        source,
        staged: parent.join(format!(".{name}.{id}.staged")),
        backup: parent.join(format!(".{name}.{id}.backup")),
        failed: parent.join(format!(".{name}.{id}.failed")),
        lock: parent.join(format!(".{name}.update-lock")),
        destination,
        sha256: update.sha256.clone(),
    };
    let job_dir = script_dir.join(format!("apply-{id}"));
    std::fs::create_dir(&job_dir)?;
    let script_path = job_dir.join("apply_update.ps1");
    let config_path = job_dir.join("config.json");
    std::fs::write(&script_path, include_str!("apply_update.ps1"))?;
    std::fs::write(&config_path, serde_json::to_vec(&config)?)?;
    Ok((script_path, config_path))
}

#[cfg(test)]
#[path = "updater_tests.rs"]
mod helper_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_release_metadata_is_not_reported_as_latest_version() {
        // Synthetic metadata tests classification, not GitHub availability.
        let release = |tag: &str| GithubLatestRelease {
            tag_name: tag.into(),
            html_url: String::new(),
            draft: false,
            prerelease: false,
            body: None,
            assets: Vec::new(),
        };
        assert!(parse_latest_release(release("v2.invalid.0"), "1.0.0")
            .unwrap_err()
            .to_string()
            .contains("Invalid update version"));
        assert!(parse_latest_release(release("v2.0.0"), "invalid")
            .unwrap_err()
            .to_string()
            .contains("Invalid update version"));
        assert!(parse_latest_release(release("v2.0.0"), "1.0.0")
            .unwrap_err()
            .to_string()
            .contains("no update artifact"));
        assert!(parse_latest_release(release("v1.0.0"), "1.0.0")
            .unwrap()
            .is_none());
        assert!(parse_latest_release(release("v0.9.0"), "1.0.0")
            .unwrap()
            .is_none());
    }

    #[test]
    fn empty_or_prerelease_only_repo_reports_no_published_releases() {
        assert!(matches!(
            parse_releases_list(Vec::new(), "1.0.0").unwrap(),
            UpdateCheckOutcome::NoPublishedReleases
        ));

        let unpublished = vec![
            GithubLatestRelease {
                tag_name: "v2.0.0-rc.1".into(),
                html_url: String::new(),
                draft: false,
                prerelease: true,
                body: None,
                assets: Vec::new(),
            },
            GithubLatestRelease {
                tag_name: "v2.0.0".into(),
                html_url: String::new(),
                draft: true,
                prerelease: false,
                body: None,
                assets: Vec::new(),
            },
        ];
        assert!(matches!(
            parse_releases_list(unpublished, "1.0.0").unwrap(),
            UpdateCheckOutcome::NoPublishedReleases
        ));

        let mut with_stable = vec![GithubLatestRelease {
            tag_name: "v2.0.0-beta.1".into(),
            html_url: String::new(),
            draft: false,
            prerelease: true,
            body: None,
            assets: Vec::new(),
        }];
        with_stable.push(GithubLatestRelease {
            tag_name: "v1.0.0".into(),
            html_url: String::new(),
            draft: false,
            prerelease: false,
            body: None,
            assets: Vec::new(),
        });
        assert!(matches!(
            parse_releases_list(with_stable, "1.0.0").unwrap(),
            UpdateCheckOutcome::UpToDate
        ));
    }

    #[test]
    fn version_precedence_is_strict_semver() {
        assert!(is_newer_semver("1.0.0", "1.0.0-rc.1"));
        assert!(is_newer_semver("1.0.0-rc.10", "1.0.0-rc.2"));
        assert!(!is_newer_semver("1.0.0+build.2", "1.0.0+build.1"));
        assert!(!is_newer_semver("2.invalid.0", "1.0.0"));
        assert!(!is_newer_semver("2.0.0", "invalid"));
    }

    #[test]
    fn update_asset_never_falls_back_to_another_architecture() {
        let assets = [
            "other-windows-arm64.exe",
            "other-macos-aarch64",
            "other-linux-x64",
            "setup.exe",
        ]
        .map(|name| GithubReleaseAsset {
            name: name.into(),
            browser_download_url: String::new(),
            size: 0,
            digest: None,
        });
        assert!(select_platform_asset(&assets).is_none());
        assert_eq!(
            expected_asset_name_for("linux", "aarch64"),
            Some("trihienkun-laucher-linux-arm64")
        );
        assert_eq!(expected_asset_name_for("windows", "aarch64"), None);
    }
}
