use crate::token_store::{StoredAuthTokens, TokenStore};
use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thk_core::models::{Account, AccountType, SkinVariant};
use thk_core::{ThkError, ThkResult};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub const LIVE_PUBLIC_CLIENT_ID: &str = "00000000402b5328";
pub const LIVE_DESKTOP_REDIRECT_URI: &str = "https://login.live.com/oauth20_desktop.srf";
pub const LIVE_SCOPE: &str = "XboxLive.signin offline_access";
pub const LIVE_AUTH_URL: &str = "https://login.live.com/oauth20_authorize.srf";
pub const LIVE_DEVICE_URL: &str = "https://login.live.com/oauth20_connect.srf";
pub const LIVE_TOKEN_URL: &str = "https://login.live.com/oauth20_token.srf";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeSession {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub interval_secs: u64,
    pub expires_in_secs: u64,
}

#[derive(Debug, Clone)]
pub struct DesktopAuthChallenge {
    pub authorize_url: String,
    pub state: String,
    pub code_verifier: String,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedMinecraftSession {
    pub account: Account,
    pub mc_access_token: String,
    pub ms_refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct LiveDeviceCodeResp {
    user_code: Option<String>,
    device_code: Option<String>,
    verification_uri: Option<String>,
    interval: Option<u64>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct OAuthTokenResp {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

pub fn generate_pkce_pair() -> (String, String) {
    let mut raw = [0u8; 32];
    rand::rng().fill_bytes(&mut raw);
    let verifier = URL_SAFE_NO_PAD.encode(raw);
    let challenge = compute_pkce_s256(&verifier);
    (verifier, challenge)
}

pub fn compute_pkce_s256(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn random_hex_state() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn build_http_client() -> ThkResult<Client> {
    Client::builder()
        .user_agent("TriHienKun-Laucher/1.0")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(ThkError::from)
}

pub async fn start_device_code_login(open_browser: bool) -> ThkResult<DeviceCodeSession> {
    let client = build_http_client()?;
    let params = [
        ("client_id", LIVE_PUBLIC_CLIENT_ID),
        ("scope", LIVE_SCOPE),
        ("response_type", "device_code"),
    ];

    let resp = client
        .post(LIVE_DEVICE_URL)
        .form(&params)
        .header("Accept", "application/json")
        .send()
        .await?;

    let body: LiveDeviceCodeResp = resp.json().await?;
    if let Some(err) = body.error {
        let desc = body.error_description.unwrap_or(err);
        return Err(ThkError::Auth(format!(
            "Microsoft Device Code initialization failed: {desc}"
        )));
    }

    let session = DeviceCodeSession {
        user_code: body
            .user_code
            .ok_or_else(|| ThkError::Auth("Missing user_code from Microsoft Live".into()))?,
        device_code: body
            .device_code
            .ok_or_else(|| ThkError::Auth("Missing device_code from Microsoft Live".into()))?,
        verification_uri: body
            .verification_uri
            .unwrap_or_else(|| "https://www.microsoft.com/link".into()),
        interval_secs: body.interval.unwrap_or(5).max(3),
        expires_in_secs: body.expires_in.unwrap_or(900),
    };

    if open_browser {
        let _ = open::that(&session.verification_uri);
    }

    Ok(session)
}

pub async fn poll_device_code_login(
    session: &DeviceCodeSession,
    cancel: CancellationToken,
) -> ThkResult<AuthenticatedMinecraftSession> {
    let client = build_http_client()?;
    let mut wait_secs = session.interval_secs.max(3);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(session.expires_in_secs);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Err(ThkError::Cancelled),
            _ = tokio::time::sleep(Duration::from_secs(wait_secs)) => {}
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(ThkError::Auth(
                "Mã đăng nhập Microsoft đã hết hạn. Vui lòng thử lại.".into(),
            ));
        }

        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", LIVE_PUBLIC_CLIENT_ID),
            ("device_code", session.device_code.as_str()),
        ];

        let resp = client
            .post(LIVE_TOKEN_URL)
            .form(&params)
            .header("Accept", "application/json")
            .send()
            .await?;

        let token_body: OAuthTokenResp = resp.json().await?;
        if let Some(access_token) = token_body.access_token {
            let authed = exchange_ms_tokens_to_minecraft(
                &client,
                &access_token,
                token_body.refresh_token,
            )
            .await?;
            persist_tokens_in_keyring(&authed)?;
            return Ok(authed);
        }

        match token_body.error.as_deref() {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                wait_secs += 5;
                continue;
            }
            Some("authorization_declined") => {
                return Err(ThkError::Auth(
                    "Người dùng đã từ chối yêu cầu đăng nhập Microsoft.".into(),
                ));
            }
            Some("expired_token") | Some("bad_verification_code") => {
                return Err(ThkError::Auth(
                    "Mã xác thực Microsoft đã hết hạn hoặc không hợp lệ.".into(),
                ));
            }
            Some(other) => {
                let desc = token_body.error_description.unwrap_or_else(|| other.to_string());
                return Err(ThkError::Auth(format!("Đăng nhập Microsoft thất bại: {desc}")));
            }
            None => continue,
        }
    }
}

pub fn build_desktop_auth_challenge(open_browser: bool) -> DesktopAuthChallenge {
    let state = random_hex_state();
    let (verifier, challenge) = generate_pkce_pair();
    let authorize_url = format!(
        "{LIVE_AUTH_URL}?client_id={LIVE_PUBLIC_CLIENT_ID}&response_type=code&redirect_uri={}&scope={}&state={state}&code_challenge={challenge}&code_challenge_method=S256&prompt=select_account",
        urlencoding::encode(LIVE_DESKTOP_REDIRECT_URI),
        urlencoding::encode(LIVE_SCOPE),
    );
    if open_browser {
        let _ = open::that(&authorize_url);
    }
    DesktopAuthChallenge {
        authorize_url,
        state,
        code_verifier: verifier,
    }
}

pub fn extract_auth_code_from_redirect_or_raw(
    input: &str,
    expected_state: Option<&str>,
) -> ThkResult<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ThkError::InvalidInput(
            "Vui lòng dán URL chuyển hướng hoặc mã xác thực Microsoft.".into(),
        ));
    }

    if !trimmed.contains("://") && !trimmed.contains('?') && !trimmed.contains('&') {
        return Ok(trimmed.to_string());
    }

    let query_part = match trimmed.split_once('?') {
        Some((_, q)) => q.split('#').next().unwrap_or(q),
        None => trimmed,
    };

    let mut params = HashMap::new();
    for pair in query_part.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if let Ok(decoded) = urlencoding::decode(v) {
                params.insert(k.to_string(), decoded.into_owned());
            }
        }
    }

    if let Some(err) = params.get("error") {
        let desc = params.get("error_description").unwrap_or(err);
        return Err(ThkError::Auth(format!("Microsoft OAuth từ chối: {desc}")));
    }

    if let Some(exp_state) = expected_state {
        if let Some(got_state) = params.get("state") {
            if got_state != exp_state {
                return Err(ThkError::Auth(
                    "State OAuth không khớp (nghi vấn giả mạo phiên đăng nhập).".into(),
                ));
            }
        }
    }

    params
        .remove("code")
        .filter(|c| !c.is_empty())
        .ok_or_else(|| ThkError::InvalidInput("Không tìm thấy tham số `code` trong URL đã dán.".into()))
}

