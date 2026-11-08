use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;
use thk_core::models::{DownloadProgress, LoaderType, ServerEngine};
use thk_core::{DownloadManager, DownloadTask, SafePath, ThkError, ThkResult};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::task::JoinSet;

const MOJANG_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
const PAPER_API_BASE: &str = "https://fill.papermc.io/v3/projects";
const PURPUR_API_BASE: &str = "https://api.purpurmc.org/v2/purpur";
const FABRIC_META_BASE: &str = "https://meta.fabricmc.net/v2";
const MOHIST_API_BASE: &str = "https://mohistmc.com/api/v2/projects/mohist";
const SPONGE_API_BASE: &str =
    "https://dl-api.spongepowered.org/v2/groups/org.spongepowered/artifacts/spongevanilla";
const ARCLIGHT_RELEASES_URL: &str =
    "https://api.github.com/repos/IzzelAliz/Arclight/releases?per_page=30";
const BMCL_FORGE_BASE: &str = "https://bmclapi2.bangbang93.com/forge/minecraft";
const FORGE_PROMOS_URL: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const FORGE_MAVEN_BASE: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
const NEOFORGE_MAVEN_BASE: &str = "https://maven.neoforged.net/releases";

#[derive(Deserialize)]
struct MojangManifest {
    versions: Vec<MojangManifestEntry>,
}

