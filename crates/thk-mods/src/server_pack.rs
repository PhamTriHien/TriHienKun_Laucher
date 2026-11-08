use crate::curseforge::{
    build_curseforge_cdn_fallback_url, cf_get_json, resolve_curseforge_file_download,
    resolve_curseforge_pack_entries_concurrent, CfEnvelope, CfFile,
};
use crate::ftb::populate_server_from_ftb_version_with_url;
use crate::provider::{ProjectSummary, ProjectVersion};
use crate::technic::install_technic_modpack_version;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use thk_core::archive::{
    extract_zip_cancellable, extract_zip_prefix_cancellable, flatten_single_subdir,
    read_zip_entry_string,
};
use thk_core::models::{
    ContentKind, DownloadProgress, GameProfile, LoaderType, ProviderSource, ServerEngine,
};
use thk_core::paths::SafePath;
use thk_core::{DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use zip::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerModpackFormat {
    ModrinthMrpack,
    CurseForgeManifest,
    RawServerZip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerModpackInspection {
    pub name: String,
    pub game_version: String,
    pub engine: ServerEngine,
    pub loader_version: Option<String>,
    pub format: ServerModpackFormat,
    pub file_count: usize,
    pub recommended_ram_mb: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerModpackOutcome {
    pub name: String,
    pub game_version: String,
    pub engine: ServerEngine,
    pub installed_files: usize,
    pub skipped_client_files: usize,
    pub recommended_ram_mb: Option<u32>,
}

#[derive(Deserialize)]
struct MrpackServerIndex {
    name: Option<String>,
    dependencies: HashMap<String, String>,
    #[serde(default)]
    files: Vec<MrpackServerFile>,
}

#[derive(Deserialize)]
struct MrpackServerFile {
    path: String,
    #[serde(default)]
    downloads: Vec<String>,
    hashes: Option<MrpackHashes>,
    #[serde(default)]
    #[serde(rename = "fileSize")]
    file_size: u64,
    env: Option<MrpackServerEnv>,
}

#[derive(Deserialize)]
struct MrpackHashes {
    sha1: Option<String>,
    sha512: Option<String>,
}

#[derive(Deserialize)]
struct MrpackServerEnv {
    server: Option<String>,
}

#[derive(Deserialize)]
struct CfServerPackManifest {
    name: Option<String>,
    overrides: Option<String>,
    minecraft: CfServerPackMc,
    #[serde(default)]
    files: Vec<CfServerPackFileEntry>,
}

#[derive(Deserialize)]
struct CfServerPackMc {
    version: String,
    #[serde(rename = "modLoaders", default)]
    mod_loaders: Vec<CfServerPackLoader>,
    #[serde(rename = "recommendedRam")]
    recommended_ram: Option<u32>,
}

#[derive(Deserialize)]
struct CfServerPackLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
struct CfServerPackFileEntry {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(default = "default_required")]
    required: bool,
}

fn default_required() -> bool {
    true
}

pub fn loader_to_server_engine(loader: LoaderType) -> ThkResult<ServerEngine> {
    match loader {
        LoaderType::Vanilla => Ok(ServerEngine::Vanilla),
        LoaderType::Fabric => Ok(ServerEngine::Fabric),
        LoaderType::Forge => Ok(ServerEngine::Forge),
        LoaderType::NeoForge => Ok(ServerEngine::NeoForge),
        LoaderType::Quilt => Err(ThkError::InvalidInput(
            "Quilt modpack does not have a managed server engine (supported: Fabric, Forge, NeoForge, Vanilla)".into(),
        )),
    }
}

pub fn is_mrpack_entry_server_compatible(server_env: Option<&str>, rel_path: &str) -> bool {
    if server_env.is_some_and(|s| s.eq_ignore_ascii_case("unsupported")) {
        return false;
    }
    let norm = rel_path.trim_start_matches('/').replace('\\', "/");
    let lower = norm.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "options.txt" | "optionsof.txt" | "optionsshaders.txt" | "servers.dat" | "servers.dat_old"
    ) {
        return false;
    }
    let top = norm.split('/').next().unwrap_or("");
    !matches!(
        top.to_ascii_lowercase().as_str(),
        "shaderpacks" | "resourcepacks" | "texturepacks" | "screenshots" | "saves"
    )
}

pub fn is_cf_file_client_only(game_versions: &[String]) -> bool {
    let has_client = game_versions
        .iter()
        .any(|t| t.eq_ignore_ascii_case("client"));
    let has_server = game_versions
        .iter()
        .any(|t| t.eq_ignore_ascii_case("server"));
    has_client && !has_server
}

pub fn is_client_only_mod_jar(jar_path: &Path) -> bool {
    if let Ok(Some(fabric_raw)) = read_zip_entry_string(jar_path, "fabric.mod.json") {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&fabric_raw) {
            if val
                .get("environment")
                .and_then(|v| v.as_str())
                .is_some_and(|env| env.eq_ignore_ascii_case("client"))
            {
                return true;
            }
        }
    }

    if let Ok(Some(quilt_raw)) = read_zip_entry_string(jar_path, "quilt.mod.json") {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&quilt_raw) {
            let env = val
                .pointer("/minecraft/environment")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    val.pointer("/quilt_loader/metadata/environment")
                        .and_then(|v| v.as_str())
                });
            if env.is_some_and(|e| e.eq_ignore_ascii_case("client")) {
                return true;
            }
        }
    }

    for toml_entry in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Ok(Some(toml_raw)) = read_zip_entry_string(jar_path, toml_entry) {
            if toml_root_declares_client_side_only(&toml_raw) {
                return true;
            }
        }
    }

    false
}

