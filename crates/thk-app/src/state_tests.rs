use crate::state::{
    inspect_disk_pack_archive, is_safe_managed_dir_to_delete, AppAsyncEvent, DeleteTarget,
    LauncherAppState, NewServerSourceMode, OptimizationQueryKey, OptimizationSuggestionDraft,
};
use crate::updater::VerifiedUpdate;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use thk_core::models::{
    ContentKind, DownloadProgress, GameProfile, Language, LoaderType, ManagedServer,
    ProviderSource, ServerEngine,
};
use thk_core::{AppPaths, Database};
use thk_mods::{OptimizationTarget, ProjectVersion, ResolvedOptimizationSuggestion, VersionFile};
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

fn make_opt_suggestion(
    id: &str,
    title: &str,
    recommended_by_default: bool,
    version_number: Option<&str>,
) -> ResolvedOptimizationSuggestion {
    let compatible_version = version_number.map(|ver| ProjectVersion {
        id: format!("ver-{id}-{ver}"),
        project_id: id.to_string(),
        provider: ProviderSource::Modrinth,
        name: format!("{title} {ver}"),
        version_number: ver.to_string(),
        release_channel: "release".to_string(),
        published_at: None,
        game_versions: vec!["1.21.4".to_string()],
        loaders: vec!["fabric".to_string()],
        files: vec![VersionFile {
            url: format!("https://example.invalid/{id}-{ver}.jar"),
            filename: format!("{id}-{ver}.jar"),
            size: 1024,
            sha1: None,
            sha512: None,
            primary: true,
        }],
        dependencies: Vec::new(),
        server_pack_file_id: None,
    });
    ResolvedOptimizationSuggestion {
        project_id: id.to_string(),
        slug: id.to_string(),
        title: title.to_string(),
        content_kind: ContentKind::Mod,
        summary_vi: format!("Tối ưu {title}"),
        summary_en: format!("Optimize with {title}"),
        recommended_by_default,
        project_summary: None,
        compatible_version,
        error: None,
    }
}

