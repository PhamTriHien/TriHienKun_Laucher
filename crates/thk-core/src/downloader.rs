use crate::error::{ThkError, ThkResult};
use crate::models::DownloadProgress;
use futures_util::{StreamExt, TryStreamExt};
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

pub const USER_AGENT: &str = "TriHienKun-Laucher/1.0.0 (Pure-Rust; https://github.com/trihienkun/trihienkun-laucher)";

#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub url: String,
    pub fallback_urls: Vec<String>,
    pub dest: PathBuf,
    pub expected_sha1: Option<String>,
    pub expected_sha256: Option<String>,
    pub expected_sha512: Option<String>,
    pub expected_size: Option<u64>,
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

impl DownloadManager {
    pub fn new() -> ThkResult<Self> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(32)
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

    pub async fn get_bytes(&self, url: &str) -> ThkResult<Vec<u8>> {
        let resp = self.client.get(url).send().await?.error_for_status()?;
        Ok(resp.bytes().await?.to_vec())
    }

    pub async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> ThkResult<T> {
        let resp = self.client.get(url).send().await?.error_for_status()?;
        Ok(resp.json::<T>().await?)
    }

    pub async fn get_json_with_headers<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> ThkResult<T> {
        let mut req = self.client.get(url);
        for &(k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req.send().await?.error_for_status()?;
        Ok(resp.json::<T>().await?)
    }

    pub async fn needs_download(task: &DownloadTask) -> bool {
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
        if let Some(parent) = task.dest.parent() {
            fs::create_dir_all(parent).await?;
        }

        let tmp_path = temp_part_path(&task.dest);
        let resp = self.client.get(url).send().await?.error_for_status()?;
        let total = resp.content_length().or(task.expected_size).unwrap_or(0);

        let mut file = File::create(&tmp_path).await?;
        let mut stream = resp.bytes_stream();
        let mut sha1_hasher = task.expected_sha1.as_ref().map(|_| Sha1::new());
        let mut sha256_hasher = task.expected_sha256.as_ref().map(|_| Sha256::new());
        let mut sha512_hasher = task.expected_sha512.as_ref().map(|_| Sha512::new());
        let mut downloaded = 0u64;

        let write_res: ThkResult<()> = async {
            while let Some(chunk_res) = stream.next().await {
                if let Some(tok) = cancel {
                    if tok.is_cancelled() {
                        return Err(ThkError::Cancelled);
                    }
                }
                let chunk = chunk_res?;
                file.write_all(&chunk).await?;
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
            file.flush().await?;
            file.sync_all().await?;
            Ok(())
        }
        .await;

        drop(file);
        if let Err(err) = write_res {
            let _ = fs::remove_file(&tmp_path).await;
            return Err(err);
        }

        if let (Some(expected), Some(hasher)) = (&task.expected_sha1, sha1_hasher) {
            let actual = hex::encode(hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected.trim()) {
                let _ = fs::remove_file(&tmp_path).await;
                return Err(ThkError::HashMismatch {
                    file: task.label.clone(),
                    expected: expected.trim().to_ascii_lowercase(),
                    actual,
                });
            }
        }

        if let (Some(expected), Some(hasher)) = (&task.expected_sha256, sha256_hasher) {
            let actual = hex::encode(hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected.trim()) {
                let _ = fs::remove_file(&tmp_path).await;
                return Err(ThkError::HashMismatch {
                    file: task.label.clone(),
                    expected: expected.trim().to_ascii_lowercase(),
                    actual,
                });
            }
        }

        if let (Some(expected), Some(hasher)) = (&task.expected_sha512, sha512_hasher) {
            let actual = hex::encode(hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected.trim()) {
                let _ = fs::remove_file(&tmp_path).await;
                return Err(ThkError::HashMismatch {
                    file: task.label.clone(),
                    expected: expected.trim().to_ascii_lowercase(),
                    actual,
                });
            }
        }

        if task.dest.exists() {
            let _ = fs::remove_file(&task.dest).await;
        }
        fs::rename(&tmp_path, &task.dest).await?;
        Ok(downloaded)
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
        let worker_limit = if concurrency <= 1 || total_files <= 1 {
            1
        } else {
            self.max_concurrency()
        };

        futures_util::stream::iter(tasks.into_iter().map(|task| {
            let mgr = self.clone();
            let completed = Arc::clone(&completed_files);
            let dl_bytes = Arc::clone(&downloaded_bytes);
            let meter = Arc::clone(&meter);
            let cb = Arc::clone(&cb);
            let phase_name = phase_str.clone();
            let cancel_tok = cancel.clone();

            async move {
                let label = task.label.clone();
                let res = mgr
                    .download_single(&task, cancel_tok.as_ref(), |delta, _file_dl, _file_tot| {
                        if delta > 0 {
                            let cur_bytes = dl_bytes.fetch_add(delta, Ordering::Relaxed) + delta;
                            let speed = meter.record_delta(delta);
                            let done = completed.load(Ordering::Relaxed);
                            let pct = if total_bytes_est > 0 {
                                ((cur_bytes as f64 / total_bytes_est as f64) * 100.0).min(99.9) as f32
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
        .buffer_unordered(worker_limit)
        .try_collect::<Vec<()>>()
        .await?;

        Ok(())
    }
}

fn temp_part_path(dest: &Path) -> PathBuf {
    let fname = dest
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    dest.with_file_name(format!(".{fname}.{}.part", Uuid::new_v4().simple()))
}