fn toml_root_declares_client_side_only(raw: &str) -> bool {
    for line in raw.lines() {
        let trimmed = line.split('#').next().unwrap_or("").trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') {
            break;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim() == "clientSideOnly" && v.trim().eq_ignore_ascii_case("true") {
                return true;
            }
        }
    }
    false
}

pub fn inspect_modpack_for_server(archive_path: &Path) -> ThkResult<ServerModpackInspection> {
    inspect_modpack_for_server_with_hints(archive_path, None, None, None)
}

pub fn inspect_modpack_for_server_with_hints(
    archive_path: &Path,
    fallback_title: Option<&str>,
    fallback_mc: Option<&str>,
    fallback_engine: Option<ServerEngine>,
) -> ThkResult<ServerModpackInspection> {
    let fallback_name = fallback_title
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            archive_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "Modpack Server".to_string());

    if let Some(index_raw) = read_zip_entry_string(archive_path, "modrinth.index.json")? {
        let index: MrpackServerIndex = serde_json::from_str(&index_raw)?;
        let game_version = index
            .dependencies
            .get("minecraft")
            .cloned()
            .ok_or_else(|| {
                ThkError::InvalidInput("modrinth.index.json missing minecraft version".into())
            })?;
        let (loader, loader_version) = parse_mrpack_loader(&index.dependencies);
        let engine = loader_to_server_engine(loader)?;
        let file_count = index
            .files
            .iter()
            .filter(|f| {
                is_mrpack_entry_server_compatible(
                    f.env.as_ref().and_then(|e| e.server.as_deref()),
                    &f.path,
                )
            })
            .count();
        return Ok(ServerModpackInspection {
            name: index.name.unwrap_or(fallback_name),
            game_version,
            engine,
            loader_version,
            format: ServerModpackFormat::ModrinthMrpack,
            file_count,
            recommended_ram_mb: Some(4096),
        });
    }

    if let Some(manifest_raw) = read_zip_entry_string(archive_path, "manifest.json")? {
        if let Ok(manifest) = serde_json::from_str::<CfServerPackManifest>(&manifest_raw) {
            if !manifest.minecraft.version.trim().is_empty() {
                let (loader, loader_version) = parse_cf_loader(&manifest.minecraft.mod_loaders);
                let engine = loader_to_server_engine(loader)?;
                let file_count = manifest.files.iter().filter(|f| f.required).count();
                return Ok(ServerModpackInspection {
                    name: manifest.name.unwrap_or(fallback_name),
                    game_version: manifest.minecraft.version,
                    engine,
                    loader_version,
                    format: ServerModpackFormat::CurseForgeManifest,
                    file_count,
                    recommended_ram_mb: manifest.minecraft.recommended_ram,
                });
            }
        }
    }

    inspect_raw_server_zip(archive_path, fallback_name, fallback_mc, fallback_engine)
}

fn parse_mrpack_loader(deps: &HashMap<String, String>) -> (LoaderType, Option<String>) {
    if let Some(v) = deps.get("fabric-loader") {
        (LoaderType::Fabric, Some(v.clone()))
    } else if let Some(v) = deps.get("quilt-loader") {
        (LoaderType::Quilt, Some(v.clone()))
    } else if let Some(v) = deps.get("neoforge") {
        (LoaderType::NeoForge, Some(v.clone()))
    } else if let Some(v) = deps.get("forge") {
        (LoaderType::Forge, Some(v.clone()))
    } else {
        (LoaderType::Vanilla, None)
    }
}

