use crate::error::{ThkError, ThkResult};
use crate::paths::SafePath;
use flate2::read::GzDecoder;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use tar::Archive;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub fn extract_zip_safe(zip_path: &Path, dest_dir: &Path) -> ThkResult<Vec<PathBuf>> {
    extract_zip_filtered(zip_path, dest_dir, |_| true)
}

pub fn extract_zip_filtered<F>(zip_path: &Path, dest_dir: &Path, mut filter: F) -> ThkResult<Vec<PathBuf>>
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

pub fn extract_zip_prefix_safe(zip_path: &Path, prefix: &str, dest_dir: &Path) -> ThkResult<Vec<PathBuf>> {
    fs::create_dir_all(dest_dir)?;
    let clean_prefix = prefix.trim_matches('/').to_string() + "/";
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
            io::copy(&mut entry, &mut out)?;
            out.flush()?;
            extracted.push(target);
        }
    }
    Ok(extracted)
}

pub fn read_zip_entry_bytes(zip_path: &Path, entry_name: &str) -> ThkResult<Option<Vec<u8>>> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let result = match archive.by_name(entry_name) {
        Ok(mut entry) => {
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes)?;
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
    fs::create_dir_all(dest_dir)?;
    let file = File::open(archive_path)?;
    let gz = GzDecoder::new(file);
    let mut archive = Archive::new(gz);
    let mut extracted = Vec::new();

    for entry_res in archive.entries()? {
        let mut entry = entry_res?;
        let rel_path = entry.path()?.into_owned();
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
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
            io::copy(&mut entry, &mut out)?;
            out.flush()?;
            if let Ok(mode) = entry.header().mode() {
                apply_unix_permissions(&target, Some(mode));
            }
            extracted.push(target);
        }
    }
    Ok(extracted)
}

pub fn compress_paths_to_zip(base_dir: &Path, relative_items: &[String], output_zip: &Path) -> ThkResult<()> {
    let out_file = File::create(output_zip)?;
    let mut zip = ZipWriter::new(out_file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for item in relative_items {
        let src = SafePath::resolve_child(base_dir, item)?;
        if !src.exists() {
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
    let rel = current
        .strip_prefix(base_dir)
        .map_err(|_| ThkError::PathTraversal(current.to_path_buf()))?;
    let rel_unix = rel.to_string_lossy().replace('\\', "/");

    if current.is_dir() {
        if !rel_unix.is_empty() {
            zip.add_directory(format!("{rel_unix}/"), opts)?;
        }
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            add_path_recursive(zip, base_dir, &entry.path(), opts)?;
        }
    } else if current.is_file() {
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
    if !only.is_dir() {
        return Ok(());
    }
    let temp_dir = dir.with_extension("flatten_tmp");
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
