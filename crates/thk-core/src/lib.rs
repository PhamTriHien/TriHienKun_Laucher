#![deny(unsafe_code)]

pub mod archive;
pub mod db;
pub mod downloader;
pub mod error;
pub mod hash;
pub mod i18n;
pub mod models;
pub mod paths;

pub use db::Database;
pub use downloader::{DownloadManager, DownloadTask};
pub use error::{ThkError, ThkResult};
pub use i18n::I18n;
pub use paths::{AppPaths, SafePath, APP_DATA_FOLDER, APP_DISPLAY_NAME, APP_ID};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CloseBehavior, GameProfile, Language, LoaderType};
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
    fn sqlite_wal_settings_and_atomic_playtime() {
        let db = Database::open_in_memory().unwrap();

        let mut settings = db.load_settings().unwrap();
        settings.close_behavior = CloseBehavior::MinimizeToTray;
        settings.language = Language::En;
        db.save_settings(&settings).unwrap();

        let reloaded = db.load_settings().unwrap();
        assert_eq!(reloaded.close_behavior, CloseBehavior::MinimizeToTray);
        assert_eq!(reloaded.language, Language::En);

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
        assert_eq!(vi.t("app.name"), "TriHienKun Laucher");
        assert_eq!(en.t("app.name"), "TriHienKun Laucher");
        assert_ne!(vi.t("home.play"), "home.play");
        assert_ne!(en.t("home.play"), "home.play");
    }
}
