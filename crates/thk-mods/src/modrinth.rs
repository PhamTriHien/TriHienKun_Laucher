use crate::provider::{
    DependencyRelation, DependencySpec, GalleryImage, ImportedPackMetadata, ProjectDetail,
    ProjectSummary, ProjectVersion, SearchPage, SearchQuery, VersionFile,
};
use chrono::Utc;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thk_core::archive::{extract_zip_prefix_safe, read_zip_entry_string};
use thk_core::hash::sha1_file;
use thk_core::models::{
    ContentKind, DownloadProgress, GameProfile, InstalledContentRecord, LoaderType, ProviderSource,
};
use thk_core::paths::SafePath;
use thk_core::{Database, DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;

pub const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v2";

pub const FABRIC_PERF_MOD_IDS: [&str; 8] = [
    "P7dR8mSH", // Fabric API
    "AANobbMI", // Sodium
    "gvQqBUqZ", // Lithium
    "uXXizFIs", // FerriteCore
    "NNAgCjsB", // EntityCulling
    "nmDcB62a", // ModernFix
    "YL57xq9U", // Iris Shaders
    "mOgUt4GM", // Mod Menu
];

#[derive(Deserialize)]
struct ModrinthSearchResp {
    hits: Vec<ModrinthHit>,
    total_hits: usize,
}

#[derive(Deserialize)]
struct ModrinthHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    downloads: u64,
    icon_url: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    date_modified: Option<String>,
    project_type: Option<String>,
}

#[derive(Deserialize)]
struct ModrinthProjectFull {
    id: String,
    slug: String,
    title: String,
    description: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    downloads: u64,
    icon_url: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    updated: Option<String>,
    project_type: Option<String>,
    source_url: Option<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    gallery: Vec<ModrinthGalleryItem>,
}

