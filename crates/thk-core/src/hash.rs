use crate::error::{ThkError, ThkResult};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub fn sha1_bytes(data: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

pub fn sha1_file(path: &Path) -> ThkResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha256_file(path: &Path) -> ThkResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha512_file(path: &Path) -> ThkResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha512::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn verify_file_sha1(path: &Path, expected_hex: &str) -> ThkResult<()> {
    let actual = sha1_file(path)?;
    if !actual.eq_ignore_ascii_case(expected_hex.trim()) {
        return Err(ThkError::HashMismatch {
            file: path.display().to_string(),
            expected: expected_hex.trim().to_ascii_lowercase(),
            actual,
        });
    }
    Ok(())
}

pub fn verify_file_sha256(path: &Path, expected_hex: &str) -> ThkResult<()> {
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected_hex.trim()) {
        return Err(ThkError::HashMismatch {
            file: path.display().to_string(),
            expected: expected_hex.trim().to_ascii_lowercase(),
            actual,
        });
    }
    Ok(())
}

pub fn verify_file_sha512(path: &Path, expected_hex: &str) -> ThkResult<()> {
    let actual = sha512_file(path)?;
    if !actual.eq_ignore_ascii_case(expected_hex.trim()) {
        return Err(ThkError::HashMismatch {
            file: path.display().to_string(),
            expected: expected_hex.trim().to_ascii_lowercase(),
            actual,
        });
    }
    Ok(())
}