#[derive(Deserialize)]
struct MojangManifestEntry {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

#[derive(Deserialize)]
struct MojangVersionDetail {
    downloads: Option<MojangDownloads>,
}

#[derive(Deserialize)]
struct MojangDownloads {
    server: Option<MojangServerArtifact>,
}

#[derive(Deserialize)]
struct MojangServerArtifact {
    sha1: String,
    size: u64,
    url: String,
}

#[derive(Deserialize)]
struct ProjectVersionsResp {
    versions: Vec<String>,
}

#[derive(Deserialize)]
struct PaperProjectResp {
    versions: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct PaperBuildItem {
    id: u32,
    channel: String,
    downloads: HashMap<String, PaperAppArtifact>,
}

#[derive(Deserialize)]
struct PaperChecksums {
    sha256: String,
}

#[derive(Deserialize)]
struct PaperAppArtifact {
    url: String,
    size: u64,
    checksums: PaperChecksums,
}

fn paper_release_versions(project: PaperProjectResp) -> Vec<String> {
    let mut versions: Vec<(Vec<u32>, String)> = project
        .versions
        .into_values()
        .flatten()
        .filter_map(|version| {
            let components = version
                .split('.')
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            Some((components, version))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.dedup_by(|a, b| a.1 == b.1);
    versions.into_iter().map(|(_, version)| version).collect()
}

fn paper_server_artifact(
    builds: Vec<PaperBuildItem>,
    project: &str,
    mc_version: &str,
) -> ThkResult<PaperAppArtifact> {
    let mut latest = builds
        .into_iter()
        .filter(|build| build.channel == "STABLE")
        .max_by_key(|build| build.id)
        .ok_or_else(|| {
            ThkError::Loader(format!(
                "No stable {project} builds available for Minecraft {mc_version}"
            ))
        })?;
    latest.downloads.remove("server:default").ok_or_else(|| {
        ThkError::Loader(format!(
            "Missing {project} server:default download metadata"
        ))
    })
}

#[derive(Deserialize)]
struct FabricGameVer {
    version: String,
    stable: bool,
}

#[derive(Deserialize)]
struct FabricLoaderEntry {
    loader: FabricVerNode,
}

#[derive(Deserialize)]
struct FabricVerNode {
    version: String,
}

#[derive(Deserialize)]
struct MohistBuildsResp {
    builds: Vec<MohistBuildItem>,
}

#[derive(Deserialize)]
struct MohistBuildItem {
    #[serde(rename = "fileSha256")]
    file_sha256: Option<String>,
    #[serde(rename = "originUrl")]
    origin_url: Option<String>,
    url: String,
    #[serde(rename = "createdAt", default)]
    created_at: i64,
}

#[derive(Deserialize)]
struct GithubReleaseItem {
    assets: Vec<GithubAssetItem>,
}

#[derive(Deserialize)]
struct GithubAssetItem {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
}

#[derive(Deserialize)]
struct SpongeArtifactOverview {
    tags: Option<SpongeTags>,
}

#[derive(Deserialize)]
struct SpongeTags {
    #[serde(default)]
    minecraft: Vec<String>,
}

#[derive(Deserialize)]
struct SpongeVersionsResp {
    #[serde(default)]
    artifacts: BTreeMap<String, SpongeVersionEntry>,
}

#[derive(Deserialize)]
pub(crate) struct SpongeVersionEntry {
    #[serde(default)]
    pub(crate) recommended: bool,
}

#[derive(Deserialize)]
struct SpongeSingleVersionResp {
    #[serde(default)]
    assets: Vec<SpongeAssetItem>,
}

#[derive(Deserialize)]
struct SpongeAssetItem {
    #[serde(default)]
    classifier: String,
    #[serde(rename = "downloadUrl")]
    download_url: String,
    #[serde(default)]
    extension: String,
    sha1: Option<String>,
}

#[derive(Deserialize)]
struct BmclForgeItem {
    #[serde(default)]
    build: u64,
    version: String,
    #[serde(default)]
    branch: Option<String>,
}

#[derive(Deserialize)]
struct ForgePromosResp {
    promos: HashMap<String, String>,
}

pub async fn list_supported_versions_for_engine(
    downloader: &DownloadManager,
    engine: ServerEngine,
) -> ThkResult<Vec<String>> {
    match engine {
        ServerEngine::Vanilla => fetch_mojang_releases(downloader, 50).await,
        ServerEngine::Paper | ServerEngine::Folia => {
            let project = if engine == ServerEngine::Folia {
                "folia"
            } else {
                "paper"
            };
            let resp: PaperProjectResp = downloader
                .get_json(&format!("{PAPER_API_BASE}/{project}"))
                .await?;
            Ok(paper_release_versions(resp))
        }
        ServerEngine::Purpur => {
            let mut resp: ProjectVersionsResp = downloader.get_json(PURPUR_API_BASE).await?;
            resp.versions.reverse();
            Ok(resp.versions)
        }
        ServerEngine::Fabric => {
            let list: Vec<FabricGameVer> = downloader
                .get_json(&format!("{FABRIC_META_BASE}/versions/game"))
                .await?;
            Ok(list
                .into_iter()
                .filter(|v| v.stable)
                .map(|v| v.version)
                .take(45)
                .collect())
        }
        ServerEngine::Forge => {
            let all = fetch_mojang_releases(downloader, 80).await?;
            Ok(all.into_iter().filter(|v| is_mc_at_least_1_7(v)).collect())
        }
        ServerEngine::NeoForge => {
            let all = fetch_mojang_releases(downloader, 40).await?;
            Ok(all
                .into_iter()
                .filter(|v| is_mc_at_least_1_20_1(v))
                .collect())
        }
        ServerEngine::Mohist => {
            let mut resp: ProjectVersionsResp = downloader.get_json(MOHIST_API_BASE).await?;
            resp.versions.reverse();
            let mut set = JoinSet::new();
            for (idx, ver) in resp.versions.into_iter().enumerate() {
                let dl = downloader.clone();
                set.spawn(async move {
                    let builds_url = format!("{MOHIST_API_BASE}/{ver}/builds");
                    let has_builds = dl
                        .get_json::<MohistBuildsResp>(&builds_url)
                        .await
                        .map(|r| !r.builds.is_empty())
                        .unwrap_or(false);
                    (idx, ver, has_builds)
                });
            }
            let mut valid = Vec::new();
            while let Some(Ok((idx, ver, ok))) = set.join_next().await {
                if ok {
                    valid.push((idx, ver));
                }
            }
            valid.sort_by_key(|(idx, _)| *idx);
            Ok(valid.into_iter().map(|(_, v)| v).collect())
        }
        ServerEngine::Arclight => {
            let releases: Vec<GithubReleaseItem> =
                downloader.get_json(ARCLIGHT_RELEASES_URL).await?;
            let re =
                Regex::new(r"^arclight-(?:fabric|forge|neoforge)-([0-9]+\.[0-9]+(?:\.[0-9]+)?)-")
                    .expect("valid regex");
            let mut seen = HashSet::new();
            let mut out = Vec::new();
            for rel in releases {
                for asset in rel.assets {
                    if !asset.name.ends_with(".jar") {
                        continue;
                    }
                    if let Some(caps) = re.captures(&asset.name) {
                        let ver = caps[1].to_string();
                        if seen.insert(ver.clone()) {
                            out.push(ver);
                        }
                    }
                }
            }
            Ok(out)
        }
        ServerEngine::Sponge => {
            let overview: SpongeArtifactOverview = downloader.get_json(SPONGE_API_BASE).await?;
            let all = overview.tags.map(|t| t.minecraft).unwrap_or_default();
            let stable: Vec<String> = all
                .iter()
                .filter(|v| {
                    !v.contains('-')
                        && !v.contains('w')
                        && v.chars().next().is_some_and(|c| c.is_ascii_digit())
                })
                .cloned()
                .collect();
            if stable.is_empty() {
                Ok(all)
            } else {
                Ok(stable)
            }
        }
    }
}

pub async fn install_server_engine<F>(
    downloader: &DownloadManager,
    engine: ServerEngine,
    mc_version: &str,
    server_dir: &Path,
    java_executable: &Path,
    on_progress: F,
) -> ThkResult<String>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let clean_mc = SafePath::validate_single_name(mc_version)?;
    tokio::fs::create_dir_all(server_dir).await?;

    match engine {
        ServerEngine::Vanilla => {
            let manifest: MojangManifest = downloader.get_json(MOJANG_MANIFEST_URL).await?;
            let entry = manifest
                .versions
                .into_iter()
                .find(|v| v.id == clean_mc)
                .ok_or_else(|| {
                    ThkError::Loader(format!(
                        "Minecraft version {clean_mc} not found in Mojang manifest"
                    ))
                })?;
            let detail: MojangVersionDetail = downloader.get_json(&entry.url).await?;
            let srv = detail.downloads.and_then(|d| d.server).ok_or_else(|| {
                ThkError::Loader(format!("Mojang provides no server JAR for {clean_mc}"))
            })?;

            let jar_name = format!("vanilla-{clean_mc}.jar");
            let dest = SafePath::resolve_child(server_dir, &jar_name)?;
            let mut task = DownloadTask::new(srv.url, dest);
            task.expected_sha1 = Some(srv.sha1);
            task.expected_size = Some(srv.size);
            task.force_overwrite = true;
            downloader
                .download_batch("server-jar", vec![task], 1, None, on_progress)
                .await?;
            Ok(jar_name)
        }
        ServerEngine::Paper | ServerEngine::Folia => {
            let proj = if engine == ServerEngine::Folia {
                "folia"
            } else {
                "paper"
            };
            let builds_url = format!("{PAPER_API_BASE}/{proj}/versions/{clean_mc}/builds");
            let builds: Vec<PaperBuildItem> = downloader.get_json(&builds_url).await?;
            let app = paper_server_artifact(builds, proj, clean_mc)?;
            let jar_name = format!("{proj}-{clean_mc}.jar");
            let dest = SafePath::resolve_child(server_dir, &jar_name)?;
            let mut task = DownloadTask::new(app.url, dest);
            task.expected_sha256 = Some(app.checksums.sha256);
            task.expected_size = Some(app.size);
            task.force_overwrite = true;
            downloader
                .download_batch("server-jar", vec![task], 1, None, on_progress)
                .await?;
            Ok(jar_name)
        }
        ServerEngine::Purpur => {
            let dl_url = format!("{PURPUR_API_BASE}/{clean_mc}/latest/download");
            let jar_name = format!("purpur-{clean_mc}.jar");
            let dest = SafePath::resolve_child(server_dir, &jar_name)?;
            let mut task = DownloadTask::new(dl_url, dest);
            task.force_overwrite = true;
            downloader
                .download_batch("server-jar", vec![task], 1, None, on_progress)
                .await?;
            Ok(jar_name)
        }
        ServerEngine::Fabric => {
            let loaders: Vec<FabricLoaderEntry> = downloader
                .get_json(&format!("{FABRIC_META_BASE}/versions/loader/{clean_mc}"))
                .await?;
            let loader_ver = loaders
                .first()
                .map(|l| l.loader.version.as_str())
                .ok_or_else(|| ThkError::Loader(format!("No Fabric loader for {clean_mc}")))?;
            let installers: Vec<FabricVerNode> = downloader
                .get_json(&format!("{FABRIC_META_BASE}/versions/installer"))
                .await?;
            let inst_ver = installers
                .first()
                .map(|i| i.version.as_str())
                .ok_or_else(|| ThkError::Loader("No Fabric installer metadata found".into()))?;

            let dl_url = format!(
                "{FABRIC_META_BASE}/versions/loader/{clean_mc}/{loader_ver}/{inst_ver}/server/jar"
            );
            let jar_name = format!("fabric-server-{clean_mc}.jar");
            let dest = SafePath::resolve_child(server_dir, &jar_name)?;
            let mut task = DownloadTask::new(dl_url, dest);
            task.force_overwrite = true;
            downloader
                .download_batch("server-jar", vec![task], 1, None, on_progress)
                .await?;
            Ok(jar_name)
        }
        ServerEngine::Mohist => {
            let builds_url = format!("{MOHIST_API_BASE}/{clean_mc}/builds");
            let mut resp: MohistBuildsResp = downloader.get_json(&builds_url).await?;
            resp.builds.sort_by_key(|b| b.created_at);
            let latest = resp.builds.pop().ok_or_else(|| {
                ThkError::Loader(format!("No Mohist server builds found for {clean_mc}"))
            })?;

            let jar_name = format!("mohist-{clean_mc}.jar");
            let dest = SafePath::resolve_child(server_dir, &jar_name)?;
            let mut task = DownloadTask::new(latest.url, dest);
            if let Some(orig) = latest.origin_url {
                task.fallback_urls.push(orig);
            }
            task.expected_sha256 = latest.file_sha256;
            task.force_overwrite = true;
            downloader
                .download_batch("server-jar", vec![task], 1, None, on_progress)
                .await?;
            Ok(jar_name)
        }
        ServerEngine::Arclight => {
            let releases: Vec<GithubReleaseItem> =
                downloader.get_json(ARCLIGHT_RELEASES_URL).await?;
            let needle = format!("-{clean_mc}-");
            let mut chosen: Option<GithubAssetItem> = None;
            for rel in releases {
                let mut candidates: Vec<GithubAssetItem> = rel
                    .assets
                    .into_iter()
                    .filter(|a| a.name.ends_with(".jar") && a.name.contains(&needle))
                    .collect();
                if !candidates.is_empty() {
                    candidates.sort_by_key(|a| {
                        if a.name.contains("neoforge") {
                            0
                        } else if a.name.contains("forge") {
                            1
                        } else {
                            2
                        }
                    });
                    chosen = Some(candidates.remove(0));
                    break;
                }
            }
            let asset = chosen.ok_or_else(|| {
                ThkError::Loader(format!(
                    "No Arclight release JAR found for Minecraft {clean_mc}"
                ))
            })?;
            let arclight_loader = if asset.name.contains("neoforge") {
                "neoforge"
            } else if asset.name.contains("fabric") {
                "fabric"
            } else {
                "forge"
            };
            let jar_name = format!("arclight-{clean_mc}.jar");
            let dest = SafePath::resolve_child(server_dir, &jar_name)?;
            let mut task = DownloadTask::new(asset.browser_download_url, dest);
            task.expected_size = asset.size;
            task.force_overwrite = true;
            downloader
                .download_batch("server-jar", vec![task], 1, None, on_progress)
                .await?;
            if let Ok(marker) = SafePath::resolve_child(server_dir, ".thk-arclight-loader") {
                let _ = tokio::fs::write(marker, arclight_loader).await;
            }
            Ok(jar_name)
        }
        ServerEngine::Sponge => {
            let vers_url = format!("{SPONGE_API_BASE}/versions?limit=25&tags=minecraft:{clean_mc}");
            let vers_resp: SpongeVersionsResp = downloader.get_json(&vers_url).await?;
            let ver_key = pick_best_sponge_version(&vers_resp.artifacts).ok_or_else(|| {
                ThkError::Loader(format!("No SpongeVanilla version found for {clean_mc}"))
            })?;

            let detail_url = format!("{SPONGE_API_BASE}/versions/{ver_key}");
            let detail: SpongeSingleVersionResp = downloader.get_json(&detail_url).await?;
            let asset = detail
                .assets
                .iter()
                .find(|a| a.extension == "jar" && a.classifier == "universal")
                .or_else(|| {
                    detail
                        .assets
                        .iter()
                        .find(|a| a.extension == "jar" && a.classifier.is_empty())
                })
                .ok_or_else(|| {
                    ThkError::Loader(format!("No runnable SpongeVanilla JAR asset in {ver_key}"))
                })?;

            let jar_name = format!("spongevanilla-{clean_mc}.jar");
            let dest = SafePath::resolve_child(server_dir, &jar_name)?;
            let mut task = DownloadTask::new(asset.download_url.clone(), dest);
            task.expected_sha1 = asset.sha1.clone();
            task.force_overwrite = true;
            downloader
                .download_batch("server-jar", vec![task], 1, None, on_progress)
                .await?;
            Ok(jar_name)
        }
        ServerEngine::Forge => {
            install_forge_server(
                downloader,
                clean_mc,
                server_dir,
                java_executable,
                on_progress,
            )
            .await
        }
        ServerEngine::NeoForge => {
            install_neoforge_server(
                downloader,
                clean_mc,
                server_dir,
                java_executable,
                on_progress,
            )
            .await
        }
    }
}

pub fn detect_arclight_loader(server_dir: &Path, mc_version: &str) -> LoaderType {
    if let Ok(marker) = SafePath::resolve_child(server_dir, ".thk-arclight-loader") {
        if let Ok(raw) = std::fs::read_to_string(marker) {
            match raw.trim().to_ascii_lowercase().as_str() {
                "neoforge" => return LoaderType::NeoForge,
                "fabric" => return LoaderType::Fabric,
                "forge" => return LoaderType::Forge,
                _ => {}
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(server_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.starts_with("arclight-") && name.ends_with(".jar") {
                if name.contains("neoforge") {
                    return LoaderType::NeoForge;
                }
                if name.contains("fabric") {
                    return LoaderType::Fabric;
                }
                if name.contains("forge") {
                    return LoaderType::Forge;
                }
            }
        }
    }

    let parts: Vec<u32> = mc_version
        .split('.')
        .filter_map(|p| p.parse::<u32>().ok())
        .collect();
    match parts.as_slice() {
        [major, ..] if *major >= 25 => LoaderType::Fabric,
        [1, 21, patch, ..] if *patch >= 4 => LoaderType::Fabric,
        [1, minor, ..] if *minor >= 21 => LoaderType::NeoForge,
        _ => LoaderType::Forge,
    }
}

pub(crate) fn pick_best_sponge_version(
    artifacts: &BTreeMap<String, SpongeVersionEntry>,
) -> Option<String> {
    artifacts
        .iter()
        .filter(|(_, meta)| meta.recommended)
        .max_by_key(|(k, _)| sponge_version_sort_key(k))
        .or_else(|| {
            artifacts
                .iter()
                .max_by_key(|(k, _)| sponge_version_sort_key(k))
        })
        .map(|(k, _)| k.clone())
}

fn sponge_version_sort_key(ver: &str) -> (Vec<u32>, u8, u32) {
    let lower = ver.to_ascii_lowercase();
    let (main_part, pre_rank, pre_num) = if let Some((base, rc)) = lower.rsplit_once("-rc") {
        (base, 2u8, rc.parse::<u32>().unwrap_or(0))
    } else if let Some((base, beta)) = lower.rsplit_once("-beta") {
        (
            base,
            1u8,
            beta.trim_start_matches('.').parse::<u32>().unwrap_or(0),
        )
    } else if let Some((base, alpha)) = lower.rsplit_once("-alpha") {
        (
            base,
            0u8,
            alpha.trim_start_matches('.').parse::<u32>().unwrap_or(0),
        )
    } else {
        (lower.as_str(), 3u8, u32::MAX)
    };

    let nums: Vec<u32> = main_part
        .split(['.', '-'])
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();
    (nums, pre_rank, pre_num)
}

async fn install_forge_server<F>(
    downloader: &DownloadManager,
    mc_version: &str,
    server_dir: &Path,
    java_executable: &Path,
    on_progress: F,
) -> ThkResult<String>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let forge_ver = resolve_latest_forge_version(downloader, mc_version).await?;
    let full_coord = format!("{mc_version}-{forge_ver}");
    let installer_name = format!("forge-{full_coord}-installer.jar");
    let installer_url = format!("{FORGE_MAVEN_BASE}/{full_coord}/{installer_name}");
    let installer_path = SafePath::resolve_child(server_dir, &installer_name)?;

    let mut task = DownloadTask::new(installer_url, installer_path.clone());
    task.force_overwrite = true;
    task.fallback_urls.push(format!(
        "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/{full_coord}/{installer_name}"
    ));
    if !forge_ver.contains('-') {
        let branched = format!("{mc_version}-{forge_ver}-{mc_version}");
        let branched_name = format!("forge-{branched}-installer.jar");
        task.fallback_urls
            .push(format!("{FORGE_MAVEN_BASE}/{branched}/{branched_name}"));
        task.fallback_urls.push(format!(
            "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/{branched}/{branched_name}"
        ));
    }
    let on_progress = std::sync::Arc::new(on_progress);
    let dl_cb = {
        let cb = std::sync::Arc::clone(&on_progress);
        move |p| cb(p)
    };
    downloader
        .download_batch("forge-server-installer", vec![task], 1, None, dl_cb)
        .await?;

    run_server_installer_jar(java_executable, server_dir, &installer_path, &*on_progress).await?;
    let _ = tokio::fs::remove_file(&installer_path).await;
    let _ = tokio::fs::remove_file(server_dir.join(format!("{installer_name}.log"))).await;

    for coord in [
        full_coord.clone(),
        format!("{mc_version}-{forge_ver}-{mc_version}"),
    ] {
        let argfile_rel = format!(
            "libraries/net/minecraftforge/forge/{coord}/{}",
            platform_args_filename()
        );
        if server_dir.join(&argfile_rel).is_file() {
            return Ok(format!("@{argfile_rel}"));
        }

        for candidate in [
            format!("forge-{coord}.jar"),
            format!("forge-{coord}-universal.jar"),
            format!("forge-{coord}-shim.jar"),
        ] {
            if server_dir.join(&candidate).is_file() {
                return Ok(candidate);
            }
        }
    }

    Err(ThkError::Loader(format!(
        "Forge --installServer completed for {full_coord}, but launch target was not found"
    )))
}

async fn install_neoforge_server<F>(
    downloader: &DownloadManager,
    mc_version: &str,
    server_dir: &Path,
    java_executable: &Path,
    on_progress: F,
) -> ThkResult<String>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let (artifact_group_path, nf_ver) =
        resolve_latest_neoforge_version(downloader, mc_version).await?;
    let artifact_name = if mc_version == "1.20.1" {
        "forge"
    } else {
        "neoforge"
    };
    let installer_name = format!("{artifact_name}-{nf_ver}-installer.jar");
    let installer_url =
        format!("{NEOFORGE_MAVEN_BASE}/{artifact_group_path}/{nf_ver}/{installer_name}");
    let installer_path = SafePath::resolve_child(server_dir, &installer_name)?;

    let mut task = DownloadTask::new(installer_url, installer_path.clone());
    task.force_overwrite = true;
    let on_progress = std::sync::Arc::new(on_progress);
    let dl_cb = {
        let cb = std::sync::Arc::clone(&on_progress);
        move |p| cb(p)
    };
    downloader
        .download_batch("neoforge-server-installer", vec![task], 1, None, dl_cb)
        .await?;

    run_server_installer_jar(java_executable, server_dir, &installer_path, &*on_progress).await?;
    let _ = tokio::fs::remove_file(&installer_path).await;
    let _ = tokio::fs::remove_file(server_dir.join(format!("{installer_name}.log"))).await;

    let argfile_rel = format!(
        "libraries/{artifact_group_path}/{nf_ver}/{}",
        platform_args_filename()
    );
    if server_dir.join(&argfile_rel).is_file() {
        return Ok(format!("@{argfile_rel}"));
    }

    Err(ThkError::Loader(format!(
        "NeoForge --installServer completed for {nf_ver}, but {argfile_rel} was not generated"
    )))
}

async fn run_server_installer_jar<F>(
    java_executable: &Path,
    server_dir: &Path,
    installer_jar: &Path,
    on_progress: &F,
) -> ThkResult<()>
where
    F: Fn(DownloadProgress) + Send + Sync,
{
    let mut cmd = Command::new(java_executable);
    thk_java::configure_tokio_java_command_env(&mut cmd, java_executable, Some(server_dir));
    cmd.current_dir(server_dir)
        .args(thk_java::android_jvm_system_property_args(server_dir))
        .arg("-Djava.awt.headless=true")
        .arg("-jar")
        .arg(installer_jar)
        .arg("--installServer")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let mut child = cmd.spawn()?;
    let stdout_opt = child.stdout.take();
    let stderr_opt = child.stderr.take();

    let out_fut = async {
        let mut last = String::new();
        if let Some(out) = stdout_opt {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    last = line.trim().to_string();
                    on_progress(DownloadProgress {
                        phase: "Server Installer".into(),
                        message: last.clone(),
                        completed_files: 0,
                        total_files: 1,
                        downloaded_bytes: 0,
                        total_bytes: 0,
                        bytes_per_sec: 0,
                        percent: 92.0,
                    });
                }
            }
        }
        last
    };

