use crate::curseforge::resolve_curseforge_file_download;
use crate::provider::{
    GalleryImage, ImportedPackMetadata, ProjectDetail, ProjectSummary, ProjectVersion, SearchPage,
    SearchQuery, VersionFile,
};
use chrono::DateTime;
use futures_util::stream::{self, StreamExt};
use serde::Deserialize;
use std::path::Path;
use thk_core::models::{ContentKind, DownloadProgress, LoaderType, ProviderSource};
use thk_core::paths::SafePath;
use thk_core::{DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;

pub const FTB_API_BASE: &str = "https://api.modpacks.ch/public";

#[derive(Deserialize)]
struct FtbIdList {
    #[serde(default)]
    packs: Vec<u64>,
}

#[derive(Deserialize)]
struct FtbPackRaw {
    id: Option<u64>,
    name: Option<String>,
    synopsis: Option<String>,
    description: Option<String>,
    #[serde(default)]
    installs: u64,
    updated: Option<i64>,
    #[serde(default)]
    authors: Vec<FtbAuthor>,
    #[serde(default)]
    art: Vec<FtbArt>,
    #[serde(default)]
    tags: Vec<FtbTag>,
    #[serde(default)]
    versions: Vec<FtbVersionSummary>,
    status: Option<String>,
}

#[derive(Deserialize)]
struct FtbAuthor {
    name: String,
}

#[derive(Deserialize)]
struct FtbArt {
    #[serde(rename = "type", default)]
    art_type: String,
    url: String,
}

#[derive(Deserialize)]
struct FtbTag {
    name: String,
}

#[derive(Deserialize)]
struct FtbVersionSummary {
    id: u64,
    name: String,
    #[serde(rename = "type", default)]
    ver_type: String,
    updated: Option<i64>,
    #[serde(default)]
    targets: Vec<FtbTarget>,
}

#[derive(Deserialize)]
struct FtbTarget {
    #[serde(rename = "type", default)]
    target_type: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
}

#[derive(Deserialize)]
struct FtbVersionManifest {
    name: Option<String>,
    #[serde(default)]
    targets: Vec<FtbTarget>,
    #[serde(default)]
    files: Vec<FtbFileEntry>,
    specs: Option<FtbSpecs>,
}

#[derive(Deserialize)]
struct FtbSpecs {
    recommended: Option<u32>,
}

#[derive(Deserialize)]
struct FtbFileEntry {
    #[serde(default)]
    path: String,
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    mirrors: Vec<String>,
    sha1: Option<String>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    serveronly: bool,
    curseforge: Option<FtbCurseForgeRef>,
}

#[derive(Deserialize)]
struct FtbCurseForgeRef {
    project: u64,
    file: u64,
}

pub async fn search_ftb(downloader: &DownloadManager, q: &SearchQuery) -> ThkResult<SearchPage> {
    let list_url = if q.query.trim().is_empty() || q.query.trim().eq_ignore_ascii_case("ftb") {
        format!("{FTB_API_BASE}/modpack/popular/installs/100")
    } else {
        format!(
            "{FTB_API_BASE}/modpack/search/100?term={}",
            urlencoding::encode(q.query.trim())
        )
    };

    let id_list: FtbIdList = downloader.get_json(&list_url).await?;
    let total_hits = id_list.packs.len();
    let limit = q.limit.clamp(1, 30);
    let slice: Vec<u64> = id_list
        .packs
        .into_iter()
        .skip(q.offset)
        .take(limit)
        .collect();

    let fetched: Vec<Option<ProjectSummary>> = stream::iter(slice)
        .map(|id| async move {
            let url = format!("{FTB_API_BASE}/modpack/{id}");
            let raw: FtbPackRaw = downloader.get_json(&url).await.ok()?;
            map_ftb_summary(&raw)
        })
        .buffered(6)
        .collect()
        .await;

    Ok(SearchPage {
        hits: fetched.into_iter().flatten().collect(),
        total_hits,
    })
}

fn map_ftb_summary(raw: &FtbPackRaw) -> Option<ProjectSummary> {
    if raw.status.as_deref() == Some("error") {
        return None;
    }
    let id = raw.id?;
    let icon_url = raw
        .art
        .iter()
        .find(|a| a.art_type == "square")
        .or_else(|| raw.art.first())
        .map(|a| a.url.clone());

    let updated_at = raw
        .updated
        .and_then(|ts| DateTime::from_timestamp(ts, 0).map(|d| d.to_rfc3339()));

    let mut categories: Vec<String> = Vec::new();
    if let Some(latest) = raw.versions.last() {
        for t in &latest.targets {
            if t.target_type == "modloader" && !t.name.is_empty() {
                categories.push(t.name.to_ascii_lowercase());
            } else if t.target_type == "game" && !t.version.is_empty() {
                categories.push(t.version.clone());
            }
        }
    }
    for tag in &raw.tags {
        if !tag.name.is_empty() && !categories.iter().any(|c| c.eq_ignore_ascii_case(&tag.name)) {
            categories.push(tag.name.clone());
        }
    }

    Some(ProjectSummary {
        provider: ProviderSource::Ftb,
        project_id: id.to_string(),
        slug: id.to_string(),
        title: raw.name.clone().unwrap_or_else(|| format!("FTB Pack {id}")),
        summary: raw.synopsis.clone().unwrap_or_default(),
        author: raw
            .authors
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "FTB Team".into()),
        downloads: raw.installs,
        icon_url,
        categories,
        updated_at,
        content_kind: ContentKind::Modpack,
    })
}

