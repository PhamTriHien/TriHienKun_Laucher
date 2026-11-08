use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_DISCORD_APP_ID: &str = "1502586952040452249";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscordPresenceState {
    Disabled,
    Menu,
    Browsing {
        section: String,
    },
    Launching {
        version: String,
    },
    Playing {
        version: String,
        profile_name: String,
        username: String,
    },
    HostingServer {
        name: String,
        engine: String,
    },
}

enum RpcCommand {
    Update {
        client_id: Option<String>,
        state: DiscordPresenceState,
    },
    Shutdown,
}

#[derive(Clone)]
pub struct DiscordRpcService {
    tx: mpsc::Sender<RpcCommand>,
}

impl DiscordRpcService {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<RpcCommand>();
        thread::Builder::new()
            .name("thk-discord-rpc".into())
            .spawn(move || rpc_worker_loop(rx))
            .ok();
        Self { tx }
    }

    pub fn set_presence(&self, custom_client_id: Option<String>, state: DiscordPresenceState) {
        let _ = self.tx.send(RpcCommand::Update {
            client_id: custom_client_id,
            state,
        });
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(RpcCommand::Shutdown);
    }
}

fn rpc_worker_loop(rx: mpsc::Receiver<RpcCommand>) {
    let mut active_client: Option<(String, DiscordIpcClient)> = None;
    let mut current_state = DiscordPresenceState::Disabled;
    let mut desired_app_id = DEFAULT_DISCORD_APP_ID.to_string();
    let mut intentional_disconnect = true;
    let mut started_epoch = current_unix_secs();

    loop {
        let timeout_dur = if intentional_disconnect || active_client.is_some() {
            Duration::from_secs(60)
        } else {
            Duration::from_secs(15)
        };

        match rx.recv_timeout(timeout_dur) {
            Ok(RpcCommand::Shutdown) => {
                if let Some((_, mut client)) = active_client.take() {
                    let _ = client.clear_activity();
                    let _ = client.close();
                }
                break;
            }
            Ok(RpcCommand::Update { client_id, state }) => {
                let resolved_id = client_id
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| DEFAULT_DISCORD_APP_ID.to_string());

                if matches!(state, DiscordPresenceState::Disabled) {
                    intentional_disconnect = true;
                    current_state = DiscordPresenceState::Disabled;
                    if let Some((_, mut client)) = active_client.take() {
                        let _ = client.clear_activity();
                        let _ = client.close();
                    }
                    continue;
                }

                if current_state != state {
                    started_epoch = current_unix_secs();
                }
                intentional_disconnect = false;
                current_state = state;

                let needs_reinit = active_client
                    .as_ref()
                    .map(|(id, _)| id != &resolved_id)
                    .unwrap_or(true);
                desired_app_id = resolved_id;

                if needs_reinit {
                    if let Some((_, mut old)) = active_client.take() {
                        let _ = old.close();
                    }
                    active_client = try_connect_ipc(&desired_app_id);
                }

                let mut reconnected = false;
                if let Some((_, ref mut client)) = active_client {
                    if apply_activity(client, &current_state, started_epoch).is_err() {
                        let _ = client.close();
                        active_client = None;
                        reconnected = true;
                    }
                }
                if reconnected {
                    active_client = try_connect_ipc(&desired_app_id);
                    if let Some((_, ref mut client)) = active_client {
                        if apply_activity(client, &current_state, started_epoch).is_err() {
                            let _ = client.close();
                            active_client = None;
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !intentional_disconnect && active_client.is_none() {
                    active_client = try_connect_ipc(&desired_app_id);
                    if let Some((_, ref mut client)) = active_client {
                        if apply_activity(client, &current_state, started_epoch).is_err() {
                            let _ = client.close();
                            active_client = None;
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some((_, mut client)) = active_client.take() {
                    let _ = client.close();
                }
                break;
            }
        }
    }
}

fn try_connect_ipc(app_id: &str) -> Option<(String, DiscordIpcClient)> {
    let mut client = DiscordIpcClient::new(app_id);
    client.connect().ok()?;
    Some((app_id.to_string(), client))
}

fn apply_activity(
    client: &mut DiscordIpcClient,
    state: &DiscordPresenceState,
    started_epoch: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let (details_text, state_text) = match state {
        DiscordPresenceState::Disabled => return Ok(()),
        DiscordPresenceState::Menu => (
            "TriHienKun Laucher".to_string(),
            "Đang ở màn hình chính".to_string(),
        ),
        DiscordPresenceState::Browsing { section } => (
            "TriHienKun Laucher".to_string(),
            format!("Đang xem: {section}"),
        ),
        DiscordPresenceState::Launching { version } => (
            format!("Đang khởi chạy Minecraft {version}"),
            "Chuẩn bị tài nguyên game...".to_string(),
        ),
        DiscordPresenceState::Playing {
            version,
            profile_name,
            username,
        } => (
            format!("Đang chơi Minecraft {version}"),
            format!("{profile_name} · {username}"),
        ),
        DiscordPresenceState::HostingServer { name, engine } => (
            format!("Đang mở Server: {name}"),
            format!("Engine: {engine}"),
        ),
    };

    let safe_details = truncate_discord_field(&details_text, 128);
    let safe_state = truncate_discord_field(&state_text, 128);

    let payload = activity::Activity::new()
        .details(&safe_details)
        .state(&safe_state)
        .timestamps(activity::Timestamps::new().start(started_epoch));

    client.set_activity(payload)?;
    Ok(())
}

pub fn truncate_discord_field(s: &str, max_bytes: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() < 2 {
        return format!("{trimmed}  ");
    }
    if trimmed.len() <= max_bytes {
        return trimmed.to_string();
    }
    let ellipsis = "…";
    let budget = max_bytes.saturating_sub(ellipsis.len());
    let mut end = budget.min(trimmed.len());
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{ellipsis}", &trimmed[..end])
}

fn current_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
