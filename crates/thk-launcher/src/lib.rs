#![deny(unsafe_code)]

pub mod crash;
pub mod loaders;
pub mod mobile_jvm;
pub mod supervisor;
pub mod vanilla;

pub use crash::{
    analyze_crash_logs, analyze_crash_logs_with_instance_dir, auto_fix_mod_from_modrinth,
    CrashAnalysisReport, CrashCategory, CrashFixAction, MissingModAdvice,
};
pub use loaders::{
    list_loader_versions, neoforge_prefix_for_mc, prepare_loader_patch, LoaderLaunchPatch,
    LoaderVersionEntry,
};
pub use mobile_jvm::{MobileInputEvent, MobileLaunchSpec, MobileSessionHandle};
pub use supervisor::{
    InstanceKey, InstanceSupervisor, LaunchRequest, LauncherEvent, RunningInstanceSnapshot,
};
pub use vanilla::{
    evaluate_rules, fetch_version_manifest, library_dedup_key, maven_coord_to_rel_path,
    prepare_vanilla_files, prepare_vanilla_files_with_natives, resolve_vanilla_version_json,
    PreparedVanillaAssets, VanillaVersionJson, VersionManifestV2, VersionSummary,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vanilla::{current_mojang_os_name, MojangOsRule, MojangRule};
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn launch_from_ui_thread_uses_explicit_runtime_and_reports_pipeline_errors() {
        use std::sync::Arc;
        use thk_core::models::{GameProfile, LoaderType, SkinVariant};
        use thk_core::{AppPaths, Database, DownloadManager};

        let root = std::env::temp_dir().join(format!("thk-ui-launch-{}", uuid::Uuid::new_v4()));
        let paths = AppPaths::from_root(root.clone()).unwrap();
        let blocker = root.join("not-a-directory");
        std::fs::write(&blocker, b"test fixture").unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (supervisor, mut events) = InstanceSupervisor::new(
            paths,
            Arc::new(Database::open_in_memory().unwrap()),
            DownloadManager::new().unwrap(),
        );
        let profile = GameProfile {
            id: "ui-launch-test".into(),
            name: "UI launch regression".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Vanilla,
            loader_version: None,
            instance_path: blocker.join("instance"),
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
        let account = thk_auth::create_offline_account("THK_GuiTest", SkinVariant::Wide).unwrap();
        let key = InstanceKey::new(profile.id.clone(), account.id.clone());
        assert!(tokio::runtime::Handle::try_current().is_err());
        supervisor
            .launch_async(
                runtime.handle(),
                LaunchRequest {
                    profile,
                    account,
                    quick_play_server: None,
                    custom_ms_client_id: None,
                    high_process_priority: false,
                    java_runtime_mode: thk_core::models::JavaRuntimeMode::OptimalGraalVm,
                    mobile_renderer: thk_core::models::MobileRendererMode::HolyGl4es,
                    mobile_render_scale_pct: 85,
                    mobile_surface_size: None,
                },
            )
            .unwrap();
        let event = runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), events.recv())
                .await
                .unwrap()
                .unwrap()
        });
        assert!(matches!(
            event,
            LauncherEvent::LaunchFailed { key: failed, error } if failed == key && !error.is_empty()
        ));
        assert!(!supervisor.is_running(&key));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mojang_rules_evaluate_allow_and_disallow_accurately() {
        let feats = HashMap::new();
        let rules = vec![
            MojangRule {
                action: "allow".into(),
                os: None,
                features: None,
            },
            MojangRule {
                action: "disallow".into(),
                os: Some(MojangOsRule {
                    name: Some(current_mojang_os_name().into()),
                    arch: None,
                }),
                features: None,
            },
        ];
        assert!(!evaluate_rules(Some(&rules), &feats));

        let demo_rule = vec![MojangRule {
            action: "allow".into(),
            os: None,
            features: Some(HashMap::from([("is_demo_user".to_string(), true)])),
        }];
        assert!(!evaluate_rules(Some(&demo_rule), &feats));
    }

    #[test]
    fn maven_coordinate_and_dedup_key_preserve_classifiers() {
        assert_eq!(
            maven_coord_to_rel_path("org.ow2.asm:asm:9.7.1").as_deref(),
            Some("org/ow2/asm/asm/9.7.1/asm-9.7.1.jar")
        );
        assert_eq!(
            maven_coord_to_rel_path("net.minecraftforge:forge:1.20.1-47.3.0:universal@zip")
                .as_deref(),
            Some("net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-universal.zip")
        );

        let asm_old = Path::new("/root/libraries/org/ow2/asm/asm/9.3/asm-9.3.jar");
        let asm_new = Path::new("/root/libraries/org/ow2/asm/asm/9.7.1/asm-9.7.1.jar");
        let lwjgl_main = Path::new("/root/libraries/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar");
        let lwjgl_nat =
            Path::new("/root/libraries/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar");

        assert_eq!(library_dedup_key(asm_old), library_dedup_key(asm_new));
        assert_ne!(library_dedup_key(lwjgl_main), library_dedup_key(lwjgl_nat));
    }

    #[test]
    fn crash_analyzer_parses_fabric_and_oom_accurately() {
        let fabric_logs = vec![
            "[main/ERROR]: Incompatible mods found!".to_string(),
            "net.fabricmc.loader.impl.FormattedException: Some of your mods are incompatible with the game or each other!".to_string(),
            "A potential solution has been determined, this may resolve your problem:".to_string(),
            " - Install fabric-api, version 0.100.0 or later.".to_string(),
            " - Mod 'Sodium' (sodium) 0.6.0 requires any version of indium, which is missing!".to_string(),
        ];

        let report = analyze_crash_logs(&fabric_logs, 1);
        assert_eq!(report.category, CrashCategory::FabricIncompatible);
        assert_eq!(report.missing_mods.len(), 2);
        assert!(report.missing_mods.iter().any(|m| m.mod_id == "fabric-api"));
        assert!(report.missing_mods.iter().any(|m| m.mod_id == "indium"));

        let rc_logs = vec![
            "[main/ERROR]: Incompatible mods found!".to_string(),
            " - Mod 'Sodium' (sodium) 0.6.0-rc.1 requires any version of iris, which is missing!".to_string(),
            " - Mod 'Continuity' (continuity) 3.0.0-beta.5 requires version 0.16.9 or later of fabricloader (currently 0.15.11)!".to_string(),
        ];
        let rc_rep = analyze_crash_logs(&rc_logs, 1);
        assert!(rc_rep.missing_mods.iter().any(|m| m.mod_id == "iris"));
        assert!(rc_rep
            .missing_mods
            .iter()
            .any(|m| m.mod_id == "fabricloader" && m.action == CrashFixAction::UpgradeLoader));

        let oom_logs = vec!["Caused by: java.lang.OutOfMemoryError: Java heap space".to_string()];
        let oom_rep = analyze_crash_logs(&oom_logs, 1);
        assert_eq!(oom_rep.category, CrashCategory::OutOfMemory);
    }

    #[test]
    fn neoforge_prefix_handles_classic_and_year_based_minecraft_versions() {
        assert_eq!(neoforge_prefix_for_mc("1.21"), "21.0.");
        assert_eq!(neoforge_prefix_for_mc("1.21.4"), "21.4.");
        assert_eq!(neoforge_prefix_for_mc("26.1"), "26.1.0.");
        assert_eq!(neoforge_prefix_for_mc("26.1.2"), "26.1.2.");
        assert_eq!(neoforge_prefix_for_mc("26.3"), "26.3.0.");
    }

    #[test]
    fn custom_resolution_and_fullscreen_arguments_wire_accurately() {
        use crate::supervisor::build_full_jvm_and_game_args;
        use crate::vanilla::MojangArguments;
        use serde_json::json;
        use std::path::PathBuf;
        use thk_core::models::{Account, AccountType, GameProfile, LoaderType, SkinVariant};

        let profile = GameProfile {
            id: "p1".into(),
            name: "1080p".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Fabric,
            loader_version: None,
            instance_path: PathBuf::from("inst"),
            is_custom_path: false,
            ram_mb: 4096,
            java_path: None,
            jvm_args: String::new(),
            auto_perf_mods: false,
            icon_url: None,
            window_width: Some(1920),
            window_height: Some(1080),
            fullscreen: true,
            total_play_seconds: 0,
            session_count: 0,
            crash_count: 0,
            last_played_at: None,
            created_at: 0,
        };
        let account = Account {
            id: "a1".into(),
            username: "TriHienKun".into(),
            uuid: "b50ad385-829d-3141-a216-7e7d7539ba7f".into(),
            account_type: AccountType::Offline,
            skin_variant: SkinVariant::Wide,
            skin_url: None,
            cape_url: None,
            custom_skin_path: None,
            custom_cape_path: None,
            custom_elytra_path: None,
            oauth_client_id: None,
            token_expires_at: None,
            created_at: 0,
        };
        let vanilla = PreparedVanillaAssets {
            version_json: VanillaVersionJson {
                id: "1.21.4".into(),
                main_class: "net.minecraft.client.main.Main".into(),
                minecraft_arguments: None,
                arguments: Some(MojangArguments {
                    jvm: vec![json!("-cp"), json!("${classpath}")],
                    game: vec![json!({
                        "rules": [{ "action": "allow", "features": { "has_custom_resolution": true } }],
                        "value": ["--width", "${resolution_width}", "--height", "${resolution_height}"]
                    })],
                }),
                libraries: vec![],
                downloads: None,
                asset_index: None,
                assets: None,
                java_version: None,
            },
            client_jar: PathBuf::from("client.jar"),
            libraries: vec![],
            natives_dir: PathBuf::from("natives"),
            assets_dir: PathBuf::from("assets"),
            virtual_assets_dir: Some(PathBuf::from("assets/virtual/legacy")),
            map_to_resources: false,
            asset_index_id: "19".into(),
        };

        let loader_jvm = vec!["-cp".to_string(), "${classpath}".to_string()];
        let authlib_flags = vec![
            "-Dauthlibinjector.yggdrasil.prefetched=eyJtZXRhIjp7fX0=".to_string(),
            "-javaagent:authlib-injector.jar=http://127.0.0.1:52117".to_string(),
        ];
        let args = build_full_jvm_and_game_args(
            &profile,
            &account,
            "0",
            &vanilla,
            "net.minecraft.client.main.Main",
            true,
            &loader_jvm,
            &[],
            None,
            &authlib_flags,
            None,
        );
        assert_eq!(args.iter().filter(|a| *a == "-cp").count(), 1);
        let pref_pos = args
            .iter()
            .position(|a| a.starts_with("-Dauthlibinjector.yggdrasil.prefetched="))
            .unwrap();
        let agent_pos = args
            .iter()
            .position(|a| a.starts_with("-javaagent:authlib-injector.jar="))
            .unwrap();
        let cp_pos = args.iter().position(|a| a == "-cp").unwrap();
        let main_pos = args
            .iter()
            .position(|a| a == "net.minecraft.client.main.Main")
            .unwrap();
        assert!(pref_pos < agent_pos);
        assert!(agent_pos < cp_pos);
        assert!(cp_pos < main_pos);
        let w_pos = args.iter().position(|a| a == "--width").unwrap();
        assert_eq!(args[w_pos + 1], "1920");
        let h_pos = args.iter().position(|a| a == "--height").unwrap();
        assert_eq!(args[h_pos + 1], "1080");
        assert!(args.iter().any(|a| a == "--fullscreen"));
        assert_eq!(args.iter().filter(|a| *a == "--width").count(), 1);
    }

    #[test]
    fn mobile_jvm_spec_builder_handles_renderers_lwjgl_and_library_upgrades() {
        use crate::mobile_jvm::{
            build_mobile_launch_spec, detect_lwjgl_bundle_version, MobileSpecInputs,
        };
        use crate::vanilla::{android_upgraded_library_spec, MojangArguments, MojangLibrary};
        use serde_json::json;
        use std::path::PathBuf;
        use thk_core::models::{
            Account, AccountType, GameProfile, LoaderType, MobileRendererMode, SkinVariant,
        };
        use thk_core::AppPaths;

        assert_eq!(
            android_upgraded_library_spec("net.java.dev.jna:jna:5.12.1").map(|t| t.0),
            Some("net.java.dev.jna:jna:5.13.0")
        );
        assert!(android_upgraded_library_spec("net.java.dev.jna:jna:5.14.0").is_none());
        assert_eq!(
            android_upgraded_library_spec("com.github.oshi:oshi-core:6.2.2").map(|t| t.0),
            Some("com.github.oshi:oshi-core:6.3.0")
        );
        assert_eq!(
            android_upgraded_library_spec("org.ow2.asm:asm-all:4.1").map(|t| t.0),
            Some("org.ow2.asm:asm-all:5.0.4")
        );

        let root = std::env::temp_dir().join(format!("thk-mobile-jvm-{}", uuid::Uuid::new_v4()));
        let paths = AppPaths::from_root(root.clone()).unwrap();
        let inst_dir = root.join("instance");
        std::fs::create_dir_all(&inst_dir).unwrap();
        let runtime_root = root.join("mobile_runtime");
        std::fs::create_dir_all(runtime_root.join("lwjgl3/3.4.1")).unwrap();
        std::fs::write(runtime_root.join("lwjgl3/3.4.1/lwjgl.jar"), b"").unwrap();
        std::fs::write(
            runtime_root.join("lwjgl3/3.4.1/lwjgl-3.4.1-merged-modules.jar"),
            b"",
        )
        .unwrap();
        std::fs::create_dir_all(runtime_root.join("methods_injector_agent")).unwrap();
        std::fs::write(
            runtime_root.join("methods_injector_agent/methods_injector_agent.jar"),
            b"",
        )
        .unwrap();

        let profile = GameProfile {
            id: "mob-1".into(),
            name: "Mobile 26.1".into(),
            game_version: "26.1".into(),
            loader: LoaderType::Vanilla,
            loader_version: None,
            instance_path: inst_dir.clone(),
            is_custom_path: true,
            ram_mb: 3072,
            java_path: None,
            jvm_args: String::new(),
            auto_perf_mods: false,
            icon_url: None,
            window_width: None,
            window_height: None,
            fullscreen: true,
            total_play_seconds: 0,
            session_count: 0,
            crash_count: 0,
            last_played_at: None,
            created_at: 0,
        };
        let account = Account {
            id: "a1".into(),
            username: "TriHienKun".into(),
            uuid: "b50ad385-829d-3141-a216-7e7d7539ba7f".into(),
            account_type: AccountType::Offline,
            skin_variant: SkinVariant::Wide,
            skin_url: None,
            cape_url: None,
            custom_skin_path: None,
            custom_cape_path: None,
            custom_elytra_path: None,
            oauth_client_id: None,
            token_expires_at: None,
            created_at: 0,
        };
        let vanilla = PreparedVanillaAssets {
            version_json: VanillaVersionJson {
                id: "26.1".into(),
                main_class: "net.minecraft.client.main.Main".into(),
                minecraft_arguments: None,
                arguments: Some(MojangArguments {
                    jvm: vec![json!("-cp"), json!("${classpath}")],
                    game: vec![],
                }),
                libraries: vec![MojangLibrary {
                    name: "org.lwjgl:lwjgl:3.4.1".into(),
                    downloads: None,
                    url: None,
                    rules: None,
                    natives: None,
                }],
                downloads: None,
                asset_index: None,
                assets: None,
                java_version: None,
            },
            client_jar: root.join("client.jar"),
            libraries: vec![
                PathBuf::from("/libs/org/lwjgl/lwjgl/3.4.1/lwjgl-3.4.1.jar"),
                PathBuf::from("/libs/com/mojang/brigadier/1.0/brigadier-1.0.jar"),
            ],
            natives_dir: root.join("natives"),
            assets_dir: root.join("assets"),
            virtual_assets_dir: None,
            map_to_resources: false,
            asset_index_id: "26".into(),
        };

        assert_eq!(detect_lwjgl_bundle_version(&vanilla), ("3.4.1", false));
        let java_bin = root.join("jre21/bin/java");
        let log_file = root.join("mobile_game.log");
        let spec = build_mobile_launch_spec(MobileSpecInputs {
            paths: &paths,
            profile: &profile,
            account: &account,
            access_token: "0",
            vanilla: &vanilla,
            java_bin: &java_bin,
            java_major: 21,
            main_class: "net.minecraft.client.main.Main",
            include_client_jar: true,
            loader_jvm_args: &[],
            loader_game_args: &[],
            legacy_mc_args_override: None,
            authlib_jvm_args: &[],
            quick_play_server: None,
            renderer: MobileRendererMode::MobileGlues,
            render_scale_pct: 80,
            surface_size: (2400, 1080),
            input_port: 49152,
            runtime_root: &runtime_root,
            log_file: &log_file,
        })
        .unwrap();

        assert_eq!(spec.lwjgl_version, "3.4.1");
        assert_eq!(spec.window_width, 1920);
        assert_eq!(spec.window_height, 864);
        assert_eq!(spec.renderer_lib, "libmobileglues.so");
        assert_eq!(
            spec.env.get("POJAVEXEC_EGL").map(String::as_str),
            Some("libmobileglues.so")
        );
        assert_eq!(spec.argv[0], "java");
        assert!(!spec.argv.iter().any(|a| a == "--fullscreen"));
        assert_eq!(
            MobileInputEvent::Key {
                keycode: 87,
                down: true
            }
            .to_wire_line(),
            "K 87 1\n"
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