    let err_fut = async {
        let mut buf = Vec::new();
        if let Some(err) = stderr_opt {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    if buf.len() >= 12 {
                        buf.remove(0);
                    }
                    buf.push(line.trim().to_string());
                }
            }
        }
        buf.join(" | ")
    };

    let (last_out, stderr_tail) = tokio::join!(out_fut, err_fut);
    let status = child.wait().await?;
    if !status.success() {
        return Err(ThkError::Loader(format!(
            "Server installer exited with {status}: {stderr_tail} {last_out}"
        )));
    }
    Ok(())
}

async fn resolve_latest_forge_version(
    downloader: &DownloadManager,
    mc_version: &str,
) -> ThkResult<String> {
    if let Ok(list) = downloader
        .get_json::<Vec<BmclForgeItem>>(&format!("{BMCL_FORGE_BASE}/{mc_version}"))
        .await
    {
        if let Some(best) = list.into_iter().max_by_key(|i| i.build) {
            let ver = match best
                .branch
                .as_deref()
                .map(str::trim)
                .filter(|b| !b.is_empty())
            {
                Some(b) if !best.version.ends_with(&format!("-{b}")) => {
                    format!("{}-{b}", best.version)
                }
                _ => best.version,
            };
            return Ok(ver);
        }
    }

    let promos: ForgePromosResp = downloader.get_json(FORGE_PROMOS_URL).await?;
    if let Some(rec) = promos.promos.get(&format!("{mc_version}-recommended")) {
        return Ok(rec.clone());
    }
    if let Some(lat) = promos.promos.get(&format!("{mc_version}-latest")) {
        return Ok(lat.clone());
    }
    Err(ThkError::Loader(format!(
        "No Forge version found for Minecraft {mc_version}"
    )))
}

