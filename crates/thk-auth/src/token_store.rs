#[cfg(not(target_os = "android"))]
use keyring::Entry;
use serde::{Deserialize, Serialize};
use thk_core::ThkResult;
#[cfg(not(target_os = "android"))]
use thk_core::{ThkError, APP_ID};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAuthTokens {
    pub mc_access_token: String,
    pub ms_refresh_token: Option<String>,
}

pub struct TokenStore;

#[cfg(not(target_os = "android"))]
impl TokenStore {
    fn legacy_entry_for(account_id: &str) -> ThkResult<Entry> {
        Entry::new(APP_ID, &format!("account:{account_id}"))
            .map_err(|e| ThkError::Auth(format!("Keyring initialization failed: {e}")))
    }

    fn mc_entry_for(account_id: &str) -> ThkResult<Entry> {
        Entry::new(APP_ID, &format!("account:{account_id}:mc"))
            .map_err(|e| ThkError::Auth(format!("Keyring initialization failed: {e}")))
    }

    fn ms_entry_for(account_id: &str) -> ThkResult<Entry> {
        Entry::new(APP_ID, &format!("account:{account_id}:ms"))
            .map_err(|e| ThkError::Auth(format!("Keyring initialization failed: {e}")))
    }

    pub fn save_tokens(account_id: &str, tokens: &StoredAuthTokens) -> ThkResult<()> {
        let mc_entry = Self::mc_entry_for(account_id)?;
        mc_entry
            .set_password(&tokens.mc_access_token)
            .map_err(|e| {
                ThkError::Auth(format!(
                    "Failed saving Minecraft access token to OS Keyring: {e}"
                ))
            })?;

        let ms_entry = Self::ms_entry_for(account_id)?;
        match &tokens.ms_refresh_token {
            Some(refresh) if !refresh.is_empty() => {
                ms_entry.set_password(refresh).map_err(|e| {
                    ThkError::Auth(format!(
                        "Failed saving Microsoft refresh token to OS Keyring: {e}"
                    ))
                })?;
            }
            _ => match ms_entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => {
                    return Err(ThkError::Auth(format!(
                        "Failed clearing stale refresh token from OS Keyring: {e}"
                    )));
                }
            },
        }

        if let Ok(legacy) = Self::legacy_entry_for(account_id) {
            let _ = legacy.delete_credential();
        }
        Ok(())
    }

    pub fn load_tokens(account_id: &str) -> ThkResult<Option<StoredAuthTokens>> {
        let mc_entry = Self::mc_entry_for(account_id)?;
        match mc_entry.get_password() {
            Ok(mc_access_token) => {
                let ms_entry = Self::ms_entry_for(account_id)?;
                let ms_refresh_token = match ms_entry.get_password() {
                    Ok(val) if !val.is_empty() => Some(val),
                    Ok(_) | Err(keyring::Error::NoEntry) => None,
                    Err(e) => {
                        return Err(ThkError::Auth(format!(
                            "Failed reading refresh token from OS Keyring: {e}"
                        )));
                    }
                };
                return Ok(Some(StoredAuthTokens {
                    mc_access_token,
                    ms_refresh_token,
                }));
            }
            Err(keyring::Error::NoEntry) => {}
            Err(e) => {
                return Err(ThkError::Auth(format!(
                    "Failed reading Minecraft token from OS Keyring: {e}"
                )));
            }
        }

        let legacy = Self::legacy_entry_for(account_id)?;
        match legacy.get_password() {
            Ok(raw) => {
                let parsed: StoredAuthTokens = serde_json::from_str(&raw)?;
                let _ = Self::save_tokens(account_id, &parsed);
                Ok(Some(parsed))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(ThkError::Auth(format!(
                "Failed reading credentials from OS Keyring: {e}"
            ))),
        }
    }

    pub fn delete_tokens(account_id: &str) -> ThkResult<()> {
        for entry_res in [
            Self::mc_entry_for(account_id),
            Self::ms_entry_for(account_id),
            Self::legacy_entry_for(account_id),
        ] {
            let entry = entry_res?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => {
                    return Err(ThkError::Auth(format!(
                        "Failed deleting credentials from OS Keyring: {e}"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "android")]
impl TokenStore {
    pub fn save_tokens(account_id: &str, tokens: &StoredAuthTokens) -> ThkResult<()> {
        thk_core::store_sandboxed_secret(
            &format!("account:{account_id}:mc"),
            &tokens.mc_access_token,
        )?;
        match &tokens.ms_refresh_token {
            Some(refresh) if !refresh.is_empty() => {
                thk_core::store_sandboxed_secret(&format!("account:{account_id}:ms"), refresh)?;
            }
            _ => {
                let _ = thk_core::delete_sandboxed_secret(&format!("account:{account_id}:ms"));
            }
        }
        Ok(())
    }

    pub fn load_tokens(account_id: &str) -> ThkResult<Option<StoredAuthTokens>> {
        let Some(mc_access_token) =
            thk_core::load_sandboxed_secret(&format!("account:{account_id}:mc"))?
        else {
            return Ok(None);
        };
        let ms_refresh_token =
            thk_core::load_sandboxed_secret(&format!("account:{account_id}:ms"))?
                .filter(|v| !v.is_empty());
        Ok(Some(StoredAuthTokens {
            mc_access_token,
            ms_refresh_token,
        }))
    }

    pub fn delete_tokens(account_id: &str) -> ThkResult<()> {
        thk_core::delete_sandboxed_secret(&format!("account:{account_id}:mc"))?;
        thk_core::delete_sandboxed_secret(&format!("account:{account_id}:ms"))?;
        Ok(())
    }
}
