use crate::vanilla::{evaluate_rules, maven_coord_to_rel_path, MojangLibrary};
use quick_xml::de::from_str as xml_from_str;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thk_core::archive::read_zip_entry_string;
use thk_core::models::{DownloadProgress, LoaderType};
use thk_core::paths::SafePath;
use thk_core::{AppPaths, DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const FABRIC_META_BASE: &str = "https://meta.fabricmc.net/v2";
const QUILT_META_BASE: &str = "https://meta.quiltmc.org/v3";
const FORGE_MAVEN_BASE: &str = "https://files.minecraftforge.net/maven";
const BMCLAPI_BASE: &str = "https://bmclapi2.bangbang93.com";
const NEOFORGE_MAVEN_BASE: &str = "https://maven.neoforged.net/releases";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderVersionEntry {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Clone)]
pub struct LoaderLaunchPatch {
    pub main_class: String,
    pub extra_libraries: Vec<PathBuf>,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub legacy_minecraft_arguments: Option<String>,
    pub include_client_jar_on_cp: bool,
}

pub async fn list_loader_versions(
    downloader: &DownloadManager,
    loader: LoaderType,
    mc_version: &str,
) -> ThkResult<Vec<LoaderVersionEntry>> {
    match loader {
        LoaderType::Vanilla => Ok(Vec::new()),
        LoaderType::Fabric => list_fabric_versions(downloader, mc_version).await,
        LoaderType::Quilt => list_quilt_versions(downloader, mc_version).await,
        LoaderType::Forge => list_forge_versions(downloader, mc_version).await,
        LoaderType::NeoForge => list_neoforge_versions(downloader, mc_version).await,
    }
}

#[derive(Deserialize)]
struct MetaLoaderEntry {
    loader: MetaLoaderInner,
}

#[derive(Deserialize)]
struct MetaLoaderInner {
    version: String,
    #[serde(default = "default_true")]
    stable: bool,
}

fn default_true() -> bool {
    true
}

async fn list_fabric_versions(
    downloader: &DownloadManager,
    mc_version: &str,
) -> ThkResult<Vec<LoaderVersionEntry>> {
    let url = format!("{FABRIC_META_BASE}/versions/loader/{mc_version}");
    let entries: Vec<MetaLoaderEntry> = downloader.get_json(&url).await.unwrap_or_default();
    Ok(entries
        .into_iter()
        .map(|e| LoaderVersionEntry {
            version: e.loader.version,
            stable: e.loader.stable,
        })
        .collect())
}

async fn list_quilt_versions(
    downloader: &DownloadManager,
    mc_version: &str,
) -> ThkResult<Vec<LoaderVersionEntry>> {
    let url = format!("{QUILT_META_BASE}/versions/loader/{mc_version}");
    let entries: Vec<MetaLoaderEntry> = downloader.get_json(&url).await.unwrap_or_default();
    Ok(entries
        .into_iter()
        .map(|e| {
            let stable = !e.loader.version.contains("beta") && !e.loader.version.contains("alpha");
            LoaderVersionEntry {
                version: e.loader.version,
                stable,
            }
        })
        .collect())
}

#[derive(Deserialize)]
struct BmclForgeItem {
    version: String,
}

#[derive(Deserialize)]
struct ForgePromotions {
    promos: HashMap<String, String>,
}

