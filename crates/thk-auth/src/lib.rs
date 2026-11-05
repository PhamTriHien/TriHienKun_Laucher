#![deny(unsafe_code)]

pub mod microsoft;
pub mod offline;
pub mod token_store;
pub mod yggdrasil;

pub use microsoft::{
    build_desktop_auth_challenge, complete_desktop_auth_challenge, ensure_valid_minecraft_token,
    extract_auth_code_from_redirect_or_raw, force_refresh_account_session, login_with_loopback_pkce,
    poll_device_code_login, refresh_microsoft_session, reset_mojang_skin, start_device_code_login,
    sync_mojang_profile, upload_mojang_skin, AuthenticatedMinecraftSession, DesktopAuthChallenge,
    DeviceCodeSession,
};
pub use offline::{compute_offline_uuid, create_offline_account};
pub use token_store::{StoredAuthTokens, TokenStore};
pub use yggdrasil::{
    ensure_authlib_injector, start_local_yggdrasil, LocalYggdrasilHandle, AUTHLIB_INJECTOR_URL,
};

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine;
    use thk_core::models::SkinVariant;

    #[test]
    fn offline_uuid_v3_matches_java_name_uuid_from_bytes() {
        // Java UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes(UTF_8)) == b50ad385-829d-3141-a216-7e7d7539ba7f
        let notch_uuid = compute_offline_uuid("Notch");
        assert_eq!(notch_uuid, "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(&notch_uuid[14..15], "3");

        let acct = create_offline_account("TriHienKun", SkinVariant::Slim).unwrap();
        assert_eq!(acct.username, "TriHienKun");
        assert_eq!(acct.skin_variant, SkinVariant::Slim);
        assert!(create_offline_account("ab", SkinVariant::Wide).is_err());
        assert!(create_offline_account("bad name!", SkinVariant::Wide).is_err());
    }

    #[test]
    fn pkce_s256_and_redirect_code_parser_work() {
        // RFC 7636 Appendix B test vector
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            microsoft::compute_pkce_s256(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );

        let url = "https://login.live.com/oauth20_desktop.srf?code=M.C507_BL2.2.abc123&state=deadbeef&lc=1033";
        let code = extract_auth_code_from_redirect_or_raw(url, Some("deadbeef")).unwrap();
        assert_eq!(code, "M.C507_BL2.2.abc123");
        assert!(extract_auth_code_from_redirect_or_raw(url, Some("wrong")).is_err());
    }

    #[tokio::test]
    async fn local_yggdrasil_serves_slim_skin_and_textures() {
        let tmp = std::env::temp_dir().join(format!("thk-ygg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let skin_file = tmp.join("skin.png");
        std::fs::write(&skin_file, b"\x89PNG\r\n\x1a\ntest").unwrap();

        let mut acct = create_offline_account("TriHienKun", SkinVariant::Slim).unwrap();
        acct.custom_skin_path = Some(skin_file.clone());

        let handle = start_local_yggdrasil(&acct).await.unwrap();
        let raw_uuid = acct.uuid.replace('-', "");
        let client = reqwest::Client::new();

        let prof: serde_json::Value = client
            .get(format!(
                "http://127.0.0.1:{}/sessionserver/session/minecraft/profile/{raw_uuid}",
                handle.port
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(prof["name"], "TriHienKun");
        let b64_tex = prof["properties"][0]["value"].as_str().unwrap();
        let decoded: serde_json::Value =
            serde_json::from_slice(&BASE64.decode(b64_tex).unwrap()).unwrap();
        assert_eq!(decoded["textures"]["SKIN"]["metadata"]["model"], "slim");

        let tex_url = decoded["textures"]["SKIN"]["url"].as_str().unwrap();
        let bytes = client.get(tex_url).send().await.unwrap().bytes().await.unwrap();
        assert_eq!(&bytes[..], b"\x89PNG\r\n\x1a\ntest");

        handle.stop();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
