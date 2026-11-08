use std::fs;
use std::path::{Path, PathBuf};
use thk_core::models::{DownloadProgress, GameProfile, InstalledContentRecord};
use thk_core::{Database, ThkError, ThkResult};
use tokio_util::sync::CancellationToken;

static PACK_COMMIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) struct PackInstallation {
    staging: PathBuf,
    payload: PathBuf,
    destination: PathBuf,
    preserve: bool,
}

impl PackInstallation {
    pub(crate) fn prepare(destination: &Path) -> ThkResult<Self> {
        require_empty_destination(destination)?;
        let parent = destination.parent().ok_or_else(|| {
            ThkError::InvalidInput("Modpack destination must have a parent directory".into())
        })?;
        fs::create_dir_all(parent)?;
        let staging = parent.join(format!(".thk-pack-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&staging)?;
        let installation = Self {
            payload: staging.join("payload"),
            staging,
            destination: destination.to_path_buf(),
            preserve: false,
        };
        fs::create_dir(&installation.payload)?;
        Ok(installation)
    }

    pub(crate) fn payload(&self) -> &Path {
        &self.payload
    }

    #[cfg(test)]
    pub(crate) fn commit(
        self,
        db: &Database,
        profile: &GameProfile,
        installed_content: &[InstalledContentRecord],
        cancel: Option<&CancellationToken>,
    ) -> ThkResult<()> {
        self.commit_with_progress(db, profile, installed_content, cancel, |_| {})
    }

    pub(crate) fn commit_with_progress<F>(
        mut self,
        db: &Database,
        profile: &GameProfile,
        installed_content: &[InstalledContentRecord],
        cancel: Option<&CancellationToken>,
        on_progress: F,
    ) -> ThkResult<()>
    where
        F: Fn(DownloadProgress),
    {
        let _lock = PACK_COMMIT_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        check_cancelled(cancel)?;
        require_empty_destination(&self.destination)?;
        let canonical_destination = fs::canonicalize(&self.destination).ok();
        if profile.instance_path != self.destination
            || db.list_profiles()?.iter().any(|saved| {
                saved.id == profile.id
                    || saved.instance_path == self.destination
                    || canonical_destination.as_ref().is_some_and(|destination| {
                        fs::canonicalize(&saved.instance_path).as_ref().ok() == Some(destination)
                    })
            })
        {
            return Err(ThkError::InvalidInput("Conflicting modpack profile".into()));
        }
        let normalized_content: Vec<InstalledContentRecord> = installed_content
            .iter()
            .cloned()
            .map(|mut rec| {
                rec.profile_id = profile.id.clone();
                rec
            })
            .collect();
        let mut files = Vec::new();
        let mut directories = Vec::new();
        collect_payload(&self.payload, &self.payload, &mut directories, &mut files)?;
        let total_files = files.len();
        on_progress(DownloadProgress {
            phase: "Finalizing Modpack".into(),
            message: format!("Finalizing 0 / {total_files} files..."),
            completed_files: 0,
            total_files: total_files.max(1),
            downloaded_bytes: 0,
            total_bytes: 0,
            bytes_per_sec: 0,
            percent: 98.0,
        });
        let recovery = serde_json::json!({
            "destination": self.destination,
            "files": files,
            "directories": directories,
            "profile": profile,
            "installed_content_count": normalized_content.len(),
        });
        let recovery_file = fs::File::create(self.staging.join("recovery.json"))?;
        serde_json::to_writer_pretty(&recovery_file, &recovery)?;
        recovery_file.sync_all()?;
        let mut installed = Vec::new();
        let mut created_dirs = Vec::new();
        // Keep recovery on panic; rollback only removes paths created by this transaction.
        self.preserve = true;
        let result = (|| -> ThkResult<()> {
            match fs::create_dir(&self.destination) {
                Ok(()) => created_dirs.push(self.destination.clone()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    require_empty_destination(&self.destination)?;
                }
                Err(error) => return Err(error.into()),
            }
            for relative in &directories {
                check_cancelled(cancel)?;
                let path = self.destination.join(relative);
                fs::create_dir(&path)?;
                created_dirs.push(path);
            }
            for (idx, relative) in files.iter().enumerate() {
                check_cancelled(cancel)?;
                let src = self.payload.join(relative);
                let path = self.destination.join(relative);
                match fs::hard_link(&src, &path) {
                    Ok(()) => {
                        installed.push(path);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        return Err(error.into());
                    }
                    Err(_) => {
                        let mut input = fs::File::open(&src)?;
                        let mut output = fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&path)?;
                        installed.push(path);
                        std::io::copy(&mut input, &mut output)?;
                    }
                }
                let done = idx + 1;
                if done == total_files || done % 25 == 0 {
                    on_progress(DownloadProgress {
                        phase: "Finalizing Modpack".into(),
                        message: format!("Finalized {done} / {total_files} files..."),
                        completed_files: done,
                        total_files: total_files.max(1),
                        downloaded_bytes: 0,
                        total_bytes: 0,
                        bytes_per_sec: 0,
                        percent: 98.0 + (done as f32 / total_files.max(1) as f32) * 2.0,
                    });
                }
            }
            check_cancelled(cancel)?;
            db.upsert_profile_with_installed_content(profile, &normalized_content)
        })();
        if let Err(error) = result {
            return Err(self.rollback(error, &installed, &created_dirs));
        }
        self.preserve = false;
        Ok(())
    }

    fn rollback(
        &mut self,
        error: ThkError,
        installed: &[PathBuf],
        created_dirs: &[PathBuf],
    ) -> ThkError {
        let mut failures = Vec::new();
        for path in installed.iter().rev() {
            if let Err(error) = fs::remove_file(path) {
                failures.push(format!("{}: {error}", path.display()));
            }
        }
        for path in created_dirs.iter().rev() {
            if let Err(error) = fs::remove_dir(path) {
                failures.push(format!("{}: {error}", path.display()));
            }
        }
        self.preserve = !failures.is_empty();
        if self.preserve {
            return ThkError::Process(format!(
                "{error}; rollback failed: {}; recovery retained at {}",
                failures.join("; "),
                self.staging.display()
            ));
        }
        error
    }
}

impl Drop for PackInstallation {
    fn drop(&mut self) {
        if !self.preserve {
            if let Err(error) = fs::remove_dir_all(&self.staging) {
                tracing::warn!(path = %self.staging.display(), %error, "Could not clean modpack staging");
            }
        }
    }
}

fn check_cancelled(cancel: Option<&CancellationToken>) -> ThkResult<()> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }
    Ok(())
}

fn require_empty_destination(path: &Path) -> ThkResult<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            if fs::read_dir(path)?.next().transpose()?.is_none() {
                return Ok(());
            }
            Err(ThkError::InvalidInput(format!(
                "Choose an empty directory for the modpack: {}",
                path.display()
            )))
        }
        Ok(_) => Err(ThkError::InvalidInput(format!(
            "Modpack destination is not a regular directory: {}",
            path.display()
        ))),
    }
}

fn collect_payload(
    root: &Path,
    directory: &Path,
    directories: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> ThkResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|error| {
                ThkError::InvalidInput(format!("Invalid staged modpack path: {error}"))
            })?
            .to_path_buf();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            directories.push(relative);
            collect_payload(root, &path, directories, files)?;
        } else if kind.is_file() {
            files.push(relative);
        } else {
            return Err(ThkError::InvalidInput(format!(
                "Unsupported staged modpack entry: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "pack_install_tests.rs"]
mod tests;
