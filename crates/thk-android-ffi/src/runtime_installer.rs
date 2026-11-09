use crate::protocol::{strip_zone_index, write_atomic_bytes};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use zip::ZipArchive;

pub const EXPECTED_RUNTIME_VERSION: &str = "thk-mobile-runtime-1.1.8-arm64-v1";
pub const ASSET_ENTRY_PATH: &str = "assets/mobile_runtime_arm64.zip";
pub const APK_FFI_SHIM_ENTRY_PATH: &str = "lib/arm64-v8a/libthk_android_ffi.so";

static EXTRACT_LOCK: Mutex<()> = Mutex::new(());
static SHIM_LOCK: Mutex<()> = Mutex::new(());
static TMP_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn mobile_runtime_root(files_dir: &Path) -> PathBuf {
    files_dir
        .join(".TriHienKunLaucher")
        .join("runtimes")
        .join("mobile_runtime")
}

pub fn stage_apk_ffi_shim(files_dir: &Path, apk_source_dir: &Path) -> Result<PathBuf, String> {
    let _guard = SHIM_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let compat_dir = files_dir.join(".TriHienKunLaucher").join(".jvm_compat");
    fs::create_dir_all(&compat_dir).map_err(|e| {
        format!(
            "Failed creating .jvm_compat at {}: {e}",
            compat_dir.display()
        )
    })?;
    let shim_path = compat_dir.join("libthk_android_ffi.so");

    let apk_file = File::open(apk_source_dir).map_err(|e| {
        format!(
            "Failed opening APK at {} for FFI shim staging: {e}",
            apk_source_dir.display()
        )
    })?;
    let mut apk_zip = ZipArchive::new(apk_file).map_err(|e| {
        format!(
            "Invalid APK zip header at {}: {e}",
            apk_source_dir.display()
        )
    })?;
    let mut entry = apk_zip
        .by_name(APK_FFI_SHIM_ENTRY_PATH)
        .map_err(|e| format!("Missing {APK_FFI_SHIM_ENTRY_PATH} in APK: {e}"))?;
    let expected_size = entry.size();
    if expected_size == 0 {
        return Err(format!("{APK_FFI_SHIM_ENTRY_PATH} in APK has zero size"));
    }

    if let Ok(meta) = fs::metadata(&shim_path) {
        if meta.is_file() && meta.len() == expected_size {
            set_mode_executable_0755(&shim_path);
            return Ok(shim_path);
        }
    }

    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_path = compat_dir.join(format!(
        ".libthk_android_ffi.{}.{seq}.tmp",
        std::process::id()
    ));

    let write_res = (|| -> Result<(), String> {
        let mut out = File::create(&tmp_path)
            .map_err(|e| format!("Failed creating {}: {e}", tmp_path.display()))?;
        let copied = io::copy(&mut entry, &mut out)
            .map_err(|e| format!("Failed copying {APK_FFI_SHIM_ENTRY_PATH}: {e}"))?;
        if copied != expected_size {
            return Err(format!(
                "Incomplete copy for {APK_FFI_SHIM_ENTRY_PATH}: got {copied}, expected {expected_size}"
            ));
        }
        out.flush()
            .map_err(|e| format!("Failed flushing {}: {e}", tmp_path.display()))?;
        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    set_mode_executable_0755(&tmp_path);
    make_writable_if_exists(&shim_path);
    if let Err(err) = fs::rename(&tmp_path, &shim_path) {
        #[cfg(not(unix))]
        if shim_path.exists() {
            let _ = fs::remove_file(&shim_path);
            if fs::rename(&tmp_path, &shim_path).is_ok() {
                return Ok(shim_path);
            }
        }
        let _ = fs::remove_file(&tmp_path);
        return Err(format!(
            "Failed renaming staged FFI shim to {}: {err}",
            shim_path.display()
        ));
    }

    Ok(shim_path)
}

pub fn ensure_mobile_runtime_extracted(
    files_dir: &Path,
    apk_source_dir: &Path,
    dns_servers: &[String],
) -> Result<PathBuf, String> {
    let _guard = EXTRACT_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let runtimes_dir = files_dir.join(".TriHienKunLaucher").join("runtimes");
    let root = runtimes_dir.join("mobile_runtime");

    if is_runtime_intact(&root) {
        write_resolv_conf(&root, dns_servers);
        return Ok(root);
    }

    fs::create_dir_all(&runtimes_dir).map_err(|e| {
        format!(
            "Failed creating runtimes dir {}: {e}",
            runtimes_dir.display()
        )
    })?;

    let stage = runtimes_dir.join(".mobile_runtime_stage");
    force_remove_tree(&stage)?;
    fs::create_dir_all(&stage)
        .map_err(|e| format!("Failed creating staging dir {}: {e}", stage.display()))?;

    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let bundle_tmp = runtimes_dir.join(format!(
        ".mobile_runtime_bundle.{}.{seq}.zip.tmp",
        std::process::id()
    ));
    let _ = fs::remove_file(&bundle_tmp);

    let extract_result = (|| -> Result<(), String> {
        {
            let apk_file = File::open(apk_source_dir).map_err(|e| {
                format!(
                    "Failed opening APK at {} for mobile runtime extraction: {e}",
                    apk_source_dir.display()
                )
            })?;
            let mut apk_zip = ZipArchive::new(apk_file).map_err(|e| {
                format!(
                    "Invalid APK zip archive at {}: {e}",
                    apk_source_dir.display()
                )
            })?;
            let mut asset_entry = apk_zip
                .by_name(ASSET_ENTRY_PATH)
                .map_err(|e| format!("Missing {ASSET_ENTRY_PATH} in APK: {e}"))?;
            let mut tmp_out = File::create(&bundle_tmp).map_err(|e| {
                format!(
                    "Failed creating temporary runtime archive {}: {e}",
                    bundle_tmp.display()
                )
            })?;
            io::copy(&mut asset_entry, &mut tmp_out).map_err(|e| {
                format!("Failed streaming {ASSET_ENTRY_PATH} to temporary archive: {e}")
            })?;
            tmp_out
                .flush()
                .map_err(|e| format!("Failed flushing {}: {e}", bundle_tmp.display()))?;
        }

        extract_bundle_zip_into_stage(&bundle_tmp, &stage)
    })();

    let _ = fs::remove_file(&bundle_tmp);

    if let Err(err) = extract_result {
        let _ = force_remove_tree(&stage);
        return Err(err);
    }

    let staged_ver = stage.join("version.txt");
    make_writable_if_exists(&staged_ver);
    {
        let mut ver_out = File::create(&staged_ver).map_err(|e| {
            format!(
                "Failed writing runtime version marker {}: {e}",
                staged_ver.display()
            )
        })?;
        ver_out
            .write_all(EXPECTED_RUNTIME_VERSION.as_bytes())
            .map_err(|e| format!("Failed writing {}: {e}", staged_ver.display()))?;
        ver_out
            .flush()
            .map_err(|e| format!("Failed flushing {}: {e}", staged_ver.display()))?;
    }
    set_unix_mode(&staged_ver, 0o644)?;

    force_remove_tree(&root)?;
    if let Err(err) = fs::rename(&stage, &root) {
        let _ = force_remove_tree(&stage);
        return Err(format!(
            "Failed finalizing mobile runtime directory at {}: {err}",
            root.display()
        ));
    }

    write_resolv_conf(&root, dns_servers);
    Ok(root)
}

pub fn write_resolv_conf(runtime_root: &Path, dns_servers: &[String]) {
    if !runtime_root.is_dir() && fs::create_dir_all(runtime_root).is_err() {
        return;
    }

    let mut seen = HashSet::new();
    let mut ordered = Vec::new();

    for raw in dns_servers
        .iter()
        .map(String::as_str)
        .chain(["1.1.1.1", "8.8.8.8"])
    {
        let clean = strip_zone_index(raw);
        if clean.is_empty() || clean.chars().any(|c| c.is_whitespace() || c.is_control()) {
            continue;
        }
        if seen.insert(clean.to_string()) {
            ordered.push(clean.to_string());
        }
    }

    let mut content = String::new();
    for server in ordered {
        content.push_str("nameserver ");
        content.push_str(&server);
        content.push('\n');
    }

    make_writable_if_exists(&runtime_root.join("resolv.conf"));
    let _ = write_atomic_bytes(
        runtime_root,
        ".resolv.conf.tmp",
        "resolv.conf",
        content.as_bytes(),
    );
}

fn is_runtime_intact(root: &Path) -> bool {
    let ver_file = root.join("version.txt");
    if !ver_file.is_file()
        || !root.join("lib").join("libpojavexec.so").is_file()
        || !root.join("lib").join("libng_gl4es.so").is_file()
        || !root
            .join("lwjgl3")
            .join("3.3.3")
            .join("lwjgl.jar")
            .is_file()
        || !root
            .join("lwjgl3")
            .join("3.3.3")
            .join("lwjgl-glfw.jar")
            .is_file()
    {
        return false;
    }
    let Ok(raw_ver) = fs::read_to_string(&ver_file) else {
        return false;
    };
    raw_ver.trim_start_matches('\u{feff}').trim() == EXPECTED_RUNTIME_VERSION
}

fn extract_bundle_zip_into_stage(bundle_zip_path: &Path, stage: &Path) -> Result<(), String> {
    let canon_stage = stage
        .canonicalize()
        .map_err(|e| format!("Failed canonicalizing stage {}: {e}", stage.display()))?;

    let bundle_file = File::open(bundle_zip_path).map_err(|e| {
        format!(
            "Failed opening staged runtime bundle {}: {e}",
            bundle_zip_path.display()
        )
    })?;
    let mut archive = ZipArchive::new(bundle_file).map_err(|e| {
        format!(
            "Corrupt mobile runtime zip {}: {e}",
            bundle_zip_path.display()
        )
    })?;

    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|e| format!("Failed reading runtime zip entry #{idx}: {e}"))?;
        let raw_name = entry
            .name()
            .map_err(|e| format!("Blocked invalid UTF-8 zip entry #{idx}: {e}"))?
            .into_owned();
        if raw_name.contains('\0') {
            return Err("Blocked zip entry containing NUL byte".to_string());
        }
        if entry.is_symlink() {
            return Err(format!(
                "Blocked symlink zip entry in mobile runtime bundle: {raw_name}"
            ));
        }

        let normalized = raw_name.replace('\\', "/");
        if normalized.is_empty() {
            continue;
        }
        if normalized.starts_with('/') {
            return Err(format!(
                "Blocked absolute zip entry in mobile runtime bundle: {raw_name}"
            ));
        }

        let enclosed = entry.enclosed_name().ok_or_else(|| {
            format!("Blocked unsafe zip entry path in mobile runtime bundle: {raw_name}")
        })?;
        let target = resolve_safe_stage_child(stage, &canon_stage, &enclosed)?;

        if entry.is_dir() || normalized.ends_with('/') {
            fs::create_dir_all(&target)
                .map_err(|e| format!("Failed creating runtime dir {}: {e}", target.display()))?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed creating parent dir {}: {e}", parent.display()))?;
        }
        make_writable_if_exists(&target);
        {
            let mut out = File::create(&target)
                .map_err(|e| format!("Failed creating {}: {e}", target.display()))?;
            io::copy(&mut entry, &mut out)
                .map_err(|e| format!("Failed extracting {}: {e}", target.display()))?;
            out.flush()
                .map_err(|e| format!("Failed flushing {}: {e}", target.display()))?;
        }

        let lower = normalized.to_ascii_lowercase();
        let mode = if lower.ends_with(".so") {
            0o555
        } else if lower.ends_with(".jar") || lower.ends_with(".dex") {
            0o444
        } else {
            0o644
        };
        set_unix_mode(&target, mode)?;
    }

    Ok(())
}

