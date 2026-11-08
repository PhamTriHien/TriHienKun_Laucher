use crate::matcher::split_filename_title_and_version;
use crate::provider::{
    extract_canonical_version_number, DependencyRelation, DependencySpec, GalleryImage,
    ImportedPackMetadata, ProjectDetail, ProjectSummary, ProjectVersion, SearchPage, SearchQuery,
    VersionFile,
};
use chrono::Utc;
use futures_util::stream::{self, StreamExt};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thk_core::archive::{extract_zip_prefix_cancellable, read_zip_entry_string};
use thk_core::hash::sha1_file;
use thk_core::models::{
    ContentKind, DownloadProgress, GameProfile, InstalledContentRecord, LoaderType, ProviderSource,
};
use thk_core::paths::SafePath;
use thk_core::{Database, DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const CF_OFFICIAL_BASE: &str = "https://api.curseforge.com/v1";
pub const CF_PROXY_BASE: &str = "https://api.curse.tools/v1/cf";

pub(crate) async fn cf_get_json<T: DeserializeOwned>(
    downloader: &DownloadManager,
    endpoint: &str,
    api_key: Option<&str>,
) -> ThkResult<T> {
    if let Some(key) = api_key.map(str::trim).filter(|k| !k.is_empty()) {
        let url = format!("{CF_OFFICIAL_BASE}{endpoint}");
        let resp = downloader
            .client()
            .get(&url)
            .header("Accept", "application/json")
            .header("x-api-key", key)
            .send()
            .await?;
        let status = resp.status();
        if status.is_success() {
            return resp.json::<T>().await.map_err(ThkError::from);
        }
        if matches!(status.as_u16(), 401 | 403) {
            return Err(ThkError::Provider(format!(
                "CurseForge API key rejected (HTTP {}). Check or clear your CurseForge API key in Settings.",
                status.as_u16()
            )));
        }
    }
    let proxy_url = format!("{CF_PROXY_BASE}{endpoint}");
    downloader.get_json(&proxy_url).await
}

fn cf_class_id(kind: ContentKind) -> u32 {
    match kind {
        ContentKind::Mod => 6,
        ContentKind::Modpack => 4471,
        ContentKind::Shader => 6552,
        ContentKind::ResourcePack => 12,
        ContentKind::Plugin => 5,
    }
}

fn cf_loader_id(loader: LoaderType) -> u32 {
    match loader {
        LoaderType::Vanilla => 0,
        LoaderType::Forge => 1,
        LoaderType::Fabric => 4,
        LoaderType::Quilt => 5,
        LoaderType::NeoForge => 6,
    }
}

pub fn build_curseforge_cdn_fallback_url(file_id: u64, file_name: &str) -> String {
    let major = file_id / 1000;
    let minor = file_id % 1000;
    format!(
        "https://edge.forgecdn.net/files/{major}/{minor}/{}",
        urlencoding::encode(file_name)
    )
}

#[derive(Deserialize)]
pub(crate) struct CfEnvelope<T> {
    pub(crate) data: T,
    pub(crate) pagination: Option<CfPagination>,
}

#[derive(Deserialize)]
pub(crate) struct CfPagination {
    #[serde(rename = "totalCount")]
    pub(crate) total_count: usize,
}

#[derive(Deserialize)]
struct CfMod {
    id: u64,
    slug: String,
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(rename = "downloadCount", default)]
    download_count: f64,
    #[serde(default)]
    authors: Vec<CfAuthor>,
    logo: Option<CfAsset>,
    #[serde(default)]
    screenshots: Vec<CfAsset>,
    #[serde(default)]
    categories: Vec<CfCategory>,
    #[serde(rename = "dateModified")]
    date_modified: Option<String>,
    #[serde(rename = "classId")]
    class_id: Option<u32>,
    links: Option<CfLinks>,
}

#[derive(Deserialize)]
struct CfAuthor {
    name: String,
}

#[derive(Deserialize)]
struct CfAsset {
    #[serde(rename = "thumbnailUrl")]
    thumbnail_url: Option<String>,
    url: Option<String>,
    title: Option<String>,
}

#[derive(Deserialize)]
struct CfCategory {
    name: String,
}

