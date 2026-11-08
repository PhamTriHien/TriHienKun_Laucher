use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use thk_core::{ThkError, ThkResult};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JavaInstallation {
    pub path: PathBuf,
    pub major_version: u32,
    pub full_version: String,
    pub vendor: String,
    pub is_managed: bool,
}

pub fn scan_java_installations(runtimes_dir: &Path) -> Vec<JavaInstallation> {
    let mut candidates = Vec::new();

    if runtimes_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(runtimes_dir) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                if let Some(bin) = find_java_executable_in_dir(&entry.path(), 4) {
                    candidates.push((bin, true));
                }
            }
        }
    }

    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        if let Some(bin) = find_java_executable_in_dir(Path::new(&java_home), 2) {
            candidates.push((bin, false));
        }
    }

    for sys_root in platform_java_search_roots() {
        if sys_root.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&sys_root) {
                for entry in entries.flatten() {
                    if let Some(bin) = find_java_executable_in_dir(&entry.path(), 4) {
                        candidates.push((bin, false));
                    }
                }
            }
        }
    }

    let mut seen = HashSet::new();
    let mut results = Vec::new();
    for (bin, is_managed) in candidates {
        let key = bin.canonicalize().unwrap_or_else(|_| bin.clone());
        if !seen.insert(key) {
            continue;
        }
        if let Ok(info) = probe_java_binary(&bin, is_managed) {
            results.push(info);
        }
    }

    results.sort_by(|a, b| {
        b.is_managed
            .cmp(&a.is_managed)
            .then_with(|| b.major_version.cmp(&a.major_version))
    });
    results
}

pub fn find_java_executable_in_dir(dir: &Path, max_depth: usize) -> Option<PathBuf> {
    let bin_name = if cfg!(windows) { "java.exe" } else { "java" };

    // Direct check for standard JDK/JRE and macOS .bundle/.jdk layouts
    let direct_candidates = [
        dir.join("bin").join(bin_name),
        dir.join("Contents").join("Home").join("bin").join(bin_name),
        dir.join("jre.bundle")
            .join("Contents")
            .join("Home")
            .join("bin")
            .join(bin_name),
    ];
    for cand in direct_candidates {
        if cand.is_file() {
            return Some(cand);
        }
    }

    if max_depth == 0 || !dir.is_dir() {
        return None;
    }

    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_java_executable_in_dir(&p, max_depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

pub fn resolve_java_home(java_bin: &Path) -> Option<PathBuf> {
    let bin_dir = java_bin.parent()?;
    if bin_dir
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("bin"))
    {
        bin_dir.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

pub fn build_java_ld_library_path(java_home: &Path, existing: Option<&str>) -> Option<String> {
    let rel_dirs = [
        "lib",
        "lib/jli",
        "lib/server",
        "lib/aarch64",
        "lib/aarch64/jli",
        "lib/aarch64/server",
        "jre/lib",
        "jre/lib/jli",
        "jre/lib/server",
        "jre/lib/aarch64",
        "jre/lib/aarch64/jli",
        "jre/lib/aarch64/server",
    ];

    let mut parts: Vec<String> = rel_dirs
        .into_iter()
        .map(|rel| {
            rel.split('/')
                .fold(java_home.to_path_buf(), |acc, seg| acc.join(seg))
        })
        .filter(|dir| dir.is_dir())
        .map(|dir| dir.to_string_lossy().into_owned())
        .collect();

    if let Some(prev) = existing.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(prev.to_string());
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(":"))
    }
}

#[cfg(target_os = "android")]
const ANDROID_JVM_SHIM_SO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/libthk_jvm_shim.so"));

#[cfg(target_os = "android")]
pub fn ensure_android_jvm_shim_library(fallback_dir: Option<&Path>) -> Option<PathBuf> {
    let base_dir = thk_core::AppPaths::resolve_default()
        .ok()
        .map(|p| p.root.join(".jvm_compat"))
        .or_else(|| fallback_dir.map(|d| d.join(".jvm_compat")))?;
    std::fs::create_dir_all(&base_dir).ok()?;
    let shim_path = base_dir.join("libthk_jvm_shim.so");
    let needs_write = match std::fs::metadata(&shim_path) {
        Ok(meta) => meta.len() != ANDROID_JVM_SHIM_SO.len() as u64,
        Err(_) => true,
    };
    if needs_write {
        let tmp_path = base_dir.join(format!(".libthk_jvm_shim.{}.tmp", std::process::id()));
        std::fs::write(&tmp_path, ANDROID_JVM_SHIM_SO).ok()?;
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755));
        }
        if std::fs::rename(&tmp_path, &shim_path).is_err() {
            let _ = std::fs::remove_file(&tmp_path);
            if !shim_path.is_file() {
                return None;
            }
        }
    } else {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&shim_path, std::fs::Permissions::from_mode(0o755));
    }
    Some(shim_path)
}

