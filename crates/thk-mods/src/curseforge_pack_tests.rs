use super::*;
use std::io::Write;

fn required_file() -> CfPackFileEntry {
    CfPackFileEntry {
        project_id: 123,
        file_id: 456,
        required: true,
    }
}

#[test]
fn required_pack_file_resolution_errors_are_not_silently_skipped() {
    let error = required_pack_download(
        Path::new("mods"),
        &required_file(),
        Err(ThkError::Provider("resolution failed".into())),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("project 123 file 456"), "{error}");
    assert!(error.contains("resolution failed"), "{error}");
    assert!(matches!(
        required_pack_download(
            Path::new("mods"),
            &required_file(),
            Err(ThkError::Cancelled)
        ),
        Err(ThkError::Cancelled)
    ));
}

#[test]
fn required_pack_file_rejects_unsafe_name_and_retains_hash() {
    for name in ["../escape.jar", "nested/mod.jar", "C:\\escape.jar", ""] {
        assert!(
            required_pack_download(
                Path::new("mods"),
                &required_file(),
                Ok(("https://example.invalid/mod".into(), name.into(), None)),
            )
            .is_err(),
            "{name}"
        );
    }
    let task = required_pack_download(
        Path::new("mods"),
        &required_file(),
        Ok((
            "https://example.invalid/mod".into(),
            "mod.jar".into(),
            Some("test-hash".into()),
        )),
    )
    .unwrap();
    assert_eq!(task.expected_sha1.as_deref(), Some("test-hash"));
    assert_eq!(task.dest, Path::new("mods/mod.jar"));
}

#[tokio::test]
async fn required_pack_planning_stops_on_error_instead_of_returning_partial_tasks() {
    let progress_updates = std::sync::Mutex::new(Vec::new());
    let result = resolve_pack_entries_with(
        &[(123, 456), (123, 457), (123, 458)],
        HashMap::new(),
        "Resolving Modpack Manifest",
        None,
        &|p| {
            progress_updates
                .lock()
                .unwrap()
                .push((p.completed_files, p.total_files))
        },
        |_, file| {
            std::future::ready(if file == 457 {
                Err(ThkError::Provider("isolated resolution failure".into()))
            } else {
                Ok(ResolvedCfPackFile {
                    url: "https://example.invalid/mod".into(),
                    file_name: format!("{file}.jar"),
                    sha1: None,
                    file_length: 1024,
                    game_versions: Vec::new(),
                })
            })
        },
    )
    .await;
    let error = result.unwrap_err().to_string();
    assert!(error.contains("project 123 file 457"), "{error}");
    assert!(error.contains("isolated resolution failure"), "{error}");
}

#[tokio::test]
async fn concurrent_pack_resolution_preserves_order_and_emits_progress() {
    let mut pre_resolved = HashMap::new();
    pre_resolved.insert(
        200,
        ResolvedCfPackFile {
            url: "https://example.invalid/200.jar".into(),
            file_name: "bulk-200.jar".into(),
            sha1: Some("sha1-200".into()),
            file_length: 2048,
            game_versions: vec!["1.19.2".into()],
        },
    );
    let progress_steps = std::sync::Mutex::new(Vec::new());
    let single_calls = std::sync::Mutex::new(Vec::new());
    let resolved = resolve_pack_entries_with(
        &[(10, 100), (20, 200), (30, 300)],
        pre_resolved,
        "Resolving Modpack Manifest",
        None,
        &|p| {
            progress_steps
                .lock()
                .unwrap()
                .push((p.phase, p.completed_files, p.total_files));
        },
        |pid, fid| {
            single_calls.lock().unwrap().push((pid, fid));
            async move {
                if fid == 100 {
                    tokio::task::yield_now().await;
                }
                Ok(ResolvedCfPackFile {
                    url: format!("https://example.invalid/{fid}.jar"),
                    file_name: format!("single-{fid}.jar"),
                    sha1: Some(format!("sha1-{fid}")),
                    file_length: 512,
                    game_versions: Vec::new(),
                })
            }
        },
    )
    .await
    .unwrap();

    assert_eq!(
        resolved
            .iter()
            .map(|r| r.file_name.as_str())
            .collect::<Vec<_>>(),
        vec!["single-100.jar", "bulk-200.jar", "single-300.jar"]
    );
    assert_eq!(*single_calls.lock().unwrap(), vec![(10, 100), (30, 300)]);
    let steps = progress_steps.into_inner().unwrap();
    assert_eq!(
        steps.first().unwrap(),
        &("Resolving Modpack Manifest".into(), 0, 3)
    );
    assert_eq!(
        steps.last().unwrap(),
        &("Resolving Modpack Manifest".into(), 3, 3)
    );
}

#[tokio::test]
async fn required_pack_planning_cancels_a_pending_resolution_without_a_timer() {
    let cancel = CancellationToken::new();
    let result = resolve_pack_entries_with(
        &[(123, 456)],
        HashMap::new(),
        "Resolving Modpack Manifest",
        Some(&cancel),
        &|_| {},
        |_, _| {
            let cancel = cancel.clone();
            async move {
                cancel.cancel();
                std::future::pending().await
            }
        },
    )
    .await;
    assert!(matches!(result, Err(ThkError::Cancelled)));
}

#[tokio::test]
async fn curseforge_precancel_leaves_destination_untouched() {
    let root = std::env::temp_dir().join(format!("thk-cf-cancel-{}", uuid::Uuid::new_v4()));
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = import_curseforge_zip(
        &DownloadManager::new().unwrap(),
        &root.join("missing.zip"),
        &root,
        None,
        Some(cancel),
        |_| {},
    )
    .await;
    assert!(matches!(result, Err(ThkError::Cancelled)));
    assert!(!root.exists());
}

#[tokio::test]
async fn curseforge_import_extracts_custom_overrides_and_skips_optional_files() {
    let root = std::env::temp_dir().join(format!("thk-cf-import-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("pack.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&source).unwrap());
    let manifest = serde_json::json!({
        "name": "Isolated regression fixture",
        "minecraft": {"version": "1.21.4", "modLoaders": [{"id": "fabric-0.16.10", "primary": true}]},
        "files": [{"projectID": 123, "fileID": 456, "required": false}],
        "overrides": "custom"
    });
    for (name, data) in [
        ("manifest.json", manifest.to_string()),
        ("custom/config/settings.txt", "fixture".into()),
        ("overrides/ignored.txt", "ignored".into()),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let dest = root.join("payload");
    let meta = import_curseforge_zip(
        &DownloadManager::new().unwrap(),
        &source,
        &dest,
        None,
        Some(CancellationToken::new()),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(meta.loader, LoaderType::Fabric);
    assert_eq!(meta.loader_version.as_deref(), Some("0.16.10"));
    assert_eq!(
        std::fs::read(dest.join("config/settings.txt")).unwrap(),
        b"fixture"
    );
    assert!(!dest.join("ignored.txt").exists());
    assert_eq!(std::fs::read_dir(dest.join("mods")).unwrap().count(), 0);
    std::fs::remove_dir_all(root).unwrap();
}
