use crate::provider::{
    GalleryImage, ImportedPackMetadata, ProjectDetail, ProjectSummary, ProjectVersion, SearchPage,
    SearchQuery, VersionFile,
};
use chrono::DateTime;
use md5::{Digest, Md5};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::path::Path;
use thk_core::archive::{extract_zip_filtered, flatten_single_subdir, read_zip_entry_string};
use thk_core::models::{ContentKind, DownloadProgress, LoaderType, ProviderSource};
use thk_core::paths::SafePath;
use thk_core::{DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;

pub const TECHNIC_API_BASE: &str = "https://api.technicpack.net";
const TECHNIC_USER_AGENT: &str = "TechnicLauncher/167";

async fn technic_get_json<T: DeserializeOwned>(
    downloader: &DownloadManager,
    url: &str,
) -> ThkResult<T> {
    let resp = downloader
        .client()
        .get(url)
        .header("User-Agent", TECHNIC_USER_AGENT)
        .header("Accept", "application/json")
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(ThkError::Provider(format!(
            "Technic HTTP {} from {url}",
            resp.status()
        )));
    }
    resp.json::<T>().await.map_err(ThkError::from)
}

#[derive(Deserialize)]
struct TechnicSearchResp {
    #[serde(default)]
    modpacks: Vec<TechnicSearchHit>,
}

#[derive(Deserialize)]
struct TechnicSearchHit {
    id: Option<u64>,
    slug: String,
    name: String,
    #[serde(rename = "iconUrl")]
    icon_url: Option<String>,
}

#[derive(Deserialize)]
struct TechnicPackDetail {
    name: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    user: Option<String>,
    url: Option<String>,
    #[serde(rename = "platformUrl")]
    platform_url: Option<String>,
    minecraft: Option<String>,
    #[serde(default)]
    installs: u64,
    description: Option<String>,
    version: Option<String>,
    solder: Option<String>,
    icon: Option<TechnicAsset>,
    background: Option<TechnicAsset>,
    #[serde(default)]
    feed: Vec<TechnicFeed>,
}

#[derive(Deserialize)]
struct TechnicAsset {
    url: Option<String>,
}

#[derive(Deserialize)]
struct TechnicFeed {
    date: Option<i64>,
}

#[derive(Deserialize)]
struct SolderPackInfo {
    #[serde(default)]
    builds: Vec<String>,
    recommended: Option<String>,
}

#[derive(Deserialize)]
struct SolderBuildManifest {
    minecraft: Option<String>,
    forge: Option<String>,
    memory: Option<u32>,
    #[serde(default)]
    mods: Vec<SolderModEntry>,
}

#[derive(Deserialize)]
struct SolderModEntry {
    name: String,
    version: String,
    url: String,
    md5: Option<String>,
    #[serde(default)]
    filesize: u64,
}

pub async fn search_technic(
    downloader: &DownloadManager,
    q: &SearchQuery,
) -> ThkResult<SearchPage> {
    let term = if q.query.trim().is_empty() {
        "tekkit"
    } else {
        q.query.trim()
    };
    let url = format!(
        "{TECHNIC_API_BASE}/search?q={}&build=999&limit=200",
        urlencoding::encode(term)
    );
    let resp: TechnicSearchResp = technic_get_json(downloader, &url).await?;
    let total_hits = resp.modpacks.len();
    let hits = resp
        .modpacks
        .into_iter()
        .skip(q.offset)
        .take(q.limit.clamp(1, 50))
        .map(|m| ProjectSummary {
            provider: ProviderSource::Technic,
            project_id: m.slug.clone(),
            slug: m.slug,
            title: m.name,
            summary: m
                .id
                .map(|id| format!("Technic Modpack #{id}"))
                .unwrap_or_default(),
            author: String::new(),
            downloads: 0,
            icon_url: m.icon_url,
            categories: vec!["Modpack".into()],
            updated_at: None,
            content_kind: ContentKind::Modpack,
        })
        .collect();

    Ok(SearchPage { hits, total_hits })
}

pub async fn get_technic_project(
    downloader: &DownloadManager,
    slug: &str,
) -> ThkResult<ProjectDetail> {
    let url = format!(
        "{TECHNIC_API_BASE}/modpack/{}?build=999",
        urlencoding::encode(slug)
    );
    let p: TechnicPackDetail = technic_get_json(downloader, &url).await?;

    let updated_at = p
        .feed
        .first()
        .and_then(|f| f.date)
        .and_then(|ts| DateTime::from_timestamp(ts, 0).map(|d| d.to_rfc3339()));

    let gallery = p
        .background
        .and_then(|b| b.url)
        .into_iter()
        .map(|u| GalleryImage {
            url: u,
            title: String::new(),
        })
        .collect();

    let mc_vers = p.minecraft.clone().into_iter().collect();
    let title = p.display_name.unwrap_or_else(|| p.name.clone());
    let desc = p.description.unwrap_or_default();

    Ok(ProjectDetail {
        summary: ProjectSummary {
            provider: ProviderSource::Technic,
            project_id: p.name.clone(),
            slug: p.name,
            title,
            summary: desc.lines().next().unwrap_or_default().to_string(),
            author: p.user.unwrap_or_else(|| "Technic Author".into()),
            downloads: p.installs,
            icon_url: p.icon.and_then(|i| i.url),
            categories: vec!["Modpack".into()],
            updated_at,
            content_kind: ContentKind::Modpack,
        },
        body_markdown: desc,
        gallery,
        source_url: p.platform_url.or(p.url),
        game_versions: mc_vers,
        loaders: vec!["forge".into()],
    })
}