pub async fn complete_desktop_auth_challenge(
    challenge: &DesktopAuthChallenge,
    redirect_url_or_code: &str,
) -> ThkResult<AuthenticatedMinecraftSession> {
    let code = extract_auth_code_from_redirect_or_raw(redirect_url_or_code, Some(&challenge.state))?;
    let client = build_http_client()?;

    let params = [
        ("client_id", LIVE_PUBLIC_CLIENT_ID),
        ("code", code.as_str()),
        ("code_verifier", challenge.code_verifier.as_str()),
        ("grant_type", "authorization_code"),
        ("redirect_uri", LIVE_DESKTOP_REDIRECT_URI),
        ("scope", LIVE_SCOPE),
    ];

    let resp = client
        .post(LIVE_TOKEN_URL)
        .form(&params)
        .header("Accept", "application/json")
        .send()
        .await?;

    let token_body: OAuthTokenResp = resp.json().await?;
    let access_token = token_body.access_token.ok_or_else(|| {
        let desc = token_body
            .error_description
            .or(token_body.error)
            .unwrap_or_else(|| "Trao đổi mã xác thực Microsoft thất bại".into());
        ThkError::Auth(desc)
    })?;

    let authed =
        exchange_ms_tokens_to_minecraft(&client, &access_token, token_body.refresh_token).await?;
    persist_tokens_in_keyring(&authed)?;
    Ok(authed)
}

