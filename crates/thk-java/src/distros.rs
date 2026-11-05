use crate::scanner::{find_java_executable_in_dir, probe_java_binary, JavaInstallation};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thk_core::archive::{extract_targz_safe, extract_zip_safe, flatten_single_subdir};
use thk_core::models::DownloadProgress;
use thk_core::paths::SafePath;
use thk_core::{DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;

pub const SUPPORTED_JAVA_MAJORS: [u32; 5] = [8, 11, 17, 21, 25];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JavaDistroKind {
    Adoptium,
    Azul,
    GraalVm,
}

impl JavaDistroKind {
    pub const ALL: [JavaDistroKind; 3] = [Self::Adoptium, Self::Azul, Self::GraalVm];

    pub fn slug(&self) -> &'static str {
        match self {
            Self::Adoptium => "adoptium",
            Self::Azul => "azul",
            Self::GraalVm => "graalvm",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Adoptium => "Eclipse Temurin (Adoptium)",
            Self::Azul => "Azul Zulu JRE",
            Self::GraalVm => "GraalVM Community JDK",
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
    match distro {
        JavaDistroKind::Adoptium => fetch_adoptium_packages(downloader).await,
        JavaDistroKind::Azul => fetch_azul_packages(downloader).await,
        JavaDistroKind::GraalVm => fetch_graalvm_packages(downloader).await,
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
    let install_folder = format!("{}-{}", pkg.distro.slug(), pkg.major_version);
    let dest_dir = SafePath::resolve_child(runtimes_dir, &install_folder)?;
    let tmp_dir = SafePath::resolve_child(runtimes_dir, ".jre_downloads")?;
    tokio::fs::create_dir_all(&tmp_dir).await?;

    let archive_name = SafePath::validate_single_name(&pkg.file_name)?;
    let archive_path = SafePath::resolve_child(&tmp_dir, archive_name)?;

    let mut task = DownloadTask::new(pkg.download_url.clone(), archive_path.clone());
    task.expected_sha256 = pkg.sha256.clone();
    task.expected_size = pkg.size_bytes;
    task.label = format!("{} Java {}", pkg.distro.display_name(), pkg.major_version);

    downloader
        .download_batch(
            &format!("Downloading {} {}", pkg.distro.display_name(), pkg.major_version),
            vec![task],
            1,
            cancel,
            on_progress,
        )
        .await?;

    if dest_dir.exists() {
        tokio::fs::remove_dir_all(&dest_dir).await?;
    }
    tokio::fs::create_dir_all(&dest_dir).await?;

    let archive_clone = archive_path.clone();
    let dest_clone = dest_dir.clone();
    let is_zip = archive_name.to_ascii_lowercase().ends_with(".zip");
    tokio::task::spawn_blocking(move || -> ThkResult<()> {
        if is_zip {
            extract_zip_safe(&archive_clone, &dest_clone)?;
        } else {
            extract_targz_safe(&archive_clone, &dest_clone)?;
        }
        flatten_single_subdir(&dest_clone)?;
        Ok(())
    })
    .await
    .map_err(|e| ThkError::Java(format!("Extraction worker failed: {e}")))??;

    let _ = tokio::fs::remove_file(&archive_path).await;

    let java_bin = find_java_executable_in_dir(&dest_dir, 4).ok_or_else(|| {
        ThkError::Java(format!(
            "Java executable not found inside extracted {} archive",
            pkg.distro.display_name()
        ))
    })?;

    probe_java_binary(&java_bin, true)
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

async fn fetch_adoptium_packages(downloader: &DownloadManager) -> ThkResult<Vec<DistroReleasePackage>> {
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
    name: String,
    download_url: String,
    java_version: Vec<u32>,
}

async fn fetch_azul_packages(downloader: &DownloadManager) -> ThkResult<Vec<DistroReleasePackage>> {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm"
    } else {
        "x86"
    };
    let hw_bitness = "64";
    let ext = if cfg!(target_os = "windows") { "zip" } else { "tar.gz" };

    let mut out = Vec::new();
    for major in SUPPORTED_JAVA_MAJORS {
        let url = format!(
            "https://api.azul.com/metadata/v1/zulu/packages/?java_version={major}&os={os}&arch={arch}&hw_bitness={hw_bitness}&archive_type={ext}&java_package_type=jre&javafx_bundled=false&latest=true&release_status=ga&availability_types=CA&page=1&page_size=1"
        );
        if let Ok(pkgs) = downloader.get_json::<Vec<AzulPkg>>(&url).await {
            if let Some(first) = pkgs.into_iter().next() {
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
                    sha256: None,
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

async fn fetch_graalvm_packages(downloader: &DownloadManager) -> ThkResult<Vec<DistroReleasePackage>> {
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
    let ext = if cfg!(target_os = "windows") { ".zip" } else { ".tar.gz" };

    let releases: Vec<GhRelease> = downloader
        .get_json("https://api.github.com/repos/graalvm/graalvm-ce-builds/releases?per_page=30")
        .await?;

    let mut out = Vec::new();
    for major in [17u32, 21, 25] {
        let prefix = format!("graalvm-community-jdk-{major}");
        let suffix = format!("_{os_token}-{arch_token}_bin{ext}");
        'rel: for rel in &releases {
            for asset in &rel.assets {
                if asset.name.starts_with(&prefix) && asset.name.ends_with(&suffix) {
                    out.push(DistroReleasePackage {
                        distro: JavaDistroKind::GraalVm,
                        major_version: major,
                        release_name: rel.tag_name.clone(),
                        download_url: asset.browser_download_url.clone(),
                        file_name: asset.name.clone(),
                        sha256: None,
                        size_bytes: Some(asset.size),
                    });
                    break 'rel;
                }
            }
        }
    }
    Ok(out)
}
