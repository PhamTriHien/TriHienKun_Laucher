use crate::mojang_jre::{ensure_mojang_java, infer_java_major_from_mc_version, RequiredJavaSpec};
use crate::scanner::{
    find_java_executable_in_dir, probe_java_binary, scan_java_installations, JavaInstallation,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thk_core::archive::{
    extract_targz_cancellable, extract_tarxz_cancellable, extract_zip_cancellable,
    flatten_single_subdir,
};
use thk_core::models::DownloadProgress;
use thk_core::paths::SafePath;
use thk_core::{DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const SUPPORTED_JAVA_MAJORS: [u32; 5] = [8, 11, 17, 21, 25];
pub const GRAALVM_RELEASES_API_URL: &str =
    "https://api.github.com/repos/graalvm/graalvm-ce-builds/releases";

pub fn optimal_graalvm_major_for_spec(required_major: u32) -> Option<u32> {
    match required_major {
        0..=8 => None,
        9..=17 => Some(17),
        18..=21 => Some(21),
        22..=25 => Some(25),
        _ => None,
    }
}

pub fn android_openjdk_major_for_spec(required_major: u32) -> Option<u32> {
    match required_major {
        0..=8 => Some(8),
        9..=17 => Some(17),
        18..=21 => Some(21),
        22..=25 => Some(25),
        _ => None,
    }
}

pub fn android_openjdk_packages() -> Vec<DistroReleasePackage> {
    vec![
        DistroReleasePackage {
            distro: JavaDistroKind::AndroidOpenJdk,
            major_version: 8,
            release_name: "OpenJDK 8".to_string(),
            download_url: "https://github.com/AngelAuraMC/angelauramc-openjdk-build/releases/download/download_jre8/jre8-android-arm64.tar.xz".to_string(),
            file_name: "jre8-android-arm64.tar.xz".to_string(),
            sha256: Some("9a59124d9791957d55c68be664ab76831f336cf2e1e1cd4414220c6fdbf0e06d".to_string()),
            size_bytes: Some(28_026_736),
        },
        DistroReleasePackage {
            distro: JavaDistroKind::AndroidOpenJdk,
            major_version: 17,
            release_name: "OpenJDK 17".to_string(),
            download_url: "https://github.com/AngelAuraMC/angelauramc-openjdk-build/releases/download/download_jre17/jre17-android-arm64.tar.xz".to_string(),
            file_name: "jre17-android-arm64.tar.xz".to_string(),
            sha256: Some("e162c860fe05ee4a4e4af7606437419879f6c748386a7b09fa77d10db6a64091".to_string()),
            size_bytes: Some(26_900_804),
        },
        DistroReleasePackage {
            distro: JavaDistroKind::AndroidOpenJdk,
            major_version: 21,
            release_name: "OpenJDK 21".to_string(),
            download_url: "https://github.com/AngelAuraMC/angelauramc-openjdk-build/releases/download/download_jre21/jre21-android-arm64.tar.xz".to_string(),
            file_name: "jre21-android-arm64.tar.xz".to_string(),
            sha256: Some("8d41ec401ee59f7722df60ed991f81ad146e130452804bfdd8a05d3436f7bbfe".to_string()),
            size_bytes: Some(28_675_516),
        },
        DistroReleasePackage {
            distro: JavaDistroKind::AndroidOpenJdk,
            major_version: 25,
            release_name: "OpenJDK 25".to_string(),
            download_url: "https://github.com/AngelAuraMC/angelauramc-openjdk-build/releases/download/download_jre25/jre25-android-arm64.tar.xz".to_string(),
            file_name: "jre25-android-arm64.tar.xz".to_string(),
            sha256: Some("d3eb7afe2240c26728a1bb440502c5f18ac3883e932d202dd7f0c9bcbbce4c37".to_string()),
            size_bytes: Some(38_031_580),
        },
    ]
}

pub fn android_openjdk_package_for_major(major: u32) -> Option<DistroReleasePackage> {
    android_openjdk_packages()
        .into_iter()
        .find(|p| p.major_version == major)
}

pub fn recommended_runtime_label_for_mc(mc_version: &str, prefer_optimal: bool) -> String {
    let req = infer_java_major_from_mc_version(mc_version);
    if cfg!(target_os = "android") {
        let major = android_openjdk_major_for_spec(req).unwrap_or(req);
        return format!("Java {major}");
    }
    if prefer_optimal {
        if let Some(graal_major) = optimal_graalvm_major_for_spec(req) {
            return format!("GraalVM {graal_major}");
        }
    }
    format!("Java {req}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JavaDistroKind {
    Adoptium,
    Azul,
    GraalVm,
    AndroidOpenJdk,
}

impl JavaDistroKind {
    pub const ALL: [JavaDistroKind; 3] = [Self::Adoptium, Self::Azul, Self::GraalVm];
    pub const ANDROID_ALL: [JavaDistroKind; 1] = [Self::AndroidOpenJdk];

    pub fn platform_distros() -> &'static [JavaDistroKind] {
        if cfg!(target_os = "android") {
            &Self::ANDROID_ALL
        } else {
            &Self::ALL
        }
    }

    pub fn default_for_platform() -> Self {
        if cfg!(target_os = "android") {
            Self::AndroidOpenJdk
        } else {
            Self::Adoptium
        }
    }

    pub fn slug(&self) -> &'static str {
        match self {
            Self::Adoptium => "adoptium",
            Self::Azul => "azul",
            Self::GraalVm => "graalvm",
            Self::AndroidOpenJdk => "openjdk",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Adoptium => "Eclipse Temurin (Adoptium)",
            Self::Azul => "Azul Zulu JRE",
            Self::GraalVm => "GraalVM Community JDK",
            Self::AndroidOpenJdk => "OpenJDK",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistroReleasePackage {
    pub distro: JavaDistroKind,
    pub major_version: u32,
    pub release_name: String,
    pub download_url: String,
    pub file_name: String,
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
}

pub async fn fetch_available_distro_packages(
    downloader: &DownloadManager,
    distro: JavaDistroKind,
) -> ThkResult<Vec<DistroReleasePackage>> {
    if cfg!(target_os = "android") || distro == JavaDistroKind::AndroidOpenJdk {
        if cfg!(target_os = "android") && !cfg!(target_arch = "aarch64") {
            return Ok(Vec::new());
        }
        return Ok(android_openjdk_packages());
    }
    match distro {
        JavaDistroKind::Adoptium => fetch_adoptium_packages(downloader).await,
        JavaDistroKind::Azul => fetch_azul_packages(downloader).await,
        JavaDistroKind::GraalVm => fetch_graalvm_packages(downloader).await,
        JavaDistroKind::AndroidOpenJdk => Ok(android_openjdk_packages()),
    }
}

struct TempInstallCleanupGuard {
    archive_path: PathBuf,
    staging_dir: Option<PathBuf>,
}

impl Drop for TempInstallCleanupGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.archive_path);
        if let Some(ref staging) = self.staging_dir {
            let _ = std::fs::remove_dir_all(staging);
        }
    }
}

pub async fn install_distro_package<F>(
    downloader: &DownloadManager,
    runtimes_dir: &Path,
    pkg: &DistroReleasePackage,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<JavaInstallation>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    if cfg!(target_os = "android") && pkg.distro != JavaDistroKind::AndroidOpenJdk {
        return Err(ThkError::Java(
            "Android (ARM64 Bionic) chỉ hỗ trợ gói OpenJDK biên dịch riêng cho Android.".into(),
        ));
    }
    if cfg!(target_os = "android") && !cfg!(target_arch = "aarch64") {
        return Err(ThkError::Java(
            "Gói OpenJDK Android hiện yêu cầu kiến trúc arm64-v8a (aarch64).".into(),
        ));
    }
    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }

    let install_folder = format!("{}-{}", pkg.distro.slug(), pkg.major_version);
    let dest_dir = SafePath::resolve_child(runtimes_dir, &install_folder)?;
    let tmp_dir = SafePath::resolve_child(runtimes_dir, ".jre_downloads")?;
    tokio::fs::create_dir_all(&tmp_dir).await?;

    let archive_name = SafePath::validate_single_name(&pkg.file_name)?;
    let temp_archive_name = format!("{}-{archive_name}", Uuid::new_v4());
    let archive_path = SafePath::resolve_child(&tmp_dir, &temp_archive_name)?;
    let staging_dir =
        SafePath::resolve_child(runtimes_dir, format!(".thk-jre-staging-{}", Uuid::new_v4()))?;

    let mut cleanup_guard = TempInstallCleanupGuard {
        archive_path: archive_path.clone(),
        staging_dir: Some(staging_dir.clone()),
    };

    let mut task = DownloadTask::new(pkg.download_url.clone(), archive_path.clone());
    task.expected_sha256 = pkg.sha256.clone();
    task.expected_size = pkg.size_bytes;
    task.force_overwrite = true;
    task.label = format!("{} Java {}", pkg.distro.display_name(), pkg.major_version);

    downloader
        .download_batch(
            &format!(
                "Downloading {} {}",
                pkg.distro.display_name(),
                pkg.major_version
            ),
            vec![task],
            1,
            cancel.clone(),
            on_progress,
        )
        .await?;

    tokio::fs::create_dir_all(&staging_dir).await?;

    let archive_clone = archive_path.clone();
    let staging_clone = staging_dir.clone();
    let cancel_for_extract = cancel.clone();
    let lower_archive = archive_name.to_ascii_lowercase();
    tokio::task::spawn_blocking(move || -> ThkResult<()> {
        if cancel_for_extract
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            return Err(ThkError::Cancelled);
        }
        if lower_archive.ends_with(".zip") {
            extract_zip_cancellable(&archive_clone, &staging_clone, cancel_for_extract.as_ref())?;
        } else if lower_archive.ends_with(".tar.xz") || lower_archive.ends_with(".txz") {
            extract_tarxz_cancellable(&archive_clone, &staging_clone, cancel_for_extract.as_ref())?;
        } else {
            extract_targz_cancellable(&archive_clone, &staging_clone, cancel_for_extract.as_ref())?;
        }
        if cancel_for_extract
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            return Err(ThkError::Cancelled);
        }
        flatten_single_subdir(&staging_clone)?;
        ensure_runtime_executable_permissions(&staging_clone);
        Ok(())
    })
    .await
    .map_err(|e| ThkError::Java(format!("Extraction worker failed: {e}")))??;

    let _ = tokio::fs::remove_file(&archive_path).await;

    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }

    let staging_bin = find_java_executable_in_dir(&staging_dir, 4).ok_or_else(|| {
        ThkError::Java(format!(
            "Java executable not found inside extracted {} archive",
            pkg.distro.display_name()
        ))
    })?;
    probe_java_binary(&staging_bin, true)?;

    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }

    let backup_dir = if dest_dir.exists() {
        let bkp =
            SafePath::resolve_child(runtimes_dir, format!(".thk-jre-backup-{}", Uuid::new_v4()))?;
        tokio::fs::rename(&dest_dir, &bkp).await?;
        Some(bkp)
    } else {
        None
    };

    if let Err(rename_err) = tokio::fs::rename(&staging_dir, &dest_dir).await {
        if let Some(ref bkp) = backup_dir {
            let _ = tokio::fs::rename(bkp, &dest_dir).await;
        }
        return Err(ThkError::Io(rename_err));
    }

    cleanup_guard.staging_dir = None;
    if let Some(bkp) = backup_dir {
        let _ = tokio::fs::remove_dir_all(&bkp).await;
    }

    let java_bin = find_java_executable_in_dir(&dest_dir, 4).ok_or_else(|| {
        ThkError::Java(format!(
            "Java executable not found inside installed {} directory",
            pkg.distro.display_name()
        ))
    })?;

    probe_java_binary(&java_bin, true)
}

