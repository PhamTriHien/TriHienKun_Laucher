#![deny(unsafe_code)]

pub mod curseforge;
pub mod ftb;
mod install;
pub mod matcher;
pub mod modrinth;
pub mod provider;
pub mod server_pack;
pub mod spiget;
pub mod technic;

pub use curseforge::{
    build_curseforge_cdn_fallback_url, import_curseforge_zip, install_curseforge_content,
    install_curseforge_modpack_version,
};
pub use ftb::{install_ftb_modpack_version, populate_server_from_ftb_version};
pub use matcher::{
    check_local_content_updates, compute_curseforge_fingerprint, delete_local_content,
    delete_local_world, delete_server_addon_file, enrich_unmatched_via_curseforge_fingerprint,
    enrich_unmatched_via_modrinth_sha1, import_server_addon_jar, install_server_provider_addon,
    list_local_content, list_local_worlds, list_server_addon_files,
    split_filename_title_and_version, toggle_local_content, toggle_server_addon_file,
    ContentUpdateCandidate, LocalContentItem, LocalWorldInfo, ServerAddonFile,
};
pub use modrinth::{
    ensure_fabric_performance_mods, import_mrpack_archive, install_modrinth_content,
    install_modrinth_modpack_version, FABRIC_PERF_MOD_IDS,
};
pub use provider::{
    extract_canonical_version_number, fetch_provider_project, fetch_provider_versions,
    search_provider, DependencyRelation, DependencySpec, GalleryImage, ImportedPackMetadata,
    ProjectDetail, ProjectSummary, ProjectVersion, SearchPage, SearchQuery, SearchSort,
    VersionFile,
};
pub use server_pack::{
    inspect_modpack_for_server, inspect_modpack_for_server_with_hints, is_cf_file_client_only,
    is_client_only_mod_jar, is_mrpack_entry_server_compatible, loader_to_server_engine,
    populate_server_from_cloud_modpack, populate_server_from_modpack_archive,
    populate_server_from_modpack_archive_with_hints, populate_server_from_profile,
    prune_client_only_server_files, ServerModpackFormat, ServerModpackInspection,
    ServerModpackOutcome,
};
pub use spiget::install_spiget_plugin;
pub use technic::install_technic_modpack_version;

