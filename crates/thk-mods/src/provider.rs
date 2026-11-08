use serde::{Deserialize, Serialize};
use thk_core::models::{ContentKind, InstalledContentRecord, LoaderType, ProviderSource};
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
    #[serde(default)]
    pub server_pack_file_id: Option<u64>,
}

impl ProjectVersion {
    pub fn canonical_version_label(&self) -> String {
        let raw = self.version_number.trim();
        let primary_filename = self
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())
            .map(|f| f.filename.as_str())
            .unwrap_or("");

        if raw.is_empty()
            || raw == self.name.trim()
            || is_mc_version_token(raw, &self.game_versions)
        {
            let extracted =
                extract_canonical_version_number(&self.name, primary_filename, &self.game_versions);
            if !extracted.is_empty() && !is_mc_version_token(&extracted, &self.game_versions) {
                return extracted;
            }
        }

        if !raw.is_empty() {
            strip_archive_extension(raw).to_string()
        } else {
            strip_archive_extension(&self.name).to_string()
        }
    }
}

pub fn extract_canonical_version_number(
    display_name: &str,
    file_name: &str,
    game_versions: &[String],
) -> String {
    if let Some(v) = extract_version_from_stem(display_name, game_versions, true) {
        return v;
    }
    if let Some(v) = extract_version_from_stem(file_name, game_versions, true) {
        return v;
    }
    if let Some(v) = extract_version_from_stem(display_name, game_versions, false) {
        return v;
    }
    if let Some(v) = extract_version_from_stem(file_name, game_versions, false) {
        return v;
    }
    let fallback = strip_archive_extension(display_name);
    if !fallback.is_empty() {
        fallback.to_string()
    } else {
        strip_archive_extension(file_name).to_string()
    }
}

fn strip_archive_extension(input: &str) -> &str {
    let mut s = input.trim();
    for ext in [".disabled", ".mrpack", ".zip", ".jar", ".solder"] {
        if s.len() > ext.len() && s[s.len() - ext.len()..].eq_ignore_ascii_case(ext) {
            s = s[..s.len() - ext.len()].trim();
        }
    }
    s
}

fn strip_brackets_and_copy_counters(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            while i < chars.len() && chars[i] != ']' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            out.push(' ');
            continue;
        }
        if chars[i] == '(' {
            if let Some(rel_end) = chars[i + 1..].iter().position(|&c| c == ')') {
                let inner: String = chars[i + 1..i + 1 + rel_end].iter().collect();
                let trimmed = inner.trim();
                if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
                    i += rel_end + 2;
                    out.push(' ');
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_mc_version_token(token: &str, game_versions: &[String]) -> bool {
    let clean = token.trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | ':' | ',' | ';'));
    let stripped = if clean.len() > 2 && clean[..2].eq_ignore_ascii_case("mc") {
        clean[2..].trim_start_matches(['-', '_'])
    } else {
        clean
    };
    if game_versions
        .iter()
        .any(|gv| gv.eq_ignore_ascii_case(stripped))
    {
        return true;
    }
    if clean.len() > 2
        && clean[..2].eq_ignore_ascii_case("mc")
        && (stripped.starts_with("1.") || stripped.starts_with("26."))
        && stripped.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        return true;
    }
    false
}

fn is_noise_or_loader_token(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "forge"
            | "fabric"
            | "quilt"
            | "neoforge"
            | "vanilla"
            | "modpack"
            | "server"
            | "client"
            | "release"
            | "mc"
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VersionStrength {
    Strong,
    Weak,
}

fn classify_version_token(token: &str, game_versions: &[String]) -> Option<VersionStrength> {
    if token.is_empty()
        || !token.is_ascii()
        || is_noise_or_loader_token(token)
        || is_mc_version_token(token, game_versions)
    {
        return None;
    }
    if !token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_'))
    {
        return None;
    }
    let bytes = token.as_bytes();
    if matches!(bytes[0], b'v' | b'V') && bytes.get(1).is_some_and(u8::is_ascii_digit) {
        return Some(VersionStrength::Strong);
    }
    if bytes[0].is_ascii_digit() {
        let has_dot_digit =
            token.contains('.') && bytes.iter().filter(|b| b.is_ascii_digit()).count() >= 2;
        return Some(if has_dot_digit {
            VersionStrength::Strong
        } else {
            VersionStrength::Weak
        });
    }
    None
}

fn is_trailing_qualifier(token: &str) -> bool {
    if token.is_empty() || !token.is_ascii() {
        return false;
    }
    let low = token.to_ascii_lowercase();
    if matches!(
        low.as_str(),
        "hotfix" | "hf" | "fix" | "patch" | "beta" | "alpha" | "rc" | "pre"
    ) {
        return true;
    }
    for prefix in [
        "hotfix", "hf", "rc", "beta", "alpha", "pre", "rev", "patch", "b",
    ] {
        if let Some(rest) = low.strip_prefix(prefix) {
            let rest = rest.trim_start_matches(['.', '_']);
            if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

fn extract_version_from_stem(
    raw: &str,
    game_versions: &[String],
    require_strong: bool,
) -> Option<String> {
    let stem = strip_archive_extension(raw);
    if stem.is_empty() {
        return None;
    }
    let debracketed = strip_brackets_and_copy_counters(stem);
    let mut tokens: Vec<&str> = Vec::new();
    for part in debracketed.split([' ', '-', '+']) {
        let trimmed =
            part.trim_matches(|c: char| matches!(c, '(' | ')' | ':' | ',' | ';' | '\'' | '"'));
        if trimmed.is_empty() {
            continue;
        }
        if classify_version_token(trimmed, game_versions).is_some() {
            tokens.push(trimmed);
        } else if trimmed.contains('_') {
            for sub in trimmed.split('_').filter(|s| !s.is_empty()) {
                tokens.push(sub);
            }
        } else {
            tokens.push(trimmed);
        }
    }

    let target_strength = if require_strong {
        VersionStrength::Strong
    } else {
        VersionStrength::Weak
    };

    let idx = tokens
        .iter()
        .rposition(|tok| classify_version_token(tok, game_versions) == Some(target_strength))?;

    let base = tokens[idx];
    if let Some(&next) = tokens.get(idx + 1) {
        if !is_mc_version_token(next, game_versions) && !is_noise_or_loader_token(next) {
            if is_trailing_qualifier(next) {
                return Some(format!("{base}-{next}"));
            }
            if target_strength == VersionStrength::Strong
                && next.len() <= 5
                && next.chars().all(|c| c.is_ascii_digit())
            {
                return Some(format!("{base}-{next}"));
            }
        }
    }

    Some(base.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedPackMetadata {
    pub name: String,
    pub game_version: String,
    pub loader: LoaderType,
    pub loader_version: Option<String>,
    pub icon_url: Option<String>,
    pub recommended_ram_mb: Option<u32>,
    #[serde(default)]
    pub installed_content: Vec<InstalledContentRecord>,
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
        ProviderSource::Technic => {
            crate::technic::get_technic_versions(downloader, project_id).await
        }
        ProviderSource::Spiget => crate::spiget::get_spiget_versions(downloader, project_id).await,
        ProviderSource::Local => Ok(Vec::new()),
    }
}