#[derive(Deserialize)]
struct CfLinks {
    #[serde(rename = "sourceUrl")]
    source_url: Option<String>,
    #[serde(rename = "websiteUrl")]
    website_url: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CfFile {
    pub(crate) id: u64,
    #[serde(rename = "modId")]
    pub(crate) mod_id: u64,
    #[serde(rename = "displayName")]
    pub(crate) display_name: String,
    #[serde(rename = "fileName")]
    pub(crate) file_name: String,
    #[serde(rename = "releaseType", default)]
    pub(crate) release_type: u32,
    #[serde(rename = "isServerPack")]
    pub(crate) is_server_pack: Option<bool>,
    #[serde(rename = "serverPackFileId")]
    pub(crate) server_pack_file_id: Option<u64>,
    #[serde(rename = "fileDate")]
    pub(crate) file_date: Option<String>,
    #[serde(rename = "fileLength", default)]
    pub(crate) file_length: u64,
    #[serde(rename = "fileFingerprint", default)]
    pub(crate) file_fingerprint: u64,
    #[serde(rename = "downloadUrl")]
    pub(crate) download_url: Option<String>,
    #[serde(rename = "gameVersions", default)]
    pub(crate) game_versions: Vec<String>,
    #[serde(default)]
    pub(crate) hashes: Vec<CfHash>,
    #[serde(default)]
    pub(crate) dependencies: Vec<CfDependency>,
}

#[derive(Deserialize)]
pub(crate) struct CfHash {
    pub(crate) value: String,
    pub(crate) algo: u32,
}

#[derive(Deserialize)]
pub(crate) struct CfDependency {
    #[serde(rename = "modId")]
    pub(crate) mod_id: u64,
    #[serde(rename = "relationType")]
    pub(crate) relation_type: u32,
}

pub async fn search_curseforge(
    downloader: &DownloadManager,
    q: &SearchQuery,
    api_key: Option<&str>,
) -> ThkResult<SearchPage> {
    let mut endpoint = format!(
        "/mods/search?gameId=432&classId={}&sortField={}&sortOrder=desc&index={}&pageSize={}",
        cf_class_id(q.content_kind),
        q.sort.curseforge_field(),
        q.offset,
        q.limit.clamp(1, 50),
    );

    if !q.query.trim().is_empty() {
        endpoint.push_str(&format!("&searchFilter={}", urlencoding::encode(&q.query)));
    }
    if let Some(ref gv) = q.game_version {
        if !gv.trim().is_empty() {
            endpoint.push_str(&format!("&gameVersion={}", urlencoding::encode(gv)));
        }
    }
    if let Some(ldr) = q.loader {
        let lid = cf_loader_id(ldr);
        if lid > 0 && matches!(q.content_kind, ContentKind::Mod | ContentKind::Modpack) {
            endpoint.push_str(&format!("&modLoaderType={lid}"));
        }
    }

    let env: CfEnvelope<Vec<CfMod>> = cf_get_json(downloader, &endpoint, api_key).await?;
    let total_hits = env
        .pagination
        .map(|p| p.total_count)
        .unwrap_or(env.data.len());
    let hits = env
        .data
        .into_iter()
        .map(|m| map_cf_summary(m, q.content_kind))
        .collect();

    Ok(SearchPage { hits, total_hits })
}

fn map_cf_summary(m: CfMod, fallback_kind: ContentKind) -> ProjectSummary {
    let kind = match m.class_id {
        Some(6) => ContentKind::Mod,
        Some(4471) => ContentKind::Modpack,
        Some(6552) => ContentKind::Shader,
        Some(12) => ContentKind::ResourcePack,
        _ => fallback_kind,
    };
    ProjectSummary {
        provider: ProviderSource::CurseForge,
        project_id: m.id.to_string(),
        slug: m.slug,
        title: m.name,
        summary: m.summary,
        author: m
            .authors
            .into_iter()
            .map(|a| a.name)
            .collect::<Vec<_>>()
            .join(", "),
        downloads: m.download_count as u64,
        icon_url: m.logo.and_then(|l| l.thumbnail_url.or(l.url)),
        categories: m.categories.into_iter().map(|c| c.name).collect(),
        updated_at: m.date_modified,
        content_kind: kind,
    }
}

pub async fn get_curseforge_project(
    downloader: &DownloadManager,
    project_id: &str,
    api_key: Option<&str>,
) -> ThkResult<ProjectDetail> {
    let env: CfEnvelope<CfMod> = cf_get_json(
        downloader,
        &format!("/mods/{}", urlencoding::encode(project_id)),
        api_key,
    )
    .await?;

    let desc_html = cf_get_json::<CfEnvelope<String>>(
        downloader,
        &format!("/mods/{}/description", urlencoding::encode(project_id)),
        api_key,
    )
    .await
    .map(|d| strip_basic_html_to_markdown(&d.data))
    .unwrap_or_else(|_| env.data.summary.clone());

    let gallery = env
        .data
        .screenshots
        .iter()
        .filter_map(|s| {
            let url = s.url.clone().or_else(|| s.thumbnail_url.clone())?;
            Some(GalleryImage {
                url,
                title: s.title.clone().unwrap_or_default(),
            })
        })
        .collect();

    let source_url = env
        .data
        .links
        .as_ref()
        .and_then(|l| l.source_url.clone().or_else(|| l.website_url.clone()));

    Ok(ProjectDetail {
        summary: map_cf_summary(env.data, ContentKind::Mod),
        body_markdown: desc_html,
        gallery,
        source_url,
        game_versions: Vec::new(),
        loaders: Vec::new(),
    })
}

fn strip_basic_html_to_markdown(html: &str) -> String {
    let with_breaks = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n\n")
        .replace("</li>", "\n")
        .replace("<li>", "- ");
    let mut out = String::with_capacity(with_breaks.len());
    let mut in_tag = false;
    for ch in with_breaks.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .trim()
        .to_string()
}

pub async fn get_curseforge_versions(
    downloader: &DownloadManager,
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<LoaderType>,
    api_key: Option<&str>,
) -> ThkResult<Vec<ProjectVersion>> {
    let mut endpoint = format!(
        "/mods/{}/files?pageSize=50",
        urlencoding::encode(project_id)
    );
    if let Some(gv) = game_version.filter(|s| !s.trim().is_empty()) {
        endpoint.push_str(&format!("&gameVersion={}", urlencoding::encode(gv)));
    }
    if let Some(ldr) = loader.filter(|l| *l != LoaderType::Vanilla) {
        let lid = cf_loader_id(ldr);
        if lid > 0 {
            endpoint.push_str(&format!("&modLoaderType={lid}"));
        }
    }

    let env: CfEnvelope<Vec<CfFile>> = cf_get_json(downloader, &endpoint, api_key).await?;
    Ok(env
        .data
        .into_iter()
        .filter(|f| f.is_server_pack != Some(true))
        .map(map_cf_file_to_version)
        .collect())
}

pub(crate) fn map_cf_file_to_version(f: CfFile) -> ProjectVersion {
    let sha1 = f
        .hashes
        .iter()
        .find(|h| h.algo == 1)
        .map(|h| h.value.clone());
    let url = f
        .download_url
        .unwrap_or_else(|| build_curseforge_cdn_fallback_url(f.id, &f.file_name));

    let mut gvs = Vec::new();
    let mut ldrs = Vec::new();
    for tag in f.game_versions {
        let lower = tag.to_ascii_lowercase();
        if matches!(lower.as_str(), "forge" | "fabric" | "quilt" | "neoforge") {
            ldrs.push(lower);
        } else if tag.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            gvs.push(tag);
        }
    }

