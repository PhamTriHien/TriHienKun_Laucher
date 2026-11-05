use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thk_core::archive::{compress_paths_to_zip, extract_zip_safe};
use thk_core::{SafePath, ThkError, ThkResult};

const MAX_EDITABLE_TEXT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhitelistEntry {
    pub uuid: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BannedPlayerEntry {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub expires: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerFileEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
}

pub fn is_eula_accepted(server_dir: &Path) -> bool {
    let Ok(eula_path) = SafePath::resolve_child(server_dir, "eula.txt") else {
        return false;
    };
    let Ok(raw) = fs::read_to_string(eula_path) else {
        return false;
    };
    raw.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#')
            && trimmed
                .split_once('=')
                .is_some_and(|(k, v)| k.trim() == "eula" && v.trim().eq_ignore_ascii_case("true"))
    })
}

pub fn write_eula_accepted(server_dir: &Path) -> ThkResult<()> {
    fs::create_dir_all(server_dir)?;
    let eula_path = SafePath::resolve_child(server_dir, "eula.txt")?;
    let content = "# Agreed via TriHienKun Laucher (https://aka.ms/MinecraftEULA)\neula=true\n";
    fs::write(eula_path, content)?;
    Ok(())
}

pub fn read_server_properties(server_dir: &Path) -> ThkResult<BTreeMap<String, String>> {
    let props_path = SafePath::resolve_child(server_dir, "server.properties")?;
    if !props_path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = fs::read_to_string(props_path)?;
    Ok(parse_properties_str(&raw))
}

pub fn parse_properties_str(raw: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            let key = k.trim().to_string();
            if !key.is_empty() {
                map.insert(key, v.trim().to_string());
            }
        }
    }
    map
}

pub fn write_server_properties_patch(
    server_dir: &Path,
    patch: &BTreeMap<String, String>,
) -> ThkResult<()> {
    fs::create_dir_all(server_dir)?;
    let props_path = SafePath::resolve_child(server_dir, "server.properties")?;
    let existing = if props_path.exists() {
        fs::read_to_string(&props_path)?
    } else {
        "#Minecraft server properties\n".to_string()
    };

    let updated = apply_properties_patch_str(&existing, patch);
    fs::write(props_path, updated)?;
    Ok(())
}

pub fn apply_properties_patch_str(existing: &str, patch: &BTreeMap<String, String>) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut out_lines: Vec<String> = Vec::new();

    for line in existing.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with('!') {
            if let Some((k, _)) = trimmed.split_once('=') {
                let key = k.trim();
                if let Some(new_val) = patch.get(key) {
                    let clean_val = sanitize_single_line(new_val);
                    out_lines.push(format!("{key}={clean_val}"));
                    seen.insert(key.to_string());
                    continue;
                }
            }
        }
        out_lines.push(line.to_string());
    }

    for (k, v) in patch {
        if !seen.contains(k) {
            let clean_key = sanitize_single_line(k);
            let clean_val = sanitize_single_line(v);
            if !clean_key.is_empty() {
                out_lines.push(format!("{clean_key}={clean_val}"));
            }
        }
    }

    let mut joined = out_lines.join("\n");
    joined.push('\n');
    joined
}

pub fn read_whitelist(server_dir: &Path) -> ThkResult<Vec<WhitelistEntry>> {
    let path = SafePath::resolve_child(server_dir, "whitelist.json")?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<Vec<WhitelistEntry>>(&raw).unwrap_or_default())
}

