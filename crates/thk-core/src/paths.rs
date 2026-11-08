use crate::error::{ThkError, ThkResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use uuid::Uuid;

pub const APP_ID: &str = "com.trihienkun.laucher";
pub const APP_DISPLAY_NAME: &str = "TriHienKun Laucher";
pub const APP_DATA_FOLDER: &str = ".TriHienKunLaucher";

static PLATFORM_DATA_ROOT: OnceLock<PathBuf> = OnceLock::new();

pub fn set_platform_data_root(root: PathBuf) {
    let _ = PLATFORM_DATA_ROOT.set(root);
}

pub fn platform_data_root() -> Option<&'static Path> {
    PLATFORM_DATA_ROOT.get().map(PathBuf::as_path)
}

pub fn android_bridge_dir() -> Option<PathBuf> {
    let base = platform_data_root()?;
    let dir = base.join(".thk_bridge");
    let _ = fs::create_dir_all(&dir);
    Some(dir)
}

pub fn android_bridge_open_url(url: &str) -> ThkResult<()> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err(ThkError::InvalidInput(
            "Only http:// and https:// URLs can be opened externally on Android".into(),
        ));
    }
    let Some(dir) = android_bridge_dir() else {
        return Err(ThkError::InvalidInput(
            "Android native bridge directory is unavailable".into(),
        ));
    };
    let tmp = dir.join("open_url.tmp");
    let dst = dir.join("open_url.txt");
    fs::write(&tmp, trimmed.as_bytes())?;
    fs::rename(&tmp, &dst)?;
    Ok(())
}

pub fn android_bridge_set_clipboard(text: &str) {
    let Some(dir) = android_bridge_dir() else {
        return;
    };
    let tmp = dir.join("clipboard.tmp");
    let dst = dir.join("clipboard.txt");
    if fs::write(&tmp, text.as_bytes()).is_ok() {
        let _ = fs::rename(&tmp, &dst);
    }
}