fn parse_cf_loader(loaders: &[CfServerPackLoader]) -> (LoaderType, Option<String>) {
    let primary = loaders
        .iter()
        .find(|l| l.primary)
        .or_else(|| loaders.first());
    match primary.map(|l| l.id.as_str()) {
        Some(s) if s.starts_with("fabric-") => (
            LoaderType::Fabric,
            Some(s.trim_start_matches("fabric-").to_string()),
        ),
        Some(s) if s.starts_with("quilt-") => (
            LoaderType::Quilt,
            Some(s.trim_start_matches("quilt-").to_string()),
        ),
        Some(s) if s.starts_with("neoforge-") => (
            LoaderType::NeoForge,
            Some(s.trim_start_matches("neoforge-").to_string()),
        ),
        Some(s) if s.starts_with("forge-") => (
            LoaderType::Forge,
            Some(s.trim_start_matches("forge-").to_string()),
        ),
        _ => (LoaderType::Forge, None),
    }
}

fn inspect_raw_server_zip(
    archive_path: &Path,
    fallback_name: String,
    fallback_mc: Option<&str>,
    fallback_engine: Option<ServerEngine>,
) -> ThkResult<ServerModpackInspection> {
    let file = File::open(archive_path)?;
    let mut zip = ZipArchive::new(file)?;
    let mut file_count = 0usize;
    let mut detected_engine: Option<ServerEngine> = None;
    let mut detected_mc: Option<String> = None;
    let mut regex_mc: Option<String> = None;
    let mut variables_entry_name: Option<String> = None;
    let mc_re = Regex::new(r"(?:^|[-_. /])(1\.\d{1,2}(?:\.\d{1,2})?)(?:[-_. /]|$)")
        .expect("valid mc version regex");

    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        file_count += 1;
        let raw_name = entry.name()?.into_owned();
        let name = raw_name.replace('\\', "/");
        let lower = name.to_ascii_lowercase();

        if lower.ends_with("variables.txt") && variables_entry_name.is_none() {
            variables_entry_name = Some(raw_name);
        }

        if lower.contains("libraries/net/neoforged/neoforge/") || lower.contains("neoforge") {
            detected_engine.get_or_insert(ServerEngine::NeoForge);
        } else if lower.contains("libraries/net/minecraftforge/forge/") {
            detected_engine.get_or_insert(ServerEngine::Forge);
            if detected_mc.is_none() {
                if let Some(after) = name.split("libraries/net/minecraftforge/forge/").nth(1) {
                    if let Some(seg) = after.split('/').next() {
                        if let Some((mc, _)) = seg.split_once('-') {
                            detected_mc = Some(mc.to_string());
                        }
                    }
                }
            }
        } else if lower.contains("fabric-server") || lower.contains(".fabric/") {
            detected_engine.get_or_insert(ServerEngine::Fabric);
        } else if lower.contains("forge") && lower.ends_with(".jar") && !lower.contains("mods/") {
            detected_engine.get_or_insert(ServerEngine::Forge);
        }

        if regex_mc.is_none() && !lower.contains("/mods/") && !lower.starts_with("mods/") {
            if let Some(caps) = mc_re.captures(&name) {
                regex_mc = Some(caps[1].to_string());
            }
        }
    }
    drop(zip);

    if let Some(var_entry) = variables_entry_name {
        if let Ok(Some(vars_raw)) = read_zip_entry_string(archive_path, &var_entry) {
            for line in vars_raw.lines() {
                let trimmed = line.trim();
                if let Some((k, v)) = trimmed.split_once('=') {
                    let val = v.trim().trim_matches('"').trim_matches('\'');
                    match k.trim() {
                        "MINECRAFT_VERSION" if !val.is_empty() => {
                            detected_mc = Some(val.to_string());
                        }
                        "MODLOADER" if !val.is_empty() => {
                            detected_engine = Some(match val.to_ascii_lowercase().as_str() {
                                "fabric" => ServerEngine::Fabric,
                                "neoforge" => ServerEngine::NeoForge,
                                "forge" => ServerEngine::Forge,
                                _ => ServerEngine::Vanilla,
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if detected_mc.is_none() {
        if let Some(mc) = fallback_mc.map(str::trim).filter(|s| !s.is_empty()) {
            detected_mc = Some(mc.to_string());
        } else if let Some(rmc) = regex_mc {
            detected_mc = Some(rmc);
        } else if let Some(caps) = mc_re.captures(&fallback_name) {
            detected_mc = Some(caps[1].to_string());
        }
    }

    Ok(ServerModpackInspection {
        name: fallback_name,
        game_version: detected_mc.unwrap_or_else(|| "1.21.1".to_string()),
        engine: detected_engine
            .or(fallback_engine)
            .unwrap_or(ServerEngine::Forge),
        loader_version: None,
        format: ServerModpackFormat::RawServerZip,
        file_count,
        recommended_ram_mb: Some(4096),
    })
}

pub fn prune_client_only_server_files(server_dir: &Path) -> ThkResult<usize> {
    let mut removed = 0usize;

    for client_dir_name in ["shaderpacks", "resourcepacks", "texturepacks"] {
        if let Ok(dir) = SafePath::resolve_child(server_dir, client_dir_name) {
            if dir.is_dir() {
                fs::remove_dir_all(&dir)?;
                removed += 1;
            }
        }
    }

    for client_file_name in [
        "options.txt",
        "optionsof.txt",
        "optionsshaders.txt",
        "servers.dat",
        "servers.dat_old",
    ] {
        if let Ok(file_path) = SafePath::resolve_child(server_dir, client_file_name) {
            if file_path.is_file() {
                fs::remove_file(&file_path)?;
                removed += 1;
            }
        }
    }

    let mods_dir = SafePath::resolve_child(server_dir, "mods")?;
    if mods_dir.is_dir() {
        for entry_res in fs::read_dir(&mods_dir)? {
            let entry = entry_res?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let is_jar = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jar"));
            if is_jar && is_client_only_mod_jar(&path) {
                fs::remove_file(&path)?;
                removed += 1;
            }
        }
    }

    Ok(removed)
}

#[allow(clippy::too_many_arguments)]
pub async fn populate_server_from_modpack_archive<F>(
    downloader: &DownloadManager,
    archive_path: &Path,
    server_dir: &Path,
    curseforge_api_key: Option<&str>,
    max_concurrent: usize,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<ServerModpackOutcome>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    populate_server_from_modpack_archive_with_hints(
        downloader,
        archive_path,
        server_dir,
        curseforge_api_key,
        max_concurrent,
        None,
        None,
        None,
        cancel,
        on_progress,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn populate_server_from_modpack_archive_with_hints<F>(
    downloader: &DownloadManager,
    archive_path: &Path,
    server_dir: &Path,
    curseforge_api_key: Option<&str>,
    max_concurrent: usize,
    fallback_title: Option<&str>,
    fallback_mc: Option<&str>,
    fallback_engine: Option<ServerEngine>,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<ServerModpackOutcome>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }
    let inspection = inspect_modpack_for_server_with_hints(
        archive_path,
        fallback_title,
        fallback_mc,
        fallback_engine,
    )?;
    tokio::fs::create_dir_all(server_dir).await?;
    let concurrency = max_concurrent.clamp(1, 24);

    match inspection.format {
        ServerModpackFormat::ModrinthMrpack => {
            let index_raw = read_zip_entry_string(archive_path, "modrinth.index.json")?
                .ok_or_else(|| {
                    ThkError::InvalidInput("Missing modrinth.index.json inside .mrpack".into())
                })?;
            let index: MrpackServerIndex = serde_json::from_str(&index_raw)?;
            let mut tasks = Vec::new();
            let mut skipped_client = 0usize;

            for entry in index.files {
                let server_env = entry.env.as_ref().and_then(|e| e.server.as_deref());
                if !is_mrpack_entry_server_compatible(server_env, &entry.path) {
                    skipped_client += 1;
                    continue;
                }
                let primary_url = entry.downloads.first().cloned().ok_or_else(|| {
                    ThkError::InvalidInput(format!(
                        "No download URL for server modpack file '{}'",
                        entry.path
                    ))
                })?;
                let dest = SafePath::resolve_child(server_dir, &entry.path)?;
                let mut task = DownloadTask::new(primary_url, dest);
                if entry.downloads.len() > 1 {
                    task.fallback_urls = entry.downloads[1..].to_vec();
                }
                if let Some(h) = entry.hashes {
                    task.expected_sha1 = h.sha1;
                    task.expected_sha512 = h.sha512;
                }
                if entry.file_size > 0 {
                    task.expected_size = Some(entry.file_size);
                }
                task.label = entry.path;
                tasks.push(task);
            }

            let downloaded_count = tasks.len();
            downloader
                .download_batch(
                    &format!("Downloading server mods for {}", inspection.name),
                    tasks,
                    concurrency,
                    cancel.clone(),
                    on_progress,
                )
                .await?;

            let archive_owned = archive_path.to_path_buf();
            let dest_owned = server_dir.to_path_buf();
            let extract_cancel = cancel.clone();
            let pruned = tokio::task::spawn_blocking(move || -> ThkResult<usize> {
                extract_zip_prefix_cancellable(
                    &archive_owned,
                    "overrides",
                    &dest_owned,
                    extract_cancel.as_ref(),
                )?;
                extract_zip_prefix_cancellable(
                    &archive_owned,
                    "server-overrides",
                    &dest_owned,
                    extract_cancel.as_ref(),
                )?;
                prune_client_only_server_files(&dest_owned)
            })
            .await
            .map_err(|e| ThkError::Process(format!("Server override extraction failed: {e}")))??;

            Ok(ServerModpackOutcome {
                name: inspection.name,
                game_version: inspection.game_version,
                engine: inspection.engine,
                installed_files: downloaded_count,
                skipped_client_files: skipped_client + pruned,
                recommended_ram_mb: inspection.recommended_ram_mb,
            })
        }
        ServerModpackFormat::CurseForgeManifest => {
            let manifest_raw =
                read_zip_entry_string(archive_path, "manifest.json")?.ok_or_else(|| {
                    ThkError::InvalidInput("Missing manifest.json inside CurseForge ZIP".into())
                })?;
            let manifest: CfServerPackManifest = serde_json::from_str(&manifest_raw)?;
            let mods_dir = SafePath::resolve_child(server_dir, "mods")?;
            tokio::fs::create_dir_all(&mods_dir).await?;

            let req_pairs: Vec<(u64, u64)> = manifest
                .files
                .into_iter()
                .filter(|f| f.required)
                .map(|f| (f.project_id, f.file_id))
                .collect();

            let resolved_files = resolve_curseforge_pack_entries_concurrent(
                downloader,
                &req_pairs,
                curseforge_api_key,
                "Resolving Server Modpack",
                cancel.as_ref(),
                &on_progress,
            )
            .await?;

            let mut tasks = Vec::with_capacity(resolved_files.len());
            let mut skipped_client = 0usize;

            for meta in resolved_files {
                if is_cf_file_client_only(&meta.game_versions) {
                    skipped_client += 1;
                    continue;
                }

                let clean_name = SafePath::validate_single_name(&meta.file_name)?;
                let dest = SafePath::resolve_child(&mods_dir, clean_name)?;
                let mut task = DownloadTask::new(meta.url, dest);
                task.expected_sha1 = meta.sha1;
                if meta.file_length > 0 {
                    task.expected_size = Some(meta.file_length);
                }
                task.label = clean_name.to_string();
                tasks.push(task);
            }

            let downloaded_count = tasks.len();
            downloader
                .download_batch(
                    &format!("Downloading server mods for {}", inspection.name),
                    tasks,
                    concurrency.min(16),
                    cancel.clone(),
                    on_progress.clone(),
                )
                .await?;

            on_progress(DownloadProgress {
                phase: "Extracting Server Overrides".into(),
                message: "Extracting overrides & filtering client-only mods...".into(),
                completed_files: 0,
                total_files: 1,
                downloaded_bytes: 0,
                total_bytes: 0,
                bytes_per_sec: 0,
                percent: 96.0,
            });

            let overrides_folder = manifest.overrides.unwrap_or_else(|| "overrides".into());
            let archive_owned = archive_path.to_path_buf();
            let dest_owned = server_dir.to_path_buf();
            let extract_cancel = cancel.clone();
            let pruned = tokio::task::spawn_blocking(move || -> ThkResult<usize> {
                extract_zip_prefix_cancellable(
                    &archive_owned,
                    &overrides_folder,
                    &dest_owned,
                    extract_cancel.as_ref(),
                )?;
                extract_zip_prefix_cancellable(
                    &archive_owned,
                    "server-overrides",
                    &dest_owned,
                    extract_cancel.as_ref(),
                )?;
                prune_client_only_server_files(&dest_owned)
            })
            .await
            .map_err(|e| {
                ThkError::Process(format!("CurseForge override extraction failed: {e}"))
            })??;

            Ok(ServerModpackOutcome {
                name: inspection.name,
                game_version: inspection.game_version,
                engine: inspection.engine,
                installed_files: downloaded_count,
                skipped_client_files: skipped_client + pruned,
                recommended_ram_mb: inspection.recommended_ram_mb,
            })
        }
        ServerModpackFormat::RawServerZip => {
            on_progress(DownloadProgress {
                phase: "extracting-server-pack".into(),
                message: format!("Extracting {}...", inspection.name),
                completed_files: 0,
                total_files: inspection.file_count.max(1),
                downloaded_bytes: 0,
                total_bytes: 0,
                bytes_per_sec: 0,
                percent: 20.0,
            });

            let archive_owned = archive_path.to_path_buf();
            let dest_owned = server_dir.to_path_buf();
            let extract_cancel = cancel.clone();
            let (extracted_count, pruned) =
                tokio::task::spawn_blocking(move || -> ThkResult<(usize, usize)> {
                    let extracted = extract_zip_cancellable(
                        &archive_owned,
                        &dest_owned,
                        extract_cancel.as_ref(),
                    )?;
                    flatten_single_subdir(&dest_owned)?;
                    let pruned = prune_client_only_server_files(&dest_owned)?;
                    Ok((extracted.len(), pruned))
                })
                .await
                .map_err(|e| ThkError::Process(format!("Server ZIP extraction failed: {e}")))??;

            Ok(ServerModpackOutcome {
                name: inspection.name,
                game_version: inspection.game_version,
                engine: inspection.engine,
                installed_files: extracted_count.saturating_sub(pruned),
                skipped_client_files: pruned,
                recommended_ram_mb: inspection.recommended_ram_mb,
            })
        }
    }
}

pub fn populate_server_from_profile(
    profile: &GameProfile,
    server_dir: &Path,
    cancel: Option<&CancellationToken>,
) -> ThkResult<(ServerEngine, usize, usize)> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }
    let engine = loader_to_server_engine(profile.loader)?;
    fs::create_dir_all(server_dir)?;

    let mut copied_files = 0usize;
    let mut skipped_client = 0usize;

    for folder in ["config", "defaultconfigs", "kubejs", "scripts", "datapacks"] {
        let src_sub = SafePath::resolve_child(&profile.instance_path, folder)?;
        if !src_sub.is_dir() {
            continue;
        }
        let dst_sub = SafePath::resolve_child(server_dir, folder)?;
        copied_files += copy_tree_cancellable(&src_sub, &dst_sub, cancel)?;
    }

    let src_mods = SafePath::resolve_child(&profile.instance_path, "mods")?;
    if src_mods.is_dir() {
        let dst_mods = SafePath::resolve_child(server_dir, "mods")?;
        fs::create_dir_all(&dst_mods)?;
        for entry_res in fs::read_dir(&src_mods)? {
            if cancel.is_some_and(CancellationToken::is_cancelled) {
                return Err(ThkError::Cancelled);
            }
            let entry = entry_res?;
            let meta = fs::symlink_metadata(entry.path())?;
            if meta.file_type().is_symlink() || !meta.is_file() {
                continue;
            }
            let fname = entry.file_name().to_string_lossy().to_string();
            if !fname.to_ascii_lowercase().ends_with(".jar") {
                continue;
            }
            let src_path = entry.path();
            if is_client_only_mod_jar(&src_path) {
                skipped_client += 1;
                continue;
            }
            let clean_name = SafePath::validate_single_name(&fname)?;
            let dst_path = SafePath::resolve_child(&dst_mods, clean_name)?;
            fs::copy(&src_path, &dst_path)?;
            copied_files += 1;
        }
    }

    Ok((engine, copied_files, skipped_client))
}

fn copy_tree_cancellable(
    src_dir: &Path,
    dst_dir: &Path,
    cancel: Option<&CancellationToken>,
) -> ThkResult<usize> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }
    fs::create_dir_all(dst_dir)?;
    let mut copied = 0usize;

    for entry_res in fs::read_dir(src_dir)? {
        if cancel.is_some_and(CancellationToken::is_cancelled) {
            return Err(ThkError::Cancelled);
        }
        let entry = entry_res?;
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.file_type().is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(".thk-") || name.ends_with(".part") {
            continue;
        }
        let clean_name = SafePath::validate_single_name(&name)?;
        let dst_child = SafePath::resolve_child(dst_dir, clean_name)?;
        if meta.is_dir() {
            copied += copy_tree_cancellable(&entry.path(), &dst_child, cancel)?;
        } else if meta.is_file() {
            fs::copy(entry.path(), dst_child)?;
            copied += 1;
        }
    }

    Ok(copied)
}

struct TempArchiveCleanup(PathBuf);

impl Drop for TempArchiveCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn populate_server_from_cloud_modpack<F>(
    downloader: &DownloadManager,
    summary: &ProjectSummary,
    version: &ProjectVersion,
    cache_dir: &Path,
    server_dir: &Path,
    curseforge_api_key: Option<&str>,
    max_concurrent: usize,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<ServerModpackOutcome>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }

    if !version.loaders.is_empty()
        && version
            .loaders
            .iter()
            .all(|l| LoaderType::from_str_lossy(l) == LoaderType::Quilt)
    {
        return Err(ThkError::InvalidInput(
            "Quilt modpack does not have a managed server engine (supported: Fabric, Forge, NeoForge, Vanilla)".into(),
        ));
    }

    let hint_mc = version.game_versions.first().map(String::as_str);
    let hint_engine = version
        .loaders
        .iter()
        .find_map(|l| loader_to_server_engine(LoaderType::from_str_lossy(l)).ok());

    match summary.provider {
        ProviderSource::Modrinth => {
            let file = version
                .files
                .iter()
                .find(|f| f.primary)
                .or_else(|| version.files.first())
                .ok_or_else(|| {
                    ThkError::Provider(format!(
                        "Modrinth version '{}' has no downloadable .mrpack file",
                        version.name
                    ))
                })?;

            tokio::fs::create_dir_all(cache_dir).await?;
            let clean_name = SafePath::validate_single_name(&file.filename)?;
            let tmp_archive = SafePath::resolve_child(
                cache_dir,
                format!("srv-mrpack-{}-{clean_name}", Uuid::new_v4()),
            )?;
            let _archive_guard = TempArchiveCleanup(tmp_archive.clone());

            let mut task = DownloadTask::new(file.url.clone(), tmp_archive.clone());
            task.force_overwrite = true;
            task.expected_sha1 = file.sha1.clone();
            task.expected_sha512 = file.sha512.clone();
            if file.size > 0 {
                task.expected_size = Some(file.size);
            }
            task.label = clean_name.to_string();

            downloader
                .download_batch(
                    &format!("Downloading {}", summary.title),
                    vec![task],
                    1,
                    cancel.clone(),
                    on_progress.clone(),
                )
                .await?;

            populate_server_from_modpack_archive_with_hints(
                downloader,
                &tmp_archive,
                server_dir,
                curseforge_api_key,
                max_concurrent,
                Some(&summary.title),
                hint_mc,
                hint_engine,
                cancel,
                on_progress,
            )
            .await
        }
        ProviderSource::CurseForge => {
            tokio::fs::create_dir_all(cache_dir).await?;
            let mod_id = version
                .project_id
                .parse::<u64>()
                .or_else(|_| summary.project_id.parse::<u64>())
                .unwrap_or(0);

            if let Some(srv_fid) = version
                .server_pack_file_id
                .filter(|&id| id > 0 && mod_id > 0)
            {
                let srv_dl_res: ThkResult<(String, String, Option<String>, u64)> = async {
                    let endpoint = format!("/mods/{mod_id}/files/{srv_fid}");
                    let meta_res: ThkResult<CfEnvelope<CfFile>> = tokio::select! {
                        biased;
                        _ = async {
                            match cancel.as_ref() {
                                Some(token) => token.cancelled().await,
                                None => std::future::pending::<()>().await,
                            }
                        } => return Err(ThkError::Cancelled),
                        res = cf_get_json(downloader, &endpoint, curseforge_api_key) => res,
                    };

                    match meta_res {
                        Ok(env) => {
                            let sha1 = env
                                .data
                                .hashes
                                .iter()
                                .find(|h| h.algo == 1)
                                .map(|h| h.value.clone());
                            let url = env.data.download_url.unwrap_or_else(|| {
                                build_curseforge_cdn_fallback_url(env.data.id, &env.data.file_name)
                            });
                            Ok((url, env.data.file_name, sha1, env.data.file_length))
                        }
                        Err(_) => {
                            let (url, fname, sha1) = tokio::select! {
                                biased;
                                _ = async {
                                    match cancel.as_ref() {
                                        Some(token) => token.cancelled().await,
                                        None => std::future::pending::<()>().await,
                                    }
                                } => return Err(ThkError::Cancelled),
                                res = resolve_curseforge_file_download(
                                    downloader,
                                    mod_id,
                                    srv_fid,
                                    curseforge_api_key,
                                ) => res?,
                            };
                            Ok((url, fname, sha1, 0))
                        }
                    }
                }
                .await;

                match srv_dl_res {
                    Err(ThkError::Cancelled) => return Err(ThkError::Cancelled),
                    Ok((srv_url, srv_fname, srv_sha1, srv_size)) => {
                        if let Ok(clean_srv_name) = SafePath::validate_single_name(&srv_fname) {
                            let tmp_srv = SafePath::resolve_child(
                                cache_dir,
                                format!("srv-cfpack-{}-{clean_srv_name}", Uuid::new_v4()),
                            )?;
                            let _srv_guard = TempArchiveCleanup(tmp_srv.clone());
                            let mut task = DownloadTask::new(srv_url, tmp_srv.clone());
                            task.force_overwrite = true;
                            task.expected_sha1 = srv_sha1;
                            if srv_size > 0 {
                                task.expected_size = Some(srv_size);
                            }
                            task.label = clean_srv_name.to_string();

                            let dl_ok = downloader
                                .download_batch(
                                    &format!("Downloading Server Pack for {}", summary.title),
                                    vec![task],
                                    1,
                                    cancel.clone(),
                                    on_progress.clone(),
                                )
                                .await;

                            match dl_ok {
                                Err(ThkError::Cancelled) => return Err(ThkError::Cancelled),
                                Ok(())
                                    if inspect_modpack_for_server_with_hints(
                                        &tmp_srv,
                                        Some(&summary.title),
                                        hint_mc,
                                        hint_engine,
                                    )
                                    .is_ok() =>
                                {
                                    return populate_server_from_modpack_archive_with_hints(
                                        downloader,
                                        &tmp_srv,
                                        server_dir,
                                        curseforge_api_key,
                                        max_concurrent,
                                        Some(&summary.title),
                                        hint_mc,
                                        hint_engine,
                                        cancel,
                                        on_progress,
                                    )
                                    .await;
                                }
                                Ok(()) | Err(_) => {
                                    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled)
                                    {
                                        return Err(ThkError::Cancelled);
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => {
                        if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
                            return Err(ThkError::Cancelled);
                        }
                    }
                }
            }

            let file = version
                .files
                .iter()
                .find(|f| f.primary)
                .or_else(|| version.files.first())
                .ok_or_else(|| {
                    ThkError::Provider(format!(
                        "CurseForge version '{}' has no downloadable archive",
                        version.name
                    ))
                })?;

            let clean_name = SafePath::validate_single_name(&file.filename)?;
            let tmp_archive = SafePath::resolve_child(
                cache_dir,
                format!("srv-cfclient-{}-{clean_name}", Uuid::new_v4()),
            )?;
            let _archive_guard = TempArchiveCleanup(tmp_archive.clone());

            let mut task = DownloadTask::new(file.url.clone(), tmp_archive.clone());
            task.force_overwrite = true;
            task.expected_sha1 = file.sha1.clone();
            if file.size > 0 {
                task.expected_size = Some(file.size);
            }
            task.label = clean_name.to_string();

            downloader
                .download_batch(
                    &format!("Downloading {}", summary.title),
                    vec![task],
                    1,
                    cancel.clone(),
                    on_progress.clone(),
                )
                .await?;

            populate_server_from_modpack_archive_with_hints(
                downloader,
                &tmp_archive,
                server_dir,
                curseforge_api_key,
                max_concurrent,
                Some(&summary.title),
                hint_mc,
                hint_engine,
                cancel,
                on_progress,
            )
            .await
        }
        ProviderSource::Ftb => {
            let manifest_url = version.files.first().map(|f| f.url.as_str());
            populate_server_from_ftb_version_with_url(
                downloader,
                &summary.project_id,
                &version.id,
                &summary.title,
                manifest_url,
                server_dir,
                curseforge_api_key,
                max_concurrent,
                cancel,
                on_progress,
            )
            .await
        }
        ProviderSource::Technic => {
            let slug = if summary.slug.trim().is_empty() {
                summary.project_id.as_str()
            } else {
                summary.slug.as_str()
            };
            let meta = install_technic_modpack_version(
                downloader,
                slug,
                &version.id,
                cache_dir,
                server_dir,
                cancel.clone(),
                on_progress,
            )
            .await?;

            if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err(ThkError::Cancelled);
            }

            let engine = loader_to_server_engine(meta.loader)?;
            let srv_owned = server_dir.to_path_buf();
            let pruned = tokio::task::spawn_blocking(move || -> ThkResult<usize> {
                let mut removed_extra = 0usize;
                if let Ok(bin_dir) = SafePath::resolve_child(&srv_owned, "bin") {
                    if bin_dir.is_dir() {
                        fs::remove_dir_all(&bin_dir)?;
                        removed_extra += 1;
                    }
                }
                let pruned_client = prune_client_only_server_files(&srv_owned)?;
                Ok(removed_extra + pruned_client)
            })
            .await
            .map_err(|e| ThkError::Process(format!("Technic server cleanup failed: {e}")))??;

            let server_mod_count = meta
                .installed_content
                .iter()
                .filter(|r| r.content_kind == ContentKind::Mod)
                .count();

            Ok(ServerModpackOutcome {
                name: meta.name,
                game_version: meta.game_version,
                engine,
                installed_files: server_mod_count.saturating_sub(pruned.min(server_mod_count)),
                skipped_client_files: pruned,
                recommended_ram_mb: meta.recommended_ram_mb,
            })
        }
        ProviderSource::Spiget | ProviderSource::Local => Err(ThkError::InvalidInput(format!(
            "Provider {} does not host cloud modpacks for server creation",
            summary.provider.display_name()
        ))),
    }
}
