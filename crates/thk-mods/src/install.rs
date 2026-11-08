use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thk_core::hash::sha1_file;
use thk_core::models::{DownloadProgress, InstalledContentRecord};
use thk_core::paths::SafePath;
use thk_core::{Database, DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;

static COMMIT_LOCK: Mutex<()> = Mutex::new(());

struct StagingDirectory {
    path: PathBuf,
    preserve: bool,
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if !self.preserve {
            if let Err(err) = std::fs::remove_dir_all(&self.path) {
                tracing::warn!(path = %self.path.display(), %err, "Could not clean installation staging directory");
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn install_content_batch<F>(
    downloader: &DownloadManager,
    db: &Database,
    dest_dir: &Path,
    title: &str,
    mut tasks: Vec<DownloadTask>,
    pending: Vec<(PathBuf, InstalledContentRecord)>,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<Vec<PathBuf>>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    if tasks.len() != pending.len() {
        return Err(ThkError::InvalidInput(
            "Installation task/record mismatch".into(),
        ));
    }
    let mut names = HashSet::new();
    let mut projects = HashSet::new();
    for (_, rec) in &pending {
        SafePath::validate_single_name(&rec.file_name)?;
        if !names.insert(rec.file_name.to_lowercase())
            || !projects.insert((rec.provider.as_str(), &rec.project_id))
        {
            return Err(ThkError::InvalidInput(
                "Conflicting installation filenames or projects".into(),
            ));
        }
    }
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    let path = SafePath::resolve_child(dest_dir, format!(".thk-install-{}", uuid::Uuid::new_v4()))?;
    std::fs::create_dir(&path)?;
    let mut staging = StagingDirectory {
        path,
        preserve: false,
    };
    for (index, task) in tasks.iter_mut().enumerate() {
        task.dest = staging.path.join(index.to_string());
    }
    downloader
        .download_batch(title, tasks, 8, cancel.clone(), on_progress)
        .await?;
    let mut records: Vec<_> = pending.into_iter().map(|(_, rec)| rec).collect();
    let staging_for_hash = staging.path.clone();
    let record_count = records.len();
    let hashes = tokio::task::spawn_blocking(move || -> ThkResult<Vec<String>> {
        let mut out = Vec::with_capacity(record_count);
        for index in 0..record_count {
            out.push(sha1_file(&staging_for_hash.join(index.to_string()))?);
        }
        Ok(out)
    })
    .await
    .map_err(|e| ThkError::Process(format!("SHA-1 worker failed: {e}")))??;
    for (rec, digest) in records.iter_mut().zip(hashes) {
        rec.sha1 = digest;
    }
    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }
    // No await within the commit: cancellation cannot drop a half-applied file transaction.
    let _lock = COMMIT_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }
    commit_batch(db, dest_dir, &mut staging, &mut records)
}

fn commit_batch(
    db: &Database,
    dest_dir: &Path,
    staging: &mut StagingDirectory,
    records: &mut [InstalledContentRecord],
) -> ThkResult<Vec<PathBuf>> {
    let mut old_paths = HashSet::new();
    let mut destination_names = HashSet::new();
    let mut destinations = Vec::new();
    for rec in records.iter_mut() {
        let existing = db.list_installed_content(&rec.profile_id, rec.content_kind)?;
        let same_project = |item: &&InstalledContentRecord| {
            item.provider == rec.provider && item.project_id == rec.project_id
        };
        if let Some(old) = existing.iter().find(same_project) {
            rec.enabled = old.enabled;
        }
        let file_name = if rec.enabled {
            rec.file_name.clone()
        } else {
            format!("{}.disabled", rec.file_name)
        };
        if !destination_names.insert(file_name.to_lowercase()) {
            return Err(ThkError::InvalidInput(format!(
                "Conflicting installation destination: {file_name}"
            )));
        }
        let dest = SafePath::resolve_child(dest_dir, &file_name)?;
        if existing.iter().any(|item| {
            (item.file_name.eq_ignore_ascii_case(&rec.file_name)
                || format!("{}.disabled", item.file_name).eq_ignore_ascii_case(&rec.file_name)
                || item
                    .file_name
                    .eq_ignore_ascii_case(&format!("{}.disabled", rec.file_name)))
                && (item.provider != rec.provider || item.project_id != rec.project_id)
        }) {
            return Err(ThkError::InvalidInput(format!(
                "Filename belongs to another project: {}",
                rec.file_name
            )));
        }
        for old in existing.iter().filter(same_project) {
            let old_name = if old.enabled {
                old.file_name.clone()
            } else {
                format!("{}.disabled", old.file_name)
            };
            let path = SafePath::resolve_child(dest_dir, &old_name)?;
            match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_file() => {
                    old_paths.insert(path);
                }
                Ok(_) => {
                    return Err(ThkError::InvalidInput(format!(
                        "Not a regular mod file: {}",
                        path.display()
                    )))
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
        if dest.try_exists()? && !old_paths.contains(&dest) {
            return Err(ThkError::InvalidInput(format!(
                "Refusing to overwrite unmanaged file: {}",
                dest.display()
            )));
        }
        destinations.push(dest);
    }
    let backup_dir = staging.path.join("backup");
    std::fs::create_dir(&backup_dir)?;
    let backup_plan: Vec<_> = old_paths
        .into_iter()
        .enumerate()
        .map(|(index, original)| (original, backup_dir.join(index.to_string())))
        .collect();
    let recovery = serde_json::json!({
        "backups": backup_plan,
        "destinations": destinations,
        "records": records,
    });
    let mut recovery_file = std::fs::File::create(staging.path.join("recovery.json"))?;
    serde_json::to_writer_pretty(&mut recovery_file, &recovery)?;
    recovery_file.sync_all()?;
    drop(recovery_file);
    let mut backups = Vec::new();
    let mut installed = Vec::new();
    // Retain recovery data if unwinding interrupts a filesystem transaction.
    staging.preserve = true;
    let result = (|| -> ThkResult<()> {
        for (old, backup) in backup_plan {
            std::fs::rename(&old, &backup)?;
            backups.push((old, backup));
        }
        for (index, dest) in destinations.iter().enumerate() {
            let source = staging.path.join(index.to_string());
            let mut input = std::fs::File::open(source)?;
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dest)?;
            installed.push(dest.clone());
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
        }
        db.commit_installed_content(records)
    })();
    if let Err(error) = result {
        let mut rollback_errors = Vec::new();
        for path in installed.iter().rev() {
            if let Err(err) = std::fs::remove_file(path) {
                rollback_errors.push(format!("{}: {err}", path.display()));
            }
        }
        for (original, backup) in backups.iter().rev() {
            if let Err(err) = restore_backup(backup, original) {
                rollback_errors.push(format!("{}: {err}", original.display()));
            }
        }
        if !rollback_errors.is_empty() {
            staging.preserve = true;
            return Err(ThkError::Process(format!(
                "{error}; rollback failed: {}; recovery files retained at {}",
                rollback_errors.join("; "),
                staging.path.display()
            )));
        }
        staging.preserve = false;
        return Err(error);
    }
    staging.preserve = false;
    Ok(destinations)
}

fn restore_backup(backup: &Path, original: &Path) -> std::io::Result<()> {
    let mut input = std::fs::File::open(backup)?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(original)?;
    std::io::copy(&mut input, &mut output)?;
    output.sync_all()
}

#[cfg(test)]
#[path = "install_transaction_tests.rs"]
mod tests;