pub fn android_bridge_set_orientation(mode: &str) {
    let clean = mode.trim();
    if clean.is_empty() {
        return;
    }
    let Some(dir) = android_bridge_dir() else {
        return;
    };
    let saved = dir.join("saved_orientation.txt");
    let _ = fs::write(&saved, clean.as_bytes());
    let tmp = dir.join("orientation.tmp");
    let dst = dir.join("orientation.txt");
    if fs::write(&tmp, clean.as_bytes()).is_ok() {
        let _ = fs::rename(&tmp, &dst);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AndroidPermissionStatus {
    pub ignore_battery_optimizations: bool,
    pub can_draw_overlays: bool,
    pub notifications_enabled: bool,
    pub keep_alive_running: bool,
    pub overlay_showing: bool,
    pub surface_ready: bool,
    pub surface_width: u32,
    pub surface_height: u32,
    pub game_process_running: bool,
    pub safe_inset_left_px: u32,
    pub safe_inset_top_px: u32,
    pub safe_inset_right_px: u32,
    pub safe_inset_bottom_px: u32,
}

pub fn read_android_permission_status() -> AndroidPermissionStatus {
    let Some(dir) = android_bridge_dir() else {
        return AndroidPermissionStatus::default();
    };
    let path = dir.join("permissions.json");
    let Ok(raw) = fs::read_to_string(&path) else {
        return AndroidPermissionStatus::default();
    };
    serde_json::from_str::<AndroidPermissionStatus>(&raw).unwrap_or_default()
}

pub fn poll_android_focus_request() -> Option<String> {
    let dir = android_bridge_dir()?;
    let path = dir.join("focus_mode.txt");
    if !path.is_file() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    let _ = fs::remove_file(&path);
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn android_bridge_send_cmd(cmd: &serde_json::Value) -> ThkResult<()> {
    let Some(dir) = android_bridge_dir() else {
        return Err(ThkError::InvalidInput(
            "Android native bridge directory is unavailable".into(),
        ));
    };
    let payload = serde_json::to_vec(cmd)?;
    let id = Uuid::new_v4();
    let tmp = dir.join(format!("cmd_{id}.tmp"));
    let dst = dir.join(format!("cmd_{id}.json"));
    fs::write(&tmp, &payload)?;
    fs::rename(&tmp, &dst)?;
    Ok(())
}

pub fn store_sandboxed_secret(namespace_key: &str, secret: &str) -> ThkResult<()> {
    let paths = AppPaths::resolve_default()?;
    let cred_dir = paths.root.join(".credentials");
    fs::create_dir_all(&cred_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&cred_dir, fs::Permissions::from_mode(0o700));
    }
    let file_name = format!("{}.sec", hex::encode(namespace_key.as_bytes()));
    let target = SafePath::resolve_child(&cred_dir, &file_name)?;
    let tmp = SafePath::resolve_child(&cred_dir, format!("{file_name}.tmp"))?;
    fs::write(&tmp, secret.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, &target)?;
    Ok(())
}

pub fn load_sandboxed_secret(namespace_key: &str) -> ThkResult<Option<String>> {
    let paths = AppPaths::resolve_default()?;
    let cred_dir = paths.root.join(".credentials");
    let file_name = format!("{}.sec", hex::encode(namespace_key.as_bytes()));
    let target = SafePath::resolve_child(&cred_dir, &file_name)?;
    if !target.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&target)?;
    Ok(Some(raw))
}

pub fn delete_sandboxed_secret(namespace_key: &str) -> ThkResult<()> {
    let paths = AppPaths::resolve_default()?;
    let cred_dir = paths.root.join(".credentials");
    let file_name = format!("{}.sec", hex::encode(namespace_key.as_bytes()));
    let target = SafePath::resolve_child(&cred_dir, &file_name)?;
    if target.is_file() {
        fs::remove_file(&target)?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub db_file: PathBuf,
    pub instances_dir: PathBuf,
    pub launcher_dir: PathBuf,
    pub versions_dir: PathBuf,
    pub libraries_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub runtimes_dir: PathBuf,
    pub servers_dir: PathBuf,
    pub skins_dir: PathBuf,
    pub capes_dir: PathBuf,
    pub elytras_dir: PathBuf,
    pub backgrounds_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub authlib_jar: PathBuf,
}

impl AppPaths {
    pub fn resolve_default() -> ThkResult<Self> {
        if let Some(base) = platform_data_root() {
            return Self::from_root(base.join(APP_DATA_FOLDER));
        }
        if let Some(override_root) = std::env::var_os("THK_DATA_DIR") {
            if !override_root.is_empty() {
                return Self::from_root(PathBuf::from(override_root));
            }
        }
        let base = dirs::config_dir().or_else(dirs::data_dir).ok_or_else(|| {
            ThkError::InvalidInput("Cannot resolve OS application data directory".into())
        })?;
        Self::from_root(base.join(APP_DATA_FOLDER))
    }

    pub fn from_root(root: PathBuf) -> ThkResult<Self> {
        let launcher_dir = root.join("launcher");
        let paths = Self {
            db_file: root.join("thk_laucher.db"),
            instances_dir: root.join("instances"),
            versions_dir: launcher_dir.join("versions"),
            libraries_dir: launcher_dir.join("libraries"),
            assets_dir: launcher_dir.join("assets"),
            launcher_dir,
            runtimes_dir: root.join("runtimes"),
            servers_dir: root.join("servers"),
            skins_dir: root.join("account_skins"),
            capes_dir: root.join("account_capes"),
            elytras_dir: root.join("account_elytras"),
            backgrounds_dir: root.join("backgrounds"),
            cache_dir: root.join("cache"),
            authlib_jar: root.join("authlib-injector.jar"),
            root,
        };
        paths.ensure_directories()?;
        Ok(paths)
    }

    pub fn ensure_directories(&self) -> ThkResult<()> {
        for dir in [
            &self.root,
            &self.instances_dir,
            &self.launcher_dir,
            &self.versions_dir,
            &self.libraries_dir,
            &self.assets_dir,
            &self.runtimes_dir,
            &self.servers_dir,
            &self.skins_dir,
            &self.capes_dir,
            &self.elytras_dir,
            &self.backgrounds_dir,
            &self.cache_dir,
        ] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

pub struct SafePath;

impl SafePath {
    pub fn resolve_child(base: &Path, relative: impl AsRef<Path>) -> ThkResult<PathBuf> {
        let rel = relative.as_ref();
        let rel_str = rel.to_string_lossy();
        if rel_str.contains('\0') {
            return Err(ThkError::PathTraversal(rel.to_path_buf()));
        }

        let mut normalized_rel = PathBuf::new();
        for comp in rel.components() {
            match comp {
                Component::Normal(part) => {
                    let s = part.to_string_lossy();
                    if s.is_empty() || s == "." || s == ".." {
                        return Err(ThkError::PathTraversal(rel.to_path_buf()));
                    }
                    normalized_rel.push(part);
                }
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(ThkError::PathTraversal(rel.to_path_buf()));
                }
            }
        }

        let joined = base.join(&normalized_rel);
        if base.exists() {
            let canon_base = base.canonicalize()?;
            let mut existing_ancestor = joined.as_path();
            let mut tail_parts = Vec::new();
            while !existing_ancestor.exists() {
                if let Some(name) = existing_ancestor.file_name() {
                    tail_parts.push(name.to_os_string());
                }
                match existing_ancestor.parent() {
                    Some(p) => existing_ancestor = p,
                    None => break,
                }
            }
            if existing_ancestor.exists() {
                let mut resolved = existing_ancestor.canonicalize()?;
                for part in tail_parts.into_iter().rev() {
                    resolved.push(part);
                }
                if !resolved.starts_with(&canon_base) {
                    return Err(ThkError::PathTraversal(joined));
                }
                return Ok(joined);
            }
        }

        Ok(joined)
    }

    pub fn validate_single_name(name: &str) -> ThkResult<&str> {
        let trimmed = name.trim();
        if trimmed.is_empty()
            || trimmed == "."
            || trimmed == ".."
            || trimmed.contains('/')
            || trimmed.contains('\\')
            || trimmed.contains('\0')
            || trimmed.contains(':')
        {
            return Err(ThkError::PathTraversal(PathBuf::from(name)));
        }
        Ok(trimmed)
    }

    pub fn slugify_dir_name(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut prev_dash = false;
        for ch in input.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
                prev_dash = false;
            } else if (ch == '-' || ch == '_' || ch.is_whitespace())
                && !prev_dash
                && !out.is_empty()
            {
                out.push('-');
                prev_dash = true;
            }
        }
        let cleaned = out.trim_matches('-').to_string();
        if cleaned.is_empty() {
            "instance".to_string()
        } else {
            cleaned
        }
    }

    pub fn unique_child_dir(parent: &Path, preferred_name: &str) -> ThkResult<PathBuf> {
        fs::create_dir_all(parent)?;
        let slug = Self::slugify_dir_name(preferred_name);
        let first = Self::resolve_child(parent, &slug)?;
        if !first.exists() {
            return Ok(first);
        }
        for idx in 2..10_000 {
            let candidate = Self::resolve_child(parent, format!("{slug}-{idx}"))?;
            if !candidate.exists() {
                return Ok(candidate);
            }
        }
        Err(ThkError::InvalidInput(
            "Too many colliding instance directories".into(),
        ))
    }
}

pub fn compute_dir_size_bytes(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            total += compute_dir_size_bytes(&entry.path());
        } else if ft.is_file() {
            if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

pub fn clear_directory_contents(dir: &Path) -> ThkResult<u64> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
        return Ok(0);
    }
    let mut freed = 0u64;
    for entry_res in fs::read_dir(dir)? {
        let entry = entry_res?;
        freed += remove_entry_counting_bytes(&entry.path());
    }
    fs::create_dir_all(dir)?;
    Ok(freed)
}