#[test]
fn optimization_suggestion_draft_tracks_selection_compatibility_and_cache() {
    let mut draft = OptimizationSuggestionDraft::default();
    let key_1214 = OptimizationQueryKey {
        target: OptimizationTarget::Client(LoaderType::Fabric),
        game_version: "1.21.4".into(),
    };
    let key_1201 = OptimizationQueryKey {
        target: OptimizationTarget::Client(LoaderType::Fabric),
        game_version: "1.20.1".into(),
    };
    let key_forge = OptimizationQueryKey {
        target: OptimizationTarget::Client(LoaderType::Forge),
        game_version: "1.20.1".into(),
    };

    let items_1214 = vec![
        make_opt_suggestion("sodium", "Sodium", true, Some("mc1.21.4-0.6.13-fabric")),
        make_opt_suggestion("c2me", "C2ME", false, Some("0.3.0+1.21.4")),
        make_opt_suggestion("ferrite", "FerriteCore", true, Some("7.1.0-fabric")),
    ];
    draft.apply_resolved(&key_1214, Ok(items_1214));
    let selected: Vec<String> = draft
        .selected_suggestions()
        .into_iter()
        .map(|s| s.project_id)
        .collect();
    assert_eq!(selected, vec!["sodium".to_string(), "ferrite".to_string()]);

    draft.set_checked("sodium", false);
    draft.set_checked("c2me", true);
    assert!(draft.user_unselected_ids.contains("sodium"));
    assert!(draft.selected_ids.contains("c2me"));

    // Switching MC version on the same loader keeps explicit deselections and drops unavailable items.
    let items_1201 = vec![
        make_opt_suggestion("sodium", "Sodium", true, Some("mc1.20.1-0.5.8")),
        make_opt_suggestion("c2me", "C2ME", false, None),
        make_opt_suggestion("ferrite", "FerriteCore", true, None),
    ];
    draft.apply_resolved(&key_1201, Ok(items_1201));
    assert!(!draft.selected_ids.contains("sodium"));
    assert!(!draft.selected_ids.contains("c2me"));
    assert!(!draft.selected_ids.contains("ferrite"));

    // Cache hit restores 1.21.4 without spawning and still honors `user_unselected_ids` for Sodium.
    assert!(draft
        .prepare_resolution(Some(key_1214.clone()), false)
        .is_none());
    let selected_after_cache: Vec<String> = draft
        .selected_suggestions()
        .into_iter()
        .map(|s| s.project_id)
        .collect();
    assert_eq!(selected_after_cache, vec!["ferrite".to_string()]);

    draft.select_all_compatible();
    assert_eq!(draft.selected_suggestions().len(), 3);
    draft.select_recommended();
    assert_eq!(
        draft
            .selected_suggestions()
            .into_iter()
            .map(|s| s.project_id)
            .collect::<Vec<_>>(),
        vec!["sodium".to_string(), "ferrite".to_string()]
    );
    draft.select_none();
    assert!(draft.selected_suggestions().is_empty());

    // Error state preserves cache but clears displayed list.
    draft.apply_resolved(&key_1201, Err("network timeout".into()));
    assert_eq!(draft.error.as_deref(), Some("network timeout"));
    assert!(draft.suggestions.is_empty());
    assert!(draft.cache.contains_key(&key_1214));

    // Target switch clears previous loader selections/unselections cleanly.
    draft.apply_resolved(
        &key_forge,
        Ok(vec![make_opt_suggestion(
            "sodium",
            "Embeddium",
            true,
            Some("0.3.31"),
        )]),
    );
    assert!(draft.user_unselected_ids.is_empty());
    assert!(draft.selected_ids.contains("sodium"));

    // Partial item errors must render in `suggestions` without poisoning `cache` so retry works.
    let key_partial = OptimizationQueryKey {
        target: OptimizationTarget::Client(LoaderType::NeoForge),
        game_version: "1.21.1".into(),
    };
    let mut partial_err = make_opt_suggestion("radium", "Radium", true, None);
    partial_err.error = Some("HTTP 503".into());
    draft.apply_resolved(
        &key_partial,
        Ok(vec![
            make_opt_suggestion("embeddium", "Embeddium", true, Some("1.0.0")),
            partial_err,
        ]),
    );
    assert!(draft.error.is_none());
    assert_eq!(draft.suggestions.len(), 2);
    assert!(!draft.cache.contains_key(&key_partial));
    assert!(draft.prepare_resolution(Some(key_forge), false).is_none());
    assert!(draft
        .prepare_resolution(Some(key_partial.clone()), false)
        .is_some());

    draft.reset_active_for_reuse();
    assert!(draft.active_key.is_none());
    assert!(draft.suggestions.is_empty());
    assert!(draft.selected_ids.is_empty());
    assert!(draft.user_unselected_ids.is_empty());
    assert!(draft.error.is_none());
    assert!(!draft.loading);
    assert!(draft.cache.contains_key(&key_1214));
}