pub async fn get_ftb_project(
    downloader: &DownloadManager,
    project_id: &str,
) -> ThkResult<ProjectDetail> {
    let id: u64 = project_id
        .parse()
        .map_err(|_| ThkError::InvalidInput(format!("Invalid FTB pack id: {project_id}")))?;
    let raw: FtbPackRaw = downloader
        .get_json(&format!("{FTB_API_BASE}/modpack/{id}"))
        .await?;
    let summary = map_ftb_summary(&raw)
        .ok_or_else(|| ThkError::Provider(format!("FTB pack {id} not found")))?;

    let mut gvs = Vec::new();
    let mut ldrs = Vec::new();
    for v in &raw.versions {
        for t in &v.targets {
            if t.target_type == "game" && !t.version.is_empty() && !gvs.contains(&t.version) {
                gvs.push(t.version.clone());
            } else if t.target_type == "modloader" && !t.name.is_empty() && !ldrs.contains(&t.name) {
                ldrs.push(t.name.clone());
            }
        }
    }

    let gallery = raw
        .art
        .iter()
        .filter(|a| a.art_type == "splash" || a.art_type == "background")
        .map(|a| GalleryImage {
            url: a.url.clone(),
            title: String::new(),
        })
        .collect();

    Ok(ProjectDetail {
        body_markdown: raw
            .description
            .or(raw.synopsis)
            .unwrap_or_else(|| summary.summary.clone()),
        summary,
        gallery,
        source_url: Some(format!("https://www.feed-the-beast.com/modpacks/{id}")),
        game_versions: gvs,
        loaders: ldrs,
    })
}

