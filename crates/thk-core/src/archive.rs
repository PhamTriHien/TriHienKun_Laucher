use crate::error::{ThkError, ThkResult};
use crate::paths::SafePath;
use flate2::read::GzDecoder;
use lzma_rust2::XzReader;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use tar::Archive;
use tokio_util::sync::CancellationToken;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub fn extract_zip_safe(zip_path: &Path, dest_dir: &Path) -> ThkResult<Vec<PathBuf>> {
    extract_zip_filtered(zip_path, dest_dir, |_| true)
}

pub fn extract_zip_cancellable(
    zip_path: &Path,
    dest_dir: &Path,
    cancel: Option<&CancellationToken>,
) -> ThkResult<Vec<PathBuf>> {
    check_cancelled(cancel)?;
    fs::create_dir_all(dest_dir)?;
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut extracted = Vec::new();

    for i in 0..archive.len() {
        check_cancelled(cancel)?;
        let mut entry = archive.by_index(i)?;
        let enclosed = entry.enclosed_name().ok_or_else(|| {
            ThkError::PathTraversal(
                entry
                    .mangled_name()
                    .unwrap_or_else(|_| PathBuf::from("<invalid-zip-entry>")),
            )
        })?;
        let target = SafePath::resolve_child(dest_dir, &enclosed)?;
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&target)?;
            copy_cancellable(&mut entry, &mut out, cancel)?;
            out.flush()?;
            apply_unix_permissions(&target, entry.unix_mode());
            extracted.push(target);
        }
    }

    check_cancelled(cancel)?;
    Ok(extracted)
}

pub fn extract_zip_filtered<F>(
    zip_path: &Path,
    dest_dir: &Path,
    mut filter: F,
) -> ThkResult<Vec<PathBuf>>
where
    F: FnMut(&str) -> bool,
{
    fs::create_dir_all(dest_dir)?;
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut extracted = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let enclosed = entry.enclosed_name().ok_or_else(|| {
            ThkError::PathTraversal(
                entry
                    .mangled_name()
                    .unwrap_or_else(|_| PathBuf::from("<invalid-zip-entry>")),
            )
        })?;
        let rel_str = enclosed.to_string_lossy().replace('\\', "/");
        if !filter(&rel_str) {
            continue;
        }

        let target = SafePath::resolve_child(dest_dir, &enclosed)?;
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&target)?;
            io::copy(&mut entry, &mut out)?;
            out.flush()?;
            apply_unix_permissions(&target, entry.unix_mode());
            extracted.push(target);
        }
    }

    Ok(extracted)
}

pub fn extract_zip_prefix_safe(
    zip_path: &Path,
    prefix: &str,
    dest_dir: &Path,
) -> ThkResult<Vec<PathBuf>> {
    extract_zip_prefix_cancellable(zip_path, prefix, dest_dir, None)
}

/// Extracts cooperatively; callers must await the worker before cleaning its destination.
/// Cancellation can leave a partial file in the destination (use an isolated staging directory).
pub fn extract_zip_prefix_cancellable(
    zip_path: &Path,
    prefix: &str,
    dest_dir: &Path,
    cancel: Option<&CancellationToken>,
) -> ThkResult<Vec<PathBuf>> {
    check_cancelled(cancel)?;
    fs::create_dir_all(dest_dir)?;
    let clean_prefix = prefix.trim_matches('/').to_string() + "/";
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut extracted = Vec::new();

    for i in 0..archive.len() {
        check_cancelled(cancel)?;
        let mut entry = archive.by_index(i)?;
        let enclosed = entry.enclosed_name().ok_or_else(|| {
            ThkError::PathTraversal(
                entry
                    .mangled_name()
                    .unwrap_or_else(|_| PathBuf::from("<invalid-zip-entry>")),
            )
        })?;
        let rel_str = enclosed.to_string_lossy().replace('\\', "/");
        let Some(stripped) = rel_str.strip_prefix(&clean_prefix) else {
            continue;
        };
        if stripped.is_empty() {
            continue;
        }

        let target = SafePath::resolve_child(dest_dir, stripped)?;
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&target)?;
            copy_cancellable(&mut entry, &mut out, cancel)?;
            out.flush()?;
            extracted.push(target);
        }
    }
    check_cancelled(cancel)?;
    Ok(extracted)
}

