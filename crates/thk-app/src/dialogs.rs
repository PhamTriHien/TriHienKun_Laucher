use std::path::{Path, PathBuf};

#[cfg(target_os = "android")]
#[derive(serde::Serialize)]
struct AndroidFilePickRequest<'a> {
    id: String,
    multiple: bool,
    extensions: &'a [&'a str],
}

#[cfg(target_os = "android")]
#[derive(serde::Deserialize)]
struct AndroidFilePickResponse {
    #[serde(default)]
    paths: Vec<String>,
}

#[cfg(target_os = "android")]
async fn android_pick_via_bridge(multiple: bool, extensions: &[&str]) -> Option<Vec<PathBuf>> {
    use std::time::{Duration, Instant};

    let dir = thk_core::android_bridge_dir()?;
    let req_id = uuid::Uuid::new_v4().simple().to_string();
    let req = AndroidFilePickRequest {
        id: req_id.clone(),
        multiple,
        extensions,
    };
    let payload = serde_json::to_vec(&req).ok()?;
    let tmp_path = dir.join("pick_req.tmp");
    let req_path = dir.join("pick_req.json");
    let resp_path = dir.join(format!("pick_resp_{req_id}.json"));
    let _ = tokio::fs::remove_file(&resp_path).await;
    tokio::fs::write(&tmp_path, &payload).await.ok()?;
    tokio::fs::rename(&tmp_path, &req_path).await.ok()?;

    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline {
        if let Ok(raw) = tokio::fs::read(&resp_path).await {
            let _ = tokio::fs::remove_file(&resp_path).await;
            let parsed: AndroidFilePickResponse = serde_json::from_slice(&raw).ok()?;
            let paths: Vec<PathBuf> = parsed
                .paths
                .into_iter()
                .map(PathBuf::from)
                .filter(|p| p.is_file())
                .collect();
            return (!paths.is_empty()).then_some(paths);
        }
        tokio::time::sleep(Duration::from_millis(90)).await;
    }
    let _ = tokio::fs::remove_file(&req_path).await;
    None
}

pub async fn pick_file_async(
    title: &str,
    filter_name: Option<&str>,
    extensions: &[&str],
) -> Option<PathBuf> {
    #[cfg(not(target_os = "android"))]
    {
        let mut dlg = rfd::AsyncFileDialog::new().set_title(title);
        if let Some(name) = filter_name.filter(|_| !extensions.is_empty()) {
            dlg = dlg.add_filter(name, extensions);
        }
        dlg.pick_file().await.map(|f| f.path().to_path_buf())
    }
    #[cfg(target_os = "android")]
    {
        let _ = (title, filter_name);
        android_pick_via_bridge(false, extensions)
            .await
            .and_then(|mut list| list.drain(..).next())
    }
}

pub async fn pick_files_async(
    title: &str,
    filter_name: Option<&str>,
    extensions: &[&str],
) -> Option<Vec<PathBuf>> {
    #[cfg(not(target_os = "android"))]
    {
        let mut dlg = rfd::AsyncFileDialog::new().set_title(title);
        if let Some(name) = filter_name.filter(|_| !extensions.is_empty()) {
            dlg = dlg.add_filter(name, extensions);
        }
        let picked = dlg.pick_files().await?;
        Some(picked.into_iter().map(|f| f.path().to_path_buf()).collect())
    }
    #[cfg(target_os = "android")]
    {
        let _ = (title, filter_name);
        android_pick_via_bridge(true, extensions).await
    }
}

pub async fn pick_folder_async(title: &str) -> Option<PathBuf> {
    #[cfg(not(target_os = "android"))]
    {
        rfd::AsyncFileDialog::new()
            .set_title(title)
            .pick_folder()
            .await
            .map(|f| f.path().to_path_buf())
    }
    #[cfg(target_os = "android")]
    {
        let _ = title;
        None
    }
}

pub fn pick_file_sync() -> Option<PathBuf> {
    #[cfg(not(target_os = "android"))]
    {
        rfd::FileDialog::new().pick_file()
    }
    #[cfg(target_os = "android")]
    {
        None
    }
}

pub fn pick_folder_sync() -> Option<PathBuf> {
    #[cfg(not(target_os = "android"))]
    {
        rfd::FileDialog::new().pick_folder()
    }
    #[cfg(target_os = "android")]
    {
        None
    }
}

pub fn open_external_url(url: &str) -> Result<(), String> {
    #[cfg(not(target_os = "android"))]
    {
        open::that(url).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "android")]
    {
        thk_core::android_bridge_open_url(url).map_err(|e| e.to_string())
    }
}

pub fn open_local_path(path: &Path) -> Result<(), String> {
    #[cfg(not(target_os = "android"))]
    {
        open::that(path).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "android")]
    {
        Err(format!("Sandbox: {}", path.display()))
    }
}

pub fn sync_clipboard_text(ctx: &egui::Context, text: &str) {
    ctx.copy_text(text.to_owned());
    #[cfg(target_os = "android")]
    {
        thk_core::android_bridge_set_clipboard(text);
    }
}

pub fn sync_mobile_orientation(mode: thk_core::models::MobileOrientationMode) {
    #[cfg(target_os = "android")]
    {
        thk_core::android_bridge_set_orientation(mode.as_bridge_str());
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = mode;
    }
}

pub fn show_startup_error_dialog(title: &str, description: &str) {
    #[cfg(not(target_os = "android"))]
    {
        rfd::MessageDialog::new()
            .set_title(title)
            .set_description(description)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
    #[cfg(target_os = "android")]
    {
        tracing::error!("{title}: {description}");
    }
}
