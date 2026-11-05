use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use thk_core::models::{Account, SkinVariant};
use thk_core::{AppPaths, DownloadManager, DownloadTask, ThkError, ThkResult};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub const AUTHLIB_INJECTOR_URL: &str =
    "https://github.com/yushijinhun/authlib-injector/releases/download/v1.2.7/authlib-injector-1.2.7.jar";
pub const AUTHLIB_INJECTOR_SHA256: &str =
    "eab52a5c32fb3be1c6ea4cf02ffc1b09ec08d89ed2d7ef39cc23af4bd5ba4bf9";

#[derive(Debug, Clone)]
pub struct YggdrasilPlayerEntry {
    pub raw_uuid: String,
    pub username: String,
    pub skin_variant: SkinVariant,
    pub skin_path: Option<PathBuf>,
    pub cape_path: Option<PathBuf>,
}

#[derive(Clone)]
struct YggState {
    port: u16,
    players: Arc<RwLock<HashMap<String, YggdrasilPlayerEntry>>>,
}

pub struct LocalYggdrasilHandle {
    pub port: u16,
    cancel: CancellationToken,
}

impl LocalYggdrasilHandle {
    pub fn jvm_agent_arg(&self, authlib_jar: &std::path::Path) -> String {
        format!(
            "-javaagent:{}=http://127.0.0.1:{}",
            authlib_jar.display(),
            self.port
        )
    }

    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

impl Drop for LocalYggdrasilHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub async fn ensure_authlib_injector(
    downloader: &DownloadManager,
    paths: &AppPaths,
) -> ThkResult<PathBuf> {
    let mut task = DownloadTask::new(AUTHLIB_INJECTOR_URL, paths.authlib_jar.clone());
    task.fallback_urls.push(
        "https://bmclapi2.bangbang93.com/mirrors/authlib-injector/artifact/53/authlib-injector-1.2.7.jar".into(),
    );
    if let Ok(meta) = tokio::fs::metadata(&paths.authlib_jar).await {
        if meta.len() > 50_000 {
            return Ok(paths.authlib_jar.clone());
        }
    }
    downloader.download_single(&task, None, |_, _, _| {}).await?;
    Ok(paths.authlib_jar.clone())
}

pub async fn start_local_yggdrasil(account: &Account) -> ThkResult<LocalYggdrasilHandle> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| ThkError::Process(format!("Failed binding local Yggdrasil server: {e}")))?;
    let addr: SocketAddr = listener.local_addr()?;
    let port = addr.port();

    let raw_uuid = account.uuid.replace('-', "").to_ascii_lowercase();
    let entry = YggdrasilPlayerEntry {
        raw_uuid: raw_uuid.clone(),
        username: account.username.clone(),
        skin_variant: account.skin_variant,
        skin_path: account.custom_skin_path.clone().filter(|p| p.is_file()),
        cape_path: account
            .custom_cape_path
            .clone()
            .filter(|p| p.is_file())
            .or_else(|| account.custom_elytra_path.clone().filter(|p| p.is_file())),
    };

    let mut map = HashMap::new();
    map.insert(raw_uuid, entry);

    let state = YggState {
        port,
        players: Arc::new(RwLock::new(map)),
    };

    let app = Router::new()
        .route("/", get(handle_root))
        .route("/minecraft/profile", get(handle_query_profile))
        .route("/session/minecraft/profile/{uuid}", get(handle_session_profile))
        .route(
            "/sessionserver/session/minecraft/profile/{uuid}",
            get(handle_session_profile),
        )
        .route("/textures/{uuid}", get(handle_texture))
        .route("/authserver/validate", any(handle_no_content))
        .route("/authserver/invalidate", any(handle_no_content))
        .route("/authserver/signout", any(handle_no_content))
        .route("/auth/validate", any(handle_no_content))
        .route("/auth/invalidate", any(handle_no_content))
        .route("/auth/signout", any(handle_no_content))
        .with_state(state);

    let cancel = CancellationToken::new();
    let shutdown_tok = cancel.clone();

    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown_tok.cancelled().await;
            })
            .await;
    });

    Ok(LocalYggdrasilHandle { port, cancel })
}

async fn handle_root() -> Json<Value> {
    Json(json!({
        "meta": {
            "serverName": "TriHienKun Laucher Local Skin Server",
            "implementationName": "thk-yggdrasil",
            "implementationVersion": "1.0.0",
            "feature.non_email_login": true
        },
        "skinDomains": ["localhost", "127.0.0.1", "::1"]
    }))
}

#[derive(Deserialize)]
struct ProfileQuery {
    uuid: Option<String>,
    id: Option<String>,
}

async fn handle_query_profile(
    State(state): State<YggState>,
    Query(q): Query<ProfileQuery>,
) -> Response {
    let target = q
        .uuid
        .or(q.id)
        .map(|s| s.replace('-', "").to_ascii_lowercase());
    let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
    let found = match target {
        Some(ref id) => guard.get(id).or_else(|| guard.values().next()),
        None => guard.values().next(),
    };
    match found {
        Some(entry) => Json(build_profile_json(entry, state.port)).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn handle_session_profile(
    State(state): State<YggState>,
    AxumPath(uuid): AxumPath<String>,
) -> Response {
    let key = uuid.replace('-', "").to_ascii_lowercase();
    let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
    match guard.get(&key).or_else(|| guard.values().next()) {
        Some(entry) => Json(build_profile_json(entry, state.port)).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

#[derive(Deserialize)]
struct TextureQuery {
    #[serde(rename = "type")]
    tex_type: Option<String>,
}

async fn handle_texture(
    State(state): State<YggState>,
    AxumPath(uuid): AxumPath<String>,
    Query(q): Query<TextureQuery>,
) -> Response {
    let key = uuid.replace('-', "").to_ascii_lowercase();
    let path_opt = {
        let guard = state.players.read().unwrap_or_else(|e| e.into_inner());
        guard.get(&key).and_then(|entry| match q.tex_type.as_deref() {
            Some("cape") => entry.cape_path.clone(),
            _ => entry.skin_path.clone(),
        })
    };

    let Some(file_path) = path_opt else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match tokio::fs::read(&file_path).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "image/png")], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn handle_no_content() -> StatusCode {
    StatusCode::NO_CONTENT
}

fn build_profile_json(entry: &YggdrasilPlayerEntry, port: u16) -> Value {
    let mut textures = serde_json::Map::new();
    if entry.skin_path.is_some() {
        let mut skin_obj = serde_json::Map::new();
        skin_obj.insert(
            "url".into(),
            Value::String(format!(
                "http://127.0.0.1:{port}/textures/{}?type=skin",
                entry.raw_uuid
            )),
        );
        if entry.skin_variant == SkinVariant::Slim {
            skin_obj.insert("metadata".into(), json!({ "model": "slim" }));
        }
        textures.insert("SKIN".into(), Value::Object(skin_obj));
    }
    if entry.cape_path.is_some() {
        textures.insert(
            "CAPE".into(),
            json!({
                "url": format!("http://127.0.0.1:{port}/textures/{}?type=cape", entry.raw_uuid)
            }),
        );
    }

    let tex_payload = json!({
        "timestamp": Utc::now().timestamp_millis(),
        "profileId": entry.raw_uuid,
        "profileName": entry.username,
        "textures": textures,
    });
    let encoded = BASE64.encode(tex_payload.to_string().as_bytes());

    json!({
        "id": entry.raw_uuid,
        "name": entry.username,
        "properties": [
            {
                "name": "textures",
                "value": encoded
            }
        ]
    })
}
