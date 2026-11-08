use crate::error::{ThkError, ThkResult};
use crate::models::DownloadProgress;
use futures_util::StreamExt;
use reqwest::Client;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const USER_AGENT: &str =
    "TriHienKun-Laucher/1.0.0 (Pure-Rust; https://github.com/PhamTriHien/TriHienKun_Laucher)";

#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub url: String,
    pub fallback_urls: Vec<String>,
    pub dest: PathBuf,
    pub expected_sha1: Option<String>,
    pub expected_sha256: Option<String>,
    pub expected_sha512: Option<String>,
    pub expected_size: Option<u64>,
    pub force_overwrite: bool,
    pub label: String,
}

impl DownloadTask {
    pub fn new(url: impl Into<String>, dest: PathBuf) -> Self {
        let url_str = url.into();
        let label = dest
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| url_str.clone());
        Self {
            url: url_str,
            fallback_urls: Vec::new(),
            dest,
            expected_sha1: None,
            expected_sha256: None,
            expected_sha512: None,
            expected_size: None,
            force_overwrite: false,
            label,
        }
    }
}

struct SpeedMeter {
    samples: Mutex<VecDeque<(Instant, u64)>>,
}

impl SpeedMeter {
    fn new() -> Self {
        Self {
            samples: Mutex::new(VecDeque::with_capacity(128)),
        }
    }

    fn record_delta(&self, bytes_delta: u64) -> u64 {
        let now = Instant::now();
        let window = Duration::from_secs(2);
        let mut guard = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        if bytes_delta > 0 {
            guard.push_back((now, bytes_delta));
        }
        while let Some(&(ts, _)) = guard.front() {
            if now.duration_since(ts) > window {
                guard.pop_front();
            } else {
                break;
            }
        }
        let sum: u64 = guard.iter().map(|(_, b)| *b).sum();
        if let Some(&(oldest, _)) = guard.front() {
            let elapsed = now.duration_since(oldest).as_secs_f64().max(0.2);
            (sum as f64 / elapsed) as u64
        } else {
            0
        }
    }
}

#[derive(Clone)]
pub struct DownloadManager {
    client: Client,
    max_concurrency: Arc<AtomicUsize>,
}

const RETRY_BACKOFFS_MS: [u64; 3] = [150, 350, 750];
const METADATA_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const STREAM_CHUNK_IDLE_TIMEOUT: Duration = Duration::from_secs(45);

fn is_transient_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504)
}

fn is_transient_reqwest_error(err: &reqwest::Error) -> bool {
    if let Some(status) = err.status() {
        return is_transient_status(status);
    }
    err.is_timeout() || err.is_body()
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let val = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let secs: u64 = val.trim().parse().ok()?;
    Some(Duration::from_secs(secs.clamp(1, 5)))
}

