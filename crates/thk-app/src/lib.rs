#![deny(unsafe_code)]

mod audio;
mod dialogs;
mod fx3d;
mod pack_install;
mod state;
#[cfg(test)]
mod state_tests;
mod theme;
mod tray;
mod ui_assets;
mod updater;
mod views;

use crate::state::LauncherAppState;
use crate::tray::{TrayAction, TrayController};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex, Once};
use thk_core::models::CloseBehavior;
use thk_core::{AppPaths, Database};
use tracing_subscriber::fmt::MakeWriter;

pub(crate) const LAUNCHER_LOG_FILE_NAME: &str = "launcher.log";
const LAUNCHER_PREV_LOG_FILE_NAME: &str = "launcher.prev.log";
const MAX_LAUNCHER_LOG_BYTES: u64 = 4 * 1024 * 1024;
static PANIC_HOOK_ONCE: Once = Once::new();

struct TriHienKunApp {
    state: LauncherAppState,
    tray: Option<TrayController>,
    shutdown_completed: bool,
}

impl TriHienKunApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        rt_handle: tokio::runtime::Handle,
        paths: AppPaths,
        db: Arc<Database>,
    ) -> thk_core::ThkResult<Self> {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::install_unicode_fonts(&cc.egui_ctx);

        let state = LauncherAppState::initialize(
            rt_handle,
            cc.egui_ctx.clone(),
            paths,
            db,
            cc.wgpu_render_state.as_ref(),
        )?;
        theme::apply_theme(
            &cc.egui_ctx,
            state.settings.theme,
            state.settings.accent_preset,
        );
        theme::apply_font_scale(&cc.egui_ctx, state.settings.font_scale);

        let tray = TrayController::try_new(state.settings.language, cc.egui_ctx.clone());
        Ok(Self {
            state,
            tray,
            shutdown_completed: false,
        })
    }

    fn perform_graceful_shutdown(&mut self) {
        if !self.shutdown_completed {
            self.shutdown_completed = true;
            self.state.shutdown_all_background_services();
        }
    }
}

impl Drop for TriHienKunApp {
    fn drop(&mut self) {
        self.perform_graceful_shutdown();
    }
}

impl eframe::App for TriHienKunApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.state.drain_async_events();

        if self.state.font_scale_drag_dirty && !ctx.egui_is_using_pointer() {
            self.state.font_scale_drag_dirty = false;
            self.state.persist_settings();
        }

        if self.state.pending_memory_trim {
            self.state.pending_memory_trim = false;
            self.state.trim_launcher_memory(frame.wgpu_render_state());
        }

        if let Some(ref mut tray) = self.tray {
            tray.sync_language(self.state.settings.language);
            if let Some(action) = tray.poll_action() {
                match action {
                    TrayAction::RestoreWindow => {
                        tray::restore_and_focus_viewport(ctx);
                    }
                    TrayAction::QuitApp => {
                        self.state.allow_immediate_quit = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        ctx.request_repaint();
                    }
                }
            }
        }

        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.state.allow_immediate_quit {
            match (self.state.settings.close_behavior, self.tray.is_some()) {
                (CloseBehavior::MinimizeToTray, true) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    self.state.trim_launcher_memory(frame.wgpu_render_state());
                }
                (CloseBehavior::Ask, true) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.state.show_close_confirm_modal = true;
                }
                _ => {
                    self.perform_graceful_shutdown();
                }
            }
        } else if close_requested {
            self.perform_graceful_shutdown();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        views::render_root_ui(&mut self.state, ui, frame.wgpu_render_state());
        #[cfg(not(target_os = "android"))]
        views::render_detached_viewports(&mut self.state, ui.ctx(), frame.wgpu_render_state());
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.state.is_mobile_game_viewport_transparent() {
            [0.0, 0.0, 0.0, 0.0]
        } else if visuals.dark_mode {
            theme::BG_VOID_DARK.to_normalized_gamma_f32()
        } else {
            theme::BG_SKY_LIGHT.to_normalized_gamma_f32()
        }
    }
}

