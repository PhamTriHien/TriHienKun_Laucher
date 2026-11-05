use crate::modrinth::{ModrinthVersionRaw, MODRINTH_API_BASE};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thk_core::hash::sha1_file;
use thk_core::models::{ContentKind, InstalledContentRecord, ProviderSource};
use thk_core::paths::SafePath;
use thk_core::{Database, DownloadManager, ThkError, ThkResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalContentItem {
    pub file_name: String,
    pub base_file_name: String,
    pub enabled: bool,
    pub size_bytes: u64,
    pub title: String,
    pub version_number: String,
    pub provider: ProviderSource,
    pub project_id: Option<String>,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalWorldInfo {
    pub folder_name: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub icon_path: Option<PathBuf>,
    pub last_modified_unix: i64,
}

pub fn list_local_content(
    db: &Database,
    profile_id: &str,
    instance_dir: &Path,
    kind: ContentKind,
) -> ThkResult<Vec<LocalContentItem>> {
    let subdir = SafePath::resolve_child(instance_dir, kind.instance_subdir())?;
    if !subdir.is_dir() {
        return Ok(Vec::new());
    }

    let db_records = db.list_installed_content(profile_id, kind)?;
    let by_name: HashMap<String, InstalledContentRecord> = db_records
        .into_iter()
        .map(|r| (r.file_name.clone(), r))
        .collect();

    let mut out = Vec::new();
    for entry in fs::read_dir(&subdir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let raw_name = entry.file_name().to_string_lossy().into_owned();
        if raw_name.starts_with('.') {
            continue;
        }

        let (base_name, enabled) = match raw_name.strip_suffix(".disabled") {
            Some(stripped) => (stripped.to_string(), false),
            None => (raw_name.clone(), true),
        };

        let lower = base_name.to_ascii_lowercase();
        let valid_ext = match kind {
            ContentKind::Mod | ContentKind::Plugin => lower.ends_with(".jar"),
            ContentKind::Shader | ContentKind::ResourcePack => {
                lower.ends_with(".zip") || lower.ends_with(".jar")
            }
            ContentKind::Modpack => false,
        };
        if !valid_ext {
            continue;
        }

        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if let Some(rec) = by_name.get(&base_name) {
            out.push(LocalContentItem {
                file_name: raw_name,
                base_file_name: base_name,
                enabled,
                size_bytes,
                title: rec.title.clone(),
                version_number: rec.version_number.clone(),
                provider: rec.provider,
                project_id: (!rec.project_id.is_empty()).then(|| rec.project_id.clone()),
                icon_url: rec.icon_url.clone(),
            });
        } else {
            let (guessed_title, guessed_ver) = split_filename_title_and_version(&base_name);
            out.push(LocalContentItem {
                file_name: raw_name,
                base_file_name: base_name,
                enabled,
                size_bytes,
                title: guessed_title,
                version_number: guessed_ver,
                provider: ProviderSource::Local,
                project_id: None,
                icon_url: None,
            });
        }
    }

    out.sort_by_key(|a| a.title.to_lowercase());
    Ok(out)
}

pub fn toggle_local_content(
    db: &Database,
    profile_id: &str,
    instance_dir: &Path,
    kind: ContentKind,
    current_file_name: &str,
    enable: bool,
) -> ThkResult<String> {
    let subdir = SafePath::resolve_child(instance_dir, kind.instance_subdir())?;
    let clean_current = SafePath::validate_single_name(current_file_name)?;
    let src = SafePath::resolve_child(&subdir, clean_current)?;
    if !src.is_file() {
        return Err(ThkError::Provider(format!(
            "Content file not found: {clean_current}"
        )));
    }

    let base_name = clean_current
        .strip_suffix(".disabled")
        .unwrap_or(clean_current);
    let target_name = if enable {
        base_name.to_string()
    } else {
        format!("{base_name}.disabled")
    };

    if target_name != clean_current {
        let dst = SafePath::resolve_child(&subdir, &target_name)?;
        fs::rename(&src, &dst)?;
    }

    let _ = db.set_installed_content_enabled(profile_id, kind, base_name, enable);
    Ok(target_name)
}

pub fn delete_local_content(
    db: &Database,
    profile_id: &str,
    instance_dir: &Path,
    kind: ContentKind,
    file_name: &str,
) -> ThkResult<()> {
    let subdir = SafePath::resolve_child(instance_dir, kind.instance_subdir())?;
    let clean = SafePath::validate_single_name(file_name)?;
    let base_name = clean.strip_suffix(".disabled").unwrap_or(clean);

    for cand in [clean.to_string(), base_name.to_string(), format!("{base_name}.disabled")] {
        let p = SafePath::resolve_child(&subdir, &cand)?;
        if p.is_file() {
            fs::remove_file(p)?;
        }
    }

    db.remove_installed_content(profile_id, kind, base_name)?;
    Ok(())
}

pub async fn enrich_unmatched_via_modrinth_sha1(
    downloader: &DownloadManager,
    db: &Database,
    profile_id: &str,
    instance_dir: &Path,
    kind: ContentKind,
) -> ThkResult<usize> {
    let subdir = SafePath::resolve_child(instance_dir, kind.instance_subdir())?;
    if !subdir.is_dir() {
        return Ok(0);
    }

    let existing: HashMap<String, InstalledContentRecord> = db
        .list_installed_content(profile_id, kind)?
        .into_iter()
        .map(|r| (r.file_name.clone(), r))
        .collect();

    let mut hash_to_file: HashMap<String, (String, bool)> = HashMap::new();
    for entry in fs::read_dir(&subdir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let raw_name = entry.file_name().to_string_lossy().into_owned();
        let (base_name, enabled) = match raw_name.strip_suffix(".disabled") {
            Some(s) => (s.to_string(), false),
            None => (raw_name, true),
        };
        if existing.contains_key(&base_name) {
            continue;
        }
        if let Ok(digest) = sha1_file(&path) {
            hash_to_file.insert(digest, (base_name, enabled));
        }
    }

    if hash_to_file.is_empty() {
        return Ok(0);
    }

    let hashes: Vec<String> = hash_to_file.keys().cloned().collect();
    let resp = downloader
        .client()
        .post(format!("{MODRINTH_API_BASE}/version_files"))
        .json(&json!({
            "hashes": hashes,
            "algorithm": "sha1"
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(0);
    }

    let matched_map: HashMap<String, ModrinthVersionRaw> = resp.json().await.unwrap_or_default();
    let mut enriched_count = 0usize;

    for (sha1, ver) in matched_map {
        let Some((base_name, enabled)) = hash_to_file.get(&sha1) else {
            continue;
        };
        let (title, icon_url) = match crate::modrinth::get_modrinth_project(downloader, &ver.project_id).await {
            Ok(p) => (p.summary.title, p.summary.icon_url),
            Err(_) => (ver.name.clone(), None),
        };

        let rec = InstalledContentRecord {
            profile_id: profile_id.to_string(),
            content_kind: kind,
            file_name: base_name.clone(),
            sha1,
            provider: ProviderSource::Modrinth,
            project_id: ver.project_id,
            version_id: ver.id,
            title,
            version_number: ver.version_number,
            icon_url,
            enabled: *enabled,
            installed_at: Utc::now().timestamp(),
        };
        db.upsert_installed_content(&rec)?;
        enriched_count += 1;
    }

    Ok(enriched_count)
}

pub fn list_local_worlds(instance_dir: &Path) -> ThkResult<Vec<LocalWorldInfo>> {
    let saves_dir = SafePath::resolve_child(instance_dir, "saves")?;
    if !saves_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut worlds = Vec::new();
    for entry in fs::read_dir(&saves_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() || !path.join("level.dat").is_file() {
            continue;
        }
        let folder_name = entry.file_name().to_string_lossy().into_owned();
        let size_bytes = compute_dir_size(&path);
        let icon = path.join("icon.png");
        let last_modified_unix = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        worlds.push(LocalWorldInfo {
            display_name: folder_name.clone(),
            folder_name,
            size_bytes,
            icon_path: icon.is_file().then_some(icon),
            last_modified_unix,
        });
    }

    worlds.sort_by_key(|a| std::cmp::Reverse(a.last_modified_unix));
    Ok(worlds)
}

pub fn delete_local_world(instance_dir: &Path, world_folder_name: &str) -> ThkResult<()> {
    let saves_dir = SafePath::resolve_child(instance_dir, "saves")?;
    let clean_folder = SafePath::validate_single_name(world_folder_name)?;
    let target = SafePath::resolve_child(&saves_dir, clean_folder)?;
    if target.is_dir() {
        fs::remove_dir_all(target)?;
    }
    Ok(())
}

fn compute_dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += compute_dir_size(&p);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

pub fn split_filename_title_and_version(filename: &str) -> (String, String) {
    let stem = filename
        .strip_suffix(".disabled")
        .unwrap_or(filename)
        .strip_suffix(".jar")
        .or_else(|| filename.strip_suffix(".zip"))
        .unwrap_or(filename);

    let tokens: Vec<&str> = stem.split(['-', '_', '+']).filter(|t| !t.is_empty()).collect();
    let mut name_parts = Vec::new();
    let mut ver_parts = Vec::new();
    let mut in_version = false;

    for tok in tokens {
        let lower = tok.to_ascii_lowercase();
        if matches!(lower.as_str(), "fabric" | "forge" | "neoforge" | "quilt" | "mc") && !name_parts.is_empty() {
            continue;
        }
        let looks_like_ver = tok
            .trim_start_matches('v')
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit());
        if looks_like_ver && !name_parts.is_empty() {
            in_version = true;
        }
        if in_version {
            ver_parts.push(tok);
        } else {
            name_parts.push(tok);
        }
    }

    let title = if name_parts.is_empty() {
        stem.to_string()
    } else {
        name_parts.join(" ")
    };
    let version = if ver_parts.is_empty() {
        "local".to_string()
    } else {
        ver_parts.join("-")
    };
    (title, version)
}
