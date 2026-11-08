use crate::config_files::read_server_properties;
#[cfg(not(target_os = "android"))]
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream};
use suppaftp::tokio_rustls::TlsConnector;
use suppaftp::types::{FileType, Mode};
use thk_core::models::DownloadProgress;
#[cfg(not(target_os = "android"))]
use thk_core::APP_ID;
use thk_core::{ThkError, ThkResult};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::lookup_host;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FtpDeployConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub remote_dir: String,
    pub use_ftps: bool,
    pub include_world: bool,
}

impl Default for FtpDeployConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 21,
            username: String::new(),
            remote_dir: "/".to_string(),
            use_ftps: true,
            include_world: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FtpDeploySummary {
    pub uploaded_files: usize,
    pub uploaded_bytes: u64,
    pub remote_dir: String,
}

#[cfg(not(target_os = "android"))]
fn ftp_keyring_entry(server_id: &str) -> ThkResult<Entry> {
    Entry::new(APP_ID, &format!("server-ftp:{server_id}"))
        .map_err(|e| ThkError::Server(format!("OS Keyring unavailable: {e}")))
}

#[cfg(not(target_os = "android"))]
pub fn save_server_ftp_password(server_id: &str, password: &str) -> ThkResult<()> {
    let entry = ftp_keyring_entry(server_id)?;
    if password.is_empty() {
        return delete_server_ftp_password(server_id);
    }
    entry
        .set_password(password)
        .map_err(|e| ThkError::Server(format!("Failed storing FTP password in OS Keyring: {e}")))
}

#[cfg(not(target_os = "android"))]
pub fn load_server_ftp_password(server_id: &str) -> ThkResult<Option<String>> {
    let entry = ftp_keyring_entry(server_id)?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(ThkError::Server(format!(
            "Failed reading FTP password from OS Keyring: {e}"
        ))),
    }
}

#[cfg(not(target_os = "android"))]
pub fn delete_server_ftp_password(server_id: &str) -> ThkResult<()> {
    let entry = ftp_keyring_entry(server_id)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(ThkError::Server(format!(
            "Failed deleting FTP password from OS Keyring: {e}"
        ))),
    }
}

#[cfg(target_os = "android")]
pub fn save_server_ftp_password(server_id: &str, password: &str) -> ThkResult<()> {
    if password.is_empty() {
        return delete_server_ftp_password(server_id);
    }
    thk_core::store_sandboxed_secret(&format!("server-ftp:{server_id}"), password)
}

#[cfg(target_os = "android")]
pub fn load_server_ftp_password(server_id: &str) -> ThkResult<Option<String>> {
    thk_core::load_sandboxed_secret(&format!("server-ftp:{server_id}"))
}

#[cfg(target_os = "android")]
pub fn delete_server_ftp_password(server_id: &str) -> ThkResult<()> {
    thk_core::delete_sandboxed_secret(&format!("server-ftp:{server_id}"))
}

pub fn normalize_remote_ftp_dir(raw: &str) -> ThkResult<String> {
    if raw.chars().any(|c| c.is_control()) {
        return Err(ThkError::InvalidInput(
            "Remote FTP directory contains invalid control characters".into(),
        ));
    }
    let normalized = raw.trim().replace('\\', "/");
    if normalized.is_empty() || normalized == "/" {
        return Ok("/".to_string());
    }

    let mut clean_segments = Vec::new();
    for seg in normalized.split('/') {
        let s = seg.trim();
        if s.is_empty() || s == "." {
            continue;
        }
        if s == ".." {
            return Err(ThkError::InvalidInput(
                "Remote FTP directory cannot contain '..' segments".into(),
            ));
        }
        clean_segments.push(s);
    }

    if clean_segments.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", clean_segments.join("/")))
    }
}