pub(crate) fn prepare_rolling_log_file(log_dir: &Path, max_bytes: u64) -> Option<File> {
    std::fs::create_dir_all(log_dir).ok()?;
    let log_path = log_dir.join(LAUNCHER_LOG_FILE_NAME);
    if log_path.metadata().map(|m| m.len()).unwrap_or(0) > max_bytes {
        let prev_path = log_dir.join(LAUNCHER_PREV_LOG_FILE_NAME);
        let _ = std::fs::remove_file(&prev_path);
        let _ = std::fs::rename(&log_path, &prev_path);
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok()
}

pub(crate) fn read_log_tail_utf8(log_path: &Path, max_bytes: usize) -> std::io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = File::open(log_path)?;
    let total_len = file.metadata()?.len();
    let start = total_len.saturating_sub(max_bytes as u64);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity((total_len - start) as usize);
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    if start > 0 {
        if let Some(newline_idx) = text.find('\n') {
            return Ok(text[newline_idx + 1..].to_string());
        }
    }
    Ok(text.into_owned())
}

#[derive(Clone)]
struct CombinedLogWriter {
    file: Option<Arc<Mutex<File>>>,
}

struct CombinedLogGuard {
    file: Option<Arc<Mutex<File>>>,
}

impl Write for CombinedLogGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Some(ref shared) = self.file {
            if let Ok(mut f) = shared.lock() {
                let _ = f.write_all(buf);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        if let Some(ref shared) = self.file {
            if let Ok(mut f) = shared.lock() {
                let _ = f.flush();
            }
        }
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CombinedLogWriter {
    type Writer = CombinedLogGuard;

    fn make_writer(&'a self) -> Self::Writer {
        CombinedLogGuard {
            file: self.file.clone(),
        }
    }
}

fn install_panic_log_hook(log_dir: &Path) {
    let log_path = log_dir.join(LAUNCHER_LOG_FILE_NAME);
    PANIC_HOOK_ONCE.call_once(move || {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&log_path) {
                let ts = chrono::Utc::now().to_rfc3339();
                let _ = writeln!(f, "{ts} ERROR panic: {info}");
                let _ = f.flush();
            }
            prev(info);
        }));
    });
}

fn init_tracing(log_dir: Option<&Path>) {
    let file = log_dir
        .and_then(|dir| prepare_rolling_log_file(dir, MAX_LAUNCHER_LOG_BYTES))
        .map(|f| Arc::new(Mutex::new(f)));
    if let Some(dir) = log_dir {
        install_panic_log_hook(dir);
    }
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("info,wgpu_core=warn,wgpu_hal=warn,wgpu_hal::gles=error")
    });
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(CombinedLogWriter { file })
        .try_init();
}

pub fn run_desktop_app() -> Result<(), Box<dyn std::error::Error>> {
    let paths = AppPaths::resolve_default()?;
    init_tracing(Some(&paths.root));
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        data_root = %paths.root.display(),
        "Starting TriHienKun Laucher"
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("thk-tokio-worker")
        .build()?;
    let rt_handle = runtime.handle().clone();

    let _ = theme::write_icon_assets_if_missing(&paths.root.join("icons"));
    if Path::new("assets").is_dir() {
        let _ = theme::write_icon_assets_if_missing(Path::new("assets/icons"));
    }

    let db = Arc::new(Database::open(&paths.db_file)?);

    let viewport = egui::ViewportBuilder::default()
        .with_title("TriHienKun Laucher")
        .with_app_id(thk_core::APP_ID)
        .with_inner_size([1240.0, 780.0])
        .with_min_inner_size([400.0, 520.0])
        .with_icon(Arc::new(theme::build_window_icon_data()));

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "TriHienKun Laucher",
        native_options,
        Box::new(move |cc| match TriHienKunApp::new(cc, rt_handle, paths, db) {
            Ok(app) => Ok(Box::new(app)),
            Err(error) => {
                tracing::error!(%error, "Launcher initialization failed");
                dialogs::show_startup_error_dialog(
                    "TriHienKun Laucher — Startup error",
                    &format!(
                        "Không thể khởi tạo launcher / Could not initialize launcher.\n\
                         Dữ liệu cài đặt không bị thay bằng mặc định / Settings were not reset.\n\n{error}"
                    ),
                );
                Err(Box::new(error))
            }
        }),
    )
    .map_err(|e| format!("eframe runtime error: {e}"))?;

    Ok(())
}

#[cfg(target_os = "android")]
#[allow(unsafe_code)]
#[no_mangle]
pub fn android_main(app: winit::platform::android::activity::AndroidApp) {
    if let Err(err) = run_android_app(app) {
        tracing::error!("Android launcher exited with error: {err}");
        eprintln!("Android launcher exited with error: {err}");
    }
}