    let release_channel = match f.release_type {
        1 => "release",
        2 => "beta",
        _ => "alpha",
    }
    .to_string();

    let server_pack_file_id = f.server_pack_file_id.filter(|&id| id > 0);
    let version_number = extract_canonical_version_number(&f.display_name, &f.file_name, &gvs);

    ProjectVersion {
        id: f.id.to_string(),
        project_id: f.mod_id.to_string(),
        provider: ProviderSource::CurseForge,
        name: f.display_name,
        version_number,
        release_channel,
        published_at: f.file_date,
        game_versions: gvs,
        loaders: ldrs,
        files: vec![VersionFile {
            url,
            filename: f.file_name,
            size: f.file_length,
            sha1,
            sha512: None,
            primary: true,
        }],
        dependencies: f
            .dependencies
            .into_iter()
            .map(|d| DependencySpec {
                project_id: d.mod_id.to_string(),
                version_id: None,
                relation: match d.relation_type {
                    3 => DependencyRelation::Required,
                    2 => DependencyRelation::Optional,
                    5 => DependencyRelation::Incompatible,
                    1 => DependencyRelation::Embedded,
                    _ => DependencyRelation::Optional,
                },
            })
            .collect(),
        server_pack_file_id,
    }
}

pub async fn resolve_curseforge_file_download(
    downloader: &DownloadManager,
    project_id: u64,
    file_id: u64,
    api_key: Option<&str>,
) -> ThkResult<(String, String, Option<String>)> {
    if let Ok(env) = cf_get_json::<CfEnvelope<CfFile>>(
        downloader,
        &format!("/mods/{project_id}/files/{file_id}"),
        api_key,
    )
    .await
    {
        let f = env.data;
        let sha1 = f
            .hashes
            .iter()
            .find(|h| h.algo == 1)
            .map(|h| h.value.clone());
        let url = f
            .download_url
            .unwrap_or_else(|| build_curseforge_cdn_fallback_url(f.id, &f.file_name));
        return Ok((url, f.file_name, sha1));
    }

    if let Ok(env) = cf_get_json::<CfEnvelope<String>>(
        downloader,
        &format!("/mods/{project_id}/files/{file_id}/download-url"),
        api_key,
    )
    .await
    {
        let fname = env
            .data
            .rsplit('/')
            .next()
            .and_then(|s| urlencoding::decode(s).ok().map(|c| c.into_owned()))
            .unwrap_or_else(|| format!("cf-{project_id}-{file_id}.jar"));
        return Ok((env.data, fname, None));
    }

    Err(ThkError::Provider(format!(
        "Cannot resolve CurseForge download URL for project {project_id} file {file_id}"
    )))
}

