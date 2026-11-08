use super::*;
use std::io::Write;
use thk_core::models::{ContentKind, LoaderType, ProviderSource};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let base = if cfg!(windows) {
            PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join("Temp/opencode")
        } else {
            std::env::temp_dir()
        };
        let path = base.join(format!("thk-pack-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

// Synthetic profile and archives use only isolated test directories and SQLite databases.
fn profile(destination: &Path) -> GameProfile {
    GameProfile {
        id: "pack-regression".into(),
        name: "Pack regression".into(),
        game_version: "1.21.4".into(),
        loader: LoaderType::Vanilla,
        loader_version: None,
        instance_path: destination.into(),
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
    }
}

fn write_payload(installation: &PackInstallation) {
    fs::create_dir(installation.payload().join("config")).unwrap();
    fs::write(
        installation.payload().join("config/test.txt"),
        b"test configuration",
    )
    .unwrap();
}

#[test]
fn pack_install_rejects_existing_data_and_preserves_late_conflicts() {
    let root = TestDirectory::new();
    let destination = root.0.join("instance");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("options.txt"), b"personal options").unwrap();
    assert!(PackInstallation::prepare(&destination).is_err());
    assert_eq!(
        fs::read(destination.join("options.txt")).unwrap(),
        b"personal options"
    );
    fs::remove_file(destination.join("options.txt")).unwrap();
    let installation = PackInstallation::prepare(&destination).unwrap();
    let staging = installation.staging.clone();
    write_payload(&installation);
    fs::write(destination.join("options.txt"), b"concurrent file").unwrap();
    let db = Database::open_in_memory().unwrap();
    assert!(installation
        .commit(&db, &profile(&destination), &[], None)
        .is_err());
    assert_eq!(
        fs::read(destination.join("options.txt")).unwrap(),
        b"concurrent file"
    );
    assert!(!destination.join("config").exists());
    assert!(!staging.exists());
    assert!(db.list_profiles().unwrap().is_empty());
}

#[test]
fn pack_install_does_not_reuse_another_profiles_empty_directory() {
    let root = TestDirectory::new();
    let destination = root.0.join("instance");
    fs::create_dir(&destination).unwrap();
    let db = Database::open_in_memory().unwrap();
    let mut existing = profile(&destination);
    existing.id = "existing-profile".into();
    db.upsert_profile(&existing).unwrap();
    let installation = PackInstallation::prepare(&destination).unwrap();
    let staging = installation.staging.clone();
    write_payload(&installation);
    assert!(installation
        .commit(&db, &profile(&destination), &[], None)
        .is_err());
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    assert!(!staging.exists());
    assert_eq!(db.list_profiles().unwrap()[0].id, existing.id);
}

#[test]
fn pack_install_cancel_cleans_staging_without_touching_destination() {
    let root = TestDirectory::new();
    let destination = root.0.join("instance");
    fs::create_dir(&destination).unwrap();
    let installation = PackInstallation::prepare(&destination).unwrap();
    let staging = installation.staging.clone();
    write_payload(&installation);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let db = Database::open_in_memory().unwrap();
    assert!(matches!(
        installation.commit(&db, &profile(&destination), &[], Some(&cancel)),
        Err(ThkError::Cancelled)
    ));
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    assert!(!staging.exists());
    assert!(db.list_profiles().unwrap().is_empty());
}

#[test]
fn pack_install_sqlite_failure_rolls_back_new_files_and_preserves_empty_custom_directory() {
    for existing in [false, true] {
        let root = TestDirectory::new();
        let destination = root.0.join("instance");
        if existing {
            fs::create_dir(&destination).unwrap();
        }
        let db_path = root.0.join("test.sqlite");
        let db = Database::open(&db_path).unwrap();
        let connection = rusqlite::Connection::open(&db_path).unwrap();
        connection.execute_batch(
            "CREATE TRIGGER reject_pack BEFORE INSERT ON profiles BEGIN SELECT RAISE(ABORT, 'isolated test rejection'); END;"
        ).unwrap();
        let installation = PackInstallation::prepare(&destination).unwrap();
        let staging = installation.staging.clone();
        write_payload(&installation);
        let sample_record = InstalledContentRecord {
            profile_id: String::new(),
            content_kind: ContentKind::Mod,
            file_name: "sodium.jar".into(),
            sha1: "0123456789abcdef0123456789abcdef01234567".into(),
            provider: ProviderSource::Modrinth,
            project_id: "AANobbMI".into(),
            version_id: "ver-1".into(),
            title: "Sodium".into(),
            version_number: "0.6.5".into(),
            icon_url: None,
            enabled: true,
            installed_at: 1000,
        };
        assert!(installation
            .commit(
                &db,
                &profile(&destination),
                std::slice::from_ref(&sample_record),
                None
            )
            .is_err());
        assert_eq!(destination.exists(), existing);
        if existing {
            assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        }
        assert!(!staging.exists());
        assert!(db.list_profiles().unwrap().is_empty());
        assert!(db
            .list_installed_content("pack-regression", ContentKind::Mod)
            .unwrap()
            .is_empty());
    }
}

#[cfg(windows)]
#[test]
fn pack_rollback_sharing_violation_retains_payload_and_recovery_after_drop() {
    use std::os::windows::fs::OpenOptionsExt;

    let root = TestDirectory::new();
    let destination = root.0.join("instance");
    let mut installation = PackInstallation::prepare(&destination).unwrap();
    let staging = installation.staging.clone();
    write_payload(&installation);
    fs::create_dir(&destination).unwrap();
    let config = destination.join("config");
    fs::create_dir(&config).unwrap();
    let installed_file = config.join("test.txt");
    fs::copy(
        installation.payload().join("config/test.txt"),
        &installed_file,
    )
    .unwrap();
    // Reconstruct the post-copy transaction state without timing-dependent worker races.
    let recovery = serde_json::json!({
        "destination": destination,
        "files": ["config/test.txt"],
        "directories": ["config"],
        "profile": profile(&destination),
    });
    let recovery_bytes = serde_json::to_vec_pretty(&recovery).unwrap();
    fs::write(staging.join("recovery.json"), &recovery_bytes).unwrap();
    installation.preserve = true;
    // FILE_SHARE_READ permits inspection but denies deletion during rollback.
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&installed_file)
        .unwrap();
    let error = installation.rollback(
        ThkError::Cancelled,
        std::slice::from_ref(&installed_file),
        &[destination.clone(), config],
    );
    let message = error.to_string();
    assert!(matches!(error, ThkError::Process(_)));
    assert!(message.contains("rollback failed"));
    assert!(message.contains(&staging.display().to_string()));
    assert!(message.contains(&installed_file.display().to_string()));
    drop(installation);
    assert_eq!(
        fs::read(staging.join("recovery.json")).unwrap(),
        recovery_bytes
    );
    assert_eq!(
        fs::read(staging.join("payload/config/test.txt")).unwrap(),
        b"test configuration"
    );
    assert_eq!(fs::read(&installed_file).unwrap(), b"test configuration");
    drop(locked);
    fs::remove_file(&installed_file).unwrap();
    fs::remove_dir(destination.join("config")).unwrap();
    fs::remove_dir(&destination).unwrap();
    assert!(staging.join("recovery.json").is_file());
}