async fn resolve_latest_neoforge_version(
    downloader: &DownloadManager,
    mc_version: &str,
) -> ThkResult<(&'static str, String)> {
    let (group_path, prefix) = if mc_version == "1.20.1" {
        ("net/neoforged/forge", "1.20.1-".to_string())
    } else {
        let parts: Vec<&str> = mc_version.split('.').collect();
        let pref = match parts.as_slice() {
            ["1", minor] => format!("{minor}.0."),
            ["1", minor, patch] => format!("{minor}.{patch}."),
            [yr, drop] => format!("{yr}.{drop}.0."),
            [yr, drop, patch] => format!("{yr}.{drop}.{patch}."),
            _ => format!("{mc_version}."),
        };
        ("net/neoforged/neoforge", pref)
    };

    let xml_bytes = downloader
        .get_bytes(&format!(
            "{NEOFORGE_MAVEN_BASE}/{group_path}/maven-metadata.xml"
        ))
        .await?;
    let xml = String::from_utf8_lossy(&xml_bytes);
    let re = Regex::new(r"<version>([^<]+)</version>").expect("valid xml tag regex");
    let matching: Vec<String> = re
        .captures_iter(&xml)
        .map(|c| c[1].trim().to_string())
        .filter(|v| v.starts_with(&prefix))
        .collect();

    let chosen = matching
        .iter()
        .rev()
        .find(|v| !v.contains("beta") && !v.contains("alpha"))
        .or_else(|| matching.last())
        .cloned()
        .ok_or_else(|| {
            ThkError::Loader(format!(
                "No NeoForge version found for Minecraft {mc_version}"
            ))
        })?;

    Ok((group_path, chosen))
}