#[allow(clippy::too_many_arguments)]
pub async fn install_curseforge_content<F>(
    downloader: &DownloadManager,
    db: &Database,
    profile: &GameProfile,
    content_kind: ContentKind,
    project_summary: &ProjectSummary,
    version: &ProjectVersion,
    api_key: Option<&str>,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<Vec<PathBuf>>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let dest_subdir =
        SafePath::resolve_child(&profile.instance_path, content_kind.instance_subdir())?;
    tokio::fs::create_dir_all(&dest_subdir).await?;

    let mut queue = vec![(project_summary.clone(), version.clone())];
    let mut visited = HashSet::new();
    let mut tasks = Vec::new();
    let mut records = Vec::new();

    while let Some((proj, ver)) = queue.pop() {
        if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err(ThkError::Cancelled);
        }
        if !visited.insert(proj.project_id.clone()) {
            continue;
        }
        let Some(file) = ver.files.first() else {
            return Err(ThkError::Provider(format!(
                "No downloadable file for {}",
                proj.title
            )));
        };
        let clean_name = SafePath::validate_single_name(&file.filename)?;
        let dest = SafePath::resolve_child(&dest_subdir, clean_name)?;

        let mut task = DownloadTask::new(file.url.clone(), dest.clone());
        task.expected_sha1 = file.sha1.clone();
        task.expected_size = (file.size > 0).then_some(file.size);
        task.label = proj.title.clone();
        tasks.push(task);

        records.push((
            dest,
            InstalledContentRecord {
                profile_id: profile.id.clone(),
                content_kind,
                file_name: clean_name.to_string(),
                sha1: file.sha1.clone().unwrap_or_default(),
                provider: ProviderSource::CurseForge,
                project_id: proj.project_id.clone(),
                version_id: ver.id.clone(),
                title: proj.title.clone(),
                version_number: ver.version_number.clone(),
                icon_url: proj.icon_url.clone(),
                enabled: true,
                installed_at: Utc::now().timestamp(),
            },
        ));

        if matches!(content_kind, ContentKind::Mod | ContentKind::Plugin) {
            let ldr = (content_kind == ContentKind::Mod && profile.loader != LoaderType::Vanilla)
                .then_some(profile.loader);
            for dep in &ver.dependencies {
                if dep.relation != DependencyRelation::Required || visited.contains(&dep.project_id)
                {
                    continue;
                }
                let dep_vers = get_curseforge_versions(
                    downloader,
                    &dep.project_id,
                    Some(&profile.game_version),
                    ldr,
                    api_key,
                )
                .await?;
                let dependency = dep_vers
                    .iter()
                    .find(|v| v.release_channel == "release")
                    .cloned()
                    .or_else(|| dep_vers.into_iter().next())
                    .ok_or_else(|| {
                        ThkError::Provider(format!(
                            "No compatible version for required dependency {}",
                            dep.project_id
                        ))
                    })?;
                let project = get_curseforge_project(downloader, &dep.project_id, api_key).await?;
                queue.push((project.summary, dependency));
            }
        }
    }

    crate::install::install_content_batch(
        downloader,
        db,
        &dest_subdir,
        &format!("Installing {}", project_summary.title),
        tasks,
        records,
        cancel,
        on_progress,
    )
    .await
}