#[test]
fn optimization_query_keys_handle_client_server_modes_and_arclight_loaders() {
    with_test_state(|app, _| {
        app.home.new_draft.loader = LoaderType::Vanilla;
        app.home.new_draft.game_version = "1.21.4".into();
        assert!(app.current_profile_optimization_key().is_none());

        app.home.new_draft.loader = LoaderType::Fabric;
        app.home.new_draft.game_version = " 1.21.4 ".into();
        assert_eq!(
            app.current_profile_optimization_key(),
            Some(OptimizationQueryKey {
                target: OptimizationTarget::Client(LoaderType::Fabric),
                game_version: "1.21.4".into(),
            })
        );

        app.home.new_draft.game_version = "   ".into();
        assert!(app.current_profile_optimization_key().is_none());

        app.servers.new_draft.source_mode = NewServerSourceMode::ModpackArchive;
        app.servers.new_draft.engine = ServerEngine::Paper;
        app.servers.new_draft.game_version = "1.21.4".into();
        assert!(app.current_server_optimization_key().is_none());

        app.servers.new_draft.source_mode = NewServerSourceMode::StandardEngine;
        app.servers.new_draft.engine = ServerEngine::Vanilla;
        assert!(app.current_server_optimization_key().is_none());

        app.servers.new_draft.engine = ServerEngine::Paper;
        assert_eq!(
            app.current_server_optimization_key(),
            Some(OptimizationQueryKey {
                target: OptimizationTarget::Server {
                    engine: ServerEngine::Paper,
                    mod_loader: None,
                },
                game_version: "1.21.4".into(),
            })
        );

        app.servers.new_draft.engine = ServerEngine::Arclight;
        app.servers.new_draft.game_version = "1.21.4".into();
        assert_eq!(
            app.current_server_optimization_key(),
            Some(OptimizationQueryKey {
                target: OptimizationTarget::Server {
                    engine: ServerEngine::Arclight,
                    mod_loader: Some(LoaderType::Fabric),
                },
                game_version: "1.21.4".into(),
            })
        );

        app.servers.new_draft.game_version = "1.21.1".into();
        assert_eq!(
            app.current_server_optimization_key(),
            Some(OptimizationQueryKey {
                target: OptimizationTarget::Server {
                    engine: ServerEngine::Arclight,
                    mod_loader: Some(LoaderType::NeoForge),
                },
                game_version: "1.21.1".into(),
            })
        );

        app.servers.new_draft.game_version = "1.20.1".into();
        assert_eq!(
            app.current_server_optimization_key(),
            Some(OptimizationQueryKey {
                target: OptimizationTarget::Server {
                    engine: ServerEngine::Arclight,
                    mod_loader: Some(LoaderType::Forge),
                },
                game_version: "1.20.1".into(),
            })
        );

        app.servers.new_draft.game_version.clear();
        assert!(app.current_server_optimization_key().is_none());
    });
}