fn archive(path: &Path, override_path: &str) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("modrinth.index.json", options).unwrap();
    zip.write_all(br#"{"formatVersion":1,"game":"minecraft","versionId":"test","name":"Isolated test pack","files":[],"dependencies":{"minecraft":"1.21.4"}}"#).unwrap();
    zip.start_file(override_path, options).unwrap();
    zip.write_all(b"test configuration").unwrap();
    zip.finish().unwrap();
}

#[tokio::test]
async fn pack_install_imports_real_archive_and_commits_files_with_profile() {
    let root = TestDirectory::new();
    let source = root.0.join("test.mrpack");
    archive(&source, "overrides/config/test.txt");
    let destination = root.0.join("instance");
    let installation = PackInstallation::prepare(&destination).unwrap();
    let staging = installation.staging.clone();
    let downloader = thk_core::DownloadManager::new().unwrap();
    let metadata =
        thk_mods::import_mrpack_archive(&downloader, &source, installation.payload(), None, |_| {})
            .await
            .unwrap();
    assert_eq!(metadata.game_version, "1.21.4");
    assert!(!destination.exists());
    let db = Database::open_in_memory().unwrap();
    let sample_record = InstalledContentRecord {
        profile_id: String::new(),
        content_kind: ContentKind::Mod,
        file_name: "sodium.jar".into(),
        sha1: "0123456789abcdef0123456789abcdef01234567".into(),
        provider: ProviderSource::Modrinth,
        project_id: "AANobbMI".into(),
        version_id: "ver-1".into(),
        title: "Sodium".into(),
        version_number: "0.6.5".into(),
        icon_url: None,
        enabled: true,
        installed_at: 1000,
    };
    installation
        .commit(
            &db,
            &profile(&destination),
            std::slice::from_ref(&sample_record),
            None,
        )
        .unwrap();
    assert_eq!(
        fs::read(destination.join("config/test.txt")).unwrap(),
        b"test configuration"
    );
    assert_eq!(db.list_profiles().unwrap()[0].instance_path, destination);
    let saved_records = db
        .list_installed_content("pack-regression", ContentKind::Mod)
        .unwrap();
    assert_eq!(saved_records.len(), 1);
    assert_eq!(saved_records[0].profile_id, "pack-regression");
    assert_eq!(saved_records[0].file_name, "sodium.jar");
    assert!(!staging.exists());
}