async fn list_forge_versions(
    downloader: &DownloadManager,
    mc_version: &str,
) -> ThkResult<Vec<LoaderVersionEntry>> {
    let bmcl_url = format!("{BMCLAPI_BASE}/forge/minecraft/{mc_version}");
    if let Ok(mut list) = downloader.get_json::<Vec<BmclForgeItem>>(&bmcl_url).await {
        if !list.is_empty() {
            list.reverse();
            return Ok(list
                .into_iter()
                .map(|i| LoaderVersionEntry {
                    version: i.version,
                    stable: true,
                })
                .collect());
        }
    }

    let promos: ForgePromotions = downloader
        .get_json("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json")
        .await?;
    let mut out = Vec::new();
    if let Some(rec) = promos.promos.get(&format!("{mc_version}-recommended")) {
        out.push(LoaderVersionEntry {
            version: rec.clone(),
            stable: true,
        });
    }
    if let Some(lat) = promos.promos.get(&format!("{mc_version}-latest")) {
        if !out.iter().any(|x| x.version == *lat) {
            out.push(LoaderVersionEntry {
                version: lat.clone(),
                stable: false,
            });
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct MavenMetadataXml {
    versioning: Option<MavenVersioningXml>,
}

#[derive(Deserialize)]
struct MavenVersioningXml {
    versions: Option<MavenVersionsXml>,
}

#[derive(Deserialize)]
struct MavenVersionsXml {
    #[serde(rename = "version", default)]
    items: Vec<String>,
}

async fn list_neoforge_versions(
    downloader: &DownloadManager,
    mc_version: &str,
) -> ThkResult<Vec<LoaderVersionEntry>> {
    if mc_version == "1.20.1" {
        let xml_bytes = downloader
            .get_bytes(&format!(
                "{NEOFORGE_MAVEN_BASE}/net/neoforged/forge/maven-metadata.xml"
            ))
            .await?;
        let xml_str = String::from_utf8_lossy(&xml_bytes);
        let parsed: MavenMetadataXml = xml_from_str(&xml_str)
            .map_err(|e| ThkError::Loader(format!("Invalid NeoForge XML metadata: {e}")))?;
        let mut vers: Vec<LoaderVersionEntry> = parsed
            .versioning
            .and_then(|v| v.versions)
            .map(|v| v.items)
            .unwrap_or_default()
            .into_iter()
            .filter(|v| v.starts_with("1.20.1-"))
            .map(|v| LoaderVersionEntry {
                stable: !v.contains("beta"),
                version: v,
            })
            .collect();
        vers.reverse();
        return Ok(vers);
    }

    let prefix = neoforge_prefix_for_mc(mc_version);
    if prefix.is_empty() {
        return Ok(Vec::new());
    }

    let xml_bytes = downloader
        .get_bytes(&format!(
            "{NEOFORGE_MAVEN_BASE}/net/neoforged/neoforge/maven-metadata.xml"
        ))
        .await?;
    let xml_str = String::from_utf8_lossy(&xml_bytes);
    let parsed: MavenMetadataXml = xml_from_str(&xml_str)
        .map_err(|e| ThkError::Loader(format!("Invalid NeoForge XML metadata: {e}")))?;

    let mut vers: Vec<LoaderVersionEntry> = parsed
        .versioning
        .and_then(|v| v.versions)
        .map(|v| v.items)
        .unwrap_or_default()
        .into_iter()
        .filter(|v| v.starts_with(&prefix))
        .map(|v| LoaderVersionEntry {
            stable: !v.contains("beta") && !v.contains("alpha"),
            version: v,
        })
        .collect();
    vers.reverse();
    Ok(vers)
}

fn neoforge_prefix_for_mc(mc_version: &str) -> String {
    let parts: Vec<&str> = mc_version.split('.').collect();
    match parts.as_slice() {
        ["1", minor] => format!("{minor}.0."),
        ["1", minor, patch] => format!("{minor}.{patch}."),
        [yr, minor, ..] => format!("{yr}.{minor}."),
        _ => String::new(),
    }
}

#[derive(Deserialize)]
struct LoaderProfileJson {
    id: Option<String>,
    #[serde(rename = "mainClass")]
    main_class: String,
    #[serde(rename = "minecraftArguments")]
    minecraft_arguments: Option<String>,
    arguments: Option<LoaderArguments>,
    #[serde(default)]
    libraries: Vec<MojangLibrary>,
}

#[derive(Deserialize)]
struct LoaderArguments {
    #[serde(default)]
    game: Vec<Value>,
    #[serde(default)]
    jvm: Vec<Value>,
}

#[allow(clippy::too_many_arguments)]
pub async fn prepare_loader_patch<F>(
    downloader: &DownloadManager,
    paths: &AppPaths,
    loader: LoaderType,
    mc_version: &str,
    loader_version: Option<&str>,
    client_jar: &Path,
    java_bin: &Path,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<Option<LoaderLaunchPatch>>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    match loader {
        LoaderType::Vanilla => Ok(None),
        LoaderType::Fabric => {
            let ver = resolve_or_pick_latest_loader(downloader, LoaderType::Fabric, mc_version, loader_version).await?;
            let patch = setup_fabric_or_quilt(
                downloader,
                paths,
                &format!("{FABRIC_META_BASE}/versions/loader/{mc_version}/{ver}/profile/json"),
                "https://maven.fabricmc.net",
                "Fabric",
                cancel,
                on_progress,
            )
            .await?;
            Ok(Some(patch))
        }
        LoaderType::Quilt => {
            let ver = resolve_or_pick_latest_loader(downloader, LoaderType::Quilt, mc_version, loader_version).await?;
            let patch = setup_fabric_or_quilt(
                downloader,
                paths,
                &format!("{QUILT_META_BASE}/versions/loader/{mc_version}/{ver}/profile/json"),
                "https://maven.quiltmc.org/repository/release",
                "Quilt",
                cancel,
                on_progress,
            )
            .await?;
            Ok(Some(patch))
        }
        LoaderType::Forge => {
            let ver = resolve_or_pick_latest_loader(downloader, LoaderType::Forge, mc_version, loader_version).await?;
            let patch = setup_forge_or_neoforge(
                downloader,
                paths,
                false,
                mc_version,
                &ver,
                client_jar,
                java_bin,
                cancel,
                on_progress,
            )
            .await?;
            Ok(Some(patch))
        }
        LoaderType::NeoForge => {
            let ver = resolve_or_pick_latest_loader(downloader, LoaderType::NeoForge, mc_version, loader_version).await?;
            let patch = setup_forge_or_neoforge(
                downloader,
                paths,
                true,
                mc_version,
                &ver,
                client_jar,
                java_bin,
                cancel,
                on_progress,
            )
            .await?;
            Ok(Some(patch))
        }
    }
}

async fn resolve_or_pick_latest_loader(
    downloader: &DownloadManager,
    loader: LoaderType,
    mc_version: &str,
    explicit: Option<&str>,
) -> ThkResult<String> {
    if let Some(v) = explicit.filter(|s| !s.trim().is_empty()) {
        return Ok(v.trim().to_string());
    }
    let list = list_loader_versions(downloader, loader, mc_version).await?;
    list.iter()
        .find(|e| e.stable)
        .or_else(|| list.first())
        .map(|e| e.version.clone())
        .ok_or_else(|| {
            ThkError::Loader(format!(
                "No {} loader version available for Minecraft {mc_version}",
                loader.display_name()
            ))
        })
}

async fn setup_fabric_or_quilt<F>(
    downloader: &DownloadManager,
    paths: &AppPaths,
    profile_url: &str,
    default_maven: &str,
    label: &str,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<LoaderLaunchPatch>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let prof: LoaderProfileJson = downloader.get_json(profile_url).await?;
    let (lib_paths, tasks) = collect_loader_library_tasks(&prof.libraries, &paths.libraries_dir, default_maven)?;

    downloader
        .download_batch(
            &format!("{label} Libraries"),
            tasks,
            8,
            cancel,
            on_progress,
        )
        .await?;

    let ver_id = prof.id.unwrap_or_else(|| label.to_ascii_lowercase());
    let jvm_args = prof
        .arguments
        .as_ref()
        .map(|a| resolve_loader_jvm_args(&a.jvm, &paths.libraries_dir, &ver_id))
        .unwrap_or_default();
    let game_args = prof
        .arguments
        .as_ref()
        .map(|a| extract_string_args(&a.game))
        .unwrap_or_default();

    Ok(LoaderLaunchPatch {
        main_class: prof.main_class,
        extra_libraries: lib_paths,
        jvm_args,
        game_args,
        legacy_minecraft_arguments: prof.minecraft_arguments,
        include_client_jar_on_cp: true,
    })
}

#[allow(clippy::too_many_arguments)]
async fn setup_forge_or_neoforge<F>(
    downloader: &DownloadManager,
    paths: &AppPaths,
    is_neoforge: bool,
    mc_version: &str,
    raw_loader_ver: &str,
    client_jar: &Path,
    java_bin: &Path,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<LoaderLaunchPatch>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    let (installer_rel, primary_url, fallback_url, installed_version_id, jvm_version_token) = if is_neoforge {
        let is_legacy_neo = raw_loader_ver.starts_with("1.20.1-");
        let (group, file_name, ver_id) = if is_legacy_neo {
            (
                "net/neoforged/forge",
                format!("forge-{raw_loader_ver}-installer.jar"),
                format!("neoforge-{raw_loader_ver}"),
            )
        } else {
            (
                "net/neoforged/neoforge",
                format!("neoforge-{raw_loader_ver}-installer.jar"),
                format!("neoforge-{raw_loader_ver}"),
            )
        };
        let rel = format!("{group}/{raw_loader_ver}/{file_name}");
        let url = format!("{NEOFORGE_MAVEN_BASE}/{rel}");
        (rel, url, None, ver_id.clone(), ver_id)
    } else {
        let build_only = raw_loader_ver
            .strip_prefix(&format!("{mc_version}-"))
            .unwrap_or(raw_loader_ver);
        let full_ver = format!("{mc_version}-{build_only}");
        let file_name = format!("forge-{full_ver}-installer.jar");
        let rel = format!("net/minecraftforge/forge/{full_ver}/{file_name}");
        let primary = format!("{FORGE_MAVEN_BASE}/{rel}");
        let mirror = format!("{BMCLAPI_BASE}/maven/{rel}");
        let ver_id = format!("{mc_version}-forge-{build_only}");
        let jvm_tok = format!("forge-{build_only}");
        (rel, primary, Some(mirror), ver_id, jvm_tok)
    };

    let installer_path = SafePath::resolve_child(&paths.libraries_dir, &installer_rel)?;
    if !installer_path.is_file() {
        let mut t = DownloadTask::new(primary_url, installer_path.clone());
        if let Some(fb) = fallback_url {
            t.fallback_urls.push(fb);
        }
        t.label = installer_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "installer.jar".into());
        downloader
            .download_batch("Downloading Loader Installer", vec![t], 1, cancel.clone(), on_progress.clone())
            .await?;
    }

    // Check if legacy Forge (<= 1.12.2) embeds a standalone version.json or install_profile.json versionInfo
    let embedded_ver_raw = read_zip_entry_string(&installer_path, "version.json")?;
    let embedded_install_raw = read_zip_entry_string(&installer_path, "install_profile.json")?;

    if embedded_ver_raw.is_none() {
        if let Some(ref ip_str) = embedded_install_raw {
            let ip_val: Value = serde_json::from_str(ip_str)?;
            if let Some(ver_info) = ip_val.get("versionInfo") {
                let legacy_prof: LoaderProfileJson = serde_json::from_value(ver_info.clone())?;
                let (lib_paths, tasks) = collect_loader_library_tasks(
                    &legacy_prof.libraries,
                    &paths.libraries_dir,
                    FORGE_MAVEN_BASE,
                )?;
                downloader
                    .download_batch("Forge Legacy Libraries", tasks, 8, cancel, on_progress)
                    .await?;
                return Ok(LoaderLaunchPatch {
                    main_class: legacy_prof.main_class,
                    extra_libraries: lib_paths,
                    jvm_args: Vec::new(),
                    game_args: Vec::new(),
                    legacy_minecraft_arguments: legacy_prof.minecraft_arguments,
                    include_client_jar_on_cp: true,
                });
            }
        }
    }

    ensure_launcher_profile_stub(&paths.launcher_dir, mc_version).await?;
    let vanilla_ver_dir = SafePath::resolve_child(&paths.versions_dir, mc_version)?;
    tokio::fs::create_dir_all(&vanilla_ver_dir).await?;
    let expected_vanilla_jar = SafePath::resolve_child(&vanilla_ver_dir, format!("{mc_version}.jar"))?;
    if !expected_vanilla_jar.is_file() && client_jar.is_file() {
        tokio::fs::copy(client_jar, &expected_vanilla_jar).await?;
    }

    let target_ver_json = find_installed_loader_json(&paths.versions_dir, &installed_version_id, mc_version, is_neoforge, raw_loader_ver)
        .unwrap_or_else(|| {
            paths
                .versions_dir
                .join(&installed_version_id)
                .join(format!("{installed_version_id}.json"))
        });

    let needs_installer_run = !target_ver_json.is_file()
        || !verify_processor_outputs(&target_ver_json, &paths.libraries_dir, mc_version, is_neoforge, raw_loader_ver);

    if needs_installer_run {
        run_headless_installer(java_bin, &installer_path, &paths.launcher_dir, on_progress.clone()).await?;
    }

    let resolved_json_path = find_installed_loader_json(
        &paths.versions_dir,
        &installed_version_id,
        mc_version,
        is_neoforge,
        raw_loader_ver,
    )
    .ok_or_else(|| {
        ThkError::Loader(format!(
            "Loader installer completed but version JSON was not generated in {}",
            paths.versions_dir.display()
        ))
    })?;

    let raw_prof = tokio::fs::read_to_string(&resolved_json_path).await?;
    let prof: LoaderProfileJson = serde_json::from_str(&raw_prof)?;
    let default_maven = if is_neoforge {
        NEOFORGE_MAVEN_BASE
    } else {
        FORGE_MAVEN_BASE
    };

    let (lib_paths, tasks) =
        collect_loader_library_tasks(&prof.libraries, &paths.libraries_dir, default_maven)?;
    if !tasks.is_empty() {
        downloader
            .download_batch("Verifying Loader Libraries", tasks, 8, cancel, on_progress)
            .await?;
    }

    let actual_ver_id = prof.id.as_deref().unwrap_or(&jvm_version_token);
    let jvm_tok = if is_neoforge {
        actual_ver_id
    } else {
        &jvm_version_token
    };
    let jvm_args = prof
        .arguments
        .as_ref()
        .map(|a| resolve_loader_jvm_args(&a.jvm, &paths.libraries_dir, jvm_tok))
        .unwrap_or_default();
    let game_args = prof
        .arguments
        .as_ref()
        .map(|a| extract_string_args(&a.game))
        .unwrap_or_default();

    let include_client_jar_on_cp = !prof.main_class.contains("BootstrapLauncher");

    Ok(LoaderLaunchPatch {
        main_class: prof.main_class,
        extra_libraries: lib_paths,
        jvm_args,
        game_args,
        legacy_minecraft_arguments: prof.minecraft_arguments,
        include_client_jar_on_cp,
    })
}

fn find_installed_loader_json(
    versions_dir: &Path,
    expected_id: &str,
    mc_version: &str,
    is_neoforge: bool,
    raw_loader_ver: &str,
) -> Option<PathBuf> {
    let direct = versions_dir.join(expected_id).join(format!("{expected_id}.json"));
    if direct.is_file() {
        return Some(direct);
    }

    let build_only = raw_loader_ver
        .strip_prefix(&format!("{mc_version}-"))
        .unwrap_or(raw_loader_ver);

    let entries = std::fs::read_dir(versions_dir).ok()?;
    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        let matches = if is_neoforge {
            dir_name.contains("neoforge") && dir_name.contains(build_only)
        } else {
            dir_name.contains("forge") && !dir_name.contains("neoforge") && dir_name.contains(build_only)
        };
        if matches {
            let candidate = entry.path().join(format!("{dir_name}.json"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn verify_processor_outputs(
    ver_json_path: &Path,
    libraries_dir: &Path,
    mc_version: &str,
    is_neoforge: bool,
    raw_loader_ver: &str,
) -> bool {
    let Ok(raw) = std::fs::read_to_string(ver_json_path) else {
        return false;
    };
    let Ok(prof) = serde_json::from_str::<LoaderProfileJson>(&raw) else {
        return false;
    };
    let Some(args) = prof.arguments else {
        return true;
    };
    let game_strs = extract_string_args(&args.game);

    if let Some(pos) = game_strs.iter().position(|s| s == "--fml.neoFormVersion") {
        if let Some(neo_form) = game_strs.get(pos + 1) {
            let mc_client_dir = libraries_dir
                .join("net")
                .join("minecraft")
                .join("client")
                .join(format!("{mc_version}-{neo_form}"));
            let srg = mc_client_dir.join(format!("client-{mc_version}-{neo_form}-srg.jar"));
            let extra = mc_client_dir.join(format!("client-{mc_version}-{neo_form}-extra.jar"));
            if !srg.is_file() || !extra.is_file() {
                return false;
            }
        }
    } else if let Some(pos) = game_strs.iter().position(|s| s == "--fml.mcpVersion") {
        if let Some(mcp_ver) = game_strs.get(pos + 1) {
            let build_only = raw_loader_ver
                .strip_prefix(&format!("{mc_version}-"))
                .unwrap_or(raw_loader_ver);
            let full_ver = format!("{mc_version}-{build_only}");
            let client_patched = if is_neoforge {
                libraries_dir
                    .join("net/neoforged/forge")
                    .join(&full_ver)
                    .join(format!("forge-{full_ver}-client.jar"))
            } else {
                libraries_dir
                    .join("net/minecraftforge/forge")
                    .join(&full_ver)
                    .join(format!("forge-{full_ver}-client.jar"))
            };
            let srg = libraries_dir
                .join("net/minecraft/client")
                .join(format!("{mc_version}-{mcp_ver}"))
                .join(format!("client-{mc_version}-{mcp_ver}-srg.jar"));
            if !client_patched.is_file() || !srg.is_file() {
                return false;
            }
        }
    }

    true
}

async fn ensure_launcher_profile_stub(launcher_dir: &Path, mc_version: &str) -> ThkResult<()> {
    let prof_path = launcher_dir.join("launcher_profiles.json");
    if prof_path.is_file() {
        return Ok(());
    }
    let payload = json!({
        "authenticationDatabase": {},
        "clientToken": "",
        "launcherVersion": { "name": "TriHienKun Laucher", "format": 21, "profilesFormat": 2 },
        "profiles": {
            mc_version: {
                "lastVersionId": mc_version,
                "name": mc_version,
                "type": "latest-release"
            }
        },
        "selectedProfile": mc_version,
        "selectedUser": "",
        "settings": {}
    });
    tokio::fs::write(&prof_path, serde_json::to_vec_pretty(&payload)?).await?;
    Ok(())
}

async fn run_headless_installer<F>(
    java_bin: &Path,
    installer_jar: &Path,
    launcher_dir: &Path,
    on_progress: F,
) -> ThkResult<()>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let mut cmd = Command::new(java_bin);
    cmd.arg("-Djava.awt.headless=true")
        .arg("-jar")
        .arg(installer_jar)
        .arg("--installClient")
        .arg(launcher_dir)
        .current_dir(launcher_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| ThkError::Loader(format!("Failed spawning loader installer JVM: {e}")))?;

    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            on_progress(DownloadProgress {
                phase: "Loader Processors".into(),
                message: line,
                completed_files: 0,
                total_files: 1,
                downloaded_bytes: 0,
                total_bytes: 0,
                bytes_per_sec: 0,
                percent: 94.0,
            });
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| ThkError::Loader(format!("Loader installer wait failed: {e}")))?;

    if !status.success() {
        return Err(ThkError::Loader(format!(
            "Loader installer exited with non-zero status: {status}"
        )));
    }
    Ok(())
}

fn collect_loader_library_tasks(
    libs: &[MojangLibrary],
    libraries_dir: &Path,
    default_maven: &str,
) -> ThkResult<(Vec<PathBuf>, Vec<DownloadTask>)> {
    let mut paths = Vec::new();
    let mut tasks = Vec::new();
    let features = HashMap::new();
    let base_default = default_maven.trim_end_matches('/');

    for lib in libs {
        if !evaluate_rules(lib.rules.as_deref(), &features) {
            continue;
        }
        let art = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());
        let rel_opt = art
            .and_then(|a| a.path.clone())
            .or_else(|| maven_coord_to_rel_path(&lib.name));
        let Some(rel_path) = rel_opt else {
            continue;
        };

        let dest = SafePath::resolve_child(libraries_dir, &rel_path)?;
        paths.push(dest.clone());

        if dest.is_file() {
            continue;
        }

        let url = art
            .map(|a| a.url.clone())
            .filter(|u| !u.trim().is_empty())
            .or_else(|| {
                lib.url
                    .as_deref()
                    .filter(|u| !u.trim().is_empty())
                    .map(|u| format!("{}/{rel_path}", u.trim_end_matches('/')))
            })
            .unwrap_or_else(|| format!("{base_default}/{rel_path}"));

        let mut task = DownloadTask::new(url, dest);
        if default_maven == FORGE_MAVEN_BASE {
            task.fallback_urls
                .push(format!("{BMCLAPI_BASE}/maven/{rel_path}"));
            task.fallback_urls
                .push(format!("https://libraries.minecraft.net/{rel_path}"));
        }
        if let Some(a) = art {
            task.expected_sha1 = a.sha1.clone();
            task.expected_size = a.size;
        }
        task.label = lib.name.clone();
        tasks.push(task);
    }

    Ok((paths, tasks))
}

fn resolve_loader_jvm_args(raw_jvm: &[Value], libraries_dir: &Path, version_name: &str) -> Vec<String> {
    let cp_sep = if cfg!(windows) { ";" } else { ":" };
    let lib_dir_fwd = libraries_dir.to_string_lossy().replace('\\', "/");
    let features = HashMap::new();
    let mut out = Vec::new();

    let subst = |s: &str| -> String {
        s.replace("${library_directory}", &lib_dir_fwd)
            .replace("${classpath_separator}", cp_sep)
            .replace("${version_name}", version_name)
    };

    for item in raw_jvm {
        match item {
            Value::String(s) => out.push(subst(s)),
            Value::Object(obj) => {
                if let Some(rules_val) = obj.get("rules") {
                    if let Ok(rules) = serde_json::from_value::<Vec<MojangRuleLike>>(rules_val.clone()) {
                        let converted: Vec<_> = rules.into_iter().map(|r| r.into_mojang_rule()).collect();
                        if !evaluate_rules(Some(&converted), &features) {
                            continue;
                        }
                    }
                }
                if let Some(val) = obj.get("value") {
                    match val {
                        Value::String(s) => out.push(subst(s)),
                        Value::Array(arr) => {
                            for v in arr {
                                if let Some(s) = v.as_str() {
                                    out.push(subst(s));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[derive(Deserialize)]
struct MojangRuleLike {
    action: String,
    os: Option<crate::vanilla::MojangOsRule>,
    features: Option<HashMap<String, bool>>,
}

impl MojangRuleLike {
    fn into_mojang_rule(self) -> crate::vanilla::MojangRule {
        crate::vanilla::MojangRule {
            action: self.action,
            os: self.os,
            features: self.features,
        }
    }
}

fn extract_string_args(raw: &[Value]) -> Vec<String> {
    raw.iter()
        .filter_map(|v| v.as_str().map(ToString::to_string))
        .collect()
}