pub(crate) async fn ensure_fallback_distro_java<F>(
    downloader: &DownloadManager,
    runtimes_dir: &Path,
    required_major: u32,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<JavaInstallation>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let candidate_majors: Vec<u32> = SUPPORTED_JAVA_MAJORS
        .into_iter()
        .filter(|&m| m >= required_major)
        .collect();

    for distro in [JavaDistroKind::Adoptium, JavaDistroKind::Azul] {
        if let Ok(mut pkgs) = fetch_available_distro_packages(downloader, distro).await {
            pkgs.sort_by_key(|p| p.major_version);
            if let Some(chosen) = pkgs
                .into_iter()
                .find(|p| candidate_majors.contains(&p.major_version))
            {
                return install_distro_package(
                    downloader,
                    runtimes_dir,
                    &chosen,
                    cancel,
                    on_progress,
                )
                .await;
            }
        }
    }

    Err(ThkError::Java(format!(
        "No compatible native JRE distribution found for Java {required_major}"
    )))
}

#[derive(Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
    release_name: String,
}

#[derive(Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Deserialize)]
struct AdoptiumPackage {
    name: String,
    link: String,
    checksum: Option<String>,
    size: Option<u64>,
}

async fn fetch_adoptium_packages(
    downloader: &DownloadManager,
) -> ThkResult<Vec<DistroReleasePackage>> {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    };

    let mut out = Vec::new();
    for major in SUPPORTED_JAVA_MAJORS {
        let url = format!(
            "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture={arch}&image_type=jre&os={os}&vendor=eclipse"
        );
        if let Ok(assets) = downloader.get_json::<Vec<AdoptiumAsset>>(&url).await {
            if let Some(first) = assets.into_iter().next() {
                out.push(DistroReleasePackage {
                    distro: JavaDistroKind::Adoptium,
                    major_version: major,
                    release_name: first.release_name,
                    download_url: first.binary.package.link,
                    file_name: first.binary.package.name,
                    sha256: first.binary.package.checksum,
                    size_bytes: first.binary.package.size,
                });
            }
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct AzulPkg {
    #[serde(default)]
    package_uuid: Option<String>,
    name: String,
    download_url: String,
    java_version: Vec<u32>,
}

#[derive(Deserialize)]
struct AzulPkgDetail {
    #[serde(default)]
    sha256_hash: Option<String>,
}

async fn fetch_azul_packages(downloader: &DownloadManager) -> ThkResult<Vec<DistroReleasePackage>> {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux-glibc"
    };
    let arch_query = if cfg!(target_arch = "aarch64") {
        "arch=aarch64"
    } else {
        "arch=x86&hw_bitness=64"
    };
    let ext = if cfg!(target_os = "windows") {
        "zip"
    } else {
        "tar.gz"
    };

    let mut out = Vec::new();
    for major in SUPPORTED_JAVA_MAJORS {
        let url = format!(
            "https://api.azul.com/metadata/v1/zulu/packages/?java_version={major}&os={os}&{arch_query}&archive_type={ext}&java_package_type=jre&javafx_bundled=false&latest=true&release_status=ga&availability_types=CA&page=1&page_size=1"
        );
        if let Ok(pkgs) = downloader.get_json::<Vec<AzulPkg>>(&url).await {
            if let Some(first) = pkgs.into_iter().next() {
                let mut sha256 = None;
                if let Some(ref uuid) = first.package_uuid {
                    let detail_url =
                        format!("https://api.azul.com/metadata/v1/zulu/packages/{uuid}");
                    if let Ok(detail) = downloader.get_json::<AzulPkgDetail>(&detail_url).await {
                        sha256 = detail
                            .sha256_hash
                            .and_then(|s| parse_sha256_sidecar_text(&s));
                    }
                }
                let ver_str = first
                    .java_version
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(".");
                out.push(DistroReleasePackage {
                    distro: JavaDistroKind::Azul,
                    major_version: major,
                    release_name: format!("Zulu {ver_str}"),
                    download_url: first.download_url,
                    file_name: first.name,
                    sha256,
                    size_bytes: None,
                });
            }
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

pub(crate) fn parse_sha256_sidecar_text(raw: &str) -> Option<String> {
    let token = raw.split_whitespace().next()?.trim();
    if token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(token.to_ascii_lowercase())
    } else {
        None
    }
}

#[cfg(unix)]
fn ensure_runtime_executable_permissions(runtime_dir: &Path) {
    fn walk_and_chmod(dir: &Path, depth: usize) {
        if depth == 0 || !dir.is_dir() {
            return;
        }
        let in_bin_dir = dir
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("bin"));
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let ft = meta.file_type();
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                walk_and_chmod(&path, depth - 1);
            } else if ft.is_file() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                let needs_exec = in_bin_dir
                    || name_str == "jspawnhelper"
                    || name_str == "jexec"
                    || name_str.ends_with(".so")
                    || name_str.contains(".so.");
                if needs_exec {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = meta.permissions();
                    perms.set_mode(perms.mode() | 0o755);
                    let _ = std::fs::set_permissions(&path, perms);
                }
            }
        }
    }
    walk_and_chmod(runtime_dir, 6);
}

