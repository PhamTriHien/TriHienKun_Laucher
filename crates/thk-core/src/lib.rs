#![deny(unsafe_code)]

pub mod archive;
pub mod db;
pub mod downloader;
pub mod error;
pub mod hash;
pub mod i18n;
pub mod mobile_controls;
pub mod models;
pub mod paths;

pub use db::Database;
pub use downloader::{DownloadManager, DownloadTask};
pub use error::{ThkError, ThkResult};
pub use i18n::I18n;
pub use paths::{
    android_bridge_dir, android_bridge_open_url, android_bridge_send_cmd,
    android_bridge_set_clipboard, android_bridge_set_orientation, clear_directory_contents,
    compute_dir_size_bytes, copy_profile_tree_selective, delete_sandboxed_secret,
    load_sandboxed_secret, platform_data_root, poll_android_focus_request,
    read_android_permission_status, scan_recovery_artifacts, set_platform_data_root,
    store_sandboxed_secret, AndroidPermissionStatus, AppPaths, RecoveryArtifact,
    RecoveryArtifactKind, SafePath, APP_DATA_FOLDER, APP_DISPLAY_NAME, APP_ID,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        sanitize_font_scale, AppSettings, CloseBehavior, GameProfile, JavaRuntimeMode, Language,
        LoaderType, DEFAULT_FONT_SCALE, MAX_FONT_SCALE, MIN_FONT_SCALE,
    };
    use std::io::Write;
    use uuid::Uuid;

    #[test]
    fn safe_path_blocks_traversal() {
        let temp = std::env::temp_dir().join(format!("thk-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp).unwrap();

        assert!(SafePath::resolve_child(&temp, "mods/sodium.jar").is_ok());
        assert!(SafePath::resolve_child(&temp, "../outside.txt").is_err());
        assert!(SafePath::resolve_child(&temp, "mods/../../outside.txt").is_err());
        assert!(SafePath::validate_single_name("world_1").is_ok());
        assert!(SafePath::validate_single_name("../world").is_err());
        assert!(SafePath::validate_single_name("a/b").is_err());

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn settings_migrate_old_updater_repo_without_resetting_preferences() {
        let db = Database::open_in_memory().unwrap();
        db.set_kv("app_settings", Some(r#"{"github_repo":"trihienkun/trihienkun-laucher","default_ram_mb":8192,"music_enabled":true}"#)).unwrap();
        let settings = db.load_settings().unwrap();
        assert_eq!(settings.github_repo, "PhamTriHien/TriHienKun_Laucher");
        assert_eq!(settings.default_ram_mb, 8192);
        assert!(settings.music_enabled);
        assert_eq!(settings.java_runtime_mode, JavaRuntimeMode::OptimalGraalVm);
        assert!((settings.font_scale - DEFAULT_FONT_SCALE).abs() < f32::EPSILON);
        assert!((sanitize_font_scale(f32::NAN) - DEFAULT_FONT_SCALE).abs() < f32::EPSILON);
        assert!((sanitize_font_scale(0.35) - MIN_FONT_SCALE).abs() < f32::EPSILON);
        assert!((sanitize_font_scale(2.50) - MAX_FONT_SCALE).abs() < f32::EPSILON);
        let saved: AppSettings =
            serde_json::from_str(&db.get_kv("app_settings").unwrap().unwrap()).unwrap();
        assert_eq!(saved.github_repo, settings.github_repo);

        let mut custom = settings;
        custom.github_repo = "owner/custom-launcher".into();
        db.save_settings(&custom).unwrap();
        assert_eq!(db.load_settings().unwrap().github_repo, custom.github_repo);

        db.set_kv("app_settings", Some("not valid json")).unwrap();
        assert!(db.load_settings().is_err());
        assert_eq!(
            db.get_kv("app_settings").unwrap().as_deref(),
            Some("not valid json")
        );
    }

    #[test]
    fn sqlite_wal_settings_and_atomic_playtime() {
        let db = Database::open_in_memory().unwrap();

        let mut settings = db.load_settings().unwrap();
        assert!(!settings.low_ram_mode);
        assert_eq!(settings.java_runtime_mode, JavaRuntimeMode::OptimalGraalVm);
        assert_eq!(
            settings.mobile_orientation,
            crate::models::MobileOrientationMode::FullSensor
        );
        settings.close_behavior = CloseBehavior::MinimizeToTray;
        settings.language = Language::En;
        settings.low_ram_mode = true;
        settings.java_runtime_mode = JavaRuntimeMode::MojangOfficial;
        settings.font_scale = 1.25;
        settings.mobile_orientation = crate::models::MobileOrientationMode::ReverseLandscape;
        db.save_settings(&settings).unwrap();

        let reloaded = db.load_settings().unwrap();
        assert_eq!(reloaded.close_behavior, CloseBehavior::MinimizeToTray);
        assert_eq!(reloaded.language, Language::En);
        assert!(reloaded.low_ram_mode);
        assert_eq!(reloaded.java_runtime_mode, JavaRuntimeMode::MojangOfficial);
        assert!((reloaded.font_scale - 1.25).abs() < f32::EPSILON);
        assert_eq!(
            reloaded.mobile_orientation,
            crate::models::MobileOrientationMode::ReverseLandscape
        );
        assert_eq!(
            reloaded.mobile_orientation.next_cycle(),
            crate::models::MobileOrientationMode::Portrait
        );

        let mut prof = GameProfile {
            id: "prof-1".into(),
            name: "Survival 1.21.4".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Fabric,
            loader_version: Some("0.16.10".into()),
            instance_path: std::path::PathBuf::from("instances/survival"),
            is_custom_path: false,
            ram_mb: 4096,
            java_path: None,
            jvm_args: "-XX:+UseG1GC".into(),
            auto_perf_mods: true,
            icon_url: None,
            window_width: Some(1600),
            window_height: Some(900),
            fullscreen: false,
            total_play_seconds: 0,
            session_count: 0,
            crash_count: 0,
            last_played_at: None,
            created_at: 1_700_000_000,
        };
        db.upsert_profile(&prof).unwrap();

        // Simulate user updating RAM while game is running, then game exits recording session
        prof.ram_mb = 8192;
        db.upsert_profile(&prof).unwrap();
        db.record_play_session("prof-1", 3600, false).unwrap();

        let fetched = db.get_profile("prof-1").unwrap().unwrap();
        assert_eq!(fetched.ram_mb, 8192);
        assert_eq!(fetched.total_play_seconds, 3600);
        assert_eq!(fetched.session_count, 1);
        assert_eq!(fetched.crash_count, 0);
    }

    #[test]
    fn sha1_and_safe_zip_roundtrip() {
        let temp = std::env::temp_dir().join(format!("thk-zip-{}", Uuid::new_v4()));
        let src_dir = temp.join("src");
        let out_dir = temp.join("out");
        std::fs::create_dir_all(src_dir.join("mods")).unwrap();

        let sample = src_dir.join("mods").join("hello.txt");
        let mut f = std::fs::File::create(&sample).unwrap();
        f.write_all(b"TriHienKun Laucher").unwrap();
        drop(f);

        let digest = hash::sha1_file(&sample).unwrap();
        hash::verify_file_sha1(&sample, &digest).unwrap();

        let zip_path = temp.join("pack.zip");
        archive::compress_paths_to_zip(&src_dir, &["mods".to_string()], &zip_path).unwrap();
        let extracted = archive::extract_zip_safe(&zip_path, &out_dir).unwrap();
        assert_eq!(extracted.len(), 1);
        assert_eq!(
            std::fs::read_to_string(out_dir.join("mods").join("hello.txt")).unwrap(),
            "TriHienKun Laucher"
        );

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn i18n_keys_synchronized() {
        let vi = I18n::new(Language::Vi);
        let en = I18n::new(Language::En);
        assert!(
            vi.mismatched_keys().is_empty(),
            "Locale key mismatch: {:?}",
            vi.mismatched_keys()
        );
        assert_eq!(vi.t("app.name"), "TriHienKun Laucher");
        assert_eq!(en.t("app.name"), "TriHienKun Laucher");
        for key in [
            "home.play",
            "settings.close_ask_title",
            "settings.close_ask_desc",
            "settings.remember_choice",
            "settings.custom_bg",
            "common.cancel",
            "common.delete",
        ] {
            assert_ne!(vi.t(key), key);
            assert_ne!(en.t(key), key);
        }
    }

    #[test]
    fn sqlite_migrates_legacy_v1_schema_and_clones_content() {
        let temp = std::env::temp_dir().join(format!("thk-mig-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp).unwrap();
        let db_file = temp.join("legacy.sqlite3");

        {
            let raw = rusqlite::Connection::open(&db_file).unwrap();
            raw.execute_batch(
                "
                CREATE TABLE profiles (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    game_version TEXT NOT NULL,
                    loader TEXT NOT NULL,
                    loader_version TEXT,
                    instance_path TEXT NOT NULL,
                    is_custom_path INTEGER NOT NULL DEFAULT 0,
                    ram_mb INTEGER NOT NULL DEFAULT 4096,
                    java_path TEXT,
                    jvm_args TEXT NOT NULL DEFAULT '',
                    auto_perf_mods INTEGER NOT NULL DEFAULT 1,
                    icon_url TEXT,
                    total_play_seconds INTEGER NOT NULL DEFAULT 0,
                    session_count INTEGER NOT NULL DEFAULT 0,
                    crash_count INTEGER NOT NULL DEFAULT 0,
                    last_played_at INTEGER,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE accounts (
                    id TEXT PRIMARY KEY,
                    username TEXT NOT NULL,
                    uuid TEXT NOT NULL,
                    account_type TEXT NOT NULL,
                    skin_variant TEXT NOT NULL DEFAULT 'wide',
                    skin_url TEXT,
                    custom_skin_path TEXT,
                    custom_cape_path TEXT,
                    custom_elytra_path TEXT,
                    token_expires_at INTEGER,
                    created_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        }

        let db = Database::open(&db_file).unwrap();
        let prof = GameProfile {
            id: "p-legacy".into(),
            name: "Legacy Migrated".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Fabric,
            loader_version: None,
            instance_path: temp.join("inst"),
            is_custom_path: false,
            ram_mb: 4096,
            java_path: None,
            jvm_args: String::new(),
            auto_perf_mods: true,
            icon_url: None,
            window_width: Some(1920),
            window_height: Some(1080),
            fullscreen: true,
            total_play_seconds: 0,
            session_count: 0,
            crash_count: 0,
            last_played_at: None,
            created_at: 1_700_000_000,
        };
        db.upsert_profile(&prof).unwrap();
        let loaded = db.get_profile("p-legacy").unwrap().unwrap();
        assert_eq!(loaded.window_width, Some(1920));
        assert_eq!(loaded.window_height, Some(1080));
        assert!(loaded.fullscreen);

        let rec = crate::models::InstalledContentRecord {
            profile_id: "p-legacy".into(),
            content_kind: crate::models::ContentKind::Mod,
            provider: crate::models::ProviderSource::Modrinth,
            project_id: "AANobbMI".into(),
            version_id: "ver-1".into(),
            title: "Sodium".into(),
            version_number: "0.6.0".into(),
            file_name: "sodium.jar".into(),
            sha1: "deadbeef".into(),
            enabled: true,
            icon_url: Some("https://cdn.modrinth.com/icon.png".into()),
            installed_at: 1_700_000_000,
        };
        db.upsert_installed_content(&rec).unwrap();
        let cloned_count = db
            .clone_installed_content_for_profile("p-legacy", "p-copy")
            .unwrap();
        assert_eq!(cloned_count, 1);
        let cloned_items = db
            .list_installed_content("p-copy", crate::models::ContentKind::Mod)
            .unwrap();
        assert_eq!(cloned_items.len(), 1);
        assert_eq!(cloned_items[0].title, "Sodium");

        drop(db);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn flatten_dotted_dir_and_cascade_triggers_and_recovery_scan() {
        let temp = std::env::temp_dir().join(format!("thk-core-extra-{}", Uuid::new_v4()));
        let dotted = temp.join("adoptium-21.0.2");
        let sibling = temp.join("adoptium-21.0.flatten_tmp");
        std::fs::create_dir_all(dotted.join("jdk-21.0.2+13").join("bin")).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("keep.txt"), b"must survive").unwrap();
        std::fs::write(
            dotted.join("jdk-21.0.2+13").join("bin").join("java.exe"),
            b"bin",
        )
        .unwrap();

        archive::flatten_single_subdir(&dotted).unwrap();
        assert!(dotted.join("bin").join("java.exe").is_file());
        assert!(sibling.join("keep.txt").is_file());

        let db = Database::open_in_memory().unwrap();
        let prof = GameProfile {
            id: "prof-cascade".into(),
            name: "Cascade Test".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Fabric,
            loader_version: None,
            instance_path: temp.join("inst"),
            is_custom_path: false,
            ram_mb: 4096,
            java_path: None,
            jvm_args: String::new(),
            auto_perf_mods: false,
            icon_url: None,
            window_width: None,
            window_height: None,
            fullscreen: false,
            total_play_seconds: 0,
            session_count: 0,
            crash_count: 0,
            last_played_at: None,
            created_at: 1_700_000_000,
        };
        let rec = crate::models::InstalledContentRecord {
            profile_id: "prof-cascade".into(),
            content_kind: crate::models::ContentKind::Mod,
            provider: crate::models::ProviderSource::Modrinth,
            project_id: "AANobbMI".into(),
            version_id: "v1".into(),
            title: "Sodium".into(),
            version_number: "0.6.0".into(),
            file_name: "Sodium-0.6.0.jar".into(),
            sha1: "abc".into(),
            enabled: true,
            icon_url: None,
            installed_at: 1_700_000_000,
        };
        db.upsert_profile_with_installed_content(&prof, std::slice::from_ref(&rec))
            .unwrap();
        db.set_installed_content_enabled(
            "prof-cascade",
            crate::models::ContentKind::Mod,
            "sodium-0.6.0.jar",
            false,
        )
        .unwrap();
        let items = db
            .list_installed_content("prof-cascade", crate::models::ContentKind::Mod)
            .unwrap();
        assert_eq!(items.len(), 1);
        assert!(!items[0].enabled);

        db.delete_profile("prof-cascade").unwrap();
        assert!(db
            .list_installed_content("prof-cascade", crate::models::ContentKind::Mod)
            .unwrap()
            .is_empty());

        let app_paths = AppPaths::from_root(temp.join("app")).unwrap();
        let pack_rec = app_paths.instances_dir.join(".thk-pack-123");
        std::fs::create_dir_all(&pack_rec).unwrap();
        std::fs::write(pack_rec.join("recovery.json"), b"{}").unwrap();
        let found = scan_recovery_artifacts(&app_paths, &[], &[], None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, RecoveryArtifactKind::ModpackStaging);

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn mobile_control_layout_roundtrips_and_sanitizes() {
        let temp = std::env::temp_dir().join(format!("thk-mobile-ctrl-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp).unwrap();

        let mut layout = crate::mobile_controls::MobileControlLayout::load_or_default(&temp);
        assert!(!layout.buttons.is_empty());
        assert_eq!(
            layout.buttons[0].shape,
            crate::mobile_controls::ControlShape::Dpad
        );
        layout.mouse_sensitivity = 9.5;
        layout.buttons[0].scale = 5.0;
        layout.buttons[0].opacity = 0.01;
        layout.save(&temp).unwrap();

        let loaded = crate::mobile_controls::MobileControlLayout::load_or_default(&temp);
        assert!((loaded.mouse_sensitivity - 4.0).abs() < f32::EPSILON);
        assert!((loaded.buttons[0].scale - 2.2).abs() < f32::EPSILON);
        assert!((loaded.buttons[0].opacity - 0.15).abs() < f32::EPSILON);

        let _ = std::fs::remove_dir_all(&temp);
    }
}