#[tokio::test]
async fn pack_install_failed_import_and_cancel_clean_staging_without_creating_destination() {
    for cancelled in [false, true] {
        let root = TestDirectory::new();
        let source = root.0.join("test.mrpack");
        archive(&source, "overrides/../../escape.txt");
        let destination = root.0.join("instance");
        let installation = PackInstallation::prepare(&destination).unwrap();
        let staging = installation.staging.clone();
        let downloader = thk_core::DownloadManager::new().unwrap();
        let cancel = CancellationToken::new();
        if cancelled {
            cancel.cancel();
        }
        let result = thk_mods::import_mrpack_archive(
            &downloader,
            &source,
            installation.payload(),
            Some(cancel),
            |_| {},
        )
        .await;
        assert!(result.is_err());
        if cancelled {
            assert!(matches!(result, Err(ThkError::Cancelled)));
        }
        drop(installation);
        assert!(!staging.exists());
        assert!(!destination.exists());
        assert!(!root.0.join("escape.txt").exists());
    }
}

#[tokio::test]
#[ignore = "downloads a pinned public Modrinth pack; requires network access"]
async fn live_modrinth_pack_download_hashes_and_profile_commit() {
    use thk_core::models::ProviderSource;
    use thk_mods::{fetch_provider_versions, install_modrinth_modpack_version};

    let root = TestDirectory::new();
    let downloader = thk_core::DownloadManager::new().unwrap();
    let versions = fetch_provider_versions(
        &downloader,
        ProviderSource::Modrinth,
        "Aa5L6RtV",
        Some("1.21.4"),
        Some(LoaderType::Quilt),
        None,
    )
    .await
    .unwrap();
    let version = versions.iter().find(|v| v.id == "K3sPpfVz").unwrap();
    let file = version.files.iter().find(|file| file.primary).unwrap();
    let archive = root.0.join("provider.mrpack");
    fs::write(&archive, downloader.get_bytes(&file.url).await.unwrap()).unwrap();
    assert_eq!(fs::metadata(&archive).unwrap().len(), file.size);
    thk_core::hash::verify_file_sha1(&archive, file.sha1.as_deref().unwrap()).unwrap();
    thk_core::hash::verify_file_sha512(&archive, file.sha512.as_deref().unwrap()).unwrap();
    let index: serde_json::Value = serde_json::from_str(
        &thk_core::archive::read_zip_entry_string(&archive, "modrinth.index.json")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let destination = root.0.join("instance");
    let installation = PackInstallation::prepare(&destination).unwrap();
    let staging = installation.staging.clone();
    let metadata = install_modrinth_modpack_version(
        &downloader,
        version,
        &root.0.join("cache"),
        installation.payload(),
        None,
        None,
        |_| {},
    )
    .await
    .unwrap();
    assert!(!destination.exists());
    assert_eq!(
        metadata.game_version,
        index["dependencies"]["minecraft"].as_str().unwrap()
    );
    assert_eq!(metadata.loader, LoaderType::Quilt);
    assert_eq!(
        metadata.loader_version.as_deref(),
        index["dependencies"]["quilt-loader"].as_str()
    );
    let installed_records = metadata.installed_content.clone();
    let saved = crate::state::build_profile_from_imported_pack(
        "live-modrinth-regression".into(),
        destination.clone(),
        metadata,
        4096,
    );
    let db = Database::open(&root.0.join("profiles.sqlite")).unwrap();
    installation
        .commit(&db, &saved, &installed_records, None)
        .unwrap();
    assert_eq!(
        db.list_profiles().unwrap()[0].loader_version,
        saved.loader_version
    );
    assert_eq!(db.list_profiles().unwrap()[0].instance_path, destination);
    let mut verified = 0;
    for entry in index["files"].as_array().unwrap() {
        if entry["env"]["client"].as_str() == Some("unsupported") {
            continue;
        }
        let path =
            thk_core::paths::SafePath::resolve_child(&destination, entry["path"].as_str().unwrap())
                .unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            entry["fileSize"].as_u64().unwrap()
        );
        thk_core::hash::verify_file_sha1(&path, entry["hashes"]["sha1"].as_str().unwrap()).unwrap();
        thk_core::hash::verify_file_sha512(&path, entry["hashes"]["sha512"].as_str().unwrap())
            .unwrap();
        verified += 1;
    }
    assert!(verified > 0);
    assert!(!staging.exists());
    eprintln!("Modrinth project={} version={} archive_sha1={} verified_files={} minecraft={} loader={:?} loader_version={:?}",
        version.project_id, version.id, file.sha1.as_deref().unwrap(), verified,
        saved.game_version, saved.loader, saved.loader_version);
}

