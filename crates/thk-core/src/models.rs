use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderType {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl LoaderType {
    pub const ALL: [LoaderType; 5] = [
        Self::Vanilla,
        Self::Fabric,
        Self::Quilt,
        Self::Forge,
        Self::NeoForge,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }

    pub fn from_str_lossy(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "fabric" | "fabric-loader" => Self::Fabric,
            "quilt" | "quilt-loader" => Self::Quilt,
            "forge" => Self::Forge,
            "neoforge" => Self::NeoForge,
            _ => Self::Vanilla,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameProfile {
    pub id: String,
    pub name: String,
    pub game_version: String,
    pub loader: LoaderType,
    pub loader_version: Option<String>,
    pub instance_path: PathBuf,
    pub is_custom_path: bool,
    pub ram_mb: u32,
    pub java_path: Option<PathBuf>,
    pub jvm_args: String,
    pub auto_perf_mods: bool,
    pub icon_url: Option<String>,
    pub total_play_seconds: u64,
    pub session_count: u32,
    pub crash_count: u32,
    pub last_played_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountType {
    Offline,
    Microsoft,
}

impl AccountType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Microsoft => "microsoft",
        }
    }

    pub fn from_str_lossy(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("microsoft") {
            Self::Microsoft
        } else {
            Self::Offline
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinVariant {
    Wide,
    Slim,
}

impl SkinVariant {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Wide => "wide",
            Self::Slim => "slim",
        }
    }

    pub fn from_str_lossy(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("slim") || raw.eq_ignore_ascii_case("alex") {
            Self::Slim
        } else {
            Self::Wide
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub username: String,
    pub uuid: String,
    pub account_type: AccountType,
    pub skin_variant: SkinVariant,
    pub skin_url: Option<String>,
    pub custom_skin_path: Option<PathBuf>,
    pub custom_cape_path: Option<PathBuf>,
    pub custom_elytra_path: Option<PathBuf>,
    pub token_expires_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerEngine {
    Vanilla,
    Paper,
    Purpur,
    Folia,
    Fabric,
    Forge,
    NeoForge,
    Mohist,
    Arclight,
    Sponge,
}

impl ServerEngine {
    pub const ALL: [ServerEngine; 10] = [
        Self::Paper,
        Self::Purpur,
        Self::Vanilla,
        Self::Fabric,
        Self::Forge,
        Self::NeoForge,
        Self::Folia,
        Self::Arclight,
        Self::Mohist,
        Self::Sponge,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Paper => "paper",
            Self::Purpur => "purpur",
            Self::Folia => "folia",
            Self::Fabric => "fabric",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
            Self::Mohist => "mohist",
            Self::Arclight => "arclight",
            Self::Sponge => "sponge",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Paper => "Paper",
            Self::Purpur => "Purpur",
            Self::Folia => "Folia",
            Self::Fabric => "Fabric Server",
            Self::Forge => "Forge Server",
            Self::NeoForge => "NeoForge Server",
            Self::Mohist => "Mohist",
            Self::Arclight => "Arclight",
            Self::Sponge => "SpongeVanilla",
        }
    }

    pub fn supports_plugins(&self) -> bool {
        matches!(
            self,
            Self::Paper | Self::Purpur | Self::Folia | Self::Mohist | Self::Arclight | Self::Sponge
        )
    }

    pub fn supports_mods(&self) -> bool {
        matches!(
            self,
            Self::Fabric | Self::Forge | Self::NeoForge | Self::Mohist | Self::Arclight
        )
    }

    pub fn from_str_lossy(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "paper" => Self::Paper,
            "purpur" => Self::Purpur,
            "folia" => Self::Folia,
            "fabric" => Self::Fabric,
            "forge" => Self::Forge,
            "neoforge" => Self::NeoForge,
            "mohist" => Self::Mohist,
            "arclight" => Self::Arclight,
            "sponge" => Self::Sponge,
            _ => Self::Vanilla,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedServer {
    pub id: String,
    pub name: String,
    pub engine: ServerEngine,
    pub game_version: String,
    pub server_dir: PathBuf,
    pub is_custom_dir: bool,
    pub port: u16,
    pub ram_min_mb: u32,
    pub ram_max_mb: u32,
    pub java_path: Option<PathBuf>,
    pub jvm_flags: String,
    pub jar_file: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerBookmark {
    pub id: String,
    pub name: String,
    pub address: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentKind {
    Mod,
    Modpack,
    Shader,
    ResourcePack,
    Plugin,
}

impl ContentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
            Self::Shader => "shader",
            Self::ResourcePack => "resourcepack",
            Self::Plugin => "plugin",
        }
    }

    pub fn instance_subdir(&self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::Modpack => "",
            Self::Shader => "shaderpacks",
            Self::ResourcePack => "resourcepacks",
            Self::Plugin => "plugins",
        }
    }

    pub fn from_str_lossy(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "modpack" => Self::Modpack,
            "shader" | "shaderpack" | "shaderpacks" => Self::Shader,
            "resourcepack" | "resourcepacks" => Self::ResourcePack,
            "plugin" | "plugins" => Self::Plugin,
            _ => Self::Mod,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderSource {
    Modrinth,
    CurseForge,
    Ftb,
    Technic,
    Spiget,
    Local,
}

impl ProviderSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Modrinth => "modrinth",
            Self::CurseForge => "curseforge",
            Self::Ftb => "ftb",
            Self::Technic => "technic",
            Self::Spiget => "spiget",
            Self::Local => "local",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Modrinth => "Modrinth",
            Self::CurseForge => "CurseForge",
            Self::Ftb => "FTB Modpacks",
            Self::Technic => "Technic",
            Self::Spiget => "Spiget",
            Self::Local => "Local",
        }
    }

    pub fn from_str_lossy(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "modrinth" => Self::Modrinth,
            "curseforge" => Self::CurseForge,
            "ftb" => Self::Ftb,
            "technic" => Self::Technic,
            "spiget" => Self::Spiget,
            _ => Self::Local,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledContentRecord {
    pub profile_id: String,
    pub content_kind: ContentKind,
    pub file_name: String,
    pub sha1: String,
    pub provider: ProviderSource,
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub version_number: String,
    pub icon_url: Option<String>,
    pub enabled: bool,
    pub installed_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Vi,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    Ask,
    MinimizeToTray,
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub language: Language,
    pub theme: ThemeMode,
    pub close_behavior: CloseBehavior,
    pub default_ram_mb: u32,
    pub max_concurrent_downloads: usize,
    pub discord_rpc_enabled: bool,
    pub sfx_enabled: bool,
    pub music_enabled: bool,
    pub music_volume: f32,
    pub custom_background_path: Option<PathBuf>,
    pub curseforge_api_key: Option<String>,
    pub auto_check_updates: bool,
    pub github_repo: String,
    pub discord_client_id: Option<String>,
    pub setup_completed: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: Language::Vi,
            theme: ThemeMode::Dark,
            close_behavior: CloseBehavior::Ask,
            default_ram_mb: 4096,
            max_concurrent_downloads: 12,
            discord_rpc_enabled: true,
            sfx_enabled: true,
            music_enabled: false,
            music_volume: 0.35,
            custom_background_path: None,
            curseforge_api_key: None,
            auto_check_updates: true,
            github_repo: "trihienkun/trihienkun-laucher".to_string(),
            discord_client_id: None,
            setup_completed: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyPlayStat {
    pub day_yyyy_mm_dd: String,
    pub seconds_played: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub phase: String,
    pub message: String,
    pub completed_files: usize,
    pub total_files: usize,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub bytes_per_sec: u64,
    pub percent: f32,
}