fn remove_entry_counting_bytes(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    let ft = meta.file_type();
    if ft.is_symlink() {
        let _ = fs::remove_file(path).or_else(|_| fs::remove_dir(path));
        return 0;
    }
    if ft.is_dir() {
        let mut freed = 0u64;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                freed += remove_entry_counting_bytes(&entry.path());
            }
        }
        let _ = fs::remove_dir(path);
        freed
    } else if ft.is_file() {
        let len = meta.len();
        if fs::remove_file(path).is_ok() {
            len
        } else {
            0
        }
    } else {
        0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryArtifactKind {
    ModpackStaging,
    ContentBatchStaging,
    UpdaterBackup,
    UpdaterFailed,
    UpdaterLock,
}

#[derive(Debug, Clone)]
pub struct RecoveryArtifact {
    pub kind: RecoveryArtifactKind,
    pub path: PathBuf,
    pub size_bytes: u64,
}

pub fn scan_recovery_artifacts(
    paths: &AppPaths,
    extra_instance_dirs: &[PathBuf],
    extra_server_dirs: &[PathBuf],
    current_exe: Option<&Path>,
) -> Vec<RecoveryArtifact> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut push_unique = |kind: RecoveryArtifactKind, p: PathBuf, is_dir: bool| {
        if seen.insert(p.clone()) {
            let size_bytes = if is_dir {
                compute_dir_size_bytes(&p)
            } else {
                fs::metadata(&p).map(|m| m.len()).unwrap_or(0)
            };
            found.push(RecoveryArtifact {
                kind,
                path: p,
                size_bytes,
            });
        }
    };

    let mut pack_parents = vec![paths.instances_dir.clone()];
    for inst in extra_instance_dirs {
        if let Some(parent) = inst.parent() {
            pack_parents.push(parent.to_path_buf());
        }
    }
    for parent in pack_parents {
        let Ok(entries) = fs::read_dir(&parent) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_s = name.to_string_lossy();
            let p = entry.path();
            if name_s.starts_with(".thk-pack-") && p.join("recovery.json").is_file() {
                push_unique(RecoveryArtifactKind::ModpackStaging, p, true);
            }
        }
    }

    let mut instance_roots = Vec::new();
    if let Ok(entries) = fs::read_dir(&paths.instances_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                instance_roots.push(p);
            }
        }
    }
    instance_roots.extend(extra_instance_dirs.iter().cloned());
    for inst in instance_roots {
        for sub in ["mods", "shaderpacks", "resourcepacks"] {
            scan_install_staging_in_dir(&inst.join(sub), &mut push_unique);
        }
    }

    let mut server_roots = Vec::new();
    if let Ok(entries) = fs::read_dir(&paths.servers_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                server_roots.push(p);
            }
        }
    }
    server_roots.extend(extra_server_dirs.iter().cloned());
    for srv in server_roots {
        for sub in ["plugins", "mods"] {
            scan_install_staging_in_dir(&srv.join(sub), &mut push_unique);
        }
    }

    if let Some(exe) = current_exe {
        if let (Some(exe_dir), Some(exe_name)) = (
            exe.parent(),
            exe.file_name().map(|n| n.to_string_lossy().to_string()),
        ) {
            let prefix = format!(".{exe_name}.");
            let lock_name = format!(".{exe_name}.update-lock");
            if let Ok(entries) = fs::read_dir(exe_dir) {
                for entry in entries.flatten() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    let p = entry.path();
                    if fname == lock_name && p.is_file() {
                        push_unique(RecoveryArtifactKind::UpdaterLock, p, false);
                    } else if fname.starts_with(&prefix) && p.is_file() {
                        if fname.ends_with(".backup") {
                            push_unique(RecoveryArtifactKind::UpdaterBackup, p, false);
                        } else if fname.ends_with(".failed") {
                            push_unique(RecoveryArtifactKind::UpdaterFailed, p, false);
                        }
                    }
                }
            }
        }
    }

    found
}

