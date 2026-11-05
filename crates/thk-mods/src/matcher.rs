use crate::modrinth::{map_modrinth_version, ModrinthVersionRaw, MODRINTH_API_BASE};
use crate::provider::{ProjectSummary, ProjectVersion};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thk_core::hash::sha1_file;
use thk_core::models::{
    ContentKind, GameProfile, InstalledContentRecord, LoaderType, ProviderSource,
};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentUpdateCandidate {
    pub file_name: String,
    pub title: String,
    pub current_version: String,
    pub latest_version: ProjectVersion,
    pub summary: ProjectSummary,
}

pub async fn check_local_content_updates(
    downloader: &DownloadManager,
    db: &Database,
    profile: &GameProfile,
    kind: ContentKind,
    cf_api_key: Option<&str>,
) -> ThkResult<Vec<ContentUpdateCandidate>> {
    let subdir = SafePath::resolve_child(&profile.instance_path, kind.instance_subdir())?;
    if !subdir.is_dir() {
        return Ok(Vec::new());
    }

    let _ = enrich_unmatched_via_modrinth_sha1(
        downloader,
        db,
        &profile.id,
        &profile.instance_path,
        kind,
    )
    .await;

    let records = db.list_installed_content(&profile.id, kind)?;
    if records.is_empty() {
        return Ok(Vec::new());
    }

    let mut active_records = Vec::new();
    for rec in records {
        let enabled_path = subdir.join(&rec.file_name);
        let disabled_path = subdir.join(format!("{}.disabled", rec.file_name));
        if enabled_path.is_file() || disabled_path.is_file() {
            active_records.push(rec);
        }
    }
    if active_records.is_empty() {
        return Ok(Vec::new());
    }

    let mut candidates = Vec::new();

    let mr_by_hash: HashMap<String, &InstalledContentRecord> = active_records
        .iter()
        .filter(|r| r.provider == ProviderSource::Modrinth && !r.sha1.is_empty())
        .map(|r| (r.sha1.clone(), r))
        .collect();

    if !mr_by_hash.is_empty() {
        let loaders: Vec<String> = match kind {
            ContentKind::Mod => {
                if profile.loader == LoaderType::Vanilla {
                    Vec::new()
                } else {
                    vec![profile.loader.as_str().to_string()]
                }
            }
            ContentKind::Shader => vec![
                "iris".into(),
                "optifine".into(),
                "canvas".into(),
                "vanilla".into(),
            ],
            ContentKind::ResourcePack => vec!["minecraft".into()],
            ContentKind::Plugin => vec![
                "paper".into(),
                "purpur".into(),
                "folia".into(),
                "spigot".into(),
                "bukkit".into(),
            ],
            ContentKind::Modpack => Vec::new(),
        };

        let hashes: Vec<String> = mr_by_hash.keys().cloned().collect();
        let mut payload = json!({
            "hashes": hashes,
            "algorithm": "sha1",
            "game_versions": [profile.game_version.clone()],
        });
        if !loaders.is_empty() {
            payload["loaders"] = json!(loaders);
        }

        if let Ok(resp) = downloader
            .client()
            .post(format!("{MODRINTH_API_BASE}/version_files/update"))
            .json(&payload)
            .send()
            .await
        {
            if resp.status().is_success() {
                let updates: HashMap<String, ModrinthVersionRaw> =
                    resp.json().await.unwrap_or_default();
                for (old_sha1, raw_ver) in updates {
                    let Some(rec) = mr_by_hash.get(&old_sha1) else {
                        continue;
                    };
                    let latest = map_modrinth_version(raw_ver);
                    let latest_sha1 = latest
                        .files
                        .iter()
                        .find(|f| f.primary)
                        .or_else(|| latest.files.first())
                        .and_then(|f| f.sha1.as_deref());
                    if latest.id != rec.version_id
                        && latest.version_number != rec.version_number
                        && latest_sha1 != Some(rec.sha1.as_str())
                    {
                        candidates.push(ContentUpdateCandidate {
                            file_name: rec.file_name.clone(),
                            title: rec.title.clone(),
                            current_version: rec.version_number.clone(),
                            summary: ProjectSummary {
                                provider: ProviderSource::Modrinth,
                                project_id: rec.project_id.clone(),
                                slug: rec.project_id.clone(),
                                title: rec.title.clone(),
                                summary: String::new(),
                                author: String::new(),
                                downloads: 0,
                                icon_url: rec.icon_url.clone(),
                                categories: Vec::new(),
                                updated_at: None,
                                content_kind: kind,
                            },
                            latest_version: latest,
                        });
                    }
                }
            }
        }
    }

    let cf_loader = (kind == ContentKind::Mod && profile.loader != LoaderType::Vanilla)
        .then_some(profile.loader);
    for rec in active_records
        .iter()
        .filter(|r| r.provider == ProviderSource::CurseForge && !r.project_id.is_empty())
    {
        if let Ok(mut versions) = crate::curseforge::get_curseforge_versions(
            downloader,
            &rec.project_id,
            Some(&profile.game_version),
            cf_loader,
            cf_api_key,
        )
        .await
        {
            if let Some(latest) = versions.drain(..).next() {
                if latest.id != rec.version_id && latest.version_number != rec.version_number {
                    candidates.push(ContentUpdateCandidate {
                        file_name: rec.file_name.clone(),
                        title: rec.title.clone(),
                        current_version: rec.version_number.clone(),
                        summary: ProjectSummary {
                            provider: ProviderSource::CurseForge,
                            project_id: rec.project_id.clone(),
                            slug: rec.project_id.clone(),
                            title: rec.title.clone(),
                            summary: String::new(),
                            author: String::new(),
                            downloads: 0,
                            icon_url: rec.icon_url.clone(),
                            categories: Vec::new(),
                            updated_at: None,
                            content_kind: kind,
                        },
                        latest_version: latest,
                    });
                }
            }
        }
    }

    candidates.sort_by_key(|c| c.title.to_lowercase());
    Ok(candidates)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAddonFile {
    pub file_name: String,
    pub base_file_name: String,
    pub folder: String,
    pub enabled: bool,
    pub size_bytes: u64,
    pub title: String,
    pub version_number: String,
}

pub fn list_server_addon_files(
    server_dir: &Path,
    include_plugins: bool,
    include_mods: bool,
) -> ThkResult<Vec<ServerAddonFile>> {
    let mut folders = Vec::new();
    if include_plugins {
        folders.push("plugins");
    }
    if include_mods {
        folders.push("mods");
    }

    let mut out = Vec::new();
    for folder in folders {
        let subdir = SafePath::resolve_child(server_dir, folder)?;
        if !subdir.is_dir() {
            continue;
        }
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
                Some(s) => (s.to_string(), false),
                None => (raw_name.clone(), true),
            };
            if !base_name.to_ascii_lowercase().ends_with(".jar") {
                continue;
            }
            let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let (title, version_number) = split_filename_title_and_version(&base_name);
            out.push(ServerAddonFile {
                file_name: raw_name,
                base_file_name: base_name,
                folder: folder.to_string(),
                enabled,
                size_bytes,
                title,
                version_number,
            });
        }
    }

    out.sort_by(|a, b| {
        a.folder
            .cmp(&b.folder)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    Ok(out)
}

fn validate_server_addon_folder(folder: &str) -> ThkResult<&'static str> {
    match folder {
        "plugins" => Ok("plugins"),
        "mods" => Ok("mods"),
        other => Err(ThkError::InvalidInput(format!(
            "Unsupported server addon folder: {other}"
        ))),
    }
}