pub async fn get_technic_versions(
    downloader: &DownloadManager,
    slug: &str,
) -> ThkResult<Vec<ProjectVersion>> {
    let url = format!(
        "{TECHNIC_API_BASE}/modpack/{}?build=999",
        urlencoding::encode(slug)
    );
    let p: TechnicPackDetail = technic_get_json(downloader, &url).await?;
    let mc_vers: Vec<String> = p.minecraft.clone().into_iter().collect();

    if let Some(solder_raw) = p.solder.filter(|s| !s.trim().is_empty()) {
        let base = format!("{}/", solder_raw.trim_end_matches('/'));
        if let Ok(info) = technic_get_json::<SolderPackInfo>(
            downloader,
            &format!("{base}modpack/{}", urlencoding::encode(&p.name)),
        )
        .await
        {
            if !info.builds.is_empty() {
                let mut out: Vec<ProjectVersion> = info
                    .builds
                    .into_iter()
                    .map(|b| ProjectVersion {
                        id: b.clone(),
                        project_id: p.name.clone(),
                        provider: ProviderSource::Technic,
                        name: format!("Build {b}"),
                        version_number: b.clone(),
                        release_channel: if info.recommended.as_deref() == Some(b.as_str()) {
                            "release".into()
                        } else {
                            "beta".into()
                        },
                        published_at: None,
                        game_versions: mc_vers.clone(),
                        loaders: vec!["forge".into()],
                        files: vec![VersionFile {
                            url: format!("{base}modpack/{}/{b}", urlencoding::encode(&p.name)),
                            filename: format!("{}-{b}.solder", p.name),
                            size: 0,
                            sha1: None,
                            sha512: None,
                            primary: true,
                        }],
                        dependencies: Vec::new(),
                    })
                    .collect();
                out.reverse();
                return Ok(out);
            }
        }
    }

    if let Some(zip_url) = p.url.filter(|u| !u.trim().is_empty()) {
        let ver = p.version.unwrap_or_else(|| "1.0".into());
        return Ok(vec![ProjectVersion {
            id: ver.clone(),
            project_id: p.name.clone(),
            provider: ProviderSource::Technic,
            name: ver.clone(),
            version_number: ver.clone(),
            release_channel: "release".into(),
            published_at: None,
            game_versions: mc_vers,
            loaders: vec!["forge".into()],
            files: vec![VersionFile {
                url: zip_url,
                filename: format!("{}-{ver}.zip", p.name),
                size: 0,
                sha1: None,
                sha512: None,
                primary: true,
            }],
            dependencies: Vec::new(),
        }]);
    }

    Ok(Vec::new())
}