#[cfg(test)]
mod install_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use thk_core::models::ContentKind;
    use thk_core::Database;

    #[test]
    fn curseforge_cdn_url_and_filename_splitter_work() {
        let url = build_curseforge_cdn_fallback_url(5_432_109, "jei-1.21.1-fabric-19.21.0.jar");
        assert_eq!(
            url,
            "https://edge.forgecdn.net/files/5432/109/jei-1.21.1-fabric-19.21.0.jar"
        );

        let (title, ver) = split_filename_title_and_version("sodium-fabric-0.6.5+mc1.21.4.jar");
        assert_eq!(title, "sodium");
        assert_eq!(ver, "0.6.5-mc1.21.4");

        let (sh_title, sh_ver) =
            split_filename_title_and_version("ComplementaryUnbound_5.3.zip.disabled");
        assert_eq!(sh_title, "ComplementaryUnbound");
        assert_eq!(sh_ver, "5.3");

        assert_eq!(
            extract_canonical_version_number(
                "Cisco's Fantasy Medieval RPG [Ultimate] -V8E.zip",
                "Cisco's Fantasy Medieval RPG [Ultimate] -V8E.zip",
                &["1.19.2".to_string()],
            ),
            "V8E"
        );
        assert_eq!(
            extract_canonical_version_number(
                "Cisco's Fantasy Medieval RPG [Ultimate] (1)-V8A.zip",
                "Cisco's Fantasy Medieval RPG [Ultimate] (1)-V8A.zip",
                &["1.19.2".to_string()],
            ),
            "V8A"
        );
        assert_eq!(
            extract_canonical_version_number(
                "UFO SMP Modpack-V0.04 HotFix.zip",
                "UFO SMP-V0.04 HotFix.zip",
                &["1.19.2".to_string()],
            ),
            "V0.04-HotFix"
        );
        assert_eq!(
            extract_canonical_version_number(
                "Better MC [FORGE] 1.20.1 v32.5",
                "BMC4-1.20.1-v32.5.zip",
                &["1.20.1".to_string()],
            ),
            "v32.5"
        );
        assert_eq!(
            extract_canonical_version_number(
                "All the Mods 9",
                "All the Mods 9-0.3.2.zip",
                &["1.20.1".to_string()],
            ),
            "0.3.2"
        );
        assert_eq!(
            extract_canonical_version_number(
                "DawnCraft - Echoes of Legends 2.0.13_f.zip",
                "DawnCraft 2.0.13_f.zip",
                &["1.18.2".to_string()],
            ),
            "2.0.13_f"
        );
        assert_eq!(
            extract_canonical_version_number(
                "jei-1.20.1-forge-15.20.0.105.jar",
                "jei-1.20.1-forge-15.20.0.105.jar",
                &["1.20.1".to_string()],
            ),
            "15.20.0.105"
        );
    }

    #[test]
    fn curseforge_murmur2_technic_loader_and_modrinth_cdn_helpers_work() {
        let raw_a = b"PK\x03\x04example-mod-payload";
        let raw_b = b"P K\t\r\n\x03\x04example-mod-payload ";
        assert_eq!(
            compute_curseforge_fingerprint(raw_a),
            compute_curseforge_fingerprint(raw_b)
        );
        assert_ne!(compute_curseforge_fingerprint(raw_a), 0);

        let hit_str: technic::TechnicSearchHit =
            serde_json::from_str(r#"{"id":"735902","slug":"fabric-pack","name":"Fabric Pack"}"#)
                .unwrap();
        assert_eq!(hit_str.id, Some(735902));

        let hit_num: technic::TechnicSearchHit =
            serde_json::from_str(r#"{"id":42,"slug":"tekkit","name":"Tekkit"}"#).unwrap();
        assert_eq!(hit_num.id, Some(42));

        let fabric_vj = r#"{
            "id": "1.20.1-fabric-loader-0.16.9",
            "libraries": [{"name": "net.fabricmc:fabric-loader:0.16.9"}]
        }"#;
        assert_eq!(
            technic::detect_loader_from_version_json_str(fabric_vj),
            Some((
                thk_core::models::LoaderType::Fabric,
                Some("0.16.9".to_string())
            ))
        );

        assert_eq!(
            modrinth::parse_modrinth_cdn_ids(
                "https://cdn.modrinth.com/data/AANobbMI/versions/v12345/sodium-fabric-0.6.5.jar"
            ),
            Some(("AANobbMI".to_string(), "v12345".to_string()))
        );
    }

    #[test]
    fn local_content_toggle_and_safe_delete_work() {
        let db = Database::open_in_memory().unwrap();
        let tmp = std::env::temp_dir().join(format!("thk-mods-{}", uuid::Uuid::new_v4()));
        let mods_dir = tmp.join("mods");
        std::fs::create_dir_all(&mods_dir).unwrap();
        std::fs::write(mods_dir.join("lithium-0.14.0.jar"), b"fakejar").unwrap();

        let items = list_local_content(&db, "p1", &tmp, ContentKind::Mod).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].enabled);

        let disabled = toggle_local_content(
            &db,
            "p1",
            &tmp,
            ContentKind::Mod,
            "lithium-0.14.0.jar",
            false,
        )
        .unwrap();
        assert_eq!(disabled, "lithium-0.14.0.jar.disabled");
        assert!(mods_dir.join("lithium-0.14.0.jar.disabled").is_file());

        // Ensure path traversal attempt is rejected
        assert!(delete_local_content(&db, "p1", &tmp, ContentKind::Mod, "../outside.jar").is_err());

        delete_local_content(&db, "p1", &tmp, ContentKind::Mod, &disabled).unwrap();
        assert!(list_local_content(&db, "p1", &tmp, ContentKind::Mod)
            .unwrap()
            .is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn server_addon_jar_list_toggle_import_and_delete_work() {
        let tmp = std::env::temp_dir().join(format!("thk-srvaddon-{}", uuid::Uuid::new_v4()));
        let ext_dir = tmp.join("ext");
        let srv_dir = tmp.join("srv");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::create_dir_all(&srv_dir).unwrap();

        let sample_jar = ext_dir.join("EssentialsX-2.20.1.jar");
        std::fs::write(&sample_jar, b"fakeplugin").unwrap();

        let imported = import_server_addon_jar(&srv_dir, "plugins", &sample_jar).unwrap();
        assert_eq!(imported, "EssentialsX-2.20.1.jar");

        let list = list_server_addon_files(&srv_dir, true, true).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].enabled);
        assert_eq!(list[0].folder, "plugins");

        let disabled =
            toggle_server_addon_file(&srv_dir, "plugins", "EssentialsX-2.20.1.jar", false).unwrap();
        assert_eq!(disabled, "EssentialsX-2.20.1.jar.disabled");
        assert!(delete_server_addon_file(&srv_dir, "evil", &disabled).is_err());

        delete_server_addon_file(&srv_dir, "plugins", &disabled).unwrap();
        assert!(list_server_addon_files(&srv_dir, true, true)
            .unwrap()
            .is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn server_pack_filters_client_only_mods_and_clones_profile_cleanly() {
        use std::io::Write;
        use thk_core::models::{GameProfile, LoaderType, ServerEngine};

        assert!(is_mrpack_entry_server_compatible(
            Some("required"),
            "mods/lithium.jar"
        ));
        assert!(!is_mrpack_entry_server_compatible(
            Some("unsupported"),
            "mods/iris.jar"
        ));
        assert!(!is_mrpack_entry_server_compatible(
            None,
            "shaderpacks/bsl.zip"
        ));

        assert!(is_cf_file_client_only(&[
            "1.21.1".to_string(),
            "Fabric".to_string(),
            "Client".to_string(),
        ]));
        assert!(!is_cf_file_client_only(&[
            "1.21.1".to_string(),
            "Client".to_string(),
            "Server".to_string(),
        ]));

        let tmp = std::env::temp_dir().join(format!("thk-srvpack-{}", uuid::Uuid::new_v4()));
        let prof_dir = tmp.join("profile");
        let srv_dir = tmp.join("server");
        std::fs::create_dir_all(prof_dir.join("mods")).unwrap();
        std::fs::create_dir_all(prof_dir.join("config")).unwrap();
        std::fs::create_dir_all(prof_dir.join("shaderpacks")).unwrap();
        std::fs::write(prof_dir.join("config").join("common.toml"), b"port=25565").unwrap();
        std::fs::write(prof_dir.join("shaderpacks").join("comp.zip"), b"shader").unwrap();

        let make_mod_jar = |path: &std::path::Path, env: &str| {
            let f = std::fs::File::create(path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            zw.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
                .unwrap();
            write!(
                zw,
                r#"{{"schemaVersion":1,"id":"m","environment":"{env}"}}"#
            )
            .unwrap();
            zw.finish().unwrap();
        };
        make_mod_jar(&prof_dir.join("mods").join("sodium-client.jar"), "client");
        make_mod_jar(&prof_dir.join("mods").join("lithium-both.jar"), "*");

        assert!(is_client_only_mod_jar(
            &prof_dir.join("mods").join("sodium-client.jar")
        ));
        assert!(!is_client_only_mod_jar(
            &prof_dir.join("mods").join("lithium-both.jar")
        ));

        let profile = GameProfile {
            id: "prof-srv".into(),
            name: "Fabric Pack".into(),
            game_version: "1.21.4".into(),
            loader: LoaderType::Fabric,
            loader_version: Some("0.16.10".into()),
            instance_path: prof_dir,
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
            created_at: 0,
        };

        let (engine, copied, skipped) =
            populate_server_from_profile(&profile, &srv_dir, None).unwrap();
        assert_eq!(engine, ServerEngine::Fabric);
        assert_eq!(copied, 2);
        assert_eq!(skipped, 1);
        assert!(srv_dir.join("mods").join("lithium-both.jar").is_file());
        assert!(!srv_dir.join("mods").join("sodium-client.jar").exists());
        assert!(srv_dir.join("config").join("common.toml").is_file());
        assert!(!srv_dir.join("shaderpacks").exists());

        let mrpack_path = tmp.join("sample.mrpack");
        {
            let f = std::fs::File::create(&mrpack_path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            zw.start_file(
                "modrinth.index.json",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            write!(
                zw,
                r#"{{
                    "name": "Cobblemon Server",
                    "dependencies": {{"minecraft": "1.21.1", "fabric-loader": "0.16.9"}},
                    "files": [
                        {{"path": "mods/cobblemon.jar", "downloads": ["https://example.invalid/a.jar"], "env": {{"server": "required"}}}},
                        {{"path": "mods/iris.jar", "downloads": ["https://example.invalid/b.jar"], "env": {{"server": "unsupported"}}}}
                    ]
                }}"#
            )
            .unwrap();
            zw.finish().unwrap();
        }
        let inspected = inspect_modpack_for_server(&mrpack_path).unwrap();
        assert_eq!(inspected.name, "Cobblemon Server");
        assert_eq!(inspected.game_version, "1.21.1");
        assert_eq!(inspected.engine, ServerEngine::Fabric);
        assert_eq!(inspected.file_count, 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn cloud_modpack_server_provisioning_handles_modrinth_ftb_and_raw_hints() {
        use std::collections::HashMap;
        use std::io::Write;
        use std::sync::Arc;
        use thk_core::models::{ProviderSource, ServerEngine};
        use thk_core::DownloadManager;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let cf_with_srv: curseforge::CfFile = serde_json::from_str(
            r#"{
                "id": 6001,
                "modId": 999,
                "displayName": "Prominence II v3.1",
                "fileName": "Prominence-II-3.1.zip",
                "releaseType": 1,
                "serverPackFileId": 6002,
                "gameVersions": ["1.20.1", "Fabric"]
            }"#,
        )
        .unwrap();
        let mapped = curseforge::map_cf_file_to_version(cf_with_srv);
        assert_eq!(mapped.server_pack_file_id, Some(6002));

        let tmp = std::env::temp_dir().join(format!("thk-cloudsrv-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        let raw_srv_zip = tmp.join("srv-cfpack-temp.zip");
        {
            let f = std::fs::File::create(&raw_srv_zip).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            zw.start_file(
                "config/fancymenu_1.2.0.json",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zw.write_all(b"{}").unwrap();
            zw.finish().unwrap();
        }
        let hinted = inspect_modpack_for_server_with_hints(
            &raw_srv_zip,
            Some("ATM9 Dedicated"),
            Some("1.20.1"),
            Some(ServerEngine::NeoForge),
        )
        .unwrap();
        assert_eq!(hinted.name, "ATM9 Dedicated");
        assert_eq!(hinted.game_version, "1.20.1");
        assert_eq!(hinted.engine, ServerEngine::NeoForge);
        assert_eq!(hinted.format, ServerModpackFormat::RawServerZip);

        let build_jar_bytes = |env_str: &str| -> Vec<u8> {
            let mut cur = std::io::Cursor::new(Vec::new());
            {
                let mut zw = zip::ZipWriter::new(&mut cur);
                zw.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
                    .unwrap();
                write!(
                    zw,
                    r#"{{"schemaVersion":1,"id":"mod","environment":"{env_str}"}}"#
                )
                .unwrap();
                zw.finish().unwrap();
            }
            cur.into_inner()
        };
        let srv_jar = build_jar_bytes("*");
        let cli_jar = build_jar_bytes("client");

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());

        let mrpack_bytes = {
            let mut cur = std::io::Cursor::new(Vec::new());
            {
                let mut zw = zip::ZipWriter::new(&mut cur);
                zw.start_file(
                    "modrinth.index.json",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
                write!(
                    zw,
                    r#"{{
                        "name": "Cloud Fabric Pack",
                        "dependencies": {{"minecraft": "1.21.4", "fabric-loader": "0.16.10"}},
                        "files": [
                            {{"path": "mods/server-core.jar", "downloads": ["{base_url}/mods/server-core.jar"], "env": {{"server": "required"}}}},
                            {{"path": "mods/iris.jar", "downloads": ["{base_url}/mods/client.jar"], "env": {{"server": "unsupported"}}}}
                        ]
                    }}"#
                )
                .unwrap();
                zw.start_file(
                    "overrides/options.txt",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
                zw.write_all(b"fov:1.0").unwrap();
                zw.start_file(
                    "server-overrides/config/server.toml",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
                zw.write_all(b"max_ticks=20").unwrap();
                zw.finish().unwrap();
            }
            cur.into_inner()
        };

        let ftb_manifest_json = format!(
            r#"{{
                "name": "2.4.0",
                "targets": [
                    {{"type": "game", "name": "minecraft", "version": "1.21.1"}},
                    {{"type": "modloader", "name": "neoforge", "version": "21.1.90"}}
                ],
                "specs": {{"recommended": 6144}},
                "files": [
                    {{"path": "./mods/", "name": "ftb-chunks.jar", "url": "{base_url}/mods/server-core.jar", "serveronly": true, "clientonly": false}},
                    {{"path": "./mods/", "name": "oculus.jar", "url": "{base_url}/mods/client.jar", "serveronly": false, "clientonly": true}},
                    {{"path": "./shaderpacks/", "name": "comp.zip", "url": "{base_url}/mods/client.jar", "serveronly": false, "clientonly": false}}
                ]
            }}"#
        );

        let mut routes: HashMap<String, (String, Vec<u8>)> = HashMap::new();
        routes.insert(
            "/pack.mrpack".into(),
            ("application/octet-stream".into(), mrpack_bytes),
        );
        routes.insert(
            "/mods/server-core.jar".into(),
            ("application/java-archive".into(), srv_jar),
        );
        routes.insert(
            "/mods/client.jar".into(),
            ("application/java-archive".into(), cli_jar),
        );
        routes.insert(
            "/ftb/modpack/77/900".into(),
            ("application/json".into(), ftb_manifest_json.into_bytes()),
        );
        let routes = Arc::new(routes);
        let srv_routes = Arc::clone(&routes);

        let http_task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let map = Arc::clone(&srv_routes);
                tokio::spawn(async move {
                    let mut req_buf = [0u8; 2048];
                    let n = stream.read(&mut req_buf).await.unwrap_or(0);
                    let req_line = String::from_utf8_lossy(&req_buf[..n]);
                    let path = req_line
                        .lines()
                        .next()
                        .and_then(|l| l.split_whitespace().nth(1))
                        .unwrap_or("/");
                    if let Some((ctype, body)) = map.get(path) {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.write_all(body).await;
                    } else {
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                    }
                });
            }
        });

        let dl = DownloadManager::new().unwrap();
        let cache_dir = tmp.join("cache");
        let mr_srv_dir = tmp.join("mr-server");

        let mr_summary = ProjectSummary {
            provider: ProviderSource::Modrinth,
            project_id: "p-mr".into(),
            slug: "cloud-fabric-pack".into(),
            title: "Cloud Fabric Pack".into(),
            summary: String::new(),
            author: "Author".into(),
            downloads: 100,
            icon_url: None,
            categories: Vec::new(),
            updated_at: None,
            content_kind: ContentKind::Modpack,
        };
        let mr_version = ProjectVersion {
            id: "v-mr".into(),
            project_id: "p-mr".into(),
            provider: ProviderSource::Modrinth,
            name: "1.0.0".into(),
            version_number: "1.0.0".into(),
            release_channel: "release".into(),
            published_at: None,
            game_versions: vec!["1.21.4".into()],
            loaders: vec!["fabric".into()],
            files: vec![VersionFile {
                url: format!("{base_url}/pack.mrpack"),
                filename: "cloud-fabric-pack.mrpack".into(),
                size: 0,
                sha1: None,
                sha512: None,
                primary: true,
            }],
            dependencies: Vec::new(),
            server_pack_file_id: None,
        };

        let mr_out = populate_server_from_cloud_modpack(
            &dl,
            &mr_summary,
            &mr_version,
            &cache_dir,
            &mr_srv_dir,
            None,
            4,
            None,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(mr_out.game_version, "1.21.4");
        assert_eq!(mr_out.engine, ServerEngine::Fabric);
        assert_eq!(mr_out.installed_files, 1);
        assert!(mr_srv_dir.join("mods").join("server-core.jar").is_file());
        assert!(!mr_srv_dir.join("mods").join("iris.jar").exists());
        assert!(mr_srv_dir.join("config").join("server.toml").is_file());
        assert!(!mr_srv_dir.join("options.txt").exists());
        assert_eq!(std::fs::read_dir(&cache_dir).unwrap().count(), 0);

        let ftb_srv_dir = tmp.join("ftb-server");
        let ftb_summary = ProjectSummary {
            provider: ProviderSource::Ftb,
            project_id: "77".into(),
            slug: "77".into(),
            title: "FTB Skies".into(),
            summary: String::new(),
            author: "FTB".into(),
            downloads: 500,
            icon_url: None,
            categories: Vec::new(),
            updated_at: None,
            content_kind: ContentKind::Modpack,
        };
        let ftb_version = ProjectVersion {
            id: "900".into(),
            project_id: "77".into(),
            provider: ProviderSource::Ftb,
            name: "2.4.0".into(),
            version_number: "2.4.0".into(),
            release_channel: "release".into(),
            published_at: None,
            game_versions: vec!["1.21.1".into()],
            loaders: vec!["neoforge".into()],
            files: vec![VersionFile {
                url: format!("{base_url}/ftb/modpack/77/900"),
                filename: "ftb-77-900.json".into(),
                size: 0,
                sha1: None,
                sha512: None,
                primary: true,
            }],
            dependencies: Vec::new(),
            server_pack_file_id: None,
        };

        let ftb_out = populate_server_from_cloud_modpack(
            &dl,
            &ftb_summary,
            &ftb_version,
            &cache_dir,
            &ftb_srv_dir,
            None,
            4,
            None,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(ftb_out.game_version, "1.21.1");
        assert_eq!(ftb_out.engine, ServerEngine::NeoForge);
        assert_eq!(ftb_out.installed_files, 1);
        assert_eq!(ftb_out.skipped_client_files, 2);
        assert_eq!(ftb_out.recommended_ram_mb, Some(6144));
        assert!(ftb_srv_dir.join("mods").join("ftb-chunks.jar").is_file());
        assert!(!ftb_srv_dir.join("mods").join("oculus.jar").exists());
        assert!(!ftb_srv_dir.join("shaderpacks").exists());

        http_task.abort();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
