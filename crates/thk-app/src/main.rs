#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_code)]

mod audio;
mod state;
mod theme;
mod tray;
mod updater;
mod views;

use crate::state::LauncherAppState;
use crate::tray::{TrayAction, TrayController};
use std::path::Path;
use std::sync::Arc;
use thk_core::models::CloseBehavior;
use thk_core::{AppPaths, Database};

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
    ) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::install_unicode_fonts(&cc.egui_ctx);

        let state = LauncherAppState::initialize(
            rt_handle,
            cc.egui_ctx.clone(),
            paths,
            db,
            cc.wgpu_render_state.as_ref(),
        );
        theme::apply_theme(&cc.egui_ctx, state.settings.theme);

        let tray = TrayController::try_new();
        Self {
            state,
            tray,
            shutdown_completed: false,
        }
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
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.state.drain_async_events();

        if let Some(ref tray) = self.tray {
            if let Some(action) = tray.poll_action() {
                match action {
                    TrayAction::RestoreWindow => {
                        tray::restore_and_focus_viewport(&ctx);
                    }
                    TrayAction::QuitApp => {
                        self.state.allow_immediate_quit = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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

        views::render_root_ui(&mut self.state, ui, frame.wgpu_render_state());
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,wgpu_core=warn,wgpu_hal=warn"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("thk-tokio-worker")
        .build()?;
    let rt_handle = runtime.handle().clone();

    let paths = AppPaths::resolve_default()?;
    let _ = theme::write_icon_assets_if_missing(&paths.root.join("icons"));
    if Path::new("assets").is_dir() {
        let _ = theme::write_icon_assets_if_missing(Path::new("assets/icons"));
    }

    let db = Arc::new(Database::open(&paths.db_file)?);

    let viewport = egui::ViewportBuilder::default()
        .with_title("TriHienKun Laucher")
        .with_app_id(thk_core::APP_ID)
        .with_inner_size([1240.0, 780.0])
        .with_min_inner_size([980.0, 620.0])
        .with_icon(Arc::new(theme::build_window_icon_data()));

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "TriHienKun Laucher",
        native_options,
        Box::new(move |cc| Ok(Box::new(TriHienKunApp::new(cc, rt_handle, paths, db)))),
    )
    .map_err(|e| format!("eframe runtime error: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::audio::{synthesize_sfx_samples, SoundEffect};
    use crate::theme::{build_window_icon_data, generate_brand_icon_image};
    use crate::updater::{extract_sha256_for_asset, is_newer_semver};

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

        let img = generate_brand_icon_image(64);
        assert_eq!(img.dimensions(), (64, 64));
        let icon_data = build_window_icon_data();
        assert_eq!(icon_data.width, 64);
        assert_eq!(icon_data.height, 64);
        assert_eq!(icon_data.rgba.len(), 64 * 64 * 4);
    }
}