fn java_subprocess_env_vars(
    java_bin: &Path,
    work_dir: Option<&Path>,
) -> Vec<(&'static str, OsString)> {
    let mut vars = Vec::new();
    let java_home = resolve_java_home(java_bin);

    if let Some(ref home) = java_home {
        vars.push(("JAVA_HOME", home.as_os_str().to_os_string()));

        #[cfg(unix)]
        {
            let bin_dir = home.join("bin");
            let mut new_path = bin_dir.into_os_string();
            new_path.push(":");
            match std::env::var_os("PATH") {
                Some(existing) if !existing.is_empty() => new_path.push(existing),
                _ => new_path.push("/system/bin:/bin:/usr/bin"),
            }
            vars.push(("PATH", new_path));
        }

        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            let existing_ld = std::env::var("LD_LIBRARY_PATH").ok();
            if let Some(ld_path) = build_java_ld_library_path(home, existing_ld.as_deref()) {
                vars.push(("LD_LIBRARY_PATH", OsString::from(ld_path)));
            }
        }
    }

    #[cfg(target_os = "android")]
    {
        if let Some(shim_path) = ensure_android_jvm_shim_library(java_home.as_deref().or(work_dir))
        {
            let mut preload = shim_path.into_os_string();
            if let Some(existing) = std::env::var_os("LD_PRELOAD").filter(|s| !s.is_empty()) {
                preload.push(":");
                preload.push(existing);
            }
            vars.push(("LD_PRELOAD", preload));
        }

        let sandbox_home = work_dir
            .map(Path::to_path_buf)
            .or(java_home)
            .or_else(|| thk_core::platform_data_root().map(Path::to_path_buf));
        if let Some(home_dir) = sandbox_home {
            let tmp_dir = home_dir.join(".jvm_tmp");
            let _ = std::fs::create_dir_all(&tmp_dir);
            vars.push(("HOME", home_dir.into_os_string()));
            let tmp_os = tmp_dir.into_os_string();
            vars.push(("TMPDIR", tmp_os.clone()));
            vars.push(("TEMP", tmp_os.clone()));
            vars.push(("TMP", tmp_os));
        }
    }

    #[cfg(not(target_os = "android"))]
    let _ = work_dir;

    vars
}

pub fn configure_std_java_command_env(cmd: &mut Command, java_bin: &Path, work_dir: Option<&Path>) {
    for (k, v) in java_subprocess_env_vars(java_bin, work_dir) {
        cmd.env(k, v);
    }
}

pub fn configure_tokio_java_command_env(
    cmd: &mut tokio::process::Command,
    java_bin: &Path,
    work_dir: Option<&Path>,
) {
    for (k, v) in java_subprocess_env_vars(java_bin, work_dir) {
        cmd.env(k, v);
    }
}

pub fn android_jvm_system_property_args(work_dir: &Path) -> Vec<String> {
    #[cfg(target_os = "android")]
    {
        let tmp_dir = work_dir.join(".jvm_tmp");
        let _ = std::fs::create_dir_all(&tmp_dir);
        vec![
            format!("-Djava.io.tmpdir={}", tmp_dir.display()),
            format!("-Duser.home={}", work_dir.display()),
            format!("-Djna.tmpdir={}", tmp_dir.display()),
            format!("-Dio.netty.native.workdir={}", tmp_dir.display()),
        ]
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = work_dir;
        Vec::new()
    }
}

pub fn probe_java_binary(java_bin: &Path, is_managed: bool) -> ThkResult<JavaInstallation> {
    if !java_bin.is_file() {
        return Err(ThkError::Java(format!(
            "Java executable does not exist: {}",
            java_bin.display()
        )));
    }

    let mut cmd = Command::new(java_bin);
    configure_std_java_command_env(&mut cmd, java_bin, None);
    cmd.arg("-version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output().map_err(|e| {
        ThkError::Java(format!(
            "Failed running java -version ({}): {e}",
            java_bin.display()
        ))
    })?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let (major_version, full_version, vendor) =
        parse_java_version_output(&combined).ok_or_else(|| {
            ThkError::Java(format!(
                "Unrecognized java -version output (status {}): {}",
                output.status,
                combined.trim()
            ))
        })?;

    Ok(JavaInstallation {
        path: java_bin.to_path_buf(),
        major_version,
        full_version,
        vendor,
        is_managed,
    })
}

pub fn parse_java_version_output(output: &str) -> Option<(u32, String, String)> {
    let ver_str = output.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        let executable = words.next()?;
        if !executable.eq_ignore_ascii_case("openjdk") && !executable.eq_ignore_ascii_case("java") {
            return None;
        }
        let next = words.next()?;
        let version = if next == "version" {
            words.next()?
        } else {
            next
        };
        let version = version.trim_matches('"');
        version.chars().next()?.is_ascii_digit().then_some(version)
    })?;

    let major = if let Some(after_one) = ver_str.strip_prefix("1.") {
        after_one
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse::<u32>()
            .ok()?
    } else {
        ver_str
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse::<u32>()
            .ok()?
    };

    let lower = output.to_ascii_lowercase();
    let vendor = if lower.contains("temurin") || lower.contains("adoptium") {
        "Eclipse Temurin"
    } else if lower.contains("zulu") || lower.contains("azul") {
        "Azul Zulu"
    } else if lower.contains("graalvm") {
        "GraalVM"
    } else if lower.contains("microsoft") {
        "Microsoft OpenJDK"
    } else if lower.contains("corretto") {
        "Amazon Corretto"
    } else if lower.contains("liberica") || lower.contains("bellsoft") {
        "BellSoft Liberica"
    } else if lower.contains("openjdk") {
        "OpenJDK"
    } else {
        "Oracle / Java"
    }
    .to_string();

    Some((major, ver_str.to_string(), vendor))
}

#[cfg(target_os = "android")]
fn platform_java_search_roots() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(not(target_os = "android"))]
fn platform_java_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for env_key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Ok(base) = std::env::var(env_key) {
                let p = PathBuf::from(base);
                for sub in [
                    "Java",
                    "Eclipse Adoptium",
                    "Zulu",
                    "BellSoft",
                    "Microsoft",
                    "GraalVM",
                    "Programs\\Eclipse Adoptium",
                ] {
                    roots.push(p.join(sub));
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        roots.push(PathBuf::from("/usr/lib/jvm"));
        roots.push(PathBuf::from("/usr/java"));
    }
    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
        roots.push(PathBuf::from("/opt/homebrew/opt"));
        roots.push(PathBuf::from("/usr/local/opt"));
    }
    roots
}
