#![deny(unsafe_code)]

pub mod crash;
pub mod loaders;
pub mod supervisor;
pub mod vanilla;

pub use crash::{
    analyze_crash_logs, auto_fix_mod_from_modrinth, CrashAnalysisReport, CrashCategory,
    CrashFixAction, MissingModAdvice,
};
pub use loaders::{list_loader_versions, prepare_loader_patch, LoaderLaunchPatch, LoaderVersionEntry};
pub use supervisor::{
    InstanceKey, InstanceSupervisor, LaunchRequest, LauncherEvent, RunningInstanceSnapshot,
};
pub use vanilla::{
    evaluate_rules, fetch_version_manifest, library_dedup_key, maven_coord_to_rel_path,
    prepare_vanilla_files, resolve_vanilla_version_json, PreparedVanillaAssets, VanillaVersionJson,
    VersionManifestV2, VersionSummary,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vanilla::{current_mojang_os_name, MojangOsRule, MojangRule};
    use std::collections::HashMap;
    use std::path::Path;

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

        let oom_logs = vec!["Caused by: java.lang.OutOfMemoryError: Java heap space".to_string()];
        let oom_rep = analyze_crash_logs(&oom_logs, 1);
        assert_eq!(oom_rep.category, CrashCategory::OutOfMemory);
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
            token_expires_at: None,
            created_at: 0,
        };
        let vanilla = PreparedVanillaAssets {
            version_json: VanillaVersionJson {
                id: "1.21.4".into(),
                main_class: "net.minecraft.client.main.Main".into(),
                minecraft_arguments: None,
                arguments: Some(MojangArguments {
                    jvm: vec![],
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
            asset_index_id: "19".into(),
        };

        let args = build_full_jvm_and_game_args(
            &profile,
            &account,
            "0",
            &vanilla,
            "net.minecraft.client.main.Main",
            true,
            &[],
            &[],
            None,
            None,
            None,
        );
        let w_pos = args.iter().position(|a| a == "--width").unwrap();
        assert_eq!(args[w_pos + 1], "1920");
        let h_pos = args.iter().position(|a| a == "--height").unwrap();
        assert_eq!(args[h_pos + 1], "1080");
        assert!(args.iter().any(|a| a == "--fullscreen"));
        assert_eq!(args.iter().filter(|a| *a == "--width").count(), 1);
    }
}