fn resolve_safe_stage_child(
    stage: &Path,
    canon_stage: &Path,
    rel: &Path,
) -> Result<PathBuf, String> {
    let mut clean_rel = PathBuf::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(part) => {
                let s = part.to_string_lossy();
                if s.is_empty() || s == "." || s == ".." || s.contains('\0') {
                    return Err(format!(
                        "Blocked invalid path component in runtime bundle: {}",
                        rel.display()
                    ));
                }
                clean_rel.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "Blocked path traversal in runtime bundle: {}",
                    rel.display()
                ));
            }
        }
    }
    if clean_rel.as_os_str().is_empty() {
        return Err("Empty relative path in runtime bundle".to_string());
    }

    let joined = stage.join(&clean_rel);
    let mut ancestor = joined.as_path();
    let mut tail_parts = Vec::new();
    while !ancestor.exists() {
        if let Some(name) = ancestor.file_name() {
            tail_parts.push(name.to_os_string());
        }
        match ancestor.parent() {
            Some(p) => ancestor = p,
            None => break,
        }
    }
    if ancestor.exists() {
        let mut resolved = ancestor
            .canonicalize()
            .map_err(|e| format!("Failed canonicalizing {}: {e}", ancestor.display()))?;
        for part in tail_parts.into_iter().rev() {
            resolved.push(part);
        }
        if !resolved.starts_with(canon_stage) {
            return Err(format!(
                "Blocked path escape outside stage: {}",
                rel.display()
            ));
        }
    }

    Ok(joined)
}