#[test]
fn drain_async_events_discards_stale_optimization_seq_and_updates_auto_perf_mods() {
    with_test_state(|app, _| {
        let prof_key = OptimizationQueryKey {
            target: OptimizationTarget::Client(LoaderType::Fabric),
            game_version: "1.21.4".into(),
        };
        app.home.new_draft.name = "Opt Client".into();
        app.home.new_draft.loader = LoaderType::Fabric;
        app.home.new_draft.game_version = "1.21.4".into();
        app.home.new_draft.auto_perf_mods = true;
        app.home.new_draft.optimizations.active_key = Some(prof_key.clone());
        app.home.new_draft.optimizations.loading = true;
        app.home.new_draft.optimizations.request_seq = 2;

        // Stale sequence must be discarded cleanly.
        app.ui_tx
            .send(AppAsyncEvent::OptimizationSuggestionsLoaded {
                is_server: false,
                key: prof_key.clone(),
                seq: 1,
                result: Ok(vec![make_opt_suggestion(
                    "stale",
                    "Stale",
                    true,
                    Some("0.0.1"),
                )]),
            })
            .unwrap();
        app.drain_async_events();
        assert!(app.home.new_draft.optimizations.loading);
        assert!(app.home.new_draft.optimizations.suggestions.is_empty());

        // Matching sequence resolves suggestions and auto-selects recommended items.
        app.ui_tx
            .send(AppAsyncEvent::OptimizationSuggestionsLoaded {
                is_server: false,
                key: prof_key.clone(),
                seq: 2,
                result: Ok(vec![make_opt_suggestion(
                    "lithium",
                    "Lithium",
                    true,
                    Some("mc1.21.4-0.14.8-fabric"),
                )]),
            })
            .unwrap();
        app.drain_async_events();
        assert!(!app.home.new_draft.optimizations.loading);
        assert_eq!(
            app.home
                .new_draft
                .optimizations
                .selected_suggestions()
                .len(),
            1
        );

        // User unchecks all suggestions before profile creation -> `auto_perf_mods` must be false.
        app.home.new_draft.optimizations.select_none();
        app.create_profile_from_draft();
        let created = app
            .home
            .profiles
            .iter()
            .find(|p| p.name == "Opt Client")
            .expect("created profile must exist");
        assert!(!created.auto_perf_mods);
        assert!(app.home.new_draft.optimizations.active_key.is_none());
        assert!(app
            .home
            .new_draft
            .optimizations
            .cache
            .contains_key(&prof_key));

        // When suggestions are resolved AND 1+ items remain selected, `auto_perf_mods` must STILL
        // be false so launch-time `ensure_fabric_performance_mods` never overrides user choices.
        app.home.new_draft.name = "Opt Client Selected".into();
        app.home.new_draft.loader = LoaderType::Fabric;
        app.home.new_draft.game_version = "1.21.4".into();
        app.home.new_draft.auto_perf_mods = true;
        assert!(app
            .home
            .new_draft
            .optimizations
            .prepare_resolution(Some(prof_key.clone()), false)
            .is_none());
        assert_eq!(
            app.home
                .new_draft
                .optimizations
                .selected_suggestions()
                .len(),
            1
        );
        app.create_profile_from_draft();
        let created_with_sel = app
            .home
            .profiles
            .iter()
            .find(|p| p.name == "Opt Client Selected")
            .expect("created profile with selected opts must exist");
        assert!(!created_with_sel.auto_perf_mods);

        // When suggestions have NOT been resolved (`!has_resolved_opts`), preserve `new_draft.auto_perf_mods`.
        app.home.new_draft.name = "Offline Fabric Default".into();
        app.home.new_draft.loader = LoaderType::Fabric;
        app.home.new_draft.game_version = "1.21.4".into();
        app.home.new_draft.auto_perf_mods = true;
        app.home.new_draft.optimizations.reset_active_for_reuse();
        app.create_profile_from_draft();
        let created_unresolved = app
            .home
            .profiles
            .iter()
            .find(|p| p.name == "Offline Fabric Default")
            .expect("created unresolved profile must exist");
        assert!(created_unresolved.auto_perf_mods);

        app.home.new_draft.name = "Offline Fabric Disabled".into();
        app.home.new_draft.auto_perf_mods = false;
        app.create_profile_from_draft();
        let created_disabled = app
            .home
            .profiles
            .iter()
            .find(|p| p.name == "Offline Fabric Disabled")
            .expect("created disabled profile must exist");
        assert!(!created_disabled.auto_perf_mods);

        // Verify server optimization channel discards stale seq and applies active seq.
        let srv_key = OptimizationQueryKey {
            target: OptimizationTarget::Server {
                engine: ServerEngine::Paper,
                mod_loader: None,
            },
            game_version: "1.21.4".into(),
        };
        app.servers.new_draft.optimizations.active_key = Some(srv_key.clone());
        app.servers.new_draft.optimizations.loading = true;
        app.servers.new_draft.optimizations.request_seq = 5;
        app.ui_tx
            .send(AppAsyncEvent::OptimizationSuggestionsLoaded {
                is_server: true,
                key: srv_key.clone(),
                seq: 4,
                result: Ok(vec![make_opt_suggestion("old", "Old", true, Some("1.0"))]),
            })
            .unwrap();
        app.ui_tx
            .send(AppAsyncEvent::OptimizationSuggestionsLoaded {
                is_server: true,
                key: srv_key,
                seq: 5,
                result: Ok(vec![make_opt_suggestion(
                    "spark",
                    "spark",
                    true,
                    Some("1.10.119"),
                )]),
            })
            .unwrap();
        app.drain_async_events();
        assert!(!app.servers.new_draft.optimizations.loading);
        assert_eq!(app.servers.new_draft.optimizations.suggestions.len(), 1);
        assert_eq!(
            app.servers.new_draft.optimizations.suggestions[0].project_id,
            "spark"
        );
    });
}