#[derive(Deserialize)]
struct CfPackManifest {
    name: Option<String>,
    minecraft: CfPackMinecraft,
    #[serde(default)]
    files: Vec<CfPackFileEntry>,
    overrides: Option<String>,
}

#[derive(Deserialize)]
struct CfPackMinecraft {
    version: String,
    #[serde(rename = "modLoaders", default)]
    mod_loaders: Vec<CfPackLoader>,
    #[serde(rename = "recommendedRam")]
    recommended_ram: Option<u32>,
}

#[derive(Deserialize)]
struct CfPackLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
struct CfPackFileEntry {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(default = "default_true")]
    required: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedCfPackFile {
    pub(crate) url: String,
    pub(crate) file_name: String,
    pub(crate) sha1: Option<String>,
    pub(crate) file_length: u64,
    pub(crate) game_versions: Vec<String>,
}

impl ResolvedCfPackFile {
    fn from_cf_file(f: CfFile) -> Self {
        let sha1 = f
            .hashes
            .iter()
            .find(|h| h.algo == 1)
            .map(|h| h.value.clone());
        let url = f
            .download_url
            .unwrap_or_else(|| build_curseforge_cdn_fallback_url(f.id, &f.file_name));
        Self {
            url,
            file_name: f.file_name,
            sha1,
            file_length: f.file_length,
            game_versions: f.game_versions,
        }
    }
}

async fn resolve_single_pack_file_meta(
    downloader: &DownloadManager,
    project_id: u64,
    file_id: u64,
    api_key: Option<&str>,
) -> ThkResult<ResolvedCfPackFile> {
    if let Ok(env) = cf_get_json::<CfEnvelope<CfFile>>(
        downloader,
        &format!("/mods/{project_id}/files/{file_id}"),
        api_key,
    )
    .await
    {
        return Ok(ResolvedCfPackFile::from_cf_file(env.data));
    }

    if let Ok(env) = cf_get_json::<CfEnvelope<String>>(
        downloader,
        &format!("/mods/{project_id}/files/{file_id}/download-url"),
        api_key,
    )
    .await
    {
        let fname = env
            .data
            .rsplit('/')
            .next()
            .and_then(|s| urlencoding::decode(s).ok().map(|c| c.into_owned()))
            .unwrap_or_else(|| format!("cf-{project_id}-{file_id}.jar"));
        return Ok(ResolvedCfPackFile {
            url: env.data,
            file_name: fname,
            sha1: None,
            file_length: 0,
            game_versions: Vec::new(),
        });
    }

    Err(ThkError::Provider(format!(
        "Cannot resolve CurseForge download URL for project {project_id} file {file_id}"
    )))
}

async fn try_bulk_fetch_cf_files(
    downloader: &DownloadManager,
    file_ids: &[u64],
    api_key: &str,
    cancel: Option<&CancellationToken>,
) -> ThkResult<HashMap<u64, ResolvedCfPackFile>> {
    let mut out = HashMap::with_capacity(file_ids.len());
    let url = format!("{CF_OFFICIAL_BASE}/mods/files");

    for chunk in file_ids.chunks(200) {
        if cancel.is_some_and(CancellationToken::is_cancelled) {
            return Err(ThkError::Cancelled);
        }
        let send_fut = downloader
            .client()
            .post(&url)
            .header("Accept", "application/json")
            .header("x-api-key", api_key)
            .json(&json!({ "fileIds": chunk }))
            .send();

        let resp_res = tokio::select! {
            biased;
            _ = async {
                match cancel {
                    Some(token) => token.cancelled().await,
                    None => std::future::pending::<()>().await,
                }
            } => return Err(ThkError::Cancelled),
            r = send_fut => r,
        };

        let Ok(resp) = resp_res else {
            break;
        };
        let status = resp.status();
        if matches!(status.as_u16(), 401 | 403) {
            return Err(ThkError::Provider(format!(
                "CurseForge API key rejected (HTTP {}). Check or clear your CurseForge API key in Settings.",
                status.as_u16()
            )));
        }
        if !status.is_success() {
            break;
        }
        if let Ok(env) = resp.json::<CfEnvelope<Vec<CfFile>>>().await {
            for f in env.data {
                out.insert(f.id, ResolvedCfPackFile::from_cf_file(f));
            }
        }
    }

    Ok(out)
}