#[derive(Clone)]
struct LoopbackCallbackState {
    expected_state: String,
    result_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<ThkResult<String>>>>>,
}

pub async fn login_with_loopback_pkce(
    custom_client_id: &str,
    cancel: CancellationToken,
) -> ThkResult<AuthenticatedMinecraftSession> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| ThkError::Auth(format!("Không thể mở cổng loopback OAuth2: {e}")))?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://localhost:{port}/callback");

    let state = random_hex_state();
    let (verifier, challenge) = generate_pkce_pair();
    let (tx, rx) = tokio::sync::oneshot::channel::<ThkResult<String>>();

    let cb_state = LoopbackCallbackState {
        expected_state: state.clone(),
        result_tx: Arc::new(Mutex::new(Some(tx))),
    };

    let app = Router::new()
        .route("/callback", get(handle_oauth_loopback))
        .with_state(cb_state);

    let srv_cancel = cancel.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                srv_cancel.cancelled().await;
            })
            .await;
    });

    let auth_url = format!(
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&state={state}&code_challenge={challenge}&code_challenge_method=S256&prompt=select_account",
        urlencoding::encode(custom_client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode("XboxLive.signin offline_access"),
    );

    open::that(&auth_url)
        .map_err(|e| ThkError::Auth(format!("Không thể mở trình duyệt hệ thống: {e}")))?;

    let code = tokio::select! {
        _ = cancel.cancelled() => return Err(ThkError::Cancelled),
        res = rx => {
            cancel.cancel();
            res.map_err(|_| ThkError::Auth("Phiên callback OAuth2 đã đóng bất ngờ.".into()))??
        }
    };

    let client = build_http_client()?;
    let params = [
        ("client_id", custom_client_id),
        ("code", code.as_str()),
        ("code_verifier", verifier.as_str()),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri.as_str()),
        ("scope", "XboxLive.signin offline_access"),
    ];

    let resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&params)
        .header("Accept", "application/json")
        .send()
        .await?;

    let token_body: OAuthTokenResp = resp.json().await?;
    let access_token = token_body.access_token.ok_or_else(|| {
        let desc = token_body
            .error_description
            .or(token_body.error)
            .unwrap_or_else(|| "OAuth2 PKCE token exchange failed".into());
        ThkError::Auth(desc)
    })?;

    let authed =
        exchange_ms_tokens_to_minecraft(&client, &access_token, token_body.refresh_token).await?;
    persist_tokens_in_keyring(&authed)?;
    Ok(authed)
}