pub fn toggle_server_addon_file(
    server_dir: &Path,
    folder: &str,
    current_file_name: &str,
    enable: bool,
) -> ThkResult<String> {
    let valid_folder = validate_server_addon_folder(folder)?;
    let subdir = SafePath::resolve_child(server_dir, valid_folder)?;
    let clean_current = SafePath::validate_single_name(current_file_name)?;
    let src = SafePath::resolve_child(&subdir, clean_current)?;
    if !src.is_file() {
        return Err(ThkError::Provider(format!(
            "Addon file not found: {clean_current}"
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
    Ok(target_name)
}

pub fn delete_server_addon_file(
    server_dir: &Path,
    folder: &str,
    file_name: &str,
) -> ThkResult<()> {
    let valid_folder = validate_server_addon_folder(folder)?;
    let subdir = SafePath::resolve_child(server_dir, valid_folder)?;
    let clean = SafePath::validate_single_name(file_name)?;
    let base_name = clean.strip_suffix(".disabled").unwrap_or(clean);
    for cand in [
        clean.to_string(),
        base_name.to_string(),
        format!("{base_name}.disabled"),
    ] {
        let p = SafePath::resolve_child(&subdir, &cand)?;
        if p.is_file() {
            fs::remove_file(p)?;
        }
    }
    Ok(())
}

pub fn import_server_addon_jar(
    server_dir: &Path,
    folder: &str,
    external_jar: &Path,
) -> ThkResult<String> {
    let valid_folder = validate_server_addon_folder(folder)?;
    if !external_jar.is_file() {
        return Err(ThkError::InvalidInput("Selected JAR file does not exist.".into()));
    }
    let raw_name = external_jar
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ThkError::InvalidInput("Invalid JAR file name.".into()))?;
    let clean_name = SafePath::validate_single_name(raw_name)?;
    if !clean_name.to_ascii_lowercase().ends_with(".jar") {
        return Err(ThkError::InvalidInput(
            "Only .jar files can be imported into server plugins/mods.".into(),
        ));
    }
    let subdir = SafePath::resolve_child(server_dir, valid_folder)?;
    fs::create_dir_all(&subdir)?;
    let dst = SafePath::resolve_child(&subdir, clean_name)?;
    fs::copy(external_jar, &dst)?;
    Ok(clean_name.to_string())
}