#[tokio::test]
async fn cancellation_after_override_extraction_keeps_destination_untouched() {
    let root = TestDirectory::new();
    let source = root.0.join("test.mrpack");
    archive(&source, "overrides/config/test.txt");
    let destination = root.0.join("instance");
    let installation = PackInstallation::prepare(&destination).unwrap();
    let staging = installation.staging.clone();
    let cancel = CancellationToken::new();
    let downloader = thk_core::DownloadManager::new().unwrap();
    thk_mods::import_mrpack_archive(
        &downloader,
        &source,
        installation.payload(),
        Some(cancel.clone()),
        |_| {},
    )
    .await
    .unwrap();
    assert!(installation.payload().join("config/test.txt").is_file());
    cancel.cancel();
    let db = Database::open_in_memory().unwrap();
    assert!(matches!(
        installation.commit(&db, &profile(&destination), &[], Some(&cancel)),
        Err(ThkError::Cancelled)
    ));
    assert!(db.list_profiles().unwrap().is_empty());
    assert!(!destination.exists());
    assert!(!staging.exists());
}

#[test]
fn pack_install_commit_with_progress_links_files_and_reports_progress() {
    let root = TestDirectory::new();
    let destination = root.0.join("instance");
    let installation = PackInstallation::prepare(&destination).unwrap();
    let staging = installation.staging.clone();
    write_payload(&installation);
    let db = Database::open_in_memory().unwrap();
    let phases = std::cell::RefCell::new(Vec::new());
    installation
        .commit_with_progress(&db, &profile(&destination), &[], None, |p| {
            phases
                .borrow_mut()
                .push((p.phase, p.completed_files, p.total_files));
        })
        .unwrap();
    let recorded = phases.into_inner();
    assert!(recorded.len() >= 2, "{recorded:?}");
    assert_eq!(recorded.first().unwrap().0, "Finalizing Modpack");
    assert_eq!(
        recorded.last().unwrap(),
        &("Finalizing Modpack".into(), 1, 1)
    );
    assert!(!staging.exists());
    assert_eq!(
        fs::read(destination.join("config/test.txt")).unwrap(),
        b"test configuration"
    );
}
