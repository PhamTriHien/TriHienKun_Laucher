use crate::error::{ThkError, ThkResult};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const APP_ID: &str = "com.trihienkun.laucher";
pub const APP_DISPLAY_NAME: &str = "TriHienKun Laucher";
pub const APP_DATA_FOLDER: &str = ".TriHienKunLaucher";

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
        let base = dirs::config_dir()
            .or_else(dirs::data_dir)
            .ok_or_else(|| ThkError::InvalidInput("Cannot resolve OS application data directory".into()))?;
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
            } else if (ch == '-' || ch == '_' || ch.is_whitespace()) && !prev_dash && !out.is_empty() {
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
        Err(ThkError::InvalidInput("Too many colliding instance directories".into()))
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
    let freed = compute_dir_size_bytes(dir);
    for entry_res in fs::read_dir(dir)? {
        let entry = entry_res?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() && !ft.is_symlink() {
            let _ = fs::remove_dir_all(&path);
        } else {
            let _ = fs::remove_file(&path);
        }
    }
    fs::create_dir_all(dir)?;
    Ok(freed)
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
