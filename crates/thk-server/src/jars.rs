use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;
use thk_core::models::{DownloadProgress, ServerEngine};
use thk_core::{DownloadManager, DownloadTask, SafePath, ThkError, ThkResult};
use tokio::process::Command;

const MOJANG_MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
const PAPER_API_BASE: &str = "https://api.papermc.io/v2/projects";
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
struct PaperBuildsResp {
    builds: Vec<PaperBuildItem>,
}

#[derive(Deserialize)]
struct PaperBuildItem {
    build: u32,
    downloads: Option<PaperBuildDownloads>,
}

#[derive(Deserialize)]
struct PaperBuildDownloads {
    application: Option<PaperAppArtifact>,
}

#[derive(Deserialize)]
struct PaperAppArtifact {
    name: String,
    sha256: Option<String>,
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
struct SpongeVersionEntry {
    #[serde(default)]
    recommended: bool,
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
    version: String,
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
        ServerEngine::Paper => {
            let mut resp: ProjectVersionsResp =
                downloader.get_json(&format!("{PAPER_API_BASE}/paper")).await?;
            resp.versions.reverse();
            Ok(resp.versions)
        }
        ServerEngine::Purpur => {
            let mut resp: ProjectVersionsResp = downloader.get_json(PURPUR_API_BASE).await?;
            resp.versions.reverse();
            Ok(resp.versions)
        }
        ServerEngine::Folia => {
            let mut resp: ProjectVersionsResp =
                downloader.get_json(&format!("{PAPER_API_BASE}/folia")).await?;
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
            Ok(all
                .into_iter()
                .filter(|v| is_mc_at_least_1_7(v))
                .collect())
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
            Ok(resp.versions)
        }
        ServerEngine::Arclight => {
            let releases: Vec<GithubReleaseItem> =
                downloader.get_json(ARCLIGHT_RELEASES_URL).await?;
            let re = Regex::new(r"^arclight-(?:fabric|forge|neoforge)-([0-9]+\.[0-9]+(?:\.[0-9]+)?)-")
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
                    !v.contains('-') && !v.contains('w') && v.chars().next().is_some_and(|c| c.is_ascii_digit())
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
                    ThkError::Loader(format!("Minecraft version {clean_mc} not found in Mojang manifest"))
                })?;
            let detail: MojangVersionDetail = downloader.get_json(&entry.url).await?;
            let srv = detail
                .downloads
                .and_then(|d| d.server)
                .ok_or_else(|| ThkError::Loader(format!("Mojang provides no server JAR for {clean_mc}")))?;

            let jar_name = format!("vanilla-{clean_mc}.jar");
            let dest = SafePath::resolve_child(server_dir, &jar_name)?;
            let mut task = DownloadTask::new(srv.url, dest);
            task.expected_sha1 = Some(srv.sha1);
            task.expected_size = Some(srv.size);
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
            let builds: PaperBuildsResp = downloader.get_json(&builds_url).await?;
            let latest = builds.builds.into_iter().last().ok_or_else(|| {
                ThkError::Loader(format!("No {proj} builds available for Minecraft {clean_mc}"))
            })?;
            let app = latest
                .downloads
                .and_then(|d| d.application)
                .ok_or_else(|| ThkError::Loader(format!("Missing {proj} application download metadata")))?;