pub fn upsert_whitelist_player(
    server_dir: &Path,
    name: &str,
    uuid: &str,
) -> ThkResult<Vec<WhitelistEntry>> {
    let clean_name = sanitize_single_line(name);
    if clean_name.is_empty() {
        return Err(ThkError::InvalidInput("Player name cannot be empty".into()));
    }
    let mut list = read_whitelist(server_dir)?;
    if let Some(existing) = list
        .iter_mut()
        .find(|e| e.name.eq_ignore_ascii_case(&clean_name))
    {
        if !uuid.trim().is_empty() {
            existing.uuid = sanitize_single_line(uuid);
        }
    } else {
        list.push(WhitelistEntry {
            uuid: if uuid.trim().is_empty() {
                "00000000-0000-0000-0000-000000000000".to_string()
            } else {
                sanitize_single_line(uuid)
            },
            name: clean_name,
        });
    }

    let path = SafePath::resolve_child(server_dir, "whitelist.json")?;
    fs::write(path, serde_json::to_string_pretty(&list)?)?;
    Ok(list)
}

pub fn remove_whitelist_player(server_dir: &Path, name: &str) -> ThkResult<Vec<WhitelistEntry>> {
    let mut list = read_whitelist(server_dir)?;
    list.retain(|e| !e.name.eq_ignore_ascii_case(name.trim()));
    let path = SafePath::resolve_child(server_dir, "whitelist.json")?;
    fs::write(path, serde_json::to_string_pretty(&list)?)?;
    Ok(list)
}

pub fn read_banned_players(server_dir: &Path) -> ThkResult<Vec<BannedPlayerEntry>> {
    let path = SafePath::resolve_child(server_dir, "banned-players.json")?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<Vec<BannedPlayerEntry>>(&raw).unwrap_or_default())
}

pub fn add_banned_player(
    server_dir: &Path,
    name: &str,
    uuid: &str,
    reason: &str,
) -> ThkResult<Vec<BannedPlayerEntry>> {
    let clean_name = sanitize_single_line(name);
    if clean_name.is_empty() {
        return Err(ThkError::InvalidInput("Player name cannot be empty".into()));
    }
    let mut list = read_banned_players(server_dir)?;
    list.retain(|e| !e.name.eq_ignore_ascii_case(&clean_name));
    list.push(BannedPlayerEntry {
        uuid: if uuid.trim().is_empty() {
            "00000000-0000-0000-0000-000000000000".to_string()
        } else {
            sanitize_single_line(uuid)
        },
        name: clean_name,
        created: Utc::now().format("%Y-%m-%d %H:%M:%S %z").to_string(),
        source: "TriHienKun Laucher".to_string(),
        expires: "forever".to_string(),
        reason: if reason.trim().is_empty() {
            "Banned by an operator.".to_string()
        } else {
            sanitize_single_line(reason)
        },
    });
    let path = SafePath::resolve_child(server_dir, "banned-players.json")?;
    fs::write(path, serde_json::to_string_pretty(&list)?)?;
    Ok(list)
}

pub fn remove_banned_player(server_dir: &Path, name: &str) -> ThkResult<Vec<BannedPlayerEntry>> {
    let mut list = read_banned_players(server_dir)?;
    list.retain(|e| !e.name.eq_ignore_ascii_case(name.trim()));
    let path = SafePath::resolve_child(server_dir, "banned-players.json")?;
    fs::write(path, serde_json::to_string_pretty(&list)?)?;
    Ok(list)
}