async fn handle_oauth_loopback(
    State(state): State<LoopbackCallbackState>,
    Query(params): Query<HashMap<String, String>>,
) -> Html<&'static str> {
    let outcome = if let Some(err) = params.get("error") {
        let desc = params.get("error_description").unwrap_or(err);
        Err(ThkError::Auth(format!("Microsoft OAuth error: {desc}")))
    } else if params.get("state").map(String::as_str) != Some(state.expected_state.as_str()) {
        Err(ThkError::Auth("OAuth state mismatch".into()))
    } else if let Some(code) = params.get("code") {
        Ok(code.clone())
    } else {
        Err(ThkError::Auth("Missing OAuth authorization code".into()))
    };

    let ok = outcome.is_ok();
    if let Some(sender) = state.result_tx.lock().await.take() {
        let _ = sender.send(outcome);
    }

    if ok {
        Html("<!doctype html><html><head><meta charset='utf-8'><title>TriHienKun Laucher</title></head><body style='background:#0f1117;color:#e5e7eb;font-family:sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0'><div style='text-align:center'><h2>Đăng nhập thành công!</h2><p>Bạn có thể đóng tab trình duyệt này và quay lại TriHienKun Laucher.</p></div></body></html>")
    } else {
        Html("<!doctype html><html><head><meta charset='utf-8'><title>TriHienKun Laucher</title></head><body style='background:#0f1117;color:#fca5a5;font-family:sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0'><div style='text-align:center'><h2>Đăng nhập thất bại</h2><p>Vui lòng quay lại TriHienKun Laucher để kiểm tra chi tiết lỗi.</p></div></body></html>")
    }
}

pub async fn ensure_valid_minecraft_token(
    account: &mut Account,
    custom_client_id: Option<&str>,
) -> ThkResult<String> {
    if account.account_type == AccountType::Offline {
        return Ok("0".to_string());
    }

    let stored = TokenStore::load_tokens(&account.id)?.ok_or_else(|| {
        ThkError::Auth(
            "Không tìm thấy phiên đăng nhập Microsoft trong OS Keyring. Vui lòng đăng nhập lại."
                .into(),
        )
    })?;

    let now = Utc::now().timestamp();
    if let Some(exp) = account.token_expires_at {
        if exp - now > 300 && !stored.mc_access_token.is_empty() {
            return Ok(stored.mc_access_token);
        }
    }

    let refresh_tok = stored.ms_refresh_token.ok_or_else(|| {
        ThkError::Auth("Token Minecraft đã hết hạn và không có refresh_token. Vui lòng đăng nhập lại.".into())
    })?;

    let refreshed = refresh_microsoft_session(&refresh_tok, custom_client_id).await?;
    persist_tokens_in_keyring(&refreshed)?;
    account.username = refreshed.account.username;
    account.uuid = refreshed.account.uuid;
    account.skin_url = refreshed.account.skin_url;
    account.cape_url = refreshed.account.cape_url;
    account.skin_variant = refreshed.account.skin_variant;
    account.token_expires_at = refreshed.account.token_expires_at;
    Ok(refreshed.mc_access_token)
}

pub async fn force_refresh_account_session(
    account: &Account,
    custom_client_id: Option<&str>,
) -> ThkResult<AuthenticatedMinecraftSession> {
    if account.account_type != AccountType::Microsoft {
        return Err(ThkError::InvalidInput(
            "Only Microsoft accounts support session refresh.".into(),
        ));
    }
    let stored = TokenStore::load_tokens(&account.id)?.ok_or_else(|| {
        ThkError::Auth(
            "Microsoft session not found in OS Keyring. Please sign in again.".into(),
        )
    })?;
    let refresh_tok = stored.ms_refresh_token.ok_or_else(|| {
        ThkError::Auth("No refresh_token available in OS Keyring. Please sign in again.".into())
    })?;
    let mut refreshed = refresh_microsoft_session(&refresh_tok, custom_client_id).await?;
    refreshed.account.custom_skin_path = account.custom_skin_path.clone();
    refreshed.account.custom_cape_path = account.custom_cape_path.clone();
    refreshed.account.custom_elytra_path = account.custom_elytra_path.clone();
    persist_tokens_in_keyring(&refreshed)?;
    Ok(refreshed)
}

pub async fn sync_mojang_profile(
    account: &mut Account,
    custom_client_id: Option<&str>,
) -> ThkResult<()> {
    if account.account_type != AccountType::Microsoft {
        return Ok(());
    }
    let mc_token = ensure_valid_minecraft_token(account, custom_client_id).await?;
    let client = build_http_client()?;
    let (uuid, username, skin_url, cape_url, skin_variant) =
        fetch_minecraft_profile(&client, &mc_token).await?;
    account.uuid = uuid;
    account.username = username;
    account.skin_url = skin_url;
    account.cape_url = cape_url;
    account.skin_variant = skin_variant;
    Ok(())
}