pub fn collect_deployable_server_files(
    server_dir: &Path,
    include_world: bool,
) -> ThkResult<Vec<(PathBuf, String, u64)>> {
    if !server_dir.is_dir() {
        return Err(ThkError::InvalidInput(format!(
            "Server directory does not exist: {}",
            server_dir.display()
        )));
    }

    let level_name = read_server_properties(server_dir)
        .ok()
        .and_then(|mut m| m.remove("level-name"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "world".to_string());

    let mut excluded_roots: HashSet<String> = ["logs", "crash-reports", ".git", "backups"]
        .into_iter()
        .map(String::from)
        .collect();

    if !include_world {
        for base in ["world", level_name.as_str()] {
            let b = base.to_ascii_lowercase();
            excluded_roots.insert(b.clone());
            excluded_roots.insert(format!("{b}_nether"));
            excluded_roots.insert(format!("{b}_the_end"));
        }
    }

    let mut files = Vec::new();
    collect_files_recursive(server_dir, server_dir, &excluded_roots, &mut files)?;
    files.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(files)
}

fn collect_files_recursive(
    root_dir: &Path,
    cur_dir: &Path,
    excluded_roots: &HashSet<String>,
    out: &mut Vec<(PathBuf, String, u64)>,
) -> ThkResult<()> {
    for entry_res in fs::read_dir(cur_dir)? {
        let entry = entry_res?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let lower_name = name.to_ascii_lowercase();
        if lower_name.starts_with(".thk-")
            || lower_name.ends_with(".part")
            || lower_name.ends_with(".staged")
            || lower_name.ends_with(".backup")
            || lower_name.ends_with(".failed")
            || lower_name.ends_with(".update-lock")
            || lower_name == "session.lock"
        {
            continue;
        }

        let rel = path
            .strip_prefix(root_dir)
            .map_err(|_| ThkError::PathTraversal(path.clone()))?;
        let rel_unix = rel.to_string_lossy().replace('\\', "/");

        if cur_dir == root_dir && meta.is_dir() && excluded_roots.contains(&lower_name) {
            continue;
        }

        if meta.is_dir() {
            collect_files_recursive(root_dir, &path, excluded_roots, out)?;
        } else if meta.is_file() {
            out.push((path, rel_unix, meta.len()));
        }
    }
    Ok(())
}

fn validate_ftp_credentials(config: &FtpDeployConfig) -> ThkResult<&str> {
    let host = config.host.trim();
    if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(ThkError::InvalidInput("Invalid FTP host address".into()));
    }
    if config.port == 0 {
        return Err(ThkError::InvalidInput(
            "FTP port must be between 1 and 65535".into(),
        ));
    }
    let user = config.username.trim();
    if user.is_empty() || user.chars().any(|c| c.is_control()) {
        return Err(ThkError::InvalidInput("Invalid FTP username".into()));
    }
    Ok(host)
}

async fn connect_authenticated_ftp(
    config: &FtpDeployConfig,
    password: &str,
) -> ThkResult<AsyncRustlsFtpStream> {
    let host = validate_ftp_credentials(config)?;
    if password
        .chars()
        .any(|c| c == '\r' || c == '\n' || c == '\0')
    {
        return Err(ThkError::InvalidInput(
            "FTP password contains invalid control characters".into(),
        ));
    }

    let addrs: Vec<SocketAddr> = timeout(Duration::from_secs(12), lookup_host((host, config.port)))
        .await
        .map_err(|_| ThkError::Server(format!("Timed out resolving FTP host '{host}'")))?
        .map_err(|e| ThkError::Server(format!("Could not resolve FTP host '{host}': {e}")))?
        .collect();

    if addrs.is_empty() {
        return Err(ThkError::Server(format!(
            "No IP addresses found for FTP host '{host}'"
        )));
    }

    let mut connected: Option<(AsyncRustlsFtpStream, SocketAddr)> = None;
    let mut last_err = String::new();
    for addr in addrs {
        match AsyncRustlsFtpStream::connect_timeout(addr, Duration::from_secs(15)).await {
            Ok(stream) => {
                connected = Some((stream, addr));
                break;
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }
    }

    let (mut ftp, peer_addr) = connected.ok_or_else(|| {
        ThkError::Server(format!(
            "Failed connecting to FTP server {}:{}: {last_err}",
            host, config.port
        ))
    })?;

    if config.use_ftps {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls_cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| ThkError::Server(format!("TLS protocol setup error: {e}")))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = AsyncRustlsConnector::from(TlsConnector::from(Arc::new(tls_cfg)));
        ftp = ftp
            .into_secure(connector, host)
            .await
            .map_err(|e| ThkError::Server(format!("Explicit FTPS TLS handshake failed: {e}")))?;
    }

    ftp.login(config.username.trim(), password)
        .await
        .map_err(|e| ThkError::Server(format!("FTP authentication failed: {e}")))?;

    ftp.transfer_type(FileType::Binary)
        .await
        .map_err(|e| ThkError::Server(format!("Failed setting FTP binary mode: {e}")))?;

    if peer_addr.is_ipv6() {
        ftp.set_mode(Mode::ExtendedPassive);
    } else {
        ftp.set_mode(Mode::Passive);
        ftp.set_passive_nat_workaround(true);
    }

    Ok(ftp)
}

async fn ensure_remote_dir_path(
    ftp: &mut AsyncRustlsFtpStream,
    remote_abs_dir: &str,
) -> ThkResult<()> {
    ftp.cwd("/")
        .await
        .map_err(|e| ThkError::Server(format!("FTP CWD '/' failed: {e}")))?;
    if remote_abs_dir == "/" {
        return Ok(());
    }
    for seg in remote_abs_dir.trim_start_matches('/').split('/') {
        if seg.is_empty() {
            continue;
        }
        if ftp.cwd(seg).await.is_err() {
            ftp.mkdir(seg).await.map_err(|e| {
                ThkError::Server(format!(
                    "Failed creating remote FTP directory '{seg}' in '{remote_abs_dir}': {e}"
                ))
            })?;
            ftp.cwd(seg).await.map_err(|e| {
                ThkError::Server(format!(
                    "Failed entering remote FTP directory '{seg}' in '{remote_abs_dir}': {e}"
                ))
            })?;
        }
    }
    Ok(())
}

