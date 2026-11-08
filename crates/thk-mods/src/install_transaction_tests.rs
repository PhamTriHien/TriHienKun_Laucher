use super::{commit_batch, install_content_batch, StagingDirectory};
use thk_core::models::{ContentKind, InstalledContentRecord, ProviderSource};
use thk_core::{Database, DownloadManager, DownloadTask, ThkError};

fn record(project: &str, name: &str) -> InstalledContentRecord {
    InstalledContentRecord {
        profile_id: "test-profile".into(),
        content_kind: ContentKind::Mod,
        file_name: name.into(),
        sha1: String::new(),
        provider: ProviderSource::Modrinth,
        project_id: project.into(),
        version_id: "old".into(),
        title: "Transaction regression".into(),
        version_number: "1".into(),
        icon_url: None,
        enabled: true,
        installed_at: 0,
    }
}

fn staging() -> (StagingDirectory, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("thk-transaction-{}", uuid::Uuid::new_v4()));
    let mods = path.join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    (
        StagingDirectory {
            path,
            preserve: false,
        },
        mods,
    )
}

#[test]
fn same_filename_update_preserves_disabled_state_and_unmanaged_counterpart() {
    let (mut root, mods) = staging();
    let db = Database::open_in_memory().unwrap();
    let mut old = record("one", "mod.jar");
    old.enabled = false;
    db.upsert_installed_content(&old).unwrap();
    std::fs::write(mods.join("mod.jar.disabled"), b"old").unwrap();
    std::fs::write(mods.join("mod.jar"), b"unmanaged").unwrap();
    std::fs::write(root.path.join("0"), b"new").unwrap();
    let mut new = old.clone();
    new.enabled = true;
    new.version_id = "new".into();
    let mut records = [new];
    let paths = commit_batch(&db, &mods, &mut root, &mut records).unwrap();
    assert_eq!(paths, vec![mods.join("mod.jar.disabled")]);
    assert_eq!(std::fs::read(&paths[0]).unwrap(), b"new");
    assert_eq!(std::fs::read(mods.join("mod.jar")).unwrap(), b"unmanaged");
    let saved = db
        .list_installed_content("test-profile", ContentKind::Mod)
        .unwrap();
    assert_eq!(saved.len(), 1);
    assert!(!saved[0].enabled);
    assert_eq!(saved[0].version_id, "new");
}

#[test]
fn failure_after_first_file_restores_entire_batch() {
    let (mut root, mods) = staging();
    let db = Database::open_in_memory().unwrap();
    let old = record("one", "old.jar");
    db.upsert_installed_content(&old).unwrap();
    std::fs::write(mods.join("old.jar"), b"old").unwrap();
    std::fs::write(root.path.join("0"), b"new").unwrap();
    let mut records = [record("one", "new.jar"), record("two", "missing.jar")];
    assert!(commit_batch(&db, &mods, &mut root, &mut records).is_err());
    assert_eq!(std::fs::read(mods.join("old.jar")).unwrap(), b"old");
    assert_eq!(std::fs::read_dir(&mods).unwrap().count(), 1);
    let saved = db
        .list_installed_content("test-profile", ContentKind::Mod)
        .unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].file_name, "old.jar");
    assert!(!root.preserve);
}

#[test]
fn sqlite_failure_rolls_back_files_and_database() {
    let (mut root, mods) = staging();
    let db_path = root.path.join("test.sqlite");
    let db = Database::open(&db_path).unwrap();
    let old = record("one", "old.jar");
    db.upsert_installed_content(&old).unwrap();
    let connection = rusqlite::Connection::open(&db_path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_test_install BEFORE INSERT ON installed_content
         WHEN NEW.version_id = 'rejected' BEGIN SELECT RAISE(ABORT, 'test rejection'); END;",
        )
        .unwrap();
    std::fs::write(mods.join("old.jar"), b"old").unwrap();
    std::fs::write(root.path.join("0"), b"one").unwrap();
    std::fs::write(root.path.join("1"), b"two").unwrap();
    let mut rejected = record("two", "two.jar");
    rejected.version_id = "rejected".into();
    let mut records = [record("one", "new.jar"), rejected];
    assert!(commit_batch(&db, &mods, &mut root, &mut records).is_err());
    assert_eq!(std::fs::read(mods.join("old.jar")).unwrap(), b"old");
    assert_eq!(std::fs::read_dir(&mods).unwrap().count(), 1);
    let saved = db
        .list_installed_content("test-profile", ContentKind::Mod)
        .unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].file_name, "old.jar");
}

#[test]
fn unmanaged_destination_is_never_overwritten() {
    let (mut root, mods) = staging();
    let db = Database::open_in_memory().unwrap();
    std::fs::write(mods.join("new.jar"), b"user-file").unwrap();
    std::fs::write(root.path.join("0"), b"new").unwrap();
    let mut records = [record("one", "new.jar")];
    assert!(matches!(
        commit_batch(&db, &mods, &mut root, &mut records),
        Err(ThkError::InvalidInput(_))
    ));
    assert_eq!(std::fs::read(mods.join("new.jar")).unwrap(), b"user-file");
}

#[tokio::test]
async fn duplicate_filenames_are_rejected_before_download() {
    let (_root, mods) = staging();
    let db = Database::open_in_memory().unwrap();
    let downloader = DownloadManager::new().unwrap();
    let records = [record("one", "mod.jar"), record("two", "MOD.jar")];
    let pending: Vec<_> = records
        .into_iter()
        .map(|rec| (mods.join(&rec.file_name), rec))
        .collect();
    let tasks = pending
        .iter()
        .map(|(path, _)| DownloadTask::new("http://127.0.0.1:1/test-only", path.clone()))
        .collect();
    let result = install_content_batch(
        &downloader,
        &db,
        &mods,
        "Test",
        tasks,
        pending,
        None,
        |_| {},
    )
    .await;
    assert!(matches!(result, Err(ThkError::InvalidInput(_))));
    assert_eq!(std::fs::read_dir(mods).unwrap().count(), 0);
}