fn check_cancelled(cancel: Option<&CancellationToken>) -> ThkResult<()> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }
    Ok(())
}

fn copy_cancellable(
    reader: &mut impl Read,
    writer: &mut impl Write,
    cancel: Option<&CancellationToken>,
) -> ThkResult<()> {
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        check_cancelled(cancel)?;
        let count = match reader.read(&mut buffer) {
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        check_cancelled(cancel)?;
        if count == 0 {
            return Ok(());
        }
        writer.write_all(&buffer[..count])?;
    }
}

pub const MAX_ZIP_ENTRY_BYTES: u64 = 32 * 1024 * 1024;

pub fn read_zip_entry_bytes(zip_path: &Path, entry_name: &str) -> ThkResult<Option<Vec<u8>>> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let result = match archive.by_name(entry_name) {
        Ok(entry) => {
            if entry.size() > MAX_ZIP_ENTRY_BYTES {
                return Err(ThkError::InvalidInput(format!(
                    "Archive entry '{entry_name}' exceeds {} MiB safety limit",
                    MAX_ZIP_ENTRY_BYTES / (1024 * 1024)
                )));
            }
            let cap = entry.size().min(256 * 1024) as usize;
            let mut bytes = Vec::with_capacity(cap);
            entry
                .take(MAX_ZIP_ENTRY_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_ZIP_ENTRY_BYTES {
                return Err(ThkError::InvalidInput(format!(
                    "Archive entry '{entry_name}' exceeds {} MiB decompressed safety limit",
                    MAX_ZIP_ENTRY_BYTES / (1024 * 1024)
                )));
            }
            Ok(Some(bytes))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(err) => Err(ThkError::Zip(err)),
    };
    result
}

pub fn read_zip_entry_string(zip_path: &Path, entry_name: &str) -> ThkResult<Option<String>> {
    match read_zip_entry_bytes(zip_path, entry_name)? {
        Some(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|e| ThkError::InvalidInput(format!("Invalid UTF-8 in {entry_name}: {e}"))),
        None => Ok(None),
    }
}

pub fn extract_targz_safe(archive_path: &Path, dest_dir: &Path) -> ThkResult<Vec<PathBuf>> {
    extract_targz_cancellable(archive_path, dest_dir, None)
}

pub fn extract_targz_cancellable(
    archive_path: &Path,
    dest_dir: &Path,
    cancel: Option<&CancellationToken>,
) -> ThkResult<Vec<PathBuf>> {
    check_cancelled(cancel)?;
    let file = BufReader::new(File::open(archive_path)?);
    let gz = GzDecoder::new(file);
    extract_tar_reader_cancellable(gz, dest_dir, cancel)
}

pub fn extract_tarxz_safe(archive_path: &Path, dest_dir: &Path) -> ThkResult<Vec<PathBuf>> {
    extract_tarxz_cancellable(archive_path, dest_dir, None)
}

pub fn extract_tarxz_cancellable(
    archive_path: &Path,
    dest_dir: &Path,
    cancel: Option<&CancellationToken>,
) -> ThkResult<Vec<PathBuf>> {
    check_cancelled(cancel)?;
    let file = BufReader::new(File::open(archive_path)?);
    let xz = XzReader::new(file, true);
    extract_tar_reader_cancellable(xz, dest_dir, cancel)
}

fn extract_tar_reader_cancellable<R: Read>(
    reader: R,
    dest_dir: &Path,
    cancel: Option<&CancellationToken>,
) -> ThkResult<Vec<PathBuf>> {
    check_cancelled(cancel)?;
    fs::create_dir_all(dest_dir)?;
    let mut archive = Archive::new(reader);
    let mut extracted = Vec::new();
    #[cfg(unix)]
    let mut pending_symlinks: Vec<(PathBuf, PathBuf)> = Vec::new();

    for entry_res in archive.entries()? {
        check_cancelled(cancel)?;
        let mut entry = entry_res?;
        let rel_path = entry.path()?.into_owned();
        let entry_type = entry.header().entry_type();
        if entry_type.is_hard_link() {
            continue;
        }
        if entry_type.is_symlink() {
            #[cfg(unix)]
            if let Some(link_dest) = entry.link_name()? {
                let target = SafePath::resolve_child(dest_dir, &rel_path)?;
                if is_safe_internal_symlink(dest_dir, &target, &link_dest) {
                    pending_symlinks.push((link_dest.into_owned(), target));
                }
            }
            continue;
        }
        let target = SafePath::resolve_child(dest_dir, &rel_path)?;
        if entry_type.is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry_type.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&target)?;
            copy_cancellable(&mut entry, &mut out, cancel)?;
            out.flush()?;
            if let Ok(mode) = entry.header().mode() {
                apply_unix_permissions(&target, Some(mode));
            }
            extracted.push(target);
        }
    }

    #[cfg(unix)]
    for (link_dest, target) in pending_symlinks {
        check_cancelled(cancel)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let _ = fs::remove_file(&target);
        std::os::unix::fs::symlink(&link_dest, &target)?;
        extracted.push(target);
    }

    check_cancelled(cancel)?;
    Ok(extracted)
}