pub async fn install_technic_modpack_version<F>(
    downloader: &DownloadManager,
    slug: &str,
    build_id: &str,
    cache_dir: &Path,
    instance_dir: &Path,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<ImportedPackMetadata>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    tokio::fs::create_dir_all(instance_dir).await?;
    let detail_url = format!(
        "{TECHNIC_API_BASE}/modpack/{}?build=999",
        urlencoding::encode(slug)
    );
    let pack: TechnicPackDetail = technic_get_json(downloader, &detail_url).await?;
    let display_title = pack
        .display_name
        .clone()
        .unwrap_or_else(|| pack.name.clone());
    let icon_url = pack.icon.and_then(|i| i.url);

    if let Some(solder_raw) = pack.solder.filter(|s| !s.trim().is_empty()) {
        let base = format!("{}/", solder_raw.trim_end_matches('/'));
        let build_url = format!(
            "{base}modpack/{}/{}",
            urlencoding::encode(&pack.name),
            urlencoding::encode(build_id)
        );
        let manifest: SolderBuildManifest = technic_get_json(downloader, &build_url).await?;

        let solder_tmp = SafePath::resolve_child(cache_dir, format!("solder-{}-{build_id}", pack.name))?;
        if solder_tmp.exists() {
            tokio::fs::remove_dir_all(&solder_tmp).await?;
        }
        tokio::fs::create_dir_all(&solder_tmp).await?;

        let mut tasks = Vec::new();
        let mut archives = Vec::new();
        for (idx, m) in manifest.mods.iter().enumerate() {
            let zip_name = format!("{idx:04}-{}-{}.zip", SafePath::slugify_dir_name(&m.name), SafePath::slugify_dir_name(&m.version));
            let dest = SafePath::resolve_child(&solder_tmp, &zip_name)?;
            let mut task = DownloadTask::new(m.url.clone(), dest.clone());
            if m.filesize > 0 {
                task.expected_size = Some(m.filesize);
            }
            task.label = format!("{} {}", m.name, m.version);
            tasks.push(task);
            archives.push((dest, m.md5.clone()));
        }

        downloader
            .download_batch(
                &format!("Downloading Solder {display_title}"),
                tasks,
                10,
                cancel,
                on_progress,
            )
            .await?;

        let inst_owned = instance_dir.to_path_buf();
        let detected_forge = tokio::task::spawn_blocking(move || -> ThkResult<Option<String>> {
            for (zip_file, expected_md5) in &archives {
                if let Some(md5_hex) = expected_md5.as_deref().filter(|s| s.len() == 32) {
                    let bytes = std::fs::read(zip_file)?;
                    let actual = hex::encode(Md5::digest(&bytes));
                    if !actual.eq_ignore_ascii_case(md5_hex) {
                        return Err(ThkError::HashMismatch {
                            file: zip_file.display().to_string(),
                            expected: md5_hex.to_string(),
                            actual,
                        });
                    }
                }
                extract_zip_filtered(zip_file, &inst_owned, |_| true)?;
            }
            Ok(detect_loader_from_modpack_jar(&inst_owned))
        })
        .await
        .map_err(|e| ThkError::Process(format!("Solder extraction failed: {e}")))??;

        let _ = tokio::fs::remove_dir_all(&solder_tmp).await;

        let mc_version = manifest
            .minecraft
            .or(pack.minecraft)
            .ok_or_else(|| ThkError::InvalidInput("Technic Solder pack missing Minecraft version".into()))?;
        let loader_ver = manifest.forge.or(detected_forge);
        let loader = if loader_ver.is_some() || instance_dir.join("mods").is_dir() {
            LoaderType::Forge
        } else {
            LoaderType::Vanilla
        };

        return Ok(ImportedPackMetadata {
            name: format!("{display_title} ({build_id})"),
            game_version: mc_version,
            loader,
            loader_version: loader_ver,
            icon_url,
            recommended_ram_mb: manifest.memory.map(|m| m.max(2048)),
        });
    }

    let zip_url = pack
        .url
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| ThkError::Provider("Technic modpack provides neither Solder nor ZIP URL".into()))?;

    let tmp_zip = SafePath::resolve_child(cache_dir, format!("technic-{}-{build_id}.zip", pack.name))?;
    let mut task = DownloadTask::new(zip_url, tmp_zip.clone());
    task.label = display_title.clone();

    downloader
        .download_batch(
            &format!("Downloading {display_title}"),
            vec![task],
            1,
            cancel,
            on_progress,
        )
        .await?;

    let inst_owned = instance_dir.to_path_buf();
    let zip_clone = tmp_zip.clone();
    let detected_forge = tokio::task::spawn_blocking(move || -> ThkResult<Option<String>> {
        extract_zip_filtered(&zip_clone, &inst_owned, |_| true)?;
        flatten_single_subdir(&inst_owned)?;
        Ok(detect_loader_from_modpack_jar(&inst_owned))
    })
    .await
    .map_err(|e| ThkError::Process(format!("Technic ZIP extraction failed: {e}")))??;

    let _ = tokio::fs::remove_file(&tmp_zip).await;

    let mc_version = pack
        .minecraft
        .ok_or_else(|| ThkError::InvalidInput("Technic modpack missing Minecraft version".into()))?;

    Ok(ImportedPackMetadata {
        name: display_title,
        game_version: mc_version,
        loader: LoaderType::Forge,
        loader_version: detected_forge,
        icon_url,
        recommended_ram_mb: Some(4096),
    })
}

#[derive(Deserialize)]
struct LegacyModpackVersionJson {
    id: Option<String>,
}

fn detect_loader_from_modpack_jar(instance_dir: &Path) -> Option<String> {
    let modpack_jar = instance_dir.join("bin").join("modpack.jar");
    if !modpack_jar.is_file() {
        return None;
    }
    let raw = read_zip_entry_string(&modpack_jar, "version.json").ok()??;
    let parsed: LegacyModpackVersionJson = serde_json::from_str(&raw).ok()?;
    let id = parsed.id?;
    if let Some((_, rhs)) = id.split_once("-forge-") {
        return Some(rhs.to_string());
    }
    if let Some((_, rhs)) = id.split_once("Forge") {
        return Some(rhs.trim_start_matches('-').to_string());
    }
    None
}
