#![deny(unsafe_code)]

pub mod config_files;
pub mod cpu;
pub mod ftp_deploy;
pub mod jars;
pub mod manager;
pub mod telemetry;

pub use config_files::{
    add_banned_ip, add_banned_player, apply_properties_patch_str, compress_server_entries,
    delete_player_data, delete_server_entries, extract_server_zip, import_external_file_streaming,
    is_eula_accepted, list_runnable_server_targets, list_server_dir, offline_player_uuid,
    parse_properties_str, read_banned_ips, read_banned_players, read_known_players, read_ops,
    read_server_properties, read_server_text_file, read_whitelist, remove_banned_ip,
    remove_banned_player, remove_op_player, remove_whitelist_player,
    resolve_player_uuid_for_server, upsert_op_player, upsert_whitelist_player, write_eula_accepted,
    write_server_properties_patch, write_server_text_file, BannedIpEntry, BannedPlayerEntry,
    KnownPlayerEntry, OpEntry, ServerFileEntry, WhitelistEntry,
};
pub use cpu::{
    compute_process_cpu_sample, parse_cpu_list_count, parse_proc_status_allowed_cores,
    read_process_allowed_cores, summarize_cpu_brands, CpuHardwareInfo,
};
pub use ftp_deploy::{
    collect_deployable_server_files, delete_server_ftp_password, deploy_server_via_ftp,
    load_server_ftp_password, normalize_remote_ftp_dir, save_server_ftp_password,
    test_ftp_connection, FtpDeployConfig, FtpDeploySummary,
};
pub use jars::{detect_arclight_loader, install_server_engine, list_supported_versions_for_engine};
pub use manager::{
    build_server_jvm_and_target_args, resolve_effective_server_port, sanitize_console_command,
    strip_console_ansi_and_noise, LocalServerSupervisor, ServerResourceStats, ServerRuntimeState,
    ServerSupervisorEvent,
};
pub use telemetry::{
    extract_player_join_or_leave, fallback_tps_probe_command, is_automated_tps_probe_noise,
    is_unknown_command_line, parse_tps_or_mspt_line, supports_vanilla_tick_query,
    tps_probe_command_for_server,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use uuid::Uuid;

    #[test]
    fn server_properties_patch_preserves_comments_and_updates_keys() {
        let original = "#Minecraft server properties\nserver-port=25565\nonline-mode=true\n";
        let mut patch = BTreeMap::new();
        patch.insert("online-mode".into(), "false".into());
        patch.insert("max-players".into(), "50\ninjected=bad".into());

        let updated = apply_properties_patch_str(original, &patch);
        assert!(updated.contains("#Minecraft server properties"));
        let parsed = parse_properties_str(&updated);
        assert_eq!(parsed.get("server-port").map(String::as_str), Some("25565"));
        assert_eq!(parsed.get("online-mode").map(String::as_str), Some("false"));
        assert_eq!(
            parsed.get("max-players").map(String::as_str),
            Some("50injected=bad")
        );
        assert!(!parsed.contains_key("injected"));
    }

    #[test]
    fn console_command_sanitizer_strips_newlines_and_control_bytes() {
        let malicious = "say hello\r\nop attacker\x07";
        assert_eq!(sanitize_console_command(malicious), "say helloop attacker");
        assert_eq!(sanitize_console_command(" say hi\u{7f}\u{85}\t "), "say hi");
        assert_eq!(sanitize_console_command(" /help "), "help");
        assert_eq!(sanitize_console_command("/op TriHienKun"), "op TriHienKun");
        assert_eq!(sanitize_console_command("//wand"), "//wand");
    }

    #[test]
    fn server_jvm_args_disable_headless_jline_and_strip_ansi_noise() {
        use thk_core::models::{ManagedServer, ServerEngine};

        let tmp = std::env::temp_dir().join(format!("thk-srv-jvm-{}", Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("libraries/net/minecraftforge/forge/1.19.2")).unwrap();
        std::fs::write(
            tmp.join("libraries/net/minecraftforge/forge/1.19.2/win_args.txt"),
            "-p libraries\n",
        )
        .unwrap();
        std::fs::write(tmp.join("server.properties"), "server-port=25577\n").unwrap();

        let srv = ManagedServer {
            id: "forge-srv".into(),
            name: "Forge Test".into(),
            engine: ServerEngine::Forge,
            game_version: "1.19.2".into(),
            server_dir: tmp.clone(),
            is_custom_dir: false,
            port: 25565,
            ram_min_mb: 1024,
            ram_max_mb: 4096,
            java_path: None,
            jvm_flags: String::new(),
            jar_file: "@libraries/net/minecraftforge/forge/1.19.2/win_args.txt".into(),
            created_at: 0,
        };
        assert_eq!(resolve_effective_server_port(&srv), 25577);

        let args = build_server_jvm_and_target_args(&srv).unwrap();
        assert!(args.iter().any(|a| a == "-Dterminal.jline=false"));
        assert!(args.iter().any(|a| a == "-Dterminal.ansi=false"));
        assert!(args
            .iter()
            .any(|a| a == "-Djline.terminal=jline.UnsupportedTerminal"));
        assert_eq!(args.last().map(String::as_str), Some("nogui"));

        let raw_ansi = "\0\x1b[32;1m[02:18:10] [Server thread/INFO]: Done (38.41s)! For help, type \"help\"\x1b[m\r";
        assert_eq!(
            strip_console_ansi_and_noise(raw_ansi),
            "[02:18:10] [Server thread/INFO]: Done (38.41s)! For help, type \"help\""
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn start_rejects_port_already_bound_before_spawning_duplicate_jvm() {
        use thk_core::models::{ManagedServer, ServerEngine};
        use thk_core::{AppPaths, DownloadManager, ThkError};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bound_port = listener.local_addr().unwrap().port();

        let root = std::env::temp_dir().join(format!("thk-port-busy-{}", Uuid::new_v4()));
        let paths = AppPaths::from_root(root.clone()).unwrap();
        let server_dir = paths.servers_dir.join("busy-test");
        std::fs::create_dir_all(&server_dir).unwrap();
        write_eula_accepted(&server_dir).unwrap();
        std::fs::write(server_dir.join("server.jar"), b"stub").unwrap();

        let server = ManagedServer {
            id: "busy-test".into(),
            name: "Port Collision Regression".into(),
            engine: ServerEngine::Vanilla,
            game_version: "1.21.4".into(),
            server_dir,
            is_custom_dir: false,
            port: bound_port,
            ram_min_mb: 1024,
            ram_max_mb: 2048,
            java_path: Some(root.join("missing-java")),
            jvm_flags: String::new(),
            jar_file: "server.jar".into(),
            created_at: 0,
        };

        let (supervisor, _events) =
            LocalServerSupervisor::new(paths, DownloadManager::new().unwrap());
        let res = supervisor.start_server(&server, false).await;
        assert!(
            matches!(res, Err(ThkError::Process(ref msg)) if msg.contains(&bound_port.to_string())),
            "expected port-in-use error, got {res:?}"
        );
        drop(listener);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn start_rejects_unaccepted_eula_before_resolving_java_or_spawning_server() {
        use thk_core::models::{ManagedServer, ServerEngine};
        use thk_core::{AppPaths, DownloadManager, ThkError};

        let root = std::env::temp_dir().join(format!("thk-eula-{}", Uuid::new_v4()));
        let paths = AppPaths::from_root(root.clone()).unwrap();
        let server_dir = paths.servers_dir.join("eula-test");
        let server = ManagedServer {
            id: "eula-test".into(),
            name: "EULA regression".into(),
            engine: ServerEngine::Vanilla,
            game_version: "26.1".into(),
            server_dir: server_dir.clone(),
            is_custom_dir: false,
            port: 25565,
            ram_min_mb: 1024,
            ram_max_mb: 2048,
            java_path: Some(root.join("missing-java")),
            jvm_flags: String::new(),
            jar_file: "server.jar".into(),
            created_at: 0,
        };
        let (supervisor, _events) =
            LocalServerSupervisor::new(paths, DownloadManager::new().unwrap());
        let result = supervisor.start_server(&server, false).await;
        assert!(
            matches!(result, Err(ThkError::InvalidInput(ref message)) if message.contains("EULA"))
        );
        assert_eq!(
            supervisor.runtime_state(&server.id),
            ServerRuntimeState::Offline
        );
        assert!(!is_eula_accepted(&server_dir));
        assert!(!server_dir.join("eula.txt").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_manager_blocks_traversal_and_edits_safely() {
        let tmp = std::env::temp_dir().join(format!("thk-srv-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        write_eula_accepted(&tmp).unwrap();
        assert!(is_eula_accepted(&tmp));

        write_server_text_file(
            &tmp,
            "config/paper-global.yml",
            "timings:\n  enabled: true\n",
        )
        .unwrap();
        let read_back = read_server_text_file(&tmp, "config/paper-global.yml").unwrap();
        assert!(read_back.contains("enabled: true"));

        assert!(read_server_text_file(&tmp, "../../Windows/win.ini").is_err());

        let wl = upsert_whitelist_player(&tmp, "TriHienKun", "").unwrap();
        assert_eq!(wl.len(), 1);
        assert_ne!(wl[0].uuid, "00000000-0000-0000-0000-000000000000");
        assert_eq!(wl[0].uuid, offline_player_uuid("TriHienKun"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sponge_semantic_sort_and_arclight_loader_detection_work() {
        let mut sponge_map = BTreeMap::new();
        sponge_map.insert(
            "1.21.4-14.0.0-RC9".to_string(),
            crate::jars::SpongeVersionEntry { recommended: false },
        );
        sponge_map.insert(
            "1.21.4-14.0.0-RC10".to_string(),
            crate::jars::SpongeVersionEntry { recommended: false },
        );
        assert_eq!(
            crate::jars::pick_best_sponge_version(&sponge_map).as_deref(),
            Some("1.21.4-14.0.0-RC10")
        );

        let tmp = std::env::temp_dir().join(format!("thk-arc-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join(".thk-arclight-loader"), "neoforge").unwrap();
        assert_eq!(
            detect_arclight_loader(&tmp, "1.21.1"),
            thk_core::models::LoaderType::NeoForge
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn ftp_deploy_collects_filtered_files_and_streams_to_ftp_server() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
        use tokio::net::TcpListener;

        assert_eq!(
            normalize_remote_ftp_dir("  \\mc\\srv/ ").unwrap(),
            "/mc/srv"
        );
        assert!(normalize_remote_ftp_dir("/mc/../etc").is_err());

        let tmp = std::env::temp_dir().join(format!("thk-ftp-{}", Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("mods")).unwrap();
        std::fs::create_dir_all(tmp.join("logs")).unwrap();
        std::fs::create_dir_all(tmp.join("survival_world")).unwrap();
        std::fs::write(
            tmp.join("server.properties"),
            b"level-name=survival_world\nserver-port=25565\n",
        )
        .unwrap();
        std::fs::write(tmp.join("mods").join("fabric-api.jar"), b"mod-bytes-123").unwrap();
        std::fs::write(tmp.join("logs").join("latest.log"), b"ignore").unwrap();
        std::fs::write(tmp.join("survival_world").join("level.dat"), b"worlddata").unwrap();
        std::fs::write(tmp.join("partial.jar.part"), b"ignore").unwrap();

        let without_world = collect_deployable_server_files(&tmp, false).unwrap();
        let rels: Vec<&str> = without_world.iter().map(|(_, r, _)| r.as_str()).collect();
        assert_eq!(rels, vec!["mods/fabric-api.jar", "server.properties"]);

        let with_world = collect_deployable_server_files(&tmp, true).unwrap();
        assert_eq!(with_world.len(), 3);

        let stored: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));
        let stored_srv = Arc::clone(&stored);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_task = tokio::spawn(async move {
            for _ in 0..2 {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let stored_conn = Arc::clone(&stored_srv);
                tokio::spawn(async move {
                    let (rd, mut wr) = stream.into_split();
                    let mut lines = BufReader::new(rd).lines();
                    let _ = wr.write_all(b"220 Mock FTP Ready\r\n").await;
                    let mut cwd = "/".to_string();
                    let mut dirs: std::collections::HashSet<String> =
                        ["/".to_string()].into_iter().collect();
                    let mut pending_data: Option<TcpListener> = None;

                    while let Ok(Some(line)) = lines.next_line().await {
                        let trimmed = line.trim_end_matches(['\r', '\n']);
                        let (cmd, arg) = match trimmed.split_once(' ') {
                            Some((c, a)) => (c.to_ascii_uppercase(), a.trim()),
                            None => (trimmed.to_ascii_uppercase(), ""),
                        };
                        match cmd.as_str() {
                            "USER" => {
                                let _ = wr.write_all(b"331 Password required\r\n").await;
                            }
                            "PASS" => {
                                let _ = wr.write_all(b"230 Logged in\r\n").await;
                            }
                            "TYPE" => {
                                let _ = wr.write_all(b"200 Type set\r\n").await;
                            }
                            "PWD" => {
                                let msg = format!("257 \"{cwd}\" is current directory\r\n");
                                let _ = wr.write_all(msg.as_bytes()).await;
                            }
                            "CWD" => {
                                let next = if arg.starts_with('/') {
                                    arg.to_string()
                                } else if cwd == "/" {
                                    format!("/{arg}")
                                } else {
                                    format!("{cwd}/{arg}")
                                };
                                if dirs.contains(&next) {
                                    cwd = next;
                                    let _ = wr.write_all(b"250 CWD ok\r\n").await;
                                } else {
                                    let _ = wr.write_all(b"550 No such directory\r\n").await;
                                }
                            }
                            "MKD" => {
                                let created = if arg.starts_with('/') {
                                    arg.to_string()
                                } else if cwd == "/" {
                                    format!("/{arg}")
                                } else {
                                    format!("{cwd}/{arg}")
                                };
                                dirs.insert(created.clone());
                                let msg = format!("257 \"{created}\" created\r\n");
                                let _ = wr.write_all(msg.as_bytes()).await;
                            }
                            "PASV" => {
                                let dl = TcpListener::bind("127.0.0.1:0").await.unwrap();
                                let dp = dl.local_addr().unwrap().port();
                                let p1 = dp / 256;
                                let p2 = dp % 256;
                                pending_data = Some(dl);
                                let msg =
                                    format!("227 Entering Passive Mode (127,0,0,1,{p1},{p2}).\r\n");
                                let _ = wr.write_all(msg.as_bytes()).await;
                            }
                            "STOR" => {
                                if let Some(dl) = pending_data.take() {
                                    let _ = wr.write_all(b"150 Opening binary data\r\n").await;
                                    let (mut ds, _) = dl.accept().await.unwrap();
                                    let mut payload = Vec::new();
                                    ds.read_to_end(&mut payload).await.unwrap();
                                    let full = if cwd == "/" {
                                        format!("/{arg}")
                                    } else {
                                        format!("{cwd}/{arg}")
                                    };
                                    stored_conn.lock().unwrap().insert(full, payload);
                                    let _ = wr.write_all(b"226 Transfer complete\r\n").await;
                                }
                            }
                            "QUIT" => {
                                let _ = wr.write_all(b"221 Bye\r\n").await;
                                break;
                            }
                            _ => {
                                let _ = wr.write_all(b"200 OK\r\n").await;
                            }
                        }
                    }
                });
            }
        });

        let cfg = FtpDeployConfig {
            host: "127.0.0.1".into(),
            port,
            username: "paneluser".into(),
            remote_dir: "/srv/modpack".into(),
            use_ftps: false,
            include_world: false,
        };

        let pwd = test_ftp_connection(&cfg, "secret123").await.unwrap();
        assert_eq!(pwd, "/srv/modpack");

        let summary = deploy_server_via_ftp(&tmp, &cfg, "secret123", None, |_| {})
            .await
            .unwrap();
        assert_eq!(summary.uploaded_files, 2);
        assert!(summary.uploaded_bytes > 20);

        {
            let map = stored.lock().unwrap();
            assert_eq!(
                map.get("/srv/modpack/mods/fabric-api.jar")
                    .map(Vec::as_slice),
                Some(b"mod-bytes-123".as_slice())
            );
            assert!(map.contains_key("/srv/modpack/server.properties"));
            assert!(!map.contains_key("/srv/modpack/logs/latest.log"));
        }

        server_task.abort();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cpu_telemetry_computes_exact_percentage_and_used_cores_without_proc_stat() {
        assert_eq!(compute_process_cpu_sample(0, 1000.0, 8), (0.0, 0.0));
        assert_eq!(compute_process_cpu_sample(1000, 1000.0, 8), (12.5, 1.0));
        assert_eq!(compute_process_cpu_sample(3000, 1000.0, 8), (37.5, 3.0));
        assert_eq!(compute_process_cpu_sample(12000, 1000.0, 8), (100.0, 8.0));
        assert_eq!(compute_process_cpu_sample(500, 0.0, 8), (0.0, 0.0));
        assert_eq!(compute_process_cpu_sample(500, f64::NAN, 8), (0.0, 0.0));
        assert_eq!(compute_process_cpu_sample(500, 1000.0, 0), (0.0, 0.0));
    }

    #[test]
    fn cpu_list_and_proc_status_parsers_handle_ranges_and_invalid_inputs() {
        assert_eq!(parse_cpu_list_count("0-7"), Some(8));
        assert_eq!(parse_cpu_list_count("0-3,6-7"), Some(6));
        assert_eq!(parse_cpu_list_count(" 0 "), Some(1));
        assert_eq!(parse_cpu_list_count(""), None);
        assert_eq!(parse_cpu_list_count("7-3"), None);
        assert_eq!(parse_cpu_list_count("0-abc"), None);

        let status = "Name:\tjava\nState:\tS (sleeping)\nCpus_allowed_list:\t0-5\n";
        assert_eq!(parse_proc_status_allowed_cores(status), Some(6));
        assert_eq!(parse_proc_status_allowed_cores("Name:\tjava\n"), None);
    }

    #[test]
    fn cpu_brand_summarizer_formats_desktop_and_heterogeneous_arm_socs() {
        let mt6991_cores = [
            "Cortex-A720",
            "Cortex-A720",
            "Cortex-A720",
            "Cortex-A720",
            "Cortex-X4",
            "Cortex-X4",
            "Cortex-X4",
            "Cortex-X925",
        ];
        let (short_soc, full_soc) = summarize_cpu_brands(&mt6991_cores, Some("Mediatek MT6991"));
        assert_eq!(short_soc, "MediaTek MT6991");
        assert_eq!(
            full_soc,
            "MediaTek MT6991 (1×Cortex-X925 + 3×Cortex-X4 + 4×Cortex-A720)"
        );

        let (short_arm, full_arm) = summarize_cpu_brands(&mt6991_cores, None);
        assert_eq!(short_arm, "1×Cortex-X925 + 3×Cortex-X4 + 4×Cortex-A720");
        assert_eq!(full_arm, short_arm);

        let desktop = ["AMD Ryzen 7 7840HS w/ Radeon 780M Graphics"; 8];
        let (short_x86, full_x86) = summarize_cpu_brands(&desktop, None);
        assert_eq!(short_x86, "AMD Ryzen 7 7840HS");
        assert_eq!(full_x86, "AMD Ryzen 7 7840HS w/ Radeon 780M Graphics");

        let host = CpuHardwareInfo::host();
        assert!(host.logical_cores >= 1);
        assert!(!host.short_model.is_empty());
        assert!(!host.model_name.is_empty());
    }

    #[test]
    fn tps_and_mspt_telemetry_parses_paper_forge_and_vanilla_tick_query() {
        use thk_core::models::ServerEngine;

        assert!(supports_vanilla_tick_query("1.20.3"));
        assert!(supports_vanilla_tick_query("1.21.4"));
        assert!(supports_vanilla_tick_query("26.1.2"));
        assert!(!supports_vanilla_tick_query("1.20.2"));
        assert!(!supports_vanilla_tick_query("1.19.4"));
        assert!(!supports_vanilla_tick_query("1.8.8"));

        assert_eq!(
            tps_probe_command_for_server(ServerEngine::Paper, "1.8.8"),
            Some("tps")
        );
        assert_eq!(
            tps_probe_command_for_server(ServerEngine::Vanilla, "1.21.4"),
            Some("tick query")
        );
        assert_eq!(
            tps_probe_command_for_server(ServerEngine::Vanilla, "1.19.4"),
            None
        );
        assert_eq!(
            fallback_tps_probe_command("neoforge tps", "1.21.4"),
            Some("tick query")
        );
        assert_eq!(fallback_tps_probe_command("forge tps", "1.18.2"), None);

        let paper_line = "[12:00:00 INFO]: TPS from last 1m, 5m, 15m: *19.95, 20.0, 20.0";
        let (p_tps, p_mspt) = parse_tps_or_mspt_line(paper_line).unwrap();
        assert!((p_tps.unwrap() - 19.95).abs() < 0.01);
        assert!(p_mspt.is_none());
        assert!(is_automated_tps_probe_noise(paper_line));

        let forge_line =
            "[12:00:00] [Server thread/INFO]: Overall: Mean tick time: 4.25 ms. Mean TPS: 20.00";
        let (f_tps, f_mspt) = parse_tps_or_mspt_line(forge_line).unwrap();
        assert!((f_tps.unwrap() - 20.0).abs() < 0.01);
        assert!((f_mspt.unwrap() - 4.25).abs() < 0.01);
        assert!(is_automated_tps_probe_noise(
            "[12:00:00] [Server thread/INFO]: Dim minecraft:overworld: Mean tick time: 2.10 ms. Mean TPS: 20.00"
        ));

        let tick_normal =
            "[12:00:00] [Server thread/INFO]: Average time per tick: 2.4ms (Target: 50.0ms)";
        let (v_tps, v_mspt) = parse_tps_or_mspt_line(tick_normal).unwrap();
        assert!((v_tps.unwrap() - 20.0).abs() < 0.01);
        assert!((v_mspt.unwrap() - 2.4).abs() < 0.01);

        let tick_lag =
            "[12:00:00] [Server thread/INFO]: Average time per tick: 80.0ms (Target: 50.0ms)";
        let (lag_tps, lag_mspt) = parse_tps_or_mspt_line(tick_lag).unwrap();
        assert!((lag_tps.unwrap() - 12.5).abs() < 0.01);
        assert!((lag_mspt.unwrap() - 80.0).abs() < 0.01);

        assert!(is_unknown_command_line(
            "[12:00:00] [Server thread/INFO]: Unknown or incomplete command, see below for error"
        ));
        assert_eq!(
            extract_player_join_or_leave(
                "[12:00:05] [Server thread/INFO]: TriHienKun joined the game"
            ),
            Some(("TriHienKun".to_string(), true))
        );
        assert_eq!(
            extract_player_join_or_leave(
                "[12:05:09] [Server thread/INFO]: TriHienKun left the game"
            ),
            Some(("TriHienKun".to_string(), false))
        );
        assert_eq!(
            extract_player_join_or_leave(
                "[12:05:10] [Server thread/INFO]: <Steve> I joined the game"
            ),
            None
        );
    }

    #[test]
    fn player_ops_ip_bans_and_playerdata_management_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("thk-srv-players-{}", Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("world/playerdata")).unwrap();
        std::fs::create_dir_all(tmp.join("world/stats")).unwrap();

        let ops = upsert_op_player(&tmp, "TriHienKun", "", 4, true).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].level, 4);
        assert!(ops[0].bypasses_player_limit);
        let raw_ops = std::fs::read_to_string(tmp.join("ops.json")).unwrap();
        assert!(raw_ops.contains("\"bypassesPlayerLimit\": true"));
        let uuid = ops[0].uuid.clone();

        let ops_updated = upsert_op_player(&tmp, "TriHienKun", &uuid, 2, false).unwrap();
        assert_eq!(ops_updated.len(), 1);
        assert_eq!(ops_updated[0].level, 2);
        assert!(!ops_updated[0].bypasses_player_limit);
        assert!(remove_op_player(&tmp, "TriHienKun").unwrap().is_empty());

        let ips = add_banned_ip(&tmp, "203.0.113.42", "DDoS attempt").unwrap();
        assert_eq!(ips.len(), 1);
        assert_eq!(ips[0].ip, "203.0.113.42");
        assert!(add_banned_ip(&tmp, "not-an-ip", "bad").is_err());
        assert!(remove_banned_ip(&tmp, "203.0.113.42").unwrap().is_empty());

        std::fs::write(
            tmp.join("usercache.json"),
            format!(
                r#"[{{"name":"TriHienKun","uuid":"{uuid}","expiresOn":"2027-01-01 00:00:00 +0700"}}]"#
            ),
        )
        .unwrap();
        let dat_file = tmp.join(format!("world/playerdata/{uuid}.dat"));
        let stats_file = tmp.join(format!("world/stats/{uuid}.json"));
        std::fs::write(&dat_file, vec![0x1f, 0x8b, 0x08, 0x00, 0xaa, 0xbb]).unwrap();
        std::fs::write(&stats_file, b"{}").unwrap();

        let known = read_known_players(&tmp).unwrap();
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].name, "TriHienKun");
        assert!(known[0].has_playerdata);
        assert_eq!(known[0].playerdata_bytes, 6);

        assert!(delete_player_data(&tmp, "../outside").is_err());
        let removed_cnt = delete_player_data(&tmp, &uuid).unwrap();
        assert_eq!(removed_cnt, 2);
        assert!(!dat_file.exists());
        assert!(!stats_file.exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
