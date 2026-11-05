use serde::{Deserialize, Serialize};
use thk_core::models::{ContentKind, LoaderType, ProviderSource};
use thk_core::{DownloadManager, ThkResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchSort {
    Relevance,
    Downloads,
    Updated,
    Newest,
}

impl SearchSort {
    pub fn modrinth_index(&self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Updated => "updated",
            Self::Newest => "newest",
        }
    }

    pub fn curseforge_field(&self) -> u32 {
        match self {
            Self::Relevance => 1,
            Self::Downloads => 2,
            Self::Updated => 3,
            Self::Newest => 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub content_kind: ContentKind,
    pub game_version: Option<String>,
    pub loader: Option<LoaderType>,
    pub sort: SearchSort,
    pub offset: usize,
    pub limit: usize,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            content_kind: ContentKind::Mod,
            game_version: None,
            loader: None,
            sort: SearchSort::Downloads,
            offset: 0,
            limit: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPage {
    pub hits: Vec<ProjectSummary>,
    pub total_hits: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub provider: ProviderSource,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub author: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub categories: Vec<String>,
    pub updated_at: Option<String>,
    pub content_kind: ContentKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryImage {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetail {
    pub summary: ProjectSummary,
    pub body_markdown: String,
    pub gallery: Vec<GalleryImage>,
    pub source_url: Option<String>,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyRelation {
    Required,
    Optional,
    Incompatible,
    Embedded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySpec {
    pub project_id: String,
    pub version_id: Option<String>,
    pub relation: DependencyRelation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    pub size: u64,
    pub sha1: Option<String>,
    pub sha512: Option<String>,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectVersion {
    pub id: String,
    pub project_id: String,
    pub provider: ProviderSource,
    pub name: String,
    pub version_number: String,
    pub release_channel: String,
    pub published_at: Option<String>,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<VersionFile>,
    pub dependencies: Vec<DependencySpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedPackMetadata {
    pub name: String,
    pub game_version: String,
    pub loader: LoaderType,
    pub loader_version: Option<String>,
    pub icon_url: Option<String>,
    pub recommended_ram_mb: Option<u32>,
}

pub async fn search_provider(
    downloader: &DownloadManager,
    provider: ProviderSource,
    query: &SearchQuery,
    cf_api_key: Option<&str>,
) -> ThkResult<SearchPage> {
    match provider {
        ProviderSource::Modrinth => crate::modrinth::search_modrinth(downloader, query).await,
        ProviderSource::CurseForge => {
            crate::curseforge::search_curseforge(downloader, query, cf_api_key).await
        }
        ProviderSource::Ftb => crate::ftb::search_ftb(downloader, query).await,
        ProviderSource::Technic => crate::technic::search_technic(downloader, query).await,
        ProviderSource::Spiget => crate::spiget::search_spiget(downloader, query).await,
        ProviderSource::Local => Ok(SearchPage {
            hits: Vec::new(),
            total_hits: 0,
        }),
    }
}

pub async fn fetch_provider_project(
    downloader: &DownloadManager,
    provider: ProviderSource,
    project_id: &str,
    cf_api_key: Option<&str>,
) -> ThkResult<ProjectDetail> {
    match provider {
        ProviderSource::Modrinth => {
            crate::modrinth::get_modrinth_project(downloader, project_id).await
        }
        ProviderSource::CurseForge => {
            crate::curseforge::get_curseforge_project(downloader, project_id, cf_api_key).await
        }
        ProviderSource::Ftb => crate::ftb::get_ftb_project(downloader, project_id).await,
        ProviderSource::Technic => {
            crate::technic::get_technic_project(downloader, project_id).await
        }
        ProviderSource::Spiget => crate::spiget::get_spiget_project(downloader, project_id).await,
        ProviderSource::Local => Err(thk_core::ThkError::InvalidInput(
            "Local content does not have remote project details".into(),
        )),
    }
}

pub async fn fetch_provider_versions(
    downloader: &DownloadManager,
    provider: ProviderSource,
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<LoaderType>,
    cf_api_key: Option<&str>,
) -> ThkResult<Vec<ProjectVersion>> {
    match provider {
        ProviderSource::Modrinth => {
            crate::modrinth::get_modrinth_versions(downloader, project_id, game_version, loader)
                .await
        }
        ProviderSource::CurseForge => {
            crate::curseforge::get_curseforge_versions(
                downloader,
                project_id,
                game_version,
                loader,
                cf_api_key,
            )
            .await
        }
        ProviderSource::Ftb => crate::ftb::get_ftb_versions(downloader, project_id).await,
        ProviderSource::Technic => crate::technic::get_technic_versions(downloader, project_id).await,
        ProviderSource::Spiget => crate::spiget::get_spiget_versions(downloader, project_id).await,
        ProviderSource::Local => Ok(Vec::new()),
    }
}