fn scan_install_staging_in_dir<F>(dir: &Path, push_unique: &mut F)
where
    F: FnMut(RecoveryArtifactKind, PathBuf, bool),
{
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(".thk-install-") {
            let p = entry.path();
            if p.join("recovery.json").is_file() {
                push_unique(RecoveryArtifactKind::ContentBatchStaging, p, true);
            }
        }
    }
}

pub fn copy_profile_tree_selective(src_instance: &Path, dst_instance: &Path) -> ThkResult<usize> {
    fs::create_dir_all(dst_instance)?;
    if !src_instance.is_dir() {
        return Ok(0);
    }

    const COPY_DIRS: &[&str] = &[
        "mods",
        "shaderpacks",
        "resourcepacks",
        "config",
        "defaultconfigs",
        "kubejs",
        "scripts",
    ];
    const COPY_FILES: &[&str] = &["options.txt", "optionsof.txt", "servers.dat"];

    let mut copied_files = 0usize;
    for sub in COPY_DIRS {
        let src_sub = SafePath::resolve_child(src_instance, sub)?;
        if src_sub.is_dir() {
            let dst_sub = SafePath::resolve_child(dst_instance, sub)?;
            copied_files += copy_dir_recursive(&src_sub, &dst_sub)?;
        }
    }
    for fname in COPY_FILES {
        let src_file = SafePath::resolve_child(src_instance, fname)?;
        if src_file.is_file() {
            let dst_file = SafePath::resolve_child(dst_instance, fname)?;
            fs::copy(&src_file, &dst_file)?;
            copied_files += 1;
        }
    }
    Ok(copied_files)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> ThkResult<usize> {
    fs::create_dir_all(dst)?;
    let mut count = 0usize;
    for entry_res in fs::read_dir(src)? {
        let entry = entry_res?;
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let clean = SafePath::validate_single_name(&name_str)?;
        let dst_child = SafePath::resolve_child(dst, clean)?;
        if ft.is_dir() {
            count += copy_dir_recursive(&entry.path(), &dst_child)?;
        } else if ft.is_file() {
            fs::copy(entry.path(), &dst_child)?;
            count += 1;
        }
    }
    Ok(count)
}