fn force_remove_tree(target: &Path) -> Result<(), String> {
    let Ok(meta) = fs::symlink_metadata(target) else {
        return Ok(());
    };
    let ft = meta.file_type();
    if ft.is_symlink() {
        let _ = fs::remove_file(target).or_else(|_| fs::remove_dir(target));
        return Ok(());
    }
    if ft.is_dir() {
        set_dir_writable(target);
        if let Ok(entries) = fs::read_dir(target) {
            for entry in entries.flatten() {
                force_remove_tree(&entry.path())?;
            }
        }
        set_dir_writable(target);
        fs::remove_dir(target)
            .map_err(|e| format!("Failed removing directory {}: {e}", target.display()))?;
    } else {
        make_writable_if_exists(target);
        fs::remove_file(target)
            .map_err(|e| format!("Failed removing file {}: {e}", target.display()))?;
    }
    Ok(())
}

fn set_mode_executable_0755(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn set_unix_mode(path: &Path, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|e| format!("Failed setting mode {mode:o} on {}: {e}", path.display()))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

fn set_dir_writable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
    #[cfg(not(unix))]
    {
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            if perms.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                let _ = fs::set_permissions(path, perms);
            }
        }
    }
}

fn make_writable_if_exists(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::symlink_metadata(path).is_ok() {
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        }
    }
    #[cfg(not(unix))]
    {
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            if perms.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                let _ = fs::set_permissions(path, perms);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn unique_test_dir(tag: &str) -> PathBuf {
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("thk-ffi-rt-{tag}-{}-{seq}", std::process::id()))
    }

    fn build_zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zw = ZipWriter::new(&mut cursor);
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            for (name, data) in entries {
                zw.start_file(*name, opts).unwrap();
                zw.write_all(data).unwrap();
            }
            zw.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn stages_ffi_shim_and_extracts_runtime_bundle_with_resolv_conf() {
        let root = unique_test_dir("extract");
        let files_dir = root.join("files");
        fs::create_dir_all(&root).unwrap();

        let runtime_zip = build_zip_bytes(&[
            ("version.txt", b"old-marker"),
            ("lib/libpojavexec.so", b"\x7fELFpojav"),
            ("lib/libng_gl4es.so", b"\x7fELFgl4es"),
            ("lwjgl3/3.3.3/lwjgl.jar", b"PKlwjgl"),
            ("lwjgl3/3.3.3/lwjgl-glfw.jar", b"PKglfw"),
            ("caciocavallo17/classes.dex", b"dex035"),
        ]);
        let apk_bytes = build_zip_bytes(&[
            (APK_FFI_SHIM_ENTRY_PATH, b"\x7fELFthk_android_ffi_arm64"),
            (ASSET_ENTRY_PATH, &runtime_zip),
        ]);
        let apk_path = root.join("base.apk");
        fs::write(&apk_path, &apk_bytes).unwrap();

        let shim = stage_apk_ffi_shim(&files_dir, &apk_path).unwrap();
        assert_eq!(fs::read(&shim).unwrap(), b"\x7fELFthk_android_ffi_arm64");
        let shim_again = stage_apk_ffi_shim(&files_dir, &apk_path).unwrap();
        assert_eq!(shim, shim_again);

        let dns = vec![
            "192.168.1.1%wlan0".to_string(),
            "2405:4802::1%rmnet0".to_string(),
            "1.1.1.1".to_string(),
        ];
        let rt_root = ensure_mobile_runtime_extracted(&files_dir, &apk_path, &dns).unwrap();
        assert_eq!(
            fs::read_to_string(rt_root.join("version.txt")).unwrap(),
            EXPECTED_RUNTIME_VERSION
        );
        assert_eq!(
            fs::read(rt_root.join("lib/libpojavexec.so")).unwrap(),
            b"\x7fELFpojav"
        );
        assert_eq!(
            fs::read_to_string(rt_root.join("resolv.conf")).unwrap(),
            "nameserver 192.168.1.1\nnameserver 2405:4802::1\nnameserver 1.1.1.1\nnameserver 8.8.8.8\n"
        );

        let rt_cached =
            ensure_mobile_runtime_extracted(&files_dir, &apk_path, &["9.9.9.9".into()]).unwrap();
        assert_eq!(rt_cached, rt_root);
        assert_eq!(
            fs::read_to_string(rt_root.join("resolv.conf")).unwrap(),
            "nameserver 9.9.9.9\nnameserver 1.1.1.1\nnameserver 8.8.8.8\n"
        );

        let _ = force_remove_tree(&root);
    }

    #[test]
    fn blocks_zip_slip_entries_inside_runtime_bundle() {
        let root = unique_test_dir("zipslip");
        let files_dir = root.join("files");
        fs::create_dir_all(&root).unwrap();

        let evil_runtime_zip = build_zip_bytes(&[
            ("lib/libpojavexec.so", b"\x7fELF"),
            ("../../escaped.txt", b"pwned"),
        ]);
        let apk_bytes = build_zip_bytes(&[(ASSET_ENTRY_PATH, &evil_runtime_zip)]);
        let apk_path = root.join("evil.apk");
        fs::write(&apk_path, &apk_bytes).unwrap();

        let err = ensure_mobile_runtime_extracted(&files_dir, &apk_path, &[]).unwrap_err();
        assert!(err.contains("Blocked"));
        assert!(!root.join("escaped.txt").exists());
        assert!(!mobile_runtime_root(&files_dir).exists());

        let _ = force_remove_tree(&root);
    }
}