#[cfg(not(unix))]
fn ensure_runtime_executable_permissions(_runtime_dir: &Path) {}

pub(crate) async fn ensure_android_openjdk_java<F>(
    downloader: &DownloadManager,
    runtimes_dir: &Path,
    spec: &RequiredJavaSpec,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<JavaInstallation>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    if !cfg!(target_arch = "aarch64") {
        return Err(ThkError::Java(
            "Gói OpenJDK Android hiện yêu cầu kiến trúc arm64-v8a (aarch64).".into(),
        ));
    }
    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }

    if let Some(target_major) = android_openjdk_major_for_spec(spec.major_version) {
        let managed_dir = SafePath::resolve_child(runtimes_dir, format!("openjdk-{target_major}"))?;
        if let Some(existing_bin) = find_java_executable_in_dir(&managed_dir, 4) {
            if let Ok(info) = probe_java_binary(&existing_bin, true) {
                if info.major_version >= spec.major_version {
                    return Ok(info);
                }
            }
        }

        let runtimes_owned = runtimes_dir.to_path_buf();
        let req_major = spec.major_version;
        if let Ok(Some(installed)) = tokio::task::spawn_blocking(move || {
            let list = scan_java_installations(&runtimes_owned);
            list.iter()
                .find(|j| j.major_version == target_major && j.major_version >= req_major)
                .cloned()
                .or_else(|| list.into_iter().find(|j| j.major_version >= req_major))
        })
        .await
        {
            return Ok(installed);
        }

        let pkg = android_openjdk_package_for_major(target_major).ok_or_else(|| {
            ThkError::Java(format!(
                "Không tìm thấy gói OpenJDK Android cho Java {target_major}"
            ))
        })?;

        return install_distro_package(downloader, runtimes_dir, &pkg, cancel, on_progress).await;
    }

    let runtimes_owned = runtimes_dir.to_path_buf();
    let req_major = spec.major_version;
    if let Ok(Some(installed)) = tokio::task::spawn_blocking(move || {
        scan_java_installations(&runtimes_owned)
            .into_iter()
            .find(|j| j.major_version >= req_major)
    })
    .await
    {
        return Ok(installed);
    }

    Err(ThkError::Java(format!(
        "Chưa có gói OpenJDK Android cho Java {}",
        spec.major_version
    )))
}