#[cfg(target_os = "android")]
fn run_android_app(
    app: winit::platform::android::activity::AndroidApp,
) -> Result<(), Box<dyn std::error::Error>> {
    let data_root = app
        .internal_data_path()
        .or_else(|| app.external_data_path())
        .ok_or("Android application data directory is unavailable")?;
    thk_core::set_platform_data_root(data_root);

    let paths = AppPaths::resolve_default()?;
    init_tracing(Some(&paths.root));
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        data_root = %paths.root.display(),
        "Starting TriHienKun Laucher on Android"
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("thk-tokio-worker")
        .build()?;
    let rt_handle = runtime.handle().clone();

    let _ = theme::write_icon_assets_if_missing(&paths.root.join("icons"));

    let db = Arc::new(Database::open(&paths.db_file)?);
    if db.get_kv("app_settings")?.is_none() {
        let initial = thk_core::models::AppSettings {
            font_scale: 0.78,
            discord_rpc_enabled: false,
            ..Default::default()
        };
        db.save_settings(&initial)?;
    }

    let persistence_dir = paths.root.join("eframe_state");
    let _ = std::fs::create_dir_all(&persistence_dir);
    let persistence_file = persistence_dir.join("app.ron");

    let viewport = egui::ViewportBuilder::default()
        .with_title("TriHienKun Laucher")
        .with_app_id(thk_core::APP_ID)
        .with_transparent(true);

    let mut create_new = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create_new.instance_descriptor.backends = egui_wgpu::wgpu::Backends::GL;
    let wgpu_options = egui_wgpu::WgpuConfiguration {
        wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(create_new),
        ..Default::default()
    };

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options,
        persistence_path: Some(persistence_file),
        android_app: Some(app),
        ..Default::default()
    };

    eframe::run_native(
        "TriHienKun Laucher",
        native_options,
        Box::new(
            move |cc| match TriHienKunApp::new(cc, rt_handle, paths, db) {
                Ok(app) => Ok(Box::new(app)),
                Err(error) => {
                    tracing::error!(%error, "Android launcher initialization failed");
                    dialogs::show_startup_error_dialog(
                        "TriHienKun Laucher — Startup error",
                        &error.to_string(),
                    );
                    Err(Box::new(error))
                }
            },
        ),
    )
    .map_err(|e| format!("eframe runtime error: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::audio::{synthesize_ambient_loop, synthesize_sfx_samples, SoundEffect};
    use crate::fx3d::{build_icon_3d_scene, Icon3dKind, Poly3dBuffer};
    use crate::theme::{build_window_icon_data, generate_brand_icon_image};
    use crate::updater::{extract_sha256_for_asset, is_newer_semver};

    #[test]
    fn corrupt_settings_abort_initialization_without_overwriting_stored_data() {
        let root = std::env::temp_dir().join(format!("thk-startup-{}", uuid::Uuid::new_v4()));
        let paths = thk_core::AppPaths::from_root(root.clone()).unwrap();
        let db = std::sync::Arc::new(thk_core::Database::open_in_memory().unwrap());
        let corrupt = "{invalid-settings";
        db.set_kv("app_settings", Some(corrupt)).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = crate::state::LauncherAppState::initialize(
            runtime.handle().clone(),
            egui::Context::default(),
            paths,
            std::sync::Arc::clone(&db),
            None,
        );
        assert!(matches!(result, Err(thk_core::ThkError::Json(_))));
        assert_eq!(db.get_kv("app_settings").unwrap().as_deref(), Some(corrupt));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn updater_semver_and_sha256sums_parsing() {
        assert!(is_newer_semver("1.0.1", "1.0.0"));
        assert!(is_newer_semver("1.2.0", "1.1.9"));
        assert!(!is_newer_semver("1.0.0", "1.0.0"));
        assert!(!is_newer_semver("0.9.9", "1.0.0"));

        let sums = "\
# Release checksums
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 *trihienkun-laucher-windows-x64.exe
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  trihienkun-laucher-linux-x64
";
        let found = extract_sha256_for_asset(sums, "trihienkun-laucher-windows-x64.exe");
        assert_eq!(
            found.as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        assert!(extract_sha256_for_asset(sums, "missing.exe").is_none());
    }

    #[test]
    #[cfg(windows)]
    fn updater_startup_and_manual_check_preserve_errors_instead_of_reporting_latest() {
        use crate::state::{AppAsyncEvent, LauncherAppState};
        use std::sync::Arc;
        use thk_core::{AppPaths, Database};

        for auto_check in [true, false] {
            let root = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
                .join("Temp/opencode")
                .join(format!("thk-update-status-{}", uuid::Uuid::new_v4()));
            let paths = AppPaths::from_root(root.clone()).unwrap();
            let db = Arc::new(Database::open_in_memory().unwrap());
            let mut settings = db.load_settings().unwrap();
            settings.github_repo = "invalid-repository-slug".into();
            settings.auto_check_updates = auto_check;
            settings.music_enabled = false;
            settings.discord_rpc_enabled = false;
            db.save_settings(&settings).unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let mut app = LauncherAppState::initialize(
                runtime.handle().clone(),
                egui::Context::default(),
                paths,
                db,
                None,
            )
            .unwrap();
            if !auto_check {
                assert!(app.updater.status_line.is_none());
                app.check_app_updates();
            }
            assert!(app.updater.checking);
            let event = runtime.block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    loop {
                        let event = app.ui_rx.recv().await.unwrap();
                        if matches!(event, AppAsyncEvent::UpdateCheckCompleted(_)) {
                            return event;
                        }
                    }
                })
                .await
                .unwrap()
            });
            assert!(
                matches!(&event, AppAsyncEvent::UpdateCheckCompleted(Err(error))
                if error.contains("Invalid GitHub repository slug"))
            );
            app.ui_tx.send(event).unwrap();
            app.drain_async_events();
            assert!(!app.updater.checking);
            assert!(app.updater.available.is_none());
            assert!(app
                .updater
                .status_line
                .as_deref()
                .unwrap()
                .contains("Invalid GitHub repository slug"));
            app.audio.shutdown();
            drop(app);
            drop(runtime);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn synthesized_audio_and_brand_icons_are_valid() {
        for sfx in [
            SoundEffect::Click,
            SoundEffect::Success,
            SoundEffect::Launch,
            SoundEffect::Error,
        ] {
            let pcm = synthesize_sfx_samples(sfx);
            assert!(!pcm.is_empty());
            assert!(pcm.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }

        let bgm = synthesize_ambient_loop();
        assert_eq!(bgm.len(), 2_540_160 * 2);
        let mut peak = 0.0_f32;
        let mut stereo_diff = 0.0_f32;
        for &[l, r] in bgm.as_chunks::<2>().0 {
            assert!(l.is_finite() && r.is_finite());
            peak = peak.max(l.abs()).max(r.abs());
            stereo_diff = stereo_diff.max((l - r).abs());
        }
        assert!((0.68..=0.76).contains(&peak), "unexpected bgm peak: {peak}");
        assert!(stereo_diff > 0.02, "expected true stereo separation");
        let n = bgm.len();
        assert!((bgm[n - 2] - bgm[0]).abs() < 0.05);
        assert!((bgm[n - 1] - bgm[1]).abs() < 0.05);

        let img = generate_brand_icon_image(64);
        assert_eq!(img.dimensions(), (64, 64));
        let icon_data = build_window_icon_data();
        assert_eq!(icon_data.width, 64);
        assert_eq!(icon_data.height, 64);
        assert_eq!(icon_data.rgba.len(), 64 * 64 * 4);
    }

    #[test]
    fn fx3d_procedural_icons_and_projection_are_valid() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(48.0, 48.0));
        let probe = Poly3dBuffer::new(rect, 4.4, 2.55);
        let (pt, depth) = probe.project(glam::Vec3::new(0.25, -0.5, 0.4));
        assert!(pt.x.is_finite() && pt.y.is_finite());
        assert!(depth > 0.4);

        let kinds = [
            Icon3dKind::BrandVoxelCore,
            Icon3dKind::HomePortal,
            Icon3dKind::ModpackCrate,
            Icon3dKind::ServerBladeTower,
            Icon3dKind::AccountCharacter,
            Icon3dKind::JavaReactor,
            Icon3dKind::LanRadar,
            Icon3dKind::SettingsGyro,
            Icon3dKind::LaunchPrism,
            Icon3dKind::MultiWindow,
            Icon3dKind::MobileGamepad,
            Icon3dKind::ScreenRotate,
        ];
        for kind in kinds {
            for sample_t in [0.0_f32, 0.65, 1.85, std::f32::consts::PI] {
                let buf = build_icon_3d_scene(rect, kind, sample_t);
                assert!(buf.face_count() >= 2);
            }
        }

        let ctx = egui::Context::default();
        let atlas = crate::ui_assets::ui_textures(&ctx);
        assert_eq!(
            atlas
                .button(crate::ui_assets::UiButtonPlate::Emerald)
                .size(),
            [240, 64]
        );
        assert_eq!(
            atlas.icon(crate::ui_assets::UiAssetIcon::Brand).size(),
            [96, 96]
        );
    }

    #[tokio::test]
    async fn cloud_modpack_server_provisioning_persists_server_and_cleans_up_on_cancel() {
        use crate::state::{preferred_server_version_index, provision_cloud_modpack_server};
        use std::io::Write;
        use thk_core::models::{ContentKind, ProviderSource, ServerEngine};
        use thk_core::{AppPaths, Database, DownloadManager, ThkError};
        use thk_mods::{ProjectSummary, ProjectVersion, VersionFile};
        use thk_server::{is_eula_accepted, read_server_properties, LocalServerSupervisor};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_util::sync::CancellationToken;

        let quilt_ver = ProjectVersion {
            id: "v-quilt".into(),
            project_id: "pack-1".into(),
            provider: ProviderSource::Modrinth,
            name: "1.0.0 Quilt".into(),
            version_number: "1.0.0".into(),
            release_channel: "release".into(),
            published_at: None,
            game_versions: vec!["1.21.4".into()],
            loaders: vec!["quilt".into()],
            files: Vec::new(),
            dependencies: Vec::new(),
            server_pack_file_id: None,
        };
        let fabric_beta = ProjectVersion {
            id: "v-fab-beta".into(),
            release_channel: "beta".into(),
            loaders: vec!["fabric".into()],
            ..quilt_ver.clone()
        };
        let fabric_rel = ProjectVersion {
            id: "v-fab-rel".into(),
            release_channel: "release".into(),
            loaders: vec!["fabric".into()],
            ..quilt_ver.clone()
        };
        assert_eq!(
            preferred_server_version_index(&[quilt_ver, fabric_beta, fabric_rel]),
            2
        );

        let mut mrpack_cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut mrpack_cursor);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("modrinth.index.json", opts).unwrap();
            zip.write_all(
                br#"{
                    "formatVersion": 1,
                    "game": "minecraft",
                    "versionId": "1.2.0",
                    "name": "Cloud Fabric SMP",
                    "dependencies": {
                        "minecraft": "1.21.4",
                        "fabric-loader": "0.16.10"
                    },
                    "files": []
                }"#,
            )
            .unwrap();
            zip.start_file("overrides/fabric-server-launch.jar", opts)
                .unwrap();
            zip.write_all(b"fake-fabric-launcher").unwrap();
            zip.start_file("overrides/mods/core-server-mod.jar", opts)
                .unwrap();
            zip.write_all(b"universal-mod-bytes").unwrap();
            zip.start_file("overrides/options.txt", opts).unwrap();
            zip.write_all(b"gamma:1.0").unwrap();
            zip.finish().unwrap();
        }
        let mrpack_bytes = mrpack_cursor.into_inner();
        let mrpack_sha1 = thk_core::hash::sha1_bytes(&mrpack_bytes);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let body = mrpack_bytes.clone();
        let server_task = tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut req_buf = [0u8; 1024];
                let _ = sock.read(&mut req_buf).await;
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(hdr.as_bytes()).await;
                let _ = sock.write_all(&body).await;
            }
        });

        let root = std::env::temp_dir().join(format!("thk-cloud-srv-app-{}", uuid::Uuid::new_v4()));
        let paths = AppPaths::from_root(root.clone()).unwrap();
        let db = Database::open_in_memory().unwrap();
        let dl = DownloadManager::new().unwrap();
        let (sup, _rx) = LocalServerSupervisor::new(paths.clone(), dl.clone());

        let summary = ProjectSummary {
            provider: ProviderSource::Modrinth,
            project_id: "cloud-smp".into(),
            slug: "cloud-smp".into(),
            title: "Cloud Fabric SMP".into(),
            summary: "Test cloud server pack".into(),
            author: "TriHienKun".into(),
            downloads: 500,
            icon_url: None,
            categories: vec!["multiplayer".into()],
            content_kind: ContentKind::Modpack,
            updated_at: None,
        };
        let version = ProjectVersion {
            id: "ver-1".into(),
            project_id: "cloud-smp".into(),
            provider: ProviderSource::Modrinth,
            name: "1.2.0".into(),
            version_number: "1.2.0".into(),
            release_channel: "release".into(),
            published_at: None,
            game_versions: vec!["1.21.4".into()],
            loaders: vec!["fabric".into()],
            files: vec![VersionFile {
                filename: "cloud-smp-1.2.0.mrpack".into(),
                url: format!("http://{addr}/cloud-smp-1.2.0.mrpack"),
                primary: true,
                size: mrpack_bytes.len() as u64,
                sha1: Some(mrpack_sha1),
                sha512: None,
            }],
            dependencies: Vec::new(),
            server_pack_file_id: None,
        };

        let srv_dir = paths.servers_dir.join("cloud-fabric-smp");
        let srv = provision_cloud_modpack_server(
            &dl,
            &sup,
            &db,
            &paths,
            "srv-cloud-1".into(),
            "Cloud Fabric SMP Server".into(),
            srv_dir.clone(),
            false,
            25577,
            1024,
            6144,
            false,
            &summary,
            &version,
            None,
            4,
            CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap();
        server_task.await.unwrap();

        assert_eq!(srv.engine, ServerEngine::Fabric);
        assert_eq!(srv.game_version, "1.21.4");
        assert_eq!(srv.jar_file, "fabric-server-launch.jar");
        assert_eq!(srv.port, 25577);
        assert!(!is_eula_accepted(&srv_dir));
        assert!(srv_dir.join("mods/core-server-mod.jar").is_file());
        assert!(!srv_dir.join("options.txt").exists());
        let props = read_server_properties(&srv_dir).unwrap();
        assert_eq!(props.get("server-port").map(String::as_str), Some("25577"));
        assert_eq!(db.list_servers().unwrap().len(), 1);
        let leftover_cache: Vec<_> = std::fs::read_dir(&paths.cache_dir)
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert!(leftover_cache.is_empty());

        let dup_err = provision_cloud_modpack_server(
            &dl,
            &sup,
            &db,
            &paths,
            "srv-cloud-dup".into(),
            "Dup".into(),
            srv_dir.clone(),
            true,
            25578,
            1024,
            4096,
            false,
            &summary,
            &version,
            None,
            4,
            CancellationToken::new(),
            |_| {},
        )
        .await;
        assert!(dup_err.is_err());
        assert!(srv_dir.join("fabric-server-launch.jar").is_file());

        let pre_cancelled = CancellationToken::new();
        pre_cancelled.cancel();
        let cancel_dir = paths.servers_dir.join("cancelled-cloud-srv");
        let cancel_res = provision_cloud_modpack_server(
            &dl,
            &sup,
            &db,
            &paths,
            "srv-cloud-cancel".into(),
            "Cancelled".into(),
            cancel_dir.clone(),
            false,
            25579,
            1024,
            4096,
            false,
            &summary,
            &version,
            None,
            4,
            pre_cancelled,
            |_| {},
        )
        .await;
        assert!(matches!(cancel_res, Err(ThkError::Cancelled)));
        assert!(!cancel_dir.exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rolling_launcher_log_rotates_and_tails_clean_lines() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("thk-log-test-{}", uuid::Uuid::new_v4()));
        let mut first = crate::prepare_rolling_log_file(&dir, 64).unwrap();
        writeln!(
            first,
            "2026-11-08T10:00:00Z INFO first line exceeding sixty four bytes easily for rotation"
        )
        .unwrap();
        first.flush().unwrap();
        drop(first);

        let mut second = crate::prepare_rolling_log_file(&dir, 64).unwrap();
        writeln!(second, "line-one").unwrap();
        writeln!(second, "line-two-vietnamese-tiếng-việt").unwrap();
        second.flush().unwrap();
        drop(second);

        assert!(dir.join(crate::LAUNCHER_PREV_LOG_FILE_NAME).is_file());
        let log_path = dir.join(crate::LAUNCHER_LOG_FILE_NAME);
        let full = crate::read_log_tail_utf8(&log_path, 4096).unwrap();
        assert!(full.contains("line-one"));
        assert!(full.contains("line-two-vietnamese-tiếng-việt"));

        let tail = crate::read_log_tail_utf8(&log_path, 40).unwrap();
        assert!(!tail.contains("line-one"));
        assert!(tail.contains("line-two-vietnamese-tiếng-việt"));

        std::fs::remove_dir_all(dir).unwrap();
    }
}