pub fn list_server_dir(server_dir: &Path, sub_path: &str) -> ThkResult<Vec<ServerFileEntry>> {
    let target = if sub_path.trim().is_empty() || sub_path.trim() == "/" || sub_path.trim() == "." {
        server_dir.to_path_buf()
    } else {
        SafePath::resolve_child(server_dir, sub_path.trim_start_matches('/'))?
    };

    if !target.exists() {
        return Ok(Vec::new());
    }

    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let prefix = sub_path.trim().trim_matches('/').replace('\\', "/");

    for entry_res in fs::read_dir(&target)? {
        let entry = entry_res?;
        let name = entry.file_name().to_string_lossy().to_string();
        let meta = entry.metadata()?;
        let rel_path = if prefix.is_empty() || prefix == "." {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let item = ServerFileEntry {
            name,
            rel_path,
            is_dir: meta.is_dir(),
            size_bytes: if meta.is_file() { meta.len() } else { 0 },
        };
        if item.is_dir {
            dirs.push(item);
        } else {
            files.push(item);
        }
    }

    dirs.sort_by_key(|a| a.name.to_ascii_lowercase());
    files.sort_by_key(|a| a.name.to_ascii_lowercase());
    dirs.extend(files);
    Ok(dirs)
}

pub fn read_server_text_file(server_dir: &Path, rel_path: &str) -> ThkResult<String> {
    let target = SafePath::resolve_child(server_dir, rel_path.trim_start_matches('/'))?;
    let meta = fs::metadata(&target)?;
    if !meta.is_file() {
        return Err(ThkError::InvalidInput("Selected entry is not a regular file".into()));
    }
    if meta.len() > MAX_EDITABLE_TEXT_BYTES {
        return Err(ThkError::InvalidInput(format!(
            "File exceeds 8 MiB text editor limit ({} bytes)",
            meta.len()
        )));
    }
    let bytes = fs::read(&target)?;
    String::from_utf8(bytes)
        .map_err(|_| ThkError::InvalidInput("Selected file is binary (not valid UTF-8 text)".into()))
}

pub fn write_server_text_file(
    server_dir: &Path,
    rel_path: &str,
    content: &str,
) -> ThkResult<()> {
    let target = SafePath::resolve_child(server_dir, rel_path.trim_start_matches('/'))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target, content.as_bytes())?;
    Ok(())
}

pub async fn import_external_file_streaming(
    server_dir: &Path,
    sub_dir: &str,
    external_src: &Path,
) -> ThkResult<PathBuf> {
    let file_name = external_src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ThkError::InvalidInput("Invalid external source file name".into()))?;
    let clean_name = SafePath::validate_single_name(file_name)?;
    let dest_dir = if sub_dir.trim().is_empty() || sub_dir.trim() == "." {
        server_dir.to_path_buf()
    } else {
        SafePath::resolve_child(server_dir, sub_dir.trim_matches('/'))?
    };
    tokio::fs::create_dir_all(&dest_dir).await?;
    let dest_path = SafePath::resolve_child(&dest_dir, clean_name)?;
    tokio::fs::copy(external_src, &dest_path).await?;
    Ok(dest_path)
}

pub fn delete_server_entries(server_dir: &Path, rel_paths: &[String]) -> ThkResult<()> {
    for rel in rel_paths {
        let target = SafePath::resolve_child(server_dir, rel.trim_start_matches('/'))?;
        if !target.exists() {
            continue;
        }
        if target.is_dir() {
            fs::remove_dir_all(&target)?;
        } else {
            fs::remove_file(&target)?;
        }
    }
    Ok(())
}

pub fn compress_server_entries(
    server_dir: &Path,
    rel_paths: &[String],
    archive_name: &str,
) -> ThkResult<PathBuf> {
    let mut clean_name = SafePath::validate_single_name(archive_name)?.to_string();
    if !clean_name.to_ascii_lowercase().ends_with(".zip") {
        clean_name.push_str(".zip");
    }
    let out_zip = SafePath::resolve_child(server_dir, &clean_name)?;
    compress_paths_to_zip(server_dir, rel_paths, &out_zip)?;
    Ok(out_zip)
}

pub fn extract_server_zip(server_dir: &Path, rel_zip_path: &str) -> ThkResult<usize> {
    let zip_path = SafePath::resolve_child(server_dir, rel_zip_path.trim_start_matches('/'))?;
    let extracted = extract_zip_safe(&zip_path, server_dir)?;
    Ok(extracted.len())
}

fn sanitize_single_line(raw: &str) -> String {
    raw.chars()
        .filter(|&c| c != '\r' && c != '\n' && (c >= ' ' || c == '\t'))
        .collect::<String>()
        .trim()
        .to_string()
}
