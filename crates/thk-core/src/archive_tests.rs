use super::*;

struct TestArchive {
    root: PathBuf,
    path: PathBuf,
}

impl TestArchive {
    fn new(entries: &[(&str, &[u8])]) -> Self {
        let root = std::env::temp_dir().join(format!("thk-archive-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("test.zip");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        for (name, contents) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
        Self { root, path }
    }
}

impl Drop for TestArchive {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn prefix_precancel_does_not_create_destination_or_open_archive() {
    let archive = TestArchive::new(&[]);
    let dest = archive.root.join("payload");
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        extract_zip_prefix_cancellable(
            &archive.root.join("missing.zip"),
            "overrides",
            &dest,
            Some(&cancel)
        ),
        Err(ThkError::Cancelled)
    ));
    assert!(!dest.exists());
}

#[test]
fn prefix_extraction_preserves_client_override_precedence() {
    let archive = TestArchive::new(&[
        ("overrides/config/settings.txt", b"common"),
        ("client-overrides/config/settings.txt", b"client"),
        ("server-overrides/server.properties", b"server"),
        ("overrides-other/ignored.txt", b"ignored"),
    ]);
    let dest = archive.root.join("payload");
    let cancel = CancellationToken::new();
    for prefix in ["overrides", "client-overrides"] {
        let files =
            extract_zip_prefix_cancellable(&archive.path, prefix, &dest, Some(&cancel)).unwrap();
        assert_eq!(files, vec![dest.join("config/settings.txt")]);
    }
    assert_eq!(
        fs::read(dest.join("config/settings.txt")).unwrap(),
        b"client"
    );
    assert!(!dest.join("server.properties").exists());
    assert!(!dest.join("ignored.txt").exists());
}

#[test]
fn prefix_extraction_rejects_zip_traversal() {
    let archive = TestArchive::new(&[("overrides/../../escape.txt", b"unsafe")]);
    let dest = archive.root.join("payload");
    assert!(matches!(
        extract_zip_prefix_cancellable(&archive.path, "overrides", &dest, None),
        Err(ThkError::PathTraversal(_))
    ));
    assert!(!archive.root.join("escape.txt").exists());
}

#[test]
fn zip_copy_cancels_inside_a_large_entry_without_reading_next_chunk() {
    struct CancelAfterWrite {
        file: File,
        cancel: CancellationToken,
    }
    impl Write for CancelAfterWrite {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            let count = self.file.write(data)?;
            self.cancel.cancel();
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }

    let bytes = vec![42; 256 * 1024];
    let archive = TestArchive::new(&[("overrides/large.bin", &bytes)]);
    let mut zip = ZipArchive::new(File::open(&archive.path).unwrap()).unwrap();
    let mut entry = zip.by_index(0).unwrap();
    let cancel = CancellationToken::new();
    let output = archive.root.join("partial.bin");
    let mut writer = CancelAfterWrite {
        file: File::create(&output).unwrap(),
        cancel: cancel.clone(),
    };
    assert!(matches!(
        copy_cancellable(&mut entry, &mut writer, Some(&cancel)),
        Err(ThkError::Cancelled)
    ));
    drop(writer);
    let count = fs::metadata(&output).unwrap().len();
    assert!(count > 0 && count <= 64 * 1024);
    assert!(count < bytes.len() as u64);
}

fn write_raw_tar_entry(tar_bytes: &mut Vec<u8>, raw_path: &[u8], payload: &[u8], mode: u32) {
    let mut header = [0_u8; 512];
    let copy_len = raw_path.len().min(100);
    header[..copy_len].copy_from_slice(&raw_path[..copy_len]);
    let mode_str = format!("{mode:07o}\0");
    header[100..108].copy_from_slice(mode_str.as_bytes());
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    let size_str = format!("{:011o}\0", payload.len());
    header[124..136].copy_from_slice(size_str.as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    let chk_str = format!("{checksum:06o}\0 ");
    header[148..156].copy_from_slice(chk_str.as_bytes());

    tar_bytes.extend_from_slice(&header);
    tar_bytes.extend_from_slice(payload);
    let pad = (512 - (payload.len() % 512)) % 512;
    tar_bytes.resize(tar_bytes.len() + pad, 0);
}

#[test]
fn tarxz_roundtrip_and_traversal_and_precancel() {
    let root = std::env::temp_dir().join(format!("thk-tarxz-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();

    let valid_xz = root.join("runtime.tar.xz");
    {
        let mut tar_bytes = Vec::new();
        write_raw_tar_entry(&mut tar_bytes, b"bin/java", b"android-bionic-java", 0o755);
        write_raw_tar_entry(
            &mut tar_bytes,
            b"lib/server/libjvm.so",
            b"libjvm-arm64",
            0o755,
        );
        tar_bytes.resize(tar_bytes.len() + 1024, 0);

        let out = File::create(&valid_xz).unwrap();
        let mut xz = lzma_rust2::XzWriter::new(out, lzma_rust2::XzOptions::default()).unwrap();
        xz.write_all(&tar_bytes).unwrap();
        xz.finish().unwrap();
    }

    let cancel = CancellationToken::new();
    cancel.cancel();
    let cancelled_dest = root.join("cancelled");
    assert!(matches!(
        extract_tarxz_cancellable(&valid_xz, &cancelled_dest, Some(&cancel)),
        Err(ThkError::Cancelled)
    ));
    assert!(!cancelled_dest.exists());

    let dest = root.join("extracted");
    let files = extract_tarxz_safe(&valid_xz, &dest).unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        fs::read(dest.join("bin").join("java")).unwrap(),
        b"android-bionic-java"
    );
    assert_eq!(
        fs::read(dest.join("lib").join("server").join("libjvm.so")).unwrap(),
        b"libjvm-arm64"
    );

    let traversal_xz = root.join("traversal.tar.xz");
    {
        let mut tar_bytes = Vec::new();
        write_raw_tar_entry(&mut tar_bytes, b"../../escape.sh", b"bad", 0o755);
        tar_bytes.resize(tar_bytes.len() + 1024, 0);

        let out = File::create(&traversal_xz).unwrap();
        let mut xz = lzma_rust2::XzWriter::new(out, lzma_rust2::XzOptions::default()).unwrap();
        xz.write_all(&tar_bytes).unwrap();
        xz.finish().unwrap();
    }
    let bad_dest = root.join("bad_dest");
    assert!(matches!(
        extract_tarxz_safe(&traversal_xz, &bad_dest),
        Err(ThkError::PathTraversal(_))
    ));
    assert!(!root.join("escape.sh").exists());

    fs::remove_dir_all(&root).unwrap();
}
