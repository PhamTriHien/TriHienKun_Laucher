#![cfg(windows)]

use super::*;
use std::os::windows::process::CommandExt;

struct HelperCase {
    root: PathBuf,
    script: PathBuf,
    config_path: PathBuf,
    config: WindowsUpdateConfig,
    update: VerifiedUpdate,
}

impl HelperCase {
    fn new() -> Self {
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("Temp/opencode")
            .join(format!("thk-update-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        // Synthetic files exercise filesystem failures, not release authenticity.
        let source = root.join("download [test] ' tiếng Việt.exe");
        let destination = root.join("launcher [test] ' tiếng Việt.exe");
        std::fs::write(&source, b"invalid executable test fixture").unwrap();
        std::fs::write(&destination, b"original executable test fixture").unwrap();
        let update = VerifiedUpdate {
            sha256: thk_core::hash::sha256_file(&source).unwrap(),
            path: source,
        };
        let (script, config_path) =
            prepare_windows_update(&update, &destination, &root, i32::MAX as u32).unwrap();
        let config = serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        Self {
            root,
            script,
            config_path,
            config,
            update,
        }
    }

    fn run(&self) -> std::process::Output {
        std::process::Command::new("powershell.exe")
            .env_remove("PSModulePath")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&self.script)
            .arg("-ConfigPath")
            .arg(&self.config_path)
            .creation_flags(0x0800_0000)
            .output()
            .unwrap()
    }

    fn assert_original(&self) {
        assert_eq!(
            std::fs::read(&self.config.destination).unwrap(),
            b"original executable test fixture"
        );
    }

    fn log(&self) -> String {
        std::fs::read_to_string(format!("{}.log", self.config_path.display())).unwrap()
    }

    fn save_config(&self) {
        std::fs::write(&self.config_path, serde_json::to_vec(&self.config).unwrap()).unwrap();
    }
}

impl Drop for HelperCase {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.root) {
            eprintln!(
                "Could not clean isolated updater test {}: {error}",
                self.root.display()
            );
        }
    }
}

#[test]
fn apply_rejects_cache_changes_before_spawning_helper() {
    let case = HelperCase::new();
    std::fs::write(&case.update.path, b"modified cache fixture").unwrap();
    assert!(apply_downloaded_update(&case.update)
        .unwrap_err()
        .to_string()
        .contains("SHA-256 changed"));
    case.assert_original();
}

#[test]
fn helper_rechecks_staging_hash_and_cleans_only_its_staging_file() {
    let case = HelperCase::new();
    std::fs::write(&case.update.path, b"modified after helper preparation").unwrap();
    let output = case.run();
    assert!(!output.status.success());
    case.assert_original();
    assert!(!case.config.staged.exists());
    assert!(!case.config.backup.exists());
    assert!(!case.config.lock.exists());
    assert!(case.log().contains("SHA-256 does not match"));
}

#[test]
fn helper_preserves_existing_staging_backup_and_lock_files() {
    for conflict in ["staged", "backup", "lock"] {
        let case = HelperCase::new();
        let path = match conflict {
            "staged" => &case.config.staged,
            "backup" => &case.config.backup,
            _ => &case.config.lock,
        };
        std::fs::write(path, b"existing recovery data").unwrap();
        assert!(!case.run().status.success());
        case.assert_original();
        assert_eq!(std::fs::read(path).unwrap(), b"existing recovery data");
    }
}

#[test]
fn helper_restores_original_when_replacement_cannot_start() {
    let case = HelperCase::new();
    assert!(!case.run().status.success());
    case.assert_original();
    assert!(case.log().contains("Original executable restored."));
    assert_eq!(
        std::fs::read(&case.config.failed).unwrap(),
        std::fs::read(&case.update.path).unwrap()
    );
    assert!(!case.config.backup.exists());
    assert!(!case.config.lock.exists());
}

#[test]
fn helper_preserves_backup_when_rollback_destination_is_unavailable() {
    let mut case = HelperCase::new();
    // Force the rollback backup destination to fail without replacing any unrelated file.
    std::fs::create_dir(&case.config.failed).unwrap();
    case.config.failed = case.config.failed.join("missing-parent/failed.exe");
    case.save_config();
    assert!(!case.run().status.success());
    assert!(case.log().contains("Rollback failed:"));
    assert_eq!(
        std::fs::read(&case.config.backup).unwrap(),
        b"original executable test fixture"
    );
}

#[test]
fn helper_replace_failure_leaves_original_and_releases_lock() {
    let case = HelperCase::new();
    use std::os::windows::fs::OpenOptionsExt;
    let original = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&case.config.destination)
        .unwrap();
    assert!(!case.run().status.success());
    case.assert_original();
    assert!(!case.config.staged.exists());
    assert!(!case.config.lock.exists());
    drop(original);
}

#[test]
fn helper_replaces_with_real_executable_and_keeps_original_backup() {
    let mut case = HelperCase::new();
    let executable =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/where.exe");
    std::fs::copy(executable, &case.update.path).unwrap();
    case.config.sha256 = thk_core::hash::sha256_file(&case.update.path).unwrap();
    case.save_config();
    let output = case.run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        thk_core::hash::sha256_file(&case.config.destination).unwrap(),
        case.config.sha256
    );
    assert_eq!(
        std::fs::read(&case.config.backup).unwrap(),
        b"original executable test fixture"
    );
    assert!(case.log().contains("Update applied; started process"));
    assert!(!case.config.lock.exists());
    let log = case.log();
    let process_id: u32 = log
        .split("started process ")
        .nth(1)
        .unwrap()
        .split('.')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let output = std::process::Command::new("powershell.exe")
        .env_remove("PSModulePath")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(format!("$p = Get-Process -Id {process_id} -ErrorAction Ignore; if ($null -ne $p) {{ if (-not $p.WaitForExit(20000)) {{ throw 'Test executable did not exit' }}; $p.Dispose() }}; exit 0"))
        .creation_flags(0x0800_0000)
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
