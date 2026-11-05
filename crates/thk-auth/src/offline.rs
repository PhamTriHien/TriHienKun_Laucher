use chrono::Utc;
use md5::{Digest, Md5};
use thk_core::models::{Account, AccountType, SkinVariant};

pub fn compute_offline_uuid(username: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize().into();

    // RFC 4122 version 3 (MD5 namespace) & IETF variant
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

pub fn create_offline_account(username: &str, skin_variant: SkinVariant) -> thk_core::ThkResult<Account> {
    let clean = username.trim();
    if clean.len() < 3 || clean.len() > 16 || !clean.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(thk_core::ThkError::InvalidInput(
            "Minecraft username must be 3-16 characters (letters, digits, or underscore)".into(),
        ));
    }
    let uuid = compute_offline_uuid(clean);
    Ok(Account {
        id: format!("offline-{uuid}"),
        username: clean.to_string(),
        uuid,
        account_type: AccountType::Offline,
        skin_variant,
        skin_url: None,
        custom_skin_path: None,
        custom_cape_path: None,
        custom_elytra_path: None,
        token_expires_at: None,
        created_at: Utc::now().timestamp(),
    })
}