async fn fetch_mojang_releases(
    downloader: &DownloadManager,
    limit: usize,
) -> ThkResult<Vec<String>> {
    let manifest: MojangManifest = downloader.get_json(MOJANG_MANIFEST_URL).await?;
    Ok(manifest
        .versions
        .into_iter()
        .filter(|v| v.kind == "release")
        .map(|v| v.id)
        .take(limit)
        .collect())
}

fn platform_args_filename() -> &'static str {
    if cfg!(windows) {
        "win_args.txt"
    } else {
        "unix_args.txt"
    }
}

fn is_mc_at_least_1_7(ver: &str) -> bool {
    let parts: Vec<u32> = ver.split('.').filter_map(|p| p.parse().ok()).collect();
    match parts.as_slice() {
        [major, ..] if *major >= 2 => true,
        [1, minor, ..] => *minor >= 7,
        _ => false,
    }
}

fn is_mc_at_least_1_20_1(ver: &str) -> bool {
    let parts: Vec<u32> = ver.split('.').filter_map(|p| p.parse().ok()).collect();
    match parts.as_slice() {
        [major, ..] if *major >= 2 => true,
        [1, minor, patch, ..] => *minor > 20 || (*minor == 20 && *patch >= 1),
        [1, minor] => *minor > 20,
        _ => false,
    }
}