#[test]
fn rollback_refuses_to_overwrite_a_file_created_by_another_process() {
    let (root, mods) = staging();
    let backup = root.path.join("backup-file");
    let original = mods.join("mod.jar");
    std::fs::write(&backup, b"old").unwrap();
    std::fs::write(&original, b"external").unwrap();
    assert_eq!(
        super::restore_backup(&backup, &original)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(std::fs::read(backup).unwrap(), b"old");
    assert_eq!(std::fs::read(original).unwrap(), b"external");
}

#[test]
fn database_batch_rejects_ownership_conflict_and_reverts_earlier_records() {
    let db = Database::open_in_memory().unwrap();
    let owned = record("owner", "shared.jar");
    db.upsert_installed_content(&owned).unwrap();
    let records = [record("new", "new.jar"), record("other", "SHARED.jar")];
    assert!(matches!(
        db.commit_installed_content(&records),
        Err(ThkError::InvalidInput(_))
    ));
    let saved = db
        .list_installed_content("test-profile", ContentKind::Mod)
        .unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].project_id, "owner");
}

#[cfg(windows)]
#[test]
fn rollback_sharing_violation_retains_recovery_data_until_manual_restore() {
    use std::os::windows::fs::OpenOptionsExt;

    let (root, mods) = staging();
    let transaction_path = root.path.join("transaction");
    std::fs::create_dir(&transaction_path).unwrap();
    let mut transaction = StagingDirectory {
        path: transaction_path.clone(),
        preserve: false,
    };
    let db = Database::open_in_memory().unwrap();
    db.upsert_installed_content(&record("one", "old.jar"))
        .unwrap();
    let original = mods.join("old.jar");
    std::fs::write(&original, b"old").unwrap();
    // FILE_SHARE_DELETE permits backup renaming but denies reads during restoration.
    let locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(4)
        .open(&original)
        .unwrap();
    let mut records = [record("one", "new.jar")];
    let result = commit_batch(&db, &mods, &mut transaction, &mut records);
    assert!(
        matches!(result, Err(ThkError::Process(message)) if message.contains("rollback failed"))
    );
    assert!(transaction.preserve);
    drop(transaction);
    assert!(transaction_path.join("recovery.json").is_file());
    let recovery: serde_json::Value =
        serde_json::from_slice(&std::fs::read(transaction_path.join("recovery.json")).unwrap())
            .unwrap();
    let backup = std::path::PathBuf::from(recovery["backups"][0][1].as_str().unwrap());
    assert!(backup.is_file());
    assert_eq!(
        db.list_installed_content("test-profile", ContentKind::Mod)
            .unwrap()[0]
            .file_name,
        "old.jar"
    );
    drop(locked);
    assert_eq!(std::fs::read(&backup).unwrap(), b"old");
    super::restore_backup(&backup, &original).unwrap();
    assert_eq!(std::fs::read(original).unwrap(), b"old");
}

#[tokio::test]
async fn failed_or_cancelled_batch_cleans_staging_after_a_completed_download() {
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio_util::sync::CancellationToken;

    for cancel_batch in [false, true] {
        let (_root, mods) = staging();
        let db = Database::open_in_memory().unwrap();
        let old = record("one", "old.jar");
        db.upsert_installed_content(&old).unwrap();
        std::fs::write(mods.join("old.jar"), b"old").unwrap();
        let ready = Arc::new(tokio::sync::Notify::new());
        let signal = Arc::clone(&ready);
        let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let second = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let cancel = CancellationToken::new();
        let server_cancel = cancel.clone();
        let records = [record("one", "new.jar"), record("two", "two.jar")];
        let pending: Vec<_> = records
            .into_iter()
            .map(|rec| (mods.join(&rec.file_name), rec))
            .collect();
        let tasks = [&first, &second]
            .into_iter()
            .zip(&pending)
            .map(|(listener, (dest, _))| {
                DownloadTask::new(
                    format!("http://{}/test", listener.local_addr().unwrap()),
                    dest.clone(),
                )
            })
            .collect();
        let first_server = tokio::spawn(async move {
            let (mut socket, _) = first.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nnew")
                .await
                .unwrap();
        });
        let second_server = tokio::spawn(async move {
            let (mut socket, _) = second.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            ready.notified().await;
            if cancel_batch {
                server_cancel.cancel();
            } else {
                socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            }
        });
        let manager = DownloadManager::new().unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            install_content_batch(
                &manager,
                &db,
                &mods,
                "Test",
                tasks,
                pending,
                Some(cancel),
                move |progress| {
                    if progress.completed_files == 1 {
                        signal.notify_one();
                    }
                },
            ),
        )
        .await
        .unwrap();
        if cancel_batch {
            assert!(matches!(result, Err(ThkError::Cancelled)));
        } else {
            assert!(matches!(result, Err(ThkError::Network(_))));
        }
        assert_eq!(std::fs::read_dir(&mods).unwrap().count(), 1);
        assert_eq!(std::fs::read(mods.join("old.jar")).unwrap(), b"old");
        let saved = db
            .list_installed_content("test-profile", ContentKind::Mod)
            .unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].file_name, "old.jar");
        first_server.await.unwrap();
        second_server.await.unwrap();
    }
}
