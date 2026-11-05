use std::collections::HashSet;
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

pub fn probe_java_binary(java_bin: &Path, is_managed: bool) -> ThkResult<JavaInstallation> {
    if !java_bin.is_file() {
        return Err(ThkError::Java(format!(
            "Java executable does not exist: {}",
            java_bin.display()
        )));
    }

    let mut cmd = Command::new(java_bin);
    cmd.arg("-version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd
        .output()
        .map_err(|e| ThkError::Java(format!("Failed running java -version: {e}")))?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let (major_version, full_version, vendor) = parse_java_version_output(&combined)
        .ok_or_else(|| ThkError::Java(format!("Unrecognized java -version output: {combined}")))?;

    Ok(JavaInstallation {
        path: java_bin.to_path_buf(),
        major_version,
        full_version,
        vendor,
        is_managed,
    })
}

pub fn parse_java_version_output(output: &str) -> Option<(u32, String, String)> {
    let first_line = output.lines().find(|l| l.contains("version"))?;
    let start_q = first_line.find('"')? + 1;
    let rest = &first_line[start_q..];
    let end_q = rest.find('"')?;
    let ver_str = &rest[..end_q];

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