#[cfg(test)]
mod paper_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn grouped_versions_are_numeric_descending_without_prereleases_or_duplicates() {
        let project = serde_json::from_value(json!({
            "versions": {
                "26.1": ["26.1.2", "26.1.1"],
                "1.21": ["1.21.9", "1.21.11-rc3", "1.21.11", "1.21.4", "1.21.4"],
                "1.9": ["1.9.4"]
            }
        }))
        .unwrap();
        assert_eq!(
            paper_release_versions(project),
            ["26.1.2", "26.1.1", "1.21.11", "1.21.9", "1.21.4", "1.9.4"]
        );
    }

    #[test]
    fn stable_build_selection_preserves_fill_url_size_and_checksum() {
        // Artifact metadata captured from Fill v3 for Paper 1.21.4 build 232.
        let artifact = json!({
            "name": "paper-1.21.4-232.jar",
            "checksums": {"sha256": "5ee4f542f628a14c644410b08c94ea42e772ef4d29fe92973636b6813d4eaffc"},
            "size": 51437498,
            "url": "https://fill-data.papermc.io/v1/objects/5ee4f542f628a14c644410b08c94ea42e772ef4d29fe92973636b6813d4eaffc/paper-1.21.4-232.jar"
        });
        let builds = serde_json::from_value(json!([
            {"id": 233, "channel": "EXPERIMENTAL", "downloads": {}},
            {"id": 232, "channel": "STABLE", "downloads": {"server:default": artifact}},
            {"id": 231, "channel": "STABLE", "downloads": {}}
        ]))
        .unwrap();
        let selected = paper_server_artifact(builds, "paper", "1.21.4").unwrap();
        assert_eq!(selected.url, artifact["url"].as_str().unwrap());
        assert_eq!(selected.size, 51437498);
        assert_eq!(
            selected.checksums.sha256,
            artifact["checksums"]["sha256"].as_str().unwrap()
        );
    }

    #[test]
    fn missing_stable_build_or_server_artifact_is_an_error() {
        let experimental = serde_json::from_value(json!([
            {"id": 233, "channel": "EXPERIMENTAL", "downloads": {}}
        ]))
        .unwrap();
        assert!(paper_server_artifact(experimental, "folia", "1.21.4").is_err());
        assert!(paper_server_artifact(Vec::new(), "paper", "1.21.4").is_err());
        let missing_artifact = serde_json::from_value(json!([
            {"id": 232, "channel": "STABLE", "downloads": {}}
        ]))
        .unwrap();
        assert!(paper_server_artifact(missing_artifact, "paper", "1.21.4").is_err());
    }

    #[test]
    fn sponge_version_picker_prefers_rc10_over_rc9() {
        let mut map = BTreeMap::new();
        map.insert(
            "1.21.4-14.0.0-RC9".to_string(),
            SpongeVersionEntry { recommended: false },
        );
        map.insert(
            "1.21.4-14.0.0-RC10".to_string(),
            SpongeVersionEntry { recommended: false },
        );
        assert_eq!(
            pick_best_sponge_version(&map).as_deref(),
            Some("1.21.4-14.0.0-RC10")
        );
    }
}