pub(crate) async fn resolve_curseforge_pack_entries_concurrent<F>(
    downloader: &DownloadManager,
    entries: &[(u64, u64)],
    api_key: Option<&str>,
    phase_label: &str,
    cancel: Option<&CancellationToken>,
    on_progress: &F,
) -> ThkResult<Vec<ResolvedCfPackFile>>
where
    F: Fn(DownloadProgress) + Send + Sync,
{
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    let bulk_map = if let Some(key) = api_key.map(str::trim).filter(|k| !k.is_empty()) {
        let all_fids: Vec<u64> = entries.iter().map(|&(_, fid)| fid).collect();
        try_bulk_fetch_cf_files(downloader, &all_fids, key, cancel).await?
    } else {
        HashMap::new()
    };

    resolve_pack_entries_with(
        entries,
        bulk_map,
        phase_label,
        cancel,
        on_progress,
        |pid, fid| resolve_single_pack_file_meta(downloader, pid, fid, api_key),
    )
    .await
}

async fn resolve_pack_entries_with<R, Fut, F>(
    entries: &[(u64, u64)],
    pre_resolved: HashMap<u64, ResolvedCfPackFile>,
    phase_label: &str,
    cancel: Option<&CancellationToken>,
    on_progress: &F,
    mut resolve_one: R,
) -> ThkResult<Vec<ResolvedCfPackFile>>
where
    R: FnMut(u64, u64) -> Fut,
    Fut: std::future::Future<Output = ThkResult<ResolvedCfPackFile>>,
    F: Fn(DownloadProgress) + Send + Sync,
{
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    let total = entries.len();
    on_progress(DownloadProgress {
        phase: phase_label.to_string(),
        message: format!("Resolving 0 / {total} mod files..."),
        completed_files: 0,
        total_files: total,
        downloaded_bytes: 0,
        total_bytes: 0,
        bytes_per_sec: 0,
        percent: 0.0,
    });

    let mut ordered: Vec<Option<ResolvedCfPackFile>> = vec![None; total];
    let mut completed = 0usize;

    if !pre_resolved.is_empty() {
        for (idx, &(_, fid)) in entries.iter().enumerate() {
            if let Some(meta) = pre_resolved.get(&fid).cloned() {
                completed += 1;
                ordered[idx] = Some(meta);
            }
        }
        if completed > 0 {
            on_progress(DownloadProgress {
                phase: phase_label.to_string(),
                message: format!("Resolved {completed} / {total} mod files..."),
                completed_files: completed,
                total_files: total,
                downloaded_bytes: 0,
                total_bytes: 0,
                bytes_per_sec: 0,
                percent: (completed as f32 / total as f32) * 100.0,
            });
        }
    }

    let pending: Vec<(usize, u64, u64)> = entries
        .iter()
        .enumerate()
        .filter_map(|(idx, &(pid, fid))| ordered[idx].is_none().then_some((idx, pid, fid)))
        .collect();

    if !pending.is_empty() {
        let mut futs = stream::iter(pending)
            .map(|(idx, pid, fid)| {
                let fut = resolve_one(pid, fid);
                async move { (idx, pid, fid, fut.await) }
            })
            .buffer_unordered(16);

        loop {
            let next_item = tokio::select! {
                biased;
                _ = async {
                    match cancel {
                        Some(token) => token.cancelled().await,
                        None => std::future::pending::<()>().await,
                    }
                } => return Err(ThkError::Cancelled),
                item = futs.next() => item,
            };

            let Some((idx, pid, fid, res)) = next_item else {
                break;
            };
            let meta = res.map_err(|err| match err {
                ThkError::Cancelled => ThkError::Cancelled,
                other => ThkError::Provider(format!(
                    "Required CurseForge project {pid} file {fid}: {other}"
                )),
            })?;
            completed += 1;
            let fname = meta.file_name.clone();
            ordered[idx] = Some(meta);
            on_progress(DownloadProgress {
                phase: phase_label.to_string(),
                message: format!("{fname} ({completed}/{total})"),
                completed_files: completed,
                total_files: total,
                downloaded_bytes: 0,
                total_bytes: 0,
                bytes_per_sec: 0,
                percent: (completed as f32 / total as f32) * 100.0,
            });
        }
    }

    ordered
        .into_iter()
        .enumerate()
        .map(|(idx, opt)| {
            opt.ok_or_else(|| {
                let (pid, fid) = entries[idx];
                ThkError::Provider(format!("Unresolved CurseForge project {pid} file {fid}"))
            })
        })
        .collect()
}