pub async fn upload_mojang_skin(
    account: &mut Account,
    custom_client_id: Option<&str>,
    variant: SkinVariant,
    png_bytes: Vec<u8>,
) -> ThkResult<()> {
    if account.account_type != AccountType::Microsoft {
        return Err(ThkError::InvalidInput(
            "Mojang Skin API is only available for Microsoft accounts.".into(),
        ));
    }
    let mc_token = ensure_valid_minecraft_token(account, custom_client_id).await?;
    let client = build_http_client()?;
    let variant_str = match variant {
        SkinVariant::Slim => "slim",
        SkinVariant::Wide => "classic",
    };
    let file_part = reqwest::multipart::Part::bytes(png_bytes)
        .file_name("skin.png")
        .mime_str("image/png")
        .map_err(ThkError::from)?;
    let form = reqwest::multipart::Form::new()
        .text("variant", variant_str)
        .part("file", file_part);

    let resp = client
        .post("https://api.minecraftservices.com/minecraft/profile/skins")
        .bearer_auth(&mc_token)
        .multipart(form)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ThkError::Auth(format!(
            "Mojang skin upload failed ({status}): {body}"
        )));
    }

    if let Ok((uuid, username, skin_url, cape_url, skin_variant)) =
        fetch_minecraft_profile(&client, &mc_token).await
    {
        account.uuid = uuid;
        account.username = username;
        account.skin_url = skin_url;
        account.cape_url = cape_url;
        account.skin_variant = skin_variant;
    } else {
        account.skin_variant = variant;
    }
    Ok(())
}

pub async fn reset_mojang_skin(
    account: &mut Account,
    custom_client_id: Option<&str>,
) -> ThkResult<()> {
    if account.account_type != AccountType::Microsoft {
        return Err(ThkError::InvalidInput(
            "Mojang Skin API is only available for Microsoft accounts.".into(),
        ));
    }
    let mc_token = ensure_valid_minecraft_token(account, custom_client_id).await?;
    let client = build_http_client()?;

    let resp = client
        .delete("https://api.minecraftservices.com/minecraft/profile/skins/active")
        .bearer_auth(&mc_token)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ThkError::Auth(format!(
            "Mojang skin reset failed ({status}): {body}"
        )));
    }

    if let Ok((uuid, username, skin_url, cape_url, skin_variant)) =
        fetch_minecraft_profile(&client, &mc_token).await
    {
        account.uuid = uuid;
        account.username = username;
        account.skin_url = skin_url;
        account.cape_url = cape_url;
        account.skin_variant = skin_variant;
    } else {
        account.skin_url = None;
    }
    Ok(())
}

pub async fn refresh_microsoft_session(
    refresh_token: &str,
    custom_client_id: Option<&str>,
) -> ThkResult<AuthenticatedMinecraftSession> {
    let client = build_http_client()?;
    let (token_url, client_id) = match custom_client_id.filter(|s| !s.trim().is_empty()) {
        Some(cid) => (
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
            cid,
        ),
        None => (LIVE_TOKEN_URL, LIVE_PUBLIC_CLIENT_ID),
    };

    let params = [
        ("client_id", client_id),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("scope", LIVE_SCOPE),
    ];

    let resp = client
        .post(token_url)
        .form(&params)
        .header("Accept", "application/json")
        .send()
        .await?;

    let body: OAuthTokenResp = resp.json().await?;
    let ms_access = body.access_token.ok_or_else(|| {
        let desc = body
            .error_description
            .or(body.error)
            .unwrap_or_else(|| "Làm mới phiên Microsoft thất bại".into());
        ThkError::Auth(desc)
    })?;

    let next_refresh = body
        .refresh_token
        .or_else(|| Some(refresh_token.to_string()));
    exchange_ms_tokens_to_minecraft(&client, &ms_access, next_refresh).await
}

