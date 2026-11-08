use crate::audio::{AudioEngine, SoundEffect};
use crate::theme::{apply_font_scale, apply_theme};
use crate::updater::{
    apply_downloaded_update, check_latest_release, download_verified_update, UpdateCheckOutcome,
    UpdateReleaseInfo, VerifiedUpdate,
};
use chrono::Utc;
use egui_commonmark::CommonMarkCache;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thk_auth::{
    build_desktop_auth_challenge, complete_desktop_auth_challenge, create_offline_account,
    force_refresh_account_session, login_with_loopback_pkce, poll_device_code_login,
    reset_mojang_skin, start_device_code_login, sync_mojang_profile, upload_mojang_skin,
    DesktopAuthChallenge, DeviceCodeSession, TokenStore,
};
use thk_core::archive::read_zip_entry_bytes;
use thk_core::hash::sha1_bytes;
#[cfg(any(target_os = "android", test))]
use thk_core::models::MobileOrientationMode;
use thk_core::models::{
    Account, AccountType, AppSettings, ContentKind, DailyPlayStat, DownloadProgress, GameProfile,
    JavaRuntimeMode, Language, LoaderType, ManagedServer, ProviderSource, ServerBookmark,
    ServerEngine, SkinVariant,
};
use thk_core::{
    clear_directory_contents, compute_dir_size_bytes, copy_profile_tree_selective,
    scan_recovery_artifacts, AppPaths, Database, DownloadManager, I18n, RecoveryArtifact, SafePath,
};
use thk_java::{
    fetch_available_distro_packages, install_distro_package, scan_java_installations,
    DistroReleasePackage, JavaDistroKind, JavaInstallation,
};
use thk_launcher::{
    auto_fix_mod_from_modrinth, fetch_version_manifest, list_loader_versions, CrashAnalysisReport,
    InstanceKey, InstanceSupervisor, LaunchRequest, LauncherEvent, LoaderVersionEntry,
    VersionSummary,
};
use thk_mods::{
    check_local_content_updates, delete_local_content, delete_local_world,
    delete_server_addon_file, enrich_unmatched_via_curseforge_fingerprint,
    enrich_unmatched_via_modrinth_sha1, fetch_provider_project, fetch_provider_versions,
    import_curseforge_zip, import_mrpack_archive, import_server_addon_jar,
    inspect_modpack_for_server, install_curseforge_content, install_curseforge_modpack_version,
    install_ftb_modpack_version, install_modrinth_content, install_modrinth_modpack_version,
    install_server_provider_addon, install_spiget_plugin, install_technic_modpack_version,
    list_local_content, list_local_worlds, list_server_addon_files, loader_to_server_engine,
    populate_server_from_cloud_modpack, populate_server_from_modpack_archive,
    populate_server_from_profile, search_provider, toggle_local_content, toggle_server_addon_file,
    ContentUpdateCandidate, ImportedPackMetadata, LocalContentItem, LocalWorldInfo, ProjectDetail,
    ProjectSummary, ProjectVersion, SearchQuery, SearchSort, ServerAddonFile,
    ServerModpackInspection,
};
use thk_network::{
    detect_local_network_snapshot, detect_primary_lan_ipv4, ping_minecraft_server,
    probe_connected_network_snapshot, ConnectedNetworkSnapshot, DirectHostConfig, DirectHostStatus,
    DiscordPresenceState, DiscordRpcService, LanShareCoordinator, LanTunnelEvent,
    LanWorldAnnouncement, ServerStatusPing, TunnelStatus,
};
use thk_renderer3d::{
    detect_skin_variant, extract_vanilla_steve_skin, load_and_normalize_cape_file,
    load_and_normalize_cape_png, load_and_normalize_skin_file, load_and_normalize_skin_png,
    BackEquipmentMode, SkinPreviewViewport,
};
use thk_server::{
    add_banned_ip, add_banned_player, compress_server_entries, delete_player_data,
    delete_server_entries, delete_server_ftp_password, deploy_server_via_ftp,
    detect_arclight_loader, extract_server_zip, import_external_file_streaming,
    install_server_engine, is_eula_accepted, list_runnable_server_targets, list_server_dir,
    list_supported_versions_for_engine, load_server_ftp_password, read_banned_ips,
    read_banned_players, read_known_players, read_ops, read_server_properties,
    read_server_text_file, read_whitelist, remove_banned_ip, remove_banned_player,
    remove_op_player, remove_whitelist_player, resolve_effective_server_port,
    resolve_player_uuid_for_server, save_server_ftp_password, test_ftp_connection,
    upsert_op_player, upsert_whitelist_player, write_eula_accepted, write_server_properties_patch,
    write_server_text_file, BannedIpEntry, BannedPlayerEntry, FtpDeployConfig, FtpDeploySummary,
    KnownPlayerEntry, LocalServerSupervisor, OpEntry, ServerFileEntry, ServerResourceStats,
    ServerRuntimeState, ServerSupervisorEvent, WhitelistEntry,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveSection {
    Home,
    Modpacks,
    Servers,
    Accounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeTab {
    General,
    Mods,
    Shaders,
    ResourcePacks,
    Worlds,
    Files,
    Bookmarks,
    Logs,
    Analytics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerTab {
    Console,
    Files,
    PluginsMods,
    Properties,
    AccessControl,
    Network,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureSlot {
    Skin,
    Cape,
    Elytra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct ToastMessage {
    pub kind: ToastKind,
    pub text: String,
    pub expires_at: Instant,
}

#[derive(Clone)]
pub struct ActiveTaskEntry {
    pub id: String,
    pub progress: DownloadProgress,
    pub cancel: Option<CancellationToken>,
    pub launch_key: Option<InstanceKey>,
}

#[derive(Clone)]
pub enum DeleteTarget {
    Profile {
        profile_id: String,
        name: String,
        instance_path: PathBuf,
        is_custom_path: bool,
    },
    Server {
        server_id: String,
        name: String,
        server_dir: PathBuf,
        is_custom_dir: bool,
    },
    World {
        profile_id: String,
        instance_path: PathBuf,
        folder_name: String,
    },
    ProfileFiles {
        profile_id: String,
        instance_path: PathBuf,
        targets: Vec<String>,
    },
    ServerFiles {
        server_id: String,
        server_dir: PathBuf,
        targets: Vec<String>,
    },
}

#[derive(Clone)]
pub struct PendingDeleteModal {
    pub target: DeleteTarget,
    pub delete_files_on_disk: bool,
}

#[derive(Clone)]
pub struct DiskPackImportModalState {
    pub archive_path: PathBuf,
    pub is_mrpack: bool,
    pub format_label: String,
    pub detected_mc_version: String,
    pub detected_loader: LoaderType,
    pub detected_file_count: usize,
    pub profile_name: String,
    pub ram_mb: u32,
    pub custom_dir: Option<PathBuf>,
    pub installing: bool,
    pub minimized: bool,
    pub error_msg: Option<String>,
}

pub enum AppAsyncEvent {
    Toast {
        kind: ToastKind,
        text: String,
    },
    KeyedTaskProgress {
        task_id: String,
        progress: Option<DownloadProgress>,
        cancel: Option<CancellationToken>,
    },
    BusyStatus(Option<String>),
    Launcher(LauncherEvent),
    Server(ServerSupervisorEvent),
    Lan(LanTunnelEvent),
    MojangVersionsLoaded(Vec<VersionSummary>),
    LoaderVersionsLoaded {
        mc_version: String,
        loader: LoaderType,
        entries: Vec<LoaderVersionEntry>,
    },
    LocalContentRefreshed {
        profile_id: String,
        kind: ContentKind,
        items: Vec<LocalContentItem>,
    },
    InlineSearchCompleted {
        append: bool,
        hits: Vec<ProjectSummary>,
        total_hits: usize,
    },
    InlineProjectSelected {
        detail: ProjectDetail,
        versions: Vec<ProjectVersion>,
    },
    ContentUpdatesChecked {
        profile_id: String,
        kind: ContentKind,
        result: Result<Vec<ContentUpdateCandidate>, String>,
    },
    ContentUpdatesApplied {
        profile_id: String,
        kind: ContentKind,
        succeeded_files: Vec<String>,
        total_attempted: usize,
        first_err: Option<String>,
        cancelled: bool,
    },
    ProfileExternalFileImported {
        profile_id: String,
    },
    ProfileDuplicated {
        profile: GameProfile,
    },
    ProfileLoaderUpgraded {
        profile_id: String,
        loader: LoaderType,
        new_version: String,
    },
    ProfileFsCompleted {
        profile_id: String,
        clear_selection: bool,
        refresh_worlds: bool,
        toast: (ToastKind, String),
    },
    BookmarkPingResult {
        bookmark_id: String,
        result: Result<ServerStatusPing, String>,
    },
    DiskPackArchiveInspected(DiskPackImportModalState),
    DiskPackCustomDirPicked(PathBuf),
    DiskPackImportFinished {
        result: Result<GameProfile, String>,
        cancelled: bool,
    },
    ModpackSearchCompleted {
        seq: u64,
        append: bool,
        result: Result<(Vec<ProjectSummary>, usize), String>,
    },
    ModpackProjectLoaded {
        seq: u64,
        open_install_after: bool,
        result: Result<(ProjectDetail, Vec<ProjectVersion>), String>,
    },
    ModpackRemoteImageLoaded {
        cache_key: String,
        image: Option<egui::ColorImage>,
    },
    ModpackCustomDirPicked(PathBuf),
    ModpackInstallFinished {
        result: Result<ModpackInstallOutcome, String>,
        cancelled: bool,
    },
    EngineVersionsLoaded {
        engine: ServerEngine,
        versions: Vec<String>,
    },
    ServerJarInstalled {
        server_id: String,
        jar_file: String,
    },
    ServerExternalFileImported {
        server_id: String,
    },
    ServerFsCompleted {
        server_id: String,
        clear_selection: bool,
        toast: (ToastKind, String),
    },
    ServerAccessListUpdated {
        server_id: String,
        whitelist: Option<Vec<WhitelistEntry>>,
        banlist: Option<Vec<BannedPlayerEntry>>,
        ops_list: Option<Vec<OpEntry>>,
        banned_ips: Option<Vec<BannedIpEntry>>,
        known_players: Option<Vec<KnownPlayerEntry>>,
        live_cmd: Option<String>,
    },
    ServerAddonSearchCompleted {
        append: bool,
        hits: Vec<ProjectSummary>,
        total_hits: usize,
    },
    ServerAddonVersionsLoaded {
        summary: ProjectSummary,
        versions: Vec<ProjectVersion>,
    },
    NewServerModpackInspected(Result<(PathBuf, ServerModpackInspection), String>),
    NewServerCloudSearchCompleted {
        seq: u64,
        append: bool,
        result: Result<(Vec<ProjectSummary>, usize), String>,
    },
    NewServerCloudVersionsLoaded {
        project_id: String,
        result: Result<Vec<ProjectVersion>, String>,
    },
    ModpackServerCreated {
        result: Result<ManagedServer, String>,
        cancelled: bool,
    },
    ServerFtpTested {
        server_id: String,
        result: Result<String, String>,
    },
    ServerFtpDeployed {
        server_id: String,
        result: Result<FtpDeploySummary, String>,
        cancelled: bool,
    },
    DeviceCodeReady(DeviceCodeSession),
    MicrosoftAccountLoggedIn(Account),
    AuthError(String),
    AccountTextureSaved {
        account: Account,
        slot: TextureSlot,
    },
    AccountProfileSynced {
        account: Account,
    },
    RemoteAccountTextureCached {
        cache_key: String,
    },
    JavaScanCompleted(Vec<JavaInstallation>),
    JavaDistroPackagesLoaded {
        distro: JavaDistroKind,
        packages: Vec<DistroReleasePackage>,
    },
    UpdateCheckCompleted(Result<UpdateCheckOutcome, String>),
    UpdateDownloaded(Result<VerifiedUpdate, String>),
    UpdateApplyCompleted(Result<bool, String>),
    CustomBackgroundSaved(PathBuf),
    CustomMusicSaved,
    CacheSizeComputed(u64),
    CacheCleared(Result<u64, String>),
    RecoveryArtifactsScanned {
        artifacts: Vec<RecoveryArtifact>,
        notify_if_found: bool,
    },
    NetworkSnapshotProbed(ConnectedNetworkSnapshot),
}

pub struct NewProfileDraft {
    pub name: String,
    pub game_version: String,
    pub loader: LoaderType,
    pub loader_version: String,
    pub ram_mb: u32,
    pub auto_perf_mods: bool,
    pub custom_dir: Option<PathBuf>,
}

impl Default for NewProfileDraft {
    fn default() -> Self {
        Self {
            name: "Survival 1.21.4".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Fabric,
            loader_version: String::new(),
            ram_mb: 4096,
            auto_perf_mods: true,
            custom_dir: None,
        }
    }
}

pub struct ProfileEditDraft {
    pub loaded_for_id: String,
    pub name: String,
    pub ram_mb: u32,
    pub java_path_str: String,
    pub jvm_args: String,
    pub auto_perf_mods: bool,
    pub use_custom_resolution: bool,
    pub window_width: u32,
    pub window_height: u32,
    pub fullscreen: bool,
}

impl Default for ProfileEditDraft {
    fn default() -> Self {
        Self {
            loaded_for_id: String::new(),
            name: String::new(),
            ram_mb: 4096,
            java_path_str: String::new(),
            jvm_args: String::new(),
            auto_perf_mods: false,
            use_custom_resolution: false,
            window_width: 1600,
            window_height: 900,
            fullscreen: false,
        }
    }
}

pub struct HomeViewState {
    pub profiles: Vec<GameProfile>,
    pub selected_profile_id: Option<String>,
    pub active_tab: HomeTab,
    pub edit_draft: ProfileEditDraft,
    pub new_draft: NewProfileDraft,
    pub mojang_versions: Vec<VersionSummary>,
    pub include_snapshots: bool,
    pub loader_versions: HashMap<(String, LoaderType), Vec<LoaderVersionEntry>>,
    pub local_items: Vec<LocalContentItem>,
    pub local_filter: String,
    pub content_updates: Vec<ContentUpdateCandidate>,
    pub checking_updates: bool,
    pub show_online_browser: bool,
    pub online_provider: ProviderSource,
    pub online_query: String,
    pub online_page: usize,
    pub online_total_hits: usize,
    pub online_hits: Vec<ProjectSummary>,
    pub online_selected: Option<(ProjectDetail, Vec<ProjectVersion>)>,
    pub online_chosen_ver_idx: usize,
    pub worlds: Vec<LocalWorldInfo>,
    pub current_rel_dir: String,
    pub dir_entries: Vec<ServerFileEntry>,
    pub selected_entries: HashSet<String>,
    pub archive_name_input: String,
    pub editing_file_rel: Option<String>,
    pub editing_file_text: String,
    pub bookmarks: Vec<ServerBookmark>,
    pub bookmark_pings: HashMap<String, Result<ServerStatusPing, String>>,
    pub new_bm_name: String,
    pub new_bm_addr: String,
    pub instance_logs: HashMap<InstanceKey, Vec<String>>,
    pub log_filter: String,
    pub auto_scroll_logs: bool,
    pub daily_stats: Vec<DailyPlayStat>,
}

impl Default for HomeViewState {
    fn default() -> Self {
        Self {
            profiles: Vec::new(),
            selected_profile_id: None,
            active_tab: HomeTab::General,
            edit_draft: ProfileEditDraft::default(),
            new_draft: NewProfileDraft::default(),
            mojang_versions: Vec::new(),
            include_snapshots: false,
            loader_versions: HashMap::new(),
            local_items: Vec::new(),
            local_filter: String::new(),
            content_updates: Vec::new(),
            checking_updates: false,
            show_online_browser: false,
            online_provider: ProviderSource::Modrinth,
            online_query: String::new(),
            online_page: 0,
            online_total_hits: 0,
            online_hits: Vec::new(),
            online_selected: None,
            online_chosen_ver_idx: 0,
            worlds: Vec::new(),
            current_rel_dir: String::new(),
            dir_entries: Vec::new(),
            selected_entries: HashSet::new(),
            archive_name_input: "instance_backup.zip".into(),
            editing_file_rel: None,
            editing_file_text: String::new(),
            bookmarks: Vec::new(),
            bookmark_pings: HashMap::new(),
            new_bm_name: String::new(),
            new_bm_addr: String::new(),
            instance_logs: HashMap::new(),
            log_filter: String::new(),
            auto_scroll_logs: true,
            daily_stats: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModpackDetailTab {
    Overview,
    Versions,
    Gallery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModpackInstallTarget {
    ClientProfile,
    DedicatedServer,
}

pub enum ModpackInstallOutcome {
    NewProfile(GameProfile),
    NewServer(ManagedServer),
    AddonInstalled {
        profile_id: String,
        profile_name: String,
        kind: ContentKind,
        title: String,
        file_count: usize,
    },
}

#[derive(Clone)]
pub struct ModpackInstallModalState {
    pub content_kind: ContentKind,
    pub install_target: ModpackInstallTarget,
    pub detail: ProjectDetail,
    pub versions: Vec<ProjectVersion>,
    pub chosen_version_idx: usize,
    pub profile_name: String,
    pub ram_mb: u32,
    pub server_port: u16,
    pub accept_eula: bool,
    pub custom_dir: Option<PathBuf>,
    pub target_profile_id: Option<String>,
    pub installing: bool,
    pub minimized: bool,
    pub error_msg: Option<String>,
}

pub const MODPACKS_PER_PAGE: usize = 24;
const MAX_REMOTE_TEXTURES: usize = 180;

pub struct ModpacksViewState {
    pub provider: ProviderSource,
    pub content_kind: ContentKind,
    pub query: String,
    pub mc_version_filter: String,
    pub mc_group_filter: String,
    pub mc_version_search: String,
    pub loader_filter: Option<LoaderType>,
    pub sort: SearchSort,
    pub view_grid: bool,
    pub show_filter_sidebar: bool,
    pub page: usize,
    pub hits: Vec<ProjectSummary>,
    pub total_hits: usize,
    pub searching: bool,
    pub search_seq: u64,
    pub last_error: Option<String>,
    pub selected_preview: Option<ProjectSummary>,
    pub detail_loading: bool,
    pub detail_seq: u64,
    pub selected: Option<(ProjectDetail, Vec<ProjectVersion>)>,
    pub detail_tab: ModpackDetailTab,
    pub detail_loader_filter: Option<LoaderType>,
    pub detail_channel_filter: String,
    pub detail_mc_filter: String,
    pub lightbox_idx: Option<usize>,
    pub install_modal: Option<ModpackInstallModalState>,
    pub install_cancel: Option<CancellationToken>,
    pub image_semaphore: Arc<tokio::sync::Semaphore>,
    pub textures: HashMap<String, egui::TextureHandle>,
    pub texture_order: VecDeque<String>,
    pub pending_textures: HashSet<String>,
    pub failed_textures: HashSet<String>,
    pub md_cache: CommonMarkCache,
}

impl Default for ModpacksViewState {
    fn default() -> Self {
        Self {
            provider: ProviderSource::Modrinth,
            content_kind: ContentKind::Modpack,
            query: String::new(),
            mc_version_filter: String::new(),
            mc_group_filter: "all".into(),
            mc_version_search: String::new(),
            loader_filter: None,
            sort: SearchSort::Downloads,
            view_grid: true,
            show_filter_sidebar: true,
            page: 0,
            hits: Vec::new(),
            total_hits: 0,
            searching: false,
            search_seq: 0,
            last_error: None,
            selected_preview: None,
            detail_loading: false,
            detail_seq: 0,
            selected: None,
            detail_tab: ModpackDetailTab::Overview,
            detail_loader_filter: None,
            detail_channel_filter: "all".into(),
            detail_mc_filter: "all".into(),
            lightbox_idx: None,
            install_modal: None,
            install_cancel: None,
            image_semaphore: Arc::new(tokio::sync::Semaphore::new(6)),
            textures: HashMap::new(),
            texture_order: VecDeque::new(),
            pending_textures: HashSet::new(),
            failed_textures: HashSet::new(),
            md_cache: CommonMarkCache::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewServerSourceMode {
    StandardEngine,
    CloudModpack,
    ModpackArchive,
    ExistingProfile,
}

pub struct NewServerDraft {
    pub source_mode: NewServerSourceMode,
    pub name: String,
    pub engine: ServerEngine,
    pub game_version: String,
    pub port: u16,
    pub ram_min_mb: u32,
    pub ram_max_mb: u32,
    pub install_immediately: bool,
    pub accept_eula: bool,
    pub custom_dir: Option<PathBuf>,
    pub modpack_archive_path: Option<PathBuf>,
    pub modpack_inspection: Option<ServerModpackInspection>,
    pub source_profile_id: Option<String>,
    pub cloud_provider: ProviderSource,
    pub cloud_query: String,
    pub cloud_mc_filter: String,
    pub cloud_page: usize,
    pub cloud_total_hits: usize,
    pub cloud_hits: Vec<ProjectSummary>,
    pub cloud_searching: bool,
    pub cloud_search_seq: u64,
    pub cloud_loading_versions: bool,
    pub cloud_selected: Option<(ProjectSummary, Vec<ProjectVersion>)>,
    pub cloud_chosen_ver_idx: usize,
    pub creating: bool,
    pub create_cancel: Option<CancellationToken>,
    pub create_error: Option<String>,
}

impl Default for NewServerDraft {
    fn default() -> Self {
        Self {
            source_mode: NewServerSourceMode::StandardEngine,
            name: "TriHienKun SMP".into(),
            engine: ServerEngine::Paper,
            game_version: "1.21.4".into(),
            port: 25565,
            ram_min_mb: 1024,
            ram_max_mb: 4096,
            install_immediately: true,
            accept_eula: false,
            custom_dir: None,
            modpack_archive_path: None,
            modpack_inspection: None,
            source_profile_id: None,
            cloud_provider: ProviderSource::Modrinth,
            cloud_query: String::new(),
            cloud_mc_filter: String::new(),
            cloud_page: 0,
            cloud_total_hits: 0,
            cloud_hits: Vec::new(),
            cloud_searching: false,
            cloud_search_seq: 0,
            cloud_loading_versions: false,
            cloud_selected: None,
            cloud_chosen_ver_idx: 0,
            creating: false,
            create_cancel: None,
            create_error: None,
        }
    }
}

pub struct ServerEditDraft {
    pub loaded_for_id: String,
    pub name: String,
    pub port: u16,
    pub ram_min_mb: u32,
    pub ram_max_mb: u32,
    pub java_path_str: String,
    pub jvm_flags: String,
    pub jar_file_str: String,
}

impl Default for ServerEditDraft {
    fn default() -> Self {
        Self {
            loaded_for_id: String::new(),
            name: String::new(),
            port: 25565,
            ram_min_mb: 1024,
            ram_max_mb: 4096,
            java_path_str: String::new(),
            jvm_flags: String::new(),
            jar_file_str: String::new(),
        }
    }
}

pub struct ServersViewState {
    pub servers: Vec<ManagedServer>,
    pub selected_server_id: Option<String>,
    pub active_tab: ServerTab,
    pub new_draft: NewServerDraft,
    pub edit_draft: ServerEditDraft,
    pub engine_versions: HashMap<ServerEngine, Vec<String>>,
    pub runnable_targets: Vec<String>,
    pub console_input: String,
    pub server_logs: HashMap<String, Vec<String>>,
    pub server_stats: HashMap<String, ServerResourceStats>,
    pub server_states: HashMap<String, ServerRuntimeState>,
    pub server_tunnels: HashMap<String, TunnelStatus>,
    pub server_direct_hosts: HashMap<String, DirectHostStatus>,
    pub direct_host_port_input: String,
    pub direct_bind_ip: Option<std::net::Ipv4Addr>,
    pub direct_announce_lan: bool,
    pub direct_try_upnp: bool,
    pub network_snapshot: ConnectedNetworkSnapshot,
    pub probing_network: bool,
    pub current_rel_dir: String,
    pub dir_entries: Vec<ServerFileEntry>,
    pub selected_entries: HashSet<String>,
    pub archive_name_input: String,
    pub editing_file_rel: Option<String>,
    pub editing_file_text: String,
    pub properties_map: BTreeMap<String, String>,
    pub properties_filter: String,
    pub new_prop_key: String,
    pub new_prop_val: String,
    pub whitelist: Vec<WhitelistEntry>,
    pub banlist: Vec<BannedPlayerEntry>,
    pub ops_list: Vec<OpEntry>,
    pub banned_ips: Vec<BannedIpEntry>,
    pub known_players: Vec<KnownPlayerEntry>,
    pub new_wl_name: String,
    pub new_ban_name: String,
    pub new_ban_reason: String,
    pub new_op_name: String,
    pub new_op_level: u8,
    pub new_op_bypass_limit: bool,
    pub new_ban_ip: String,
    pub new_ban_ip_reason: String,
    pub player_filter: String,
    pub confirm_reset_player_uuid: Option<String>,
    pub cmd_history: Vec<String>,
    pub cmd_history_cursor: Option<usize>,
    pub installed_addons: Vec<ServerAddonFile>,
    pub addon_filter: String,
    pub addon_provider: ProviderSource,
    pub addon_kind: ContentKind,
    pub addon_query: String,
    pub addon_page: usize,
    pub addon_total_hits: usize,
    pub addon_hits: Vec<ProjectSummary>,
    pub addon_selected: Option<(ProjectSummary, Vec<ProjectVersion>)>,
    pub addon_chosen_ver_idx: usize,
    pub ftp_config: FtpDeployConfig,
    pub ftp_password: String,
    pub ftp_loaded_for_id: String,
    pub ftp_testing: bool,
    pub ftp_deploying: bool,
    pub ftp_deploy_cancel: Option<CancellationToken>,
    pub ftp_last_status: Option<(bool, String)>,
}

impl Default for ServersViewState {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
            selected_server_id: None,
            active_tab: ServerTab::Console,
            new_draft: NewServerDraft::default(),
            edit_draft: ServerEditDraft::default(),
            engine_versions: HashMap::new(),
            runnable_targets: Vec::new(),
            console_input: String::new(),
            server_logs: HashMap::new(),
            server_stats: HashMap::new(),
            server_states: HashMap::new(),
            server_tunnels: HashMap::new(),
            server_direct_hosts: HashMap::new(),
            direct_host_port_input: "25565".into(),
            direct_bind_ip: None,
            direct_announce_lan: true,
            direct_try_upnp: true,
            network_snapshot: detect_local_network_snapshot(),
            probing_network: false,
            current_rel_dir: String::new(),
            dir_entries: Vec::new(),
            selected_entries: HashSet::new(),
            archive_name_input: "backup.zip".into(),
            editing_file_rel: None,
            editing_file_text: String::new(),
            properties_map: BTreeMap::new(),
            properties_filter: String::new(),
            new_prop_key: String::new(),
            new_prop_val: String::new(),
            whitelist: Vec::new(),
            banlist: Vec::new(),
            ops_list: Vec::new(),
            banned_ips: Vec::new(),
            known_players: Vec::new(),
            new_wl_name: String::new(),
            new_ban_name: String::new(),
            new_ban_reason: "Banned by operator".into(),
            new_op_name: String::new(),
            new_op_level: 4,
            new_op_bypass_limit: false,
            new_ban_ip: String::new(),
            new_ban_ip_reason: "Banned by operator".into(),
            player_filter: String::new(),
            confirm_reset_player_uuid: None,
            cmd_history: Vec::new(),
            cmd_history_cursor: None,
            installed_addons: Vec::new(),
            addon_filter: String::new(),
            addon_provider: ProviderSource::Modrinth,
            addon_kind: ContentKind::Plugin,
            addon_query: String::new(),
            addon_page: 0,
            addon_total_hits: 0,
            addon_hits: Vec::new(),
            addon_selected: None,
            addon_chosen_ver_idx: 0,
            ftp_config: FtpDeployConfig::default(),
            ftp_password: String::new(),
            ftp_loaded_for_id: String::new(),
            ftp_testing: false,
            ftp_deploying: false,
            ftp_deploy_cancel: None,
            ftp_last_status: None,
        }
    }
}

pub struct AccountsViewState {
    pub accounts: Vec<Account>,
    pub selected_account_id: Option<String>,
    pub offline_username: String,
    pub offline_variant: SkinVariant,
    pub device_session: Option<DeviceCodeSession>,
    pub device_cancel: Option<CancellationToken>,
    pub desktop_challenge: Option<DesktopAuthChallenge>,
    pub desktop_redirect_input: String,
    pub custom_client_id: String,
    pub loopback_cancel: Option<CancellationToken>,
    pub skin_viewport: Option<SkinPreviewViewport>,
    pub loaded_viewport_key: String,
    pub last_back_mode_account_id: Option<String>,
    pub remote_tex_fetching: HashSet<String>,
    pub synced_profile_ids: HashSet<String>,
}

impl Default for AccountsViewState {
    fn default() -> Self {
        Self {
            accounts: Vec::new(),
            selected_account_id: None,
            offline_username: String::new(),
            offline_variant: SkinVariant::Wide,
            device_session: None,
            device_cancel: None,
            desktop_challenge: None,
            desktop_redirect_input: String::new(),
            custom_client_id: String::new(),
            loopback_cancel: None,
            skin_viewport: None,
            loaded_viewport_key: String::new(),
            last_back_mode_account_id: None,
            remote_tex_fetching: HashSet::new(),
            synced_profile_ids: HashSet::new(),
        }
    }
}

pub struct JavaModalState {
    pub installed: Vec<JavaInstallation>,
    pub selected_distro: JavaDistroKind,
    pub packages: Vec<DistroReleasePackage>,
}

impl Default for JavaModalState {
    fn default() -> Self {
        Self {
            installed: Vec::new(),
            selected_distro: JavaDistroKind::default_for_platform(),
            packages: Vec::new(),
        }
    }
}

pub struct LanModalState {
    pub scanner_active: bool,
    pub detected_world: Option<LanWorldAnnouncement>,
    pub ignored_foreign_packets: usize,
    pub manual_port_input: String,
    pub auto_open_tunnel: bool,
    pub tunnel_status: TunnelStatus,
    pub primary_lan_ip: Option<String>,
    pub direct_host_port_input: String,
    pub direct_bind_ip: Option<std::net::Ipv4Addr>,
    pub direct_announce_lan: bool,
    pub direct_try_upnp: bool,
    pub auto_open_direct_host: bool,
    pub direct_host_status: DirectHostStatus,
    pub network_snapshot: ConnectedNetworkSnapshot,
}

impl Default for LanModalState {
    fn default() -> Self {
        let snap = detect_local_network_snapshot();
        let primary_ip = snap.primary_ipv4.unwrap_or_else(detect_primary_lan_ipv4);
        Self {
            scanner_active: false,
            detected_world: None,
            ignored_foreign_packets: 0,
            manual_port_input: "25565".into(),
            auto_open_tunnel: false,
            tunnel_status: TunnelStatus::Idle,
            primary_lan_ip: Some(primary_ip.to_string()),
            direct_host_port_input: "25565".into(),
            direct_bind_ip: None,
            direct_announce_lan: true,
            direct_try_upnp: true,
            auto_open_direct_host: true,
            direct_host_status: DirectHostStatus::Idle,
            network_snapshot: snap,
        }
    }
}

#[derive(Default)]
pub struct UpdaterViewState {
    pub checking: bool,
    pub applying: bool,
    pub last_error: bool,
    pub available: Option<UpdateReleaseInfo>,
    pub downloaded_file: Option<VerifiedUpdate>,
    pub status_line: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DetachedWindowsState {
    #[cfg(not(target_os = "android"))]
    pub secondary_workspace_open: bool,
    #[cfg(not(target_os = "android"))]
    pub secondary_section: ActiveSection,
    #[cfg(not(target_os = "android"))]
    pub game_logs_window_open: bool,
    #[cfg(not(target_os = "android"))]
    pub server_console_window_open: bool,
    pub compact_modpack_filters_open: bool,
}

#[cfg_attr(target_os = "android", allow(clippy::derivable_impls))]
impl Default for DetachedWindowsState {
    fn default() -> Self {
        Self {
            #[cfg(not(target_os = "android"))]
            secondary_workspace_open: false,
            #[cfg(not(target_os = "android"))]
            secondary_section: ActiveSection::Servers,
            #[cfg(not(target_os = "android"))]
            game_logs_window_open: false,
            #[cfg(not(target_os = "android"))]
            server_console_window_open: false,
            compact_modpack_filters_open: false,
        }
    }
}

pub struct LauncherAppState {
    pub rt: tokio::runtime::Handle,
    pub egui_ctx: egui::Context,
    pub paths: AppPaths,
    pub db: Arc<Database>,
    pub downloader: DownloadManager,
    pub i18n: I18n,
    pub settings: AppSettings,
    pub audio: AudioEngine,
    pub discord_rpc: DiscordRpcService,
    pub instance_supervisor: InstanceSupervisor,
    pub server_supervisor: LocalServerSupervisor,
    pub lan_coordinator: LanShareCoordinator,
    pub ui_tx: mpsc::UnboundedSender<AppAsyncEvent>,
    pub ui_rx: mpsc::UnboundedReceiver<AppAsyncEvent>,

    pub section: ActiveSection,
    pub show_new_profile_modal: bool,
    pub show_new_server_modal: bool,
    pub show_java_modal: bool,
    pub show_lan_modal: bool,
    pub show_settings_modal: bool,
    pub show_close_confirm_modal: bool,
    pub remember_close_choice: bool,
    pub allow_immediate_quit: bool,
    pub crash_modal: Option<(String, CrashAnalysisReport)>,
    pub pending_eula_server_id: Option<(String, bool)>,
    pub pending_delete_modal: Option<PendingDeleteModal>,
    pub disk_pack_modal: Option<DiskPackImportModalState>,
    pub disk_pack_cancel: Option<CancellationToken>,
    pub recovery_artifacts: Vec<RecoveryArtifact>,
    pub confirm_clear_recovery: bool,
    pub confirm_clear_cache: bool,

    pub active_tasks: BTreeMap<String, ActiveTaskEntry>,
    pub active_progress: Option<DownloadProgress>,
    pub busy_message: Option<String>,
    pub toasts: VecDeque<ToastMessage>,
    pub custom_bg_tex: Option<(PathBuf, egui::TextureHandle)>,
    pub cached_dir_bytes: u64,
    pub pending_memory_trim: bool,
    pub font_scale_drag_dirty: bool,

    pub home: HomeViewState,
    pub modpacks: ModpacksViewState,
    pub servers: ServersViewState,
    pub accounts: AccountsViewState,
    pub java_modal: JavaModalState,
    pub lan_modal: LanModalState,
    pub updater: UpdaterViewState,
    pub detached: DetachedWindowsState,
    pub mobile_controls: thk_core::mobile_controls::MobileControlLayout,
    pub mobile_game: crate::views::mobile_game::MobileGameUiState,
    pub android_permissions: thk_core::AndroidPermissionStatus,
    pub last_android_poll: Instant,
}

impl LauncherAppState {
    pub fn initialize(
        rt: tokio::runtime::Handle,
        egui_ctx: egui::Context,
        paths: AppPaths,
        db: Arc<Database>,
        render_state: Option<&egui_wgpu::RenderState>,
    ) -> thk_core::ThkResult<Self> {
        let mut settings = db.load_settings()?;
        if db.get_kv("ui_bright_engine_v1")?.is_none() {
            if settings.theme != thk_core::models::ThemeMode::Light {
                settings.theme = thk_core::models::ThemeMode::Light;
                if db.get_kv("app_settings")?.is_some() {
                    db.save_settings(&settings)?;
                }
            }
            db.set_kv("ui_bright_engine_v1", Some("1"))?;
        }
        if db.get_kv("mobile_free_orientation_v1")?.is_none() {
            if settings.mobile_orientation != thk_core::models::MobileOrientationMode::FullSensor {
                settings.mobile_orientation = thk_core::models::MobileOrientationMode::FullSensor;
                if db.get_kv("app_settings")?.is_some() {
                    db.save_settings(&settings)?;
                }
            }
            db.set_kv("mobile_free_orientation_v1", Some("1"))?;
        }
        settings.font_scale = thk_core::models::sanitize_font_scale(settings.font_scale);
        settings.mobile_render_scale_pct =
            thk_core::models::sanitize_mobile_render_scale_pct(settings.mobile_render_scale_pct);
        let low_ram_mode = settings.low_ram_mode;
        crate::fx3d::set_low_ram_mode(low_ram_mode);
        let i18n = I18n::new(settings.language);
        let downloader = DownloadManager::new()?;
        let effective_concurrency = if settings.low_ram_mode {
            settings.max_concurrent_downloads.min(4)
        } else {
            settings.max_concurrent_downloads
        };
        downloader.set_max_concurrency(effective_concurrency);
        let audio = AudioEngine::start();
        audio.sync_music(
            settings.music_enabled && !settings.low_ram_mode,
            settings.music_volume,
            resolve_custom_bgm_file(&paths),
        );

        let discord_rpc = DiscordRpcService::start();
        if settings.discord_rpc_enabled {
            discord_rpc.set_presence(
                settings.discord_client_id.clone(),
                DiscordPresenceState::Menu,
            );
        }

        let (instance_supervisor, mut launcher_rx) =
            InstanceSupervisor::new(paths.clone(), Arc::clone(&db), downloader.clone());
        let (server_supervisor, mut server_rx) =
            LocalServerSupervisor::new(paths.clone(), downloader.clone());
        let (lan_coordinator, mut lan_rx) = LanShareCoordinator::new();

        let (ui_tx, ui_rx) = mpsc::unbounded_channel::<AppAsyncEvent>();

        {
            let tx = ui_tx.clone();
            let ctx = egui_ctx.clone();
            rt.spawn(async move {
                while let Some(ev) = launcher_rx.recv().await {
                    let _ = tx.send(AppAsyncEvent::Launcher(ev));
                    ctx.request_repaint();
                }
            });
        }
        {
            let tx = ui_tx.clone();
            let ctx = egui_ctx.clone();
            rt.spawn(async move {
                while let Some(ev) = server_rx.recv().await {
                    let _ = tx.send(AppAsyncEvent::Server(ev));
                    ctx.request_repaint();
                }
            });
        }
        {
            let tx = ui_tx.clone();
            let ctx = egui_ctx.clone();
            rt.spawn(async move {
                while let Some(ev) = lan_rx.recv().await {
                    let _ = tx.send(AppAsyncEvent::Lan(ev));
                    ctx.request_repaint();
                }
            });
        }

        let profiles = db.list_profiles().unwrap_or_default();
        let selected_profile_id = db
            .get_kv("selected_profile_id")
            .ok()
            .flatten()
            .filter(|id| profiles.iter().any(|p| &p.id == id))
            .or_else(|| profiles.first().map(|p| p.id.clone()));

        let accounts = db.list_accounts().unwrap_or_default();
        let selected_account_id = db
            .get_kv("selected_account_id")
            .ok()
            .flatten()
            .filter(|id| accounts.iter().any(|a| &a.id == id))
            .or_else(|| accounts.first().map(|a| a.id.clone()));

        let servers_list = db.list_servers().unwrap_or_default();
        let selected_server_id = servers_list.first().map(|s| s.id.clone());
        let bookmarks = db.list_bookmarks().unwrap_or_default();
        let mobile_controls =
            thk_core::mobile_controls::MobileControlLayout::load_or_default(&paths.root);

        let mut app = Self {
            rt,
            egui_ctx,
            paths,
            db,
            downloader,
            i18n,
            settings,
            audio,
            discord_rpc,
            instance_supervisor,
            server_supervisor,
            lan_coordinator,
            ui_tx,
            ui_rx,
            section: ActiveSection::Home,
            show_new_profile_modal: false,
            show_new_server_modal: false,
            show_java_modal: false,
            show_lan_modal: false,
            show_settings_modal: false,
            show_close_confirm_modal: false,
            remember_close_choice: false,
            allow_immediate_quit: false,
            crash_modal: None,
            pending_eula_server_id: None,
            pending_delete_modal: None,
            disk_pack_modal: None,
            disk_pack_cancel: None,
            recovery_artifacts: Vec::new(),
            confirm_clear_recovery: false,
            confirm_clear_cache: false,
            active_tasks: BTreeMap::new(),
            active_progress: None,
            busy_message: None,
            toasts: VecDeque::new(),
            custom_bg_tex: None,
            cached_dir_bytes: 0,
            pending_memory_trim: false,
            font_scale_drag_dirty: false,
            home: HomeViewState {
                profiles,
                selected_profile_id,
                bookmarks,
                ..HomeViewState::default()
            },
            modpacks: ModpacksViewState::default(),
            servers: ServersViewState {
                servers: servers_list,
                selected_server_id,
                ..ServersViewState::default()
            },
            accounts: AccountsViewState {
                accounts,
                selected_account_id,
                skin_viewport: render_state.map(|rs| {
                    let mut vp = SkinPreviewViewport::new(rs);
                    vp.low_power = low_ram_mode;
                    vp
                }),
                ..AccountsViewState::default()
            },
            java_modal: JavaModalState::default(),
            lan_modal: LanModalState::default(),
            updater: UpdaterViewState::default(),
            detached: DetachedWindowsState::default(),
            mobile_controls,
            mobile_game: crate::views::mobile_game::MobileGameUiState::default(),
            android_permissions: thk_core::read_android_permission_status(),
            last_android_poll: Instant::now(),
        };

        app.server_supervisor.set_prefer_optimal_java(
            app.settings.java_runtime_mode == JavaRuntimeMode::OptimalGraalVm,
        );
        crate::dialogs::sync_mobile_orientation(app.settings.mobile_orientation);
        app.sync_android_keep_alive_service();
        app.home.new_draft.ram_mb = app.settings.default_ram_mb;
        app.sync_custom_background_texture();
        app.sync_profile_edit_draft();
        app.sync_server_edit_draft();
        app.sync_selected_server_ftp_config();
        app.refresh_mojang_versions();
        app.refresh_selected_profile_tabs();
        app.refresh_selected_server_tabs();
        app.refresh_cache_size();
        app.refresh_recovery_artifacts(true);
        app.search_modpacks();
        if app.settings.auto_check_updates {
            app.check_app_updates();
        }

        Ok(app)
    }

    pub fn push_toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        let msg = text.into();
        match kind {
            ToastKind::Success => self
                .audio
                .play_sfx(self.settings.sfx_enabled, SoundEffect::Success),
            ToastKind::Error => {
                tracing::error!(notification = %msg, "Application error");
                self.audio
                    .play_sfx(self.settings.sfx_enabled, SoundEffect::Error);
            }
            ToastKind::Info => {}
        }
        if self.toasts.len() >= 5 {
            self.toasts.pop_front();
        }
        self.toasts.push_back(ToastMessage {
            kind,
            text: msg,
            expires_at: Instant::now() + Duration::from_secs(5),
        });
    }

    pub fn click_sfx(&self) {
        self.audio
            .play_sfx(self.settings.sfx_enabled, SoundEffect::Click);
    }

    pub fn copy_text_with_toast(&mut self, ctx: &egui::Context, value: impl Into<String>) {
        let text = value.into();
        if text.is_empty() {
            return;
        }
        crate::dialogs::sync_clipboard_text(ctx, &text);
        let preview = if text.chars().count() > 64 {
            let head: String = text.chars().take(61).collect();
            format!("{head}…")
        } else {
            text
        };
        let msg = if self.settings.language == Language::Vi {
            format!("Đã sao chép: {preview}")
        } else {
            format!("Copied: {preview}")
        };
        self.push_toast(ToastKind::Success, msg);
    }

    pub fn copy_launcher_log_to_clipboard(&mut self, ctx: &egui::Context) {
        const MAX_CLIPBOARD_LOG_BYTES: usize = 64 * 1024;
        let log_path = self.paths.root.join(crate::LAUNCHER_LOG_FILE_NAME);
        match crate::read_log_tail_utf8(&log_path, MAX_CLIPBOARD_LOG_BYTES) {
            Ok(content) if !content.trim().is_empty() => {
                crate::dialogs::sync_clipboard_text(ctx, &content);
                let msg = if self.settings.language == Language::Vi {
                    "Đã sao chép nhật ký ứng dụng vào khay nhớ tạm.".to_string()
                } else {
                    "Copied launcher log to clipboard.".to_string()
                };
                self.push_toast(ToastKind::Success, msg);
            }
            Ok(_) => {
                let msg = if self.settings.language == Language::Vi {
                    "Nhật ký ứng dụng hiện đang trống.".to_string()
                } else {
                    "Launcher log is currently empty.".to_string()
                };
                self.push_toast(ToastKind::Info, msg);
            }
            Err(err) => {
                let msg = if self.settings.language == Language::Vi {
                    format!("Không thể đọc tệp nhật ký: {err}")
                } else {
                    format!("Failed reading launcher log: {err}")
                };
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn open_local_folder_or_toast(&mut self, path: &Path) {
        if crate::dialogs::open_local_path(path).is_err() {
            let msg = if self.settings.language == Language::Vi {
                format!(
                    "Thư mục nội bộ: {} (dùng thẻ Quản lý Tệp tích hợp ngay trong ứng dụng)",
                    path.display()
                )
            } else {
                format!(
                    "Internal path: {} (use the built-in File Manager tab)",
                    path.display()
                )
            };
            self.push_toast(ToastKind::Info, msg);
        }
    }

    pub fn set_section(&mut self, next: ActiveSection) {
        if self.section != next {
            self.click_sfx();
            self.section = next;
            let any_server_online = self.servers.server_states.values().any(|st| {
                matches!(
                    st,
                    ServerRuntimeState::Starting | ServerRuntimeState::Online
                )
            });
            if self.settings.discord_rpc_enabled
                && self.instance_supervisor.running_keys().is_empty()
                && !any_server_online
            {
                let sec_name = match next {
                    ActiveSection::Home => "Home",
                    ActiveSection::Modpacks => "Modpacks",
                    ActiveSection::Servers => "Dedicated Servers",
                    ActiveSection::Accounts => "3D Skin & Accounts",
                };
                self.discord_rpc.set_presence(
                    self.settings.discord_client_id.clone(),
                    DiscordPresenceState::Browsing {
                        section: sec_name.to_string(),
                    },
                );
            }
        }
    }

    pub fn sync_idle_discord_presence(&self) {
        if !self.settings.discord_rpc_enabled || !self.instance_supervisor.running_keys().is_empty()
        {
            return;
        }
        let active_srv = self.servers.servers.iter().find(|s| {
            matches!(
                self.servers.server_states.get(&s.id),
                Some(ServerRuntimeState::Starting | ServerRuntimeState::Online)
            )
        });
        let state = if let Some(srv) = active_srv {
            DiscordPresenceState::HostingServer {
                name: srv.name.clone(),
                engine: srv.engine.display_name().to_string(),
            }
        } else {
            DiscordPresenceState::Menu
        };
        self.discord_rpc
            .set_presence(self.settings.discord_client_id.clone(), state);
    }

    pub fn update_keyed_task(
        &mut self,
        task_id: String,
        progress: Option<DownloadProgress>,
        cancel: Option<CancellationToken>,
        launch_key: Option<InstanceKey>,
    ) {
        match progress {
            Some(prog) => {
                let entry =
                    self.active_tasks
                        .entry(task_id.clone())
                        .or_insert_with(|| ActiveTaskEntry {
                            id: task_id,
                            progress: prog.clone(),
                            cancel: None,
                            launch_key: None,
                        });
                entry.progress = prog;
                if cancel.is_some() {
                    entry.cancel = cancel;
                }
                if launch_key.is_some() {
                    entry.launch_key = launch_key;
                }
            }
            None => {
                self.active_tasks.remove(&task_id);
            }
        }
        self.active_progress = self
            .primary_active_task()
            .map(|entry| entry.progress.clone());
    }

    pub fn primary_active_task(&self) -> Option<&ActiveTaskEntry> {
        self.active_tasks
            .get("modpack-install")
            .or_else(|| self.active_tasks.get("disk-pack-import"))
            .or_else(|| self.active_tasks.get("server-pack-create"))
            .or_else(|| self.active_tasks.values().next())
    }

    pub fn can_cancel_primary_task(&self) -> bool {
        self.primary_active_task()
            .is_some_and(|t| t.cancel.is_some() || t.launch_key.is_some())
    }

    pub fn cancel_primary_active_task(&mut self) {
        let Some(task) = self.primary_active_task().cloned() else {
            return;
        };
        if task.id == "modpack-install" {
            self.cancel_modpack_modal_install();
            return;
        }
        if task.id == "disk-pack-import" {
            self.cancel_disk_pack_import();
            return;
        }
        if task.id == "server-pack-create" {
            self.cancel_new_server_creation();
            return;
        }
        if task.id.starts_with("ftp-deploy:") {
            self.cancel_selected_server_ftp_deploy();
            return;
        }
        if let Some(ref key) = task.launch_key {
            self.instance_supervisor.stop_instance(key);
        }
        if let Some(ref tok) = task.cancel {
            tok.cancel();
        }
    }

    pub fn persist_settings(&mut self) {
        crate::fx3d::set_low_ram_mode(self.settings.low_ram_mode);
        crate::dialogs::sync_mobile_orientation(self.settings.mobile_orientation);
        self.settings.mobile_render_scale_pct = thk_core::models::sanitize_mobile_render_scale_pct(
            self.settings.mobile_render_scale_pct,
        );
        self.sync_android_keep_alive_service();
        self.server_supervisor.set_prefer_optimal_java(
            self.settings.java_runtime_mode == JavaRuntimeMode::OptimalGraalVm,
        );
        self.i18n.set_language(self.settings.language);
        let effective_concurrency = if self.settings.low_ram_mode {
            self.settings.max_concurrent_downloads.min(4)
        } else {
            self.settings.max_concurrent_downloads
        };
        self.downloader.set_max_concurrency(effective_concurrency);
        if let Some(vp) = self.accounts.skin_viewport.as_mut() {
            vp.low_power = self.settings.low_ram_mode;
            vp.mark_dirty();
        }
        self.settings.font_scale = thk_core::models::sanitize_font_scale(self.settings.font_scale);
        apply_theme(
            &self.egui_ctx,
            self.settings.theme,
            self.settings.accent_preset,
        );
        apply_font_scale(&self.egui_ctx, self.settings.font_scale);
        self.sync_custom_background_texture();
        self.sync_bgm_playback();
        if self.settings.low_ram_mode {
            self.pending_memory_trim = true;
        }
        if self.settings.discord_rpc_enabled {
            self.sync_idle_discord_presence();
        } else {
            self.discord_rpc
                .set_presence(None, DiscordPresenceState::Disabled);
        }
        if let Err(error) = self.db.save_settings(&self.settings) {
            tracing::error!(%error, "Could not persist launcher settings");
            let message = if self.settings.language == Language::Vi {
                format!("Không lưu được cài đặt. Thay đổi chỉ áp dụng trong phiên này: {error}")
            } else {
                format!("Could not save settings. Changes apply only to this session: {error}")
            };
            self.push_toast(ToastKind::Error, message);
        }
    }

    #[cfg(any(target_os = "android", test))]
    pub fn mobile_orientation_label(&self, mode: MobileOrientationMode) -> &'static str {
        match mode {
            MobileOrientationMode::FullSensor => {
                self.i18n.tr("Tự động xoay tự do", "Free Auto-Rotate")
            }
            MobileOrientationMode::Auto => self.i18n.tr("Theo cài đặt máy", "System Setting"),
            MobileOrientationMode::SensorLandscape => {
                self.i18n.tr("Ngang tự động", "Auto Landscape")
            }
            MobileOrientationMode::Landscape => self.i18n.tr("Ngang chuẩn", "Landscape"),
            MobileOrientationMode::ReverseLandscape => {
                self.i18n.tr("Ngang đảo chiều", "Flipped Landscape")
            }
            MobileOrientationMode::Portrait => self.i18n.tr("Màn hình dọc", "Portrait"),
        }
    }

    #[cfg(any(target_os = "android", test))]
    pub fn cycle_mobile_orientation(&mut self) {
        self.settings.mobile_orientation = self.settings.mobile_orientation.next_cycle();
        self.persist_settings();
        let label = self.mobile_orientation_label(self.settings.mobile_orientation);
        let msg = if self.settings.language == Language::Vi {
            format!("Chế độ xoay màn hình: {label}")
        } else {
            format!("Screen orientation: {label}")
        };
        self.push_toast(ToastKind::Info, msg);
    }

    #[cfg(any(target_os = "android", test))]
    #[cfg(target_os = "android")]
    pub fn mobile_renderer_label(
        &self,
        mode: thk_core::models::MobileRendererMode,
    ) -> &'static str {
        match mode {
            thk_core::models::MobileRendererMode::HolyGl4es => {
                self.i18n.tr("Tương thích chuẩn", "Standard Compatibility")
            }
            thk_core::models::MobileRendererMode::MobileGlues => self
                .i18n
                .tr("Hiệu năng cao (Shader)", "High Performance (Shader)"),
        }
    }

    pub fn sync_android_keep_alive_service(&self) {
        if !cfg!(target_os = "android") {
            return;
        }
        let game_active = !self.instance_supervisor.running_keys().is_empty();
        let server_active = self.servers.server_states.values().any(|st| {
            matches!(
                st,
                ServerRuntimeState::Starting | ServerRuntimeState::Online
            )
        });
        let lan_active = matches!(
            self.lan_modal.tunnel_status,
            TunnelStatus::Connecting { .. } | TunnelStatus::Running { .. }
        ) || matches!(
            self.lan_modal.direct_host_status,
            DirectHostStatus::Starting { .. } | DirectHostStatus::Running(_)
        ) || self.servers.server_tunnels.values().any(|t| {
            matches!(
                t,
                TunnelStatus::Connecting { .. } | TunnelStatus::Running { .. }
            )
        }) || self.servers.server_direct_hosts.values().any(|d| {
            matches!(
                d,
                DirectHostStatus::Starting { .. } | DirectHostStatus::Running(_)
            )
        });
        let enabled = self.settings.mobile_keep_alive || game_active || server_active || lan_active;
        let status_label = if game_active {
            self.i18n.tr(
                "Đang chơi Minecraft • Giữ kết nối mạng nền",
                "Playing Minecraft • Background network active",
            )
        } else if server_active || lan_active {
            self.i18n.tr(
                "Đang mở cổng mạng / Máy chủ • Không ngắt khi tắt màn hình",
                "Hosting LAN / Server • Screen-off keep-alive active",
            )
        } else {
            self.i18n.tr(
                "Giữ kết nối mạng ổn định khi tắt màn hình hoặc chạy ngầm",
                "Keeps network active when screen turns off or in background",
            )
        };
        let _ = thk_core::android_bridge_send_cmd(&serde_json::json!({
            "op": "set_keep_alive",
            "enabled": enabled,
            "overlay": self.settings.mobile_floating_overlay,
            "status": status_label,
        }));
    }

    #[cfg(target_os = "android")]
    pub fn request_android_ignore_battery(&mut self) {
        self.click_sfx();
        if let Err(e) = thk_core::android_bridge_send_cmd(&serde_json::json!({
            "op": "request_ignore_battery"
        })) {
            self.push_toast(ToastKind::Error, e.to_string());
        }
    }

    #[cfg(target_os = "android")]
    pub fn request_android_overlay_permission(&mut self) {
        self.click_sfx();
        if let Err(e) = thk_core::android_bridge_send_cmd(&serde_json::json!({
            "op": "request_overlay_permission"
        })) {
            self.push_toast(ToastKind::Error, e.to_string());
        }
    }

    #[cfg(target_os = "android")]
    pub fn request_android_notification_permission(&mut self) {
        self.click_sfx();
        if let Err(e) = thk_core::android_bridge_send_cmd(&serde_json::json!({
            "op": "request_notification_permission"
        })) {
            self.push_toast(ToastKind::Error, e.to_string());
        }
    }

    pub fn trim_launcher_memory(&mut self, render_state: Option<&egui_wgpu::RenderState>) {
        self.modpacks.textures.clear();
        self.modpacks.texture_order.clear();
        self.modpacks.pending_textures.clear();
        if self.settings.low_ram_mode {
            self.custom_bg_tex = None;
            for lines in self.home.instance_logs.values_mut() {
                if lines.len() > 800 {
                    let excess = lines.len() - 800;
                    lines.drain(0..excess);
                    lines.shrink_to_fit();
                }
            }
            for lines in self.servers.server_logs.values_mut() {
                if lines.len() > 800 {
                    let excess = lines.len() - 800;
                    lines.drain(0..excess);
                    lines.shrink_to_fit();
                }
            }
        }
        if self.section != ActiveSection::Accounts || render_state.is_none() {
            if let Some(vp) = self.accounts.skin_viewport.as_mut() {
                vp.release_offscreen_target(render_state);
            }
            self.accounts.loaded_viewport_key.clear();
        }
        self.egui_ctx.forget_all_images();
    }

    pub fn refresh_cache_size(&self) {
        let cache_dir = self.paths.cache_dir.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let bytes = tokio::task::spawn_blocking(move || compute_dir_size_bytes(&cache_dir))
                .await
                .unwrap_or(0);
            let _ = tx.send(AppAsyncEvent::CacheSizeComputed(bytes));
            ctx.request_repaint();
        });
    }

    pub fn clear_app_cache(&mut self) {
        let cache_dir = self.paths.cache_dir.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let res = tokio::task::spawn_blocking(move || clear_directory_contents(&cache_dir))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));
            let _ = tx.send(AppAsyncEvent::CacheCleared(res));
            ctx.request_repaint();
        });
    }

    pub fn refresh_recovery_artifacts(&self, notify_if_found: bool) {
        let paths = self.paths.clone();
        let extra_instances: Vec<PathBuf> = self
            .home
            .profiles
            .iter()
            .filter(|p| p.is_custom_path)
            .map(|p| p.instance_path.clone())
            .collect();
        let extra_servers: Vec<PathBuf> = self
            .servers
            .servers
            .iter()
            .filter(|s| s.is_custom_dir)
            .map(|s| s.server_dir.clone())
            .collect();
        let exe = std::env::current_exe().ok();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let artifacts = tokio::task::spawn_blocking(move || {
                scan_recovery_artifacts(&paths, &extra_instances, &extra_servers, exe.as_deref())
            })
            .await
            .unwrap_or_default();
            let _ = tx.send(AppAsyncEvent::RecoveryArtifactsScanned {
                artifacts,
                notify_if_found,
            });
            ctx.request_repaint();
        });
    }

    pub fn clean_recovery_artifacts(&mut self) {
        if !self.active_tasks.is_empty() || self.updater.applying {
            let msg = self
                .i18n
                .tr(
                    "Hãy chờ các tác vụ cài đặt/cập nhật đang chạy hoàn tất trước khi dọn tệp phục hồi.",
                    "Wait for active installation/update tasks to finish before cleaning recovery files.",
                )
                .to_string();
            self.push_toast(ToastKind::Info, msg);
            return;
        }
        let targets: Vec<RecoveryArtifact> = std::mem::take(&mut self.recovery_artifacts);
        self.confirm_clear_recovery = false;
        if targets.is_empty() {
            return;
        }
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let (removed, failed) = tokio::task::spawn_blocking(move || {
                let mut ok_cnt = 0usize;
                let mut err_cnt = 0usize;
                for item in targets {
                    let res = if item.path.is_dir() {
                        std::fs::remove_dir_all(&item.path)
                    } else if item.path.exists() {
                        std::fs::remove_file(&item.path)
                    } else {
                        Ok(())
                    };
                    if res.is_ok() {
                        ok_cnt += 1;
                    } else {
                        err_cnt += 1;
                    }
                }
                (ok_cnt, err_cnt)
            })
            .await
            .unwrap_or((0, 1));

            let (kind, text) = if failed == 0 {
                (
                    ToastKind::Success,
                    if lang == Language::Vi {
                        format!("Đã dọn sạch {removed} mục phục hồi giao dịch cũ.")
                    } else {
                        format!("Cleaned up {removed} leftover recovery artifact(s).")
                    },
                )
            } else {
                (
                    ToastKind::Info,
                    if lang == Language::Vi {
                        format!(
                            "Đã dọn {removed} mục ({failed} mục đang bị khóa bởi hệ điều hành)."
                        )
                    } else {
                        format!("Cleaned {removed} item(s) ({failed} locked by OS).")
                    },
                )
            };
            let _ = tx.send(AppAsyncEvent::Toast { kind, text });
            ctx.request_repaint();
        });
        self.refresh_recovery_artifacts(false);
    }

    pub fn sync_custom_background_texture(&mut self) {
        if self.settings.low_ram_mode {
            self.custom_bg_tex = None;
            return;
        }
        let Some(ref bg_path) = self.settings.custom_background_path else {
            self.custom_bg_tex = None;
            return;
        };
        if self
            .custom_bg_tex
            .as_ref()
            .is_some_and(|(loaded_p, _)| loaded_p == bg_path)
        {
            return;
        }
        let Ok(dyn_img) = image::open(bg_path) else {
            self.custom_bg_tex = None;
            return;
        };
        let rgba = dyn_img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let color_img =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
        let tex = self.egui_ctx.load_texture(
            "thk_custom_background",
            color_img,
            egui::TextureOptions::LINEAR,
        );
        self.custom_bg_tex = Some((bg_path.clone(), tex));
    }

    pub fn pick_custom_background(&mut self) {
        let bg_dir = self.paths.backgrounds_dir.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let Some(src) = crate::dialogs::pick_file_async(
                "Select Background Image (PNG / JPG / WEBP)",
                Some("Image"),
                &["png", "jpg", "jpeg", "webp"],
            )
            .await
            else {
                return;
            };
            let ext = src
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .filter(|s| matches!(s.as_str(), "png" | "jpg" | "jpeg" | "webp"))
                .unwrap_or_else(|| "png".into());
            let fname = format!("wallpaper.{ext}");
            if let Ok(dest) = SafePath::resolve_child(&bg_dir, &fname) {
                if tokio::fs::copy(&src, &dest).await.is_ok() {
                    let _ = tx.send(AppAsyncEvent::CustomBackgroundSaved(dest));
                    ctx.request_repaint();
                }
            }
        });
    }

    pub fn clear_custom_background(&mut self) {
        self.settings.custom_background_path = None;
        self.custom_bg_tex = None;
        self.persist_settings();
    }

    pub fn has_custom_music_file(&self) -> bool {
        resolve_custom_bgm_file(&self.paths).is_some()
    }

    pub fn sync_bgm_playback(&self) {
        let game_running = !self.instance_supervisor.running_keys().is_empty();
        let should_play =
            self.settings.music_enabled && !self.settings.low_ram_mode && !game_running;
        self.audio.sync_music(
            should_play,
            self.settings.music_volume,
            resolve_custom_bgm_file(&self.paths),
        );
    }

    pub fn pick_custom_music_file(&mut self) {
        let bg_dir = self.paths.backgrounds_dir.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let Some(src) = crate::dialogs::pick_file_async(
                "Select Ambient Music (MP3 / OGG / WAV)",
                Some("Audio"),
                &["mp3", "ogg", "wav"],
            )
            .await
            else {
                return;
            };
            let ext = src
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .filter(|s| matches!(s.as_str(), "mp3" | "ogg" | "wav"))
                .unwrap_or_else(|| "mp3".into());
            for old_ext in ["mp3", "ogg", "wav"] {
                if let Ok(old) = SafePath::resolve_child(&bg_dir, format!("custom_bgm.{old_ext}")) {
                    let _ = tokio::fs::remove_file(old).await;
                }
            }
            if let Ok(dest) = SafePath::resolve_child(&bg_dir, format!("custom_bgm.{ext}")) {
                if tokio::fs::copy(&src, &dest).await.is_ok() {
                    let _ = tx.send(AppAsyncEvent::CustomMusicSaved);
                    ctx.request_repaint();
                }
            }
        });
    }

    pub fn clear_custom_music_file(&mut self) {
        for ext in ["mp3", "ogg", "wav"] {
            if let Ok(p) =
                SafePath::resolve_child(&self.paths.backgrounds_dir, format!("custom_bgm.{ext}"))
            {
                let _ = std::fs::remove_file(p);
            }
        }
        self.persist_settings();
    }

    pub fn selected_profile(&self) -> Option<&GameProfile> {
        let id = self.home.selected_profile_id.as_deref()?;
        self.home.profiles.iter().find(|p| p.id == id)
    }

    pub fn selected_account(&self) -> Option<&Account> {
        let id = self.accounts.selected_account_id.as_deref()?;
        self.accounts.accounts.iter().find(|a| a.id == id)
    }

    pub fn selected_server(&self) -> Option<&ManagedServer> {
        let id = self.servers.selected_server_id.as_deref()?;
        self.servers.servers.iter().find(|s| s.id == id)
    }

    pub fn select_profile(&mut self, profile_id: String) {
        let _ = self.db.set_kv("selected_profile_id", Some(&profile_id));
        self.home.selected_profile_id = Some(profile_id);
        self.home.current_rel_dir.clear();
        self.home.editing_file_rel = None;
        self.home.selected_entries.clear();
        self.home.content_updates.clear();
        self.sync_profile_edit_draft();
        self.refresh_selected_profile_tabs();
    }

    pub fn select_account(&mut self, account_id: String) {
        let _ = self.db.set_kv("selected_account_id", Some(&account_id));
        self.accounts.selected_account_id = Some(account_id);
    }

    pub fn select_server(&mut self, server_id: String) {
        self.servers.selected_server_id = Some(server_id);
        self.servers.current_rel_dir.clear();
        self.servers.editing_file_rel = None;
        self.servers.selected_entries.clear();
        self.servers.addon_selected = None;
        self.sync_server_edit_draft();
        self.sync_selected_server_ftp_config();
        self.refresh_selected_server_tabs();
    }

    pub fn sync_server_edit_draft(&mut self) {
        if let Some(s) = self.selected_server().cloned() {
            let effective_port = resolve_effective_server_port(&s);
            let direct_busy = matches!(
                self.server_supervisor.server_direct_host_status(&s.id),
                DirectHostStatus::Running(_) | DirectHostStatus::Starting { .. }
            );
            if !direct_busy {
                self.servers.direct_host_port_input = effective_port.to_string();
            }
            self.servers.edit_draft = ServerEditDraft {
                loaded_for_id: s.id,
                name: s.name,
                port: s.port,
                ram_min_mb: s.ram_min_mb,
                ram_max_mb: s.ram_max_mb,
                java_path_str: s
                    .java_path
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
                jvm_flags: s.jvm_flags,
                jar_file_str: s.jar_file,
            };
        }
    }

    pub fn save_server_config_edits(&mut self) {
        let Some(mut srv) = self.selected_server().cloned() else {
            return;
        };
        if self.servers.edit_draft.name.trim().is_empty() {
            self.push_toast(
                ToastKind::Error,
                self.i18n.tr(
                    "Tên máy chủ không được để trống.",
                    "Server name cannot be empty.",
                ),
            );
            return;
        }
        srv.name = self.servers.edit_draft.name.trim().to_string();
        srv.port = self.servers.edit_draft.port.max(1024);
        srv.ram_min_mb = self.servers.edit_draft.ram_min_mb.max(512);
        srv.ram_max_mb = self.servers.edit_draft.ram_max_mb.max(srv.ram_min_mb);
        let j_trim = self.servers.edit_draft.java_path_str.trim();
        srv.java_path = (!j_trim.is_empty()).then(|| PathBuf::from(j_trim));
        srv.jvm_flags = self.servers.edit_draft.jvm_flags.trim().to_string();
        srv.jar_file = self.servers.edit_draft.jar_file_str.trim().to_string();

        let mut port_patch = BTreeMap::new();
        port_patch.insert("server-port".into(), srv.port.to_string());
        let _ = write_server_properties_patch(&srv.server_dir, &port_patch);
        self.servers
            .properties_map
            .insert("server-port".into(), srv.port.to_string());

        match self.db.upsert_server(&srv) {
            Ok(()) => {
                self.servers.servers = self.db.list_servers().unwrap_or_default();
                self.push_toast(
                    ToastKind::Success,
                    self.i18n.tr(
                        "Đã lưu cấu hình phần cứng & JVM máy chủ!",
                        "Saved server hardware & JVM configuration!",
                    ),
                );
            }
            Err(e) => self.push_toast(ToastKind::Error, format!("{e}")),
        }
    }

    pub fn sync_profile_edit_draft(&mut self) {
        if let Some(p) = self.selected_profile().cloned() {
            self.home.edit_draft = ProfileEditDraft {
                loaded_for_id: p.id,
                name: p.name,
                ram_mb: p.ram_mb,
                java_path_str: p
                    .java_path
                    .map(|j| j.to_string_lossy().to_string())
                    .unwrap_or_default(),
                jvm_args: p.jvm_args,
                auto_perf_mods: p.auto_perf_mods,
                use_custom_resolution: p.window_width.is_some() && p.window_height.is_some(),
                window_width: p.window_width.unwrap_or(1600),
                window_height: p.window_height.unwrap_or(900),
                fullscreen: p.fullscreen,
            };
        }
    }

    pub fn save_profile_edits(&mut self) {
        let Some(mut prof) = self.selected_profile().cloned() else {
            return;
        };
        if self.home.edit_draft.name.trim().is_empty() {
            let msg = self
                .i18n
                .tr(
                    "Tên Profile không được để trống.",
                    "Profile name cannot be empty.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        prof.name = self.home.edit_draft.name.trim().to_string();
        prof.ram_mb = self.home.edit_draft.ram_mb.max(512);
        let j_trim = self.home.edit_draft.java_path_str.trim();
        prof.java_path = if j_trim.is_empty() {
            None
        } else {
            Some(PathBuf::from(j_trim))
        };
        prof.jvm_args = self.home.edit_draft.jvm_args.trim().to_string();
        prof.auto_perf_mods = self.home.edit_draft.auto_perf_mods;
        if self.home.edit_draft.use_custom_resolution {
            prof.window_width = Some(self.home.edit_draft.window_width.clamp(640, 7680));
            prof.window_height = Some(self.home.edit_draft.window_height.clamp(480, 4320));
        } else {
            prof.window_width = None;
            prof.window_height = None;
        }
        prof.fullscreen = self.home.edit_draft.fullscreen;

        match self.db.upsert_profile(&prof) {
            Ok(()) => {
                self.home.profiles = self.db.list_profiles().unwrap_or_default();
                let msg = self
                    .i18n
                    .tr("Đã lưu cấu hình Profile.", "Saved profile configuration.")
                    .to_string();
                self.push_toast(ToastKind::Success, msg);
            }
            Err(e) => {
                let msg = format!(
                    "{}: {e}",
                    self.i18n.tr("Lỗi lưu Profile", "Failed to save profile")
                );
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn duplicate_selected_profile(&mut self) {
        let Some(src) = self.selected_profile().cloned() else {
            return;
        };
        let new_id = Uuid::new_v4().to_string();
        let copy_suffix = self.i18n.tr("Bản sao", "Copy");
        let new_name = format!("{} ({copy_suffix})", src.name);
        let slug = format!("{}-{}", SafePath::slugify_dir_name(&new_name), &new_id[..8]);
        let Ok(dst_dir) = SafePath::resolve_child(&self.paths.instances_dir, &slug) else {
            return;
        };
        let db = Arc::clone(&self.db);
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(if lang == Language::Vi {
            format!("Đang nhân bản Profile '{}'...", src.name)
        } else {
            format!("Duplicating profile '{}'...", src.name)
        });

        self.rt.spawn(async move {
            let res = tokio::task::spawn_blocking(move || -> thk_core::ThkResult<GameProfile> {
                copy_profile_tree_selective(&src.instance_path, &dst_dir)?;
                let cloned = GameProfile {
                    id: new_id.clone(),
                    name: new_name,
                    game_version: src.game_version,
                    loader: src.loader,
                    loader_version: src.loader_version,
                    instance_path: dst_dir.clone(),
                    is_custom_path: false,
                    ram_mb: src.ram_mb,
                    java_path: src.java_path,
                    jvm_args: src.jvm_args,
                    auto_perf_mods: src.auto_perf_mods,
                    icon_url: src.icon_url,
                    window_width: src.window_width,
                    window_height: src.window_height,
                    fullscreen: src.fullscreen,
                    total_play_seconds: 0,
                    session_count: 0,
                    crash_count: 0,
                    last_played_at: None,
                    created_at: Utc::now().timestamp(),
                };
                if let Err(e) = db.upsert_profile(&cloned) {
                    let _ = std::fs::remove_dir_all(&dst_dir);
                    return Err(e);
                }
                let _ = db.clone_installed_content_for_profile(&src.id, &new_id);
                Ok(cloned)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));

            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(profile) => {
                    let _ = tx.send(AppAsyncEvent::ProfileDuplicated { profile });
                }
                Err(text) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text,
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn delete_selected_profile(&mut self) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        self.pending_delete_modal = Some(PendingDeleteModal {
            delete_files_on_disk: !prof.is_custom_path,
            target: DeleteTarget::Profile {
                profile_id: prof.id,
                name: prof.name,
                instance_path: prof.instance_path,
                is_custom_path: prof.is_custom_path,
            },
        });
    }

    pub fn confirm_pending_delete(&mut self) {
        let Some(modal) = self.pending_delete_modal.take() else {
            return;
        };
        let lang = self.settings.language;
        match modal.target {
            DeleteTarget::Profile {
                profile_id,
                name,
                instance_path,
                ..
            } => {
                for snap in self.instance_supervisor.running_snapshots() {
                    if snap.key.profile_id == profile_id {
                        self.instance_supervisor.stop_instance(&snap.key);
                    }
                }
                if let Err(e) = self.db.delete_profile(&profile_id) {
                    let msg = format!(
                        "{}: {e}",
                        self.i18n
                            .tr("Không thể xóa Profile", "Failed to delete profile")
                    );
                    self.push_toast(ToastKind::Error, msg);
                    return;
                }
                self.home.profiles = self.db.list_profiles().unwrap_or_default();
                self.home.selected_profile_id = self.home.profiles.first().map(|p| p.id.clone());
                self.sync_profile_edit_draft();
                self.refresh_selected_profile_tabs();

                let shared_with_other = self
                    .home
                    .profiles
                    .iter()
                    .any(|p| p.instance_path == instance_path)
                    || self
                        .servers
                        .servers
                        .iter()
                        .any(|s| s.server_dir == instance_path);

                if modal.delete_files_on_disk
                    && !shared_with_other
                    && is_safe_managed_dir_to_delete(&self.paths, &instance_path)
                {
                    let tx = self.ui_tx.clone();
                    let ctx = self.egui_ctx.clone();
                    self.rt.spawn(async move {
                        let res = tokio::task::spawn_blocking(move || {
                            if instance_path.exists() {
                                std::fs::remove_dir_all(&instance_path)
                            } else {
                                Ok(())
                            }
                        })
                        .await;
                        let (kind, text) = match res {
                            Ok(Ok(())) => (
                                ToastKind::Info,
                                if lang == Language::Vi {
                                    format!("Đã xóa Profile '{name}' và thư mục trên đĩa.")
                                } else {
                                    format!("Deleted profile '{name}' and its instance directory.")
                                },
                            ),
                            _ => (
                                ToastKind::Info,
                                if lang == Language::Vi {
                                    format!(
                                        "Đã xóa Profile '{name}' (một số tệp trên đĩa đang bận chưa xóa hết)."
                                    )
                                } else {
                                    format!(
                                        "Deleted profile '{name}' (some files on disk were still in use)."
                                    )
                                },
                            ),
                        };
                        let _ = tx.send(AppAsyncEvent::Toast { kind, text });
                        ctx.request_repaint();
                    });
                } else {
                    let msg = if lang == Language::Vi {
                        format!("Đã xóa Profile '{name}' khỏi danh sách (giữ nguyên tệp trên đĩa).")
                    } else {
                        format!("Removed profile '{name}' (kept files on disk).")
                    };
                    self.push_toast(ToastKind::Info, msg);
                }
            }
            DeleteTarget::Server {
                server_id,
                name,
                server_dir,
                ..
            } => {
                if self.server_supervisor.runtime_state(&server_id) != ServerRuntimeState::Offline {
                    let msg = self
                        .i18n
                        .tr(
                            "Vui lòng dừng máy chủ trước khi xóa khỏi danh sách.",
                            "Please stop the server before deleting it.",
                        )
                        .to_string();
                    self.push_toast(ToastKind::Error, msg);
                    return;
                }
                self.server_supervisor.stop_server_tunnel(&server_id);
                if let Err(e) = self.db.delete_server(&server_id) {
                    self.push_toast(ToastKind::Error, e.to_string());
                    return;
                }
                let _ = self.db.set_kv(&format!("server_ftp_cfg:{server_id}"), None);
                let _ = delete_server_ftp_password(&server_id);
                self.servers.servers = self.db.list_servers().unwrap_or_default();
                self.servers.selected_server_id =
                    self.servers.servers.first().map(|s| s.id.clone());
                self.sync_server_edit_draft();
                self.sync_selected_server_ftp_config();
                self.refresh_selected_server_tabs();

                let shared_with_other = self
                    .servers
                    .servers
                    .iter()
                    .any(|s| s.server_dir == server_dir)
                    || self
                        .home
                        .profiles
                        .iter()
                        .any(|p| p.instance_path == server_dir);

                if modal.delete_files_on_disk
                    && !shared_with_other
                    && is_safe_managed_dir_to_delete(&self.paths, &server_dir)
                {
                    let tx = self.ui_tx.clone();
                    let ctx = self.egui_ctx.clone();
                    self.rt.spawn(async move {
                        let res = tokio::task::spawn_blocking(move || {
                            if server_dir.exists() {
                                std::fs::remove_dir_all(&server_dir)
                            } else {
                                Ok(())
                            }
                        })
                        .await;
                        let (kind, text) = match res {
                            Ok(Ok(())) => (
                                ToastKind::Info,
                                if lang == Language::Vi {
                                    format!("Đã xóa máy chủ '{name}' và thư mục dữ liệu trên đĩa.")
                                } else {
                                    format!("Deleted server '{name}' and its directory on disk.")
                                },
                            ),
                            _ => (
                                ToastKind::Info,
                                if lang == Language::Vi {
                                    format!(
                                        "Đã xóa máy chủ '{name}' (một số tệp trên đĩa đang bận chưa xóa hết)."
                                    )
                                } else {
                                    format!(
                                        "Deleted server '{name}' (some files on disk were still in use)."
                                    )
                                },
                            ),
                        };
                        let _ = tx.send(AppAsyncEvent::Toast { kind, text });
                        ctx.request_repaint();
                    });
                } else {
                    let msg = if lang == Language::Vi {
                        format!("Đã xóa máy chủ '{name}' khỏi danh sách (giữ nguyên tệp trên đĩa).")
                    } else {
                        format!("Removed server '{name}' (kept files on disk).")
                    };
                    self.push_toast(ToastKind::Info, msg);
                }
            }
            DeleteTarget::World {
                profile_id,
                instance_path,
                folder_name,
            } => {
                let tx = self.ui_tx.clone();
                let ctx = self.egui_ctx.clone();
                self.rt.spawn(async move {
                    let fname_clone = folder_name.clone();
                    let res = tokio::task::spawn_blocking(move || {
                        delete_local_world(&instance_path, &fname_clone)
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| e.to_string()));
                    let toast = match res {
                        Ok(()) => (
                            ToastKind::Info,
                            if lang == Language::Vi {
                                format!("Đã xóa thế giới '{folder_name}'.")
                            } else {
                                format!("Deleted world '{folder_name}'.")
                            },
                        ),
                        Err(e) => (
                            ToastKind::Error,
                            format!(
                                "{}: {e}",
                                tr_lang(lang, "Không thể xóa thế giới", "Failed to delete world")
                            ),
                        ),
                    };
                    let _ = tx.send(AppAsyncEvent::ProfileFsCompleted {
                        profile_id,
                        clear_selection: false,
                        refresh_worlds: true,
                        toast,
                    });
                    ctx.request_repaint();
                });
            }
            DeleteTarget::ProfileFiles {
                profile_id,
                instance_path,
                targets,
            } => {
                let count = targets.len();
                let tx = self.ui_tx.clone();
                let ctx = self.egui_ctx.clone();
                self.rt.spawn(async move {
                    let res = tokio::task::spawn_blocking(move || {
                        delete_server_entries(&instance_path, &targets)
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| e.to_string()));
                    let toast = match res {
                        Ok(()) => (
                            ToastKind::Info,
                            if lang == Language::Vi {
                                format!("Đã xóa {count} mục.")
                            } else {
                                format!("Deleted {count} item(s).")
                            },
                        ),
                        Err(e) => (ToastKind::Error, e),
                    };
                    let _ = tx.send(AppAsyncEvent::ProfileFsCompleted {
                        profile_id,
                        clear_selection: true,
                        refresh_worlds: true,
                        toast,
                    });
                    ctx.request_repaint();
                });
            }
            DeleteTarget::ServerFiles {
                server_id,
                server_dir,
                targets,
            } => {
                let count = targets.len();
                let tx = self.ui_tx.clone();
                let ctx = self.egui_ctx.clone();
                self.rt.spawn(async move {
                    let res = tokio::task::spawn_blocking(move || {
                        delete_server_entries(&server_dir, &targets)
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| e.to_string()));
                    let toast = match res {
                        Ok(()) => (
                            ToastKind::Info,
                            if lang == Language::Vi {
                                format!("Đã xóa {count} mục.")
                            } else {
                                format!("Deleted {count} item(s).")
                            },
                        ),
                        Err(e) => (
                            ToastKind::Error,
                            format!(
                                "{}: {e}",
                                tr_lang(lang, "Lỗi xóa tệp", "Failed to delete files")
                            ),
                        ),
                    };
                    let _ = tx.send(AppAsyncEvent::ServerFsCompleted {
                        server_id,
                        clear_selection: true,
                        toast,
                    });
                    ctx.request_repaint();
                });
            }
        }
    }

    pub fn create_profile_from_draft(&mut self) {
        let name = self.home.new_draft.name.trim().to_string();
        let game_version = self.home.new_draft.game_version.trim().to_string();
        if name.is_empty() || game_version.is_empty() {
            let msg = self
                .i18n
                .tr(
                    "Vui lòng nhập tên Profile và phiên bản Minecraft.",
                    "Please enter a profile name and Minecraft version.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        let id = Uuid::new_v4().to_string();
        let (instance_path, is_custom_path) = match self.home.new_draft.custom_dir.take() {
            Some(dir) => (dir, true),
            None => {
                let slug = format!("{}-{}", SafePath::slugify_dir_name(&name), &id[..8]);
                let Ok(default_dir) = SafePath::resolve_child(&self.paths.instances_dir, &slug)
                else {
                    let msg = self
                        .i18n
                        .tr(
                            "Đường dẫn thư mục Profile không hợp lệ.",
                            "Invalid profile directory path.",
                        )
                        .to_string();
                    self.push_toast(ToastKind::Error, msg);
                    return;
                };
                (default_dir, false)
            }
        };
        let _ = std::fs::create_dir_all(&instance_path);

        let loader = self.home.new_draft.loader;
        let loader_version = if loader == LoaderType::Vanilla
            || self.home.new_draft.loader_version.trim().is_empty()
        {
            None
        } else {
            Some(self.home.new_draft.loader_version.trim().to_string())
        };

        let prof = GameProfile {
            id: id.clone(),
            name,
            game_version,
            loader,
            loader_version,
            instance_path,
            is_custom_path,
            ram_mb: self.home.new_draft.ram_mb.max(512),
            java_path: None,
            jvm_args: String::new(),
            auto_perf_mods: loader == LoaderType::Fabric && self.home.new_draft.auto_perf_mods,
            icon_url: None,
            window_width: None,
            window_height: None,
            fullscreen: false,
            total_play_seconds: 0,
            session_count: 0,
            crash_count: 0,
            last_played_at: None,
            created_at: Utc::now().timestamp(),
        };

        match self.db.upsert_profile(&prof) {
            Ok(()) => {
                self.home.profiles = self.db.list_profiles().unwrap_or_default();
                self.show_new_profile_modal = false;
                self.select_profile(id);
                let msg = self
                    .i18n
                    .tr(
                        "Đã tạo Profile mới thành công!",
                        "Created new profile successfully!",
                    )
                    .to_string();
                self.push_toast(ToastKind::Success, msg);
            }
            Err(e) => {
                let msg = format!(
                    "{}: {e}",
                    self.i18n.tr("Lỗi tạo Profile", "Failed to create profile")
                );
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn refresh_mojang_versions(&self) {
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Ok(manifest) = fetch_version_manifest(&dl).await {
                let _ = tx.send(AppAsyncEvent::MojangVersionsLoaded(manifest.versions));
                ctx.request_repaint();
            }
        });
    }

    pub fn ensure_loader_versions(&mut self, mc_version: &str, loader: LoaderType) {
        if loader == LoaderType::Vanilla || mc_version.trim().is_empty() {
            return;
        }
        let key = (mc_version.to_string(), loader);
        if self.home.loader_versions.contains_key(&key) {
            return;
        }
        self.home.loader_versions.insert(key.clone(), Vec::new());

        let dl = self.downloader.clone();
        let mc_owned = mc_version.to_string();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Ok(entries) = list_loader_versions(&dl, loader, &mc_owned).await {
                let _ = tx.send(AppAsyncEvent::LoaderVersionsLoaded {
                    mc_version: mc_owned,
                    loader,
                    entries,
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn refresh_selected_profile_tabs(&mut self) {
        let Some(prof) = self.selected_profile().cloned() else {
            self.home.local_items.clear();
            self.home.worlds.clear();
            self.home.dir_entries.clear();
            self.home.daily_stats.clear();
            self.home.content_updates.clear();
            return;
        };

        let _ = std::fs::create_dir_all(&prof.instance_path);
        self.home.worlds = list_local_worlds(&prof.instance_path).unwrap_or_default();
        self.home.dir_entries =
            list_server_dir(&prof.instance_path, &self.home.current_rel_dir).unwrap_or_default();
        self.home.daily_stats = self
            .db
            .get_daily_play_stats(&prof.id, 14)
            .unwrap_or_default();
        self.home.content_updates.clear();

        let kind_opt = match self.home.active_tab {
            HomeTab::Mods => Some(ContentKind::Mod),
            HomeTab::Shaders => Some(ContentKind::Shader),
            HomeTab::ResourcePacks => Some(ContentKind::ResourcePack),
            _ => None,
        };

        if let Some(kind) = kind_opt {
            self.home.local_items =
                list_local_content(&self.db, &prof.id, &prof.instance_path, kind)
                    .unwrap_or_default();
            let dl = self.downloader.clone();
            let db = Arc::clone(&self.db);
            let pid = prof.id.clone();
            let inst = prof.instance_path.clone();
            let cf_key = self.settings.curseforge_api_key.clone();
            let tx = self.ui_tx.clone();
            let ctx = self.egui_ctx.clone();
            self.rt.spawn(async move {
                let m_cnt = enrich_unmatched_via_modrinth_sha1(&dl, &db, &pid, &inst, kind)
                    .await
                    .unwrap_or(0);
                let c_cnt = enrich_unmatched_via_curseforge_fingerprint(
                    &dl,
                    &db,
                    &pid,
                    &inst,
                    kind,
                    cf_key.as_deref(),
                )
                .await
                .unwrap_or(0);
                if m_cnt + c_cnt > 0 {
                    if let Ok(items) = list_local_content(&db, &pid, &inst, kind) {
                        let _ = tx.send(AppAsyncEvent::LocalContentRefreshed {
                            profile_id: pid,
                            kind,
                            items,
                        });
                        ctx.request_repaint();
                    }
                }
            });
        }
    }

    pub fn open_profile_file_editor(&mut self, rel_path: &str) {
        let Some(prof) = self.selected_profile() else {
            return;
        };
        let inst_dir = prof.instance_path.clone();
        match read_server_text_file(&inst_dir, rel_path) {
            Ok(content) => {
                self.home.editing_file_rel = Some(rel_path.to_string());
                self.home.editing_file_text = content;
            }
            Err(e) => {
                let msg = format!(
                    "{}: {e}",
                    self.i18n.tr("Không thể mở tệp", "Failed to open file")
                );
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn save_profile_file_editor(&mut self) {
        let Some(prof) = self.selected_profile() else {
            return;
        };
        let Some(rel) = self.home.editing_file_rel.clone() else {
            return;
        };
        let inst_dir = prof.instance_path.clone();
        let body = self.home.editing_file_text.clone();
        match write_server_text_file(&inst_dir, &rel, &body) {
            Ok(()) => {
                let msg = if self.settings.language == Language::Vi {
                    format!("Đã lưu tệp '{rel}'.")
                } else {
                    format!("Saved file '{rel}'.")
                };
                self.push_toast(ToastKind::Success, msg);
            }
            Err(e) => {
                let msg = format!(
                    "{}: {e}",
                    self.i18n.tr("Lỗi lưu tệp", "Failed to save file")
                );
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn import_external_file_into_profile(&mut self) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        let rel_dir = self.home.current_rel_dir.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let Some(file_path) = crate::dialogs::pick_file_async(
                tr_lang(
                    lang,
                    "Chọn tệp tải lên thư mục Profile",
                    "Select File to Upload into Profile",
                ),
                None,
                &[],
            )
            .await
            else {
                return;
            };
            match import_external_file_streaming(&prof.instance_path, &rel_dir, &file_path).await {
                Ok(_) => {
                    let _ = tx.send(AppAsyncEvent::ProfileExternalFileImported {
                        profile_id: prof.id,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!(
                            "{}: {e}",
                            tr_lang(lang, "Không thể chép tệp", "Failed to import file")
                        ),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn delete_selected_profile_files(&mut self) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        let targets: Vec<String> = self.home.selected_entries.iter().cloned().collect();
        if targets.is_empty() {
            return;
        }
        self.pending_delete_modal = Some(PendingDeleteModal {
            delete_files_on_disk: true,
            target: DeleteTarget::ProfileFiles {
                profile_id: prof.id,
                instance_path: prof.instance_path,
                targets,
            },
        });
    }

    pub fn compress_selected_profile_files(&mut self) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        let targets: Vec<String> = self.home.selected_entries.iter().cloned().collect();
        if targets.is_empty() {
            let msg = self
                .i18n
                .tr(
                    "Hãy đánh dấu ít nhất 1 tệp/thư mục để nén.",
                    "Select at least 1 file or folder to compress.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        let zip_rel = if self.home.current_rel_dir.is_empty() {
            self.home.archive_name_input.trim().to_string()
        } else {
            format!(
                "{}/{}",
                self.home.current_rel_dir,
                self.home.archive_name_input.trim()
            )
        };
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(if lang == Language::Vi {
            format!("Đang nén sao lưu '{zip_rel}'...")
        } else {
            format!("Compressing '{zip_rel}'...")
        });
        self.rt.spawn(async move {
            let zip_rel_clone = zip_rel.clone();
            let res = tokio::task::spawn_blocking(move || {
                compress_server_entries(&prof.instance_path, &targets, &zip_rel_clone)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            let (clear_selection, toast) = match res {
                Ok(_) => (
                    true,
                    (
                        ToastKind::Success,
                        if lang == Language::Vi {
                            format!("Đã nén thành '{zip_rel}'.")
                        } else {
                            format!("Compressed into '{zip_rel}'.")
                        },
                    ),
                ),
                Err(e) => (false, (ToastKind::Error, e)),
            };
            let _ = tx.send(AppAsyncEvent::ProfileFsCompleted {
                profile_id: prof.id,
                clear_selection,
                refresh_worlds: false,
                toast,
            });
            ctx.request_repaint();
        });
    }

    pub fn extract_profile_archive(&mut self, rel_zip: &str) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        let rel_owned = rel_zip.to_string();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(if lang == Language::Vi {
            format!("Đang giải nén '{rel_owned}'...")
        } else {
            format!("Extracting '{rel_owned}'...")
        });
        self.rt.spawn(async move {
            let res = tokio::task::spawn_blocking(move || {
                extract_server_zip(&prof.instance_path, &rel_owned)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            let toast = match res {
                Ok(cnt) => (
                    ToastKind::Success,
                    if lang == Language::Vi {
                        format!("Đã giải nén {cnt} tệp.")
                    } else {
                        format!("Extracted {cnt} files.")
                    },
                ),
                Err(e) => (ToastKind::Error, e),
            };
            let _ = tx.send(AppAsyncEvent::ProfileFsCompleted {
                profile_id: prof.id,
                clear_selection: false,
                refresh_worlds: true,
                toast,
            });
            ctx.request_repaint();
        });
    }

    pub fn check_profile_content_updates(&mut self, kind: ContentKind) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        if self.home.checking_updates {
            return;
        }
        self.home.checking_updates = true;
        self.home.content_updates.clear();

        let dl = self.downloader.clone();
        let db = Arc::clone(&self.db);
        let cf_key = self.settings.curseforge_api_key.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let res = check_local_content_updates(&dl, &db, &prof, kind, cf_key.as_deref())
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(AppAsyncEvent::ContentUpdatesChecked {
                profile_id: prof.id,
                kind,
                result: res,
            });
            ctx.request_repaint();
        });
    }

    pub fn apply_single_content_update(&mut self, cand: ContentUpdateCandidate, kind: ContentKind) {
        self.home
            .content_updates
            .retain(|u| u.file_name != cand.file_name);
        self.install_inline_version(cand.summary, cand.latest_version, kind);
    }

    pub fn apply_all_content_updates(&mut self, kind: ContentKind) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        let updates = self.home.content_updates.clone();
        if updates.is_empty() {
            return;
        }
        let task_id = format!("content-updates:{}:{}", prof.id, kind.as_str());
        if self.active_tasks.contains_key(&task_id) {
            return;
        }
        let dl = self.downloader.clone();
        let db = Arc::clone(&self.db);
        let cf_key = self.settings.curseforge_api_key.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        let cancel = CancellationToken::new();

        self.rt.spawn(async move {
            let total_attempted = updates.len();
            let mut succeeded_files = Vec::new();
            let mut first_err: Option<String> = None;
            let mut cancelled = false;

            for cand in updates {
                if cancel.is_cancelled() {
                    cancelled = true;
                    break;
                }
                let tx_p = tx.clone();
                let ctx_p = ctx.clone();
                let tid_p = task_id.clone();
                let cancel_p = cancel.clone();
                let on_prog = move |p: DownloadProgress| {
                    let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                        task_id: tid_p.clone(),
                        progress: Some(p),
                        cancel: Some(cancel_p.clone()),
                    });
                    ctx_p.request_repaint();
                };
                let res = match cand.summary.provider {
                    ProviderSource::CurseForge => {
                        install_curseforge_content(
                            &dl,
                            &db,
                            &prof,
                            kind,
                            &cand.summary,
                            &cand.latest_version,
                            cf_key.as_deref(),
                            Some(cancel.clone()),
                            on_prog,
                        )
                        .await
                    }
                    _ => {
                        install_modrinth_content(
                            &dl,
                            &db,
                            &prof,
                            kind,
                            &cand.summary,
                            &cand.latest_version,
                            Some(cancel.clone()),
                            on_prog,
                        )
                        .await
                    }
                };
                match res {
                    Ok(_) => succeeded_files.push(cand.file_name),
                    Err(thk_core::ThkError::Cancelled) => {
                        cancelled = true;
                        break;
                    }
                    Err(e) => {
                        if cancel.is_cancelled() {
                            cancelled = true;
                            break;
                        }
                        if first_err.is_none() {
                            first_err = Some(format!("'{}': {e}", cand.summary.title));
                        }
                    }
                }
            }

            let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                task_id,
                progress: None,
                cancel: None,
            });
            if let Ok(items) = list_local_content(&db, &prof.id, &prof.instance_path, kind) {
                let _ = tx.send(AppAsyncEvent::LocalContentRefreshed {
                    profile_id: prof.id.clone(),
                    kind,
                    items,
                });
            }
            let _ = tx.send(AppAsyncEvent::ContentUpdatesApplied {
                profile_id: prof.id,
                kind,
                succeeded_files,
                total_attempted,
                first_err,
                cancelled,
            });
            ctx.request_repaint();
        });
    }

    pub fn toggle_local_item(&mut self, kind: ContentKind, file_name: &str, enabled: bool) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        match toggle_local_content(
            &self.db,
            &prof.id,
            &prof.instance_path,
            kind,
            file_name,
            enabled,
        ) {
            Ok(_) => {
                self.home.local_items =
                    list_local_content(&self.db, &prof.id, &prof.instance_path, kind)
                        .unwrap_or_default();
            }
            Err(e) => {
                let msg = format!(
                    "{}: {e}",
                    self.i18n
                        .tr("Không thể bật/tắt tệp", "Failed to toggle file")
                );
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn delete_local_item(&mut self, kind: ContentKind, file_name: &str) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        match delete_local_content(&self.db, &prof.id, &prof.instance_path, kind, file_name) {
            Ok(()) => {
                self.home.local_items =
                    list_local_content(&self.db, &prof.id, &prof.instance_path, kind)
                        .unwrap_or_default();
                let msg = if self.settings.language == Language::Vi {
                    format!("Đã xóa '{file_name}'.")
                } else {
                    format!("Deleted '{file_name}'.")
                };
                self.push_toast(ToastKind::Info, msg);
            }
            Err(e) => {
                let msg = format!(
                    "{}: {e}",
                    self.i18n.tr("Lỗi xóa tệp", "Failed to delete file")
                );
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn delete_world_folder(&mut self, folder_name: &str) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        self.pending_delete_modal = Some(PendingDeleteModal {
            delete_files_on_disk: true,
            target: DeleteTarget::World {
                profile_id: prof.id,
                instance_path: prof.instance_path,
                folder_name: folder_name.to_string(),
            },
        });
    }

    pub fn search_inline_profile_content(&mut self, kind: ContentKind) {
        self.search_inline_profile_content_page(kind, 0, false);
    }

    pub fn search_inline_profile_content_page(
        &mut self,
        kind: ContentKind,
        page: usize,
        append: bool,
    ) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        self.home.online_page = page;
        if !append {
            self.home.online_hits.clear();
        }
        let provider = self.home.online_provider;
        let query = SearchQuery {
            query: self.home.online_query.trim().to_string(),
            content_kind: kind,
            game_version: Some(prof.game_version),
            loader: (kind == ContentKind::Mod && prof.loader != LoaderType::Vanilla)
                .then_some(prof.loader),
            sort: SearchSort::Downloads,
            offset: page * 20,
            limit: 20,
        };
        let cf_key = self.settings.curseforge_api_key.clone();
        let lang = self.settings.language;
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(if lang == Language::Vi {
            format!("Đang tìm kiếm trên {}...", provider.display_name())
        } else {
            format!("Searching on {}...", provider.display_name())
        });

        self.rt.spawn(async move {
            let res = search_provider(&dl, provider, &query, cf_key.as_deref()).await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(res_page) => {
                    let _ = tx.send(AppAsyncEvent::InlineSearchCompleted {
                        append,
                        hits: res_page.hits,
                        total_hits: res_page.total_hits,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!(
                            "{} {}: {e}",
                            tr_lang(lang, "Lỗi tìm kiếm", "Search failed on"),
                            provider.display_name()
                        ),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn load_inline_project_versions(&mut self, summary: ProjectSummary, kind: ContentKind) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        let dl = self.downloader.clone();
        let cf_key = self.settings.curseforge_api_key.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        let loader =
            (kind == ContentKind::Mod && prof.loader != LoaderType::Vanilla).then_some(prof.loader);
        self.busy_message = Some(if lang == Language::Vi {
            format!("Đang tải phiên bản của '{}'...", summary.title)
        } else {
            format!("Loading versions for '{}'...", summary.title)
        });

        self.rt.spawn(async move {
            let (det_res, ver_res) = tokio::join!(
                fetch_provider_project(
                    &dl,
                    summary.provider,
                    &summary.project_id,
                    cf_key.as_deref()
                ),
                fetch_provider_versions(
                    &dl,
                    summary.provider,
                    &summary.project_id,
                    Some(&prof.game_version),
                    loader,
                    cf_key.as_deref()
                )
            );
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match (det_res, ver_res) {
                (Ok(detail), Ok(versions)) => {
                    let _ = tx.send(AppAsyncEvent::InlineProjectSelected { detail, versions });
                }
                (Err(e), _) | (_, Err(e)) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!(
                            "{}: {e}",
                            tr_lang(
                                lang,
                                "Không thể tải thông tin gói",
                                "Failed to load package metadata"
                            )
                        ),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn install_inline_version(
        &mut self,
        summary: ProjectSummary,
        version: ProjectVersion,
        kind: ContentKind,
    ) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        let task_id = format!("inline-addon:{}:{}", prof.id, summary.project_id);
        let dl = self.downloader.clone();
        let db = Arc::clone(&self.db);
        let cf_key = self.settings.curseforge_api_key.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        let cancel = CancellationToken::new();

        self.rt.spawn(async move {
            let tx_prog = tx.clone();
            let ctx_prog = ctx.clone();
            let tid_p = task_id.clone();
            let cancel_p = cancel.clone();
            let on_prog = move |p: DownloadProgress| {
                let _ = tx_prog.send(AppAsyncEvent::KeyedTaskProgress {
                    task_id: tid_p.clone(),
                    progress: Some(p),
                    cancel: Some(cancel_p.clone()),
                });
                ctx_prog.request_repaint();
            };

            let res = match summary.provider {
                ProviderSource::CurseForge => {
                    install_curseforge_content(
                        &dl,
                        &db,
                        &prof,
                        kind,
                        &summary,
                        &version,
                        cf_key.as_deref(),
                        Some(cancel.clone()),
                        on_prog,
                    )
                    .await
                }
                _ => {
                    install_modrinth_content(
                        &dl,
                        &db,
                        &prof,
                        kind,
                        &summary,
                        &version,
                        Some(cancel.clone()),
                        on_prog,
                    )
                    .await
                }
            };

            let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                task_id,
                progress: None,
                cancel: None,
            });
            match res {
                Ok(files) => {
                    if let Ok(items) = list_local_content(&db, &prof.id, &prof.instance_path, kind)
                    {
                        let _ = tx.send(AppAsyncEvent::LocalContentRefreshed {
                            profile_id: prof.id.clone(),
                            kind,
                            items,
                        });
                    }
                    let text = if lang == Language::Vi {
                        format!(
                            "Đã cài đặt '{}' ({} tệp bao gồm thư viện phụ thuộc).",
                            summary.title,
                            files.len()
                        )
                    } else {
                        format!(
                            "Installed '{}' ({} files including dependencies).",
                            summary.title,
                            files.len()
                        )
                    };
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Success,
                        text,
                    });
                }
                Err(e) => {
                    let was_cancelled =
                        cancel.is_cancelled() || matches!(e, thk_core::ThkError::Cancelled);
                    let (kind_t, text) = if was_cancelled {
                        (
                            ToastKind::Info,
                            if lang == Language::Vi {
                                format!("Đã hủy cài đặt '{}'.", summary.title)
                            } else {
                                format!("Cancelled installation of '{}'.", summary.title)
                            },
                        )
                    } else {
                        (
                            ToastKind::Error,
                            if lang == Language::Vi {
                                format!("Cài đặt '{}' thất bại: {e}", summary.title)
                            } else {
                                format!("Failed to install '{}': {e}", summary.title)
                            },
                        )
                    };
                    let _ = tx.send(AppAsyncEvent::Toast { kind: kind_t, text });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn import_modpack_from_disk(&mut self) {
        if self.disk_pack_modal.as_ref().is_some_and(|m| m.installing) {
            if let Some(m) = self.disk_pack_modal.as_mut() {
                m.minimized = false;
            }
            return;
        }
        let default_ram = self.settings.default_ram_mb.max(4096);
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let Some(archive_path) = crate::dialogs::pick_file_async(
                tr_lang(
                    lang,
                    "Chọn gói Modpack (.mrpack hoặc CurseForge .zip)",
                    "Select Modpack Archive (.mrpack or CurseForge .zip)",
                ),
                Some("Modpack Archive"),
                &["mrpack", "zip"],
            )
            .await
            else {
                return;
            };
            let inspected = tokio::task::spawn_blocking(move || {
                inspect_disk_pack_archive(&archive_path, default_ram)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));

            match inspected {
                Ok(modal_state) => {
                    let _ = tx.send(AppAsyncEvent::DiskPackArchiveInspected(modal_state));
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!(
                            "{}: {e}",
                            tr_lang(
                                lang,
                                "Tệp gói Modpack không hợp lệ",
                                "Invalid Modpack archive"
                            )
                        ),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn pick_disk_pack_custom_dir(&mut self) {
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        let lang = self.settings.language;
        self.rt.spawn(async move {
            if let Some(folder) = crate::dialogs::pick_folder_async(tr_lang(
                lang,
                "Chọn thư mục lưu trữ Instance riêng",
                "Select Custom Instance Directory",
            ))
            .await
            {
                let _ = tx.send(AppAsyncEvent::DiskPackCustomDirPicked(folder));
                ctx.request_repaint();
            }
        });
    }

    pub fn cancel_disk_pack_import(&mut self) {
        if let Some(tok) = self.disk_pack_cancel.take() {
            tok.cancel();
        }
    }

    pub fn start_disk_modpack_import(&mut self) {
        let Some(modal_snap) = self.disk_pack_modal.clone() else {
            return;
        };
        if modal_snap.installing {
            return;
        }
        let desired_name = {
            let trimmed = modal_snap.profile_name.trim();
            if trimmed.is_empty() {
                "Imported Modpack".to_string()
            } else {
                trimmed.to_string()
            }
        };
        let desired_ram = modal_snap.ram_mb.max(2048);
        let custom_dir = modal_snap.custom_dir.clone();
        let archive_path = modal_snap.archive_path.clone();
        let is_mrpack = modal_snap.is_mrpack;

        let dl = self.downloader.clone();
        let db = Arc::clone(&self.db);
        let instances_dir = self.paths.instances_dir.clone();
        let cf_key = self.settings.curseforge_api_key.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        let cancel = CancellationToken::new();
        self.disk_pack_cancel = Some(cancel.clone());
        if let Some(m) = self.disk_pack_modal.as_mut() {
            m.installing = true;
            m.error_msg = None;
        }

        self.rt.spawn(async move {
            let id = Uuid::new_v4().to_string();
            let (inst_dir, is_custom_dir) = match custom_dir {
                Some(dir) => (dir, true),
                None => {
                    let folder =
                        format!("{}-{}", SafePath::slugify_dir_name(&desired_name), &id[..8]);
                    let Ok(default_dir) = SafePath::resolve_child(&instances_dir, &folder) else {
                        let _ = tx.send(AppAsyncEvent::DiskPackImportFinished {
                            result: Err("Invalid instance directory path".into()),
                            cancelled: false,
                        });
                        ctx.request_repaint();
                        return;
                    };
                    (default_dir, false)
                }
            };

            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let cancel_p = cancel.clone();
            let on_prog = move |p: DownloadProgress| {
                let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                    task_id: "disk-pack-import".into(),
                    progress: Some(p),
                    cancel: Some(cancel_p.clone()),
                });
                ctx_p.request_repaint();
            };

            let cancel_tok = Some(cancel.clone());
            let result = async {
                let installation = crate::pack_install::PackInstallation::prepare(&inst_dir)?;
                let mut meta = if is_mrpack {
                    import_mrpack_archive(
                        &dl,
                        &archive_path,
                        installation.payload(),
                        cancel_tok,
                        on_prog.clone(),
                    )
                    .await?
                } else {
                    import_curseforge_zip(
                        &dl,
                        &archive_path,
                        installation.payload(),
                        cf_key.as_deref(),
                        cancel_tok,
                        on_prog.clone(),
                    )
                    .await?
                };
                meta.name = desired_name;
                let installed_records = std::mem::take(&mut meta.installed_content);
                let mut profile = build_profile_from_imported_pack(id, inst_dir, meta, desired_ram);
                profile.is_custom_path = is_custom_dir;
                profile.ram_mb = desired_ram;
                let cancel_commit = cancel.clone();
                let profile =
                    tokio::task::spawn_blocking(move || -> thk_core::ThkResult<GameProfile> {
                        installation.commit_with_progress(
                            &db,
                            &profile,
                            &installed_records,
                            Some(&cancel_commit),
                            on_prog,
                        )?;
                        Ok(profile)
                    })
                    .await
                    .map_err(|e| {
                        thk_core::ThkError::Process(format!("Modpack commit worker failed: {e}"))
                    })??;
                Ok::<_, thk_core::ThkError>(profile)
            }
            .await;

            let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                task_id: "disk-pack-import".into(),
                progress: None,
                cancel: None,
            });
            match result {
                Ok(profile) => {
                    let _ = tx.send(AppAsyncEvent::DiskPackImportFinished {
                        result: Ok(profile),
                        cancelled: false,
                    });
                }
                Err(e) => {
                    let was_cancelled =
                        cancel.is_cancelled() || matches!(e, thk_core::ThkError::Cancelled);
                    let _ = tx.send(AppAsyncEvent::DiskPackImportFinished {
                        result: Err(e.to_string()),
                        cancelled: was_cancelled,
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn launch_selected_profile(&mut self, quick_play_server: Option<String>) {
        let Some(profile) = self.selected_profile().cloned() else {
            let msg = self
                .i18n
                .tr(
                    "Vui lòng chọn hoặc tạo một Profile trước.",
                    "Please select or create a profile first.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        };
        let Some(account) = self.selected_account().cloned() else {
            self.section = ActiveSection::Accounts;
            let msg = self
                .i18n
                .tr(
                    "Hãy thêm tài khoản Microsoft hoặc Offline trước khi vào game.",
                    "Please add a Microsoft or Offline account before launching.",
                )
                .to_string();
            self.push_toast(ToastKind::Info, msg);
            return;
        };

        let custom_cid = {
            let trimmed = self.accounts.custom_client_id.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };

        let mobile_surface_size = if cfg!(target_os = "android") {
            let rect = self.egui_ctx.content_rect();
            let ppp = self.egui_ctx.pixels_per_point().max(1.0);
            let w = (rect.width() * ppp).round() as u32;
            let h = (rect.height() * ppp).round() as u32;
            (w >= 320 && h >= 240).then_some((w, h))
        } else {
            None
        };

        match self.instance_supervisor.launch_async(
            &self.rt,
            LaunchRequest {
                profile: profile.clone(),
                account,
                quick_play_server,
                custom_ms_client_id: custom_cid,
                high_process_priority: self.settings.high_process_priority,
                java_runtime_mode: self.settings.java_runtime_mode,
                mobile_renderer: self.settings.mobile_renderer,
                mobile_render_scale_pct: self.settings.mobile_render_scale_pct,
                mobile_surface_size,
            },
        ) {
            Ok(_) => {
                self.audio
                    .play_sfx(self.settings.sfx_enabled, SoundEffect::Launch);
                if cfg!(target_os = "android") {
                    self.open_mobile_game_screen(false);
                    self.sync_android_keep_alive_service();
                } else if self.settings.open_logs_on_launch {
                    self.section = ActiveSection::Home;
                    self.home.active_tab = HomeTab::Logs;
                }
                if self.settings.discord_rpc_enabled {
                    self.discord_rpc.set_presence(
                        self.settings.discord_client_id.clone(),
                        DiscordPresenceState::Launching {
                            version: profile.game_version,
                        },
                    );
                }
            }
            Err(e) => self.push_toast(ToastKind::Error, e.to_string()),
        }
    }

    pub fn add_bookmark(&mut self) {
        let name = self.home.new_bm_name.trim().to_string();
        let addr = self.home.new_bm_addr.trim().to_string();
        if addr.is_empty() {
            let msg = self
                .i18n
                .tr(
                    "Vui lòng nhập địa chỉ máy chủ.",
                    "Please enter a server address.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        let bm = ServerBookmark {
            id: Uuid::new_v4().to_string(),
            name: if name.is_empty() { addr.clone() } else { name },
            address: addr,
            created_at: Utc::now().timestamp(),
        };
        if let Err(e) = self.db.upsert_bookmark(&bm) {
            let msg = format!(
                "{}: {e}",
                self.i18n
                    .tr("Lỗi lưu máy chủ", "Failed to save server bookmark")
            );
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        self.home.new_bm_name.clear();
        self.home.new_bm_addr.clear();
        self.home.bookmarks = self.db.list_bookmarks().unwrap_or_default();
        self.ping_bookmark(&bm);
    }

    pub fn delete_bookmark(&mut self, id: &str) {
        let _ = self.db.delete_bookmark(id);
        self.home.bookmark_pings.remove(id);
        self.home.bookmarks = self.db.list_bookmarks().unwrap_or_default();
    }

    pub fn ping_all_bookmarks(&self) {
        for bm in &self.home.bookmarks {
            self.ping_bookmark(bm);
        }
    }

    pub fn ping_bookmark(&self, bm: &ServerBookmark) {
        let id = bm.id.clone();
        let addr = bm.address.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let result = ping_minecraft_server(&addr, Duration::from_secs(5))
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(AppAsyncEvent::BookmarkPingResult {
                bookmark_id: id,
                result,
            });
            ctx.request_repaint();
        });
    }

    pub fn search_modpacks(&mut self) {
        self.search_modpacks_page(0, false);
    }

    pub fn search_modpacks_page(&mut self, page: usize, append: bool) {
        if !matches!(
            self.modpacks.provider,
            ProviderSource::Modrinth | ProviderSource::CurseForge
        ) {
            self.modpacks.content_kind = ContentKind::Modpack;
        }

        self.modpacks.search_seq += 1;
        let seq = self.modpacks.search_seq;
        self.modpacks.page = page;
        self.modpacks.searching = true;
        self.modpacks.last_error = None;
        if !append {
            self.modpacks.hits.clear();
        }

        let provider = self.modpacks.provider;
        let content_kind = self.modpacks.content_kind;
        let gv = {
            let t = self.modpacks.mc_version_filter.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        let loader = matches!(content_kind, ContentKind::Modpack | ContentKind::Mod)
            .then_some(self.modpacks.loader_filter)
            .flatten();

        let query = SearchQuery {
            query: self.modpacks.query.trim().to_string(),
            content_kind,
            game_version: gv,
            loader,
            sort: self.modpacks.sort,
            offset: page * MODPACKS_PER_PAGE,
            limit: MODPACKS_PER_PAGE,
        };

        let cf_key = self.settings.curseforge_api_key.clone();
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let res = search_provider(&dl, provider, &query, cf_key.as_deref())
                .await
                .map(|p| (p.hits, p.total_hits))
                .map_err(|e| e.to_string());
            let _ = tx.send(AppAsyncEvent::ModpackSearchCompleted {
                seq,
                append,
                result: res,
            });
            ctx.request_repaint();
        });
    }

    pub fn reset_modpack_filters(&mut self) {
        self.modpacks.query.clear();
        self.modpacks.mc_version_filter.clear();
        self.modpacks.mc_group_filter = "all".into();
        self.modpacks.mc_version_search.clear();
        self.modpacks.loader_filter = None;
        self.modpacks.sort = SearchSort::Downloads;
        self.search_modpacks();
    }

    pub fn open_modpack_detail(&mut self, summary: ProjectSummary) {
        self.load_modpack_project(summary, false);
    }

    pub fn quick_install_from_card(&mut self, summary: ProjectSummary) {
        self.load_modpack_project(summary, true);
    }

    fn load_modpack_project(&mut self, summary: ProjectSummary, open_install_after: bool) {
        self.modpacks.detail_seq += 1;
        let seq = self.modpacks.detail_seq;
        self.modpacks.selected = None;
        self.modpacks.selected_preview = Some(summary.clone());
        self.modpacks.detail_loading = true;
        self.modpacks.lightbox_idx = None;
        self.modpacks.detail_tab = ModpackDetailTab::Overview;

        let dl = self.downloader.clone();
        let cf_key = self.settings.curseforge_api_key.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let (d_res, v_res) = tokio::join!(
                fetch_provider_project(
                    &dl,
                    summary.provider,
                    &summary.project_id,
                    cf_key.as_deref()
                ),
                fetch_provider_versions(
                    &dl,
                    summary.provider,
                    &summary.project_id,
                    None,
                    None,
                    cf_key.as_deref()
                )
            );
            let result = match (d_res, v_res) {
                (Ok(detail), Ok(versions)) => Ok((detail, versions)),
                (Err(e), _) | (_, Err(e)) => Err(e.to_string()),
            };
            let _ = tx.send(AppAsyncEvent::ModpackProjectLoaded {
                seq,
                open_install_after,
                result,
            });
            ctx.request_repaint();
        });
    }

    pub fn open_modpack_install_modal(&mut self, preferred_version_id: Option<&str>) {
        if self
            .modpacks
            .install_modal
            .as_ref()
            .is_some_and(|m| m.installing)
        {
            if let Some(m) = self.modpacks.install_modal.as_mut() {
                m.minimized = false;
            }
            let msg = self
                .i18n
                .tr(
                    "Đang có tiến trình cài đặt chạy nền.",
                    "An installation is already running in the background.",
                )
                .to_string();
            self.push_toast(ToastKind::Info, msg);
            return;
        }

        let Some((detail, versions)) = self.modpacks.selected.clone() else {
            return;
        };
        if versions.is_empty() {
            let msg = self
                .i18n
                .tr(
                    "Dự án này hiện chưa có tệp phiên bản khả dụng.",
                    "No downloadable versions are available for this project.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }

        let gv_pref = self.modpacks.mc_version_filter.trim();
        let ldr_pref = self.modpacks.loader_filter;

        let chosen_version_idx = if let Some(pref_id) = preferred_version_id {
            versions.iter().position(|v| v.id == pref_id).unwrap_or(0)
        } else {
            let matches_prefs = |v: &ProjectVersion| {
                let gv_ok = gv_pref.is_empty() || v.game_versions.iter().any(|g| g == gv_pref);
                let ldr_ok = match ldr_pref {
                    Some(ldr) => {
                        v.loaders.is_empty()
                            || v.loaders
                                .iter()
                                .any(|l| l.eq_ignore_ascii_case(ldr.as_str()))
                    }
                    None => true,
                };
                gv_ok && ldr_ok
            };
            versions
                .iter()
                .position(|v| matches_prefs(v) && v.release_channel.eq_ignore_ascii_case("release"))
                .or_else(|| versions.iter().position(matches_prefs))
                .or_else(|| {
                    versions
                        .iter()
                        .position(|v| v.release_channel.eq_ignore_ascii_case("release"))
                })
                .unwrap_or(0)
        };

        let content_kind = detail.summary.content_kind;
        let chosen_ver = &versions[chosen_version_idx];
        let is_profile_compatible = |p: &GameProfile| {
            let gv_ok = chosen_ver.game_versions.is_empty()
                || chosen_ver
                    .game_versions
                    .iter()
                    .any(|g| g == &p.game_version);
            let ldr_ok = if content_kind == ContentKind::Mod {
                chosen_ver.loaders.is_empty()
                    || chosen_ver
                        .loaders
                        .iter()
                        .any(|l| l.eq_ignore_ascii_case(p.loader.as_str()))
            } else {
                true
            };
            gv_ok && ldr_ok
        };

        let target_profile_id = self
            .selected_profile()
            .filter(|p| is_profile_compatible(p))
            .map(|p| p.id.clone())
            .or_else(|| {
                self.home
                    .profiles
                    .iter()
                    .find(|p| is_profile_compatible(p))
                    .map(|p| p.id.clone())
            })
            .or_else(|| self.selected_profile().map(|p| p.id.clone()))
            .or_else(|| self.home.profiles.first().map(|p| p.id.clone()));

        self.modpacks.install_modal = Some(ModpackInstallModalState {
            content_kind,
            install_target: ModpackInstallTarget::ClientProfile,
            profile_name: detail.summary.title.clone(),
            ram_mb: self.settings.default_ram_mb.max(4096),
            server_port: 25565,
            accept_eula: false,
            custom_dir: None,
            target_profile_id,
            detail,
            versions,
            chosen_version_idx,
            installing: false,
            minimized: false,
            error_msg: None,
        });
    }

    pub fn pick_modpack_custom_dir(&mut self) {
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        let lang = self.settings.language;
        let for_server = self.modpacks.install_modal.as_ref().is_some_and(|m| {
            m.content_kind == ContentKind::Modpack
                && m.install_target == ModpackInstallTarget::DedicatedServer
        });
        self.rt.spawn(async move {
            let dialog_title = if for_server {
                tr_lang(
                    lang,
                    "Chọn thư mục lưu trữ Máy chủ riêng",
                    "Select Custom Server Directory",
                )
            } else {
                tr_lang(
                    lang,
                    "Chọn thư mục cài đặt Modpack riêng",
                    "Select Custom Modpack Directory",
                )
            };
            if let Some(folder) = crate::dialogs::pick_folder_async(dialog_title).await {
                let _ = tx.send(AppAsyncEvent::ModpackCustomDirPicked(folder));
                ctx.request_repaint();
            }
        });
    }

    pub fn cancel_modpack_modal_install(&mut self) {
        if let Some(tok) = self.modpacks.install_cancel.take() {
            tok.cancel();
        }
    }

    pub fn start_modpack_modal_install(&mut self) {
        let Some(modal_snapshot) = self.modpacks.install_modal.clone() else {
            return;
        };
        if modal_snapshot.installing {
            return;
        }
        let Some(version) = modal_snapshot
            .versions
            .get(modal_snapshot.chosen_version_idx)
            .cloned()
        else {
            let msg = self
                .i18n
                .tr(
                    "Vui lòng chọn một phiên bản hợp lệ để cài đặt.",
                    "Please choose a valid version to install.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        };

        let dl = self.downloader.clone();
        let db = Arc::clone(&self.db);
        let cache_dir = self.paths.cache_dir.clone();
        let instances_dir = self.paths.instances_dir.clone();
        let cf_key = self.settings.curseforge_api_key.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        let cancel = CancellationToken::new();
        self.modpacks.install_cancel = Some(cancel.clone());

        if modal_snapshot.content_kind == ContentKind::Modpack
            && modal_snapshot.install_target == ModpackInstallTarget::DedicatedServer
        {
            let desired_name = {
                let trimmed = modal_snapshot.profile_name.trim();
                if trimmed.is_empty() {
                    format!("{} Server", modal_snapshot.detail.summary.title)
                } else {
                    trimmed.to_string()
                }
            };
            let ram_max_mb = modal_snapshot.ram_mb.max(2048);
            let ram_min_mb = 1024u32.min(ram_max_mb);
            let port = modal_snapshot.server_port.max(1024);
            let accept_eula = modal_snapshot.accept_eula;
            let custom_dir = modal_snapshot.custom_dir.clone();
            let summary = modal_snapshot.detail.summary;
            let paths = self.paths.clone();
            let sup = self.server_supervisor.clone();
            let max_conc = if self.settings.low_ram_mode {
                self.settings.max_concurrent_downloads.min(4)
            } else {
                self.settings.max_concurrent_downloads
            };

            if let Some(modal) = self.modpacks.install_modal.as_mut() {
                modal.installing = true;
                modal.error_msg = None;
            }

            self.rt.spawn(async move {
                let id = Uuid::new_v4().to_string();
                let (srv_dir, is_custom_dir) = match custom_dir {
                    Some(dir) => (dir, true),
                    None => {
                        let folder =
                            format!("{}-{}", SafePath::slugify_dir_name(&desired_name), &id[..8]);
                        let Ok(default_dir) = SafePath::resolve_child(&paths.servers_dir, &folder)
                        else {
                            let _ = tx.send(AppAsyncEvent::ModpackInstallFinished {
                                result: Err("Invalid server directory path".into()),
                                cancelled: false,
                            });
                            ctx.request_repaint();
                            return;
                        };
                        (default_dir, false)
                    }
                };

                let tx_p = tx.clone();
                let ctx_p = ctx.clone();
                let cancel_p = cancel.clone();
                let on_prog = move |p: DownloadProgress| {
                    let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                        task_id: "modpack-install".into(),
                        progress: Some(p),
                        cancel: Some(cancel_p.clone()),
                    });
                    ctx_p.request_repaint();
                };

                let res = provision_cloud_modpack_server(
                    &dl,
                    &sup,
                    &db,
                    &paths,
                    id,
                    desired_name,
                    srv_dir,
                    is_custom_dir,
                    port,
                    ram_min_mb,
                    ram_max_mb,
                    accept_eula,
                    &summary,
                    &version,
                    cf_key.as_deref(),
                    max_conc,
                    cancel.clone(),
                    on_prog,
                )
                .await;

                let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                    task_id: "modpack-install".into(),
                    progress: None,
                    cancel: None,
                });
                match res {
                    Ok(srv) => {
                        let _ = tx.send(AppAsyncEvent::ModpackInstallFinished {
                            result: Ok(ModpackInstallOutcome::NewServer(srv)),
                            cancelled: false,
                        });
                    }
                    Err(e) => {
                        let was_cancelled =
                            cancel.is_cancelled() || matches!(e, thk_core::ThkError::Cancelled);
                        let _ = tx.send(AppAsyncEvent::ModpackInstallFinished {
                            result: Err(e.to_string()),
                            cancelled: was_cancelled,
                        });
                    }
                }
                ctx.request_repaint();
            });
        } else if modal_snapshot.content_kind == ContentKind::Modpack {
            let desired_name = {
                let trimmed = modal_snapshot.profile_name.trim();
                if trimmed.is_empty() {
                    modal_snapshot.detail.summary.title.clone()
                } else {
                    trimmed.to_string()
                }
            };
            let desired_ram = modal_snapshot.ram_mb.max(2048);
            let custom_dir = modal_snapshot.custom_dir.clone();
            let detail = modal_snapshot.detail;

            if let Some(modal) = self.modpacks.install_modal.as_mut() {
                modal.installing = true;
                modal.error_msg = None;
            }

            self.rt.spawn(async move {
                let id = Uuid::new_v4().to_string();
                let (inst_dir, is_custom_dir) = match custom_dir {
                    Some(dir) => (dir, true),
                    None => {
                        let folder =
                            format!("{}-{}", SafePath::slugify_dir_name(&desired_name), &id[..8]);
                        let Ok(default_dir) = SafePath::resolve_child(&instances_dir, &folder)
                        else {
                            let _ = tx.send(AppAsyncEvent::ModpackInstallFinished {
                                result: Err("Invalid instance directory path".into()),
                                cancelled: false,
                            });
                            ctx.request_repaint();
                            return;
                        };
                        (default_dir, false)
                    }
                };

                let tx_p = tx.clone();
                let ctx_p = ctx.clone();
                let cancel_p = cancel.clone();
                let on_prog = move |p: DownloadProgress| {
                    let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                        task_id: "modpack-install".into(),
                        progress: Some(p),
                        cancel: Some(cancel_p.clone()),
                    });
                    ctx_p.request_repaint();
                };

                let icon = detail.summary.icon_url.clone();
                let cancel_tok = Some(cancel.clone());
                let res = async {
                    let installation = crate::pack_install::PackInstallation::prepare(&inst_dir)?;
                    let payload = installation.payload();
                    let mut meta = match detail.summary.provider {
                        ProviderSource::Modrinth => {
                            install_modrinth_modpack_version(
                                &dl,
                                &version,
                                &cache_dir,
                                payload,
                                icon,
                                cancel_tok,
                                on_prog.clone(),
                            )
                            .await
                        }
                        ProviderSource::CurseForge => {
                            install_curseforge_modpack_version(
                                &dl,
                                &version,
                                &cache_dir,
                                payload,
                                icon,
                                cf_key.as_deref(),
                                cancel_tok,
                                on_prog.clone(),
                            )
                            .await
                        }
                        ProviderSource::Ftb => {
                            install_ftb_modpack_version(
                                &dl,
                                &detail.summary.project_id,
                                &version.id,
                                &detail.summary.title,
                                icon,
                                payload,
                                cf_key.as_deref(),
                                cancel_tok,
                                on_prog.clone(),
                            )
                            .await
                        }
                        ProviderSource::Technic => {
                            install_technic_modpack_version(
                                &dl,
                                &detail.summary.slug,
                                &version.id,
                                &cache_dir,
                                payload,
                                cancel_tok,
                                on_prog.clone(),
                            )
                            .await
                        }
                        _ => Err(thk_core::ThkError::InvalidInput(
                            "Unsupported modpack provider".into(),
                        )),
                    }?;
                    meta.name = desired_name;
                    let installed_records = std::mem::take(&mut meta.installed_content);
                    let mut profile =
                        build_profile_from_imported_pack(id, inst_dir, meta, desired_ram);
                    profile.is_custom_path = is_custom_dir;
                    profile.ram_mb = desired_ram;
                    let cancel_commit = cancel.clone();
                    let profile =
                        tokio::task::spawn_blocking(move || -> thk_core::ThkResult<GameProfile> {
                            installation.commit_with_progress(
                                &db,
                                &profile,
                                &installed_records,
                                Some(&cancel_commit),
                                on_prog,
                            )?;
                            Ok(profile)
                        })
                        .await
                        .map_err(|e| {
                            thk_core::ThkError::Process(format!(
                                "Modpack commit worker failed: {e}"
                            ))
                        })??;
                    Ok::<_, thk_core::ThkError>(profile)
                }
                .await;

                let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                    task_id: "modpack-install".into(),
                    progress: None,
                    cancel: None,
                });
                match res {
                    Ok(profile) => {
                        let _ = tx.send(AppAsyncEvent::ModpackInstallFinished {
                            result: Ok(ModpackInstallOutcome::NewProfile(profile)),
                            cancelled: false,
                        });
                    }
                    Err(e) => {
                        let was_cancelled =
                            cancel.is_cancelled() || matches!(e, thk_core::ThkError::Cancelled);
                        let _ = tx.send(AppAsyncEvent::ModpackInstallFinished {
                            result: Err(e.to_string()),
                            cancelled: was_cancelled,
                        });
                    }
                }
                ctx.request_repaint();
            });
        } else {
            let Some(ref target_id) = modal_snapshot.target_profile_id else {
                let msg = self
                    .i18n
                    .tr(
                        "Hãy tạo hoặc chọn một Profile đích trước khi cài nội dung này.",
                        "Please create or select a target profile first.",
                    )
                    .to_string();
                if let Some(modal) = self.modpacks.install_modal.as_mut() {
                    modal.error_msg = Some(msg);
                }
                self.modpacks.install_cancel = None;
                return;
            };
            let Some(profile) = self
                .home
                .profiles
                .iter()
                .find(|p| &p.id == target_id)
                .cloned()
            else {
                let msg = self
                    .i18n
                    .tr(
                        "Không tìm thấy Profile đích đã chọn.",
                        "Selected target profile not found.",
                    )
                    .to_string();
                if let Some(modal) = self.modpacks.install_modal.as_mut() {
                    modal.error_msg = Some(msg);
                }
                self.modpacks.install_cancel = None;
                return;
            };

            let kind = modal_snapshot.content_kind;
            let summary = modal_snapshot.detail.summary;

            if let Some(modal) = self.modpacks.install_modal.as_mut() {
                modal.installing = true;
                modal.error_msg = None;
            }

            self.rt.spawn(async move {
                let tx_p = tx.clone();
                let ctx_p = ctx.clone();
                let cancel_p = cancel.clone();
                let on_prog = move |p: DownloadProgress| {
                    let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                        task_id: "modpack-install".into(),
                        progress: Some(p),
                        cancel: Some(cancel_p.clone()),
                    });
                    ctx_p.request_repaint();
                };

                let cancel_tok = Some(cancel.clone());
                let res = match summary.provider {
                    ProviderSource::Modrinth => {
                        install_modrinth_content(
                            &dl, &db, &profile, kind, &summary, &version, cancel_tok, on_prog,
                        )
                        .await
                    }
                    ProviderSource::CurseForge => {
                        install_curseforge_content(
                            &dl,
                            &db,
                            &profile,
                            kind,
                            &summary,
                            &version,
                            cf_key.as_deref(),
                            cancel_tok,
                            on_prog,
                        )
                        .await
                    }
                    _ => Err(thk_core::ThkError::InvalidInput(
                        "Provider does not support direct addon installation".into(),
                    )),
                };

                let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                    task_id: "modpack-install".into(),
                    progress: None,
                    cancel: None,
                });
                match res {
                    Ok(files) => {
                        let _ = tx.send(AppAsyncEvent::ModpackInstallFinished {
                            result: Ok(ModpackInstallOutcome::AddonInstalled {
                                profile_id: profile.id,
                                profile_name: profile.name,
                                kind,
                                title: summary.title,
                                file_count: files.len(),
                            }),
                            cancelled: false,
                        });
                    }
                    Err(e) => {
                        let was_cancelled =
                            cancel.is_cancelled() || matches!(e, thk_core::ThkError::Cancelled);
                        let _ = tx.send(AppAsyncEvent::ModpackInstallFinished {
                            result: Err(e.to_string()),
                            cancelled: was_cancelled,
                        });
                    }
                }
                ctx.request_repaint();
            });
        }
    }

    pub fn request_modpack_texture(
        &mut self,
        url: &str,
        max_dim: u32,
    ) -> Option<egui::TextureHandle> {
        let trimmed = url.trim();
        if trimmed.is_empty()
            || (!trimmed.starts_with("https://") && !trimmed.starts_with("http://"))
        {
            return None;
        }
        let cache_key = format!("{max_dim}:{trimmed}");
        if let Some(tex) = self.modpacks.textures.get(&cache_key) {
            return Some(tex.clone());
        }
        if self.modpacks.pending_textures.contains(&cache_key)
            || self.modpacks.failed_textures.contains(&cache_key)
        {
            return None;
        }

        self.modpacks.pending_textures.insert(cache_key.clone());
        let sem = Arc::clone(&self.modpacks.image_semaphore);
        let dl = self.downloader.clone();
        let fetch_url = trimmed.replace(' ', "%20");
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let _permit = sem.acquire_owned().await.ok();
            let decoded = match dl.get_bytes(&fetch_url).await {
                Ok(bytes) => tokio::task::spawn_blocking(move || {
                    if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                        let scaled = if dyn_img.width() > max_dim || dyn_img.height() > max_dim {
                            dyn_img.thumbnail(max_dim, max_dim)
                        } else {
                            dyn_img
                        };
                        let rgba = scaled.to_rgba8();
                        let (w, h) = rgba.dimensions();
                        if w == 0 || h == 0 {
                            return None;
                        }
                        Some(egui::ColorImage::from_rgba_unmultiplied(
                            [w as usize, h as usize],
                            rgba.as_raw(),
                        ))
                    } else {
                        egui_extras::image::load_svg_bytes_with_size(
                            &bytes,
                            egui::SizeHint::Size {
                                width: max_dim,
                                height: max_dim,
                                maintain_aspect_ratio: true,
                            },
                            &Default::default(),
                        )
                        .ok()
                    }
                })
                .await
                .ok()
                .flatten(),
                Err(_) => None,
            };

            let _ = tx.send(AppAsyncEvent::ModpackRemoteImageLoaded {
                cache_key,
                image: decoded,
            });
            ctx.request_repaint();
        });

        None
    }

    pub fn request_local_image_texture(
        &mut self,
        path: &Path,
        max_dim: u32,
    ) -> Option<egui::TextureHandle> {
        let cache_key = format!("local:{max_dim}:{}", path.display());
        if let Some(tex) = self.modpacks.textures.get(&cache_key) {
            return Some(tex.clone());
        }
        if self.modpacks.pending_textures.contains(&cache_key)
            || self.modpacks.failed_textures.contains(&cache_key)
        {
            return None;
        }
        if !path.is_file() {
            self.modpacks.failed_textures.insert(cache_key);
            return None;
        }

        self.modpacks.pending_textures.insert(cache_key.clone());
        let owned_path = path.to_path_buf();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let decoded = tokio::task::spawn_blocking(move || {
                let dyn_img = image::open(&owned_path).ok()?;
                let scaled = if dyn_img.width() > max_dim || dyn_img.height() > max_dim {
                    dyn_img.thumbnail(max_dim, max_dim)
                } else {
                    dyn_img
                };
                let rgba = scaled.to_rgba8();
                let (w, h) = rgba.dimensions();
                if w == 0 || h == 0 {
                    return None;
                }
                Some(egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    rgba.as_raw(),
                ))
            })
            .await
            .ok()
            .flatten();

            let _ = tx.send(AppAsyncEvent::ModpackRemoteImageLoaded {
                cache_key,
                image: decoded,
            });
            ctx.request_repaint();
        });

        None
    }

    pub fn ensure_engine_versions(&mut self, engine: ServerEngine) {
        if self.servers.engine_versions.contains_key(&engine) {
            return;
        }
        self.servers.engine_versions.insert(engine, Vec::new());
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Ok(versions) = list_supported_versions_for_engine(&dl, engine).await {
                let _ = tx.send(AppAsyncEvent::EngineVersionsLoaded { engine, versions });
                ctx.request_repaint();
            }
        });
    }

    pub fn pick_new_server_modpack_archive(&mut self) {
        if self.servers.new_draft.creating {
            return;
        }
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let Some(archive_path) = crate::dialogs::pick_file_async(
                tr_lang(
                    lang,
                    "Chọn gói Modpack (.mrpack / .zip)",
                    "Select Modpack Archive (.mrpack / .zip)",
                ),
                Some("Modpack Archive"),
                &["mrpack", "zip"],
            )
            .await
            else {
                return;
            };
            let path_for_inspect = archive_path.clone();
            let res = tokio::task::spawn_blocking(move || {
                inspect_modpack_for_server(&path_for_inspect).map(|info| (path_for_inspect, info))
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
            let _ = tx.send(AppAsyncEvent::NewServerModpackInspected(res));
            ctx.request_repaint();
        });
    }

    pub fn search_new_server_cloud_modpacks(&mut self, page: usize, append: bool) {
        if self.servers.new_draft.creating {
            return;
        }
        const CLOUD_SRV_PER_PAGE: usize = 12;
        self.servers.new_draft.cloud_search_seq += 1;
        let seq = self.servers.new_draft.cloud_search_seq;
        self.servers.new_draft.cloud_page = page;
        self.servers.new_draft.cloud_searching = true;
        self.servers.new_draft.create_error = None;
        if !append {
            self.servers.new_draft.cloud_hits.clear();
        }

        let provider = self.servers.new_draft.cloud_provider;
        let gv = {
            let t = self.servers.new_draft.cloud_mc_filter.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        let query = SearchQuery {
            query: self.servers.new_draft.cloud_query.trim().to_string(),
            content_kind: ContentKind::Modpack,
            game_version: gv,
            loader: None,
            sort: SearchSort::Downloads,
            offset: page * CLOUD_SRV_PER_PAGE,
            limit: CLOUD_SRV_PER_PAGE,
        };

        let cf_key = self.settings.curseforge_api_key.clone();
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let res = search_provider(&dl, provider, &query, cf_key.as_deref())
                .await
                .map(|p| (p.hits, p.total_hits))
                .map_err(|e| e.to_string());
            let _ = tx.send(AppAsyncEvent::NewServerCloudSearchCompleted {
                seq,
                append,
                result: res,
            });
            ctx.request_repaint();
        });
    }

    pub fn select_new_server_cloud_modpack(&mut self, summary: ProjectSummary) {
        if self.servers.new_draft.creating {
            return;
        }
        self.servers.new_draft.name = format!("{} Server", summary.title);
        self.servers.new_draft.cloud_loading_versions = true;
        self.servers.new_draft.cloud_chosen_ver_idx = 0;
        self.servers.new_draft.create_error = None;
        let project_id = summary.project_id.clone();
        let provider = summary.provider;
        let mc_filter = {
            let t = self.servers.new_draft.cloud_mc_filter.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        self.servers.new_draft.cloud_selected = Some((summary, Vec::new()));

        let cf_key = self.settings.curseforge_api_key.clone();
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let mut res = fetch_provider_versions(
                &dl,
                provider,
                &project_id,
                mc_filter.as_deref(),
                None,
                cf_key.as_deref(),
            )
            .await;
            if mc_filter.is_some() && res.as_ref().is_ok_and(Vec::is_empty) {
                res = fetch_provider_versions(
                    &dl,
                    provider,
                    &project_id,
                    None,
                    None,
                    cf_key.as_deref(),
                )
                .await;
            }
            let _ = tx.send(AppAsyncEvent::NewServerCloudVersionsLoaded {
                project_id,
                result: res.map_err(|e| e.to_string()),
            });
            ctx.request_repaint();
        });
    }

    pub fn cancel_new_server_creation(&mut self) {
        if let Some(tok) = self.servers.new_draft.create_cancel.take() {
            tok.cancel();
        }
    }

    pub fn create_server_from_draft(&mut self) {
        if self.servers.new_draft.creating {
            return;
        }
        self.servers.new_draft.create_error = None;

        let name = self.servers.new_draft.name.trim().to_string();
        if name.is_empty() {
            let msg = self
                .i18n
                .tr("Vui lòng nhập tên máy chủ.", "Please enter a server name.")
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }

        match self.servers.new_draft.source_mode {
            NewServerSourceMode::StandardEngine => {
                let mc_ver = self.servers.new_draft.game_version.trim().to_string();
                if mc_ver.is_empty() {
                    let msg = self
                        .i18n
                        .tr(
                            "Vui lòng nhập phiên bản Minecraft.",
                            "Please enter a Minecraft version.",
                        )
                        .to_string();
                    self.push_toast(ToastKind::Error, msg);
                    return;
                }

                let id = Uuid::new_v4().to_string();
                let (srv_dir, is_custom_dir) = match self.servers.new_draft.custom_dir.take() {
                    Some(dir) => (dir, true),
                    None => {
                        let folder = format!("{}-{}", SafePath::slugify_dir_name(&name), &id[..8]);
                        let Ok(default_dir) =
                            SafePath::resolve_child(&self.paths.servers_dir, &folder)
                        else {
                            return;
                        };
                        (default_dir, false)
                    }
                };
                let _ = std::fs::create_dir_all(&srv_dir);
                let port = self.servers.new_draft.port.max(1024);
                let mut init_props = BTreeMap::new();
                init_props.insert("server-port".into(), port.to_string());
                init_props.insert("motd".into(), format!("{} - TriHienKun Laucher", name));
                let _ = write_server_properties_patch(&srv_dir, &init_props);
                if self.servers.new_draft.accept_eula {
                    let _ = write_eula_accepted(&srv_dir);
                    self.servers.new_draft.accept_eula = false;
                }

                let install_now = self.servers.new_draft.install_immediately;
                let detected_target = if install_now {
                    String::new()
                } else {
                    list_runnable_server_targets(&srv_dir)
                        .into_iter()
                        .next()
                        .unwrap_or_default()
                };

                let srv = ManagedServer {
                    id: id.clone(),
                    name,
                    engine: self.servers.new_draft.engine,
                    game_version: mc_ver,
                    server_dir: srv_dir,
                    is_custom_dir,
                    port,
                    ram_min_mb: self.servers.new_draft.ram_min_mb.max(512),
                    ram_max_mb: self
                        .servers
                        .new_draft
                        .ram_max_mb
                        .max(self.servers.new_draft.ram_min_mb.max(512)),
                    java_path: None,
                    jvm_flags: String::new(),
                    jar_file: detected_target,
                    created_at: Utc::now().timestamp(),
                };

                match self.db.upsert_server(&srv) {
                    Ok(()) => {
                        self.servers.servers = self.db.list_servers().unwrap_or_default();
                        self.show_new_server_modal = false;
                        self.select_server(id);
                        let msg = self
                            .i18n
                            .tr("Đã tạo máy chủ cục bộ!", "Created local server!")
                            .to_string();
                        self.push_toast(ToastKind::Success, msg);
                        if install_now {
                            self.install_selected_server_jar();
                        }
                    }
                    Err(e) => {
                        let msg = format!(
                            "{}: {e}",
                            self.i18n.tr("Lỗi tạo máy chủ", "Failed to create server")
                        );
                        self.push_toast(ToastKind::Error, msg);
                    }
                }
            }
            NewServerSourceMode::CloudModpack => {
                let Some((summary, versions)) = self.servers.new_draft.cloud_selected.clone()
                else {
                    let msg = self
                        .i18n
                        .tr(
                            "Hãy chọn một gói Modpack từ danh sách trực tuyến.",
                            "Please select a Modpack from the online list.",
                        )
                        .to_string();
                    self.servers.new_draft.create_error = Some(msg);
                    return;
                };
                let Some(version) = versions
                    .get(self.servers.new_draft.cloud_chosen_ver_idx)
                    .cloned()
                else {
                    let msg = self
                        .i18n
                        .tr(
                            "Hãy chọn một phiên bản phát hành hợp lệ.",
                            "Please select a valid release version.",
                        )
                        .to_string();
                    self.servers.new_draft.create_error = Some(msg);
                    return;
                };

                let id = Uuid::new_v4().to_string();
                let (srv_dir, is_custom_dir) = match self.servers.new_draft.custom_dir.clone() {
                    Some(dir) => (dir, true),
                    None => {
                        let folder = format!("{}-{}", SafePath::slugify_dir_name(&name), &id[..8]);
                        let Ok(default_dir) =
                            SafePath::resolve_child(&self.paths.servers_dir, &folder)
                        else {
                            return;
                        };
                        (default_dir, false)
                    }
                };
                let port = self.servers.new_draft.port.max(1024);
                let ram_min_mb = self.servers.new_draft.ram_min_mb.max(512);
                let ram_max_mb = self.servers.new_draft.ram_max_mb.max(ram_min_mb);
                let accept_eula = self.servers.new_draft.accept_eula;
                let cf_key = self.settings.curseforge_api_key.clone();
                let max_conc = if self.settings.low_ram_mode {
                    self.settings.max_concurrent_downloads.min(4)
                } else {
                    self.settings.max_concurrent_downloads
                };

                let cancel = CancellationToken::new();
                self.servers.new_draft.creating = true;
                self.servers.new_draft.create_cancel = Some(cancel.clone());

                let dl = self.downloader.clone();
                let sup = self.server_supervisor.clone();
                let db = Arc::clone(&self.db);
                let paths = self.paths.clone();
                let tx = self.ui_tx.clone();
                let ctx = self.egui_ctx.clone();
                let task_id = "server-pack-create".to_string();

                self.rt.spawn(async move {
                    let tx_p = tx.clone();
                    let ctx_p = ctx.clone();
                    let tid_p = task_id.clone();
                    let cancel_p = cancel.clone();
                    let on_prog = move |p: DownloadProgress| {
                        let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                            task_id: tid_p.clone(),
                            progress: Some(p),
                            cancel: Some(cancel_p.clone()),
                        });
                        ctx_p.request_repaint();
                    };

                    let res = provision_cloud_modpack_server(
                        &dl,
                        &sup,
                        &db,
                        &paths,
                        id,
                        name,
                        srv_dir,
                        is_custom_dir,
                        port,
                        ram_min_mb,
                        ram_max_mb,
                        accept_eula,
                        &summary,
                        &version,
                        cf_key.as_deref(),
                        max_conc,
                        cancel.clone(),
                        on_prog,
                    )
                    .await;

                    let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                        task_id,
                        progress: None,
                        cancel: None,
                    });

                    let cancelled =
                        cancel.is_cancelled() || matches!(res, Err(thk_core::ThkError::Cancelled));
                    let _ = tx.send(AppAsyncEvent::ModpackServerCreated {
                        result: res.map_err(|e| e.to_string()),
                        cancelled,
                    });
                    ctx.request_repaint();
                });
            }
            NewServerSourceMode::ModpackArchive => {
                let Some(archive_path) = self
                    .servers
                    .new_draft
                    .modpack_archive_path
                    .clone()
                    .filter(|p| p.is_file())
                else {
                    let msg = self
                        .i18n
                        .tr(
                            "Hãy chọn tệp gói Modpack (.mrpack hoặc .zip).",
                            "Please select a Modpack archive (.mrpack or .zip).",
                        )
                        .to_string();
                    self.servers.new_draft.create_error = Some(msg);
                    return;
                };

                let id = Uuid::new_v4().to_string();
                let (srv_dir, is_custom_dir) = match self.servers.new_draft.custom_dir.clone() {
                    Some(dir) => (dir, true),
                    None => {
                        let folder = format!("{}-{}", SafePath::slugify_dir_name(&name), &id[..8]);
                        let Ok(default_dir) =
                            SafePath::resolve_child(&self.paths.servers_dir, &folder)
                        else {
                            return;
                        };
                        (default_dir, false)
                    }
                };
                let port = self.servers.new_draft.port.max(1024);
                let ram_min_mb = self.servers.new_draft.ram_min_mb.max(512);
                let ram_max_mb = self.servers.new_draft.ram_max_mb.max(ram_min_mb);
                let accept_eula = self.servers.new_draft.accept_eula;
                let cf_key = self.settings.curseforge_api_key.clone();
                let max_conc = if self.settings.low_ram_mode {
                    self.settings.max_concurrent_downloads.min(4)
                } else {
                    self.settings.max_concurrent_downloads
                };

                let cancel = CancellationToken::new();
                self.servers.new_draft.creating = true;
                self.servers.new_draft.create_cancel = Some(cancel.clone());

                let dl = self.downloader.clone();
                let sup = self.server_supervisor.clone();
                let db = Arc::clone(&self.db);
                let paths = self.paths.clone();
                let tx = self.ui_tx.clone();
                let ctx = self.egui_ctx.clone();
                let task_id = "server-pack-create".to_string();

                self.rt.spawn(async move {
                    let created_fresh_dir = !srv_dir.exists();
                    let tx_p = tx.clone();
                    let ctx_p = ctx.clone();
                    let tid_p = task_id.clone();
                    let cancel_p = cancel.clone();
                    let on_prog = move |p: DownloadProgress| {
                        let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                            task_id: tid_p.clone(),
                            progress: Some(p),
                            cancel: Some(cancel_p.clone()),
                        });
                        ctx_p.request_repaint();
                    };

                    let res: thk_core::ThkResult<ManagedServer> = async {
                        let outcome = populate_server_from_modpack_archive(
                            &dl,
                            &archive_path,
                            &srv_dir,
                            cf_key.as_deref(),
                            max_conc,
                            Some(cancel.clone()),
                            on_prog.clone(),
                        )
                        .await?;

                        if cancel.is_cancelled() {
                            return Err(thk_core::ThkError::Cancelled);
                        }

                        let mut init_props = BTreeMap::new();
                        init_props.insert("server-port".into(), port.to_string());
                        init_props.insert("motd".into(), format!("{name} - TriHienKun Laucher"));
                        write_server_properties_patch(&srv_dir, &init_props)?;
                        if accept_eula {
                            write_eula_accepted(&srv_dir)?;
                        }

                        let mut jar_target = list_runnable_server_targets(&srv_dir)
                            .into_iter()
                            .next()
                            .unwrap_or_default();

                        let mut srv = ManagedServer {
                            id: id.clone(),
                            name,
                            engine: outcome.engine,
                            game_version: outcome.game_version.clone(),
                            server_dir: srv_dir.clone(),
                            is_custom_dir,
                            port,
                            ram_min_mb,
                            ram_max_mb,
                            java_path: None,
                            jvm_flags: String::new(),
                            jar_file: jar_target.clone(),
                            created_at: Utc::now().timestamp(),
                        };

                        if jar_target.is_empty() {
                            let java_bin = sup.resolve_java_for_server(&srv).await?;
                            if cancel.is_cancelled() {
                                return Err(thk_core::ThkError::Cancelled);
                            }
                            jar_target = install_server_engine(
                                &dl,
                                srv.engine,
                                &srv.game_version,
                                &srv.server_dir,
                                &java_bin,
                                on_prog,
                            )
                            .await?;
                            srv.jar_file = jar_target;
                        }

                        db.upsert_server(&srv)?;
                        Ok(srv)
                    }
                    .await;

                    let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                        task_id,
                        progress: None,
                        cancel: None,
                    });

                    let cancelled =
                        cancel.is_cancelled() || matches!(res, Err(thk_core::ThkError::Cancelled));
                    if res.is_err()
                        && created_fresh_dir
                        && is_safe_managed_dir_to_delete(&paths, &srv_dir)
                    {
                        let _ = tokio::fs::remove_dir_all(&srv_dir).await;
                    }

                    let _ = tx.send(AppAsyncEvent::ModpackServerCreated {
                        result: res.map_err(|e| e.to_string()),
                        cancelled,
                    });
                    ctx.request_repaint();
                });
            }
            NewServerSourceMode::ExistingProfile => {
                let chosen_id = self
                    .servers
                    .new_draft
                    .source_profile_id
                    .clone()
                    .or_else(|| self.home.selected_profile_id.clone());
                let Some(prof) = chosen_id
                    .as_deref()
                    .and_then(|pid| self.home.profiles.iter().find(|p| p.id == pid))
                    .cloned()
                else {
                    let msg = self
                        .i18n
                        .tr(
                            "Hãy chọn một Profile đã cài sẵn.",
                            "Please select an existing profile.",
                        )
                        .to_string();
                    self.servers.new_draft.create_error = Some(msg);
                    return;
                };

                if let Err(e) = loader_to_server_engine(prof.loader) {
                    self.servers.new_draft.create_error = Some(e.to_string());
                    return;
                }

                let id = Uuid::new_v4().to_string();
                let (srv_dir, is_custom_dir) = match self.servers.new_draft.custom_dir.clone() {
                    Some(dir) => (dir, true),
                    None => {
                        let folder = format!("{}-{}", SafePath::slugify_dir_name(&name), &id[..8]);
                        let Ok(default_dir) =
                            SafePath::resolve_child(&self.paths.servers_dir, &folder)
                        else {
                            return;
                        };
                        (default_dir, false)
                    }
                };
                let port = self.servers.new_draft.port.max(1024);
                let ram_min_mb = self.servers.new_draft.ram_min_mb.max(512);
                let ram_max_mb = self.servers.new_draft.ram_max_mb.max(ram_min_mb);
                let accept_eula = self.servers.new_draft.accept_eula;

                let cancel = CancellationToken::new();
                self.servers.new_draft.creating = true;
                self.servers.new_draft.create_cancel = Some(cancel.clone());

                let dl = self.downloader.clone();
                let sup = self.server_supervisor.clone();
                let db = Arc::clone(&self.db);
                let paths = self.paths.clone();
                let tx = self.ui_tx.clone();
                let ctx = self.egui_ctx.clone();
                let task_id = "server-pack-create".to_string();

                self.rt.spawn(async move {
                    let created_fresh_dir = !srv_dir.exists();
                    let tx_p = tx.clone();
                    let ctx_p = ctx.clone();
                    let tid_p = task_id.clone();
                    let cancel_p = cancel.clone();
                    let on_prog = move |p: DownloadProgress| {
                        let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                            task_id: tid_p.clone(),
                            progress: Some(p),
                            cancel: Some(cancel_p.clone()),
                        });
                        ctx_p.request_repaint();
                    };

                    on_prog(DownloadProgress {
                        phase: "Profile".into(),
                        message: format!("Copying server mods & configs from '{}'", prof.name),
                        completed_files: 0,
                        total_files: 1,
                        downloaded_bytes: 0,
                        total_bytes: 0,
                        bytes_per_sec: 0,
                        percent: 15.0,
                    });

                    let res: thk_core::ThkResult<ManagedServer> = async {
                        let prof_cl = prof.clone();
                        let sdir_cl = srv_dir.clone();
                        let cancel_cl = cancel.clone();
                        let (engine, _, _) = tokio::task::spawn_blocking(move || {
                            populate_server_from_profile(&prof_cl, &sdir_cl, Some(&cancel_cl))
                        })
                        .await
                        .map_err(|e| thk_core::ThkError::Process(e.to_string()))??;

                        let mut init_props = BTreeMap::new();
                        init_props.insert("server-port".into(), port.to_string());
                        init_props.insert("motd".into(), format!("{name} - TriHienKun Laucher"));
                        write_server_properties_patch(&srv_dir, &init_props)?;
                        if accept_eula {
                            write_eula_accepted(&srv_dir)?;
                        }

                        let mut srv = ManagedServer {
                            id: id.clone(),
                            name,
                            engine,
                            game_version: prof.game_version.clone(),
                            server_dir: srv_dir.clone(),
                            is_custom_dir,
                            port,
                            ram_min_mb,
                            ram_max_mb,
                            java_path: None,
                            jvm_flags: String::new(),
                            jar_file: String::new(),
                            created_at: Utc::now().timestamp(),
                        };

                        let java_bin = sup.resolve_java_for_server(&srv).await?;
                        if cancel.is_cancelled() {
                            return Err(thk_core::ThkError::Cancelled);
                        }
                        let jar_target = install_server_engine(
                            &dl,
                            srv.engine,
                            &srv.game_version,
                            &srv.server_dir,
                            &java_bin,
                            on_prog,
                        )
                        .await?;
                        srv.jar_file = jar_target;

                        db.upsert_server(&srv)?;
                        Ok(srv)
                    }
                    .await;

                    let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                        task_id,
                        progress: None,
                        cancel: None,
                    });

                    let cancelled =
                        cancel.is_cancelled() || matches!(res, Err(thk_core::ThkError::Cancelled));
                    if res.is_err()
                        && created_fresh_dir
                        && is_safe_managed_dir_to_delete(&paths, &srv_dir)
                    {
                        let _ = tokio::fs::remove_dir_all(&srv_dir).await;
                    }

                    let _ = tx.send(AppAsyncEvent::ModpackServerCreated {
                        result: res.map_err(|e| e.to_string()),
                        cancelled,
                    });
                    ctx.request_repaint();
                });
            }
        }
    }

    pub fn install_selected_server_jar(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let dl = self.downloader.clone();
        let sup = self.server_supervisor.clone();
        let db = Arc::clone(&self.db);
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.busy_message = Some(if lang == Language::Vi {
            format!(
                "Đang cài đặt lõi {} ({})...",
                srv.engine.display_name(),
                srv.game_version
            )
        } else {
            format!(
                "Installing {} core ({})...",
                srv.engine.display_name(),
                srv.game_version
            )
        });

        self.rt.spawn(async move {
            let java_res = sup.resolve_java_for_server(&srv).await;
            let java_bin = match java_res {
                Ok(j) => j,
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::BusyStatus(None));
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!(
                            "{}: {e}",
                            tr_lang(
                                lang,
                                "Không thể chuẩn bị Java cho máy chủ",
                                "Failed to prepare Java runtime for server"
                            )
                        ),
                    });
                    ctx.request_repaint();
                    return;
                }
            };

            let task_id = format!("server-install:{}", srv.id);
            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let tid_p = task_id.clone();
            let res = install_server_engine(
                &dl,
                srv.engine,
                &srv.game_version,
                &srv.server_dir,
                &java_bin,
                move |p| {
                    let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                        task_id: tid_p.clone(),
                        progress: Some(p),
                        cancel: None,
                    });
                    ctx_p.request_repaint();
                },
            )
            .await;

            let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                task_id,
                progress: None,
                cancel: None,
            });
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(jar_target) => {
                    let mut updated = srv.clone();
                    updated.jar_file = jar_target.clone();
                    let _ = db.upsert_server(&updated);
                    let _ = tx.send(AppAsyncEvent::ServerJarInstalled {
                        server_id: srv.id,
                        jar_file: jar_target,
                    });
                }
                Err(e) => {
                    let text = if lang == Language::Vi {
                        format!("Tải lõi {} thất bại: {e}", srv.engine.display_name())
                    } else {
                        format!("Failed to install {} core: {e}", srv.engine.display_name())
                    };
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text,
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn start_selected_server(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        if !is_eula_accepted(&srv.server_dir) {
            self.pending_eula_server_id = Some((srv.id, false));
            return;
        }
        let sup = self.server_supervisor.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Err(e) = sup.start_server(&srv, false).await {
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Error,
                    text: format!(
                        "{}: {e}",
                        tr_lang(
                            lang,
                            "Không thể khởi động máy chủ",
                            "Failed to start server"
                        )
                    ),
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn stop_selected_server(&self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let sup = self.server_supervisor.clone();
        self.rt.spawn(async move {
            let _ = sup.stop_server(&srv.id).await;
        });
    }

    pub fn restart_selected_server(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        if !is_eula_accepted(&srv.server_dir) {
            self.pending_eula_server_id = Some((srv.id, true));
            return;
        }
        let sup = self.server_supervisor.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Err(e) = sup.restart_server(&srv, false).await {
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Error,
                    text: format!(
                        "{}: {e}",
                        tr_lang(lang, "Khởi động lại thất bại", "Failed to restart server")
                    ),
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn confirm_server_eula_and_start(&mut self) {
        let Some((server_id, is_restart)) = self.pending_eula_server_id.take() else {
            return;
        };
        let Some(srv) = self
            .servers
            .servers
            .iter()
            .find(|s| s.id == server_id)
            .cloned()
        else {
            return;
        };
        if let Err(e) = write_eula_accepted(&srv.server_dir) {
            let msg = format!(
                "{}: {e}",
                self.i18n
                    .tr("Không thể ghi eula.txt", "Failed to write eula.txt")
            );
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        let sup = self.server_supervisor.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let res = if is_restart {
                sup.restart_server(&srv, true).await
            } else {
                sup.start_server(&srv, true).await
            };
            if let Err(e) = res {
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Error,
                    text: format!(
                        "{}: {e}",
                        tr_lang(
                            lang,
                            "Không thể khởi động máy chủ",
                            "Failed to start server"
                        )
                    ),
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn send_selected_server_command(&mut self) {
        let Some(srv) = self.selected_server() else {
            return;
        };
        let raw_input = self.servers.console_input.trim().to_string();
        if raw_input.is_empty() {
            return;
        }
        let srv_id = srv.id.clone();
        if self.server_supervisor.runtime_state(&srv_id) == ServerRuntimeState::Offline {
            let msg = self
                .i18n
                .tr(
                    "Máy chủ đang Offline. Hãy nhấn 'Khởi động' trước khi gửi lệnh.",
                    "Server is offline. Click 'Start' before sending commands.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        let cmd = thk_server::sanitize_console_command(&raw_input);
        if cmd.is_empty() {
            return;
        }
        match self.server_supervisor.send_command(&srv_id, &cmd) {
            Ok(()) => {
                self.click_sfx();
                if self.servers.cmd_history.last() != Some(&cmd) {
                    if self.servers.cmd_history.len() >= 100 {
                        self.servers.cmd_history.remove(0);
                    }
                    self.servers.cmd_history.push(cmd.clone());
                }
                self.servers.cmd_history_cursor = None;
                self.servers
                    .server_logs
                    .entry(srv_id)
                    .or_default()
                    .push(format!("> {cmd}"));
                self.servers.console_input.clear();
            }
            Err(e) => {
                let msg = if self.settings.language == Language::Vi {
                    format!("Không thể gửi lệnh tới máy chủ: {e}")
                } else {
                    format!("Failed to send server command: {e}")
                };
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn navigate_server_cmd_history(&mut self, up: bool) {
        let len = self.servers.cmd_history.len();
        if len == 0 {
            return;
        }
        if up {
            let next_idx = match self.servers.cmd_history_cursor {
                None => len - 1,
                Some(0) => 0,
                Some(idx) => idx.saturating_sub(1),
            };
            self.servers.cmd_history_cursor = Some(next_idx);
            if let Some(entry) = self.servers.cmd_history.get(next_idx) {
                self.servers.console_input = entry.clone();
            }
        } else if let Some(idx) = self.servers.cmd_history_cursor {
            if idx + 1 < len {
                let next_idx = idx + 1;
                self.servers.cmd_history_cursor = Some(next_idx);
                if let Some(entry) = self.servers.cmd_history.get(next_idx) {
                    self.servers.console_input = entry.clone();
                }
            } else {
                self.servers.cmd_history_cursor = None;
                self.servers.console_input.clear();
            }
        }
    }

    pub fn delete_selected_server(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        if self.server_supervisor.runtime_state(&srv.id) != ServerRuntimeState::Offline {
            let msg = self
                .i18n
                .tr(
                    "Vui lòng dừng máy chủ trước khi xóa khỏi danh sách.",
                    "Please stop the server before deleting it.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        self.pending_delete_modal = Some(PendingDeleteModal {
            delete_files_on_disk: !srv.is_custom_dir,
            target: DeleteTarget::Server {
                server_id: srv.id,
                name: srv.name,
                server_dir: srv.server_dir,
                is_custom_dir: srv.is_custom_dir,
            },
        });
    }

    pub fn refresh_selected_server_tabs(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            self.servers.dir_entries.clear();
            self.servers.properties_map.clear();
            self.servers.whitelist.clear();
            self.servers.banlist.clear();
            self.servers.ops_list.clear();
            self.servers.banned_ips.clear();
            self.servers.known_players.clear();
            self.servers.confirm_reset_player_uuid = None;
            self.servers.installed_addons.clear();
            self.servers.runnable_targets.clear();
            return;
        };
        self.servers.dir_entries =
            list_server_dir(&srv.server_dir, &self.servers.current_rel_dir).unwrap_or_default();
        self.servers.properties_map = read_server_properties(&srv.server_dir).unwrap_or_default();
        self.servers.whitelist = read_whitelist(&srv.server_dir).unwrap_or_default();
        self.servers.banlist = read_banned_players(&srv.server_dir).unwrap_or_default();
        self.servers.ops_list = read_ops(&srv.server_dir).unwrap_or_default();
        self.servers.banned_ips = read_banned_ips(&srv.server_dir).unwrap_or_default();
        self.servers.known_players = read_known_players(&srv.server_dir).unwrap_or_default();
        self.servers.confirm_reset_player_uuid = None;
        self.servers.installed_addons = list_server_addon_files(
            &srv.server_dir,
            srv.engine.supports_plugins(),
            srv.engine.supports_mods(),
        )
        .unwrap_or_default();
        self.servers.runnable_targets = list_runnable_server_targets(&srv.server_dir);
        if srv.engine.supports_plugins() && !srv.engine.supports_mods() {
            self.servers.addon_kind = ContentKind::Plugin;
        } else if srv.engine.supports_mods() && !srv.engine.supports_plugins() {
            self.servers.addon_kind = ContentKind::Mod;
        }
    }

    pub fn toggle_server_installed_addon(&mut self, folder: &str, file_name: &str, enable: bool) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        match toggle_server_addon_file(&srv.server_dir, folder, file_name, enable) {
            Ok(_) => self.refresh_selected_server_tabs(),
            Err(e) => self.push_toast(ToastKind::Error, e.to_string()),
        }
    }

    pub fn delete_server_installed_addon(&mut self, folder: &str, file_name: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        match delete_server_addon_file(&srv.server_dir, folder, file_name) {
            Ok(()) => {
                let addon_kind = if folder == "plugins" {
                    ContentKind::Plugin
                } else {
                    ContentKind::Mod
                };
                let base_name = file_name.strip_suffix(".disabled").unwrap_or(file_name);
                let _ = self.db.remove_installed_content(
                    &format!("server:{}", srv.id),
                    addon_kind,
                    base_name,
                );
                self.refresh_selected_server_tabs();
                let msg = if self.settings.language == Language::Vi {
                    format!("Đã xóa '{file_name}' khỏi {folder}/.")
                } else {
                    format!("Removed '{file_name}' from {folder}/.")
                };
                self.push_toast(ToastKind::Info, msg);
            }
            Err(e) => self.push_toast(ToastKind::Error, e.to_string()),
        }
    }

    pub fn import_jar_into_server_addons(&mut self, folder: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let folder_owned = folder.to_string();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let Some(files) = crate::dialogs::pick_files_async(
                tr_lang(
                    lang,
                    "Chọn tệp .jar thêm vào máy chủ",
                    "Select .jar file to import into server",
                ),
                Some("Java Archive (.jar)"),
                &["jar"],
            )
            .await
            else {
                return;
            };
            let mut imported_count = 0usize;
            for p in files {
                let sdir = srv.server_dir.clone();
                let fld = folder_owned.clone();
                match tokio::task::spawn_blocking(move || import_server_addon_jar(&sdir, &fld, &p))
                    .await
                {
                    Ok(Ok(_)) => imported_count += 1,
                    Ok(Err(e)) => {
                        let _ = tx.send(AppAsyncEvent::Toast {
                            kind: ToastKind::Error,
                            text: e.to_string(),
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(AppAsyncEvent::Toast {
                            kind: ToastKind::Error,
                            text: e.to_string(),
                        });
                    }
                }
            }
            if imported_count > 0 {
                let _ = tx.send(AppAsyncEvent::ServerExternalFileImported { server_id: srv.id });
                let text = if lang == Language::Vi {
                    format!("Đã thêm {imported_count} tệp .jar vào {folder_owned}/!")
                } else {
                    format!("Imported {imported_count} .jar file(s) into {folder_owned}/!")
                };
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Success,
                    text,
                });
            }
            ctx.request_repaint();
        });
    }

    pub fn open_server_file_editor(&mut self, rel_path: &str) {
        let Some(srv) = self.selected_server() else {
            return;
        };
        let srv_dir = srv.server_dir.clone();
        match read_server_text_file(&srv_dir, rel_path) {
            Ok(content) => {
                self.servers.editing_file_rel = Some(rel_path.to_string());
                self.servers.editing_file_text = content;
            }
            Err(e) => {
                let msg = format!(
                    "{}: {e}",
                    self.i18n.tr("Không thể mở tệp", "Failed to open file")
                );
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn save_server_file_editor(&mut self) {
        let Some(srv) = self.selected_server() else {
            return;
        };
        let Some(rel) = self.servers.editing_file_rel.clone() else {
            return;
        };
        let srv_dir = srv.server_dir.clone();
        let body = self.servers.editing_file_text.clone();
        match write_server_text_file(&srv_dir, &rel, &body) {
            Ok(()) => {
                let msg = if self.settings.language == Language::Vi {
                    format!("Đã lưu tệp '{rel}'.")
                } else {
                    format!("Saved file '{rel}'.")
                };
                self.push_toast(ToastKind::Success, msg);
            }
            Err(e) => {
                let msg = format!(
                    "{}: {e}",
                    self.i18n.tr("Lỗi lưu tệp", "Failed to save file")
                );
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn import_external_file_into_server(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let rel_dir = self.servers.current_rel_dir.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let Some(file_path) = crate::dialogs::pick_file_async(
                tr_lang(
                    lang,
                    "Chọn tệp tải lên thư mục Máy chủ",
                    "Select File to Upload to Server Directory",
                ),
                None,
                &[],
            )
            .await
            else {
                return;
            };
            match import_external_file_streaming(&srv.server_dir, &rel_dir, &file_path).await {
                Ok(_) => {
                    let _ =
                        tx.send(AppAsyncEvent::ServerExternalFileImported { server_id: srv.id });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!(
                            "{}: {e}",
                            tr_lang(
                                lang,
                                "Không thể chép tệp vào máy chủ",
                                "Failed to copy file into server"
                            )
                        ),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn delete_selected_server_files(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let mut targets: Vec<String> = self.servers.selected_entries.iter().cloned().collect();
        targets.sort();
        if targets.is_empty() {
            return;
        }
        self.pending_delete_modal = Some(PendingDeleteModal {
            delete_files_on_disk: true,
            target: DeleteTarget::ServerFiles {
                server_id: srv.id,
                server_dir: srv.server_dir,
                targets,
            },
        });
    }

    pub fn compress_selected_server_files(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let targets: Vec<String> = self.servers.selected_entries.iter().cloned().collect();
        if targets.is_empty() {
            let msg = self
                .i18n
                .tr(
                    "Hãy đánh dấu ít nhất 1 tệp/thư mục để nén.",
                    "Select at least 1 file or folder to compress.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        let zip_rel = if self.servers.current_rel_dir.is_empty() {
            self.servers.archive_name_input.trim().to_string()
        } else {
            format!(
                "{}/{}",
                self.servers.current_rel_dir,
                self.servers.archive_name_input.trim()
            )
        };
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(
            self.i18n
                .tr(
                    "Đang nén tệp máy chủ thành ZIP...",
                    "Compressing server files into ZIP...",
                )
                .to_string(),
        );
        self.rt.spawn(async move {
            let sdir = srv.server_dir.clone();
            let zrel = zip_rel.clone();
            let res = tokio::task::spawn_blocking(move || {
                compress_server_entries(&sdir, &targets, &zrel)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(_) => {
                    let text = if lang == Language::Vi {
                        format!("Đã nén thành '{zip_rel}'.")
                    } else {
                        format!("Compressed into '{zip_rel}'.")
                    };
                    let _ = tx.send(AppAsyncEvent::ServerFsCompleted {
                        server_id: srv.id,
                        clear_selection: true,
                        toast: (ToastKind::Success, text),
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!(
                            "{}: {e}",
                            tr_lang(lang, "Lỗi nén ZIP", "ZIP compression failed")
                        ),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn extract_server_archive(&mut self, rel_zip: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let rel_owned = rel_zip.to_string();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(
            self.i18n
                .tr(
                    "Đang giải nén gói ZIP máy chủ...",
                    "Extracting server ZIP archive...",
                )
                .to_string(),
        );
        self.rt.spawn(async move {
            let sdir = srv.server_dir.clone();
            let rz = rel_owned.clone();
            let res = tokio::task::spawn_blocking(move || extract_server_zip(&sdir, &rz))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(cnt) => {
                    let text = if lang == Language::Vi {
                        format!("Đã giải nén {cnt} tệp.")
                    } else {
                        format!("Extracted {cnt} file(s).")
                    };
                    let _ = tx.send(AppAsyncEvent::ServerFsCompleted {
                        server_id: srv.id,
                        clear_selection: false,
                        toast: (ToastKind::Success, text),
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!(
                            "{}: {e}",
                            tr_lang(lang, "Giải nén thất bại", "Archive extraction failed")
                        ),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn save_server_properties_edits(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        if !self.servers.new_prop_key.trim().is_empty() {
            self.servers.properties_map.insert(
                self.servers.new_prop_key.trim().to_string(),
                self.servers.new_prop_val.trim().to_string(),
            );
            self.servers.new_prop_key.clear();
            self.servers.new_prop_val.clear();
        }
        match write_server_properties_patch(&srv.server_dir, &self.servers.properties_map) {
            Ok(()) => {
                let msg = self
                    .i18n
                    .tr("Đã lưu server.properties!", "Saved server.properties!")
                    .to_string();
                self.push_toast(ToastKind::Success, msg);
            }
            Err(e) => {
                let msg = format!(
                    "{}: {e}",
                    self.i18n.tr(
                        "Lỗi lưu server.properties",
                        "Failed to save server.properties"
                    )
                );
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn add_server_whitelist(&mut self) {
        let name = self.servers.new_wl_name.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.servers.new_wl_name.clear();
        self.add_server_whitelist_by_name(&name);
    }

    pub fn add_server_whitelist_by_name(&mut self, raw_name: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let name = raw_name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let (uuid, canonical_name) =
                resolve_player_uuid_for_server(&dl, &srv.server_dir, &name).await;
            let sdir = srv.server_dir.clone();
            let cname = canonical_name.clone();
            let res = tokio::task::spawn_blocking(move || {
                let list = upsert_whitelist_player(&sdir, &cname, &uuid)?;
                let known = read_known_players(&sdir).unwrap_or_default();
                Ok::<_, thk_core::ThkError>((list, known))
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
            match res {
                Ok((list, known)) => {
                    let _ = tx.send(AppAsyncEvent::ServerAccessListUpdated {
                        server_id: srv.id,
                        whitelist: Some(list),
                        banlist: None,
                        ops_list: None,
                        banned_ips: None,
                        known_players: Some(known),
                        live_cmd: Some(format!("whitelist add {canonical_name}")),
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: e,
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn remove_server_whitelist(&mut self, name: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        if let Ok(list) = remove_whitelist_player(&srv.server_dir, name) {
            self.servers.whitelist = list;
            self.servers.known_players = read_known_players(&srv.server_dir).unwrap_or_default();
            if self.server_supervisor.runtime_state(&srv.id) == ServerRuntimeState::Online {
                let _ = self
                    .server_supervisor
                    .send_command(&srv.id, &format!("whitelist remove {name}"));
            }
        }
    }

    pub fn add_server_ban(&mut self) {
        let name = self.servers.new_ban_name.trim().to_string();
        let reason = self.servers.new_ban_reason.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.servers.new_ban_name.clear();
        self.servers.new_ban_reason.clear();
        self.add_server_ban_by_name(&name, &reason);
    }

    pub fn add_server_ban_by_name(&mut self, raw_name: &str, raw_reason: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let name = raw_name.trim().to_string();
        let reason = raw_reason.trim().to_string();
        if name.is_empty() {
            return;
        }
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let (uuid, canonical_name) =
                resolve_player_uuid_for_server(&dl, &srv.server_dir, &name).await;
            let sdir = srv.server_dir.clone();
            let cname = canonical_name.clone();
            let rsn = reason.clone();
            let res =
                tokio::task::spawn_blocking(move || add_banned_player(&sdir, &cname, &uuid, &rsn))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| e.to_string()));
            match res {
                Ok(list) => {
                    let live_cmd = if reason.is_empty() {
                        format!("ban {canonical_name}")
                    } else {
                        format!("ban {canonical_name} {reason}")
                    };
                    let _ = tx.send(AppAsyncEvent::ServerAccessListUpdated {
                        server_id: srv.id,
                        whitelist: None,
                        banlist: Some(list),
                        ops_list: None,
                        banned_ips: None,
                        known_players: None,
                        live_cmd: Some(live_cmd),
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: e,
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn remove_server_ban(&mut self, name: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        if let Ok(list) = remove_banned_player(&srv.server_dir, name) {
            self.servers.banlist = list;
            if self.server_supervisor.runtime_state(&srv.id) == ServerRuntimeState::Online {
                let _ = self
                    .server_supervisor
                    .send_command(&srv.id, &format!("pardon {name}"));
            }
        }
    }

    pub fn add_server_op(&mut self) {
        let name = self.servers.new_op_name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let level = self.servers.new_op_level.clamp(1, 4);
        let bypass = self.servers.new_op_bypass_limit;
        self.servers.new_op_name.clear();
        self.upsert_server_op_player(&name, level, bypass);
    }

    pub fn upsert_server_op_player(
        &mut self,
        raw_name: &str,
        level: u8,
        bypasses_player_limit: bool,
    ) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let name = raw_name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let clamped_level = level.clamp(1, 4);
        let dl = self.downloader.clone();
        let sup = self.server_supervisor.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let (uuid, canonical_name) =
                resolve_player_uuid_for_server(&dl, &srv.server_dir, &name).await;
            if sup.runtime_state(&srv.id) == ServerRuntimeState::Online {
                let _ = sup.send_command(&srv.id, &format!("op {canonical_name}"));
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            let sdir = srv.server_dir.clone();
            let cname = canonical_name.clone();
            let res = tokio::task::spawn_blocking(move || {
                let ops =
                    upsert_op_player(&sdir, &cname, &uuid, clamped_level, bypasses_player_limit)?;
                let known = read_known_players(&sdir).unwrap_or_default();
                Ok::<_, thk_core::ThkError>((ops, known))
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
            match res {
                Ok((ops, known)) => {
                    let _ = tx.send(AppAsyncEvent::ServerAccessListUpdated {
                        server_id: srv.id,
                        whitelist: None,
                        banlist: None,
                        ops_list: Some(ops),
                        banned_ips: None,
                        known_players: Some(known),
                        live_cmd: None,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: e,
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn remove_server_op(&mut self, name: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        if self.server_supervisor.runtime_state(&srv.id) == ServerRuntimeState::Online {
            let _ = self
                .server_supervisor
                .send_command(&srv.id, &format!("deop {name}"));
        }
        if let Ok(list) = remove_op_player(&srv.server_dir, name) {
            self.servers.ops_list = list;
            self.servers.known_players = read_known_players(&srv.server_dir).unwrap_or_default();
        }
    }

    pub fn add_server_ban_ip(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let ip = self.servers.new_ban_ip.trim().to_string();
        let reason = self.servers.new_ban_ip_reason.trim().to_string();
        if ip.is_empty() {
            return;
        }
        match add_banned_ip(&srv.server_dir, &ip, &reason) {
            Ok(list) => {
                self.servers.new_ban_ip.clear();
                self.servers.new_ban_ip_reason.clear();
                self.servers.banned_ips = list;
                if self.server_supervisor.runtime_state(&srv.id) == ServerRuntimeState::Online {
                    let cmd = if reason.is_empty() {
                        format!("ban-ip {ip}")
                    } else {
                        format!("ban-ip {ip} {reason}")
                    };
                    let _ = self.server_supervisor.send_command(&srv.id, &cmd);
                }
            }
            Err(e) => {
                let msg = format!(
                    "{}: {e}",
                    self.i18n
                        .tr("Không thể cấm địa chỉ IP", "Failed to ban IP address")
                );
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn remove_server_ban_ip(&mut self, ip: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        if let Ok(list) = remove_banned_ip(&srv.server_dir, ip) {
            self.servers.banned_ips = list;
            if self.server_supervisor.runtime_state(&srv.id) == ServerRuntimeState::Online {
                let _ = self
                    .server_supervisor
                    .send_command(&srv.id, &format!("pardon-ip {ip}"));
            }
        }
    }

    pub fn save_player_rules_properties(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        match write_server_properties_patch(&srv.server_dir, &self.servers.properties_map) {
            Ok(()) => {
                if self.server_supervisor.runtime_state(&srv.id) == ServerRuntimeState::Online {
                    if let Some(wl) = self.servers.properties_map.get("white-list") {
                        if wl.trim().eq_ignore_ascii_case("true") {
                            let _ = self.server_supervisor.send_command(&srv.id, "whitelist on");
                            let _ = self
                                .server_supervisor
                                .send_command(&srv.id, "whitelist reload");
                        } else if wl.trim().eq_ignore_ascii_case("false") {
                            let _ = self
                                .server_supervisor
                                .send_command(&srv.id, "whitelist off");
                        }
                    }
                    if let Some(idle) = self.servers.properties_map.get("player-idle-timeout") {
                        if let Ok(mins) = idle.trim().parse::<u32>() {
                            let _ = self
                                .server_supervisor
                                .send_command(&srv.id, &format!("setidletimeout {mins}"));
                        }
                    }
                }
                let msg = self
                    .i18n
                    .tr(
                        "Đã lưu cấu hình người chơi! (Các mục khởi tạo thế giới/slot sẽ áp dụng đầy đủ sau khi khởi động lại)",
                        "Saved player rules! (Slot & startup rules take full effect after restart)",
                    )
                    .to_string();
                self.push_toast(ToastKind::Success, msg);
            }
            Err(e) => {
                let msg = format!(
                    "{}: {e}",
                    self.i18n.tr(
                        "Lỗi lưu cấu hình người chơi",
                        "Failed to save player configuration"
                    )
                );
                self.push_toast(ToastKind::Error, msg);
            }
        }
    }

    pub fn run_live_player_command(&mut self, cmd: &str, toast_msg: String) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        if self.server_supervisor.runtime_state(&srv.id) != ServerRuntimeState::Online {
            let msg = self
                .i18n
                .tr(
                    "Máy chủ đang ngoại tuyến — không thể chạy lệnh trực tiếp.",
                    "Server is offline — cannot run live player command.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        match self.server_supervisor.send_command(&srv.id, cmd) {
            Ok(()) => self.push_toast(ToastKind::Success, toast_msg),
            Err(e) => self.push_toast(ToastKind::Error, e.to_string()),
        }
    }

    pub fn reset_offline_player_data(&mut self, uuid: &str, player_name: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let is_online = self.servers.server_stats.get(&srv.id).is_some_and(|st| {
            st.online_player_names
                .iter()
                .any(|p| p.eq_ignore_ascii_case(player_name))
        });
        if is_online {
            let msg = self
                .i18n
                .tr(
                    "Hãy Kick người chơi này ra khỏi máy chủ trước khi đặt lại dữ liệu .dat!",
                    "Kick this player from the server before resetting their .dat save file!",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        self.servers.confirm_reset_player_uuid = None;
        match delete_player_data(&srv.server_dir, uuid) {
            Ok(removed) => {
                self.servers.known_players =
                    read_known_players(&srv.server_dir).unwrap_or_default();
                let msg = if self.settings.language == Language::Vi {
                    format!("Đã đặt lại dữ liệu người chơi '{player_name}' ({removed} tệp đã xóa).")
                } else {
                    format!("Reset player data for '{player_name}' ({removed} file(s) removed).")
                };
                self.push_toast(ToastKind::Success, msg);
            }
            Err(e) => {
                self.push_toast(ToastKind::Error, e.to_string());
            }
        }
    }

    pub fn search_server_addons(&mut self) {
        self.search_server_addons_page(0, false);
    }

    pub fn search_server_addons_page(&mut self, page: usize, append: bool) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        self.servers.addon_page = page;
        if !append {
            self.servers.addon_selected = None;
        }
        let provider = self.servers.addon_provider;
        let kind = if provider == ProviderSource::Spiget {
            ContentKind::Plugin
        } else if provider == ProviderSource::CurseForge
            && self.servers.addon_kind == ContentKind::Plugin
        {
            ContentKind::Mod
        } else {
            self.servers.addon_kind
        };
        let loader = resolve_server_mod_loader(&srv, kind);
        let limit = 20usize;
        let query = SearchQuery {
            query: self.servers.addon_query.trim().to_string(),
            content_kind: kind,
            game_version: Some(srv.game_version),
            loader,
            sort: SearchSort::Downloads,
            offset: page * limit,
            limit,
        };
        let cf_key = self.settings.curseforge_api_key.clone();
        let lang = self.settings.language;
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(
            self.i18n
                .tr(
                    "Đang tìm kiếm Plugin / Mod cho máy chủ...",
                    "Searching server plugins & mods...",
                )
                .to_string(),
        );

        self.rt.spawn(async move {
            let res = search_provider(&dl, provider, &query, cf_key.as_deref()).await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(res_page) => {
                    let _ = tx.send(AppAsyncEvent::ServerAddonSearchCompleted {
                        append,
                        hits: res_page.hits,
                        total_hits: res_page.total_hits,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!(
                            "{}: {e}",
                            tr_lang(
                                lang,
                                "Lỗi tìm kiếm Plugin/Mod",
                                "Server addon search failed"
                            )
                        ),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn load_server_addon_versions(&mut self, summary: ProjectSummary) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let kind = if summary.provider == ProviderSource::Spiget {
            ContentKind::Plugin
        } else {
            self.servers.addon_kind
        };
        let loader = resolve_server_mod_loader(&srv, kind);
        let cf_key = self.settings.curseforge_api_key.clone();
        let lang = self.settings.language;
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(if lang == Language::Vi {
            format!("Đang tải phiên bản của '{}'...", summary.title)
        } else {
            format!("Loading versions for '{}'...", summary.title)
        });

        self.rt.spawn(async move {
            let ver_res = fetch_provider_versions(
                &dl,
                summary.provider,
                &summary.project_id,
                Some(&srv.game_version),
                loader,
                cf_key.as_deref(),
            )
            .await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match ver_res {
                Ok(versions) => {
                    let _ = tx.send(AppAsyncEvent::ServerAddonVersionsLoaded { summary, versions });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: e.to_string(),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn install_server_addon_item(&mut self, summary: ProjectSummary) {
        self.install_server_addon_version(summary, None);
    }

    pub fn install_server_addon_version(
        &mut self,
        summary: ProjectSummary,
        chosen_version: Option<ProjectVersion>,
    ) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let kind = if summary.provider == ProviderSource::Spiget {
            ContentKind::Plugin
        } else if !srv.engine.supports_plugins() && srv.engine.supports_mods() {
            ContentKind::Mod
        } else {
            self.servers.addon_kind
        };
        let loader = resolve_server_mod_loader(&srv, kind);
        let cf_key = self.settings.curseforge_api_key.clone();
        let lang = self.settings.language;
        let dl = self.downloader.clone();
        let db = Arc::clone(&self.db);
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        let cancel = CancellationToken::new();
        let task_id = format!("srv-addon:{}:{}", srv.id, summary.project_id);

        self.rt.spawn(async move {
            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let tid_p = task_id.clone();
            let cancel_p = cancel.clone();
            let on_prog = move |p: DownloadProgress| {
                let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                    task_id: tid_p.clone(),
                    progress: Some(p),
                    cancel: Some(cancel_p.clone()),
                });
                ctx_p.request_repaint();
            };

            let res: thk_core::ThkResult<Vec<PathBuf>> =
                if summary.provider == ProviderSource::Spiget {
                    install_spiget_plugin(
                        &dl,
                        &summary.project_id,
                        &summary.title,
                        &srv.server_dir,
                        Some(cancel.clone()),
                        on_prog,
                    )
                    .await
                    .map(|p| vec![p])
                } else {
                    async {
                        let target_ver = match chosen_version {
                            Some(v) => v,
                            None => {
                                let vers = fetch_provider_versions(
                                    &dl,
                                    summary.provider,
                                    &summary.project_id,
                                    Some(&srv.game_version),
                                    loader,
                                    cf_key.as_deref(),
                                )
                                .await?;
                                vers.into_iter().next().ok_or_else(|| {
                                    thk_core::ThkError::Provider(format!(
                                        "'{}' has no compatible build for MC {}",
                                        summary.title, srv.game_version
                                    ))
                                })?
                            }
                        };
                        install_server_provider_addon(
                            &dl,
                            &db,
                            &srv.id,
                            &srv.server_dir,
                            &srv.game_version,
                            loader,
                            kind,
                            &summary,
                            &target_ver,
                            cf_key.as_deref(),
                            Some(cancel.clone()),
                            on_prog,
                        )
                        .await
                    }
                    .await
                };

            let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                task_id,
                progress: None,
                cancel: None,
            });
            match res {
                Ok(files) => {
                    let _ =
                        tx.send(AppAsyncEvent::ServerExternalFileImported { server_id: srv.id });
                    let fname = files
                        .first()
                        .and_then(|p| p.file_name())
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let text = if lang == Language::Vi {
                        format!(
                            "Đã cài đặt '{}' ({} tệp) vào máy chủ!",
                            if fname.is_empty() {
                                summary.title.as_str()
                            } else {
                                fname.as_str()
                            },
                            files.len()
                        )
                    } else {
                        format!(
                            "Installed '{}' ({} file(s)) into server!",
                            if fname.is_empty() {
                                summary.title.as_str()
                            } else {
                                fname.as_str()
                            },
                            files.len()
                        )
                    };
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Success,
                        text,
                    });
                }
                Err(e) => {
                    let was_cancelled =
                        cancel.is_cancelled() || matches!(e, thk_core::ThkError::Cancelled);
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: if was_cancelled {
                            ToastKind::Info
                        } else {
                            ToastKind::Error
                        },
                        text: if was_cancelled {
                            tr_lang(
                                lang,
                                "Đã hủy cài đặt Plugin/Mod.",
                                "Cancelled addon install.",
                            )
                            .to_string()
                        } else {
                            e.to_string()
                        },
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn open_lan_share_modal(&mut self) {
        self.show_lan_modal = true;
        self.refresh_connected_network_snapshot();
    }

    pub fn refresh_connected_network_snapshot(&mut self) {
        if self.servers.probing_network {
            return;
        }
        let local = detect_local_network_snapshot();
        if let Some(ip) = local.primary_ipv4 {
            self.lan_modal.primary_lan_ip = Some(ip.to_string());
        }
        self.servers.network_snapshot = local.clone();
        self.lan_modal.network_snapshot = local;
        self.servers.probing_network = true;

        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let snap = probe_connected_network_snapshot().await;
            let _ = tx.send(AppAsyncEvent::NetworkSnapshotProbed(snap));
            ctx.request_repaint();
        });
    }

    pub fn toggle_selected_server_direct_host(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let status = self.server_supervisor.server_direct_host_status(&srv.id);
        if matches!(
            status,
            DirectHostStatus::Running(_) | DirectHostStatus::Starting { .. }
        ) {
            self.server_supervisor.stop_server_direct_host(&srv.id);
            self.servers
                .server_direct_hosts
                .insert(srv.id, DirectHostStatus::Stopped);
            return;
        }

        let Ok(host_port) = self
            .servers
            .direct_host_port_input
            .trim()
            .parse::<u16>()
            .map_err(|_| ())
            .and_then(|p| if p > 0 { Ok(p) } else { Err(()) })
        else {
            let msg = self
                .i18n
                .tr(
                    "Cổng mở không hợp lệ (1 - 65535).",
                    "Invalid host port (1 - 65535).",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        };

        let local_target_port = resolve_effective_server_port(&srv);
        let motd = self
            .servers
            .properties_map
            .get("motd")
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| srv.name.clone());
        let cfg = DirectHostConfig {
            local_target_port,
            host_port,
            bind_ipv4: self.servers.direct_bind_ip,
            motd,
            announce_on_lan: self.servers.direct_announce_lan,
            try_upnp: self.servers.direct_try_upnp,
        };

        self.servers.server_direct_hosts.insert(
            srv.id.clone(),
            DirectHostStatus::Starting {
                local_port: local_target_port,
                host_port,
            },
        );
        let sup = self.server_supervisor.clone();
        let srv_id = srv.id;
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Err(e) = sup.start_server_direct_host(&srv_id, cfg).await {
                let err_str = e.to_string();
                let _ = tx.send(AppAsyncEvent::Server(
                    ServerSupervisorEvent::DirectHostChanged {
                        server_id: srv_id,
                        status: DirectHostStatus::Error(err_str.clone()),
                    },
                ));
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Error,
                    text: format!(
                        "{}: {err_str}",
                        tr_lang(
                            lang,
                            "Không thể mở Port trên mạng hiện tại",
                            "Failed to open port on current network"
                        )
                    ),
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn toggle_selected_server_tunnel(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let status = self.server_supervisor.server_tunnel_status(&srv.id);
        if matches!(
            status,
            TunnelStatus::Running { .. } | TunnelStatus::Connecting { .. }
        ) {
            self.server_supervisor.stop_server_tunnel(&srv.id);
            self.servers
                .server_tunnels
                .insert(srv.id, TunnelStatus::Stopped);
            return;
        }

        let local_port = resolve_effective_server_port(&srv);
        let sup = self.server_supervisor.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Err(e) = sup.start_server_tunnel(&srv.id, local_port).await {
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Error,
                    text: format!(
                        "{}: {e}",
                        tr_lang(
                            lang,
                            "Không thể mở Bore Tunnel",
                            "Failed to open Bore Tunnel"
                        )
                    ),
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn sync_selected_server_ftp_config(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            self.servers.ftp_config = FtpDeployConfig::default();
            self.servers.ftp_password.clear();
            self.servers.ftp_loaded_for_id.clear();
            self.servers.ftp_last_status = None;
            return;
        };
        let cfg = self
            .db
            .get_kv(&format!("server_ftp_cfg:{}", srv.id))
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str::<FtpDeployConfig>(&raw).ok())
            .unwrap_or_default();
        let password = load_server_ftp_password(&srv.id)
            .ok()
            .flatten()
            .unwrap_or_default();
        self.servers.ftp_config = cfg;
        self.servers.ftp_password = password;
        self.servers.ftp_loaded_for_id = srv.id;
        self.servers.ftp_last_status = None;
    }

    pub fn save_selected_server_ftp_config(&mut self, show_toast: bool) -> bool {
        let Some(srv) = self.selected_server().cloned() else {
            return false;
        };
        let Ok(json) = serde_json::to_string(&self.servers.ftp_config) else {
            return false;
        };
        if let Err(e) = self
            .db
            .set_kv(&format!("server_ftp_cfg:{}", srv.id), Some(&json))
        {
            self.push_toast(ToastKind::Error, e.to_string());
            return false;
        }
        let pw_res = if self.servers.ftp_password.is_empty() {
            delete_server_ftp_password(&srv.id)
        } else {
            save_server_ftp_password(&srv.id, &self.servers.ftp_password)
        };
        if let Err(e) = pw_res {
            self.push_toast(ToastKind::Error, e.to_string());
            return false;
        }
        if show_toast {
            let msg = self
                .i18n
                .tr(
                    "Đã lưu cấu hình FTP/FTPS!",
                    "Saved FTP/FTPS hosting configuration!",
                )
                .to_string();
            self.push_toast(ToastKind::Success, msg);
        }
        true
    }

    pub fn test_selected_server_ftp(&mut self) {
        if self.servers.ftp_testing || self.servers.ftp_deploying {
            return;
        }
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let _ = self.save_selected_server_ftp_config(false);
        let cfg = self.servers.ftp_config.clone();
        let password = self.servers.ftp_password.clone();
        let server_id = srv.id;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.servers.ftp_testing = true;
        self.servers.ftp_last_status = None;

        self.rt.spawn(async move {
            let res = test_ftp_connection(&cfg, &password)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(AppAsyncEvent::ServerFtpTested {
                server_id,
                result: res,
            });
            ctx.request_repaint();
        });
    }

    pub fn deploy_selected_server_ftp(&mut self) {
        if self.servers.ftp_deploying || self.servers.ftp_testing {
            return;
        }
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let _ = self.save_selected_server_ftp_config(false);
        let cfg = self.servers.ftp_config.clone();
        let password = self.servers.ftp_password.clone();
        let server_id = srv.id.clone();
        let server_dir = srv.server_dir.clone();
        let cancel = CancellationToken::new();
        self.servers.ftp_deploying = true;
        self.servers.ftp_deploy_cancel = Some(cancel.clone());
        self.servers.ftp_last_status = None;

        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        let task_id = format!("ftp-deploy:{server_id}");

        self.rt.spawn(async move {
            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let tid_p = task_id.clone();
            let cancel_p = cancel.clone();
            let res = deploy_server_via_ftp(
                &server_dir,
                &cfg,
                &password,
                Some(cancel.clone()),
                move |p| {
                    let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                        task_id: tid_p.clone(),
                        progress: Some(p),
                        cancel: Some(cancel_p.clone()),
                    });
                    ctx_p.request_repaint();
                },
            )
            .await;

            let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                task_id,
                progress: None,
                cancel: None,
            });
            let cancelled =
                cancel.is_cancelled() || matches!(res, Err(thk_core::ThkError::Cancelled));
            let _ = tx.send(AppAsyncEvent::ServerFtpDeployed {
                server_id,
                result: res.map_err(|e| e.to_string()),
                cancelled,
            });
            ctx.request_repaint();
        });
    }

    pub fn cancel_selected_server_ftp_deploy(&mut self) {
        if let Some(tok) = self.servers.ftp_deploy_cancel.take() {
            tok.cancel();
        }
    }

    pub fn add_offline_account(&mut self) {
        match create_offline_account(
            &self.accounts.offline_username,
            self.accounts.offline_variant,
        ) {
            Ok(acct) => {
                if let Err(e) = self.db.upsert_account(&acct) {
                    let msg = format!(
                        "{}: {e}",
                        self.i18n.tr("Lỗi lưu tài khoản", "Failed to save account")
                    );
                    self.push_toast(ToastKind::Error, msg);
                    return;
                }
                let id = acct.id.clone();
                let uname = acct.username.clone();
                self.accounts.offline_username.clear();
                self.accounts.accounts = self.db.list_accounts().unwrap_or_default();
                self.select_account(id);
                let msg = if self.settings.language == Language::Vi {
                    format!("Đã thêm tài khoản Offline '{uname}'.")
                } else {
                    format!("Added Offline account '{uname}'.")
                };
                self.push_toast(ToastKind::Success, msg);
            }
            Err(e) => self.push_toast(ToastKind::Error, e.to_string()),
        }
    }

    pub fn start_ms_device_flow(&mut self) {
        if let Some(prev) = self.accounts.device_cancel.take() {
            prev.cancel();
        }
        let cancel = CancellationToken::new();
        self.accounts.device_cancel = Some(cancel.clone());
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(
            self.i18n
                .tr(
                    "Đang khởi tạo mã đăng nhập Microsoft...",
                    "Initializing Microsoft Device Code flow...",
                )
                .to_string(),
        );

        self.rt.spawn(async move {
            match start_device_code_login(true).await {
                Ok(sess) => {
                    let _ = tx.send(AppAsyncEvent::BusyStatus(None));
                    let _ = tx.send(AppAsyncEvent::DeviceCodeReady(sess.clone()));
                    ctx.request_repaint();

                    match poll_device_code_login(&sess, cancel).await {
                        Ok(authed) => {
                            let _ =
                                tx.send(AppAsyncEvent::MicrosoftAccountLoggedIn(authed.account));
                        }
                        Err(thk_core::ThkError::Cancelled) => {}
                        Err(e) => {
                            let _ = tx.send(AppAsyncEvent::AuthError(e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::BusyStatus(None));
                    let _ = tx.send(AppAsyncEvent::AuthError(e.to_string()));
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn open_ms_desktop_redirect_flow(&mut self) {
        self.accounts.desktop_challenge = Some(build_desktop_auth_challenge(true));
        self.accounts.desktop_redirect_input.clear();
    }

    pub fn submit_ms_desktop_redirect(&mut self) {
        let Some(challenge) = self.accounts.desktop_challenge.clone() else {
            return;
        };
        let pasted = self.accounts.desktop_redirect_input.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(
            self.i18n
                .tr(
                    "Đang xác thực tài khoản Microsoft & Xbox Live...",
                    "Authenticating Microsoft & Xbox Live session...",
                )
                .to_string(),
        );

        self.rt.spawn(async move {
            let res = complete_desktop_auth_challenge(&challenge, &pasted).await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(authed) => {
                    let _ = tx.send(AppAsyncEvent::MicrosoftAccountLoggedIn(authed.account));
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::AuthError(e.to_string()));
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn start_ms_loopback_pkce_flow(&mut self) {
        let cid = self.accounts.custom_client_id.trim().to_string();
        if cid.is_empty() {
            let msg = self
                .i18n
                .tr(
                    "Vui lòng nhập Azure App Client ID trước khi dùng Loopback PKCE.",
                    "Please enter an Azure App Client ID before using Loopback PKCE.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        if let Some(prev) = self.accounts.loopback_cancel.take() {
            prev.cancel();
        }
        let cancel = CancellationToken::new();
        self.accounts.loopback_cancel = Some(cancel.clone());
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(
            self.i18n
                .tr(
                    "Đang chờ phản hồi OAuth2 trên trình duyệt...",
                    "Waiting for browser OAuth2 callback...",
                )
                .to_string(),
        );

        self.rt.spawn(async move {
            let res = login_with_loopback_pkce(&cid, cancel).await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(authed) => {
                    let _ = tx.send(AppAsyncEvent::MicrosoftAccountLoggedIn(authed.account));
                }
                Err(thk_core::ThkError::Cancelled) => {}
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::AuthError(e.to_string()));
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn refresh_selected_ms_token(&mut self) {
        let Some(acct) = self.selected_account().cloned() else {
            return;
        };
        if acct.account_type != AccountType::Microsoft {
            return;
        }
        let cid = {
            let t = self.accounts.custom_client_id.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(
            self.i18n
                .tr(
                    "Đang làm mới phiên đăng nhập & đồng bộ Skin/Cape Mojang...",
                    "Refreshing Microsoft session & syncing Mojang Skin/Cape...",
                )
                .to_string(),
        );

        self.rt.spawn(async move {
            let res = force_refresh_account_session(&acct, cid.as_deref()).await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(authed) => {
                    let _ = tx.send(AppAsyncEvent::MicrosoftAccountLoggedIn(authed.account));
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::AuthError(e.to_string()));
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn delete_selected_account(&mut self) {
        let Some(acct) = self.selected_account().cloned() else {
            return;
        };
        let _ = TokenStore::delete_tokens(&acct.id);
        let _ = self.db.delete_account(&acct.id);
        self.accounts.accounts = self.db.list_accounts().unwrap_or_default();
        self.accounts.selected_account_id = self.accounts.accounts.first().map(|a| a.id.clone());
        self.accounts.loaded_viewport_key.clear();
        let msg = if self.settings.language == Language::Vi {
            format!("Đã xóa tài khoản '{}'.", acct.username)
        } else {
            format!("Removed account '{}'.", acct.username)
        };
        self.push_toast(ToastKind::Info, msg);
    }

    pub fn update_selected_account_variant(&mut self, variant: SkinVariant) {
        let Some(mut acct) = self.selected_account().cloned() else {
            return;
        };
        acct.skin_variant = variant;
        if self.db.upsert_account(&acct).is_ok() {
            self.accounts.accounts = self.db.list_accounts().unwrap_or_default();
            if let Some(vp) = self.accounts.skin_viewport.as_mut() {
                vp.variant = variant;
            }
        }
        if acct.account_type == AccountType::Microsoft {
            let skin_file = acct
                .custom_skin_path
                .clone()
                .filter(|p| p.is_file())
                .or_else(|| {
                    acct.skin_url.as_ref().map(|url| {
                        let hash = sha1_bytes(url.as_bytes());
                        self.paths
                            .skins_dir
                            .join(format!("remote_{}.png", &hash[..16]))
                    })
                })
                .filter(|p| p.is_file());
            if let Some(png_path) = skin_file {
                let cid = {
                    let t = self.accounts.custom_client_id.trim();
                    (!t.is_empty()).then(|| t.to_string())
                };
                let db = Arc::clone(&self.db);
                let tx = self.ui_tx.clone();
                let ctx = self.egui_ctx.clone();
                self.rt.spawn(async move {
                    if let Ok(bytes) = tokio::fs::read(&png_path).await {
                        if upload_mojang_skin(&mut acct, cid.as_deref(), variant, bytes)
                            .await
                            .is_ok()
                        {
                            let _ = db.upsert_account(&acct);
                            let _ = tx.send(AppAsyncEvent::AccountProfileSynced { account: acct });
                            ctx.request_repaint();
                        }
                    }
                });
            }
        }
    }

    pub fn pick_account_texture(&mut self, slot: TextureSlot) {
        let Some(acct) = self.selected_account().cloned() else {
            return;
        };
        let env = TextureUploadEnv {
            paths: self.paths.clone(),
            cid: {
                let t = self.accounts.custom_client_id.trim();
                (!t.is_empty()).then(|| t.to_string())
            },
            lang: self.settings.language,
            db: Arc::clone(&self.db),
            tx: self.ui_tx.clone(),
            ctx: self.egui_ctx.clone(),
        };

        self.rt.spawn(async move {
            let Some(file_path) = crate::dialogs::pick_file_async(
                tr_lang(
                    env.lang,
                    "Chọn ảnh PNG (64x64 hoặc 64x32)",
                    "Select PNG Texture (64x64 or 64x32)",
                ),
                Some("PNG Texture"),
                &["png"],
            )
            .await
            else {
                return;
            };
            process_and_save_account_texture(acct, slot, file_path, env).await;
        });
    }

    pub fn apply_account_texture_from_path(&mut self, slot: TextureSlot, src_path: PathBuf) {
        let Some(acct) = self.selected_account().cloned() else {
            return;
        };
        let env = TextureUploadEnv {
            paths: self.paths.clone(),
            cid: {
                let t = self.accounts.custom_client_id.trim();
                (!t.is_empty()).then(|| t.to_string())
            },
            lang: self.settings.language,
            db: Arc::clone(&self.db),
            tx: self.ui_tx.clone(),
            ctx: self.egui_ctx.clone(),
        };

        self.rt.spawn(async move {
            process_and_save_account_texture(acct, slot, src_path, env).await;
        });
    }

    pub fn reset_selected_account_textures(&mut self) {
        let Some(mut acct) = self.selected_account().cloned() else {
            return;
        };
        if let Some(p) = acct.custom_skin_path.take() {
            let _ = std::fs::remove_file(p);
        }
        if let Some(p) = acct.custom_cape_path.take() {
            let _ = std::fs::remove_file(p);
        }
        if let Some(p) = acct.custom_elytra_path.take() {
            let _ = std::fs::remove_file(p);
        }
        let _ = self.db.upsert_account(&acct);
        self.accounts.accounts = self.db.list_accounts().unwrap_or_default();
        self.accounts.loaded_viewport_key.clear();
        self.accounts.last_back_mode_account_id = None;
        if acct.cape_url.is_none() {
            if let Some(vp) = self.accounts.skin_viewport.as_mut() {
                vp.back_mode = BackEquipmentMode::None;
                vp.mark_dirty();
            }
        }

        if acct.account_type == AccountType::Microsoft {
            let cid = {
                let t = self.accounts.custom_client_id.trim();
                (!t.is_empty()).then(|| t.to_string())
            };
            let lang = self.settings.language;
            let db = Arc::clone(&self.db);
            let tx = self.ui_tx.clone();
            let ctx = self.egui_ctx.clone();
            self.rt.spawn(async move {
                match reset_mojang_skin(&mut acct, cid.as_deref()).await {
                    Ok(()) => {
                        let _ = db.upsert_account(&acct);
                        let _ = tx.send(AppAsyncEvent::AccountProfileSynced { account: acct });
                        let _ = tx.send(AppAsyncEvent::Toast {
                            kind: ToastKind::Success,
                            text: tr_lang(
                                lang,
                                "Đã khôi phục Skin/Cape mặc định trên Mojang Cloud!",
                                "Reset Skin/Cape to default on Mojang Cloud!",
                            )
                            .to_string(),
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(AppAsyncEvent::Toast {
                            kind: ToastKind::Info,
                            text: format!(
                                "{}: {e}",
                                tr_lang(
                                    lang,
                                    "Đã xóa Skin cục bộ (chưa đồng bộ Mojang)",
                                    "Cleared local skin (Mojang sync skipped)"
                                )
                            ),
                        });
                    }
                }
                ctx.request_repaint();
            });
        } else {
            let msg = self
                .i18n
                .tr(
                    "Đã khôi phục Skin & Cape mặc định.",
                    "Reset Skin & Cape to default.",
                )
                .to_string();
            self.push_toast(ToastKind::Info, msg);
        }
    }

    pub fn sync_3d_viewport_textures(&mut self, render_state: Option<&egui_wgpu::RenderState>) {
        let Some(rs) = render_state else {
            return;
        };
        if self.accounts.skin_viewport.is_none() {
            let mut vp = SkinPreviewViewport::new(rs);
            vp.low_power = self.settings.low_ram_mode;
            self.accounts.skin_viewport = Some(vp);
        } else if let Some(vp) = self.accounts.skin_viewport.as_mut() {
            vp.low_power = self.settings.low_ram_mode;
        }
        let Some(acct) = self.selected_account().cloned() else {
            return;
        };

        if acct.account_type == AccountType::Microsoft
            && acct.skin_url.is_none()
            && !self.accounts.synced_profile_ids.contains(&acct.id)
        {
            self.accounts.synced_profile_ids.insert(acct.id.clone());
            let mut acct_sync = acct.clone();
            let cid = {
                let t = self.accounts.custom_client_id.trim();
                (!t.is_empty()).then(|| t.to_string())
            };
            let db = Arc::clone(&self.db);
            let tx = self.ui_tx.clone();
            let ctx = self.egui_ctx.clone();
            self.rt.spawn(async move {
                if sync_mojang_profile(&mut acct_sync, cid.as_deref())
                    .await
                    .is_ok()
                {
                    let _ = db.upsert_account(&acct_sync);
                    let _ = tx.send(AppAsyncEvent::AccountProfileSynced { account: acct_sync });
                    ctx.request_repaint();
                }
            });
        }

        let remote_skin_cache = acct.skin_url.as_ref().map(|url| {
            let hash = sha1_bytes(url.as_bytes());
            (
                url.clone(),
                self.paths
                    .skins_dir
                    .join(format!("remote_{}.png", &hash[..16])),
            )
        });
        if acct.custom_skin_path.is_none() {
            if let Some((ref url, ref cache_path)) = remote_skin_cache {
                if !cache_path.is_file() && !self.accounts.remote_tex_fetching.contains(url) {
                    self.accounts.remote_tex_fetching.insert(url.clone());
                    let fetch_url = url.clone();
                    let dest = cache_path.clone();
                    let dl = self.downloader.clone();
                    let tx = self.ui_tx.clone();
                    let ctx = self.egui_ctx.clone();
                    self.rt.spawn(async move {
                        if let Ok(bytes) = dl.get_bytes(&fetch_url).await {
                            let saved = tokio::task::spawn_blocking(move || {
                                if let Ok(rgba) = load_and_normalize_skin_png(&bytes) {
                                    if let Some(parent) = dest.parent() {
                                        let _ = std::fs::create_dir_all(parent);
                                    }
                                    let _ = rgba.save(&dest);
                                }
                            })
                            .await;
                            let _ = saved;
                        }
                        let _ = tx.send(AppAsyncEvent::RemoteAccountTextureCached {
                            cache_key: fetch_url,
                        });
                        ctx.request_repaint();
                    });
                }
            }
        }

        let remote_cape_cache = acct.cape_url.as_ref().map(|url| {
            let hash = sha1_bytes(url.as_bytes());
            (
                url.clone(),
                self.paths
                    .capes_dir
                    .join(format!("remote_{}.png", &hash[..16])),
            )
        });
        if acct.custom_cape_path.is_none() {
            if let Some((ref url, ref cache_path)) = remote_cape_cache {
                if !cache_path.is_file() && !self.accounts.remote_tex_fetching.contains(url) {
                    self.accounts.remote_tex_fetching.insert(url.clone());
                    let fetch_url = url.clone();
                    let dest = cache_path.clone();
                    let dl = self.downloader.clone();
                    let tx = self.ui_tx.clone();
                    let ctx = self.egui_ctx.clone();
                    self.rt.spawn(async move {
                        if let Ok(bytes) = dl.get_bytes(&fetch_url).await {
                            let saved = tokio::task::spawn_blocking(move || {
                                if let Ok(rgba) = load_and_normalize_cape_png(&bytes) {
                                    if let Some(parent) = dest.parent() {
                                        let _ = std::fs::create_dir_all(parent);
                                    }
                                    let _ = rgba.save(&dest);
                                }
                            })
                            .await;
                            let _ = saved;
                        }
                        let _ = tx.send(AppAsyncEvent::RemoteAccountTextureCached {
                            cache_key: fetch_url,
                        });
                        ctx.request_repaint();
                    });
                }
            }
        }

        let effective_skin = acct
            .custom_skin_path
            .clone()
            .filter(|p| p.is_file())
            .or_else(|| {
                remote_skin_cache
                    .as_ref()
                    .map(|(_, p)| p.clone())
                    .filter(|p| p.is_file())
            });
        let effective_cape = acct
            .custom_cape_path
            .clone()
            .filter(|p| p.is_file())
            .or_else(|| {
                remote_cape_cache
                    .as_ref()
                    .map(|(_, p)| p.clone())
                    .filter(|p| p.is_file())
            });
        let effective_elytra = acct.custom_elytra_path.clone().filter(|p| p.is_file());

        if self.accounts.last_back_mode_account_id.as_deref() != Some(&acct.id) {
            self.accounts.last_back_mode_account_id = Some(acct.id.clone());
            if let Some(vp) = self.accounts.skin_viewport.as_mut() {
                vp.back_mode = if effective_elytra.is_some() {
                    BackEquipmentMode::Elytra
                } else if effective_cape.is_some() || acct.cape_url.is_some() {
                    BackEquipmentMode::Cape
                } else {
                    BackEquipmentMode::None
                };
                vp.mark_dirty();
            }
        }

        let back_mode = self
            .accounts
            .skin_viewport
            .as_ref()
            .map(|vp| vp.back_mode)
            .unwrap_or(BackEquipmentMode::None);

        let key = format!(
            "{}:{}:{:?}:{:?}:{:?}:{:?}",
            acct.id,
            acct.skin_variant.as_str(),
            back_mode,
            effective_skin,
            effective_cape,
            effective_elytra
        );
        if self.accounts.loaded_viewport_key == key {
            return;
        }
        self.accounts.loaded_viewport_key = key;

        if let Some(vp) = self.accounts.skin_viewport.as_mut() {
            vp.variant = acct.skin_variant;
            match effective_skin
                .as_deref()
                .and_then(|p| load_and_normalize_skin_file(p).ok())
                .or_else(|| {
                    extract_vanilla_steve_skin(
                        &self.paths.versions_dir,
                        Some(&self.paths.cache_dir),
                        acct.skin_variant,
                    )
                }) {
                Some(img) => vp.set_skin_rgba(rs, &img),
                None => vp.set_default_skin_for_variant(rs, acct.skin_variant),
            }
            let back_path = match vp.back_mode {
                BackEquipmentMode::Elytra => effective_elytra.as_ref().or(effective_cape.as_ref()),
                _ => effective_cape.as_ref().or(effective_elytra.as_ref()),
            };
            match back_path.and_then(|bp| load_and_normalize_cape_file(bp).ok()) {
                Some(img) => vp.set_cape_rgba(rs, &img),
                None => vp.reset_default_cape(rs),
            }
        }
    }

    pub fn open_java_manager(&mut self) {
        self.show_java_modal = true;
        self.rescan_java_runtimes();
        self.load_java_distro_packages(self.java_modal.selected_distro);
    }

    pub fn rescan_java_runtimes(&self) {
        let runtimes = self.paths.runtimes_dir.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Ok(list) =
                tokio::task::spawn_blocking(move || scan_java_installations(&runtimes)).await
            {
                let _ = tx.send(AppAsyncEvent::JavaScanCompleted(list));
                ctx.request_repaint();
            }
        });
    }

    pub fn load_java_distro_packages(&mut self, distro: JavaDistroKind) {
        self.java_modal.selected_distro = distro;
        self.java_modal.packages.clear();
        let dl = self.downloader.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(if lang == Language::Vi {
            format!("Đang tải danh sách {}...", distro.display_name())
        } else {
            format!("Loading {} packages...", distro.display_name())
        });

        self.rt.spawn(async move {
            let res = fetch_available_distro_packages(&dl, distro).await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(packages) => {
                    let _ = tx.send(AppAsyncEvent::JavaDistroPackagesLoaded { distro, packages });
                }
                Err(e) => {
                    let text = if lang == Language::Vi {
                        format!("Không thể tải {}: {e}", distro.display_name())
                    } else {
                        format!("Failed to fetch {}: {e}", distro.display_name())
                    };
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text,
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn install_java_package(&mut self, pkg: DistroReleasePackage) {
        let dl = self.downloader.clone();
        let runtimes = self.paths.runtimes_dir.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        let cancel = CancellationToken::new();
        let task_id = format!("java:{}:{}", pkg.distro.display_name(), pkg.major_version);

        self.rt.spawn(async move {
            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let tid_p = task_id.clone();
            let cancel_p = cancel.clone();
            let res =
                install_distro_package(&dl, &runtimes, &pkg, Some(cancel.clone()), move |p| {
                    let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                        task_id: tid_p.clone(),
                        progress: Some(p),
                        cancel: Some(cancel_p.clone()),
                    });
                    ctx_p.request_repaint();
                })
                .await;

            let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                task_id,
                progress: None,
                cancel: None,
            });
            match res {
                Ok(inst) => {
                    let list =
                        tokio::task::spawn_blocking(move || scan_java_installations(&runtimes))
                            .await
                            .unwrap_or_default();
                    let _ = tx.send(AppAsyncEvent::JavaScanCompleted(list));
                    let text = if lang == Language::Vi {
                        format!("Đã cài đặt Java {} ({})!", inst.major_version, inst.vendor)
                    } else {
                        format!("Installed Java {} ({})!", inst.major_version, inst.vendor)
                    };
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Success,
                        text,
                    });
                }
                Err(e) => {
                    let was_cancelled =
                        cancel.is_cancelled() || matches!(e, thk_core::ThkError::Cancelled);
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: if was_cancelled {
                            ToastKind::Info
                        } else {
                            ToastKind::Error
                        },
                        text: if was_cancelled {
                            tr_lang(lang, "Đã hủy cài đặt Java.", "Cancelled Java installation.")
                                .to_string()
                        } else {
                            format!(
                                "{}: {e}",
                                tr_lang(lang, "Cài đặt Java thất bại", "Java installation failed")
                            )
                        },
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn sync_lan_auto_host_template(&self) {
        let template = if self.lan_modal.auto_open_direct_host {
            self.lan_modal
                .direct_host_port_input
                .trim()
                .parse::<u16>()
                .ok()
                .filter(|&p| p > 0)
                .map(|host_port| DirectHostConfig {
                    local_target_port: 25565,
                    host_port,
                    bind_ipv4: self.lan_modal.direct_bind_ip,
                    motd: self
                        .lan_modal
                        .detected_world
                        .as_ref()
                        .map(|w| w.motd.clone())
                        .unwrap_or_default(),
                    announce_on_lan: self.lan_modal.direct_announce_lan,
                    try_upnp: self.lan_modal.direct_try_upnp,
                })
        } else {
            None
        };
        self.lan_coordinator.set_auto_direct_host_template(template);
    }

    pub fn toggle_lan_scanner(&mut self) {
        if self.lan_coordinator.is_scanning() {
            self.lan_coordinator.stop_scan();
        } else {
            self.sync_lan_auto_host_template();
            if let Err(e) = self
                .lan_coordinator
                .start_lan_scan(&self.rt, self.lan_modal.auto_open_tunnel)
            {
                self.push_toast(ToastKind::Error, e.to_string());
            }
        }
    }

    pub fn toggle_lan_direct_host(&mut self) {
        let current = self.lan_coordinator.direct_host_status();
        if matches!(
            current,
            DirectHostStatus::Running(_) | DirectHostStatus::Starting { .. }
        ) {
            self.lan_coordinator.stop_direct_host();
            self.lan_modal.direct_host_status = DirectHostStatus::Stopped;
            return;
        }

        let Ok(local_target_port) = self
            .lan_modal
            .manual_port_input
            .trim()
            .parse::<u16>()
            .map_err(|_| ())
            .and_then(|p| if p > 0 { Ok(p) } else { Err(()) })
        else {
            let msg = self
                .i18n
                .tr(
                    "Cổng thế giới nội bộ không hợp lệ (1 - 65535).",
                    "Invalid local world port (1 - 65535).",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        };

        let Ok(host_port) = self
            .lan_modal
            .direct_host_port_input
            .trim()
            .parse::<u16>()
            .map_err(|_| ())
            .and_then(|p| if p > 0 { Ok(p) } else { Err(()) })
        else {
            let msg = self
                .i18n
                .tr(
                    "Cổng mở cố định không hợp lệ (1 - 65535).",
                    "Invalid host port (1 - 65535).",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        };

        let motd = self
            .lan_modal
            .detected_world
            .as_ref()
            .map(|w| w.motd.clone())
            .unwrap_or_else(|| "Minecraft LAN World".to_string());
        let cfg = DirectHostConfig {
            local_target_port,
            host_port,
            bind_ipv4: self.lan_modal.direct_bind_ip,
            motd,
            announce_on_lan: self.lan_modal.direct_announce_lan,
            try_upnp: self.lan_modal.direct_try_upnp,
        };

        self.lan_modal.direct_host_status = DirectHostStatus::Starting {
            local_port: local_target_port,
            host_port,
        };
        let coord = self.lan_coordinator.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Err(e) = coord.start_direct_host(cfg).await {
                let err_str = e.to_string();
                let _ = tx.send(AppAsyncEvent::Lan(LanTunnelEvent::DirectHostStatusChanged(
                    DirectHostStatus::Error(err_str.clone()),
                )));
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Error,
                    text: format!(
                        "{}: {err_str}",
                        tr_lang(
                            lang,
                            "Không thể mở Port trên mạng hiện tại",
                            "Failed to open port on current network"
                        )
                    ),
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn start_manual_bore_tunnel(&mut self) {
        let Ok(port) = self.lan_modal.manual_port_input.trim().parse::<u16>() else {
            let msg = self
                .i18n
                .tr(
                    "Cổng nội bộ không hợp lệ (1 - 65535).",
                    "Invalid local port (1 - 65535).",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        };
        if port == 0 {
            let msg = self
                .i18n
                .tr(
                    "Cổng nội bộ phải lớn hơn 0.",
                    "Local port must be greater than 0.",
                )
                .to_string();
            self.push_toast(ToastKind::Error, msg);
            return;
        }
        let coord = self.lan_coordinator.clone();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Err(e) = coord.start_manual_tunnel(port).await {
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Error,
                    text: format!(
                        "{}: {e}",
                        tr_lang(
                            lang,
                            "Mở Bore Tunnel thất bại",
                            "Failed to open Bore Tunnel"
                        )
                    ),
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn auto_fix_crash_mod(&mut self, profile_id: &str, mod_id: &str) {
        let Some(prof) = self
            .home
            .profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
        else {
            return;
        };
        let dl = self.downloader.clone();
        let db = Arc::clone(&self.db);
        let mod_owned = mod_id.to_string();
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        let cancel = CancellationToken::new();
        let task_id = format!("crash-fix:{mod_owned}");

        self.rt.spawn(async move {
            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let tid_p = task_id.clone();
            let cancel_p = cancel.clone();
            let res = auto_fix_mod_from_modrinth(
                &dl,
                &db,
                &prof,
                &mod_owned,
                Some(cancel.clone()),
                move |p| {
                    let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                        task_id: tid_p.clone(),
                        progress: Some(p),
                        cancel: Some(cancel_p.clone()),
                    });
                    ctx_p.request_repaint();
                },
            )
            .await;

            let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                task_id,
                progress: None,
                cancel: None,
            });
            match res {
                Ok(_) => {
                    if let Ok(items) =
                        list_local_content(&db, &prof.id, &prof.instance_path, ContentKind::Mod)
                    {
                        let _ = tx.send(AppAsyncEvent::LocalContentRefreshed {
                            profile_id: prof.id,
                            kind: ContentKind::Mod,
                            items,
                        });
                    }
                    let text = if lang == Language::Vi {
                        format!("Đã tự động tải và cài đặt '{mod_owned}' từ Modrinth!")
                    } else {
                        format!(
                            "Automatically downloaded and installed '{mod_owned}' from Modrinth!"
                        )
                    };
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Success,
                        text,
                    });
                }
                Err(e) => {
                    let was_cancelled =
                        cancel.is_cancelled() || matches!(e, thk_core::ThkError::Cancelled);
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: if was_cancelled {
                            ToastKind::Info
                        } else {
                            ToastKind::Error
                        },
                        text: if was_cancelled {
                            tr_lang(lang, "Đã hủy tự động sửa Mod.", "Cancelled mod auto-fix.")
                                .to_string()
                        } else {
                            e.to_string()
                        },
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn auto_upgrade_profile_loader(&mut self, profile_id: &str, min_version: Option<&str>) {
        let Some(mut prof) = self
            .home
            .profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
        else {
            return;
        };
        if prof.loader == LoaderType::Vanilla {
            let msg = self
                .i18n
                .tr(
                    "Profile này đang ở chế độ Vanilla nên không có Mod Loader để nâng cấp.",
                    "This profile uses Vanilla and has no Mod Loader to upgrade.",
                )
                .to_string();
            self.push_toast(ToastKind::Info, msg);
            return;
        }

        let dl = self.downloader.clone();
        let db = Arc::clone(&self.db);
        let min_req = min_version.map(|s| s.trim().to_string());
        let lang = self.settings.language;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(if lang == Language::Vi {
            format!(
                "Đang tra cứu bản {} mới nhất cho Minecraft {}...",
                prof.loader.display_name(),
                prof.game_version
            )
        } else {
            format!(
                "Fetching latest {} loader for Minecraft {}...",
                prof.loader.display_name(),
                prof.game_version
            )
        });

        self.rt.spawn(async move {
            let list_res = list_loader_versions(&dl, prof.loader, &prof.game_version).await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match list_res {
                Ok(entries) => {
                    let chosen = entries
                        .iter()
                        .find(|e| {
                            e.stable
                                && min_req
                                    .as_deref()
                                    .is_none_or(|req| is_semver_at_least(&e.version, req))
                        })
                        .or_else(|| {
                            entries.iter().find(|e| {
                                min_req
                                    .as_deref()
                                    .is_none_or(|req| is_semver_at_least(&e.version, req))
                            })
                        })
                        .or_else(|| entries.iter().find(|e| e.stable))
                        .or_else(|| entries.first())
                        .cloned();

                    let Some(target) = chosen else {
                        let _ = tx.send(AppAsyncEvent::Toast {
                            kind: ToastKind::Error,
                            text: if lang == Language::Vi {
                                format!(
                                    "Không tìm thấy phiên bản {} tương thích với Minecraft {}.",
                                    prof.loader.display_name(),
                                    prof.game_version
                                )
                            } else {
                                format!(
                                    "No compatible {} loader version found for Minecraft {}.",
                                    prof.loader.display_name(),
                                    prof.game_version
                                )
                            },
                        });
                        ctx.request_repaint();
                        return;
                    };

                    prof.loader_version = Some(target.version.clone());
                    match db.upsert_profile(&prof) {
                        Ok(()) => {
                            let _ = tx.send(AppAsyncEvent::ProfileLoaderUpgraded {
                                profile_id: prof.id,
                                loader: prof.loader,
                                new_version: target.version,
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(AppAsyncEvent::Toast {
                                kind: ToastKind::Error,
                                text: e.to_string(),
                            });
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: e.to_string(),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn check_app_updates(&mut self) {
        if self.updater.checking {
            return;
        }
        let repo = self.settings.github_repo.clone();
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.updater.checking = true;
        self.updater.last_error = false;
        self.updater.status_line = Some(
            self.i18n
                .tr(
                    "Đang kiểm tra bản phát hành mới trên GitHub...",
                    "Checking GitHub Releases for updates...",
                )
                .to_string(),
        );

        self.rt.spawn(async move {
            let result = check_latest_release(&dl, &repo, env!("CARGO_PKG_VERSION"))
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(AppAsyncEvent::UpdateCheckCompleted(result));
            ctx.request_repaint();
        });
    }

    pub fn download_verified_app_update(&mut self) {
        let Some(rel) = self.updater.available.clone() else {
            return;
        };
        let dl = self.downloader.clone();
        let updates_dir = self.paths.cache_dir.join("updates");
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let res = download_verified_update(&dl, &rel, &updates_dir, move |p| {
                let _ = tx_p.send(AppAsyncEvent::KeyedTaskProgress {
                    task_id: "app-update".into(),
                    progress: Some(p),
                    cancel: None,
                });
                ctx_p.request_repaint();
            })
            .await
            .map_err(|e| e.to_string());

            let _ = tx.send(AppAsyncEvent::KeyedTaskProgress {
                task_id: "app-update".into(),
                progress: None,
                cancel: None,
            });
            let _ = tx.send(AppAsyncEvent::UpdateDownloaded(res));
            ctx.request_repaint();
        });
    }

    pub fn apply_verified_app_update(&mut self) {
        if self.updater.applying {
            return;
        }
        let Some(update) = self.updater.downloaded_file.clone() else {
            return;
        };
        self.updater.applying = true;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            let result = tokio::task::spawn_blocking(move || apply_downloaded_update(&update))
                .await
                .map_err(|e| e.to_string())
                .and_then(|result| result.map_err(|e| e.to_string()));
            let _ = tx.send(AppAsyncEvent::UpdateApplyCompleted(result));
            ctx.request_repaint();
        });
    }

    fn handle_update_apply_completed(&mut self, result: Result<bool, String>) {
        self.updater.applying = false;
        match result {
            Ok(true) => {
                self.allow_immediate_quit = true;
                self.egui_ctx
                    .send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Ok(false) => {
                let msg = self
                    .i18n
                    .tr(
                        "Đã mở trình cài đặt bản cập nhật.",
                        "Launched update package.",
                    )
                    .to_string();
                self.push_toast(ToastKind::Info, msg);
            }
            Err(e) => self.push_toast(ToastKind::Error, e),
        }
    }

    pub fn open_updates_folder(&mut self) {
        let dir = self.paths.cache_dir.join("updates");
        let _ = std::fs::create_dir_all(&dir);
        self.open_local_folder_or_toast(&dir);
    }

    pub fn drain_async_events(&mut self) {
        let now = Instant::now();
        while self
            .toasts
            .front()
            .is_some_and(|front| front.expires_at <= now)
        {
            self.toasts.pop_front();
        }

        if cfg!(target_os = "android")
            && now.duration_since(self.last_android_poll) >= Duration::from_millis(250)
        {
            self.last_android_poll = now;
            self.android_permissions = thk_core::read_android_permission_status();
            if let Some(focus_req) = thk_core::poll_android_focus_request() {
                match focus_req.as_str() {
                    "hub" => {
                        if self.mobile_game.screen_open {
                            let sess = self
                                .instance_supervisor
                                .active_mobile_session()
                                .map(|(_, s)| s);
                            self.mobile_game.reset_transient_inputs(sess.as_ref());
                            self.mobile_game.hub_visible_over_game = true;
                        }
                    }
                    "game" => {
                        self.open_mobile_game_screen(false);
                    }
                    _ => {}
                }
            }
        }

        while let Ok(ev) = self.ui_rx.try_recv() {
            match ev {
                AppAsyncEvent::Toast { kind, text } => self.push_toast(kind, text),
                AppAsyncEvent::KeyedTaskProgress {
                    task_id,
                    progress,
                    cancel,
                } => self.update_keyed_task(task_id, progress, cancel, None),
                AppAsyncEvent::UpdateApplyCompleted(result) => {
                    self.handle_update_apply_completed(result)
                }
                AppAsyncEvent::BusyStatus(msg) => self.busy_message = msg,
                AppAsyncEvent::Launcher(lev) => self.handle_launcher_event(lev),
                AppAsyncEvent::Server(sev) => self.handle_server_event(sev),
                AppAsyncEvent::Lan(lanev) => self.handle_lan_event(lanev),
                AppAsyncEvent::MojangVersionsLoaded(vers) => {
                    self.home.mojang_versions = vers;
                }
                AppAsyncEvent::LoaderVersionsLoaded {
                    mc_version,
                    loader,
                    entries,
                } => {
                    if self.home.new_draft.game_version == mc_version
                        && self.home.new_draft.loader == loader
                        && self.home.new_draft.loader_version.is_empty()
                    {
                        if let Some(first) = entries.first() {
                            self.home.new_draft.loader_version = first.version.clone();
                        }
                    }
                    self.home
                        .loader_versions
                        .insert((mc_version, loader), entries);
                }
                AppAsyncEvent::LocalContentRefreshed {
                    profile_id,
                    kind,
                    items,
                } => {
                    let matches_selected =
                        self.home.selected_profile_id.as_deref() == Some(&profile_id);
                    let matches_tab = matches!(
                        (self.home.active_tab, kind),
                        (HomeTab::Mods, ContentKind::Mod)
                            | (HomeTab::Shaders, ContentKind::Shader)
                            | (HomeTab::ResourcePacks, ContentKind::ResourcePack)
                    );
                    if matches_selected && matches_tab {
                        self.home.local_items = items;
                    }
                }
                AppAsyncEvent::InlineSearchCompleted {
                    append,
                    hits,
                    total_hits,
                } => {
                    self.home.online_total_hits = total_hits;
                    if append {
                        for hit in hits {
                            if !self
                                .home
                                .online_hits
                                .iter()
                                .any(|existing| existing.project_id == hit.project_id)
                            {
                                self.home.online_hits.push(hit);
                            }
                        }
                    } else {
                        self.home.online_hits = hits;
                    }
                }
                AppAsyncEvent::InlineProjectSelected { detail, versions } => {
                    self.home.online_chosen_ver_idx = 0;
                    self.home.online_selected = Some((detail, versions));
                }
                AppAsyncEvent::ContentUpdatesChecked {
                    profile_id,
                    kind,
                    result,
                } => {
                    self.home.checking_updates = false;
                    let matches_selected =
                        self.home.selected_profile_id.as_deref() == Some(&profile_id);
                    let matches_tab = matches!(
                        (self.home.active_tab, kind),
                        (HomeTab::Mods, ContentKind::Mod)
                            | (HomeTab::Shaders, ContentKind::Shader)
                            | (HomeTab::ResourcePacks, ContentKind::ResourcePack)
                    );
                    match result {
                        Ok(candidates) => {
                            let count = candidates.len();
                            if matches_selected && matches_tab {
                                self.home.content_updates = candidates;
                            }
                            let msg = if count == 0 {
                                self.i18n
                                    .tr(
                                        "Tất cả gói nội dung đều đã ở phiên bản mới nhất!",
                                        "All installed packages are up to date!",
                                    )
                                    .to_string()
                            } else if self.settings.language == Language::Vi {
                                format!("Phát hiện {count} bản cập nhật mới!")
                            } else {
                                format!("Found {count} available update(s)!")
                            };
                            self.push_toast(
                                if count == 0 {
                                    ToastKind::Success
                                } else {
                                    ToastKind::Info
                                },
                                msg,
                            );
                        }
                        Err(e) => self.push_toast(ToastKind::Error, e),
                    }
                }
                AppAsyncEvent::ContentUpdatesApplied {
                    profile_id,
                    kind,
                    succeeded_files,
                    total_attempted,
                    first_err,
                    cancelled,
                } => {
                    let ok_count = succeeded_files.len();
                    if self.home.selected_profile_id.as_deref() == Some(&profile_id) {
                        self.home
                            .content_updates
                            .retain(|u| !succeeded_files.contains(&u.file_name));
                        let matches_tab = matches!(
                            (self.home.active_tab, kind),
                            (HomeTab::Mods, ContentKind::Mod)
                                | (HomeTab::Shaders, ContentKind::Shader)
                                | (HomeTab::ResourcePacks, ContentKind::ResourcePack)
                        );
                        if matches_tab && ok_count > 0 {
                            self.refresh_selected_profile_tabs();
                        }
                    }
                    let (t_kind, msg) = if cancelled {
                        (
                            ToastKind::Info,
                            if self.settings.language == Language::Vi {
                                format!(
                                    "Đã hủy cập nhật sau khi hoàn tất {ok_count}/{total_attempted} gói."
                                )
                            } else {
                                format!(
                                    "Cancelled updates after completing {ok_count}/{total_attempted} package(s)."
                                )
                            },
                        )
                    } else if let Some(err) = first_err {
                        (
                            ToastKind::Error,
                            if self.settings.language == Language::Vi {
                                format!("Đã cập nhật {ok_count}/{total_attempted} gói. Lỗi: {err}")
                            } else {
                                format!(
                                    "Updated {ok_count}/{total_attempted} package(s). Error: {err}"
                                )
                            },
                        )
                    } else {
                        (
                            ToastKind::Success,
                            if self.settings.language == Language::Vi {
                                format!("Đã cập nhật thành công {ok_count} gói nội dung!")
                            } else {
                                format!("Successfully updated {ok_count} package(s)!")
                            },
                        )
                    };
                    self.push_toast(t_kind, msg);
                }
                AppAsyncEvent::ProfileExternalFileImported { profile_id } => {
                    if self.home.selected_profile_id.as_deref() == Some(&profile_id) {
                        self.refresh_selected_profile_tabs();
                    }
                    let msg = self
                        .i18n
                        .tr(
                            "Đã tải tệp lên thư mục Profile!",
                            "Uploaded file into profile directory!",
                        )
                        .to_string();
                    self.push_toast(ToastKind::Success, msg);
                }
                AppAsyncEvent::ProfileDuplicated { profile } => {
                    let pid = profile.id.clone();
                    let pname = profile.name.clone();
                    self.home.profiles = self.db.list_profiles().unwrap_or_default();
                    self.select_profile(pid);
                    let msg = if self.settings.language == Language::Vi {
                        format!("Đã nhân bản Profile '{pname}'!")
                    } else {
                        format!("Duplicated profile '{pname}'!")
                    };
                    self.push_toast(ToastKind::Success, msg);
                }
                AppAsyncEvent::ProfileLoaderUpgraded {
                    profile_id,
                    loader,
                    new_version,
                } => {
                    self.home.profiles = self.db.list_profiles().unwrap_or_default();
                    if self.home.selected_profile_id.as_deref() == Some(&profile_id) {
                        self.sync_profile_edit_draft();
                    }
                    self.crash_modal = None;
                    let msg = if self.settings.language == Language::Vi {
                        format!(
                            "Đã nâng cấp {} lên phiên bản {}!",
                            loader.display_name(),
                            new_version
                        )
                    } else {
                        format!(
                            "Upgraded {} to version {}!",
                            loader.display_name(),
                            new_version
                        )
                    };
                    self.push_toast(ToastKind::Success, msg);
                }
                AppAsyncEvent::ProfileFsCompleted {
                    profile_id,
                    clear_selection,
                    refresh_worlds,
                    toast,
                } => {
                    if self.home.selected_profile_id.as_deref() == Some(&profile_id) {
                        if clear_selection {
                            self.home.selected_entries.clear();
                        }
                        if refresh_worlds {
                            if let Some(p) = self.selected_profile().cloned() {
                                self.home.worlds =
                                    list_local_worlds(&p.instance_path).unwrap_or_default();
                            }
                        }
                        self.refresh_selected_profile_tabs();
                    }
                    self.push_toast(toast.0, toast.1);
                }
                AppAsyncEvent::BookmarkPingResult {
                    bookmark_id,
                    result,
                } => {
                    self.home.bookmark_pings.insert(bookmark_id, result);
                }
                AppAsyncEvent::DiskPackArchiveInspected(modal_state) => {
                    self.disk_pack_modal = Some(modal_state);
                }
                AppAsyncEvent::DiskPackCustomDirPicked(dir) => {
                    if let Some(m) = self.disk_pack_modal.as_mut() {
                        m.custom_dir = Some(dir);
                    }
                }
                AppAsyncEvent::DiskPackImportFinished { result, cancelled } => {
                    self.disk_pack_cancel = None;
                    if cancelled {
                        let cancel_msg = self
                            .i18n
                            .tr("Đã hủy nhập Modpack.", "Cancelled Modpack import.")
                            .to_string();
                        if let Some(m) = self.disk_pack_modal.as_mut() {
                            m.installing = false;
                            m.minimized = false;
                            m.error_msg = Some(cancel_msg.clone());
                        }
                        self.push_toast(ToastKind::Info, cancel_msg);
                    } else {
                        match result {
                            Ok(profile) => {
                                let pname = profile.name.clone();
                                let pid = profile.id.clone();
                                self.disk_pack_modal = None;
                                self.home.profiles = self.db.list_profiles().unwrap_or_default();
                                self.modpacks.selected = None;
                                self.section = ActiveSection::Home;
                                self.select_profile(pid);
                                let msg = if self.settings.language == Language::Vi {
                                    format!("Đã nhập & cài đặt Modpack '{pname}' sẵn sàng chơi!")
                                } else {
                                    format!("Imported & installed Modpack '{pname}'!")
                                };
                                self.push_toast(ToastKind::Success, msg);
                            }
                            Err(err) => {
                                if let Some(m) = self.disk_pack_modal.as_mut() {
                                    m.installing = false;
                                    m.minimized = false;
                                    m.error_msg = Some(err.clone());
                                }
                                let msg = format!(
                                    "{}: {err}",
                                    self.i18n
                                        .tr("Nhập Modpack thất bại", "Failed to import Modpack")
                                );
                                self.push_toast(ToastKind::Error, msg);
                            }
                        }
                    }
                }
                AppAsyncEvent::ModpackSearchCompleted {
                    seq,
                    append,
                    result,
                } => {
                    if seq == self.modpacks.search_seq {
                        self.modpacks.searching = false;
                        match result {
                            Ok((new_hits, total)) => {
                                self.modpacks.total_hits = total;
                                self.modpacks.last_error = None;
                                if append {
                                    for hit in new_hits {
                                        if !self
                                            .modpacks
                                            .hits
                                            .iter()
                                            .any(|existing| existing.project_id == hit.project_id)
                                        {
                                            self.modpacks.hits.push(hit);
                                        }
                                    }
                                } else {
                                    self.modpacks.hits = new_hits;
                                }
                            }
                            Err(err) => {
                                let prov_name = self.modpacks.provider.display_name();
                                let toast_msg = format!(
                                    "{} {prov_name}: {err}",
                                    self.i18n.tr("Không thể tải từ", "Failed to load from")
                                );
                                self.modpacks.last_error = Some(err);
                                self.push_toast(ToastKind::Error, toast_msg);
                            }
                        }
                    }
                }
                AppAsyncEvent::ModpackProjectLoaded {
                    seq,
                    open_install_after,
                    result,
                } => {
                    if seq == self.modpacks.detail_seq {
                        self.modpacks.detail_loading = false;
                        match result {
                            Ok((mut detail, versions)) => {
                                if let Some(ref preview) = self.modpacks.selected_preview {
                                    detail.summary.content_kind = preview.content_kind;
                                    if detail.summary.author.trim().is_empty() {
                                        detail.summary.author = preview.author.clone();
                                    }
                                    if detail.summary.icon_url.is_none() {
                                        detail.summary.icon_url = preview.icon_url.clone();
                                    }
                                    if detail.summary.downloads == 0 && preview.downloads > 0 {
                                        detail.summary.downloads = preview.downloads;
                                    }
                                    if detail.summary.categories.is_empty() {
                                        detail.summary.categories = preview.categories.clone();
                                    }
                                    if detail.summary.updated_at.is_none() {
                                        detail.summary.updated_at = preview.updated_at.clone();
                                    }
                                }
                                self.modpacks.detail_channel_filter = "all".into();
                                self.modpacks.detail_mc_filter =
                                    if self.modpacks.mc_version_filter.trim().is_empty() {
                                        "all".into()
                                    } else {
                                        self.modpacks.mc_version_filter.trim().to_string()
                                    };
                                self.modpacks.detail_loader_filter = self.modpacks.loader_filter;
                                self.modpacks.selected = Some((detail, versions));
                                if open_install_after {
                                    self.open_modpack_install_modal(None);
                                }
                            }
                            Err(err) => {
                                self.modpacks.selected_preview = None;
                                let msg = format!(
                                    "{}: {err}",
                                    self.i18n.tr(
                                        "Không thể tải chi tiết dự án",
                                        "Failed to load project details"
                                    )
                                );
                                self.push_toast(ToastKind::Error, msg);
                            }
                        }
                    }
                }
                AppAsyncEvent::ModpackRemoteImageLoaded { cache_key, image } => {
                    self.modpacks.pending_textures.remove(&cache_key);
                    if let Some(color_img) = image {
                        let max_tex = if self.settings.low_ram_mode {
                            32
                        } else {
                            MAX_REMOTE_TEXTURES
                        };
                        while self.modpacks.textures.len() >= max_tex {
                            if let Some(oldest) = self.modpacks.texture_order.pop_front() {
                                self.modpacks.textures.remove(&oldest);
                            } else {
                                break;
                            }
                        }
                        let tex = self.egui_ctx.load_texture(
                            format!("thk_mp_img_{cache_key}"),
                            color_img,
                            egui::TextureOptions::LINEAR,
                        );
                        self.modpacks.textures.insert(cache_key.clone(), tex);
                        self.modpacks.texture_order.push_back(cache_key);
                    } else {
                        self.modpacks.failed_textures.insert(cache_key);
                    }
                }
                AppAsyncEvent::ModpackCustomDirPicked(dir) => {
                    if let Some(modal) = self.modpacks.install_modal.as_mut() {
                        modal.custom_dir = Some(dir);
                    }
                }
                AppAsyncEvent::ModpackInstallFinished { result, cancelled } => {
                    self.modpacks.install_cancel = None;
                    if cancelled {
                        let cancel_msg = self
                            .i18n
                            .tr("Đã hủy cài đặt theo yêu cầu.", "Installation cancelled.")
                            .to_string();
                        if let Some(modal) = self.modpacks.install_modal.as_mut() {
                            modal.installing = false;
                            modal.minimized = false;
                            modal.error_msg = Some(cancel_msg.clone());
                        }
                        self.push_toast(ToastKind::Info, cancel_msg);
                    } else {
                        match result {
                            Ok(ModpackInstallOutcome::NewProfile(profile)) => {
                                let pname = profile.name.clone();
                                let pid = profile.id.clone();
                                self.modpacks.install_modal = None;
                                self.modpacks.selected = None;
                                self.modpacks.selected_preview = None;
                                self.home.profiles = self.db.list_profiles().unwrap_or_default();
                                self.section = ActiveSection::Home;
                                self.select_profile(pid);
                                let msg = format!(
                                    "{} '{pname}'!",
                                    self.i18n.tr(
                                        "Đã cài đặt thành công Modpack",
                                        "Successfully installed Modpack"
                                    )
                                );
                                self.push_toast(ToastKind::Success, msg);
                            }
                            Ok(ModpackInstallOutcome::NewServer(srv)) => {
                                let sname = srv.name.clone();
                                let sid = srv.id.clone();
                                self.modpacks.install_modal = None;
                                self.modpacks.selected = None;
                                self.modpacks.selected_preview = None;
                                self.servers.servers = self.db.list_servers().unwrap_or_default();
                                self.section = ActiveSection::Servers;
                                self.select_server(sid);
                                let msg = if self.settings.language == Language::Vi {
                                    format!("Đã tạo máy chủ Modpack '{sname}' sẵn sàng khởi động!")
                                } else {
                                    format!("Created Modpack server '{sname}' ready to start!")
                                };
                                self.push_toast(ToastKind::Success, msg);
                            }
                            Ok(ModpackInstallOutcome::AddonInstalled {
                                profile_id,
                                profile_name,
                                kind,
                                title,
                                file_count,
                            }) => {
                                self.modpacks.install_modal = None;
                                if self.home.selected_profile_id.as_deref() == Some(&profile_id) {
                                    self.refresh_selected_profile_tabs();
                                }
                                let kind_lbl = match kind {
                                    ContentKind::Mod => "Mod",
                                    ContentKind::Shader => "Shader",
                                    ContentKind::ResourcePack => "Resource Pack",
                                    ContentKind::Modpack => "Modpack",
                                    ContentKind::Plugin => "Plugin",
                                };
                                let msg = if self.settings.language == Language::Vi {
                                    format!(
                                        "Đã cài {kind_lbl} '{title}' ({file_count} tệp) vào '{profile_name}'!"
                                    )
                                } else {
                                    format!(
                                        "Installed {kind_lbl} '{title}' ({file_count} files) into '{profile_name}'!"
                                    )
                                };
                                self.push_toast(ToastKind::Success, msg);
                            }
                            Err(err) => {
                                if let Some(modal) = self.modpacks.install_modal.as_mut() {
                                    modal.installing = false;
                                    modal.minimized = false;
                                    modal.error_msg = Some(err.clone());
                                }
                                let msg = format!(
                                    "{}: {err}",
                                    self.i18n.tr("Cài đặt thất bại", "Installation failed")
                                );
                                self.push_toast(ToastKind::Error, msg);
                            }
                        }
                    }
                }
                AppAsyncEvent::EngineVersionsLoaded { engine, versions } => {
                    if self.servers.new_draft.engine == engine
                        && !versions.is_empty()
                        && !versions.contains(&self.servers.new_draft.game_version)
                    {
                        self.servers.new_draft.game_version = versions[0].clone();
                    }
                    self.servers.engine_versions.insert(engine, versions);
                }
                AppAsyncEvent::ServerJarInstalled {
                    server_id,
                    jar_file,
                } => {
                    self.servers.servers = self.db.list_servers().unwrap_or_default();
                    if self.servers.selected_server_id.as_deref() == Some(&server_id) {
                        self.sync_server_edit_draft();
                        self.refresh_selected_server_tabs();
                    }
                    let msg = if self.settings.language == Language::Vi {
                        format!("Đã cài đặt lõi máy chủ ({jar_file}) sẵn sàng khởi động!")
                    } else {
                        format!("Installed server core ({jar_file}) and ready to start!")
                    };
                    self.push_toast(ToastKind::Success, msg);
                }
                AppAsyncEvent::ServerExternalFileImported { server_id } => {
                    if self.servers.selected_server_id.as_deref() == Some(&server_id) {
                        self.refresh_selected_server_tabs();
                    }
                }
                AppAsyncEvent::ServerFsCompleted {
                    server_id,
                    clear_selection,
                    toast,
                } => {
                    if self.servers.selected_server_id.as_deref() == Some(&server_id) {
                        if clear_selection {
                            self.servers.selected_entries.clear();
                        }
                        self.refresh_selected_server_tabs();
                    }
                    self.push_toast(toast.0, toast.1);
                }
                AppAsyncEvent::ServerAccessListUpdated {
                    server_id,
                    whitelist,
                    banlist,
                    ops_list,
                    banned_ips,
                    known_players,
                    live_cmd,
                } => {
                    if self.servers.selected_server_id.as_deref() == Some(&server_id) {
                        if let Some(wl) = whitelist {
                            self.servers.whitelist = wl;
                        }
                        if let Some(bl) = banlist {
                            self.servers.banlist = bl;
                        }
                        if let Some(ops) = ops_list {
                            self.servers.ops_list = ops;
                        }
                        if let Some(ips) = banned_ips {
                            self.servers.banned_ips = ips;
                        }
                        if let Some(known) = known_players {
                            self.servers.known_players = known;
                        }
                    }
                    if let Some(cmd) = live_cmd {
                        if self.server_supervisor.runtime_state(&server_id)
                            == ServerRuntimeState::Online
                        {
                            let _ = self.server_supervisor.send_command(&server_id, &cmd);
                        }
                    }
                }
                AppAsyncEvent::ServerAddonSearchCompleted {
                    append,
                    hits,
                    total_hits,
                } => {
                    self.servers.addon_total_hits = total_hits;
                    if append {
                        for hit in hits {
                            if !self
                                .servers
                                .addon_hits
                                .iter()
                                .any(|existing| existing.project_id == hit.project_id)
                            {
                                self.servers.addon_hits.push(hit);
                            }
                        }
                    } else {
                        self.servers.addon_hits = hits;
                    }
                }
                AppAsyncEvent::ServerAddonVersionsLoaded { summary, versions } => {
                    self.servers.addon_chosen_ver_idx = 0;
                    self.servers.addon_selected = Some((summary, versions));
                }
                AppAsyncEvent::NewServerModpackInspected(res) => match res {
                    Ok((path, info)) => {
                        self.servers.new_draft.name = info.name.clone();
                        self.servers.new_draft.engine = info.engine;
                        self.servers.new_draft.game_version = info.game_version.clone();
                        if let Some(rec_ram) = info.recommended_ram_mb {
                            self.servers.new_draft.ram_max_mb = rec_ram.clamp(2048, 16384);
                        }
                        self.servers.new_draft.modpack_archive_path = Some(path);
                        self.servers.new_draft.modpack_inspection = Some(info);
                        self.servers.new_draft.create_error = None;
                    }
                    Err(err) => {
                        self.servers.new_draft.create_error = Some(err.clone());
                        self.push_toast(ToastKind::Error, err);
                    }
                },
                AppAsyncEvent::NewServerCloudSearchCompleted {
                    seq,
                    append,
                    result,
                } => {
                    if seq == self.servers.new_draft.cloud_search_seq {
                        self.servers.new_draft.cloud_searching = false;
                        match result {
                            Ok((new_hits, total)) => {
                                self.servers.new_draft.cloud_total_hits = total;
                                if append {
                                    for hit in new_hits {
                                        if !self
                                            .servers
                                            .new_draft
                                            .cloud_hits
                                            .iter()
                                            .any(|existing| existing.project_id == hit.project_id)
                                        {
                                            self.servers.new_draft.cloud_hits.push(hit);
                                        }
                                    }
                                } else {
                                    self.servers.new_draft.cloud_hits = new_hits;
                                }
                            }
                            Err(err) => {
                                self.servers.new_draft.create_error = Some(err.clone());
                                self.push_toast(ToastKind::Error, err);
                            }
                        }
                    }
                }
                AppAsyncEvent::NewServerCloudVersionsLoaded { project_id, result } => {
                    let matches_selected = self
                        .servers
                        .new_draft
                        .cloud_selected
                        .as_ref()
                        .is_some_and(|(s, _)| s.project_id == project_id);
                    if matches_selected {
                        self.servers.new_draft.cloud_loading_versions = false;
                        match result {
                            Ok(versions) => {
                                self.servers.new_draft.cloud_chosen_ver_idx =
                                    preferred_server_version_index(&versions);
                                if let Some((_, slot)) =
                                    self.servers.new_draft.cloud_selected.as_mut()
                                {
                                    *slot = versions;
                                }
                            }
                            Err(err) => {
                                self.servers.new_draft.create_error = Some(err.clone());
                                self.push_toast(ToastKind::Error, err);
                            }
                        }
                    }
                }
                AppAsyncEvent::ModpackServerCreated { result, cancelled } => {
                    self.servers.new_draft.creating = false;
                    self.servers.new_draft.create_cancel = None;
                    if cancelled {
                        let msg = self
                            .i18n
                            .tr(
                                "Đã hủy tạo máy chủ Modpack.",
                                "Cancelled Modpack server creation.",
                            )
                            .to_string();
                        self.servers.new_draft.create_error = Some(msg.clone());
                        self.push_toast(ToastKind::Info, msg);
                    } else {
                        match result {
                            Ok(srv) => {
                                let id = srv.id.clone();
                                let name = srv.name.clone();
                                self.servers.new_draft.accept_eula = false;
                                self.servers.new_draft.custom_dir = None;
                                self.servers.new_draft.create_error = None;
                                self.servers.servers = self.db.list_servers().unwrap_or_default();
                                self.show_new_server_modal = false;
                                self.section = ActiveSection::Servers;
                                self.select_server(id);
                                let msg = if self.settings.language == Language::Vi {
                                    format!("Đã tạo máy chủ Modpack '{name}' sẵn sàng khởi động!")
                                } else {
                                    format!("Created Modpack server '{name}' ready to start!")
                                };
                                self.push_toast(ToastKind::Success, msg);
                            }
                            Err(err) => {
                                self.servers.new_draft.create_error = Some(err.clone());
                                self.push_toast(ToastKind::Error, err);
                            }
                        }
                    }
                }
                AppAsyncEvent::ServerFtpTested { server_id, result } => {
                    if self.servers.selected_server_id.as_deref() == Some(&server_id) {
                        self.servers.ftp_testing = false;
                        match result {
                            Ok(pwd) => {
                                let msg = if self.settings.language == Language::Vi {
                                    format!("Kết nối thành công! Thư mục đích: {pwd}")
                                } else {
                                    format!("Connection verified! Remote directory: {pwd}")
                                };
                                self.servers.ftp_last_status = Some((true, msg.clone()));
                                self.push_toast(ToastKind::Success, msg);
                            }
                            Err(err) => {
                                self.servers.ftp_last_status = Some((false, err.clone()));
                                self.push_toast(ToastKind::Error, err);
                            }
                        }
                    }
                }
                AppAsyncEvent::ServerFtpDeployed {
                    server_id,
                    result,
                    cancelled,
                } => {
                    if self.servers.selected_server_id.as_deref() == Some(&server_id) {
                        self.servers.ftp_deploying = false;
                        self.servers.ftp_deploy_cancel = None;
                        if cancelled {
                            let msg = self
                                .i18n
                                .tr(
                                    "Đã hủy tải dữ liệu lên Hosting.",
                                    "Cancelled FTP deployment.",
                                )
                                .to_string();
                            self.servers.ftp_last_status = Some((false, msg.clone()));
                            self.push_toast(ToastKind::Info, msg);
                        } else {
                            match result {
                                Ok(sum) => {
                                    let mb = sum.uploaded_bytes as f64 / (1024.0 * 1024.0);
                                    let msg = if self.settings.language == Language::Vi {
                                        format!(
                                            "Đã đẩy {} tệp ({mb:.1} MB) lên {}!",
                                            sum.uploaded_files, sum.remote_dir
                                        )
                                    } else {
                                        format!(
                                            "Deployed {} file(s) ({mb:.1} MB) to {}!",
                                            sum.uploaded_files, sum.remote_dir
                                        )
                                    };
                                    self.servers.ftp_last_status = Some((true, msg.clone()));
                                    self.push_toast(ToastKind::Success, msg);
                                }
                                Err(err) => {
                                    self.servers.ftp_last_status = Some((false, err.clone()));
                                    self.push_toast(ToastKind::Error, err);
                                }
                            }
                        }
                    }
                }
                AppAsyncEvent::DeviceCodeReady(sess) => {
                    self.accounts.device_session = Some(sess);
                }
                AppAsyncEvent::MicrosoftAccountLoggedIn(mut acct) => {
                    if let Some(prev) = self.accounts.accounts.iter().find(|a| a.id == acct.id) {
                        if acct.custom_skin_path.is_none() {
                            acct.custom_skin_path = prev.custom_skin_path.clone();
                        }
                        if acct.custom_cape_path.is_none() {
                            acct.custom_cape_path = prev.custom_cape_path.clone();
                        }
                        if acct.custom_elytra_path.is_none() {
                            acct.custom_elytra_path = prev.custom_elytra_path.clone();
                        }
                    }
                    let _ = self.db.upsert_account(&acct);
                    let id = acct.id.clone();
                    let uname = acct.username.clone();
                    self.accounts.device_session = None;
                    self.accounts.desktop_challenge = None;
                    self.accounts.desktop_redirect_input.clear();
                    self.accounts.loaded_viewport_key.clear();
                    self.accounts.accounts = self.db.list_accounts().unwrap_or_default();
                    self.select_account(id);
                    let msg = if self.settings.language == Language::Vi {
                        format!("Đăng nhập Microsoft thành công: {uname}!")
                    } else {
                        format!("Signed in with Microsoft: {uname}!")
                    };
                    self.push_toast(ToastKind::Success, msg);
                }
                AppAsyncEvent::AuthError(err) => {
                    self.push_toast(ToastKind::Error, err);
                }
                AppAsyncEvent::AccountTextureSaved { account, slot } => {
                    self.accounts.accounts = self.db.list_accounts().unwrap_or_default();
                    self.accounts.loaded_viewport_key.clear();
                    if let Some(vp) = self.accounts.skin_viewport.as_mut() {
                        match slot {
                            TextureSlot::Cape => {
                                vp.back_mode = BackEquipmentMode::Cape;
                                vp.mark_dirty();
                            }
                            TextureSlot::Elytra => {
                                vp.back_mode = BackEquipmentMode::Elytra;
                                vp.mark_dirty();
                            }
                            TextureSlot::Skin => vp.mark_dirty(),
                        }
                    }
                    let msg = if self.settings.language == Language::Vi {
                        format!("Đã cập nhật giao diện 3D cho '{}'!", account.username)
                    } else {
                        format!("Updated 3D Skin/Cape for '{}'!", account.username)
                    };
                    self.push_toast(ToastKind::Success, msg);
                }
                AppAsyncEvent::AccountProfileSynced { account } => {
                    let _ = account;
                    self.accounts.accounts = self.db.list_accounts().unwrap_or_default();
                    self.accounts.loaded_viewport_key.clear();
                }
                AppAsyncEvent::RemoteAccountTextureCached { cache_key } => {
                    self.accounts.remote_tex_fetching.remove(&cache_key);
                    self.accounts.loaded_viewport_key.clear();
                }
                AppAsyncEvent::JavaScanCompleted(list) => {
                    self.java_modal.installed = list;
                }
                AppAsyncEvent::JavaDistroPackagesLoaded { distro, packages } => {
                    if self.java_modal.selected_distro == distro {
                        self.java_modal.packages = packages;
                    }
                }
                AppAsyncEvent::UpdateCheckCompleted(res) => {
                    self.updater.checking = false;
                    match res {
                        Ok(UpdateCheckOutcome::Available(info)) => {
                            self.updater.last_error = false;
                            self.updater.status_line =
                                Some(if self.settings.language == Language::Vi {
                                    format!(
                                        "Có phiên bản mới {} ({})",
                                        info.version, info.asset_name
                                    )
                                } else {
                                    format!(
                                        "New release available: {} ({})",
                                        info.version, info.asset_name
                                    )
                                });
                            self.updater.available = Some(info.clone());
                            let msg = if self.settings.language == Language::Vi {
                                format!(
                                    "Phát hiện bản cập nhật mới TriHienKun Laucher v{}!",
                                    info.version
                                )
                            } else {
                                format!(
                                    "New TriHienKun Laucher update v{} available!",
                                    info.version
                                )
                            };
                            self.push_toast(ToastKind::Info, msg);
                        }
                        Ok(UpdateCheckOutcome::UpToDate) => {
                            self.updater.last_error = false;
                            self.updater.available = None;
                            self.updater.status_line = Some(
                                self.i18n
                                    .tr(
                                        "Bạn đang sử dụng phiên bản mới nhất.",
                                        "You are running the latest version.",
                                    )
                                    .to_string(),
                            );
                        }
                        Ok(UpdateCheckOutcome::NoPublishedReleases) => {
                            self.updater.last_error = false;
                            self.updater.available = None;
                            self.updater.status_line = Some(
                                self.i18n
                                    .tr(
                                        "Kho lưu trữ hiện chưa đăng tải bản phát hành nào.",
                                        "No releases published in this repository yet.",
                                    )
                                    .to_string(),
                            );
                        }
                        Err(e) => {
                            self.updater.last_error = true;
                            self.updater.available = None;
                            self.updater.status_line = Some(format!(
                                "{}: {e}",
                                self.i18n.tr("Lỗi kiểm tra cập nhật", "Update check failed")
                            ));
                        }
                    }
                }
                AppAsyncEvent::UpdateDownloaded(res) => match res {
                    Ok(update) => {
                        self.updater.last_error = false;
                        let path = update.path.clone();
                        self.updater.downloaded_file = Some(update);
                        self.updater.status_line =
                            Some(if self.settings.language == Language::Vi {
                                format!("Đã xác thực SHA-256 sẵn sàng cài đặt: {}", path.display())
                            } else {
                                format!("SHA-256 verified & ready to apply: {}", path.display())
                            });
                        let msg = if self.settings.language == Language::Vi {
                            format!(
                                "Đã tải & xác thực SHA-256 gói cập nhật: {}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            )
                        } else {
                            format!(
                                "Downloaded & SHA-256 verified update: {}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            )
                        };
                        self.push_toast(ToastKind::Success, msg);
                    }
                    Err(e) => self.push_toast(ToastKind::Error, e),
                },
                AppAsyncEvent::CustomBackgroundSaved(dest) => {
                    self.custom_bg_tex = None;
                    self.settings.custom_background_path = Some(dest);
                    self.persist_settings();
                    self.push_toast(
                        ToastKind::Success,
                        self.i18n.tr(
                            "Đã cập nhật ảnh nền giao diện!",
                            "Updated custom background!",
                        ),
                    );
                }
                AppAsyncEvent::CustomMusicSaved => {
                    self.settings.music_enabled = true;
                    self.persist_settings();
                    self.push_toast(
                        ToastKind::Success,
                        self.i18n.tr(
                            "Đã áp dụng nhạc nền tuỳ chỉnh!",
                            "Applied custom background music!",
                        ),
                    );
                }
                AppAsyncEvent::CacheSizeComputed(bytes) => {
                    self.cached_dir_bytes = bytes;
                }
                AppAsyncEvent::CacheCleared(res) => {
                    self.confirm_clear_cache = false;
                    if self
                        .updater
                        .downloaded_file
                        .as_ref()
                        .is_some_and(|u| !u.path.is_file())
                    {
                        self.updater.downloaded_file = None;
                        self.updater.status_line = None;
                    }
                    match res {
                        Ok(freed) => {
                            self.cached_dir_bytes = 0;
                            let mb = freed as f64 / (1024.0 * 1024.0);
                            let msg = if self.settings.language == Language::Vi {
                                format!("Đã dọn sạch bộ nhớ đệm ({mb:.1} MB)!")
                            } else {
                                format!("Cleared cache ({mb:.1} MB freed)!")
                            };
                            self.push_toast(ToastKind::Success, msg);
                        }
                        Err(e) => {
                            self.refresh_cache_size();
                            let msg = format!(
                                "{}: {e}",
                                self.i18n
                                    .tr("Không thể xóa bộ nhớ đệm", "Failed to clear cache")
                            );
                            self.push_toast(ToastKind::Error, msg);
                        }
                    }
                }
                AppAsyncEvent::RecoveryArtifactsScanned {
                    artifacts,
                    notify_if_found,
                } => {
                    let count = artifacts.len();
                    self.recovery_artifacts = artifacts;
                    if notify_if_found && count > 0 {
                        let msg = if self.settings.language == Language::Vi {
                            format!(
                                "Phát hiện {count} tệp/thư mục phục hồi từ phiên cài đặt trước (Cài đặt > Lưu trữ)."
                            )
                        } else {
                            format!(
                                "Detected {count} leftover recovery item(s) from a previous session (Settings > Storage)."
                            )
                        };
                        self.push_toast(ToastKind::Info, msg);
                    }
                }
                AppAsyncEvent::NetworkSnapshotProbed(snap) => {
                    self.servers.probing_network = false;
                    if let Some(ip) = snap.primary_ipv4 {
                        self.lan_modal.primary_lan_ip = Some(ip.to_string());
                    }
                    if let Some(sel) = self.servers.direct_bind_ip {
                        if !snap.interfaces.iter().any(|i| i.ipv4 == sel) {
                            self.servers.direct_bind_ip = None;
                        }
                    }
                    if let Some(sel) = self.lan_modal.direct_bind_ip {
                        if !snap.interfaces.iter().any(|i| i.ipv4 == sel) {
                            self.lan_modal.direct_bind_ip = None;
                        }
                    }
                    self.servers.network_snapshot = snap.clone();
                    self.lan_modal.network_snapshot = snap;
                }
            }
        }
    }

    fn handle_launcher_event(&mut self, ev: LauncherEvent) {
        match ev {
            LauncherEvent::Progress { key, progress } => {
                let task_id = format!("launch:{}:{}", key.profile_id, key.account_id);
                self.update_keyed_task(task_id, Some(progress), None, Some(key));
            }
            LauncherEvent::LogLine { key, line } => {
                let cap = if self.settings.low_ram_mode {
                    800
                } else {
                    4_000
                };
                let buf = self.home.instance_logs.entry(key).or_default();
                while buf.len() >= cap {
                    buf.remove(0);
                }
                buf.push(line);
            }
            LauncherEvent::Started { key, pid } => {
                let task_id = format!("launch:{}:{}", key.profile_id, key.account_id);
                self.update_keyed_task(task_id, None, None, None);
                self.sync_bgm_playback();
                let prof_opt = self
                    .home
                    .profiles
                    .iter()
                    .find(|p| p.id == key.profile_id)
                    .cloned();
                let acct_opt = self
                    .accounts
                    .accounts
                    .iter()
                    .find(|a| a.id == key.account_id)
                    .cloned();
                if let (Some(p), Some(a)) = (prof_opt, acct_opt) {
                    if self.settings.discord_rpc_enabled {
                        self.discord_rpc.set_presence(
                            self.settings.discord_client_id.clone(),
                            DiscordPresenceState::Playing {
                                version: p.game_version,
                                profile_name: p.name.clone(),
                                username: a.username,
                            },
                        );
                    }
                    let msg = if self.settings.language == Language::Vi {
                        format!("Minecraft '{}' đã khởi chạy (PID {pid})!", p.name)
                    } else {
                        format!("Minecraft '{}' launched (PID {pid})!", p.name)
                    };
                    self.push_toast(ToastKind::Success, msg);
                }
                if cfg!(target_os = "android") {
                    self.mobile_game.screen_open = true;
                    self.mobile_game.hub_visible_over_game = false;
                    self.sync_android_keep_alive_service();
                } else if self.settings.hide_launcher_on_launch {
                    self.egui_ctx
                        .send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    self.pending_memory_trim = true;
                }
            }
            LauncherEvent::Exited {
                key,
                exit_code,
                crash_report,
                ..
            } => {
                let task_id = format!("launch:{}:{}", key.profile_id, key.account_id);
                self.update_keyed_task(task_id, None, None, None);
                self.home.profiles = self.db.list_profiles().unwrap_or_default();
                if self.home.selected_profile_id.as_deref() == Some(&key.profile_id) {
                    self.home.daily_stats = self
                        .db
                        .get_daily_play_stats(&key.profile_id, 14)
                        .unwrap_or_default();
                }
                self.sync_idle_discord_presence();
                self.sync_bgm_playback();
                if cfg!(target_os = "android") {
                    self.mobile_game.reset_transient_inputs(None);
                    if self.instance_supervisor.running_keys().is_empty() {
                        self.mobile_game.hub_visible_over_game = false;
                        self.mobile_game.screen_open = false;
                    }
                    self.sync_android_keep_alive_service();
                } else if self.instance_supervisor.running_keys().is_empty()
                    && self.settings.hide_launcher_on_launch
                {
                    crate::tray::restore_and_focus_viewport(&self.egui_ctx);
                }
                if let Some(report) = crash_report {
                    crate::tray::restore_and_focus_viewport(&self.egui_ctx);
                    self.crash_modal = Some((key.profile_id, report));
                    self.audio
                        .play_sfx(self.settings.sfx_enabled, SoundEffect::Error);
                } else {
                    let msg = if self.settings.language == Language::Vi {
                        format!("Phiên chơi đã kết thúc (mã thoát {exit_code}).")
                    } else {
                        format!("Game session ended (exit code {exit_code}).")
                    };
                    self.push_toast(ToastKind::Info, msg);
                }
            }
            LauncherEvent::LaunchFailed { key, error } => {
                let task_id = format!("launch:{}:{}", key.profile_id, key.account_id);
                self.update_keyed_task(task_id, None, None, None);
                self.sync_idle_discord_presence();
                if error.contains("Operation cancelled") {
                    let msg = self
                        .i18n
                        .tr("Đã hủy khởi chạy game.", "Game launch cancelled.")
                        .to_string();
                    self.push_toast(ToastKind::Info, msg);
                } else {
                    let msg = format!(
                        "{}: {error}",
                        self.i18n.tr("Khởi chạy thất bại", "Launch failed")
                    );
                    self.push_toast(ToastKind::Error, msg);
                }
            }
        }
    }

    fn handle_server_event(&mut self, ev: ServerSupervisorEvent) {
        match ev {
            ServerSupervisorEvent::StatusChanged { server_id, state } => {
                self.update_keyed_task(format!("server-jvm-prep:{server_id}"), None, None, None);
                if state == ServerRuntimeState::Offline {
                    self.servers.server_stats.remove(&server_id);
                }
                self.servers.server_states.insert(server_id, state);
                self.sync_idle_discord_presence();
                self.sync_android_keep_alive_service();
            }
            ServerSupervisorEvent::LogLine { server_id, line } => {
                let refresh_known = self.servers.selected_server_id.as_deref() == Some(&server_id)
                    && (line.contains("joined the game") || line.contains("left the game"));
                let cap = if self.settings.low_ram_mode {
                    800
                } else {
                    5_000
                };
                let buf = self.servers.server_logs.entry(server_id).or_default();
                while buf.len() >= cap {
                    buf.remove(0);
                }
                buf.push(line);
                if refresh_known {
                    if let Some(srv) = self.selected_server().cloned() {
                        self.servers.known_players =
                            read_known_players(&srv.server_dir).unwrap_or_default();
                    }
                }
            }
            ServerSupervisorEvent::StatsUpdated { server_id, stats } => {
                self.servers.server_stats.insert(server_id, stats);
            }
            ServerSupervisorEvent::TunnelChanged { server_id, status } => {
                self.servers.server_tunnels.insert(server_id, status);
                self.sync_android_keep_alive_service();
            }
            ServerSupervisorEvent::DirectHostChanged { server_id, status } => {
                let was_running = matches!(
                    self.servers.server_direct_hosts.get(&server_id),
                    Some(DirectHostStatus::Running(_))
                );
                if !was_running {
                    if let DirectHostStatus::Running(ref info) = status {
                        let summary_ep = info
                            .preferred_online_endpoint()
                            .or_else(|| info.primary_lan_endpoint().map(str::to_string))
                            .unwrap_or_else(|| format!("0.0.0.0:{}", info.host_port));
                        let msg = if self.settings.language == Language::Vi {
                            format!("Đã mở Port mạng hiện tại: {summary_ep}")
                        } else {
                            format!("Network port active: {summary_ep}")
                        };
                        self.push_toast(ToastKind::Success, msg);
                    }
                }
                self.servers.server_direct_hosts.insert(server_id, status);
                self.sync_android_keep_alive_service();
            }
            ServerSupervisorEvent::InstallProgress {
                server_id,
                progress,
            } => {
                self.update_keyed_task(
                    format!("server-jvm-prep:{server_id}"),
                    Some(progress),
                    None,
                    None,
                );
            }
        }
    }

    fn handle_lan_event(&mut self, ev: LanTunnelEvent) {
        match ev {
            LanTunnelEvent::ScannerStateChanged { active } => {
                self.lan_modal.scanner_active = active;
            }
            LanTunnelEvent::LocalWorldDetected(ann) => {
                self.lan_modal.manual_port_input = ann.port.to_string();
                let msg = if self.settings.language == Language::Vi {
                    format!(
                        "Phát hiện thế giới LAN '{}' trên cổng {}!",
                        ann.motd, ann.port
                    )
                } else {
                    format!(
                        "Detected local LAN world '{}' on port {}!",
                        ann.motd, ann.port
                    )
                };
                self.push_toast(ToastKind::Success, msg);
                self.lan_modal.detected_world = Some(ann);
            }
            LanTunnelEvent::IgnoredForeignAnnouncement { .. } => {
                self.lan_modal.ignored_foreign_packets += 1;
            }
            LanTunnelEvent::TunnelStatusChanged(st) => {
                if let Some(endpoint) = st.public_endpoint() {
                    let msg = if self.settings.language == Language::Vi {
                        format!("Đã mở cổng công khai: {endpoint}")
                    } else {
                        format!("Public tunnel active: {endpoint}")
                    };
                    self.push_toast(ToastKind::Success, msg);
                }
                self.lan_modal.tunnel_status = st;
                self.sync_android_keep_alive_service();
            }
            LanTunnelEvent::DirectHostStatusChanged(st) => {
                let was_running = matches!(
                    self.lan_modal.direct_host_status,
                    DirectHostStatus::Running(_)
                );
                if !was_running {
                    if let DirectHostStatus::Running(ref info) = st {
                        let summary_ep = info
                            .preferred_online_endpoint()
                            .or_else(|| info.primary_lan_endpoint().map(str::to_string))
                            .unwrap_or_else(|| format!("0.0.0.0:{}", info.host_port));
                        let msg = if self.settings.language == Language::Vi {
                            format!("Đã mở Port mạng hiện tại: {summary_ep}")
                        } else {
                            format!("Network port active: {summary_ep}")
                        };
                        self.push_toast(ToastKind::Success, msg);
                    }
                }
                self.lan_modal.direct_host_status = st;
                self.sync_android_keep_alive_service();
            }
        }
    }

    pub fn shutdown_all_background_services(&self) {
        self.lan_coordinator.shutdown_all();
        self.discord_rpc.shutdown();
        self.audio.shutdown();
        for snap in self.instance_supervisor.running_snapshots() {
            self.instance_supervisor.stop_instance(&snap.key);
        }
        let sup = self.server_supervisor.clone();
        self.rt.block_on(async move {
            sup.shutdown_all().await;
        });
    }
}

struct TextureUploadEnv {
    paths: AppPaths,
    cid: Option<String>,
    lang: Language,
    db: Arc<Database>,
    tx: mpsc::UnboundedSender<AppAsyncEvent>,
    ctx: egui::Context,
}

async fn process_and_save_account_texture(
    mut acct: Account,
    slot: TextureSlot,
    src_path: PathBuf,
    env: TextureUploadEnv,
) {
    let target_dir = match slot {
        TextureSlot::Skin => env.paths.skins_dir.clone(),
        TextureSlot::Cape => env.paths.capes_dir.clone(),
        TextureSlot::Elytra => env.paths.elytras_dir.clone(),
    };
    let src_for_decode = src_path.clone();
    let normalized_res = tokio::task::spawn_blocking(move || match slot {
        TextureSlot::Skin => load_and_normalize_skin_file(&src_for_decode),
        TextureSlot::Cape | TextureSlot::Elytra => load_and_normalize_cape_file(&src_for_decode),
    })
    .await;

    let rgba = match normalized_res {
        Ok(Ok(img)) => img,
        Ok(Err(e)) => {
            let _ = env.tx.send(AppAsyncEvent::Toast {
                kind: ToastKind::Error,
                text: format!(
                    "{}: {e}",
                    tr_lang(env.lang, "Ảnh PNG không hợp lệ", "Invalid PNG texture")
                ),
            });
            env.ctx.request_repaint();
            return;
        }
        Err(e) => {
            let _ = env.tx.send(AppAsyncEvent::Toast {
                kind: ToastKind::Error,
                text: e.to_string(),
            });
            env.ctx.request_repaint();
            return;
        }
    };

    let detected_variant = (slot == TextureSlot::Skin)
        .then(|| detect_skin_variant(&rgba))
        .flatten();
    let fname = format!("{}.png", SafePath::slugify_dir_name(&acct.id));
    let Ok(dest) = SafePath::resolve_child(&target_dir, &fname) else {
        return;
    };
    let dest_save = dest.clone();
    if let Ok(Err(e)) = tokio::task::spawn_blocking(move || rgba.save(&dest_save)).await {
        let _ = env.tx.send(AppAsyncEvent::Toast {
            kind: ToastKind::Error,
            text: format!(
                "{}: {e}",
                tr_lang(env.lang, "Không thể lưu ảnh PNG", "Failed to save PNG")
            ),
        });
        env.ctx.request_repaint();
        return;
    }

    match slot {
        TextureSlot::Skin => {
            acct.custom_skin_path = Some(dest.clone());
            if let Some(v) = detected_variant {
                acct.skin_variant = v;
            }
        }
        TextureSlot::Cape => acct.custom_cape_path = Some(dest.clone()),
        TextureSlot::Elytra => acct.custom_elytra_path = Some(dest.clone()),
    }

    if acct.account_type == AccountType::Microsoft && slot == TextureSlot::Skin {
        if let Ok(png_bytes) = tokio::fs::read(&dest).await {
            let variant = acct.skin_variant;
            if let Err(e) =
                upload_mojang_skin(&mut acct, env.cid.as_deref(), variant, png_bytes).await
            {
                let _ = env.tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Info,
                    text: format!(
                        "{}: {e}",
                        tr_lang(
                            env.lang,
                            "Đã lưu Skin cục bộ nhưng chưa tải lên Mojang Cloud",
                            "Saved local skin (Mojang Cloud upload skipped)"
                        )
                    ),
                });
            }
        }
    }

    if env.db.upsert_account(&acct).is_ok() {
        let _ = env.tx.send(AppAsyncEvent::AccountTextureSaved {
            account: acct,
            slot,
        });
    }
    env.ctx.request_repaint();
}

pub(crate) fn build_profile_from_imported_pack(
    id: String,
    instance_path: PathBuf,
    meta: ImportedPackMetadata,
    default_ram: u32,
) -> GameProfile {
    GameProfile {
        id,
        name: meta.name,
        game_version: meta.game_version,
        loader: meta.loader,
        loader_version: meta.loader_version,
        instance_path,
        is_custom_path: false,
        ram_mb: meta.recommended_ram_mb.unwrap_or(default_ram).max(2048),
        java_path: None,
        jvm_args: String::new(),
        auto_perf_mods: false,
        icon_url: meta.icon_url,
        window_width: None,
        window_height: None,
        fullscreen: false,
        total_play_seconds: 0,
        session_count: 0,
        crash_count: 0,
        last_played_at: None,
        created_at: Utc::now().timestamp(),
    }
}

fn resolve_server_mod_loader(srv: &ManagedServer, kind: ContentKind) -> Option<LoaderType> {
    match (kind, srv.engine) {
        (ContentKind::Mod, ServerEngine::Fabric) => Some(LoaderType::Fabric),
        (ContentKind::Mod, ServerEngine::Forge | ServerEngine::Mohist) => Some(LoaderType::Forge),
        (ContentKind::Mod, ServerEngine::NeoForge) => Some(LoaderType::NeoForge),
        (ContentKind::Mod, ServerEngine::Arclight) => {
            Some(detect_arclight_loader(&srv.server_dir, &srv.game_version))
        }
        _ => None,
    }
}

fn is_semver_at_least(candidate: &str, required: &str) -> bool {
    let parse_nums = |raw: &str| -> Vec<u32> {
        raw.trim()
            .trim_start_matches('v')
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };
    let cand = parse_nums(candidate);
    let req = parse_nums(required);
    if req.is_empty() {
        return true;
    }
    let max_len = cand.len().max(req.len());
    for idx in 0..max_len {
        let a = cand.get(idx).copied().unwrap_or(0);
        let b = req.get(idx).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    true
}

pub(crate) fn inspect_disk_pack_archive(
    archive_path: &Path,
    default_ram_mb: u32,
) -> thk_core::ThkResult<DiskPackImportModalState> {
    if !archive_path.is_file() {
        return Err(thk_core::ThkError::InvalidInput(
            "Archive file does not exist.".into(),
        ));
    }

    if let Some(mr_bytes) = read_zip_entry_bytes(archive_path, "modrinth.index.json")? {
        let v: serde_json::Value = serde_json::from_slice(&mr_bytes)?;
        let fallback_name = archive_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Modrinth Pack");
        let name = v
            .get("name")
            .and_then(|x| x.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(fallback_name)
            .to_string();
        let deps = v.get("dependencies").and_then(|x| x.as_object());
        let mc_version = deps
            .and_then(|d| d.get("minecraft"))
            .and_then(|x| x.as_str())
            .unwrap_or("1.21.4")
            .to_string();
        let detected_loader = if deps.is_some_and(|d| d.contains_key("fabric-loader")) {
            LoaderType::Fabric
        } else if deps.is_some_and(|d| d.contains_key("quilt-loader")) {
            LoaderType::Quilt
        } else if deps.is_some_and(|d| d.contains_key("neoforge")) {
            LoaderType::NeoForge
        } else if deps.is_some_and(|d| d.contains_key("forge")) {
            LoaderType::Forge
        } else {
            LoaderType::Vanilla
        };
        let file_count = v
            .get("files")
            .and_then(|x| x.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        return Ok(DiskPackImportModalState {
            archive_path: archive_path.to_path_buf(),
            is_mrpack: true,
            format_label: "Modrinth (.mrpack)".into(),
            detected_mc_version: mc_version,
            detected_loader,
            detected_file_count: file_count,
            profile_name: name,
            ram_mb: default_ram_mb.max(2048),
            custom_dir: None,
            installing: false,
            minimized: false,
            error_msg: None,
        });
    }

    if let Some(cf_bytes) = read_zip_entry_bytes(archive_path, "manifest.json")? {
        let v: serde_json::Value = serde_json::from_slice(&cf_bytes)?;
        let fallback_name = archive_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("CurseForge Pack");
        let name = v
            .get("name")
            .and_then(|x| x.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(fallback_name)
            .to_string();
        let mc_obj = v.get("minecraft");
        let mc_version = mc_obj
            .and_then(|m| m.get("version"))
            .and_then(|x| x.as_str())
            .unwrap_or("1.20.1")
            .to_string();
        let rec_ram = mc_obj
            .and_then(|m| m.get("recommendedRam"))
            .and_then(|x| x.as_u64())
            .map(|r| (r as u32).clamp(2048, 32768))
            .unwrap_or_else(|| default_ram_mb.max(4096));

        let mut detected_loader = LoaderType::Vanilla;
        if let Some(loaders) = mc_obj
            .and_then(|m| m.get("modLoaders"))
            .and_then(|x| x.as_array())
        {
            let chosen_id = loaders
                .iter()
                .find(|l| l.get("primary").and_then(|p| p.as_bool()).unwrap_or(false))
                .or_else(|| loaders.first())
                .and_then(|l| l.get("id"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let lower = chosen_id.to_ascii_lowercase();
            if lower.starts_with("fabric") {
                detected_loader = LoaderType::Fabric;
            } else if lower.starts_with("quilt") {
                detected_loader = LoaderType::Quilt;
            } else if lower.starts_with("neoforge") {
                detected_loader = LoaderType::NeoForge;
            } else if lower.starts_with("forge") {
                detected_loader = LoaderType::Forge;
            }
        }

        let file_count = v
            .get("files")
            .and_then(|x| x.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        return Ok(DiskPackImportModalState {
            archive_path: archive_path.to_path_buf(),
            is_mrpack: false,
            format_label: "CurseForge (.zip)".into(),
            detected_mc_version: mc_version,
            detected_loader,
            detected_file_count: file_count,
            profile_name: name,
            ram_mb: rec_ram,
            custom_dir: None,
            installing: false,
            minimized: false,
            error_msg: None,
        });
    }

    Err(thk_core::ThkError::InvalidInput(
        "Archive contains neither modrinth.index.json nor CurseForge manifest.json.".into(),
    ))
}

pub(crate) fn is_safe_managed_dir_to_delete(paths: &AppPaths, target: &Path) -> bool {
    let Ok(target_canon) = std::fs::canonicalize(target) else {
        return false;
    };
    if !target_canon.is_dir() || target_canon.components().count() < 4 {
        return false;
    }
    let Ok(root_canon) = std::fs::canonicalize(&paths.root) else {
        return false;
    };
    if root_canon.starts_with(&target_canon) {
        return false;
    }
    for protected in [
        &paths.instances_dir,
        &paths.servers_dir,
        &paths.runtimes_dir,
        &paths.assets_dir,
        &paths.libraries_dir,
        &paths.versions_dir,
        &paths.cache_dir,
        &paths.skins_dir,
        &paths.capes_dir,
        &paths.elytras_dir,
        &paths.backgrounds_dir,
    ] {
        if let Ok(prot_canon) = std::fs::canonicalize(protected) {
            if prot_canon.starts_with(&target_canon) {
                return false;
            }
        }
    }
    true
}

fn resolve_custom_bgm_file(paths: &AppPaths) -> Option<PathBuf> {
    for ext in ["mp3", "ogg", "wav"] {
        let candidate = paths.backgrounds_dir.join(format!("custom_bgm.{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn tr_lang<'a>(lang: Language, vi: &'a str, en: &'a str) -> &'a str {
    match lang {
        Language::Vi => vi,
        Language::En => en,
    }
}

pub(crate) fn preferred_server_version_index(versions: &[ProjectVersion]) -> usize {
    let is_server_loader = |v: &ProjectVersion| {
        v.loaders.is_empty()
            || !v
                .loaders
                .iter()
                .all(|l| LoaderType::from_str_lossy(l) == LoaderType::Quilt)
    };
    versions
        .iter()
        .position(|v| is_server_loader(v) && v.release_channel.eq_ignore_ascii_case("release"))
        .or_else(|| versions.iter().position(is_server_loader))
        .unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn provision_cloud_modpack_server<F>(
    dl: &DownloadManager,
    sup: &LocalServerSupervisor,
    db: &Database,
    paths: &AppPaths,
    id: String,
    name: String,
    srv_dir: PathBuf,
    is_custom_dir: bool,
    port: u16,
    ram_min_mb: u32,
    ram_max_mb: u32,
    accept_eula: bool,
    summary: &ProjectSummary,
    version: &ProjectVersion,
    cf_key: Option<&str>,
    max_conc: usize,
    cancel: CancellationToken,
    on_prog: F,
) -> thk_core::ThkResult<ManagedServer>
where
    F: Fn(DownloadProgress) + Send + Sync + Clone + 'static,
{
    if let Ok(meta) = std::fs::symlink_metadata(&srv_dir) {
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(thk_core::ThkError::InvalidInput(
                "Server destination must be a regular directory".into(),
            ));
        }
        if std::fs::read_dir(&srv_dir)?.next().is_some() {
            return Err(thk_core::ThkError::InvalidInput(
                "Target server directory must be new or empty".into(),
            ));
        }
    }

    let canon_target = std::fs::canonicalize(&srv_dir).ok();
    let matches_target = |existing: &Path| {
        existing == srv_dir
            || canon_target
                .as_ref()
                .is_some_and(|t| std::fs::canonicalize(existing).as_ref().ok() == Some(t))
    };
    if db
        .list_profiles()?
        .iter()
        .any(|p| matches_target(&p.instance_path))
        || db
            .list_servers()?
            .iter()
            .any(|s| matches_target(&s.server_dir))
    {
        return Err(thk_core::ThkError::InvalidInput(
            "Target directory is already used by an existing Profile or Server".into(),
        ));
    }

    let created_fresh_dir = !srv_dir.exists();
    let res: thk_core::ThkResult<ManagedServer> = async {
        let outcome = populate_server_from_cloud_modpack(
            dl,
            summary,
            version,
            &paths.cache_dir,
            &srv_dir,
            cf_key,
            max_conc,
            Some(cancel.clone()),
            on_prog.clone(),
        )
        .await?;

        if cancel.is_cancelled() {
            return Err(thk_core::ThkError::Cancelled);
        }

        let mut init_props = BTreeMap::new();
        init_props.insert("server-port".into(), port.to_string());
        init_props.insert("motd".into(), format!("{name} - TriHienKun Laucher"));
        write_server_properties_patch(&srv_dir, &init_props)?;
        if accept_eula {
            write_eula_accepted(&srv_dir)?;
        }

        let mut jar_target = list_runnable_server_targets(&srv_dir)
            .into_iter()
            .next()
            .unwrap_or_default();

        let mut srv = ManagedServer {
            id,
            name,
            engine: outcome.engine,
            game_version: outcome.game_version,
            server_dir: srv_dir.clone(),
            is_custom_dir,
            port,
            ram_min_mb,
            ram_max_mb,
            java_path: None,
            jvm_flags: String::new(),
            jar_file: jar_target.clone(),
            created_at: Utc::now().timestamp(),
        };

        if jar_target.is_empty() {
            let java_bin = sup.resolve_java_for_server(&srv).await?;
            if cancel.is_cancelled() {
                return Err(thk_core::ThkError::Cancelled);
            }
            jar_target = install_server_engine(
                dl,
                srv.engine,
                &srv.game_version,
                &srv.server_dir,
                &java_bin,
                on_prog,
            )
            .await?;
            srv.jar_file = jar_target;
        }

        db.upsert_server(&srv)?;
        Ok(srv)
    }
    .await;

    if res.is_err() && is_safe_managed_dir_to_delete(paths, &srv_dir) {
        if created_fresh_dir {
            let _ = tokio::fs::remove_dir_all(&srv_dir).await;
        } else {
            let _ = clear_directory_contents(&srv_dir);
        }
    }

    res
}
