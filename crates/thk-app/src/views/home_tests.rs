use super::*;
use egui_kittest::{kittest::Queryable, Harness};
use std::sync::Arc;
use thk_core::{AppPaths, Database};

#[test]
fn general_form_scrolls_to_visible_save_and_persists_isolated_profile() {
    for language in [Language::Vi, Language::En] {
        let root = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("Temp/opencode")
            .join(format!("thk-general-test-{}", uuid::Uuid::new_v4()));
        let paths = AppPaths::from_root(root.clone()).unwrap();
        let db = Arc::new(Database::open_in_memory().unwrap());
        let mut settings = db.load_settings().unwrap();
        settings.language = language;
        settings.auto_check_updates = false;
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
            db.clone(),
            None,
        )
        .unwrap();
        // Synthetic profile in an isolated SQLite database; never use personal data.
        let profile = thk_core::models::GameProfile {
            id: "general-layout-test".into(),
            name: "General Layout Test".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Fabric,
            loader_version: None,
            instance_path: root.join("instances/general-layout-test"),
            is_custom_path: false,
            ram_mb: 4096,
            java_path: None,
            jvm_args: String::new(),
            auto_perf_mods: false,
            icon_url: None,
            window_width: Some(1280),
            window_height: Some(720),
            fullscreen: false,
            total_play_seconds: 0,
            session_count: 0,
            crash_count: 0,
            last_played_at: None,
            created_at: 0,
        };
        db.upsert_profile(&profile).unwrap();
        app.home.profiles = db.list_profiles().unwrap();
        app.home.selected_profile_id = Some(profile.id.clone());
        let save_label = app.i18n.t("profile.save").to_owned();
        let delete_label = app.i18n.t("profile.delete").to_owned();
        let duplicate_label = app
            .i18n
            .tr("Nhân bản Profile", "Duplicate Profile")
            .to_owned();
        for width in [300.0, 450.0, 710.0] {
            app.sync_profile_edit_draft();
            app.home.edit_draft.name = format!("  Saved at {width}  ");
            app.home.edit_draft.jvm_args = " -XX:+UseG1GC ".into();
            let mut harness = Harness::builder()
                .with_size(vec2(width, 250.0))
                .with_context_setup(move |ctx| {
                    crate::theme::install_unicode_fonts(ctx);
                    crate::theme::apply_theme(ctx, settings.theme, settings.accent_preset);
                })
                .build_ui(|ui| {
                    render_tab_general(&mut app, ui, &profile);
                    assert!(ui.min_rect().right() <= width);
                });
            harness.run_steps(3);
            harness.input_mut().events.extend([
                egui::Event::PointerMoved(pos2(width / 2.0, 125.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, -2000.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            harness.run_steps(8);
            let save = harness.get_by_label(&save_label);
            let save_rect = save.rect();
            let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 250.0));
            for label in [&save_label, &duplicate_label, &delete_label] {
                let rect = harness.get_by_label(label.as_str()).rect();
                assert!(
                    viewport.contains_rect(rect),
                    "clipped action {label}: {rect:?}"
                );
            }
            assert!(!save_rect.intersects(harness.get_by_label(&duplicate_label).rect()));
            save.click();
            harness.run_steps(3);
            drop(harness);
            let saved = db.list_profiles().unwrap().pop().unwrap();
            assert_eq!(saved.name, format!("Saved at {width}"));
            assert_eq!(saved.jvm_args, "-XX:+UseG1GC");
        }
        app.audio.shutdown();
        drop(app);
        drop(runtime);
        std::fs::remove_dir_all(root).unwrap();
    }
}
