use crate::state::{
    inspect_disk_pack_archive, is_safe_managed_dir_to_delete, AppAsyncEvent, DeleteTarget,
    LauncherAppState,
};
use crate::updater::VerifiedUpdate;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use thk_core::models::{
    DownloadProgress, GameProfile, Language, LoaderType, ManagedServer, ServerEngine,
};
use thk_core::{AppPaths, Database};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn with_test_state(run: impl FnOnce(&mut LauncherAppState, PathBuf)) {
    let root = std::env::temp_dir().join(format!("thk-state-test-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.clone()).unwrap();
    let db = Arc::new(Database::open_in_memory().unwrap());
    let mut settings = db.load_settings().unwrap();
    settings.language = Language::En;
    settings.auto_check_updates = false;
    settings.music_enabled = false;
    settings.discord_rpc_enabled = false;
    db.save_settings(&settings).unwrap();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut app = LauncherAppState::initialize(
        rt.handle().clone(),
        egui::Context::default(),
        paths,
        db,
        None,
    )
    .unwrap();
    run(&mut app, root.clone());
    crate::fx3d::set_low_ram_mode(false);
    app.audio.shutdown();
    drop(app);
    drop(rt);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn inspect_disk_pack_detects_mrpack_and_curseforge_manifests() {
    let dir = std::env::temp_dir().join(format!("thk-pack-inspect-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();

    let mrpack_path = dir.join("fabulously-optimized.mrpack");
    {
        let f = std::fs::File::create(&mrpack_path).unwrap();
        let mut zw = ZipWriter::new(f);
        zw.start_file("modrinth.index.json", SimpleFileOptions::default())
            .unwrap();
        zw.write_all(
            br#"{
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": "6.2.0",
                "name": "Fabulously Optimized",
                "dependencies": {
                    "minecraft": "1.21.4",
                    "fabric-loader": "0.16.10"
                },
                "files": [{"path": "mods/sodium.jar"}, {"path": "mods/lithium.jar"}]
            }"#,
        )
        .unwrap();
        zw.finish().unwrap();
    }

    let mr_state = inspect_disk_pack_archive(&mrpack_path, 4096).unwrap();
    assert!(mr_state.is_mrpack);
    assert_eq!(mr_state.profile_name, "Fabulously Optimized");
    assert_eq!(mr_state.detected_mc_version, "1.21.4");
    assert_eq!(mr_state.detected_loader, LoaderType::Fabric);
    assert_eq!(mr_state.detected_file_count, 2);

    let cf_path = dir.join("atm10.zip");
    {
        let f = std::fs::File::create(&cf_path).unwrap();
        let mut zw = ZipWriter::new(f);
        zw.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zw.write_all(
            br#"{
                "name": "All the Mods 10",
                "minecraft": {
                    "version": "1.21.1",
                    "recommendedRam": 8192,
                    "modLoaders": [{"id": "neoforge-21.1.90", "primary": true}]
                },
                "files": [{"projectID": 1, "fileID": 2}]
            }"#,
        )
        .unwrap();
        zw.finish().unwrap();
    }

    let cf_state = inspect_disk_pack_archive(&cf_path, 4096).unwrap();
    assert!(!cf_state.is_mrpack);
    assert_eq!(cf_state.profile_name, "All the Mods 10");
    assert_eq!(cf_state.detected_mc_version, "1.21.1");
    assert_eq!(cf_state.detected_loader, LoaderType::NeoForge);
    assert_eq!(cf_state.ram_mb, 8192);
    assert_eq!(cf_state.detected_file_count, 1);

    let plain_zip = dir.join("random.zip");
    {
        let f = std::fs::File::create(&plain_zip).unwrap();
        let mut zw = ZipWriter::new(f);
        zw.start_file("readme.txt", SimpleFileOptions::default())
            .unwrap();
        zw.write_all(b"not a modpack").unwrap();
        zw.finish().unwrap();
    }
    assert!(inspect_disk_pack_archive(&plain_zip, 4096).is_err());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn safe_managed_dir_guard_blocks_roots_and_allows_child_instance() {
    let root = std::env::temp_dir().join(format!("thk-safe-del-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.clone()).unwrap();
    let child_inst = paths.instances_dir.join("survival-1214");
    std::fs::create_dir_all(&child_inst).unwrap();

    assert!(!is_safe_managed_dir_to_delete(&paths, &paths.root));
    assert!(!is_safe_managed_dir_to_delete(&paths, &paths.instances_dir));
    assert!(!is_safe_managed_dir_to_delete(&paths, &paths.servers_dir));
    assert!(!is_safe_managed_dir_to_delete(&paths, &paths.cache_dir));
    if let Some(parent) = paths.root.parent() {
        assert!(!is_safe_managed_dir_to_delete(&paths, parent));
    }
    assert!(is_safe_managed_dir_to_delete(&paths, &child_inst));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn keyed_tasks_preserve_concurrent_progress_and_support_cancellation() {
    with_test_state(|app, _root| {
        let cancel_java = CancellationToken::new();
        let prog_java = DownloadProgress {
            phase: "JRE".into(),
            message: "Adoptium 21".into(),
            completed_files: 0,
            total_files: 1,
            downloaded_bytes: 512,
            total_bytes: 1024,
            bytes_per_sec: 1000,
            percent: 50.0,
        };
        let prog_srv = DownloadProgress {
            phase: "Server".into(),
            message: "Paper 1.21.4".into(),
            completed_files: 0,
            total_files: 1,
            downloaded_bytes: 256,
            total_bytes: 1024,
            bytes_per_sec: 500,
            percent: 25.0,
        };

        app.update_keyed_task(
            "java:Adoptium:21".into(),
            Some(prog_java),
            Some(cancel_java.clone()),
            None,
        );
        app.update_keyed_task("server-install:srv1".into(), Some(prog_srv), None, None);
        assert_eq!(app.active_tasks.len(), 2);
        assert!(app.active_progress.is_some());

        assert!(app.can_cancel_primary_task());
        app.cancel_primary_active_task();
        assert!(cancel_java.is_cancelled());

        app.update_keyed_task("java:Adoptium:21".into(), None, None, None);
        assert_eq!(app.active_tasks.len(), 1);
        assert_eq!(
            app.active_progress.as_ref().map(|p| p.phase.as_str()),
            Some("Server")
        );

        app.update_keyed_task("server-install:srv1".into(), None, None, None);
        assert!(app.active_tasks.is_empty());
        assert!(app.active_progress.is_none());
    });
}

#[test]
fn server_eula_gate_and_delete_modals_require_explicit_confirmation() {
    with_test_state(|app, root| {
        let srv_dir = app.paths.servers_dir.join("test-paper");
        std::fs::create_dir_all(&srv_dir).unwrap();
        let srv = ManagedServer {
            id: "srv-1".into(),
            name: "Paper SMP".into(),
            engine: ServerEngine::Paper,
            game_version: "1.21.4".into(),
            server_dir: srv_dir.clone(),
            is_custom_dir: false,
            port: 25565,
            ram_min_mb: 1024,
            ram_max_mb: 2048,
            java_path: None,
            jvm_flags: String::new(),
            jar_file: "paper.jar".into(),
            created_at: 0,
        };
        app.db.upsert_server(&srv).unwrap();
        app.servers.servers = app.db.list_servers().unwrap();
        app.servers.selected_server_id = Some(srv.id.clone());

        app.start_selected_server();
        assert_eq!(
            app.pending_eula_server_id,
            Some(("srv-1".to_string(), false))
        );
        assert!(!srv_dir.join("eula.txt").exists());

        let custom_dir = root.join("external-pack");
        std::fs::create_dir_all(&custom_dir).unwrap();
        let custom_prof = GameProfile {
            id: "prof-custom".into(),
            name: "External Pack".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Fabric,
            loader_version: None,
            instance_path: custom_dir,
            is_custom_path: true,
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
            created_at: 0,
        };
        app.db.upsert_profile(&custom_prof).unwrap();
        app.home.profiles = app.db.list_profiles().unwrap();
        app.home.selected_profile_id = Some(custom_prof.id.clone());

        app.delete_selected_profile();
        let modal = app.pending_delete_modal.as_ref().unwrap();
        assert!(!modal.delete_files_on_disk);
        assert!(matches!(modal.target, DeleteTarget::Profile { .. }));

        let missing_update = app.paths.cache_dir.join("updates/missing.exe");
        app.updater.downloaded_file = Some(VerifiedUpdate::for_test(missing_update, "deadbeef"));
        let _ = app.ui_tx.send(AppAsyncEvent::CacheCleared(Ok(2048)));
        app.drain_async_events();
        assert!(app.updater.downloaded_file.is_none());
    });
}

#[test]
fn low_ram_mode_trim_and_modpack_server_draft_work() {
    with_test_state(|app, root| {
        let srv_dir = app.paths.servers_dir.join("srv-ftp");
        std::fs::create_dir_all(&srv_dir).unwrap();
        let srv = ManagedServer {
            id: "srv-ftp".into(),
            name: "Modded FTP SMP".into(),
            engine: ServerEngine::Fabric,
            game_version: "1.21.4".into(),
            server_dir: srv_dir,
            is_custom_dir: false,
            port: 25565,
            ram_min_mb: 2048,
            ram_max_mb: 4096,
            java_path: None,
            jvm_flags: String::new(),
            jar_file: "fabric-server-launch.jar".into(),
            created_at: 0,
        };
        app.db.upsert_server(&srv).unwrap();
        app.servers.servers = app.db.list_servers().unwrap();
        app.servers.selected_server_id = Some(srv.id.clone());

        let saved_ftp = thk_server::FtpDeployConfig {
            host: "ftp.hosting.example".into(),
            port: 2121,
            username: "mcpanel".into(),
            remote_dir: "/server".into(),
            use_ftps: true,
            include_world: true,
        };
        app.db
            .set_kv(
                "server_ftp_cfg:srv-ftp",
                Some(&serde_json::to_string(&saved_ftp).unwrap()),
            )
            .unwrap();
        app.sync_selected_server_ftp_config();
        assert_eq!(app.servers.ftp_config.host, "ftp.hosting.example");
        assert_eq!(app.servers.ftp_config.port, 2121);
        assert_eq!(app.servers.ftp_config.username, "mcpanel");
        assert_eq!(app.servers.ftp_config.remote_dir, "/server");
        assert!(app.servers.ftp_config.use_ftps);
        assert!(app.servers.ftp_config.include_world);

        let pack_path = root.join("cobblemon.mrpack");
        let _ = app.ui_tx.send(AppAsyncEvent::NewServerModpackInspected(Ok((
            pack_path.clone(),
            thk_mods::ServerModpackInspection {
                name: "Cobblemon Official".into(),
                game_version: "1.21.1".into(),
                engine: ServerEngine::Fabric,
                loader_version: Some("0.16.9".into()),
                format: thk_mods::ServerModpackFormat::ModrinthMrpack,
                file_count: 142,
                recommended_ram_mb: Some(6144),
            },
        ))));
        app.drain_async_events();
        assert_eq!(
            app.servers.new_draft.modpack_archive_path.as_deref(),
            Some(pack_path.as_path())
        );
        assert_eq!(app.servers.new_draft.engine, ServerEngine::Fabric);
        assert_eq!(app.servers.new_draft.game_version, "1.21.1");
        assert_eq!(app.servers.new_draft.ram_max_mb, 6144);

        let dummy_tex = app.egui_ctx.load_texture(
            "test-thumb",
            egui::ColorImage::filled([4, 4], egui::Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        app.modpacks.textures.insert("thumb-1".into(), dummy_tex);
        app.modpacks.texture_order.push_back("thumb-1".into());
        app.servers.server_logs.insert(
            "srv-ftp".into(),
            (0..1200).map(|i| format!("line-{i}")).collect(),
        );

        app.settings.low_ram_mode = true;
        app.persist_settings();
        assert!(crate::fx3d::is_low_ram_mode());
        assert!(app.pending_memory_trim);

        app.trim_launcher_memory(None);
        assert!(app.modpacks.textures.is_empty());
        assert_eq!(app.servers.server_logs["srv-ftp"].len(), 800);

        app.settings.low_ram_mode = false;
        app.persist_settings();
        assert!(!crate::fx3d::is_low_ram_mode());
    });
}

#[test]
fn font_scale_persistence_and_canonical_version_option_formatting_work() {
    with_test_state(|app, _root| {
        assert!((app.settings.font_scale - 1.0).abs() < f32::EPSILON);

        app.settings.font_scale = 0.925;
        app.persist_settings();
        app.egui_ctx
            .run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        assert!((app.egui_ctx.zoom_factor() - 0.925).abs() < 1e-4);
        let saved = app.db.load_settings().unwrap();
        assert!((saved.font_scale - 0.925).abs() < 1e-4);

        app.settings.font_scale = 3.50;
        app.persist_settings();
        app.egui_ctx
            .run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        assert!((app.settings.font_scale - thk_core::models::MAX_FONT_SCALE).abs() < f32::EPSILON);
        assert!((app.egui_ctx.zoom_factor() - thk_core::models::MAX_FONT_SCALE).abs() < 1e-4);

        let cisco_ver = thk_mods::ProjectVersion {
            id: "6543210".into(),
            project_id: "957753".into(),
            provider: thk_core::models::ProviderSource::CurseForge,
            name: "Cisco's Fantasy Medieval RPG [Ultimate] -V8E.zip".into(),
            version_number: thk_mods::extract_canonical_version_number(
                "Cisco's Fantasy Medieval RPG [Ultimate] -V8E.zip",
                "Cisco's Fantasy Medieval RPG [Ultimate] -V8E.zip",
                &["1.19.2".to_string()],
            ),
            game_versions: vec!["1.19.2".into()],
            loaders: vec!["forge".into()],
            release_channel: "release".into(),
            published_at: None,
            files: vec![thk_mods::VersionFile {
                filename: "Cisco's Fantasy Medieval RPG [Ultimate] -V8E.zip".into(),
                url: "https://edge.forgecdn.net/files/6543/210/pack.zip".into(),
                sha1: None,
                sha512: None,
                size: 1024,
                primary: true,
            }],
            dependencies: Vec::new(),
            server_pack_file_id: Some(6543999),
        };
        assert_eq!(
            crate::views::format_project_version_option(&cisco_ver, false),
            "V8E [Release] • forge • MC 1.19.2"
        );
        assert_eq!(
            crate::views::format_project_version_option(&cisco_ver, true),
            "V8E [Release] • forge • MC 1.19.2 • Server Pack"
        );
    });
}