pub fn is_safe_internal_symlink(dest_dir: &Path, link_location: &Path, link_target: &Path) -> bool {
    use std::path::Component;
    if link_target.is_absolute() || link_target.to_string_lossy().contains('\0') {
        return false;
    }
    let Ok(rel_from_root) = link_location.strip_prefix(dest_dir) else {
        return false;
    };
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    if let Some(parent) = rel_from_root.parent() {
        for c in parent.components() {
            if let Component::Normal(p) = c {
                parts.push(p.to_os_string());
            }
        }
    }
    for c in link_target.components() {
        match c {
            Component::CurDir => {}
            Component::Normal(p) => parts.push(p.to_os_string()),
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return false;
                }
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

pub fn compress_paths_to_zip(
    base_dir: &Path,
    relative_items: &[String],
    output_zip: &Path,
) -> ThkResult<()> {
    let out_file = File::create(output_zip)?;
    let mut zip = ZipWriter::new(out_file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for item in relative_items {
        let src = SafePath::resolve_child(base_dir, item)?;
        let Ok(meta) = fs::symlink_metadata(&src) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        add_path_recursive(&mut zip, base_dir, &src, opts)?;
    }

    zip.finish()?;
    Ok(())
}

fn add_path_recursive<W: Write + io::Seek>(
    zip: &mut ZipWriter<W>,
    base_dir: &Path,
    current: &Path,
    opts: SimpleFileOptions,
) -> ThkResult<()> {
    let meta = fs::symlink_metadata(current)?;
    let ft = meta.file_type();
    if ft.is_symlink() {
        return Ok(());
    }
    let rel = current
        .strip_prefix(base_dir)
        .map_err(|_| ThkError::PathTraversal(current.to_path_buf()))?;
    let rel_unix = rel.to_string_lossy().replace('\\', "/");

    if ft.is_dir() {
        if !rel_unix.is_empty() {
            zip.add_directory(format!("{rel_unix}/"), opts)?;
        }
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            add_path_recursive(zip, base_dir, &entry.path(), opts)?;
        }
    } else if ft.is_file() {
        zip.start_file(rel_unix, opts)?;
        let mut f = File::open(current)?;
        io::copy(&mut f, zip)?;
    }
    Ok(())
}

pub fn flatten_single_subdir(dir: &Path) -> ThkResult<()> {
    let entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    if entries.len() != 1 {
        return Ok(());
    }
    let only = entries[0].path();
    let only_meta = fs::symlink_metadata(&only)?;
    if only_meta.file_type().is_symlink() || !only_meta.is_dir() {
        return Ok(());
    }
    let parent = dir.parent().unwrap_or(dir);
    let temp_dir = parent.join(format!(".thk-flatten-{}", uuid::Uuid::new_v4()));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }
    fs::rename(&only, &temp_dir)?;
    for sub in fs::read_dir(&temp_dir)? {
        let sub = sub?;
        fs::rename(sub.path(), dir.join(sub.file_name()))?;
    }
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

#[cfg(unix)]
fn apply_unix_permissions(path: &Path, mode: Option<u32>) {
    use std::os::unix::fs::PermissionsExt;
    if let Some(m) = mode {
        if m != 0 {
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(m));
        }
    }
}

#[cfg(not(unix))]
fn apply_unix_permissions(_path: &Path, _mode: Option<u32>) {}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
