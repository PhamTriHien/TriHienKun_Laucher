use keyring::Entry;
use serde::{Deserialize, Serialize};
use thk_core::{ThkError, ThkResult, APP_ID};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAuthTokens {
    pub mc_access_token: String,
    pub ms_refresh_token: Option<String>,
}

pub struct TokenStore;

impl TokenStore {
    fn entry_for(account_id: &str) -> ThkResult<Entry> {
        Entry::new(APP_ID, &format!("account:{account_id}"))
            .map_err(|e| ThkError::Auth(format!("Keyring initialization failed: {e}")))
    }

    pub fn save_tokens(account_id: &str, tokens: &StoredAuthTokens) -> ThkResult<()> {
        let payload = serde_json::to_string(tokens)?;
        let entry = Self::entry_for(account_id)?;
        entry
            .set_password(&payload)
            .map_err(|e| ThkError::Auth(format!("Failed saving credentials to OS Keyring: {e}")))
    }

    pub fn load_tokens(account_id: &str) -> ThkResult<Option<StoredAuthTokens>> {
        let entry = Self::entry_for(account_id)?;
        match entry.get_password() {
            Ok(raw) => {
                let parsed: StoredAuthTokens = serde_json::from_str(&raw)?;
                Ok(Some(parsed))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(ThkError::Auth(format!("Failed reading credentials from OS Keyring: {e}"))),
        }
    }

    pub fn delete_tokens(account_id: &str) -> ThkResult<()> {
        let entry = Self::entry_for(account_id)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(ThkError::Auth(format!("Failed deleting credentials from OS Keyring: {e}"))),
        }
    }
}
