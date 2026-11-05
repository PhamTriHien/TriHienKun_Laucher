use crate::audio::{AudioEngine, SoundEffect};
use crate::theme::apply_theme;
use crate::updater::{check_latest_release, download_verified_update, UpdateReleaseInfo};
use chrono::Utc;
use egui_commonmark::CommonMarkCache;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thk_auth::{
    build_desktop_auth_challenge, complete_desktop_auth_challenge, create_offline_account,
    login_with_loopback_pkce, poll_device_code_login, refresh_microsoft_session,
    start_device_code_login, DesktopAuthChallenge, DeviceCodeSession, TokenStore,
};
use thk_core::models::{
    Account, AccountType, AppSettings, ContentKind, DailyPlayStat, DownloadProgress, GameProfile,
    LoaderType, ManagedServer, ProviderSource, ServerBookmark, ServerEngine, SkinVariant,
};
use thk_core::{AppPaths, Database, DownloadManager, DownloadTask, I18n, SafePath};
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
    delete_local_content, delete_local_world, enrich_unmatched_via_modrinth_sha1,
    fetch_provider_project, fetch_provider_versions, import_curseforge_zip, import_mrpack_archive,
    install_curseforge_content, install_curseforge_modpack_version, install_ftb_modpack_version,
    install_modrinth_content, install_modrinth_modpack_version, install_spiget_plugin,
    install_technic_modpack_version, list_local_content, list_local_worlds, search_provider,
    toggle_local_content, ImportedPackMetadata, LocalContentItem, LocalWorldInfo, ProjectDetail,
    ProjectSummary, ProjectVersion, SearchQuery, SearchSort,
};
use thk_network::{
    detect_primary_lan_ipv4, ping_minecraft_server, DiscordPresenceState, DiscordRpcService,
    LanShareCoordinator, LanTunnelEvent, LanWorldAnnouncement, ServerStatusPing, TunnelStatus,
};
use thk_renderer3d::{
    load_and_normalize_cape_file, load_and_normalize_skin_file, BackEquipmentMode,
    SkinPreviewViewport,
};
use thk_server::{
    add_banned_player, compress_server_entries, delete_server_entries, extract_server_zip,
    import_external_file_streaming, install_server_engine, list_server_dir,
    list_supported_versions_for_engine, read_banned_players, read_server_properties,
    read_server_text_file, read_whitelist, remove_banned_player, remove_whitelist_player,
    upsert_whitelist_player, write_server_properties_patch, write_server_text_file,
    BannedPlayerEntry, LocalServerSupervisor, ServerFileEntry, ServerResourceStats,
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

pub enum AppAsyncEvent {
    Toast {
        kind: ToastKind,
        text: String,
    },
    TaskProgress(Option<DownloadProgress>),
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
        hits: Vec<ProjectSummary>,
    },
    InlineProjectSelected {
        detail: ProjectDetail,
        versions: Vec<ProjectVersion>,
    },
    BookmarkPingResult {
        bookmark_id: String,
        result: Result<ServerStatusPing, String>,
    },
    PackImported {
        profile: GameProfile,
    },
    ModpackSearchCompleted {
        hits: Vec<ProjectSummary>,
        total: usize,
    },
    ModpackProjectLoaded {
        detail: ProjectDetail,
        versions: Vec<ProjectVersion>,
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
    ServerAddonSearchCompleted {
        hits: Vec<ProjectSummary>,
    },
    DeviceCodeReady(DeviceCodeSession),
    MicrosoftAccountLoggedIn(Account),
    AuthError(String),
    AccountTextureSaved {
        account: Account,
    },
    JavaScanCompleted(Vec<JavaInstallation>),
    JavaDistroPackagesLoaded {
        distro: JavaDistroKind,
        packages: Vec<DistroReleasePackage>,
    },
    UpdateCheckCompleted(Result<Option<UpdateReleaseInfo>, String>),
    UpdateDownloaded(Result<PathBuf, String>),
}

pub struct NewProfileDraft {
    pub name: String,
    pub game_version: String,
    pub loader: LoaderType,
    pub loader_version: String,
    pub ram_mb: u32,
    pub auto_perf_mods: bool,
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
    pub show_online_browser: bool,
    pub online_provider: ProviderSource,
    pub online_query: String,
    pub online_hits: Vec<ProjectSummary>,
    pub online_selected: Option<(ProjectDetail, Vec<ProjectVersion>)>,
    pub worlds: Vec<LocalWorldInfo>,
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
            show_online_browser: false,
            online_provider: ProviderSource::Modrinth,
            online_query: String::new(),
            online_hits: Vec::new(),
            online_selected: None,
            worlds: Vec::new(),
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

pub struct ModpacksViewState {
    pub provider: ProviderSource,
    pub query: String,
    pub mc_version_filter: String,
    pub sort: SearchSort,
    pub hits: Vec<ProjectSummary>,
    pub total_hits: usize,
    pub selected: Option<(ProjectDetail, Vec<ProjectVersion>)>,
    pub chosen_version_idx: usize,
    pub md_cache: CommonMarkCache,
}

impl Default for ModpacksViewState {
    fn default() -> Self {
        Self {
            provider: ProviderSource::Modrinth,
            query: String::new(),
            mc_version_filter: String::new(),
            sort: SearchSort::Downloads,
            hits: Vec::new(),
            total_hits: 0,
            selected: None,
            chosen_version_idx: 0,
            md_cache: CommonMarkCache::default(),
        }
    }
}

pub struct NewServerDraft {
    pub name: String,
    pub engine: ServerEngine,
    pub game_version: String,
    pub port: u16,
    pub ram_min_mb: u32,
    pub ram_max_mb: u32,
    pub install_immediately: bool,
}

impl Default for NewServerDraft {
    fn default() -> Self {
        Self {
            name: "TriHienKun SMP".into(),
            engine: ServerEngine::Paper,
            game_version: "1.21.4".into(),
            port: 25565,
            ram_min_mb: 1024,
            ram_max_mb: 4096,
            install_immediately: true,
        }
    }
}

pub struct ServersViewState {
    pub servers: Vec<ManagedServer>,
    pub selected_server_id: Option<String>,
    pub active_tab: ServerTab,
    pub new_draft: NewServerDraft,
    pub engine_versions: HashMap<ServerEngine, Vec<String>>,
    pub console_input: String,
    pub server_logs: HashMap<String, Vec<String>>,
    pub server_stats: HashMap<String, ServerResourceStats>,
    pub server_states: HashMap<String, ServerRuntimeState>,
    pub server_tunnels: HashMap<String, TunnelStatus>,
    pub current_rel_dir: String,
    pub dir_entries: Vec<ServerFileEntry>,
    pub selected_entries: HashSet<String>,
    pub archive_name_input: String,
    pub editing_file_rel: Option<String>,
    pub editing_file_text: String,
    pub properties_map: BTreeMap<String, String>,
    pub new_prop_key: String,
    pub new_prop_val: String,
    pub whitelist: Vec<WhitelistEntry>,
    pub banlist: Vec<BannedPlayerEntry>,
    pub new_wl_name: String,
    pub new_ban_name: String,
    pub new_ban_reason: String,
    pub addon_provider: ProviderSource,
    pub addon_kind: ContentKind,
    pub addon_query: String,
    pub addon_hits: Vec<ProjectSummary>,
}

impl Default for ServersViewState {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
            selected_server_id: None,
            active_tab: ServerTab::Console,
            new_draft: NewServerDraft::default(),
            engine_versions: HashMap::new(),
            console_input: String::new(),
            server_logs: HashMap::new(),
            server_stats: HashMap::new(),
            server_states: HashMap::new(),
            server_tunnels: HashMap::new(),
            current_rel_dir: String::new(),
            dir_entries: Vec::new(),
            selected_entries: HashSet::new(),
            archive_name_input: "backup.zip".into(),
            editing_file_rel: None,
            editing_file_text: String::new(),
            properties_map: BTreeMap::new(),
            new_prop_key: String::new(),
            new_prop_val: String::new(),
            whitelist: Vec::new(),
            banlist: Vec::new(),
            new_wl_name: String::new(),
            new_ban_name: String::new(),
            new_ban_reason: "Banned by operator".into(),
            addon_provider: ProviderSource::Modrinth,
            addon_kind: ContentKind::Plugin,
            addon_query: String::new(),
            addon_hits: Vec::new(),
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
            selected_distro: JavaDistroKind::Adoptium,
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
}

impl Default for LanModalState {
    fn default() -> Self {
        Self {
            scanner_active: false,
            detected_world: None,
            ignored_foreign_packets: 0,
            manual_port_input: "25565".into(),
            auto_open_tunnel: true,
            tunnel_status: TunnelStatus::Idle,
            primary_lan_ip: Some(detect_primary_lan_ipv4().to_string()),
        }
    }
}

#[derive(Default)]
pub struct UpdaterViewState {
    pub checking: bool,
    pub available: Option<UpdateReleaseInfo>,
    pub downloaded_file: Option<PathBuf>,
    pub status_line: Option<String>,
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

    pub active_progress: Option<DownloadProgress>,
    pub busy_message: Option<String>,
    pub toasts: VecDeque<ToastMessage>,

    pub home: HomeViewState,
    pub modpacks: ModpacksViewState,
    pub servers: ServersViewState,
    pub accounts: AccountsViewState,
    pub java_modal: JavaModalState,
    pub lan_modal: LanModalState,
    pub updater: UpdaterViewState,
}

impl LauncherAppState {
    pub fn initialize(
        rt: tokio::runtime::Handle,
        egui_ctx: egui::Context,
        paths: AppPaths,
        db: Arc<Database>,
        render_state: Option<&egui_wgpu::RenderState>,
    ) -> Self {
        let settings = db.load_settings().unwrap_or_default();
        let i18n = I18n::new(settings.language);
        let downloader = DownloadManager::new().expect("HTTP DownloadManager init");
        let audio = AudioEngine::start();
        audio.sync_music(settings.music_enabled, settings.music_volume, None);

        let discord_rpc = DiscordRpcService::start();
        if settings.discord_rpc_enabled {
            discord_rpc.set_presence(settings.discord_client_id.clone(), DiscordPresenceState::Menu);
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
            active_progress: None,
            busy_message: None,
            toasts: VecDeque::new(),
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
                skin_viewport: render_state.map(SkinPreviewViewport::new),
                ..AccountsViewState::default()
            },
            java_modal: JavaModalState::default(),
            lan_modal: LanModalState::default(),
            updater: UpdaterViewState::default(),
        };

        app.home.new_draft.ram_mb = app.settings.default_ram_mb;
        app.sync_profile_edit_draft();
        app.refresh_mojang_versions();
        app.refresh_selected_profile_tabs();
        app.refresh_selected_server_tabs();
        app.search_modpacks();
        if app.settings.auto_check_updates {
            app.check_app_updates(true);
        }

        app
    }

    pub fn push_toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        let msg = text.into();
        match kind {
            ToastKind::Success => self.audio.play_sfx(self.settings.sfx_enabled, SoundEffect::Success),
            ToastKind::Error => self.audio.play_sfx(self.settings.sfx_enabled, SoundEffect::Error),
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
        self.audio.play_sfx(self.settings.sfx_enabled, SoundEffect::Click);
    }

    pub fn set_section(&mut self, next: ActiveSection) {
        if self.section != next {
            self.click_sfx();
            self.section = next;
            if self.settings.discord_rpc_enabled && self.instance_supervisor.running_keys().is_empty() {
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

    pub fn persist_settings(&mut self) {
        self.i18n.set_language(self.settings.language);
        apply_theme(&self.egui_ctx, self.settings.theme);
        self.audio.sync_music(
            self.settings.music_enabled,
            self.settings.music_volume,
            None,
        );
        if self.settings.discord_rpc_enabled {
            self.discord_rpc
                .set_presence(self.settings.discord_client_id.clone(), DiscordPresenceState::Menu);
        } else {
            self.discord_rpc.set_presence(None, DiscordPresenceState::Disabled);
        }
        let _ = self.db.save_settings(&self.settings);
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
        self.refresh_selected_server_tabs();
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
            };
        }
    }

    pub fn save_profile_edits(&mut self) {
        let Some(mut prof) = self.selected_profile().cloned() else {
            return;
        };
        if self.home.edit_draft.name.trim().is_empty() {
            self.push_toast(ToastKind::Error, "Tên Profile không được để trống.");
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

        match self.db.upsert_profile(&prof) {
            Ok(()) => {
                self.home.profiles = self.db.list_profiles().unwrap_or_default();
                self.push_toast(ToastKind::Success, "Đã lưu cấu hình Profile.");
            }
            Err(e) => self.push_toast(ToastKind::Error, format!("Lỗi lưu Profile: {e}")),
        }
    }

    pub fn delete_selected_profile(&mut self) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        for snap in self.instance_supervisor.running_snapshots() {
            if snap.key.profile_id == prof.id {
                self.instance_supervisor.stop_instance(&snap.key);
            }
        }
        if let Err(e) = self.db.delete_profile(&prof.id) {
            self.push_toast(ToastKind::Error, format!("Không thể xóa Profile: {e}"));
            return;
        }
        self.home.profiles = self.db.list_profiles().unwrap_or_default();
        self.home.selected_profile_id = self.home.profiles.first().map(|p| p.id.clone());
        self.sync_profile_edit_draft();
        self.refresh_selected_profile_tabs();
        self.push_toast(ToastKind::Info, format!("Đã xóa Profile '{}'.", prof.name));
    }

    pub fn create_profile_from_draft(&mut self) {
        let name = self.home.new_draft.name.trim().to_string();
        let game_version = self.home.new_draft.game_version.trim().to_string();
        if name.is_empty() || game_version.is_empty() {
            self.push_toast(ToastKind::Error, "Vui lòng nhập tên Profile và phiên bản Minecraft.");
            return;
        }
        let id = Uuid::new_v4().to_string();
        let slug = format!("{}-{}", SafePath::slugify_dir_name(&name), &id[..8]);
        let Ok(instance_path) = SafePath::resolve_child(&self.paths.instances_dir, &slug) else {
            self.push_toast(ToastKind::Error, "Đường dẫn thư mục Profile không hợp lệ.");
            return;
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
            is_custom_path: false,
            ram_mb: self.home.new_draft.ram_mb.max(512),
            java_path: None,
            jvm_args: String::new(),
            auto_perf_mods: loader == LoaderType::Fabric && self.home.new_draft.auto_perf_mods,
            icon_url: None,
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
                self.push_toast(ToastKind::Success, "Đã tạo Profile mới thành công!");
            }
            Err(e) => self.push_toast(ToastKind::Error, format!("Lỗi tạo Profile: {e}")),
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
            self.home.daily_stats.clear();
            return;
        };

        self.home.worlds = list_local_worlds(&prof.instance_path).unwrap_or_default();
        self.home.daily_stats = self.db.get_daily_play_stats(&prof.id, 14).unwrap_or_default();

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
            let tx = self.ui_tx.clone();
            let ctx = self.egui_ctx.clone();
            self.rt.spawn(async move {
                if let Ok(enriched) = enrich_unmatched_via_modrinth_sha1(&dl, &db, &pid, &inst, kind).await {
                    if enriched > 0 {
                        if let Ok(items) = list_local_content(&db, &pid, &inst, kind) {
                            let _ = tx.send(AppAsyncEvent::LocalContentRefreshed {
                                profile_id: pid,
                                kind,
                                items,
                            });
                            ctx.request_repaint();
                        }
                    }
                }
            });
        }
    }

    pub fn toggle_local_item(&mut self, kind: ContentKind, file_name: &str, enabled: bool) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        match toggle_local_content(&self.db, &prof.id, &prof.instance_path, kind, file_name, enabled) {
            Ok(_) => {
                self.home.local_items =
                    list_local_content(&self.db, &prof.id, &prof.instance_path, kind)
                        .unwrap_or_default();
            }
            Err(e) => self.push_toast(ToastKind::Error, format!("Không thể bật/tắt tệp: {e}")),
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
                self.push_toast(ToastKind::Info, format!("Đã xóa '{file_name}'."));
            }
            Err(e) => self.push_toast(ToastKind::Error, format!("Lỗi xóa tệp: {e}")),
        }
    }

    pub fn delete_world_folder(&mut self, folder_name: &str) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        match delete_local_world(&prof.instance_path, folder_name) {
            Ok(()) => {
                self.home.worlds = list_local_worlds(&prof.instance_path).unwrap_or_default();
                self.push_toast(ToastKind::Info, format!("Đã xóa thế giới '{folder_name}'."));
            }
            Err(e) => self.push_toast(ToastKind::Error, format!("Không thể xóa thế giới: {e}")),
        }
    }

    pub fn search_inline_profile_content(&mut self, kind: ContentKind) {
        let Some(prof) = self.selected_profile().cloned() else {
            return;
        };
        let provider = self.home.online_provider;
        let query = SearchQuery {
            query: self.home.online_query.trim().to_string(),
            content_kind: kind,
            game_version: Some(prof.game_version),
            loader: (kind == ContentKind::Mod && prof.loader != LoaderType::Vanilla)
                .then_some(prof.loader),
            sort: SearchSort::Downloads,
            offset: 0,
            limit: 20,
        };
        let cf_key = self.settings.curseforge_api_key.clone();
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(format!("Đang tìm kiếm trên {}...", provider.display_name()));

        self.rt.spawn(async move {
            let res = search_provider(&dl, provider, &query, cf_key.as_deref()).await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(page) => {
                    let _ = tx.send(AppAsyncEvent::InlineSearchCompleted { hits: page.hits });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Lỗi tìm kiếm {}: {e}", provider.display_name()),
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
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        let loader = (kind == ContentKind::Mod && prof.loader != LoaderType::Vanilla)
            .then_some(prof.loader);
        self.busy_message = Some(format!("Đang tải phiên bản của '{}'...", summary.title));

        self.rt.spawn(async move {
            let (det_res, ver_res) = tokio::join!(
                fetch_provider_project(&dl, summary.provider, &summary.project_id, cf_key.as_deref()),
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
                        text: format!("Không thể tải thông tin gói: {e}"),
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
        let dl = self.downloader.clone();
        let db = Arc::clone(&self.db);
        let cf_key = self.settings.curseforge_api_key.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let tx_prog = tx.clone();
            let ctx_prog = ctx.clone();
            let on_prog = move |p: DownloadProgress| {
                let _ = tx_prog.send(AppAsyncEvent::TaskProgress(Some(p)));
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
                        None,
                        on_prog,
                    )
                    .await
                }
                _ => {
                    install_modrinth_content(&dl, &db, &prof, kind, &summary, &version, None, on_prog)
                        .await
                }
            };

            let _ = tx.send(AppAsyncEvent::TaskProgress(None));
            match res {
                Ok(files) => {
                    if let Ok(items) = list_local_content(&db, &prof.id, &prof.instance_path, kind) {
                        let _ = tx.send(AppAsyncEvent::LocalContentRefreshed {
                            profile_id: prof.id.clone(),
                            kind,
                            items,
                        });
                    }
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Success,
                        text: format!(
                            "Đã cài đặt '{}' ({} tệp bao gồm thư viện phụ thuộc).",
                            summary.title,
                            files.len()
                        ),
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Cài đặt '{}' thất bại: {e}", summary.title),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn import_modpack_from_disk(&mut self) {
        let dl = self.downloader.clone();
        let db = Arc::clone(&self.db);
        let instances_dir = self.paths.instances_dir.clone();
        let default_ram = self.settings.default_ram_mb;
        let cf_key = self.settings.curseforge_api_key.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Chọn gói Modpack (.mrpack hoặc CurseForge .zip)")
                .add_filter("Modpack Archive", &["mrpack", "zip"])
                .pick_file()
                .await;
            let Some(handle) = picked else {
                return;
            };
            let archive_path = handle.path().to_path_buf();
            let id = Uuid::new_v4().to_string();
            let stem = archive_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "modpack".into());
            let folder = format!("{}-{}", SafePath::slugify_dir_name(&stem), &id[..8]);
            let Ok(inst_dir) = SafePath::resolve_child(&instances_dir, &folder) else {
                return;
            };

            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let on_prog = move |p: DownloadProgress| {
                let _ = tx_p.send(AppAsyncEvent::TaskProgress(Some(p)));
                ctx_p.request_repaint();
            };

            let is_mrpack = archive_path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("mrpack"));

            let meta_res = if is_mrpack {
                import_mrpack_archive(&dl, &archive_path, &inst_dir, None, on_prog).await
            } else {
                import_curseforge_zip(
                    &dl,
                    &archive_path,
                    &inst_dir,
                    cf_key.as_deref(),
                    None,
                    on_prog,
                )
                .await
            };

            let _ = tx.send(AppAsyncEvent::TaskProgress(None));
            match meta_res {
                Ok(meta) => {
                    let profile = build_profile_from_imported_pack(id, inst_dir, meta, default_ram);
                    if db.upsert_profile(&profile).is_ok() {
                        let _ = tx.send(AppAsyncEvent::PackImported { profile });
                    }
                }
                Err(e) => {
                    let _ = tokio::fs::remove_dir_all(&inst_dir).await;
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Nhập Modpack thất bại: {e}"),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn launch_selected_profile(&mut self, quick_play_server: Option<String>) {
        let Some(profile) = self.selected_profile().cloned() else {
            self.push_toast(ToastKind::Error, "Vui lòng chọn hoặc tạo một Profile trước.");
            return;
        };
        let Some(account) = self.selected_account().cloned() else {
            self.section = ActiveSection::Accounts;
            self.push_toast(
                ToastKind::Info,
                "Hãy thêm tài khoản Microsoft hoặc Offline trước khi vào game.",
            );
            return;
        };

        let custom_cid = {
            let trimmed = self.accounts.custom_client_id.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };

        match self.instance_supervisor.launch_async(LaunchRequest {
            profile: profile.clone(),
            account,
            quick_play_server,
            custom_ms_client_id: custom_cid,
        }) {
            Ok(_) => {
                self.audio
                    .play_sfx(self.settings.sfx_enabled, SoundEffect::Launch);
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
            self.push_toast(ToastKind::Error, "Vui lòng nhập địa chỉ máy chủ.");
            return;
        }
        let bm = ServerBookmark {
            id: Uuid::new_v4().to_string(),
            name: if name.is_empty() { addr.clone() } else { name },
            address: addr,
            created_at: Utc::now().timestamp(),
        };
        if let Err(e) = self.db.upsert_bookmark(&bm) {
            self.push_toast(ToastKind::Error, format!("Lỗi lưu máy chủ: {e}"));
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
        let provider = self.modpacks.provider;
        let gv = {
            let t = self.modpacks.mc_version_filter.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        let query = SearchQuery {
            query: self.modpacks.query.trim().to_string(),
            content_kind: ContentKind::Modpack,
            game_version: gv,
            loader: None,
            sort: self.modpacks.sort,
            offset: 0,
            limit: 24,
        };
        let cf_key = self.settings.curseforge_api_key.clone();
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(format!(
            "Đang tải danh sách Modpack từ {}...",
            provider.display_name()
        ));

        self.rt.spawn(async move {
            let res = search_provider(&dl, provider, &query, cf_key.as_deref()).await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(page) => {
                    let _ = tx.send(AppAsyncEvent::ModpackSearchCompleted {
                        hits: page.hits,
                        total: page.total_hits,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Không thể tải {}: {e}", provider.display_name()),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn open_modpack_detail(&mut self, summary: ProjectSummary) {
        let dl = self.downloader.clone();
        let cf_key = self.settings.curseforge_api_key.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        let gv_filter = {
            let t = self.modpacks.mc_version_filter.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        self.busy_message = Some(format!("Đang tải chi tiết '{}'...", summary.title));

        self.rt.spawn(async move {
            let (d_res, v_res) = tokio::join!(
                fetch_provider_project(&dl, summary.provider, &summary.project_id, cf_key.as_deref()),
                fetch_provider_versions(
                    &dl,
                    summary.provider,
                    &summary.project_id,
                    gv_filter.as_deref(),
                    None,
                    cf_key.as_deref()
                )
            );
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match (d_res, v_res) {
                (Ok(detail), Ok(versions)) => {
                    let _ = tx.send(AppAsyncEvent::ModpackProjectLoaded { detail, versions });
                }
                (Err(e), _) | (_, Err(e)) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Không thể tải chi tiết Modpack: {e}"),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn install_selected_modpack(&mut self) {
        let Some((detail, versions)) = self.modpacks.selected.clone() else {
            return;
        };
        let Some(version) = versions.get(self.modpacks.chosen_version_idx).cloned() else {
            self.push_toast(ToastKind::Error, "Modpack này chưa có phiên bản khả dụng.");
            return;
        };

        let dl = self.downloader.clone();
        let db = Arc::clone(&self.db);
        let cache_dir = self.paths.cache_dir.clone();
        let instances_dir = self.paths.instances_dir.clone();
        let cf_key = self.settings.curseforge_api_key.clone();
        let default_ram = self.settings.default_ram_mb;
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let id = Uuid::new_v4().to_string();
            let folder = format!(
                "{}-{}",
                SafePath::slugify_dir_name(&detail.summary.title),
                &id[..8]
            );
            let Ok(inst_dir) = SafePath::resolve_child(&instances_dir, &folder) else {
                return;
            };

            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let on_prog = move |p: DownloadProgress| {
                let _ = tx_p.send(AppAsyncEvent::TaskProgress(Some(p)));
                ctx_p.request_repaint();
            };

            let icon = detail.summary.icon_url.clone();
            let res = match detail.summary.provider {
                ProviderSource::Modrinth => {
                    install_modrinth_modpack_version(
                        &dl, &version, &cache_dir, &inst_dir, icon, None, on_prog,
                    )
                    .await
                }
                ProviderSource::CurseForge => {
                    install_curseforge_modpack_version(
                        &dl,
                        &version,
                        &cache_dir,
                        &inst_dir,
                        icon,
                        cf_key.as_deref(),
                        None,
                        on_prog,
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
                        &inst_dir,
                        cf_key.as_deref(),
                        None,
                        on_prog,
                    )
                    .await
                }
                ProviderSource::Technic => {
                    install_technic_modpack_version(
                        &dl,
                        &detail.summary.slug,
                        &version.id,
                        &cache_dir,
                        &inst_dir,
                        None,
                        on_prog,
                    )
                    .await
                }
                _ => return,
            };

            let _ = tx.send(AppAsyncEvent::TaskProgress(None));
            match res {
                Ok(meta) => {
                    let profile = build_profile_from_imported_pack(id, inst_dir, meta, default_ram);
                    if db.upsert_profile(&profile).is_ok() {
                        let _ = tx.send(AppAsyncEvent::PackImported { profile });
                    }
                }
                Err(e) => {
                    let _ = tokio::fs::remove_dir_all(&inst_dir).await;
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Cài đặt Modpack thất bại: {e}"),
                    });
                }
            }
            ctx.request_repaint();
        });
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

    pub fn create_server_from_draft(&mut self) {
        let name = self.servers.new_draft.name.trim().to_string();
        let mc_ver = self.servers.new_draft.game_version.trim().to_string();
        if name.is_empty() || mc_ver.is_empty() {
            self.push_toast(ToastKind::Error, "Vui lòng nhập tên máy chủ và phiên bản Minecraft.");
            return;
        }

        let id = Uuid::new_v4().to_string();
        let folder = format!("{}-{}", SafePath::slugify_dir_name(&name), &id[..8]);
        let Ok(srv_dir) = SafePath::resolve_child(&self.paths.servers_dir, &folder) else {
            return;
        };
        let _ = std::fs::create_dir_all(&srv_dir);
        let port = self.servers.new_draft.port.max(1024);
        let mut init_props = BTreeMap::new();
        init_props.insert("server-port".into(), port.to_string());
        init_props.insert("motd".into(), format!("{} - TriHienKun Laucher", name));
        let _ = write_server_properties_patch(&srv_dir, &init_props);

        let srv = ManagedServer {
            id: id.clone(),
            name,
            engine: self.servers.new_draft.engine,
            game_version: mc_ver,
            server_dir: srv_dir,
            is_custom_dir: false,
            port,
            ram_min_mb: self.servers.new_draft.ram_min_mb.max(512),
            ram_max_mb: self
                .servers
                .new_draft
                .ram_max_mb
                .max(self.servers.new_draft.ram_min_mb.max(512)),
            java_path: None,
            jvm_flags: String::new(),
            jar_file: String::new(),
            created_at: Utc::now().timestamp(),
        };

        let install_now = self.servers.new_draft.install_immediately;
        match self.db.upsert_server(&srv) {
            Ok(()) => {
                self.servers.servers = self.db.list_servers().unwrap_or_default();
                self.show_new_server_modal = false;
                self.select_server(id.clone());
                self.push_toast(ToastKind::Success, "Đã tạo máy chủ cục bộ!");
                if install_now {
                    self.install_selected_server_jar();
                }
            }
            Err(e) => self.push_toast(ToastKind::Error, format!("Lỗi tạo máy chủ: {e}")),
        }
    }

    pub fn install_selected_server_jar(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let dl = self.downloader.clone();
        let sup = self.server_supervisor.clone();
        let db = Arc::clone(&self.db);
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.busy_message = Some(format!(
            "Đang cài đặt lõi {} ({})...",
            srv.engine.display_name(),
            srv.game_version
        ));

        self.rt.spawn(async move {
            let java_res = sup.resolve_java_for_server(&srv).await;
            let java_bin = match java_res {
                Ok(j) => j,
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::BusyStatus(None));
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Không thể chuẩn bị Java cho máy chủ: {e}"),
                    });
                    ctx.request_repaint();
                    return;
                }
            };

            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let res = install_server_engine(
                &dl,
                srv.engine,
                &srv.game_version,
                &srv.server_dir,
                &java_bin,
                move |p| {
                    let _ = tx_p.send(AppAsyncEvent::TaskProgress(Some(p)));
                    ctx_p.request_repaint();
                },
            )
            .await;

            let _ = tx.send(AppAsyncEvent::TaskProgress(None));
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
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Tải lõi {} thất bại: {e}", srv.engine.display_name()),
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
        let sup = self.server_supervisor.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        if self.settings.discord_rpc_enabled {
            self.discord_rpc.set_presence(
                self.settings.discord_client_id.clone(),
                DiscordPresenceState::HostingServer {
                    name: srv.name.clone(),
                    engine: srv.engine.display_name().to_string(),
                },
            );
        }
        self.rt.spawn(async move {
            if let Err(e) = sup.start_server(&srv, true).await {
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Error,
                    text: format!("Không thể khởi động máy chủ: {e}"),
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

    pub fn restart_selected_server(&self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let sup = self.server_supervisor.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Err(e) = sup.restart_server(&srv, true).await {
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Error,
                    text: format!("Khởi động lại thất bại: {e}"),
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn send_selected_server_command(&mut self) {
        let Some(srv) = self.selected_server() else {
            return;
        };
        let cmd = self.servers.console_input.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        let srv_id = srv.id.clone();
        match self.server_supervisor.send_command(&srv_id, &cmd) {
            Ok(()) => {
                self.servers
                    .server_logs
                    .entry(srv_id)
                    .or_default()
                    .push(format!("> {cmd}"));
                self.servers.console_input.clear();
            }
            Err(e) => self.push_toast(ToastKind::Error, e.to_string()),
        }
    }

    pub fn delete_selected_server(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        if self.server_supervisor.runtime_state(&srv.id) != ServerRuntimeState::Offline {
            self.push_toast(
                ToastKind::Error,
                "Vui lòng dừng máy chủ trước khi xóa khỏi danh sách.",
            );
            return;
        }
        self.server_supervisor.stop_server_tunnel(&srv.id);
        let _ = self.db.delete_server(&srv.id);
        self.servers.servers = self.db.list_servers().unwrap_or_default();
        self.servers.selected_server_id = self.servers.servers.first().map(|s| s.id.clone());
        self.refresh_selected_server_tabs();
        self.push_toast(ToastKind::Info, format!("Đã xóa máy chủ '{}'.", srv.name));
    }

    pub fn refresh_selected_server_tabs(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            self.servers.dir_entries.clear();
            self.servers.properties_map.clear();
            self.servers.whitelist.clear();
            self.servers.banlist.clear();
            return;
        };
        self.servers.dir_entries =
            list_server_dir(&srv.server_dir, &self.servers.current_rel_dir).unwrap_or_default();
        self.servers.properties_map = read_server_properties(&srv.server_dir).unwrap_or_default();
        self.servers.whitelist = read_whitelist(&srv.server_dir).unwrap_or_default();
        self.servers.banlist = read_banned_players(&srv.server_dir).unwrap_or_default();
        if srv.engine.supports_plugins() {
            self.servers.addon_kind = ContentKind::Plugin;
        } else if srv.engine.supports_mods() {
            self.servers.addon_kind = ContentKind::Mod;
        }
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
            Err(e) => self.push_toast(ToastKind::Error, format!("Không thể mở tệp: {e}")),
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
            Ok(()) => self.push_toast(ToastKind::Success, format!("Đã lưu tệp '{rel}'.")),
            Err(e) => self.push_toast(ToastKind::Error, format!("Lỗi lưu tệp: {e}")),
        }
    }

    pub fn import_external_file_into_server(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let rel_dir = self.servers.current_rel_dir.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Chọn tệp tải lên thư mục Máy chủ")
                .pick_file()
                .await;
            let Some(file) = picked else {
                return;
            };
            match import_external_file_streaming(&srv.server_dir, &rel_dir, file.path()).await {
                Ok(_) => {
                    let _ = tx.send(AppAsyncEvent::ServerExternalFileImported {
                        server_id: srv.id,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Không thể chép tệp vào máy chủ: {e}"),
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
        let targets: Vec<String> = self.servers.selected_entries.drain().collect();
        if targets.is_empty() {
            return;
        }
        let count = targets.len();
        match delete_server_entries(&srv.server_dir, &targets) {
            Ok(()) => {
                self.servers.dir_entries =
                    list_server_dir(&srv.server_dir, &self.servers.current_rel_dir)
                        .unwrap_or_default();
                self.push_toast(ToastKind::Info, format!("Đã xóa {count} mục."));
            }
            Err(e) => self.push_toast(ToastKind::Error, format!("Lỗi xóa tệp: {e}")),
        }
    }

    pub fn compress_selected_server_files(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let targets: Vec<String> = self.servers.selected_entries.iter().cloned().collect();
        if targets.is_empty() {
            self.push_toast(ToastKind::Error, "Hãy đánh dấu ít nhất 1 tệp/thư mục để nén.");
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
        match compress_server_entries(&srv.server_dir, &targets, &zip_rel) {
            Ok(_) => {
                self.servers.selected_entries.clear();
                self.servers.dir_entries =
                    list_server_dir(&srv.server_dir, &self.servers.current_rel_dir)
                        .unwrap_or_default();
                self.push_toast(ToastKind::Success, format!("Đã nén thành '{zip_rel}'."));
            }
            Err(e) => self.push_toast(ToastKind::Error, format!("Lỗi nén ZIP: {e}")),
        }
    }

    pub fn extract_server_archive(&mut self, rel_zip: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        match extract_server_zip(&srv.server_dir, rel_zip) {
            Ok(cnt) => {
                self.servers.dir_entries =
                    list_server_dir(&srv.server_dir, &self.servers.current_rel_dir)
                        .unwrap_or_default();
                self.push_toast(ToastKind::Success, format!("Đã giải nén {cnt} tệp."));
            }
            Err(e) => self.push_toast(ToastKind::Error, format!("Giải nén thất bại: {e}")),
        }
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
            Ok(()) => self.push_toast(ToastKind::Success, "Đã lưu server.properties!"),
            Err(e) => self.push_toast(ToastKind::Error, format!("Lỗi lưu server.properties: {e}")),
        }
    }

    pub fn add_server_whitelist(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let name = self.servers.new_wl_name.trim().to_string();
        match upsert_whitelist_player(&srv.server_dir, &name, "") {
            Ok(list) => {
                self.servers.whitelist = list;
                self.servers.new_wl_name.clear();
                if self.server_supervisor.runtime_state(&srv.id) == ServerRuntimeState::Online {
                    let _ = self
                        .server_supervisor
                        .send_command(&srv.id, &format!("whitelist add {name}"));
                }
            }
            Err(e) => self.push_toast(ToastKind::Error, e.to_string()),
        }
    }

    pub fn remove_server_whitelist(&mut self, name: &str) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        if let Ok(list) = remove_whitelist_player(&srv.server_dir, name) {
            self.servers.whitelist = list;
            if self.server_supervisor.runtime_state(&srv.id) == ServerRuntimeState::Online {
                let _ = self
                    .server_supervisor
                    .send_command(&srv.id, &format!("whitelist remove {name}"));
            }
        }
    }

    pub fn add_server_ban(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let name = self.servers.new_ban_name.trim().to_string();
        let reason = self.servers.new_ban_reason.trim().to_string();
        match add_banned_player(&srv.server_dir, &name, "", &reason) {
            Ok(list) => {
                self.servers.banlist = list;
                self.servers.new_ban_name.clear();
                if self.server_supervisor.runtime_state(&srv.id) == ServerRuntimeState::Online {
                    let _ = self
                        .server_supervisor
                        .send_command(&srv.id, &format!("ban {name} {reason}"));
                }
            }
            Err(e) => self.push_toast(ToastKind::Error, e.to_string()),
        }
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

    pub fn search_server_addons(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let provider = self.servers.addon_provider;
        let kind = if provider == ProviderSource::Spiget {
            ContentKind::Plugin
        } else {
            self.servers.addon_kind
        };
        let loader = match (kind, srv.engine) {
            (ContentKind::Mod, ServerEngine::Fabric) => Some(LoaderType::Fabric),
            (ContentKind::Mod, ServerEngine::Forge | ServerEngine::Mohist | ServerEngine::Arclight) => {
                Some(LoaderType::Forge)
            }
            (ContentKind::Mod, ServerEngine::NeoForge) => Some(LoaderType::NeoForge),
            _ => None,
        };
        let query = SearchQuery {
            query: self.servers.addon_query.trim().to_string(),
            content_kind: kind,
            game_version: Some(srv.game_version),
            loader,
            sort: SearchSort::Downloads,
            offset: 0,
            limit: 20,
        };
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some("Đang tìm kiếm Plugin / Mod cho máy chủ...".into());

        self.rt.spawn(async move {
            let res = search_provider(&dl, provider, &query, None).await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(page) => {
                    let _ = tx.send(AppAsyncEvent::ServerAddonSearchCompleted { hits: page.hits });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Lỗi tìm kiếm Plugin/Mod: {e}"),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn install_server_addon_item(&mut self, summary: ProjectSummary) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let kind = if summary.provider == ProviderSource::Spiget {
            ContentKind::Plugin
        } else {
            self.servers.addon_kind
        };
        let loader = match (kind, srv.engine) {
            (ContentKind::Mod, ServerEngine::Fabric) => Some(LoaderType::Fabric),
            (ContentKind::Mod, ServerEngine::Forge | ServerEngine::Mohist | ServerEngine::Arclight) => {
                Some(LoaderType::Forge)
            }
            (ContentKind::Mod, ServerEngine::NeoForge) => Some(LoaderType::NeoForge),
            _ => None,
        };
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let on_prog = move |p: DownloadProgress| {
                let _ = tx_p.send(AppAsyncEvent::TaskProgress(Some(p)));
                ctx_p.request_repaint();
            };

            let res: thk_core::ThkResult<String> = if summary.provider == ProviderSource::Spiget {
                install_spiget_plugin(
                    &dl,
                    &summary.project_id,
                    &summary.title,
                    &srv.server_dir,
                    None,
                    on_prog,
                )
                .await
                .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
            } else {
                async {
                    let vers = fetch_provider_versions(
                        &dl,
                        ProviderSource::Modrinth,
                        &summary.project_id,
                        Some(&srv.game_version),
                        loader,
                        None,
                    )
                    .await?;
                    let v = vers.first().or_else(|| vers.last()).ok_or_else(|| {
                        thk_core::ThkError::Provider(format!(
                            "'{}' chưa có bản cho MC {}",
                            summary.title, srv.game_version
                        ))
                    })?;
                    let f = v
                        .files
                        .iter()
                        .find(|f| f.primary)
                        .or_else(|| v.files.first())
                        .ok_or_else(|| {
                            thk_core::ThkError::Provider("Không có tệp tải về".into())
                        })?;
                    let subdir = SafePath::resolve_child(&srv.server_dir, kind.instance_subdir())?;
                    tokio::fs::create_dir_all(&subdir).await?;
                    let clean = SafePath::validate_single_name(&f.filename)?;
                    let dest = SafePath::resolve_child(&subdir, clean)?;
                    let mut t = DownloadTask::new(f.url.clone(), dest);
                    t.expected_sha1 = f.sha1.clone();
                    t.expected_sha512 = f.sha512.clone();
                    dl.download_batch(&summary.title, vec![t], 1, None, on_prog)
                        .await?;
                    Ok(clean.to_string())
                }
                .await
            };

            let _ = tx.send(AppAsyncEvent::TaskProgress(None));
            match res {
                Ok(fname) => {
                    let _ = tx.send(AppAsyncEvent::ServerExternalFileImported {
                        server_id: srv.id,
                    });
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Success,
                        text: format!("Đã cài đặt '{fname}' vào máy chủ!"),
                    });
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

    pub fn toggle_selected_server_tunnel(&mut self) {
        let Some(srv) = self.selected_server().cloned() else {
            return;
        };
        let status = self.server_supervisor.server_tunnel_status(&srv.id);
        if matches!(status, TunnelStatus::Running { .. } | TunnelStatus::Connecting { .. }) {
            self.server_supervisor.stop_server_tunnel(&srv.id);
            self.servers
                .server_tunnels
                .insert(srv.id, TunnelStatus::Stopped);
            return;
        }

        let sup = self.server_supervisor.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Err(e) = sup.start_server_tunnel(&srv.id, srv.port).await {
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Error,
                    text: format!("Không thể mở Bore Tunnel: {e}"),
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn add_offline_account(&mut self) {
        match create_offline_account(
            &self.accounts.offline_username,
            self.accounts.offline_variant,
        ) {
            Ok(acct) => {
                if let Err(e) = self.db.upsert_account(&acct) {
                    self.push_toast(ToastKind::Error, format!("Lỗi lưu tài khoản: {e}"));
                    return;
                }
                let id = acct.id.clone();
                let uname = acct.username.clone();
                self.accounts.offline_username.clear();
                self.accounts.accounts = self.db.list_accounts().unwrap_or_default();
                self.select_account(id);
                self.push_toast(ToastKind::Success, format!("Đã thêm tài khoản Offline '{uname}'."));
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
        self.busy_message = Some("Đang khởi tạo mã đăng nhập Microsoft...".into());

        self.rt.spawn(async move {
            match start_device_code_login(true).await {
                Ok(sess) => {
                    let _ = tx.send(AppAsyncEvent::BusyStatus(None));
                    let _ = tx.send(AppAsyncEvent::DeviceCodeReady(sess.clone()));
                    ctx.request_repaint();

                    match poll_device_code_login(&sess, cancel).await {
                        Ok(authed) => {
                            let _ = tx.send(AppAsyncEvent::MicrosoftAccountLoggedIn(authed.account));
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
        self.busy_message = Some("Đang xác thực tài khoản Microsoft & Xbox Live...".into());

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
            self.push_toast(
                ToastKind::Error,
                "Vui lòng nhập Azure App Client ID trước khi dùng Loopback PKCE.",
            );
            return;
        }
        if let Some(prev) = self.accounts.loopback_cancel.take() {
            prev.cancel();
        }
        let cancel = CancellationToken::new();
        self.accounts.loopback_cancel = Some(cancel.clone());
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some("Đang chờ phản hồi OAuth2 trên trình duyệt...".into());

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
        self.busy_message = Some("Đang làm mới phiên đăng nhập Microsoft...".into());

        self.rt.spawn(async move {
            let res = refresh_microsoft_session(&acct.id, cid.as_deref()).await;
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
        self.push_toast(ToastKind::Info, format!("Đã xóa tài khoản '{}'.", acct.username));
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
    }

    pub fn pick_account_texture(&mut self, slot: TextureSlot) {
        let Some(mut acct) = self.selected_account().cloned() else {
            return;
        };
        let target_dir = match slot {
            TextureSlot::Skin => self.paths.skins_dir.clone(),
            TextureSlot::Cape => self.paths.capes_dir.clone(),
            TextureSlot::Elytra => self.paths.elytras_dir.clone(),
        };
        let db = Arc::clone(&self.db);
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Chọn ảnh PNG (64x64 hoặc 64x32)")
                .add_filter("PNG Texture", &["png"])
                .pick_file()
                .await;
            let Some(file) = picked else {
                return;
            };
            let src_path = file.path().to_path_buf();

            let normalized_res = match slot {
                TextureSlot::Skin => load_and_normalize_skin_file(&src_path),
                TextureSlot::Cape | TextureSlot::Elytra => load_and_normalize_cape_file(&src_path),
            };

            match normalized_res {
                Ok(rgba) => {
                    let fname = format!("{}.png", SafePath::slugify_dir_name(&acct.id));
                    if let Ok(dest) = SafePath::resolve_child(&target_dir, &fname) {
                        if let Err(e) = rgba.save(&dest) {
                            let _ = tx.send(AppAsyncEvent::Toast {
                                kind: ToastKind::Error,
                                text: format!("Không thể lưu ảnh PNG: {e}"),
                            });
                            ctx.request_repaint();
                            return;
                        }
                        match slot {
                            TextureSlot::Skin => acct.custom_skin_path = Some(dest),
                            TextureSlot::Cape => acct.custom_cape_path = Some(dest),
                            TextureSlot::Elytra => acct.custom_elytra_path = Some(dest),
                        }
                        if db.upsert_account(&acct).is_ok() {
                            let _ = tx.send(AppAsyncEvent::AccountTextureSaved { account: acct });
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Ảnh không hợp lệ: {e}"),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn sync_3d_viewport_textures(&mut self, render_state: Option<&egui_wgpu::RenderState>) {
        let Some(rs) = render_state else {
            return;
        };
        if self.accounts.skin_viewport.is_none() {
            self.accounts.skin_viewport = Some(SkinPreviewViewport::new(rs));
        }
        let Some(acct) = self.selected_account().cloned() else {
            return;
        };
        let key = format!(
            "{}:{}:{:?}:{:?}:{:?}",
            acct.id,
            acct.skin_variant.as_str(),
            acct.custom_skin_path,
            acct.custom_cape_path,
            acct.custom_elytra_path
        );
        if self.accounts.loaded_viewport_key == key {
            return;
        }
        self.accounts.loaded_viewport_key = key;

        if let Some(vp) = self.accounts.skin_viewport.as_mut() {
            vp.variant = acct.skin_variant;
            match acct
                .custom_skin_path
                .as_deref()
                .and_then(|p| load_and_normalize_skin_file(p).ok())
            {
                Some(img) => vp.set_skin_rgba(rs, &img),
                None => vp.reset_default_skin(rs),
            }
            let back_path = match vp.back_mode {
                BackEquipmentMode::Elytra => acct
                    .custom_elytra_path
                    .as_ref()
                    .or(acct.custom_cape_path.as_ref()),
                _ => acct
                    .custom_cape_path
                    .as_ref()
                    .or(acct.custom_elytra_path.as_ref()),
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
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.busy_message = Some(format!("Đang tải danh sách {}...", distro.display_name()));

        self.rt.spawn(async move {
            let res = fetch_available_distro_packages(&dl, distro).await;
            let _ = tx.send(AppAsyncEvent::BusyStatus(None));
            match res {
                Ok(packages) => {
                    let _ = tx.send(AppAsyncEvent::JavaDistroPackagesLoaded { distro, packages });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Không thể tải {}: {e}", distro.display_name()),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn install_java_package(&mut self, pkg: DistroReleasePackage) {
        let dl = self.downloader.clone();
        let runtimes = self.paths.runtimes_dir.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let res = install_distro_package(&dl, &runtimes, &pkg, None, move |p| {
                let _ = tx_p.send(AppAsyncEvent::TaskProgress(Some(p)));
                ctx_p.request_repaint();
            })
            .await;

            let _ = tx.send(AppAsyncEvent::TaskProgress(None));
            match res {
                Ok(inst) => {
                    let list = tokio::task::spawn_blocking(move || scan_java_installations(&runtimes))
                        .await
                        .unwrap_or_default();
                    let _ = tx.send(AppAsyncEvent::JavaScanCompleted(list));
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Success,
                        text: format!("Đã cài đặt Java {} ({})!", inst.major_version, inst.vendor),
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Error,
                        text: format!("Cài đặt Java thất bại: {e}"),
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn toggle_lan_scanner(&mut self) {
        if self.lan_coordinator.is_scanning() {
            self.lan_coordinator.stop_scan();
        } else if let Err(e) = self
            .lan_coordinator
            .start_lan_scan(self.lan_modal.auto_open_tunnel)
        {
            self.push_toast(ToastKind::Error, e.to_string());
        }
    }

    pub fn start_manual_bore_tunnel(&mut self) {
        let Ok(port) = self.lan_modal.manual_port_input.trim().parse::<u16>() else {
            self.push_toast(ToastKind::Error, "Cổng nội bộ không hợp lệ (1 - 65535).");
            return;
        };
        if port == 0 {
            self.push_toast(ToastKind::Error, "Cổng nội bộ phải lớn hơn 0.");
            return;
        }
        let coord = self.lan_coordinator.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.rt.spawn(async move {
            if let Err(e) = coord.start_manual_tunnel(port).await {
                let _ = tx.send(AppAsyncEvent::Toast {
                    kind: ToastKind::Error,
                    text: format!("Mở Bore Tunnel thất bại: {e}"),
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
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();

        self.rt.spawn(async move {
            let tx_p = tx.clone();
            let ctx_p = ctx.clone();
            let res = auto_fix_mod_from_modrinth(&dl, &db, &prof, &mod_owned, None, move |p| {
                let _ = tx_p.send(AppAsyncEvent::TaskProgress(Some(p)));
                ctx_p.request_repaint();
            })
            .await;

            let _ = tx.send(AppAsyncEvent::TaskProgress(None));
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
                    let _ = tx.send(AppAsyncEvent::Toast {
                        kind: ToastKind::Success,
                        text: format!("Đã tự động tải và cài đặt '{mod_owned}' từ Modrinth!"),
                    });
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

    pub fn check_app_updates(&mut self, silent_if_none: bool) {
        let repo = self.settings.github_repo.clone();
        let dl = self.downloader.clone();
        let tx = self.ui_tx.clone();
        let ctx = self.egui_ctx.clone();
        self.updater.checking = true;
        self.updater.status_line = Some("Đang kiểm tra bản phát hành mới trên GitHub...".into());

        self.rt.spawn(async move {
            let res = check_latest_release(&dl, &repo, env!("CARGO_PKG_VERSION")).await;
            let mapped = match res {
                Ok(opt) => Ok(opt),
                Err(e) if silent_if_none => {
                    let _ = e;
                    Ok(None)
                }
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(AppAsyncEvent::UpdateCheckCompleted(mapped));
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
                let _ = tx_p.send(AppAsyncEvent::TaskProgress(Some(p)));
                ctx_p.request_repaint();
            })
            .await
            .map_err(|e| e.to_string());

            let _ = tx.send(AppAsyncEvent::TaskProgress(None));
            let _ = tx.send(AppAsyncEvent::UpdateDownloaded(res));
            ctx.request_repaint();
        });
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

        while let Ok(ev) = self.ui_rx.try_recv() {
            match ev {
                AppAsyncEvent::Toast { kind, text } => self.push_toast(kind, text),
                AppAsyncEvent::TaskProgress(p) => self.active_progress = p,
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
                    self.home.loader_versions.insert((mc_version, loader), entries);
                }
                AppAsyncEvent::LocalContentRefreshed {
                    profile_id,
                    kind,
                    items,
                } => {
                    let matches_selected = self.home.selected_profile_id.as_deref() == Some(&profile_id);
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
                AppAsyncEvent::InlineSearchCompleted { hits } => {
                    self.home.online_hits = hits;
                }
                AppAsyncEvent::InlineProjectSelected { detail, versions } => {
                    self.home.online_selected = Some((detail, versions));
                }
                AppAsyncEvent::BookmarkPingResult {
                    bookmark_id,
                    result,
                } => {
                    self.home.bookmark_pings.insert(bookmark_id, result);
                }
                AppAsyncEvent::PackImported { profile } => {
                    let pname = profile.name.clone();
                    let pid = profile.id.clone();
                    self.home.profiles = self.db.list_profiles().unwrap_or_default();
                    self.modpacks.selected = None;
                    self.section = ActiveSection::Home;
                    self.select_profile(pid);
                    self.push_toast(
                        ToastKind::Success,
                        format!("Đã cài đặt Modpack '{pname}' sẵn sàng chơi!"),
                    );
                }
                AppAsyncEvent::ModpackSearchCompleted { hits, total } => {
                    self.modpacks.hits = hits;
                    self.modpacks.total_hits = total;
                }
                AppAsyncEvent::ModpackProjectLoaded { detail, versions } => {
                    self.modpacks.chosen_version_idx = 0;
                    self.modpacks.selected = Some((detail, versions));
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
                        self.refresh_selected_server_tabs();
                    }
                    self.push_toast(
                        ToastKind::Success,
                        format!("Đã cài đặt lõi máy chủ ({jar_file}) sẵn sàng khởi động!"),
                    );
                }
                AppAsyncEvent::ServerExternalFileImported { server_id } => {
                    if self.servers.selected_server_id.as_deref() == Some(&server_id) {
                        self.refresh_selected_server_tabs();
                    }
                }
                AppAsyncEvent::ServerAddonSearchCompleted { hits } => {
                    self.servers.addon_hits = hits;
                }
                AppAsyncEvent::DeviceCodeReady(sess) => {
                    self.accounts.device_session = Some(sess);
                }
                AppAsyncEvent::MicrosoftAccountLoggedIn(acct) => {
                    let _ = self.db.upsert_account(&acct);
                    let id = acct.id.clone();
                    let uname = acct.username.clone();
                    self.accounts.device_session = None;
                    self.accounts.desktop_challenge = None;
                    self.accounts.desktop_redirect_input.clear();
                    self.accounts.accounts = self.db.list_accounts().unwrap_or_default();
                    self.select_account(id);
                    self.push_toast(
                        ToastKind::Success,
                        format!("Đăng nhập Microsoft thành công: {uname}!"),
                    );
                }
                AppAsyncEvent::AuthError(err) => {
                    self.push_toast(ToastKind::Error, err);
                }
                AppAsyncEvent::AccountTextureSaved { account } => {
                    self.accounts.accounts = self.db.list_accounts().unwrap_or_default();
                    self.accounts.loaded_viewport_key.clear();
                    self.push_toast(
                        ToastKind::Success,
                        format!("Đã cập nhật giao diện 3D cho '{}'!", account.username),
                    );
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
                        Ok(Some(info)) => {
                            self.updater.status_line = Some(format!(
                                "Có phiên bản mới {} ({})",
                                info.version, info.asset_name
                            ));
                            self.updater.available = Some(info.clone());
                            self.push_toast(
                                ToastKind::Info,
                                format!(
                                    "Phát hiện bản cập nhật mới TriHienKun Laucher v{}!",
                                    info.version
                                ),
                            );
                        }
                        Ok(None) => {
                            self.updater.status_line =
                                Some("Bạn đang sử dụng phiên bản mới nhất.".into());
                        }
                        Err(e) => {
                            self.updater.status_line = Some(format!("Lỗi kiểm tra cập nhật: {e}"));
                        }
                    }
                }
                AppAsyncEvent::UpdateDownloaded(res) => match res {
                    Ok(path) => {
                        let _ = open::that(&path);
                        self.updater.downloaded_file = Some(path.clone());
                        self.push_toast(
                            ToastKind::Success,
                            format!(
                                "Đã xác thực SHA-256 & mở gói cập nhật: {}",
                                path.display()
                            ),
                        );
                    }
                    Err(e) => self.push_toast(ToastKind::Error, e),
                },
            }
        }
    }

    fn handle_launcher_event(&mut self, ev: LauncherEvent) {
        match ev {
            LauncherEvent::Progress { progress, .. } => {
                self.active_progress = Some(progress);
            }
            LauncherEvent::LogLine { key, line } => {
                let buf = self.home.instance_logs.entry(key).or_default();
                if buf.len() >= 4_000 {
                    buf.remove(0);
                }
                buf.push(line);
            }
            LauncherEvent::Started { key, pid } => {
                self.active_progress = None;
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
                    self.push_toast(
                        ToastKind::Success,
                        format!("Minecraft '{}' đã khởi chạy (PID {pid})!", p.name),
                    );
                }
            }
            LauncherEvent::Exited {
                key,
                exit_code,
                crash_report,
                ..
            } => {
                self.active_progress = None;
                self.home.profiles = self.db.list_profiles().unwrap_or_default();
                if self.home.selected_profile_id.as_deref() == Some(&key.profile_id) {
                    self.home.daily_stats = self
                        .db
                        .get_daily_play_stats(&key.profile_id, 14)
                        .unwrap_or_default();
                }
                if self.settings.discord_rpc_enabled && self.instance_supervisor.running_keys().is_empty()
                {
                    self.discord_rpc
                        .set_presence(self.settings.discord_client_id.clone(), DiscordPresenceState::Menu);
                }
                if let Some(report) = crash_report {
                    self.crash_modal = Some((key.profile_id, report));
                    self.audio
                        .play_sfx(self.settings.sfx_enabled, SoundEffect::Error);
                } else {
                    self.push_toast(
                        ToastKind::Info,
                        format!("Phiên chơi đã kết thúc (mã thoát {exit_code})."),
                    );
                }
            }
            LauncherEvent::LaunchFailed { error, .. } => {
                self.active_progress = None;
                self.push_toast(ToastKind::Error, format!("Khởi chạy thất bại: {error}"));
            }
        }
    }

    fn handle_server_event(&mut self, ev: ServerSupervisorEvent) {
        match ev {
            ServerSupervisorEvent::StatusChanged { server_id, state } => {
                self.servers.server_states.insert(server_id, state);
            }
            ServerSupervisorEvent::LogLine { server_id, line } => {
                let buf = self.servers.server_logs.entry(server_id).or_default();
                if buf.len() >= 5_000 {
                    buf.remove(0);
                }
                buf.push(line);
            }
            ServerSupervisorEvent::StatsUpdated { server_id, stats } => {
                self.servers.server_stats.insert(server_id, stats);
            }
            ServerSupervisorEvent::TunnelChanged { server_id, status } => {
                self.servers.server_tunnels.insert(server_id, status);
            }
            ServerSupervisorEvent::InstallProgress { progress, .. } => {
                self.active_progress = Some(progress);
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
                self.push_toast(
                    ToastKind::Success,
                    format!("Phát hiện thế giới LAN '{}' trên cổng {}!", ann.motd, ann.port),
                );
                self.lan_modal.detected_world = Some(ann);
            }
            LanTunnelEvent::IgnoredForeignAnnouncement { .. } => {
                self.lan_modal.ignored_foreign_packets += 1;
            }
            LanTunnelEvent::TunnelStatusChanged(st) => {
                if let TunnelStatus::Running {
                    ref remote_addr,
                    remote_port,
                    ..
                } = st
                {
                    self.push_toast(
                        ToastKind::Success,
                        format!("Đã mở cổng công khai: {remote_addr}:{remote_port}"),
                    );
                }
                self.lan_modal.tunnel_status = st;
            }
        }
    }

    pub fn shutdown_all_background_services(&self) {
        self.lan_coordinator.stop_scan();
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

fn build_profile_from_imported_pack(
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
        total_play_seconds: 0,
        session_count: 0,
        crash_count: 0,
        last_played_at: None,
        created_at: Utc::now().timestamp(),
    }
}