pub async fn import_curseforge_zip<F>(
    downloader: &DownloadManager,
    zip_path: &Path,
    instance_dir: &Path,
    api_key: Option<&str>,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<ImportedPackMetadata>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }
    tokio::fs::create_dir_all(instance_dir).await?;
    let manifest_raw = read_zip_entry_string(zip_path, "manifest.json")?.ok_or_else(|| {
        ThkError::InvalidInput("Missing manifest.json inside CurseForge modpack ZIP".into())
    })?;
    let manifest: CfPackManifest = serde_json::from_str(&manifest_raw)?;

    let primary_ldr = manifest
        .minecraft
        .mod_loaders
        .iter()
        .find(|l| l.primary)
        .or_else(|| manifest.minecraft.mod_loaders.first());

    let (loader, loader_version) = match primary_ldr.map(|l| l.id.as_str()) {
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
    };

    let mods_dir = SafePath::resolve_child(instance_dir, "mods")?;
    tokio::fs::create_dir_all(&mods_dir).await?;

    let required_entries: Vec<CfPackFileEntry> =
        manifest.files.into_iter().filter(|f| f.required).collect();
    let req_pairs: Vec<(u64, u64)> = required_entries
        .iter()
        .map(|f| (f.project_id, f.file_id))
        .collect();

    let resolved_files = resolve_curseforge_pack_entries_concurrent(
        downloader,
        &req_pairs,
        api_key,
        "Resolving Modpack Manifest",
        cancel.as_ref(),
        &on_progress,
    )
    .await?;

    let mut tasks = Vec::with_capacity(resolved_files.len());
    for (item, meta) in required_entries.iter().zip(resolved_files) {
        let mut task =
            required_pack_download(&mods_dir, item, Ok((meta.url, meta.file_name, meta.sha1)))?;
        if meta.file_length > 0 {
            task.expected_size = Some(meta.file_length);
        }
        tasks.push(task);
    }

    let now = Utc::now().timestamp();
    let tracked_entries: Vec<(PathBuf, InstalledContentRecord)> = tasks
        .iter()
        .zip(required_entries.iter())
        .filter_map(|(task, entry)| {
            let fname = task.dest.file_name()?.to_str()?.to_string();
            let (title, version_number) = split_filename_title_and_version(&fname);
            Some((
                task.dest.clone(),
                InstalledContentRecord {
                    profile_id: String::new(),
                    content_kind: ContentKind::Mod,
                    file_name: fname,
                    sha1: task.expected_sha1.clone().unwrap_or_default(),
                    provider: ProviderSource::CurseForge,
                    project_id: entry.project_id.to_string(),
                    version_id: entry.file_id.to_string(),
                    title,
                    version_number,
                    icon_url: None,
                    enabled: true,
                    installed_at: now,
                },
            ))
        })
        .collect();

    let pack_name = manifest.name.unwrap_or_else(|| {
        zip_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "CurseForge Pack".into())
    });

    downloader
        .download_batch(
            &format!("Importing {pack_name}"),
            tasks,
            16,
            cancel.clone(),
            on_progress.clone(),
        )
        .await?;

    let overrides_folder = manifest.overrides.unwrap_or_else(|| "overrides".into());
    on_progress(DownloadProgress {
        phase: "Extracting Modpack Overrides".into(),
        message: format!("Extracting {overrides_folder}/ & scanning installed files..."),
        completed_files: 0,
        total_files: 1,
        downloaded_bytes: 0,
        total_bytes: 0,
        bytes_per_sec: 0,
        percent: 96.0,
    });
    let zip_owned = zip_path.to_path_buf();
    let inst_owned = instance_dir.to_path_buf();
    let extraction_cancel = cancel.clone();
    let installed_content =
        tokio::task::spawn_blocking(move || -> ThkResult<Vec<InstalledContentRecord>> {
            extract_zip_prefix_cancellable(
                &zip_owned,
                &overrides_folder,
                &inst_owned,
                extraction_cancel.as_ref(),
            )?;

            let mut known_names = HashSet::new();
            let mut records = Vec::with_capacity(tracked_entries.len());
            for (path, mut rec) in tracked_entries {
                if rec.sha1.is_empty() {
                    rec.sha1 = sha1_file(&path).unwrap_or_default();
                }
                known_names.insert((rec.content_kind, rec.file_name.to_ascii_lowercase()));
                records.push(rec);
            }
            for extra in
                crate::technic::scan_pack_content_records(&inst_owned, ProviderSource::Local)
            {
                if known_names.insert((extra.content_kind, extra.file_name.to_ascii_lowercase())) {
                    records.push(extra);
                }
            }
            Ok(records)
        })
        .await
        .map_err(|e| ThkError::Process(format!("Overrides extraction failed: {e}")))??;

    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }

    Ok(ImportedPackMetadata {
        name: pack_name,
        game_version: manifest.minecraft.version,
        loader,
        loader_version,
        icon_url: None,
        recommended_ram_mb: manifest.minecraft.recommended_ram,
        installed_content,
    })
}

