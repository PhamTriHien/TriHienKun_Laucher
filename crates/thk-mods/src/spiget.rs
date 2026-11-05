use crate::provider::{
    ProjectDetail, ProjectSummary, ProjectVersion, SearchPage, SearchQuery, VersionFile,
};
use chrono::DateTime;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use thk_core::models::{ContentKind, DownloadProgress, ProviderSource};
use thk_core::paths::SafePath;
use thk_core::{DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;

pub const SPIGET_API_BASE: &str = "https://api.spiget.org/v2";

#[derive(Deserialize)]
struct SpigetResource {
    id: u64,
    name: String,
    #[serde(default)]
    tag: String,
    #[serde(default)]
    downloads: u64,
    #[serde(rename = "updateDate")]
    update_date: Option<i64>,
    #[serde(rename = "testedVersions", default)]
    tested_versions: Vec<String>,
    icon: Option<SpigetIcon>,
    file: Option<SpigetFile>,
}

#[derive(Deserialize)]
struct SpigetIcon {
    url: Option<String>,
}

#[derive(Deserialize)]
struct SpigetFile {
    #[serde(rename = "type", default)]
    file_type: String,
    #[serde(default)]
    size: f64,
    #[serde(rename = "externalUrl")]
    external_url: Option<String>,
}

pub async fn search_spiget(
    downloader: &DownloadManager,
    q: &SearchQuery,
) -> ThkResult<SearchPage> {
    let size = q.limit.clamp(1, 50);
    let page = (q.offset / size) + 1;
    let fields = "id,name,tag,downloads,testedVersions,icon,file,updateDate";

    let url = if q.query.trim().is_empty() {
        format!("{SPIGET_API_BASE}/resources/free?size={size}&page={page}&sort=-downloads&fields={fields}")
    } else {
        format!(
            "{SPIGET_API_BASE}/search/resources/{}?field=name&size={size}&page={page}&sort=-downloads&fields={fields}",
            urlencoding::encode(q.query.trim())
        )
    };

    let list: Vec<SpigetResource> = downloader.get_json(&url).await.unwrap_or_default();
    let hits: Vec<ProjectSummary> = list.into_iter().map(map_spiget_summary).collect();
    let total_hits = if hits.len() == size {
        q.offset + size * 2
    } else {
        q.offset + hits.len()
    };

    Ok(SearchPage { hits, total_hits })
}

fn map_spiget_summary(r: SpigetResource) -> ProjectSummary {
    let icon_url = r
        .icon
        .and_then(|i| i.url)
        .filter(|u| !u.is_empty())
        .map(|u| {
            if u.starts_with("http") {
                u
            } else {
                format!("https://www.spigotmc.org/{}", u.trim_start_matches('/'))
            }
        });

    let updated_at = r
        .update_date
        .and_then(|ts| DateTime::from_timestamp(ts, 0).map(|d| d.to_rfc3339()));

    ProjectSummary {
        provider: ProviderSource::Spiget,
        project_id: r.id.to_string(),
        slug: SafePath::slugify_dir_name(&r.name),
        title: r.name,
        summary: r.tag,
        author: "SpigotMC".into(),
        downloads: r.downloads,
        icon_url,
        categories: r.tested_versions,
        updated_at,
        content_kind: ContentKind::Plugin,
    }
}

pub async fn get_spiget_project(
    downloader: &DownloadManager,
    resource_id: &str,
) -> ThkResult<ProjectDetail> {
    let r: SpigetResource = downloader
        .get_json(&format!(
            "{SPIGET_API_BASE}/resources/{}",
            urlencoding::encode(resource_id)
        ))
        .await?;
    let gvs = r.tested_versions.clone();
    let id = r.id;
    let summary = map_spiget_summary(r);

    Ok(ProjectDetail {
        body_markdown: summary.summary.clone(),
        summary,
        gallery: Vec::new(),
        source_url: Some(format!("https://www.spigotmc.org/resources/{id}")),
        game_versions: gvs,
        loaders: vec!["paper".into(), "purpur".into(), "spigot".into()],
    })
}

pub async fn get_spiget_versions(
    downloader: &DownloadManager,
    resource_id: &str,
) -> ThkResult<Vec<ProjectVersion>> {
    let r: SpigetResource = downloader
        .get_json(&format!(
            "{SPIGET_API_BASE}/resources/{}",
            urlencoding::encode(resource_id)
        ))
        .await?;

    let is_external = r
        .file
        .as_ref()
        .is_some_and(|f| f.file_type.eq_ignore_ascii_case("external"));

    let download_url = if is_external {
        r.file
            .as_ref()
            .and_then(|f| f.external_url.clone())
            .unwrap_or_else(|| format!("https://www.spigotmc.org/resources/{}", r.id))
    } else {
        format!("{SPIGET_API_BASE}/resources/{}/download", r.id)
    };

    let slug = SafePath::slugify_dir_name(&r.name);
    Ok(vec![ProjectVersion {
        id: format!("latest-{}", r.id),
        project_id: r.id.to_string(),
        provider: ProviderSource::Spiget,
        name: if is_external {
            "External Release (Browser)".into()
        } else {
            "Latest Direct JAR".into()
        },
        version_number: "latest".into(),
        release_channel: "release".into(),
        published_at: r
            .update_date
            .and_then(|ts| DateTime::from_timestamp(ts, 0).map(|d| d.to_rfc3339())),
        game_versions: r.tested_versions,
        loaders: vec!["paper".into(), "purpur".into()],
        files: vec![VersionFile {
            url: download_url,
            filename: format!("{slug}-{}.jar", r.id),
            size: r.file.map(|f| (f.size * 1024.0) as u64).unwrap_or(0),
            sha1: None,
            sha512: None,
            primary: !is_external,
        }],
        dependencies: Vec::new(),
    }])
}

pub async fn install_spiget_plugin<F>(
    downloader: &DownloadManager,
    resource_id: &str,
    plugin_name: &str,
    server_dir: &Path,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<PathBuf>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let versions = get_spiget_versions(downloader, resource_id).await?;
    let ver = versions
        .first()
        .ok_or_else(|| ThkError::Provider("No Spiget version found".into()))?;
    let file = ver
        .files
        .first()
        .ok_or_else(|| ThkError::Provider("No Spiget file entry found".into()))?;

    if !file.primary {
        return Err(ThkError::InvalidInput(format!(
            "Plugin '{plugin_name}' được lưu trữ bên ngoài SpigotMC ({}). Vui lòng tải từ Modrinth hoặc mở trang nguồn.",
            file.url
        )));
    }

    let plugins_dir = SafePath::resolve_child(server_dir, "plugins")?;
    tokio::fs::create_dir_all(&plugins_dir).await?;
    let clean_name = SafePath::validate_single_name(&file.filename)?;
    let dest = SafePath::resolve_child(&plugins_dir, clean_name)?;

    let mut task = DownloadTask::new(file.url.clone(), dest.clone());
    task.label = plugin_name.to_string();

    downloader
        .download_batch(
            &format!("Installing plugin {plugin_name}"),
            vec![task],
            1,
            cancel,
            on_progress,
        )
        .await?;

    Ok(dest)
}