pub fn ensure_rustls_crypto_provider() {
    static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INIT.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

pub fn configure_reqwest_client_builder(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    ensure_rustls_crypto_provider();
    #[cfg(target_os = "android")]
    {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        if let Ok(cfg_builder) = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
        {
            let mut tls_cfg = cfg_builder
                .with_root_certificates(root_store)
                .with_no_client_auth();
            tls_cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
            return builder.tls_backend_preconfigured(tls_cfg);
        }
    }
    builder
}

impl DownloadManager {
    pub fn new() -> ThkResult<Self> {
        let client = configure_reqwest_client_builder(
            Client::builder()
                .user_agent(USER_AGENT)
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(STREAM_CHUNK_IDLE_TIMEOUT)
                .pool_max_idle_per_host(32),
        )
        .build()?;
        Ok(Self {
            client,
            max_concurrency: Arc::new(AtomicUsize::new(12)),
        })
    }

    pub fn set_max_concurrency(&self, limit: usize) {
        self.max_concurrency
            .store(limit.clamp(2, 32), Ordering::Relaxed);
    }

    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency.load(Ordering::Relaxed).clamp(2, 32)
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    async fn send_metadata_with_retry(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> ThkResult<reqwest::Response> {
        let mut last_status_err: Option<reqwest::Error> = None;
        for backoff_ms in RETRY_BACKOFFS_MS
            .iter()
            .copied()
            .map(Some)
            .chain(std::iter::once(None))
        {
            let mut req = self.client.get(url).timeout(METADATA_REQUEST_TIMEOUT);
            for &(k, v) in headers {
                req = req.header(k, v);
            }
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return Ok(resp);
                    }
                    let retry_after = parse_retry_after(resp.headers());
                    let err = resp.error_for_status().unwrap_err();
                    if let Some(ms) = backoff_ms.filter(|_| is_transient_status(status)) {
                        last_status_err = Some(err);
                        let delay = retry_after.unwrap_or_else(|| Duration::from_millis(ms));
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(err.into());
                }
                Err(err) => {
                    if let Some(ms) = backoff_ms.filter(|_| is_transient_reqwest_error(&err)) {
                        tokio::time::sleep(Duration::from_millis(ms)).await;
                        continue;
                    }
                    if err.status().is_none() {
                        if let Some(prev) = last_status_err {
                            return Err(prev.into());
                        }
                    }
                    return Err(err.into());
                }
            }
        }
        Err(ThkError::InvalidInput(format!(
            "Exhausted retries for {url}"
        )))
    }

    pub async fn get_bytes(&self, url: &str) -> ThkResult<Vec<u8>> {
        let resp = self.send_metadata_with_retry(url, &[]).await?;
        Ok(resp.bytes().await?.to_vec())
    }

    pub async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> ThkResult<T> {
        let resp = self.send_metadata_with_retry(url, &[]).await?;
        Ok(resp.json::<T>().await?)
    }

    pub async fn get_json_with_headers<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> ThkResult<T> {
        let resp = self.send_metadata_with_retry(url, headers).await?;
        Ok(resp.json::<T>().await?)
    }

    pub async fn needs_download(task: &DownloadTask) -> bool {
        if task.force_overwrite {
            return true;
        }
        let Ok(meta) = fs::metadata(&task.dest).await else {
            return true;
        };
        if meta.len() == 0 {
            return true;
        }
        if let Some(expected_len) = task.expected_size {
            if meta.len() != expected_len {
                return true;
            }
        }
        if let Some(ref sha1_hex) = task.expected_sha1 {
            let dest_clone = task.dest.clone();
            let expected_clone = sha1_hex.clone();
            if let Ok(Ok(())) = tokio::task::spawn_blocking(move || {
                crate::hash::verify_file_sha1(&dest_clone, &expected_clone)
            })
            .await
            {
                return false;
            }
            return true;
        }
        if let Some(ref sha256_hex) = task.expected_sha256 {
            let dest_clone = task.dest.clone();
            let expected_clone = sha256_hex.clone();
            if let Ok(Ok(())) = tokio::task::spawn_blocking(move || {
                crate::hash::verify_file_sha256(&dest_clone, &expected_clone)
            })
            .await
            {
                return false;
            }
            return true;
        }
        if let Some(ref sha512_hex) = task.expected_sha512 {
            let dest_clone = task.dest.clone();
            let expected_clone = sha512_hex.clone();
            if let Ok(Ok(())) = tokio::task::spawn_blocking(move || {
                crate::hash::verify_file_sha512(&dest_clone, &expected_clone)
            })
            .await
            {
                return false;
            }
            return true;
        }
        false
    }

    pub async fn download_single<F>(
        &self,
        task: &DownloadTask,
        cancel: Option<&CancellationToken>,
        mut on_chunk: F,
    ) -> ThkResult<u64>
    where
        F: FnMut(u64, u64, u64),
    {
        if cancel.is_some_and(CancellationToken::is_cancelled) {
            return Err(ThkError::Cancelled);
        }
        if !Self::needs_download(task).await {
            let len = fs::metadata(&task.dest).await.map(|m| m.len()).unwrap_or(0);
            on_chunk(0, len, len);
            return Ok(len);
        }

        let mut candidate_urls = Vec::with_capacity(1 + task.fallback_urls.len());
        candidate_urls.push(task.url.as_str());
        for fb in &task.fallback_urls {
            candidate_urls.push(fb.as_str());
        }

        let mut last_err = None;
        for url in candidate_urls {
            match self
                .download_from_url(url, task, cancel, &mut on_chunk)
                .await
            {
                Ok(written) => return Ok(written),
                Err(ThkError::Cancelled) => return Err(ThkError::Cancelled),
                Err(err) => last_err = Some(err),
            }
        }

        Err(last_err.unwrap_or_else(|| ThkError::InvalidInput("No download URL provided".into())))
    }

    async fn download_from_url<F>(
        &self,
        url: &str,
        task: &DownloadTask,
        cancel: Option<&CancellationToken>,
        on_chunk: &mut F,
    ) -> ThkResult<u64>
    where
        F: FnMut(u64, u64, u64),
    {
        let cancellation = cancel.cloned().unwrap_or_default();
        let mut last_status_err: Option<ThkError> = None;

        for backoff_ms in RETRY_BACKOFFS_MS
            .iter()
            .copied()
            .map(Some)
            .chain(std::iter::once(None))
        {
            if cancellation.is_cancelled() {
                return Err(ThkError::Cancelled);
            }
            match self
                .download_from_url_once(url, task, &cancellation, on_chunk)
                .await
            {
                Ok((written, _)) => return Ok(written),
                Err((ThkError::Cancelled, _)) => return Err(ThkError::Cancelled),
                Err((err, retry_hint)) => {
                    let retryable = match &err {
                        ThkError::Network(net_err) => is_transient_reqwest_error(net_err),
                        _ => retry_hint.is_some(),
                    };
                    if let ThkError::Network(ref net_err) = err {
                        if net_err.status().is_some() {
                            last_status_err = Some(ThkError::InvalidInput(net_err.to_string()));
                        }
                    }
                    if let Some(ms) = backoff_ms.filter(|_| retryable) {
                        let delay = retry_hint.unwrap_or_else(|| Duration::from_millis(ms));
                        if let ThkError::Network(ref net_err) = err {
                            if net_err.status().is_some() {
                                last_status_err = Some(err);
                            }
                        }
                        tokio::select! {
                            biased;
                            _ = cancellation.cancelled() => return Err(ThkError::Cancelled),
                            _ = tokio::time::sleep(delay) => continue,
                        }
                    }
                    if let ThkError::Network(ref net_err) = err {
                        if net_err.status().is_none() {
                            if let Some(prev) = last_status_err {
                                return Err(prev);
                            }
                        }
                    }
                    return Err(err);
                }
            }
        }
        Err(last_status_err.unwrap_or_else(|| {
            ThkError::InvalidInput(format!("Download exhausted retries for '{}'", task.label))
        }))
    }

    async fn download_from_url_once<F>(
        &self,
        url: &str,
        task: &DownloadTask,
        cancellation: &CancellationToken,
        on_chunk: &mut F,
    ) -> Result<(u64, Option<Duration>), (ThkError, Option<Duration>)>
    where
        F: FnMut(u64, u64, u64),
    {
        if let Some(parent) = task.dest.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| (ThkError::from(e), None))?;
        }

        let tmp_path = temp_part_path(&task.dest);
        let raw_resp = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err((ThkError::Cancelled, None)),
            response = self.client.get(url).send() => response.map_err(|e| (ThkError::from(e), None))?,
        };
        let status = raw_resp.status();
        if !status.is_success() {
            let retry_after = parse_retry_after(raw_resp.headers());
            let err = raw_resp.error_for_status().unwrap_err();
            return Err((ThkError::from(err), retry_after));
        }
        let resp = raw_resp;
        let total = resp.content_length().or(task.expected_size).unwrap_or(0);

        let mut file = File::create(&tmp_path)
            .await
            .map_err(|e| (ThkError::from(e), None))?;
        let mut stream = resp.bytes_stream();
        let mut sha1_hasher = task.expected_sha1.as_ref().map(|_| Sha1::new());
        let mut sha256_hasher = task.expected_sha256.as_ref().map(|_| Sha256::new());
        let mut sha512_hasher = task.expected_sha512.as_ref().map(|_| Sha512::new());
        let mut downloaded = 0u64;

        let write_res: Result<(), (ThkError, Option<Duration>)> = async {
            loop {
                let chunk_res = tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => return Err((ThkError::Cancelled, None)),
                    timed = tokio::time::timeout(STREAM_CHUNK_IDLE_TIMEOUT, stream.next()) => {
                        match timed {
                            Ok(Some(chunk)) => chunk,
                            Ok(None) => break,
                            Err(_) => return Err((
                                ThkError::InvalidInput(format!(
                                    "Stream stalled while downloading '{}'",
                                    task.label
                                )),
                                Some(Duration::from_millis(RETRY_BACKOFFS_MS[0])),
                            )),
                        }
                    }
                };
                let chunk = chunk_res.map_err(|e| (ThkError::from(e), None))?;
                file.write_all(&chunk)
                    .await
                    .map_err(|e| (ThkError::from(e), None))?;
                if let Some(ref mut h) = sha1_hasher {
                    h.update(&chunk);
                }
                if let Some(ref mut h) = sha256_hasher {
                    h.update(&chunk);
                }
                if let Some(ref mut h) = sha512_hasher {
                    h.update(&chunk);
                }
                let delta = chunk.len() as u64;
                downloaded += delta;
                on_chunk(delta, downloaded, total);
            }
            file.flush().await.map_err(|e| (ThkError::from(e), None))?;
            file.sync_all()
                .await
                .map_err(|e| (ThkError::from(e), None))?;
            Ok(())
        }
        .await;

        drop(file);
        if let Err(err) = write_res {
            let _ = fs::remove_file(&tmp_path).await;
            return Err(err);
        }

        if let Some(expected) = task.expected_size {
            if downloaded != expected {
                let _ = fs::remove_file(&tmp_path).await;
                return Err((
                    ThkError::InvalidInput(format!(
                        "Download size mismatch for '{}': expected {expected} bytes, received {downloaded}",
                        task.label
                    )),
                    None,
                ));
            }
        }

        if let (Some(expected), Some(hasher)) = (&task.expected_sha1, sha1_hasher) {
            let actual = hex::encode(hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected.trim()) {
                let _ = fs::remove_file(&tmp_path).await;
                return Err((
                    ThkError::HashMismatch {
                        file: task.label.clone(),
                        expected: expected.trim().to_ascii_lowercase(),
                        actual,
                    },
                    None,
                ));
            }
        }

        if let (Some(expected), Some(hasher)) = (&task.expected_sha256, sha256_hasher) {
            let actual = hex::encode(hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected.trim()) {
                let _ = fs::remove_file(&tmp_path).await;
                return Err((
                    ThkError::HashMismatch {
                        file: task.label.clone(),
                        expected: expected.trim().to_ascii_lowercase(),
                        actual,
                    },
                    None,
                ));
            }
        }

        if let (Some(expected), Some(hasher)) = (&task.expected_sha512, sha512_hasher) {
            let actual = hex::encode(hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected.trim()) {
                let _ = fs::remove_file(&tmp_path).await;
                return Err((
                    ThkError::HashMismatch {
                        file: task.label.clone(),
                        expected: expected.trim().to_ascii_lowercase(),
                        actual,
                    },
                    None,
                ));
            }
        }

        if cancellation.is_cancelled() {
            let _ = fs::remove_file(&tmp_path).await;
            return Err((ThkError::Cancelled, None));
        }
        if let Err(err) = fs::rename(&tmp_path, &task.dest).await {
            let _ = fs::remove_file(&tmp_path).await;
            return Err((err.into(), None));
        }
        Ok((downloaded, None))
    }

    pub async fn download_batch<F>(
        &self,
        phase: &str,
        tasks: Vec<DownloadTask>,
        concurrency: usize,
        cancel: Option<CancellationToken>,
        on_progress: F,
    ) -> ThkResult<()>
    where
        F: Fn(DownloadProgress) + Send + Sync + 'static,
    {
        let total_files = tasks.len();
        if total_files == 0 {
            return Ok(());
        }

        let total_bytes_est: u64 = tasks.iter().filter_map(|t| t.expected_size).sum();
        let completed_files = Arc::new(AtomicUsize::new(0));
        let downloaded_bytes = Arc::new(AtomicU64::new(0));
        let meter = Arc::new(SpeedMeter::new());
        let phase_str = phase.to_string();
        let cb = Arc::new(on_progress);
        let worker_limit = concurrency
            .max(1)
            .min(self.max_concurrency())
            .min(total_files);
        let batch_cancel = cancel.unwrap_or_default().child_token();
        let _cancel_guard = batch_cancel.clone().drop_guard();

        let mut downloads = futures_util::stream::iter(tasks.into_iter().map(|task| {
            let mgr = self.clone();
            let completed = Arc::clone(&completed_files);
            let dl_bytes = Arc::clone(&downloaded_bytes);
            let meter = Arc::clone(&meter);
            let cb = Arc::clone(&cb);
            let phase_name = phase_str.clone();
            let cancel_tok = batch_cancel.clone();

            async move {
                let label = task.label.clone();
                let res = mgr
                    .download_single(&task, Some(&cancel_tok), |delta, _file_dl, _file_tot| {
                        if delta > 0 {
                            let cur_bytes = dl_bytes.fetch_add(delta, Ordering::Relaxed) + delta;
                            let speed = meter.record_delta(delta);
                            let done = completed.load(Ordering::Relaxed);
                            let pct = if total_bytes_est > 0 {
                                ((cur_bytes as f64 / total_bytes_est as f64) * 100.0).min(99.9)
                                    as f32
                            } else {
                                ((done as f32 / total_files as f32) * 100.0).min(99.9)
                            };
                            cb(DownloadProgress {
                                phase: phase_name.clone(),
                                message: label.clone(),
                                completed_files: done,
                                total_files,
                                downloaded_bytes: cur_bytes,
                                total_bytes: total_bytes_est,
                                bytes_per_sec: speed,
                                percent: pct,
                            });
                        }
                    })
                    .await?;

                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                let cur_bytes = dl_bytes.load(Ordering::Relaxed);
                let speed = meter.record_delta(0);
                let pct = (done as f32 / total_files as f32) * 100.0;
                cb(DownloadProgress {
                    phase: phase_name,
                    message: task.label,
                    completed_files: done,
                    total_files,
                    downloaded_bytes: cur_bytes.max(res),
                    total_bytes: total_bytes_est,
                    bytes_per_sec: speed,
                    percent: pct,
                });
                Ok::<(), ThkError>(())
            }
        }))
        .buffer_unordered(worker_limit);
        let mut first_error = None;
        // Drain cancelled workers so open files are closed and partial downloads are removed.
        while let Some(result) = downloads.next().await {
            if let Err(error) = result {
                if first_error.is_none() {
                    first_error = Some(error);
                    batch_cancel.cancel();
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn temp_part_path(dest: &Path) -> PathBuf {
    let fname = dest
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    dest.with_file_name(format!(".{fname}.{}.part", Uuid::new_v4().simple()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn batch_failure_or_cancellation_drains_stalled_workers_and_keeps_existing_files() {
        for cancel_batch in [false, true] {
            let root = std::env::temp_dir().join(format!("thk-batch-{}", Uuid::new_v4()));
            fs::create_dir_all(&root).await.unwrap();
            let stalled = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let failing = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut tasks = Vec::new();
            for (name, address) in [
                ("stalled.jar", stalled.local_addr().unwrap()),
                ("failing.jar", failing.local_addr().unwrap()),
            ] {
                let dest = root.join(name);
                fs::write(&dest, b"existing").await.unwrap();
                let mut task = DownloadTask::new(format!("http://{address}/test"), dest);
                task.expected_size = Some(100);
                tasks.push(task);
            }
            let chunk_received = Arc::new(tokio::sync::Notify::new());
            let signal = Arc::clone(&chunk_received);
            let parent = CancellationToken::new();
            let server_cancel = parent.clone();
            let (release, hold) = tokio::sync::oneshot::channel::<()>();
            let stalled_server = tokio::spawn(async move {
                let (mut socket, _) = stalled.accept().await.unwrap();
                let mut request = [0; 4096];
                assert!(socket.read(&mut request).await.unwrap() > 0);
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nx")
                    .await
                    .unwrap();
                let _ = hold.await;
            });
            let failing_server = tokio::spawn(async move {
                let (mut socket, _) = failing.accept().await.unwrap();
                let mut request = [0; 4096];
                assert!(socket.read(&mut request).await.unwrap() > 0);
                chunk_received.notified().await;
                if cancel_batch {
                    server_cancel.cancel();
                } else {
                    socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
                }
            });
            let manager = DownloadManager::new().unwrap();
            let result = tokio::time::timeout(
                Duration::from_secs(3),
                manager.download_batch("Test", tasks, 2, Some(parent.clone()), move |progress| {
                    if progress.message == "stalled.jar" && progress.downloaded_bytes > 0 {
                        signal.notify_one();
                    }
                }),
            )
            .await
            .unwrap();
            if cancel_batch {
                assert!(matches!(result, Err(ThkError::Cancelled)));
            } else {
                assert!(matches!(result, Err(ThkError::Network(error))
                    if error.status() == Some(reqwest::StatusCode::SERVICE_UNAVAILABLE)));
            }
            assert_eq!(parent.is_cancelled(), cancel_batch);
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
            for name in ["stalled.jar", "failing.jar"] {
                assert_eq!(fs::read(root.join(name)).await.unwrap(), b"existing");
            }
            release.send(()).unwrap();
            stalled_server.await.unwrap();
            failing_server.await.unwrap();
            fs::remove_dir_all(root).await.unwrap();
        }
    }

    #[tokio::test]
    async fn successful_download_replaces_existing_file_without_leaving_partials() {
        let root = std::env::temp_dir().join(format!("thk-replace-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).await.unwrap();
        let dest = root.join("artifact.jar");
        fs::write(&dest, b"old").await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/artifact", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nnew")
                .await
                .unwrap();
        });
        let mut task = DownloadTask::new(url, dest.clone());
        task.expected_sha1 = Some("c2a6b03f190dfb2b4aa91f8af8d477a9bc3401dc".into());
        task.expected_size = Some(3);
        DownloadManager::new()
            .unwrap()
            .download_single(&task, None, |_, _, _| {})
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(fs::read(&dest).await.unwrap(), b"new");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn size_mismatch_preserves_existing_file_and_removes_partial_download() {
        let root = std::env::temp_dir().join(format!("thk-download-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).await.unwrap();
        let dest = root.join("artifact.jar");
        fs::write(&dest, b"existing").await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/artifact", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nnew")
                .await
                .unwrap();
        });
        let mut task = DownloadTask::new(url, dest.clone());
        task.expected_size = Some(10);
        let result = DownloadManager::new()
            .unwrap()
            .download_single(&task, None, |_, _, _| {})
            .await;
        assert!(result.is_err());
        assert_eq!(fs::read(&dest).await.unwrap(), b"existing");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        server.await.unwrap();
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn cancellation_interrupts_stalled_body_and_cleans_partial_file() {
        let root = std::env::temp_dir().join(format!("thk-download-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/artifact", listener.local_addr().unwrap());
        let cancel = CancellationToken::new();
        let cancel_on_chunk = cancel.clone();
        let (release, hold) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nx")
                .await
                .unwrap();
            let _ = hold.await;
        });
        let task = DownloadTask::new(url, root.join("artifact.jar"));
        let manager = DownloadManager::new().unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            manager.download_single(&task, Some(&cancel), move |_, _, _| {
                cancel_on_chunk.cancel()
            }),
        )
        .await
        .unwrap();
        assert!(matches!(result, Err(ThkError::Cancelled)));
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        release.send(()).unwrap();
        server.await.unwrap();
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn transient_429_and_503_recover_via_retry_and_force_overwrite_works() {
        let root = std::env::temp_dir().join(format!("thk-retry-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).await.unwrap();
        let dest = root.join("plugin.jar");
        fs::write(&dest, b"stale-unhashed").await.unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/plugin.jar", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for response in [
                b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".as_slice(),
                b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".as_slice(),
                b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nupdated".as_slice(),
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut req = [0u8; 2048];
                let _ = socket.read(&mut req).await.unwrap();
                socket.write_all(response).await.unwrap();
            }
        });

        let mut task = DownloadTask::new(url, dest.clone());
        assert!(!DownloadManager::needs_download(&task).await);
        task.force_overwrite = true;
        assert!(DownloadManager::needs_download(&task).await);

        let written = DownloadManager::new()
            .unwrap()
            .download_single(&task, None, |_, _, _| {})
            .await
            .unwrap();
        assert_eq!(written, 7);
        assert_eq!(fs::read(&dest).await.unwrap(), b"updated");
        server.await.unwrap();
        fs::remove_dir_all(root).await.unwrap();
    }
}