fn persist_tokens_in_keyring(session: &AuthenticatedMinecraftSession) -> ThkResult<()> {
    TokenStore::save_tokens(
        &session.account.id,
        &StoredAuthTokens {
            mc_access_token: session.mc_access_token.clone(),
            ms_refresh_token: session.ms_refresh_token.clone(),
        },
    )
}

#[derive(Deserialize)]
struct XblResp {
    #[serde(rename = "Token")]
    token: Option<String>,
    #[serde(rename = "DisplayClaims")]
    display_claims: Option<XblClaims>,
    #[serde(rename = "XErr")]
    xerr: Option<u64>,
}

#[derive(Deserialize)]
struct XblClaims {
    xui: Option<Vec<XuiEntry>>,
}

#[derive(Deserialize)]
struct XuiEntry {
    uhs: Option<String>,
}

#[derive(Deserialize)]
struct McLoginResp {
    access_token: Option<String>,
    expires_in: Option<i64>,
}

#[derive(Deserialize)]
struct McProfileResp {
    id: Option<String>,
    name: Option<String>,
    skins: Option<Vec<McSkinItem>>,
    capes: Option<Vec<McCapeItem>>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct McSkinItem {
    state: Option<String>,
    url: Option<String>,
    variant: Option<String>,
}

#[derive(Deserialize)]
struct McCapeItem {
    state: Option<String>,
    url: Option<String>,
}

async fn exchange_ms_tokens_to_minecraft(
    client: &Client,
    ms_access_token: &str,
    ms_refresh_token: Option<String>,
) -> ThkResult<AuthenticatedMinecraftSession> {
    let (xbl_token, uhs) = authenticate_xbox_live(client, ms_access_token).await?;
    let xsts_token = authorize_xsts(client, &xbl_token).await?;
    let (mc_token, expires_in) = login_minecraft_with_xbox(client, &uhs, &xsts_token).await?;
    let (uuid, username, skin_url, cape_url, skin_variant) =
        fetch_minecraft_profile(client, &mc_token).await?;

    let now = Utc::now().timestamp();
    let account = Account {
        id: format!("ms-{uuid}"),
        username,
        uuid,
        account_type: AccountType::Microsoft,
        skin_variant,
        skin_url,
        cape_url,
        custom_skin_path: None,
        custom_cape_path: None,
        custom_elytra_path: None,
        token_expires_at: Some(now + expires_in),
        created_at: now,
    };

    Ok(AuthenticatedMinecraftSession {
        account,
        mc_access_token: mc_token,
        ms_refresh_token,
    })
}

async fn authenticate_xbox_live(client: &Client, ms_access_token: &str) -> ThkResult<(String, String)> {
    for rps_ticket in [format!("d={ms_access_token}"), ms_access_token.to_string()] {
        let payload = json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": rps_ticket
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        });

        let resp = client
            .post("https://user.auth.xboxlive.com/user/authenticate")
            .header("Accept", "application/json")
            .json(&payload)
            .send()
            .await?;

        if resp.status().is_success() {
            let parsed: XblResp = resp.json().await?;
            let token = parsed
                .token
                .ok_or_else(|| ThkError::Auth("Phản hồi Xbox Live thiếu Token".into()))?;
            let uhs = parsed
                .display_claims
                .and_then(|c| c.xui)
                .and_then(|mut v| v.pop())
                .and_then(|x| x.uhs)
                .ok_or_else(|| ThkError::Auth("Phản hồi Xbox Live thiếu UserHash (uhs)".into()))?;
            return Ok((token, uhs));
        }
    }

    Err(ThkError::Auth(
        "Xác thực Xbox Live thất bại (RpsTicket bị từ chối).".into(),
    ))
}

