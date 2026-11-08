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
    #[serde(default)]
    pub window_width: Option<u32>,
    #[serde(default)]
    pub window_height: Option<u32>,
    #[serde(default)]
    pub fullscreen: bool,
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
    #[serde(default)]
    pub cape_url: Option<String>,
    pub custom_skin_path: Option<PathBuf>,
    pub custom_cape_path: Option<PathBuf>,
    pub custom_elytra_path: Option<PathBuf>,
    pub token_expires_at: Option<i64>,
    #[serde(default)]
    pub oauth_client_id: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JavaRuntimeMode {
    #[default]
    OptimalGraalVm,
    MojangOfficial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MobileOrientationMode {
    #[default]
    FullSensor,
    #[serde(alias = "unspecified")]
    Auto,
    SensorLandscape,
    Landscape,
    ReverseLandscape,
    Portrait,
}

impl MobileOrientationMode {
    pub const ALL: [MobileOrientationMode; 6] = [
        Self::FullSensor,
        Self::Auto,
        Self::SensorLandscape,
        Self::Landscape,
        Self::ReverseLandscape,
        Self::Portrait,
    ];

    pub fn as_bridge_str(&self) -> &'static str {
        match self {
            Self::FullSensor => "full_sensor",
            Self::Auto => "unspecified",
            Self::SensorLandscape => "sensor_landscape",
            Self::Landscape => "landscape",
            Self::ReverseLandscape => "reverse_landscape",
            Self::Portrait => "portrait",
        }
    }

    pub fn next_cycle(&self) -> Self {
        match self {
            Self::FullSensor => Self::Auto,
            Self::Auto => Self::SensorLandscape,
            Self::SensorLandscape => Self::Landscape,
            Self::Landscape => Self::ReverseLandscape,
            Self::ReverseLandscape => Self::Portrait,
            Self::Portrait => Self::FullSensor,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MobileRendererMode {
    #[default]
    HolyGl4es,
    MobileGlues,
}

impl MobileRendererMode {
    pub const ALL: [MobileRendererMode; 2] = [Self::HolyGl4es, Self::MobileGlues];
}

pub const MIN_MOBILE_RENDER_SCALE_PCT: u32 = 50;
pub const MAX_MOBILE_RENDER_SCALE_PCT: u32 = 100;
pub const DEFAULT_MOBILE_RENDER_SCALE_PCT: u32 = 85;

pub fn sanitize_mobile_render_scale_pct(pct: u32) -> u32 {
    pct.clamp(MIN_MOBILE_RENDER_SCALE_PCT, MAX_MOBILE_RENDER_SCALE_PCT)
}

fn default_mobile_true() -> bool {
    true
}

fn default_mobile_render_scale_pct() -> u32 {
    DEFAULT_MOBILE_RENDER_SCALE_PCT
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccentPreset {
    #[default]
    Emerald,
    Cyan,
    Blue,
    Violet,
    Amber,
    Crimson,
}

impl AccentPreset {
    pub const ALL: [AccentPreset; 6] = [
        Self::Emerald,
        Self::Cyan,
        Self::Blue,
        Self::Violet,
        Self::Amber,
        Self::Crimson,
    ];

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Emerald => "Emerald",
            Self::Cyan => "Cyan",
            Self::Blue => "Blue",
            Self::Violet => "Violet",
            Self::Amber => "Amber",
            Self::Crimson => "Crimson",
        }
    }
}

pub const MIN_FONT_SCALE: f32 = 0.50;
pub const MAX_FONT_SCALE: f32 = 2.00;
pub const DEFAULT_FONT_SCALE: f32 = 1.0;

pub fn sanitize_font_scale(scale: f32) -> f32 {
    if !scale.is_finite() {
        return DEFAULT_FONT_SCALE;
    }
    ((scale * 1000.0).round() / 1000.0).clamp(MIN_FONT_SCALE, MAX_FONT_SCALE)
}

fn default_font_scale() -> f32 {
    DEFAULT_FONT_SCALE
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    #[serde(default)]
    pub hide_launcher_on_launch: bool,
    #[serde(default)]
    pub open_logs_on_launch: bool,
    #[serde(default)]
    pub high_process_priority: bool,
    #[serde(default)]
    pub low_ram_mode: bool,
    #[serde(default)]
    pub accent_preset: AccentPreset,
    #[serde(default)]
    pub java_runtime_mode: JavaRuntimeMode,
    #[serde(default = "default_font_scale")]
    pub font_scale: f32,
    #[serde(default)]
    pub mobile_orientation: MobileOrientationMode,
    #[serde(default = "default_mobile_true")]
    pub mobile_keep_alive: bool,
    #[serde(default = "default_mobile_true")]
    pub mobile_floating_overlay: bool,
    #[serde(default)]
    pub mobile_renderer: MobileRendererMode,
    #[serde(default = "default_mobile_render_scale_pct")]
    pub mobile_render_scale_pct: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: Language::Vi,
            theme: ThemeMode::Light,
            close_behavior: CloseBehavior::Ask,
            default_ram_mb: 4096,
            max_concurrent_downloads: 12,
            discord_rpc_enabled: true,
            sfx_enabled: true,
            music_enabled: true,
            music_volume: 0.35,
            custom_background_path: None,
            curseforge_api_key: None,
            auto_check_updates: true,
            github_repo: "PhamTriHien/TriHienKun_Laucher".to_string(),
            discord_client_id: None,
            setup_completed: false,
            hide_launcher_on_launch: false,
            open_logs_on_launch: false,
            high_process_priority: false,
            low_ram_mode: false,
            accent_preset: AccentPreset::Emerald,
            java_runtime_mode: JavaRuntimeMode::OptimalGraalVm,
            font_scale: DEFAULT_FONT_SCALE,
            mobile_orientation: MobileOrientationMode::FullSensor,
            mobile_keep_alive: true,
            mobile_floating_overlay: true,
            mobile_renderer: MobileRendererMode::HolyGl4es,
            mobile_render_scale_pct: DEFAULT_MOBILE_RENDER_SCALE_PCT,
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
