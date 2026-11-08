use chrono::Utc;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thk_core::archive::{compress_paths_to_zip, extract_zip_safe};
use thk_core::{DownloadManager, SafePath, ThkError, ThkResult};

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

fn default_op_level() -> u8 {
    4
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpEntry {
    pub uuid: String,
    pub name: String,
    #[serde(default = "default_op_level")]
    pub level: u8,
    #[serde(rename = "bypassesPlayerLimit", default)]
    pub bypasses_player_limit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BannedIpEntry {
    pub ip: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub expires: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownPlayerEntry {
    pub uuid: String,
    pub name: String,
    pub expires_on: Option<String>,
    pub has_playerdata: bool,
    pub playerdata_bytes: u64,
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

pub fn offline_player_uuid(username: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{}", username.trim()).as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize().into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

#[derive(Deserialize)]
struct MojangProfileLookupResp {
    id: String,
    name: String,
}

fn format_hyphenated_uuid(raw_hex: &str) -> Option<String> {
    let clean: String = raw_hex.chars().filter(|c| *c != '-').collect();
    if clean.len() != 32 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let lower = clean.to_ascii_lowercase();
    Some(format!(
        "{}-{}-{}-{}-{}",
        &lower[0..8],
        &lower[8..12],
        &lower[12..16],
        &lower[16..20],
        &lower[20..32]
    ))
}

pub async fn resolve_player_uuid_for_server(
    downloader: &DownloadManager,
    server_dir: &Path,
    username: &str,
) -> (String, String) {
    let clean_name = sanitize_single_line(username);
    let online_mode = read_server_properties(server_dir)
        .ok()
        .and_then(|props| props.get("online-mode").cloned())
        .map(|v| !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true);

    if online_mode {
        let lookup_url = format!(
            "https://api.minecraftservices.com/minecraft/profile/lookup/name/{}",
            urlencoding::encode(&clean_name)
        );
        if let Ok(resp) = downloader
            .get_json::<MojangProfileLookupResp>(&lookup_url)
            .await
        {
            if let Some(formatted) = format_hyphenated_uuid(&resp.id) {
                let resolved_name = sanitize_single_line(&resp.name);
                return (
                    formatted,
                    if resolved_name.is_empty() {
                        clean_name
                    } else {
                        resolved_name
                    },
                );
            }
        }
        if let Ok(known) = read_known_players(server_dir) {
            if let Some(found) = known
                .into_iter()
                .find(|k| k.name.eq_ignore_ascii_case(&clean_name))
            {
                return (found.uuid, found.name);
            }
        }
    }

    (offline_player_uuid(&clean_name), clean_name)
}

pub fn list_runnable_server_targets(server_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let args_file = if cfg!(windows) {
        "win_args.txt"
    } else {
        "unix_args.txt"
    };

    for group_rel in [
        "libraries/net/minecraftforge/forge",
        "libraries/net/neoforged/neoforge",
        "libraries/net/neoforged/forge",
    ] {
        let group_dir = server_dir.join(group_rel);
        if let Ok(entries) = fs::read_dir(&group_dir) {
            for entry in entries.flatten() {
                let candidate = entry.path().join(args_file);
                if candidate.is_file() {
                    let ver = entry.file_name().to_string_lossy().into_owned();
                    out.push(format!("@{group_rel}/{ver}/{args_file}"));
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir(server_dir) {
        let mut jars = Vec::new();
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if !ft.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".jar") && !lower.ends_with("-installer.jar") {
                jars.push(name);
            }
        }
        jars.sort_by_key(|a| a.to_ascii_lowercase());
        out.extend(jars);
    }

    out
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
    let resolved_uuid = if uuid.trim().is_empty() {
        offline_player_uuid(&clean_name)
    } else {
        sanitize_single_line(uuid)
    };
    let mut list = read_whitelist(server_dir)?;
    if let Some(existing) = list.iter_mut().find(|e| {
        e.name.eq_ignore_ascii_case(&clean_name) || e.uuid.eq_ignore_ascii_case(&resolved_uuid)
    }) {
        existing.uuid = resolved_uuid;
        existing.name = clean_name;
    } else {
        list.push(WhitelistEntry {
            uuid: resolved_uuid,
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
    let resolved_uuid = if uuid.trim().is_empty() {
        offline_player_uuid(&clean_name)
    } else {
        sanitize_single_line(uuid)
    };
    let mut list = read_banned_players(server_dir)?;
    list.retain(|e| {
        !e.name.eq_ignore_ascii_case(&clean_name) && !e.uuid.eq_ignore_ascii_case(&resolved_uuid)
    });
    list.push(BannedPlayerEntry {
        uuid: resolved_uuid,
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

pub fn read_ops(server_dir: &Path) -> ThkResult<Vec<OpEntry>> {
    let path = SafePath::resolve_child(server_dir, "ops.json")?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    let mut list = serde_json::from_str::<Vec<OpEntry>>(&raw).unwrap_or_default();
    for item in &mut list {
        item.level = item.level.clamp(1, 4);
    }
    Ok(list)
}

pub fn upsert_op_player(
    server_dir: &Path,
    name: &str,
    uuid: &str,
    level: u8,
    bypasses_player_limit: bool,
) -> ThkResult<Vec<OpEntry>> {
    let clean_name = sanitize_single_line(name);
    if clean_name.is_empty() {
        return Err(ThkError::InvalidInput("Player name cannot be empty".into()));
    }
    let resolved_uuid = if uuid.trim().is_empty() {
        offline_player_uuid(&clean_name)
    } else {
        sanitize_single_line(uuid)
    };
    let clamped_level = level.clamp(1, 4);
    let mut list = read_ops(server_dir)?;
    if let Some(existing) = list.iter_mut().find(|e| {
        e.name.eq_ignore_ascii_case(&clean_name) || e.uuid.eq_ignore_ascii_case(&resolved_uuid)
    }) {
        existing.uuid = resolved_uuid;
        existing.name = clean_name;
        existing.level = clamped_level;
        existing.bypasses_player_limit = bypasses_player_limit;
    } else {
        list.push(OpEntry {
            uuid: resolved_uuid,
            name: clean_name,
            level: clamped_level,
            bypasses_player_limit,
        });
    }
    fs::create_dir_all(server_dir)?;
    let path = SafePath::resolve_child(server_dir, "ops.json")?;
    fs::write(path, serde_json::to_string_pretty(&list)?)?;
    Ok(list)
}

pub fn remove_op_player(server_dir: &Path, name: &str) -> ThkResult<Vec<OpEntry>> {
    let mut list = read_ops(server_dir)?;
    list.retain(|e| !e.name.eq_ignore_ascii_case(name.trim()));
    fs::create_dir_all(server_dir)?;
    let path = SafePath::resolve_child(server_dir, "ops.json")?;
    fs::write(path, serde_json::to_string_pretty(&list)?)?;
    Ok(list)
}

pub fn read_banned_ips(server_dir: &Path) -> ThkResult<Vec<BannedIpEntry>> {
    let path = SafePath::resolve_child(server_dir, "banned-ips.json")?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<Vec<BannedIpEntry>>(&raw).unwrap_or_default())
}

pub fn add_banned_ip(server_dir: &Path, ip: &str, reason: &str) -> ThkResult<Vec<BannedIpEntry>> {
    let clean_ip = sanitize_single_line(ip);
    let parsed_ip: std::net::IpAddr = clean_ip
        .parse()
        .map_err(|_| ThkError::InvalidInput(format!("Invalid IPv4/IPv6 address: '{clean_ip}'")))?;
    let canonical_ip = parsed_ip.to_string();

    let mut list = read_banned_ips(server_dir)?;
    list.retain(|e| !e.ip.eq_ignore_ascii_case(&canonical_ip));
    list.push(BannedIpEntry {
        ip: canonical_ip,
        created: Utc::now().format("%Y-%m-%d %H:%M:%S %z").to_string(),
        source: "TriHienKun Laucher".to_string(),
        expires: "forever".to_string(),
        reason: if reason.trim().is_empty() {
            "Banned by an operator.".to_string()
        } else {
            sanitize_single_line(reason)
        },
    });
    fs::create_dir_all(server_dir)?;
    let path = SafePath::resolve_child(server_dir, "banned-ips.json")?;
    fs::write(path, serde_json::to_string_pretty(&list)?)?;
    Ok(list)
}

pub fn remove_banned_ip(server_dir: &Path, ip: &str) -> ThkResult<Vec<BannedIpEntry>> {
    let target = ip.trim();
    let mut list = read_banned_ips(server_dir)?;
    list.retain(|e| !e.ip.eq_ignore_ascii_case(target));
    fs::create_dir_all(server_dir)?;
    let path = SafePath::resolve_child(server_dir, "banned-ips.json")?;
    fs::write(path, serde_json::to_string_pretty(&list)?)?;
    Ok(list)
}

#[derive(Deserialize)]
struct RawUserCacheEntry {
    name: String,
    uuid: String,
    #[serde(rename = "expiresOn", default)]
    expires_on: Option<String>,
}

fn resolve_server_level_dir(server_dir: &Path) -> ThkResult<PathBuf> {
    let level_name = read_server_properties(server_dir)
        .ok()
        .and_then(|m| m.get("level-name").cloned())
        .map(|s| s.trim().trim_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "world".to_string());
    SafePath::resolve_child(server_dir, &level_name)
}

pub fn read_known_players(server_dir: &Path) -> ThkResult<Vec<KnownPlayerEntry>> {
    let mut by_uuid: BTreeMap<String, KnownPlayerEntry> = BTreeMap::new();
    let level_dir = resolve_server_level_dir(server_dir).ok();
    let playerdata_dir = level_dir
        .as_ref()
        .and_then(|lvl| SafePath::resolve_child(lvl, "playerdata").ok());

    let lookup_dat_size = |uuid_str: &str| -> (bool, u64) {
        let Some(ref pd_dir) = playerdata_dir else {
            return (false, 0);
        };
        let Ok(dat_path) = SafePath::resolve_child(pd_dir, format!("{uuid_str}.dat")) else {
            return (false, 0);
        };
        match fs::metadata(dat_path) {
            Ok(meta) if meta.is_file() => (true, meta.len()),
            _ => (false, 0),
        }
    };

    if let Ok(cache_path) = SafePath::resolve_child(server_dir, "usercache.json") {
        if cache_path.is_file() {
            if let Ok(raw) = fs::read_to_string(cache_path) {
                if let Ok(entries) = serde_json::from_str::<Vec<RawUserCacheEntry>>(&raw) {
                    for entry in entries {
                        let Some(canon_uuid) = format_hyphenated_uuid(&entry.uuid) else {
                            continue;
                        };
                        let clean_name = sanitize_single_line(&entry.name);
                        if clean_name.is_empty() {
                            continue;
                        }
                        let (has_playerdata, playerdata_bytes) = lookup_dat_size(&canon_uuid);
                        by_uuid
                            .entry(canon_uuid.clone())
                            .or_insert(KnownPlayerEntry {
                                uuid: canon_uuid,
                                name: clean_name,
                                expires_on: entry.expires_on,
                                has_playerdata,
                                playerdata_bytes,
                            });
                    }
                }
            }
        }
    }

    for op in read_ops(server_dir).unwrap_or_default() {
        if let Some(canon_uuid) = format_hyphenated_uuid(&op.uuid) {
            let (has_playerdata, playerdata_bytes) = lookup_dat_size(&canon_uuid);
            by_uuid
                .entry(canon_uuid.clone())
                .or_insert(KnownPlayerEntry {
                    uuid: canon_uuid,
                    name: op.name,
                    expires_on: None,
                    has_playerdata,
                    playerdata_bytes,
                });
        }
    }
    for wl in read_whitelist(server_dir).unwrap_or_default() {
        if let Some(canon_uuid) = format_hyphenated_uuid(&wl.uuid) {
            let (has_playerdata, playerdata_bytes) = lookup_dat_size(&canon_uuid);
            by_uuid
                .entry(canon_uuid.clone())
                .or_insert(KnownPlayerEntry {
                    uuid: canon_uuid,
                    name: wl.name,
                    expires_on: None,
                    has_playerdata,
                    playerdata_bytes,
                });
        }
    }

    if let Some(ref pd_dir) = playerdata_dir {
        if let Ok(rd) = fs::read_dir(pd_dir) {
            for entry in rd.flatten() {
                let fname = entry.file_name().to_string_lossy().into_owned();
                let Some(stem) = fname.strip_suffix(".dat") else {
                    continue;
                };
                let Some(canon_uuid) = format_hyphenated_uuid(stem) else {
                    continue;
                };
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                if !meta.is_file() {
                    continue;
                }
                let size = meta.len();
                by_uuid
                    .entry(canon_uuid.clone())
                    .and_modify(|k| {
                        k.has_playerdata = true;
                        k.playerdata_bytes = size;
                    })
                    .or_insert_with(|| KnownPlayerEntry {
                        name: format!("Player-{}", &canon_uuid[..8]),
                        uuid: canon_uuid,
                        expires_on: None,
                        has_playerdata: true,
                        playerdata_bytes: size,
                    });
            }
        }
    }

    let mut out: Vec<KnownPlayerEntry> = by_uuid.into_values().collect();
    out.sort_by(|a, b| {
        b.has_playerdata.cmp(&a.has_playerdata).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });
    Ok(out)
}

pub fn delete_player_data(server_dir: &Path, uuid: &str) -> ThkResult<usize> {
    let canon_uuid = format_hyphenated_uuid(uuid.trim())
        .ok_or_else(|| ThkError::InvalidInput(format!("Invalid player UUID: '{}'", uuid.trim())))?;
    let level_dir = resolve_server_level_dir(server_dir)?;
    let candidates = [
        format!("playerdata/{canon_uuid}.dat"),
        format!("playerdata/{canon_uuid}.dat_old"),
        format!("playerdata/{canon_uuid}.dat_rsp"),
        format!("stats/{canon_uuid}.json"),
        format!("advancements/{canon_uuid}.json"),
    ];
    let mut removed = 0usize;
    for rel in candidates {
        let target = SafePath::resolve_child(&level_dir, &rel)?;
        if target.is_file() {
            fs::remove_file(target)?;
            removed += 1;
        }
    }
    Ok(removed)
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
        return Err(ThkError::InvalidInput(
            "Selected entry is not a regular file".into(),
        ));
    }
    if meta.len() > MAX_EDITABLE_TEXT_BYTES {
        return Err(ThkError::InvalidInput(format!(
            "File exceeds 8 MiB text editor limit ({} bytes)",
            meta.len()
        )));
    }
    let bytes = fs::read(&target)?;
    String::from_utf8(bytes).map_err(|_| {
        ThkError::InvalidInput("Selected file is binary (not valid UTF-8 text)".into())
    })
}

pub fn write_server_text_file(server_dir: &Path, rel_path: &str, content: &str) -> ThkResult<()> {
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