async fn authorize_xsts(client: &Client, xbl_token: &str) -> ThkResult<String> {
    let payload = json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });

    let resp = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("Accept", "application/json")
        .json(&payload)
        .send()
        .await?;

    let status = resp.status();
    let parsed: XblResp = resp.json().await?;

    if status.as_u16() == 401 {
        let msg = match parsed.xerr {
            Some(2_148_916_233) => "Tài khoản Microsoft này chưa tạo hồ sơ Xbox Live. Vui lòng đăng nhập tại xbox.com trước.".to_string(),
            Some(2_148_916_235) => "Dịch vụ Xbox Live không khả dụng tại quốc gia của tài khoản này.".to_string(),
            Some(2_148_916_238) => "Tài khoản trẻ em (dưới 18 tuổi) cần được phụ huynh thêm vào nhóm Gia đình Microsoft.".to_string(),
            Some(code) => format!("Xác thực XSTS bị từ chối (mã lỗi XErr: {code})."),
            None => "Xác thực XSTS bị từ chối (401 Unauthorized).".to_string(),
        };
        return Err(ThkError::Auth(msg));
    }

    if !status.is_success() {
        return Err(ThkError::Auth(format!("XSTS trả về mã trạng thái lỗi: {status}")));
    }

    parsed
        .token
        .ok_or_else(|| ThkError::Auth("Phản hồi XSTS thiếu Token".into()))
}

async fn login_minecraft_with_xbox(
    client: &Client,
    uhs: &str,
    xsts_token: &str,
) -> ThkResult<(String, i64)> {
    let payload = json!({
        "identityToken": format!("XBL3.0 x={uhs};{xsts_token}")
    });

    let resp = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .header("Accept", "application/json")
        .json(&payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(ThkError::Auth(format!(
            "Đăng nhập Minecraft Services thất bại ({})",
            resp.status()
        )));
    }

    let parsed: McLoginResp = resp.json().await?;
    let token = parsed
        .access_token
        .ok_or_else(|| ThkError::Auth("Minecraft Services không trả về access_token".into()))?;
    Ok((token, parsed.expires_in.unwrap_or(86_400)))
}

async fn fetch_minecraft_profile(
    client: &Client,
    mc_token: &str,
) -> ThkResult<(String, String, Option<String>, Option<String>, SkinVariant)> {
    let resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(mc_token)
        .header("Accept", "application/json")
        .send()
        .await?;

    let status = resp.status();
    let parsed: McProfileResp = resp.json().await?;

    if status.as_u16() == 404 || parsed.error.as_deref() == Some("NOT_FOUND") {
        return Err(ThkError::Auth(
            "Tài khoản Microsoft này chưa sở hữu bản quyền Minecraft: Java Edition.".into(),
        ));
    }

    if !status.is_success() {
        return Err(ThkError::Auth(format!(
            "Không thể tải hồ sơ Minecraft ({status})"
        )));
    }

    let raw_id = parsed
        .id
        .filter(|s| s.len() == 32)
        .ok_or_else(|| ThkError::Auth("UUID hồ sơ Minecraft không hợp lệ".into()))?;
    let username = parsed
        .name
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ThkError::Auth("Tên nhân vật Minecraft trống".into()))?;

    let uuid = format!(
        "{}-{}-{}-{}-{}",
        &raw_id[0..8],
        &raw_id[8..12],
        &raw_id[12..16],
        &raw_id[16..20],
        &raw_id[20..32]
    );

    let active_skin = parsed.skins.as_ref().and_then(|list| {
        list.iter()
            .find(|s| s.state.as_deref().unwrap_or("").eq_ignore_ascii_case("ACTIVE"))
            .or_else(|| list.first())
    });

    let skin_url = active_skin.and_then(|s| s.url.clone());
    let skin_variant = active_skin
        .and_then(|s| s.variant.as_deref())
        .map(SkinVariant::from_str_lossy)
        .unwrap_or(SkinVariant::Wide);

    let cape_url = parsed
        .capes
        .as_ref()
        .and_then(|list| {
            list.iter().find(|c| {
                c.state
                    .as_deref()
                    .unwrap_or("")
                    .eq_ignore_ascii_case("ACTIVE")
            })
        })
        .and_then(|c| c.url.clone());

    Ok((uuid, username, skin_url, cape_url, skin_variant))
}
