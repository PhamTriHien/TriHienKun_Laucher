use crate::{
    install_curseforge_content, install_modrinth_content, ProjectSummary, ProjectVersion,
    VersionFile,
};
use thk_core::models::{
    ContentKind, GameProfile, InstalledContentRecord, LoaderType, ProviderSource,
};
use thk_core::{Database, DownloadManager, ThkError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn provider_updates_preserve_old_install_on_failure_or_cancel_and_replace_on_success() {
    for provider in [ProviderSource::Modrinth, ProviderSource::CurseForge] {
        for outcome in ["size-error", "cancel", "success"] {
            let root = std::env::temp_dir().join(format!("thk-install-{}", uuid::Uuid::new_v4()));
            let mods_dir = root.join("mods");
            std::fs::create_dir_all(&mods_dir).unwrap();
            std::fs::write(mods_dir.join("old.jar"), b"previous-installation").unwrap();
            let profile = GameProfile {
                id: "profile".into(),
                name: "Update regression".into(),
                game_version: "1.21.4".into(),
                loader: LoaderType::Fabric,
                loader_version: None,
                instance_path: root.clone(),
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
            let db = Database::open_in_memory().unwrap();
            db.upsert_profile(&profile).unwrap();
            let old = InstalledContentRecord {
                profile_id: profile.id.clone(),
                content_kind: ContentKind::Mod,
                file_name: "old.jar".into(),
                sha1: String::new(),
                provider,
                project_id: "123".into(),
                version_id: "old".into(),
                title: "Mod".into(),
                version_number: "1".into(),
                icon_url: None,
                enabled: true,
                installed_at: 0,
            };
            db.upsert_installed_content(&old).unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/mod", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                assert!(socket.read(&mut request).await.unwrap() > 0);
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nnew",
                    )
                    .await
                    .unwrap();
            });
            let project = ProjectSummary {
                provider,
                project_id: "123".into(),
                slug: "mod".into(),
                title: "Mod".into(),
                summary: String::new(),
                author: String::new(),
                downloads: 0,
                icon_url: None,
                categories: Vec::new(),
                updated_at: None,
                content_kind: ContentKind::Mod,
            };
            let version = ProjectVersion {
                id: "new".into(),
                project_id: project.project_id.clone(),
                provider,
                name: "New version".into(),
                version_number: "2".into(),
                release_channel: "release".into(),
                published_at: None,
                game_versions: vec![profile.game_version.clone()],
                loaders: vec!["fabric".into()],
                dependencies: Vec::new(),
                server_pack_file_id: None,
                files: vec![VersionFile {
                    url,
                    filename: "new.jar".into(),
                    size: if outcome == "size-error" { 10 } else { 3 },
                    sha1: None,
                    sha512: None,
                    primary: true,
                }],
            };
            let cancel = CancellationToken::new();
            let cancel_on_progress = cancel.clone();
            let callback = move |progress: thk_core::models::DownloadProgress| {
                if outcome == "cancel" && progress.downloaded_bytes > 0 {
                    cancel_on_progress.cancel();
                }
            };
            let downloader = DownloadManager::new().unwrap();
            let result = match provider {
                ProviderSource::Modrinth => {
                    install_modrinth_content(
                        &downloader,
                        &db,
                        &profile,
                        ContentKind::Mod,
                        &project,
                        &version,
                        Some(cancel),
                        callback,
                    )
                    .await
                }
                ProviderSource::CurseForge => {
                    install_curseforge_content(
                        &downloader,
                        &db,
                        &profile,
                        ContentKind::Mod,
                        &project,
                        &version,
                        None,
                        Some(cancel),
                        callback,
                    )
                    .await
                }
                _ => unreachable!(),
            };
            server.await.unwrap();
            let records = db
                .list_installed_content(&profile.id, ContentKind::Mod)
                .unwrap();
            assert_eq!(records.len(), 1, "{provider:?}/{outcome}");
            if outcome == "success" {
                result.unwrap();
                assert!(!mods_dir.join("old.jar").exists());
                assert_eq!(std::fs::read(mods_dir.join("new.jar")).unwrap(), b"new");
                assert_eq!(records[0].version_id, "new");
            } else {
                assert!(result.is_err(), "{provider:?}/{outcome}");
                if outcome == "cancel" {
                    assert!(matches!(result, Err(ThkError::Cancelled)));
                }
                assert_eq!(
                    std::fs::read(mods_dir.join("old.jar")).unwrap(),
                    b"previous-installation"
                );
                assert!(!mods_dir.join("new.jar").exists());
                assert_eq!(records[0].version_id, "old");
            }
            assert_eq!(std::fs::read_dir(&mods_dir).unwrap().count(), 1);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}

#[test]
fn database_commit_does_not_remove_another_providers_project_with_same_id() {
    let root = std::env::temp_dir().join(format!("thk-cleanup-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let db = Database::open_in_memory().unwrap();
    let installed = InstalledContentRecord {
        profile_id: "p".into(),
        content_kind: ContentKind::Mod,
        file_name: "new.jar".into(),
        sha1: String::new(),
        provider: ProviderSource::Modrinth,
        project_id: "123".into(),
        version_id: "v2".into(),
        title: "Mod".into(),
        version_number: "2".into(),
        icon_url: None,
        enabled: true,
        installed_at: 0,
    };
    let mut unrelated = installed.clone();
    unrelated.provider = ProviderSource::CurseForge;
    unrelated.file_name = "unrelated.jar".into();
    std::fs::write(root.join(&unrelated.file_name), b"unrelated").unwrap();
    db.upsert_installed_content(&unrelated).unwrap();
    db.commit_installed_content(&[installed]).unwrap();
    assert!(root.join(&unrelated.file_name).exists());
    assert_eq!(
        db.list_installed_content("p", ContentKind::Mod)
            .unwrap()
            .len(),
        2
    );
    std::fs::remove_dir_all(root).unwrap();
}