fn required_pack_download(
    mods_dir: &Path,
    item: &CfPackFileEntry,
    resolved: ThkResult<(String, String, Option<String>)>,
) -> ThkResult<DownloadTask> {
    let context = || {
        format!(
            "Required CurseForge project {} file {}",
            item.project_id, item.file_id
        )
    };
    let (url, filename, sha1) = resolved.map_err(|err| match err {
        ThkError::Cancelled => ThkError::Cancelled,
        other => ThkError::Provider(format!("{}: {other}", context())),
    })?;
    let clean_name = SafePath::validate_single_name(&filename)
        .map_err(|err| ThkError::Provider(format!("{}: {err}", context())))?;
    let dest = SafePath::resolve_child(mods_dir, clean_name)?;
    let mut task = DownloadTask::new(url, dest);
    task.expected_sha1 = sha1;
    task.label = clean_name.to_string();
    Ok(task)
}

struct TempCfZipGuard(PathBuf);

impl Drop for TempCfZipGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn install_curseforge_modpack_version<F>(
    downloader: &DownloadManager,
    version: &ProjectVersion,
    cache_dir: &Path,
    instance_dir: &Path,
    icon_url: Option<String>,
    api_key: Option<&str>,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<ImportedPackMetadata>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    let file = version.files.first().ok_or_else(|| {
        ThkError::Provider("CurseForge modpack version contains no archive".into())
    })?;

    tokio::fs::create_dir_all(cache_dir).await?;
    let clean_name = SafePath::validate_single_name(&file.filename)?;
    let zip_dest = SafePath::resolve_child(cache_dir, format!("{}-{clean_name}", Uuid::new_v4()))?;
    let _zip_guard = TempCfZipGuard(zip_dest.clone());

    let mut task = DownloadTask::new(file.url.clone(), zip_dest.clone());
    task.expected_sha1 = file.sha1.clone();
    task.expected_size = (file.size > 0).then_some(file.size);
    task.force_overwrite = true;
    task.label = clean_name.to_string();

    downloader
        .download_batch(
            "Downloading CurseForge Modpack ZIP",
            vec![task],
            1,
            cancel.clone(),
            on_progress.clone(),
        )
        .await?;

    let mut meta = import_curseforge_zip(
        downloader,
        &zip_dest,
        instance_dir,
        api_key,
        cancel,
        on_progress,
    )
    .await?;
    meta.icon_url = icon_url;
    Ok(meta)
}

#[cfg(test)]
#[path = "curseforge_pack_tests.rs"]
mod pack_tests;