#[test]
fn arclight_server_creation_chains_optimizations_after_jar_and_skips_on_jar_failure() {
    use crate::state::ToastKind;

    with_test_state(|app, _root| {
        let arclight_key = OptimizationQueryKey {
            target: OptimizationTarget::Server {
                engine: ServerEngine::Arclight,
                mod_loader: Some(LoaderType::NeoForge),
            },
            game_version: "1.21.1".into(),
        };
        app.servers.new_draft.source_mode = NewServerSourceMode::StandardEngine;
        app.servers.new_draft.name = "Arclight Immediate".into();
        app.servers.new_draft.engine = ServerEngine::Arclight;
        app.servers.new_draft.game_version = "1.21.1".into();
        app.servers.new_draft.install_immediately = true;
        app.servers.new_draft.optimizations.apply_resolved(
            &arclight_key,
            Ok(vec![make_opt_suggestion(
                "ferrite",
                "FerriteCore",
                true,
                Some("7.0.0"),
            )]),
        );
        app.create_server_from_draft();
        app.drain_async_events();

        let srv_arc_now = app
            .servers
            .servers
            .iter()
            .find(|s| s.name == "Arclight Immediate")
            .cloned()
            .expect("Arclight Immediate server");
        assert!(app.busy_message.is_some());
        assert!(
            !app.active_tasks
                .contains_key(&format!("server-opt-install:{}", srv_arc_now.id)),
            "Arclight optimization install must wait until install_server_engine writes .thk-arclight-loader"
        );
        app.busy_message = None;

        // With `install_immediately = false`, Arclight optimizations still start right away.
        app.servers.new_draft.name = "Arclight Deferred".into();
        app.servers.new_draft.install_immediately = false;
        app.servers.new_draft.optimizations.apply_resolved(
            &arclight_key,
            Ok(vec![make_opt_suggestion(
                "ferrite",
                "FerriteCore",
                true,
                Some("7.0.0"),
            )]),
        );
        app.create_server_from_draft();
        app.drain_async_events();

        let srv_arc_later = app
            .servers
            .servers
            .iter()
            .find(|s| s.name == "Arclight Deferred")
            .cloned()
            .expect("Arclight Deferred server");
        assert!(app
            .active_tasks
            .contains_key(&format!("server-opt-install:{}", srv_arc_later.id)));

        // Non-Arclight engines keep immediate parallel optimization behavior.
        let paper_key = OptimizationQueryKey {
            target: OptimizationTarget::Server {
                engine: ServerEngine::Paper,
                mod_loader: None,
            },
            game_version: "1.21.4".into(),
        };
        app.servers.new_draft.name = "Paper Immediate".into();
        app.servers.new_draft.engine = ServerEngine::Paper;
        app.servers.new_draft.game_version = "1.21.4".into();
        app.servers.new_draft.install_immediately = true;
        app.servers.new_draft.optimizations.apply_resolved(
            &paper_key,
            Ok(vec![make_opt_suggestion(
                "spark",
                "spark",
                true,
                Some("1.10.119"),
            )]),
        );
        app.create_server_from_draft();
        app.drain_async_events();

        let srv_paper = app
            .servers
            .servers
            .iter()
            .find(|s| s.name == "Paper Immediate")
            .cloned()
            .expect("Paper Immediate server");
        assert!(app.busy_message.is_some());
        assert!(app
            .active_tasks
            .contains_key(&format!("server-opt-install:{}", srv_paper.id)));
    });

    // When Java/JAR preparation fails for Arclight, optimizations must NOT run or pretend loader succeeded.
    let root = std::env::temp_dir().join(format!("thk-arclight-fail-test-{}", Uuid::new_v4()));
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
        paths.clone(),
        db,
        None,
    )
    .unwrap();

    let srv_dir = paths.servers_dir.join("arclight-fail");
    std::fs::create_dir_all(&srv_dir).unwrap();
    let srv = ManagedServer {
        id: "srv-arc-fail".into(),
        name: "Arclight Fail".into(),
        engine: ServerEngine::Arclight,
        game_version: "1.21.1".into(),
        server_dir: srv_dir.clone(),
        is_custom_dir: false,
        port: 25565,
        ram_min_mb: 1024,
        ram_max_mb: 2048,
        java_path: Some(root.join("missing-java-binary")),
        jvm_flags: String::new(),
        jar_file: String::new(),
        created_at: 0,
    };
    app.db.upsert_server(&srv).unwrap();
    app.toasts.clear();

    app.spawn_server_jar_and_optional_optimizations(
        srv.clone(),
        vec![make_opt_suggestion(
            "ferrite",
            "FerriteCore",
            true,
            Some("7.0.0"),
        )],
    );
    rt.block_on(async {
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
    });
    app.drain_async_events();

    assert!(app.busy_message.is_none());
    assert!(!srv_dir.join(".thk-arclight-loader").exists());
    assert!(!app
        .active_tasks
        .contains_key(&format!("server-opt-install:{}", srv.id)));
    assert!(app.toasts.iter().any(|t| t.kind == ToastKind::Error));
    assert!(!app.toasts.iter().any(|t| t.kind == ToastKind::Success));

    crate::fx3d::set_low_ram_mode(false);
    app.audio.shutdown();
    drop(app);
    drop(rt);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn startup_reconciles_legacy_slim_skin_variant_once_and_preserves_manual_override() {
    use image::{Rgba, RgbaImage};
    use thk_core::models::{Account, AccountType, SkinVariant};

    let root = std::env::temp_dir().join(format!("thk-skin-reconcile-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.clone()).unwrap();
    let db = Arc::new(Database::open_in_memory().unwrap());
    let mut settings = db.load_settings().unwrap();
    settings.language = Language::En;
    settings.auto_check_updates = false;
    settings.music_enabled = false;
    settings.discord_rpc_enabled = false;
    db.save_settings(&settings).unwrap();

    let mut padded_slim = RgbaImage::from_pixel(128, 128, Rgba([0, 0, 0, 0]));
    for y in 32..40 {
        for x in 88..94 {
            padded_slim.put_pixel(x, y, Rgba([255, 235, 240, 255]));
        }
        for x in 94..100 {
            padded_slim.put_pixel(x, y, Rgba([127, 123, 164, 255]));
        }
        for x in 100..104 {
            padded_slim.put_pixel(x, y, Rgba([247, 207, 183, 255]));
        }
    }
    for y in 40..64 {
        for x in 80..108 {
            padded_slim.put_pixel(x, y, Rgba([255, 248, 249, 255]));
        }
        for x in 108..112 {
            padded_slim.put_pixel(x, y, Rgba([247, 207, 183, 255]));
        }
    }
    let skin_path = paths.skins_dir.join("legacy-padded-slim.png");
    padded_slim.save(&skin_path).unwrap();

    let account = Account {
        id: "offline-legacy-slim".into(),
        username: "LegacySlim".into(),
        uuid: "00000000-0000-0000-0000-000000000001".into(),
        account_type: AccountType::Offline,
        skin_variant: SkinVariant::Wide,
        skin_url: None,
        cape_url: None,
        custom_skin_path: Some(skin_path),
        custom_cape_path: None,
        custom_elytra_path: None,
        token_expires_at: None,
        oauth_client_id: None,
        created_at: 0,
    };
    db.upsert_account(&account).unwrap();
    db.set_kv("selected_account_id", Some(&account.id)).unwrap();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut app1 = LauncherAppState::initialize(
        rt.handle().clone(),
        egui::Context::default(),
        paths.clone(),
        Arc::clone(&db),
        None,
    )
    .unwrap();
    assert_eq!(
        app1.selected_account().map(|a| a.skin_variant),
        Some(SkinVariant::Slim),
        "startup should reconcile legacy padded Slim skin saved as Wide"
    );
    assert_eq!(
        db.get_kv("skin_variant_reconcile_v1").unwrap().as_deref(),
        Some("1")
    );
    assert_eq!(
        db.list_accounts().unwrap()[0].skin_variant,
        SkinVariant::Slim
    );

    app1.update_selected_account_variant(SkinVariant::Wide);
    assert_eq!(
        db.list_accounts().unwrap()[0].skin_variant,
        SkinVariant::Wide
    );
    app1.audio.shutdown();
    drop(app1);

    let app2 = LauncherAppState::initialize(
        rt.handle().clone(),
        egui::Context::default(),
        paths,
        Arc::clone(&db),
        None,
    )
    .unwrap();
    assert_eq!(
        app2.selected_account().map(|a| a.skin_variant),
        Some(SkinVariant::Wide),
        "subsequent startups must preserve explicit manual variant override"
    );
    app2.audio.shutdown();
    drop(app2);
    drop(rt);
    let _ = std::fs::remove_dir_all(&root);
}
