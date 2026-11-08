#![deny(unsafe_code)]

pub mod distros;
pub mod mojang_jre;
pub mod scanner;

pub use distros::{
    android_openjdk_major_for_spec, android_openjdk_package_for_major, android_openjdk_packages,
    ensure_optimal_java, fetch_available_distro_packages, install_distro_package,
    optimal_graalvm_major_for_spec, recommended_runtime_label_for_mc, DistroReleasePackage,
    JavaDistroKind, SUPPORTED_JAVA_MAJORS,
};
pub use mojang_jre::{ensure_mojang_java, infer_java_major_from_mc_version, RequiredJavaSpec};
pub use scanner::{
    android_jvm_system_property_args, build_java_ld_library_path, configure_std_java_command_env,
    configure_tokio_java_command_env, find_java_executable_in_dir, parse_java_version_output,
    probe_java_binary, resolve_java_home, scan_java_installations, JavaInstallation,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_and_modern_java_versions() {
        let v8 = "openjdk version \"1.8.0_422\"\nOpenJDK Runtime Environment (Temurin)(build 1.8.0_422-b05)";
        let (maj8, full8, vend8) = parse_java_version_output(v8).unwrap();
        assert_eq!(maj8, 8);
        assert_eq!(full8, "1.8.0_422");
        assert_eq!(vend8, "Eclipse Temurin");

        let v21 = "openjdk version \"21.0.5\" 2024-10-15 LTS\nOpenJDK Runtime Environment Zulu21.38+21-CA";
        let (maj21, full21, vend21) = parse_java_version_output(v21).unwrap();
        assert_eq!(maj21, 21);
        assert_eq!(full21, "21.0.5");
        assert_eq!(vend21, "Azul Zulu");
    }

    #[test]
    fn parses_unquoted_openjdk_versions_and_rejects_unrelated_output() {
        let output = "openjdk 21.0.2 2024-01-16\nOpenJDK Runtime Environment (build 21.0.2+13)";
        assert_eq!(
            parse_java_version_output(output),
            Some((21, "21.0.2".to_string(), "OpenJDK".to_string()))
        );
        assert!(parse_java_version_output("some tool version \"21.0.2\"").is_none());
        assert!(parse_java_version_output("openjdk Runtime Environment").is_none());
    }

    #[test]
    fn infers_java_for_all_mc_eras_including_26() {
        assert_eq!(infer_java_major_from_mc_version("1.12.2"), 8);
        assert_eq!(infer_java_major_from_mc_version("1.16.5"), 8);
        assert_eq!(infer_java_major_from_mc_version("1.17.1"), 16);
        assert_eq!(infer_java_major_from_mc_version("1.20.1"), 17);
        assert_eq!(infer_java_major_from_mc_version("1.20.5"), 21);
        assert_eq!(infer_java_major_from_mc_version("1.21.4"), 21);
        assert_eq!(infer_java_major_from_mc_version("26.1"), 25);
        assert_eq!(infer_java_major_from_mc_version("26.1-pre-1"), 25);
    }

    #[test]
    fn runtime_components_match_java_requirements_and_manifest_takes_precedence() {
        for (version, major, component) in [
            ("1.16.5", 8, "jre-legacy"),
            ("1.17", 16, "java-runtime-alpha"),
            ("1.18", 17, "java-runtime-gamma"),
            ("1.20.5", 21, "java-runtime-delta"),
            ("26.1", 25, "java-runtime-epsilon"),
        ] {
            let spec = RequiredJavaSpec::for_minecraft_version(version, None, None);
            assert_eq!(spec.major_version, major);
            assert_eq!(spec.component, component);
        }
        let spec =
            RequiredJavaSpec::for_minecraft_version("26.1", Some("custom-runtime"), Some(27));
        assert_eq!(spec.major_version, 27);
        assert_eq!(spec.component, "custom-runtime");
    }

    #[test]
    fn parses_sha256_sidecar_and_mojang_link_containment() {
        let raw = "e17b7bead097bf372a5c75df17815b0a2f30b777a019d25eff7706b21421f7fa\n";
        assert_eq!(
            distros::parse_sha256_sidecar_text(raw).as_deref(),
            Some("e17b7bead097bf372a5c75df17815b0a2f30b777a019d25eff7706b21421f7fa")
        );
        let gnu =
            "3A01D4E2EAAB87BEE5FDCCAE051FC3DC2C4DD655C04B942AA8FF7A74AAB314C4  zulu21.zip\r\n";
        assert_eq!(
            distros::parse_sha256_sidecar_text(gnu).as_deref(),
            Some("3a01d4e2eaab87bee5fdccae051fc3dc2c4dd655c04b942aa8ff7a74aab314c4")
        );
        assert!(distros::parse_sha256_sidecar_text("not_a_hash").is_none());

        let manifest_json = r#"{
            "files": {
                "jre.bundle/Contents/Home/legal/java.compiler/ADDITIONAL_LICENSE_INFO": {
                    "type": "link",
                    "target": "../java.base/ADDITIONAL_LICENSE_INFO"
                }
            }
        }"#;
        let parsed: mojang_jre::MojangFilesManifest =
            serde_json::from_str(manifest_json).expect("parse mojang manifest");
        let entry = parsed
            .files
            .get("jre.bundle/Contents/Home/legal/java.compiler/ADDITIONAL_LICENSE_INFO")
            .expect("link entry");
        assert_eq!(entry.entry_type, "link");
        assert_eq!(
            entry.target.as_deref(),
            Some("../java.base/ADDITIONAL_LICENSE_INFO")
        );

        let root = std::path::Path::new("runtimes/java-runtime-delta");
        let link_loc =
            root.join("jre.bundle/Contents/Home/legal/java.compiler/ADDITIONAL_LICENSE_INFO");
        assert!(thk_core::archive::is_safe_internal_symlink(
            root,
            &link_loc,
            std::path::Path::new("../java.base/ADDITIONAL_LICENSE_INFO")
        ));
        assert!(!thk_core::archive::is_safe_internal_symlink(
            root,
            &link_loc,
            std::path::Path::new("../../../../../../../etc/passwd")
        ));
    }

    #[tokio::test]
    async fn corrupt_distro_archive_preserves_existing_install_and_cleans_up_temp() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let body = b"not-a-valid-zip-archive";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.write_all(body).await;
            }
        });

        let tmp_root = std::env::temp_dir().join(format!("thk-java-test-{}", uuid::Uuid::new_v4()));
        let existing_dir = tmp_root.join("adoptium-21").join("bin");
        std::fs::create_dir_all(&existing_dir).unwrap();
        let marker = existing_dir.join("keep.txt");
        std::fs::write(&marker, b"intact").unwrap();

        let downloader = thk_core::DownloadManager::new().unwrap();
        let pkg = DistroReleasePackage {
            distro: JavaDistroKind::Adoptium,
            major_version: 21,
            release_name: "jdk-21.0.5+11".to_string(),
            download_url: format!("http://{addr}/corrupt.zip"),
            file_name: "corrupt.zip".to_string(),
            sha256: None,
            size_bytes: None,
        };

        let res = install_distro_package(&downloader, &tmp_root, &pkg, None, |_| {}).await;
        assert!(res.is_err());
        assert_eq!(std::fs::read(&marker).unwrap(), b"intact");

        let dl_dir = tmp_root.join(".jre_downloads");
        let leftover_files = std::fs::read_dir(&dl_dir)
            .unwrap()
            .filter_map(Result::ok)
            .count();
        assert_eq!(leftover_files, 0);

        let leftover_staging = std::fs::read_dir(&tmp_root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".thk-jre-staging-")
            })
            .count();
        assert_eq!(leftover_staging, 0);

        let _ = std::fs::remove_dir_all(&tmp_root);
    }

    #[test]
    fn maps_optimal_graalvm_majors_and_preserves_legacy_java8() {
        assert_eq!(optimal_graalvm_major_for_spec(8), None);
        assert_eq!(optimal_graalvm_major_for_spec(16), Some(17));
        assert_eq!(optimal_graalvm_major_for_spec(17), Some(17));
        assert_eq!(optimal_graalvm_major_for_spec(21), Some(21));
        assert_eq!(optimal_graalvm_major_for_spec(25), Some(25));
        assert_eq!(optimal_graalvm_major_for_spec(27), None);

        assert_eq!(recommended_runtime_label_for_mc("1.7.10", true), "Java 8");
        assert_eq!(recommended_runtime_label_for_mc("1.12.2", true), "Java 8");
        assert_eq!(recommended_runtime_label_for_mc("1.16.5", true), "Java 8");
        assert_eq!(
            recommended_runtime_label_for_mc("1.17.1", true),
            "GraalVM 17"
        );
        assert_eq!(
            recommended_runtime_label_for_mc("1.20.1", true),
            "GraalVM 17"
        );
        assert_eq!(
            recommended_runtime_label_for_mc("1.21.4", true),
            "GraalVM 21"
        );
        assert_eq!(recommended_runtime_label_for_mc("26.1", true), "GraalVM 25");

        assert_eq!(recommended_runtime_label_for_mc("1.21.4", false), "Java 21");
        assert_eq!(recommended_runtime_label_for_mc("1.17.1", false), "Java 16");
    }

    #[tokio::test]
    async fn targeted_graalvm_lookup_fetches_only_requested_major_and_sha256() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let os_token = if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        };
        let arch_token = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "x64"
        };
        let ext = if cfg!(target_os = "windows") {
            ".zip"
        } else {
            ".tar.gz"
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let asset_21 = format!("graalvm-community-jdk-21.0.2_{os_token}-{arch_token}_bin{ext}");
        let asset_17 = format!("graalvm-community-jdk-17.0.9_{os_token}-{arch_token}_bin{ext}");
        let expected_sha = "3a01d4e2eaab87bee5fdccae051fc3dc2c4dd655c04b942aa8ff7a74aab314c4";

        let asset_21_srv = asset_21.clone();
        let asset_17_srv = asset_17.clone();
        tokio::spawn(async move {
            for _ in 0..2 {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.contains("GET /releases?per_page=100&page=1") {
                    let body = format!(
                        r#"[
                            {{
                                "tag_name": "jdk-21.0.2",
                                "assets": [
                                    {{
                                        "name": "{asset_21_srv}",
                                        "browser_download_url": "http://{addr}/dl/{asset_21_srv}",
                                        "size": 198765432
                                    }},
                                    {{
                                        "name": "{asset_21_srv}.sha256",
                                        "browser_download_url": "http://{addr}/dl/{asset_21_srv}.sha256",
                                        "size": 65
                                    }}
                                ]
                            }},
                            {{
                                "tag_name": "jdk-17.0.9",
                                "assets": [
                                    {{
                                        "name": "{asset_17_srv}",
                                        "browser_download_url": "http://{addr}/dl/{asset_17_srv}",
                                        "size": 187654321
                                    }},
                                    {{
                                        "name": "{asset_17_srv}.sha256",
                                        "browser_download_url": "http://{addr}/dl/{asset_17_srv}.sha256",
                                        "size": 65
                                    }}
                                ]
                            }}
                        ]"#
                    );
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                } else if req.contains(&format!("GET /dl/{asset_21_srv}.sha256")) {
                    let body = format!("{expected_sha}  {asset_21_srv}\n");
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                } else {
                    let _ = stream
                        .write_all(b"HTTP/1.1 500 Unexpected Request\r\nContent-Length: 0\r\n\r\n")
                        .await;
                }
            }
        });

        let downloader = thk_core::DownloadManager::new().unwrap();
        let pkgs = distros::fetch_graalvm_packages_from_api(
            &downloader,
            &format!("http://{addr}/releases"),
            Some(21),
        )
        .await
        .unwrap();

        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].distro, JavaDistroKind::GraalVm);
        assert_eq!(pkgs[0].major_version, 21);
        assert_eq!(pkgs[0].release_name, "jdk-21.0.2");
        assert_eq!(pkgs[0].file_name, asset_21);
        assert_eq!(pkgs[0].sha256.as_deref(), Some(expected_sha));
        assert_eq!(pkgs[0].size_bytes, Some(198_765_432));

        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();
        let tmp_root =
            std::env::temp_dir().join(format!("thk-java-cancel-{}", uuid::Uuid::new_v4()));
        let cancel_res =
            install_distro_package(&downloader, &tmp_root, &pkgs[0], Some(token), |_| {}).await;
        assert!(matches!(cancel_res, Err(thk_core::ThkError::Cancelled)));
        let _ = std::fs::remove_dir_all(&tmp_root);
    }

    #[test]
    fn android_bionic_openjdk_catalog_and_ld_library_path_resolution() {
        let android_v17 = concat!(
            "openjdk version \"17.0.19-internal\" 2026-04-21\n",
            "OpenJDK Android Runtime Environment (build 17.0.19-internal+0-adhoc.runner.openjdk-17)\n",
            "OpenJDK 64-Bit Server VM (build 17.0.19-internal+0-adhoc.runner.openjdk-17, mixed mode)"
        );
        let (maj, full, vendor) = parse_java_version_output(android_v17).unwrap();
        assert_eq!(maj, 17);
        assert_eq!(full, "17.0.19-internal");
        assert_eq!(vendor, "OpenJDK");

        assert_eq!(android_openjdk_major_for_spec(8), Some(8));
        assert_eq!(android_openjdk_major_for_spec(16), Some(17));
        assert_eq!(android_openjdk_major_for_spec(17), Some(17));
        assert_eq!(android_openjdk_major_for_spec(21), Some(21));
        assert_eq!(android_openjdk_major_for_spec(25), Some(25));
        assert_eq!(android_openjdk_major_for_spec(27), None);

        let pkgs = android_openjdk_packages();
        assert_eq!(
            pkgs.iter().map(|p| p.major_version).collect::<Vec<_>>(),
            vec![8, 17, 21, 25]
        );
        for pkg in &pkgs {
            assert_eq!(pkg.distro, JavaDistroKind::AndroidOpenJdk);
            assert!(pkg.file_name.ends_with(".tar.xz"));
            let sha = pkg.sha256.as_deref().expect("verified sha256");
            assert_eq!(sha.len(), 64);
            assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(pkg.size_bytes.unwrap_or(0) > 20_000_000);
        }

        let tmp = std::env::temp_dir().join(format!("thk-java-ld-{}", uuid::Uuid::new_v4()));
        let jre17_home = tmp.join("openjdk-17");
        std::fs::create_dir_all(jre17_home.join("bin")).unwrap();
        std::fs::create_dir_all(jre17_home.join("lib").join("server")).unwrap();
        let java_bin = jre17_home.join("bin").join("java");
        std::fs::write(&java_bin, b"stub").unwrap();
        assert_eq!(
            resolve_java_home(&java_bin).as_deref(),
            Some(jre17_home.as_path())
        );
        let ld17 = build_java_ld_library_path(&jre17_home, Some("/system/lib64")).unwrap();
        assert!(ld17.contains(&jre17_home.join("lib").to_string_lossy().to_string()));
        assert!(ld17.contains(
            &jre17_home
                .join("lib")
                .join("server")
                .to_string_lossy()
                .to_string()
        ));
        assert!(ld17.ends_with(":/system/lib64"));

        let jre8_home = tmp.join("openjdk-8");
        std::fs::create_dir_all(jre8_home.join("lib").join("aarch64").join("jli")).unwrap();
        std::fs::create_dir_all(jre8_home.join("lib").join("aarch64").join("server")).unwrap();
        let ld8 = build_java_ld_library_path(&jre8_home, None).unwrap();
        assert!(ld8.contains("aarch64"));
        assert!(ld8.contains("jli"));
        assert!(ld8.contains("server"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