pub async fn ensure_optimal_java<F>(
    downloader: &DownloadManager,
    runtimes_dir: &Path,
    spec: &RequiredJavaSpec,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<JavaInstallation>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    if cfg!(target_os = "android") {
        return ensure_android_openjdk_java(downloader, runtimes_dir, spec, cancel, on_progress)
            .await;
    }
    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }

    if let Some(target_major) = optimal_graalvm_major_for_spec(spec.major_version) {
        let managed_dir = SafePath::resolve_child(runtimes_dir, format!("graalvm-{target_major}"))?;
        if let Some(existing_bin) = find_java_executable_in_dir(&managed_dir, 4) {
            if let Ok(info) = probe_java_binary(&existing_bin, true) {
                if info.major_version == target_major && info.major_version >= spec.major_version {
                    return Ok(info);
                }
            }
        }

        let runtimes_owned = runtimes_dir.to_path_buf();
        let req_major = spec.major_version;
        if let Ok(Some(sys_graal)) = tokio::task::spawn_blocking(move || {
            scan_java_installations(&runtimes_owned)
                .into_iter()
                .find(|j| {
                    j.vendor.eq_ignore_ascii_case("GraalVM")
                        && j.major_version == target_major
                        && j.major_version >= req_major
                })
        })
        .await
        {
            return Ok(sys_graal);
        }

        if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err(ThkError::Cancelled);
        }

        on_progress(DownloadProgress {
            phase: "GraalVM".to_string(),
            message: format!("Resolving GraalVM Community JDK {target_major}..."),
            completed_files: 0,
            total_files: 1,
            downloaded_bytes: 0,
            total_bytes: 0,
            bytes_per_sec: 0,
            percent: 0.0,
        });

        let pkg_res = match cancel.as_ref() {
            Some(tok) => {
                tokio::select! {
                    _ = tok.cancelled() => Err(ThkError::Cancelled),
                    res = fetch_graalvm_packages_from_api(
                        downloader,
                        GRAALVM_RELEASES_API_URL,
                        Some(target_major),
                    ) => res,
                }
            }
            None => {
                fetch_graalvm_packages_from_api(
                    downloader,
                    GRAALVM_RELEASES_API_URL,
                    Some(target_major),
                )
                .await
            }
        };

        match pkg_res {
            Ok(pkgs) => {
                if let Some(pkg) = pkgs.into_iter().find(|p| p.major_version == target_major) {
                    match install_distro_package(
                        downloader,
                        runtimes_dir,
                        &pkg,
                        cancel.clone(),
                        on_progress.clone(),
                    )
                    .await
                    {
                        Ok(inst) => return Ok(inst),
                        Err(ThkError::Cancelled) => return Err(ThkError::Cancelled),
                        Err(err) => {
                            if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
                                return Err(ThkError::Cancelled);
                            }
                            tracing::warn!(
                                %err,
                                target_major,
                                "GraalVM installation failed; falling back to Mojang JRE"
                            );
                        }
                    }
                } else {
                    tracing::warn!(
                        target_major,
                        "No GraalVM asset available for current platform; falling back to Mojang JRE"
                    );
                }
            }
            Err(ThkError::Cancelled) => return Err(ThkError::Cancelled),
            Err(err) => {
                if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
                    return Err(ThkError::Cancelled);
                }
                tracing::warn!(
                    %err,
                    target_major,
                    "GraalVM metadata query failed; falling back to Mojang JRE"
                );
            }
        }
    }

    ensure_mojang_java(downloader, runtimes_dir, spec, cancel, on_progress).await
}