pub async fn get_ftb_versions(
    downloader: &DownloadManager,
    project_id: &str,
) -> ThkResult<Vec<ProjectVersion>> {
    let id: u64 = project_id
        .parse()
        .map_err(|_| ThkError::InvalidInput(format!("Invalid FTB pack id: {project_id}")))?;
    let raw: FtbPackRaw = downloader
        .get_json(&format!("{FTB_API_BASE}/modpack/{id}"))
        .await?;

    let mut out: Vec<ProjectVersion> = raw
        .versions
        .into_iter()
        .map(|v| {
            let mc_ver = v
                .targets
                .iter()
                .find(|t| t.target_type == "game")
                .map(|t| t.version.clone())
                .unwrap_or_default();
            let loader_name = v
                .targets
                .iter()
                .find(|t| t.target_type == "modloader")
                .map(|t| t.name.clone())
                .unwrap_or_else(|| "forge".into());
            let published_at = v
                .updated
                .and_then(|ts| DateTime::from_timestamp(ts, 0).map(|d| d.to_rfc3339()));

            ProjectVersion {
                id: v.id.to_string(),
                project_id: id.to_string(),
                provider: ProviderSource::Ftb,
                name: v.name.clone(),
                version_number: v.name.clone(),
                release_channel: if v.ver_type.eq_ignore_ascii_case("release") {
                    "release".into()
                } else {
                    "beta".into()
                },
                published_at,
                game_versions: if mc_ver.is_empty() {
                    Vec::new()
                } else {
                    vec![mc_ver]
                },
                loaders: vec![loader_name],
                files: vec![VersionFile {
                    url: format!("{FTB_API_BASE}/modpack/{id}/{}", v.id),
                    filename: format!("ftb-{id}-{}.json", v.id),
                    size: 0,
                    sha1: None,
                    sha512: None,
                    primary: true,
                }],
                dependencies: Vec::new(),
            }
        })
        .collect();

    out.reverse();
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub async fn install_ftb_modpack_version<F>(
    downloader: &DownloadManager,
    pack_id: &str,
    version_id: &str,
    pack_title: &str,
    icon_url: Option<String>,
    instance_dir: &Path,
    cf_api_key: Option<&str>,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<ImportedPackMetadata>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    tokio::fs::create_dir_all(instance_dir).await?;
    let manifest_url = format!(
        "{FTB_API_BASE}/modpack/{}/{}",
        urlencoding::encode(pack_id),
        urlencoding::encode(version_id)
    );
    let manifest: FtbVersionManifest = downloader.get_json(&manifest_url).await?;

    let game_version = manifest
        .targets
        .iter()
        .find(|t| t.target_type == "game")
        .map(|t| t.version.clone())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ThkError::InvalidInput("FTB version manifest missing Minecraft version target".into()))?;

    let (loader, loader_version) = match manifest.targets.iter().find(|t| t.target_type == "modloader") {
        Some(t) => (
            LoaderType::from_str_lossy(&t.name),
            (!t.version.is_empty()).then(|| t.version.clone()),
        ),
        None => (LoaderType::Forge, None),
    };

    let mut tasks = Vec::new();
    for entry in manifest.files {
        if entry.serveronly {
            continue;
        }
        if let Some(ref c) = cancel {
            if c.is_cancelled() {
                return Err(ThkError::Cancelled);
            }
        }

        let clean_rel_dir = entry
            .path
            .trim_start_matches("./")
            .trim_start_matches(".\\")
            .trim_matches('/')
            .trim_matches('\\');
        let clean_fname = SafePath::validate_single_name(&entry.name)?;
        let rel_full = if clean_rel_dir.is_empty() {
            clean_fname.to_string()
        } else {
            format!("{clean_rel_dir}/{clean_fname}")
        };
        let dest = SafePath::resolve_child(instance_dir, &rel_full)?;

        let (primary_url, expected_sha1) = if !entry.url.is_empty() {
            (entry.url, entry.sha1)
        } else if let Some(cf) = entry.curseforge {
            match resolve_curseforge_file_download(downloader, cf.project, cf.file, cf_api_key).await {
                Ok((u, _, s)) => (u, entry.sha1.or(s)),
                Err(_) => continue,
            }
        } else {
            continue;
        };

        let mut task = DownloadTask::new(primary_url, dest);
        task.fallback_urls = entry.mirrors;
        task.expected_sha1 = expected_sha1;
        if entry.size > 0 {
            task.expected_size = Some(entry.size);
        }
        task.label = clean_fname.to_string();
        tasks.push(task);
    }

    let full_name = match manifest.name {
        Some(vname) if !vname.is_empty() => format!("{pack_title} ({vname})"),
        _ => pack_title.to_string(),
    };

    downloader
        .download_batch(
            &format!("Installing FTB {full_name}"),
            tasks,
            12,
            cancel,
            on_progress,
        )
        .await?;

    Ok(ImportedPackMetadata {
        name: full_name,
        game_version,
        loader,
        loader_version,
        icon_url,
        recommended_ram_mb: manifest.specs.and_then(|s| s.recommended),
    })
}