            let dl_url = format!(
                "{PAPER_API_BASE}/{proj}/versions/{clean_mc}/builds/{}/downloads/{}",
                latest.build, app.name
            );
            let jar_name = format!("{proj}-{clean_mc}.jar");
            let dest = SafePath::resolve_child(server_dir, &jar_name)?;
            let mut task = DownloadTask::new(dl_url, dest);
            task.expected_sha256 = app.sha256;
            downloader
                .download_batch("server-jar", vec![task], 1, None, on_progress)
                .await?;
            Ok(jar_name)
        }
        ServerEngine::Purpur => {
            let dl_url = format!("{PURPUR_API_BASE}/{clean_mc}/latest/download");
            let jar_name = format!("purpur-{clean_mc}.jar");
            let dest = SafePath::resolve_child(server_dir, &jar_name)?;
            let task = DownloadTask::new(dl_url, dest);
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
            let task = DownloadTask::new(dl_url, dest);
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
                ThkError::Loader(format!("No Arclight release JAR found for Minecraft {clean_mc}"))
            })?;
            let jar_name = format!("arclight-{clean_mc}.jar");
            let dest = SafePath::resolve_child(server_dir, &jar_name)?;
            let mut task = DownloadTask::new(asset.browser_download_url, dest);
            task.expected_size = asset.size;
            downloader
                .download_batch("server-jar", vec![task], 1, None, on_progress)
                .await?;
            Ok(jar_name)
        }
        ServerEngine::Sponge => {
            let vers_url = format!("{SPONGE_API_BASE}/versions?limit=10&tags=minecraft:{clean_mc}");
            let vers_resp: SpongeVersionsResp = downloader.get_json(&vers_url).await?;
            let ver_key = vers_resp
                .artifacts
                .iter()
                .rev()
                .find(|(_, meta)| meta.recommended)
                .or_else(|| vers_resp.artifacts.iter().next_back())
                .map(|(k, _)| k.clone())
                .ok_or_else(|| {
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
            downloader
                .download_batch("server-jar", vec![task], 1, None, on_progress)
                .await?;
            Ok(jar_name)
        }
        ServerEngine::Forge => {
            install_forge_server(downloader, clean_mc, server_dir, java_executable, on_progress)
                .await
        }
        ServerEngine::NeoForge => {
            install_neoforge_server(downloader, clean_mc, server_dir, java_executable, on_progress)
                .await
        }
    }
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

    let task = DownloadTask::new(installer_url, installer_path.clone());
    downloader
        .download_batch("forge-server-installer", vec![task], 1, None, on_progress)
        .await?;

    run_server_installer_jar(java_executable, server_dir, &installer_path).await?;
    let _ = tokio::fs::remove_file(&installer_path).await;
    let _ = tokio::fs::remove_file(server_dir.join(format!("{installer_name}.log"))).await;

    let argfile_rel = format!(
        "libraries/net/minecraftforge/forge/{full_coord}/{}",
        platform_args_filename()
    );
    if server_dir.join(&argfile_rel).is_file() {
        return Ok(format!("@{argfile_rel}"));
    }

    for candidate in [
        format!("forge-{full_coord}.jar"),
        format!("forge-{full_coord}-universal.jar"),
        format!("forge-{full_coord}-shim.jar"),
    ] {
        if server_dir.join(&candidate).is_file() {
            return Ok(candidate);
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

    let task = DownloadTask::new(installer_url, installer_path.clone());
    downloader
        .download_batch(
            "neoforge-server-installer",
            vec![task],
            1,
            None,
            on_progress,
        )
        .await?;

    run_server_installer_jar(java_executable, server_dir, &installer_path).await?;
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

async fn run_server_installer_jar(
    java_executable: &Path,
    server_dir: &Path,
    installer_jar: &Path,
) -> ThkResult<()> {
    let mut cmd = Command::new(java_executable);
    cmd.current_dir(server_dir)
        .arg("-jar")
        .arg(installer_jar)
        .arg("--installServer")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let output = cmd.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(ThkError::Loader(format!(
            "Server installer exited with {}: {} {}",
            output.status,
            stderr.trim(),
            stdout.trim()
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
        if let Some(last) = list.last() {
            return Ok(last.version.clone());
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
        let pref = if parts.len() >= 3 && parts[0] == "1" {
            format!("{}.{}.", parts[1], parts[2])
        } else if parts.len() == 2 && parts[0] == "1" {
            format!("{}.0.", parts[1])
        } else {
            format!("{mc_version}.")
        };
        ("net/neoforged/neoforge", pref)
    };

    let xml_bytes = downloader
        .get_bytes(&format!("{NEOFORGE_MAVEN_BASE}/{group_path}/maven-metadata.xml"))
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
            ThkError::Loader(format!("No NeoForge version found for Minecraft {mc_version}"))
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