pub async fn test_ftp_connection(config: &FtpDeployConfig, password: &str) -> ThkResult<String> {
    let remote_dir = normalize_remote_ftp_dir(&config.remote_dir)?;
    let mut ftp = connect_authenticated_ftp(config, password).await?;
    ensure_remote_dir_path(&mut ftp, &remote_dir).await?;
    let pwd = ftp.pwd().await.unwrap_or_else(|_| remote_dir.clone());
    let _ = ftp.quit().await;
    Ok(pwd)
}

pub async fn deploy_server_via_ftp<F>(
    server_dir: &Path,
    config: &FtpDeployConfig,
    password: &str,
    cancel: Option<CancellationToken>,
    on_progress: F,
) -> ThkResult<FtpDeploySummary>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ThkError::Cancelled);
    }

    let remote_base = normalize_remote_ftp_dir(&config.remote_dir)?;
    let server_dir_owned = server_dir.to_path_buf();
    let include_world = config.include_world;
    let files = tokio::task::spawn_blocking(move || {
        collect_deployable_server_files(&server_dir_owned, include_world)
    })
    .await
    .map_err(|e| ThkError::Process(format!("Failed scanning server directory: {e}")))??;

    if files.is_empty() {
        return Err(ThkError::InvalidInput(
            "Server directory contains no deployable files".into(),
        ));
    }

    let total_files = files.len();
    let total_bytes: u64 = files.iter().map(|(_, _, sz)| *sz).sum();

    let mut ftp = connect_authenticated_ftp(config, password).await?;
    ensure_remote_dir_path(&mut ftp, &remote_base).await?;

    let mut current_remote_dir = remote_base.clone();
    let mut uploaded_bytes = 0u64;
    let started = Instant::now();
    let mut chunk_buf = vec![0u8; 64 * 1024];

    for (idx, (local_path, rel_unix, _file_sz)) in files.iter().enumerate() {
        if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
            let _ = ftp.quit().await;
            return Err(ThkError::Cancelled);
        }

        let (rel_parent, file_name) = match rel_unix.rsplit_once('/') {
            Some((p, f)) => (p, f),
            None => ("", rel_unix.as_str()),
        };
        let target_remote_dir = if rel_parent.is_empty() {
            remote_base.clone()
        } else if remote_base == "/" {
            format!("/{rel_parent}")
        } else {
            format!("{remote_base}/{rel_parent}")
        };

        if current_remote_dir != target_remote_dir {
            ensure_remote_dir_path(&mut ftp, &target_remote_dir).await?;
            current_remote_dir = target_remote_dir;
        }

        let mut local_file = tokio::fs::File::open(local_path).await?;
        let mut data_stream = ftp.put_with_stream(file_name).await.map_err(|e| {
            ThkError::Server(format!("FTP STOR '{rel_unix}' failed to open stream: {e}"))
        })?;

        loop {
            if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
                drop(data_stream);
                return Err(ThkError::Cancelled);
            }
            let n = local_file.read(&mut chunk_buf).await?;
            if n == 0 {
                break;
            }
            data_stream
                .write_all(&chunk_buf[..n])
                .await
                .map_err(|e| ThkError::Server(format!("FTP write error on '{rel_unix}': {e}")))?;
            uploaded_bytes += n as u64;

            let elapsed = started.elapsed().as_secs_f64().max(0.05);
            let bps = (uploaded_bytes as f64 / elapsed) as u64;
            let pct = if total_bytes > 0 {
                ((uploaded_bytes as f64 / total_bytes as f64) * 100.0).clamp(0.0, 100.0) as f32
            } else {
                ((idx + 1) as f32 / total_files as f32) * 100.0
            };

            on_progress(DownloadProgress {
                phase: "ftp-deploy".into(),
                message: format!("Uploading {rel_unix}"),
                completed_files: idx,
                total_files,
                downloaded_bytes: uploaded_bytes,
                total_bytes,
                bytes_per_sec: bps,
                percent: pct,
            });
        }

        data_stream.finish().await.map_err(|e| {
            ThkError::Server(format!("FTP STOR '{rel_unix}' completion failed: {e}"))
        })?;
    }

    on_progress(DownloadProgress {
        phase: "ftp-deploy".into(),
        message: "Completed FTP upload".into(),
        completed_files: total_files,
        total_files,
        downloaded_bytes: uploaded_bytes,
        total_bytes,
        bytes_per_sec: (uploaded_bytes as f64 / started.elapsed().as_secs_f64().max(0.05)) as u64,
        percent: 100.0,
    });

    let _ = ftp.quit().await;

    Ok(FtpDeploySummary {
        uploaded_files: total_files,
        uploaded_bytes,
        remote_dir: remote_base,
    })
}