#[derive(Deserialize)]
struct ModrinthGalleryItem {
    url: String,
    title: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ModrinthVersionRaw {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    #[serde(default)]
    pub version_type: String,
    pub date_published: Option<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub files: Vec<ModrinthFileRaw>,
    #[serde(default)]
    pub dependencies: Vec<ModrinthDepRaw>,
}

#[derive(Deserialize)]
pub(crate) struct ModrinthFileRaw {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub primary: bool,
    pub hashes: Option<ModrinthHashes>,
}

#[derive(Deserialize)]
pub(crate) struct ModrinthHashes {
    pub sha1: Option<String>,
    pub sha512: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ModrinthDepRaw {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    #[serde(default)]
    pub dependency_type: String,
}

pub async fn search_modrinth(
    downloader: &DownloadManager,
    q: &SearchQuery,
) -> ThkResult<SearchPage> {
    let mut facets: Vec<Vec<String>> = Vec::new();
    let project_type = match q.content_kind {
        ContentKind::Mod => "mod",
        ContentKind::Modpack => "modpack",
        ContentKind::Shader => "shader",
        ContentKind::ResourcePack => "resourcepack",
        ContentKind::Plugin => "plugin",
    };
    facets.push(vec![format!("project_type:{project_type}")]);

    if let Some(ref gv) = q.game_version {
        if !gv.trim().is_empty() {
            facets.push(vec![format!("versions:{gv}")]);
        }
    }

    if let Some(loader) = q.loader {
        if matches!(q.content_kind, ContentKind::Mod | ContentKind::Modpack)
            && loader != LoaderType::Vanilla
        {
            facets.push(vec![format!("categories:{}", loader.as_str())]);
        }
    }

    let facets_json = serde_json::to_string(&facets)?;
    let url = format!(
        "{MODRINTH_API_BASE}/search?query={}&facets={}&index={}&offset={}&limit={}",
        urlencoding::encode(&q.query),
        urlencoding::encode(&facets_json),
        q.sort.modrinth_index(),
        q.offset,
        q.limit.clamp(1, 100),
    );

    let resp: ModrinthSearchResp = downloader.get_json(&url).await?;
    let hits = resp
        .hits
        .into_iter()
        .map(|h| ProjectSummary {
            provider: ProviderSource::Modrinth,
            project_id: h.project_id,
            slug: h.slug,
            title: h.title,
            summary: h.description,
            author: h.author,
            downloads: h.downloads,
            icon_url: h.icon_url,
            categories: h.categories,
            updated_at: h.date_modified,
            content_kind: h
                .project_type
                .as_deref()
                .map(ContentKind::from_str_lossy)
                .unwrap_or(q.content_kind),
        })
        .collect();

    Ok(SearchPage {
        hits,
        total_hits: resp.total_hits,
    })
}

pub async fn get_modrinth_project(
    downloader: &DownloadManager,
    project_id: &str,
) -> ThkResult<ProjectDetail> {
    let url = format!(
        "{MODRINTH_API_BASE}/project/{}",
        urlencoding::encode(project_id)
    );
    let p: ModrinthProjectFull = downloader.get_json(&url).await?;
    Ok(ProjectDetail {
        summary: ProjectSummary {
            provider: ProviderSource::Modrinth,
            project_id: p.id,
            slug: p.slug,
            title: p.title,
            summary: p.description,
            author: String::new(),
            downloads: p.downloads,
            icon_url: p.icon_url,
            categories: p.categories,
            updated_at: p.updated,
            content_kind: p
                .project_type
                .as_deref()
                .map(ContentKind::from_str_lossy)
                .unwrap_or(ContentKind::Mod),
        },
        body_markdown: p.body,
        gallery: p
            .gallery
            .into_iter()
            .map(|g| GalleryImage {
                url: g.url,
                title: g.title.unwrap_or_default(),
            })
            .collect(),
        source_url: p.source_url,
        game_versions: p.game_versions,
        loaders: p.loaders,
    })
}

pub async fn get_modrinth_versions(
    downloader: &DownloadManager,
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<LoaderType>,
) -> ThkResult<Vec<ProjectVersion>> {
    let mut url = format!(
        "{MODRINTH_API_BASE}/project/{}/version?",
        urlencoding::encode(project_id)
    );
    let mut query_parts = Vec::new();
    if let Some(gv) = game_version.filter(|s| !s.trim().is_empty()) {
        let gv_json = serde_json::to_string(&[gv])?;
        query_parts.push(format!("game_versions={}", urlencoding::encode(&gv_json)));
    }
    if let Some(ldr) = loader.filter(|l| *l != LoaderType::Vanilla) {
        let ldr_json = serde_json::to_string(&[ldr.as_str()])?;
        query_parts.push(format!("loaders={}", urlencoding::encode(&ldr_json)));
    }
    url.push_str(&query_parts.join("&"));

    let raw_list: Vec<ModrinthVersionRaw> = downloader.get_json(&url).await?;
    Ok(raw_list.into_iter().map(map_modrinth_version).collect())
}

pub(crate) fn map_modrinth_version(v: ModrinthVersionRaw) -> ProjectVersion {
    ProjectVersion {
        id: v.id,
        project_id: v.project_id,
        provider: ProviderSource::Modrinth,
        name: v.name,
        version_number: v.version_number,
        release_channel: v.version_type,
        published_at: v.date_published,
        game_versions: v.game_versions,
        loaders: v.loaders,
        files: v
            .files
            .into_iter()
            .map(|f| VersionFile {
                url: f.url,
                filename: f.filename,
                size: f.size,
                sha1: f.hashes.as_ref().and_then(|h| h.sha1.clone()),
                sha512: f.hashes.as_ref().and_then(|h| h.sha512.clone()),
                primary: f.primary,
            })
            .collect(),
        dependencies: v
            .dependencies
            .into_iter()
            .filter_map(|d| {
                let pid = d.project_id?;
                let rel = match d.dependency_type.as_str() {
                    "required" => DependencyRelation::Required,
                    "optional" => DependencyRelation::Optional,
                    "incompatible" => DependencyRelation::Incompatible,
                    "embedded" => DependencyRelation::Embedded,
                    _ => DependencyRelation::Optional,
                };
                Some(DependencySpec {
                    project_id: pid,
                    version_id: d.version_id,
                    relation: rel,
                })
            })
            .collect(),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn install_modrinth_content<F>(
    downloader: &DownloadManager,
    db: &Database,
    profile: &GameProfile,
    content_kind: ContentKind,
    project_summary: &ProjectSummary,
    version: &ProjectVersion,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<Vec<PathBuf>>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let dest_subdir = SafePath::resolve_child(&profile.instance_path, content_kind.instance_subdir())?;
    tokio::fs::create_dir_all(&dest_subdir).await?;

    let mut queue: Vec<(ProjectSummary, ProjectVersion, usize)> =
        vec![(project_summary.clone(), version.clone(), 0)];
    let mut visited = HashSet::new();
    let mut tasks = Vec::new();
    let mut pending_records = Vec::new();

    while let Some((proj, ver, depth)) = queue.pop() {
        if !visited.insert(proj.project_id.clone()) {
            continue;
        }

        let Some(file) = ver
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| ver.files.first())
        else {
            continue;
        };

        let clean_name = SafePath::validate_single_name(&file.filename)?;
        let dest_path = SafePath::resolve_child(&dest_subdir, clean_name)?;

        remove_previous_project_files(db, &profile.id, content_kind, &proj.project_id, &dest_subdir, clean_name)?;

        let mut task = DownloadTask::new(file.url.clone(), dest_path.clone());
        task.expected_sha1 = file.sha1.clone();
        task.expected_sha512 = file.sha512.clone();
        task.expected_size = (file.size > 0).then_some(file.size);
        task.label = proj.title.clone();
        tasks.push(task);

        pending_records.push((
            dest_path,
            InstalledContentRecord {
                profile_id: profile.id.clone(),
                content_kind,
                file_name: clean_name.to_string(),
                sha1: file.sha1.clone().unwrap_or_default(),
                provider: ProviderSource::Modrinth,
                project_id: proj.project_id.clone(),
                version_id: ver.id.clone(),
                title: proj.title.clone(),
                version_number: ver.version_number.clone(),
                icon_url: proj.icon_url.clone(),
                enabled: true,
                installed_at: Utc::now().timestamp(),
            },
        ));

        if content_kind == ContentKind::Mod && depth < 3 {
            for dep in &ver.dependencies {
                if dep.relation != DependencyRelation::Required
                    || visited.contains(&dep.project_id)
                {
                    continue;
                }
                let dep_loader = (profile.loader != LoaderType::Vanilla).then_some(profile.loader);
                let dep_ver_opt = if let Some(ref vid) = dep.version_id {
                    let url = format!("{MODRINTH_API_BASE}/version/{}", urlencoding::encode(vid));
                    downloader
                        .get_json::<ModrinthVersionRaw>(&url)
                        .await
                        .ok()
                        .map(map_modrinth_version)
                        .filter(|dv| version_matches_profile(dv, &profile.game_version, dep_loader))
                } else {
                    None
                };

                let resolved_dep_ver = match dep_ver_opt {
                    Some(v) => Some(v),
                    None => get_modrinth_versions(
                        downloader,
                        &dep.project_id,
                        Some(&profile.game_version),
                        dep_loader,
                    )
                    .await
                    .ok()
                    .and_then(|mut list| list.drain(..).next()),
                };

                if let Some(dv) = resolved_dep_ver {
                    if let Ok(dep_proj) = get_modrinth_project(downloader, &dep.project_id).await {
                        queue.push((dep_proj.summary, dv, depth + 1));
                    }
                }
            }
        }
    }

    downloader
        .download_batch(
            &format!("Installing {}", project_summary.title),
            tasks,
            8,
            cancel,
            on_progress,
        )
        .await?;

    let mut installed_paths = Vec::new();
    for (path, mut rec) in pending_records {
        if rec.sha1.is_empty() {
            if let Ok(h) = sha1_file(&path) {
                rec.sha1 = h;
            }
        }
        db.upsert_installed_content(&rec)?;
        installed_paths.push(path);
    }

    Ok(installed_paths)
}

fn version_matches_profile(
    v: &ProjectVersion,
    game_version: &str,
    loader: Option<LoaderType>,
) -> bool {
    let gv_ok = v.game_versions.is_empty() || v.game_versions.iter().any(|g| g == game_version);
    let ldr_ok = match loader {
        Some(l) => {
            v.loaders.is_empty()
                || v.loaders
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(l.as_str()))
        }
        None => true,
    };
    gv_ok && ldr_ok
}

pub(crate) fn remove_previous_project_files(
    db: &Database,
    profile_id: &str,
    kind: ContentKind,
    project_id: &str,
    dest_subdir: &Path,
    keep_filename: &str,
) -> ThkResult<()> {
    let existing = db.list_installed_content(profile_id, kind)?;
    for item in existing {
        if item.project_id == project_id && item.file_name != keep_filename {
            for candidate_name in [
                item.file_name.clone(),
                format!("{}.disabled", item.file_name),
            ] {
                if let Ok(old_path) = SafePath::resolve_child(dest_subdir, &candidate_name) {
                    if old_path.is_file() {
                        let _ = std::fs::remove_file(old_path);
                    }
                }
            }
            let _ = db.remove_installed_content(profile_id, kind, &item.file_name);
        }
    }
    Ok(())
}

pub async fn ensure_fabric_performance_mods<F>(
    downloader: &DownloadManager,
    db: &Database,
    profile: &GameProfile,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<usize>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    if profile.loader != LoaderType::Fabric || !profile.auto_perf_mods {
        return Ok(0);
    }

    let mods_dir = SafePath::resolve_child(&profile.instance_path, "mods")?;
    tokio::fs::create_dir_all(&mods_dir).await?;
    let existing = db.list_installed_content(&profile.id, ContentKind::Mod)?;
    let installed_projects: HashSet<String> =
        existing.into_iter().map(|r| r.project_id).collect();

    let mut tasks = Vec::new();
    let mut records = Vec::new();

    for project_id in FABRIC_PERF_MOD_IDS {
        if installed_projects.contains(project_id) {
            continue;
        }
        let Ok(mut versions) = get_modrinth_versions(
            downloader,
            project_id,
            Some(&profile.game_version),
            Some(LoaderType::Fabric),
        )
        .await
        else {
            continue;
        };
        let Some(ver) = versions.drain(..).next() else {
            continue;
        };
        let Some(file) = ver.files.iter().find(|f| f.primary).or_else(|| ver.files.first()) else {
            continue;
        };
        let Ok(clean_name) = SafePath::validate_single_name(&file.filename) else {
            continue;
        };
        let dest = SafePath::resolve_child(&mods_dir, clean_name)?;
        if dest.exists() {
            continue;
        }

        let title = get_modrinth_project(downloader, project_id)
            .await
            .map(|p| p.summary.title)
            .unwrap_or_else(|_| project_id.to_string());

        let mut task = DownloadTask::new(file.url.clone(), dest.clone());
        task.expected_sha1 = file.sha1.clone();
        task.expected_sha512 = file.sha512.clone();
        task.expected_size = (file.size > 0).then_some(file.size);
        task.label = title.clone();
        tasks.push(task);

        records.push((
            dest,
            InstalledContentRecord {
                profile_id: profile.id.clone(),
                content_kind: ContentKind::Mod,
                file_name: clean_name.to_string(),
                sha1: file.sha1.clone().unwrap_or_default(),
                provider: ProviderSource::Modrinth,
                project_id: project_id.to_string(),
                version_id: ver.id,
                title,
                version_number: ver.version_number,
                icon_url: None,
                enabled: true,
                installed_at: Utc::now().timestamp(),
            },
        ));
    }

    if tasks.is_empty() {
        return Ok(0);
    }

    let count = tasks.len();
    downloader
        .download_batch(
            "Installing Fabric Performance Mods",
            tasks,
            8,
            cancel,
            on_progress,
        )
        .await?;

    for (path, mut rec) in records {
        if rec.sha1.is_empty() {
            if let Ok(h) = sha1_file(&path) {
                rec.sha1 = h;
            }
        }
        let _ = db.upsert_installed_content(&rec);
    }

    Ok(count)
}

#[derive(Deserialize)]
struct MrpackIndex {
    name: Option<String>,
    #[serde(default)]
    dependencies: HashMap<String, String>,
    #[serde(default)]
    files: Vec<MrpackFileEntry>,
}

#[derive(Deserialize)]
struct MrpackFileEntry {
    path: String,
    #[serde(default)]
    downloads: Vec<String>,
    hashes: Option<ModrinthHashes>,
    #[serde(default)]
    file_size: u64,
    env: Option<MrpackEnv>,
}

#[derive(Deserialize)]
struct MrpackEnv {
    client: Option<String>,
}

pub async fn import_mrpack_archive<F>(
    downloader: &DownloadManager,
    mrpack_path: &Path,
    instance_dir: &Path,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<ImportedPackMetadata>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    tokio::fs::create_dir_all(instance_dir).await?;
    let index_raw = read_zip_entry_string(mrpack_path, "modrinth.index.json")?
        .ok_or_else(|| ThkError::InvalidInput("Missing modrinth.index.json inside .mrpack".into()))?;
    let index: MrpackIndex = serde_json::from_str(&index_raw)?;

    let game_version = index
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or_else(|| ThkError::InvalidInput("modrinth.index.json missing minecraft version".into()))?;

    let (loader, loader_version) = if let Some(v) = index.dependencies.get("fabric-loader") {
        (LoaderType::Fabric, Some(v.clone()))
    } else if let Some(v) = index.dependencies.get("quilt-loader") {
        (LoaderType::Quilt, Some(v.clone()))
    } else if let Some(v) = index.dependencies.get("neoforge") {
        (LoaderType::NeoForge, Some(v.clone()))
    } else if let Some(v) = index.dependencies.get("forge") {
        (LoaderType::Forge, Some(v.clone()))
    } else {
        (LoaderType::Vanilla, None)
    };

    let mut tasks = Vec::new();
    for entry in index.files {
        if entry
            .env
            .as_ref()
            .and_then(|e| e.client.as_deref())
            .is_some_and(|c| c == "unsupported")
        {
            continue;
        }
        let Some(primary_url) = entry.downloads.first().cloned() else {
            continue;
        };
        let dest = SafePath::resolve_child(instance_dir, &entry.path)?;
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

    let pack_name = index.name.unwrap_or_else(|| {
        mrpack_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Modrinth Pack".into())
    });

    downloader
        .download_batch(
            &format!("Importing {pack_name}"),
            tasks,
            12,
            cancel,
            on_progress,
        )
        .await?;

    let mrpack_owned = mrpack_path.to_path_buf();
    let inst_owned = instance_dir.to_path_buf();
    tokio::task::spawn_blocking(move || -> ThkResult<()> {
        extract_zip_prefix_safe(&mrpack_owned, "overrides", &inst_owned)?;
        extract_zip_prefix_safe(&mrpack_owned, "client-overrides", &inst_owned)?;
        Ok(())
    })
    .await
    .map_err(|e| ThkError::Process(format!("Override extraction failed: {e}")))??;

    Ok(ImportedPackMetadata {
        name: pack_name,
        game_version,
        loader,
        loader_version,
        icon_url: None,
        recommended_ram_mb: Some(4096),
    })
}

pub async fn install_modrinth_modpack_version<F>(
    downloader: &DownloadManager,
    version: &ProjectVersion,
    cache_dir: &Path,
    instance_dir: &Path,
    icon_url: Option<String>,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<ImportedPackMetadata>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    let file = version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| ThkError::Provider("Modrinth modpack version contains no .mrpack file".into()))?;

    tokio::fs::create_dir_all(cache_dir).await?;
    let clean_name = SafePath::validate_single_name(&file.filename)?;
    let mrpack_dest = SafePath::resolve_child(cache_dir, clean_name)?;

    let mut task = DownloadTask::new(file.url.clone(), mrpack_dest.clone());
    task.expected_sha1 = file.sha1.clone();
    task.expected_sha512 = file.sha512.clone();
    task.expected_size = (file.size > 0).then_some(file.size);
    task.label = clean_name.to_string();

    downloader
        .download_batch(
            "Downloading .mrpack archive",
            vec![task],
            1,
            cancel.clone(),
            on_progress.clone(),
        )
        .await?;

    let mut meta =
        import_mrpack_archive(downloader, &mrpack_dest, instance_dir, cancel, on_progress).await?;
    meta.icon_url = icon_url;
    let _ = tokio::fs::remove_file(&mrpack_dest).await;
    Ok(meta)
}