async fn fetch_graalvm_packages(
    downloader: &DownloadManager,
) -> ThkResult<Vec<DistroReleasePackage>> {
    fetch_graalvm_packages_from_api(downloader, GRAALVM_RELEASES_API_URL, None).await
}

pub(crate) async fn fetch_graalvm_packages_from_api(
    downloader: &DownloadManager,
    releases_base_url: &str,
    filter_major: Option<u32>,
) -> ThkResult<Vec<DistroReleasePackage>> {
    let os_token = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch_token = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    };
    let ext = if cfg!(target_os = "windows") {
        ".zip"
    } else {
        ".tar.gz"
    };

    let base = releases_base_url.trim_end_matches('/');
    let mut releases: Vec<GhRelease> = downloader
        .get_json(&format!("{base}?per_page=100&page=1"))
        .await?;

    let target_majors: Vec<u32> = match filter_major {
        Some(major) => vec![major],
        None => vec![17, 21, 25],
    };
    let has_all_majors = target_majors.iter().all(|major| {
        let prefix = format!("graalvm-community-jdk-{major}");
        let suffix = format!("_{os_token}-{arch_token}_bin{ext}");
        releases.iter().any(|rel| {
            rel.assets
                .iter()
                .any(|a| a.name.starts_with(&prefix) && a.name.ends_with(&suffix))
        })
    });

    if !has_all_majors && releases.len() >= 100 {
        if let Ok(mut page2) = downloader
            .get_json::<Vec<GhRelease>>(&format!("{base}?per_page=100&page=2"))
            .await
        {
            releases.append(&mut page2);
        }
    }

    let mut out = Vec::new();
    for major in target_majors {
        let prefix = format!("graalvm-community-jdk-{major}");
        let suffix = format!("_{os_token}-{arch_token}_bin{ext}");
        'rel: for rel in &releases {
            for asset in &rel.assets {
                if asset.name.starts_with(&prefix) && asset.name.ends_with(&suffix) {
                    let sha_asset_name = format!("{}.sha256", asset.name);
                    let mut sha256 = None;
                    if let Some(sha_asset) = rel.assets.iter().find(|a| a.name == sha_asset_name) {
                        if let Ok(bytes) =
                            downloader.get_bytes(&sha_asset.browser_download_url).await
                        {
                            if let Ok(txt) = std::str::from_utf8(&bytes) {
                                sha256 = parse_sha256_sidecar_text(txt);
                            }
                        }
                    }
                    out.push(DistroReleasePackage {
                        distro: JavaDistroKind::GraalVm,
                        major_version: major,
                        release_name: rel.tag_name.clone(),
                        download_url: asset.browser_download_url.clone(),
                        file_name: asset.name.clone(),
                        sha256,
                        size_bytes: Some(asset.size),
                    });
                    break 'rel;
                }
            }
        }
    }
    Ok(out)
}
