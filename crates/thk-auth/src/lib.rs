#![deny(unsafe_code)]

pub mod microsoft;
pub mod offline;
pub mod token_store;
pub mod yggdrasil;

pub use microsoft::{
    build_desktop_auth_challenge, complete_desktop_auth_challenge, ensure_valid_minecraft_token,
    extract_auth_code_from_redirect_or_raw, force_refresh_account_session,
    login_with_loopback_pkce, poll_device_code_login, refresh_microsoft_session, reset_mojang_skin,
    start_device_code_login, sync_mojang_profile, upload_mojang_skin,
    AuthenticatedMinecraftSession, DesktopAuthChallenge, DeviceCodeSession,
};
pub use offline::{compute_offline_uuid, create_offline_account};
pub use token_store::{StoredAuthTokens, TokenStore};
pub use yggdrasil::{
    ensure_authlib_injector, normalize_cape_png_bytes_for_minecraft,
    normalize_skin_png_bytes_for_minecraft, start_local_yggdrasil,
    start_local_yggdrasil_with_paths, LocalYggdrasilHandle, AUTHLIB_INJECTOR_URL,
};

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine;
    use image::{ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;
    use std::path::Path;
    use thk_core::models::SkinVariant;

    fn encode_test_png(img: &RgbaImage) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png).unwrap();
        buf.into_inner()
    }

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
        assert!(extract_auth_code_from_redirect_or_raw(
            "https://login.live.com/oauth20_desktop.srf?code=abc",
            Some("deadbeef")
        )
        .is_err());
        assert_eq!(
            extract_auth_code_from_redirect_or_raw("raw-code", Some("deadbeef")).unwrap(),
            "raw-code"
        );
    }

    #[test]
    fn skin_normalization_converts_hd_and_legacy_skins_to_64x64_without_bleed() {
        let mut hd_128 = RgbaImage::from_pixel(128, 128, Rgba([0, 0, 0, 0]));
        // Populate a 2x2 block at target texel (8, 8): 2 bright gold pixels + 2 transparent pixels
        hd_128.put_pixel(16, 16, Rgba([240, 200, 80, 255]));
        hd_128.put_pixel(17, 16, Rgba([240, 200, 80, 255]));
        let norm_bytes = normalize_skin_png_bytes_for_minecraft(&encode_test_png(&hd_128)).unwrap();
        let norm_img = image::load_from_memory(&norm_bytes).unwrap().to_rgba8();
        assert_eq!(norm_img.dimensions(), (64, 64));
        assert_eq!(*norm_img.get_pixel(8, 8), Rgba([240, 200, 80, 128]));
        assert_eq!(*norm_img.get_pixel(9, 8), Rgba([0, 0, 0, 0]));

        let mut legacy = RgbaImage::from_pixel(64, 32, Rgba([0, 0, 0, 0]));
        for y in 0..16 {
            for x in 32..64 {
                legacy.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        legacy.put_pixel(4, 20, Rgba([50, 120, 210, 255]));
        let leg_bytes = normalize_skin_png_bytes_for_minecraft(&encode_test_png(&legacy)).unwrap();
        let leg_img = image::load_from_memory(&leg_bytes).unwrap().to_rgba8();
        assert_eq!(leg_img.dimensions(), (64, 64));
        assert_eq!(leg_img.get_pixel(40, 4)[3], 0);
        assert_eq!(*leg_img.get_pixel(23, 52), Rgba([50, 120, 210, 255]));
    }

    #[tokio::test]
    async fn local_yggdrasil_serves_slim_skin_and_textures() {
        let tmp = std::env::temp_dir().join(format!("thk-ygg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        let mut hd_skin = RgbaImage::from_pixel(128, 128, Rgba([0, 0, 0, 0]));
        for y in 16..32 {
            for x in 16..32 {
                hd_skin.put_pixel(x, y, Rgba([34, 197, 94, 255]));
            }
        }
        let skin_file = tmp.join("skin_128.png");
        std::fs::write(&skin_file, encode_test_png(&hd_skin)).unwrap();

        let cape_raw = RgbaImage::from_pixel(22, 17, Rgba([20, 80, 160, 255]));
        let cape_file = tmp.join("cape_22x17.png");
        std::fs::write(&cape_file, encode_test_png(&cape_raw)).unwrap();

        let elytra_raw = RgbaImage::from_pixel(64, 32, Rgba([180, 60, 220, 255]));
        let elytra_file = tmp.join("elytra_64x32.png");
        std::fs::write(&elytra_file, encode_test_png(&elytra_raw)).unwrap();

        let mut acct = create_offline_account("TriHienKun", SkinVariant::Slim).unwrap();
        acct.custom_skin_path = Some(skin_file);
        acct.custom_cape_path = Some(cape_file);
        acct.custom_elytra_path = Some(elytra_file);

        let handle = start_local_yggdrasil(&acct).await.unwrap();
        let jvm_args = handle.jvm_args(Path::new("authlib-injector.jar"));
        assert_eq!(jvm_args.len(), 2);
        let prefetched_b64 = jvm_args[0]
            .strip_prefix("-Dauthlibinjector.yggdrasil.prefetched=")
            .unwrap();
        let prefetched_json: serde_json::Value =
            serde_json::from_slice(&BASE64.decode(prefetched_b64).unwrap()).unwrap();
        assert!(prefetched_json["signaturePublickey"]
            .as_str()
            .unwrap()
            .starts_with("-----BEGIN PUBLIC KEY-----\n"));
        assert!(jvm_args[1].starts_with("-javaagent:authlib-injector.jar=http://127.0.0.1:"));

        let raw_uuid = acct.uuid.replace('-', "");
        thk_core::downloader::ensure_rustls_crypto_provider();
        let client = reqwest::Client::new();

        let root_meta: serde_json::Value = client
            .get(format!("http://127.0.0.1:{}/", handle.port))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let pem = root_meta["signaturePublickey"].as_str().unwrap();
        assert!(pem.starts_with("-----BEGIN PUBLIC KEY-----\n"));

        let prof: serde_json::Value = client
            .get(format!(
                "http://127.0.0.1:{}/sessionserver/session/minecraft/profile/{raw_uuid}?unsigned=false",
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
        let sig_b64 = prof["properties"][0]["signature"].as_str().unwrap();
        assert!(yggdrasil::local_yggdrasil_keypair()
            .verify_sha1_pkcs1_v15_base64(b64_tex.as_bytes(), sig_b64));

        let decoded: serde_json::Value =
            serde_json::from_slice(&BASE64.decode(b64_tex).unwrap()).unwrap();
        assert_eq!(decoded["textures"]["SKIN"]["metadata"]["model"], "slim");

        let skin_url = decoded["textures"]["SKIN"]["url"].as_str().unwrap();
        let cape_url = decoded["textures"]["CAPE"]["url"].as_str().unwrap();
        let elytra_url = decoded["textures"]["ELYTRA"]["url"].as_str().unwrap();
        for url in [skin_url, cape_url, elytra_url] {
            assert!(
                !url.contains('?'),
                "Texture URL must not contain query: {url}"
            );
            let hash_seg = url.rsplit('/').next().unwrap();
            assert_eq!(hash_seg.len(), 64);
            assert!(hash_seg.chars().all(|c| c.is_ascii_hexdigit()));
        }
        assert_ne!(cape_url, elytra_url);

        let served_skin_bytes = client
            .get(skin_url)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let served_skin_img = image::load_from_memory(&served_skin_bytes)
            .unwrap()
            .to_rgba8();
        assert_eq!(served_skin_img.dimensions(), (64, 64));
        assert_eq!(*served_skin_img.get_pixel(8, 8), Rgba([34, 197, 94, 255]));

        let served_cape_bytes = client
            .get(cape_url)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let served_cape_img = image::load_from_memory(&served_cape_bytes)
            .unwrap()
            .to_rgba8();
        assert_eq!(served_cape_img.dimensions(), (64, 32));

        let lookup_resp: serde_json::Value = client
            .get(format!(
                "http://127.0.0.1:{}/api/users/profiles/minecraft/trihienkun",
                handle.port
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(lookup_resp["id"], raw_uuid);
        assert_eq!(lookup_resp["name"], "TriHienKun");

        let bulk_resp: serde_json::Value = client
            .post(format!(
                "http://127.0.0.1:{}/api/profiles/minecraft",
                handle.port
            ))
            .json(&vec!["TRIHienKun", "UnknownPlayer"])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let arr = bulk_resp.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], raw_uuid);

        let join_resp = client
            .post(format!(
                "http://127.0.0.1:{}/sessionserver/session/minecraft/join",
                handle.port
            ))
            .json(&serde_json::json!({
                "accessToken": "0",
                "selectedProfile": raw_uuid,
                "serverId": "local-test"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(join_resp.status(), reqwest::StatusCode::NO_CONTENT);

        let has_joined: serde_json::Value = client
            .get(format!(
                "http://127.0.0.1:{}/sessionserver/session/minecraft/hasJoined?username=trihienkun&serverId=local-test",
                handle.port
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(has_joined["id"], raw_uuid);
        assert!(has_joined["properties"][0]["signature"].as_str().is_some());

        let attrs: serde_json::Value = client
            .get(format!(
                "http://127.0.0.1:{}/minecraftservices/player/attributes",
                handle.port
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(attrs["privileges"]["multiplayerServer"]["enabled"], true);

        let unknown_resp = client
            .get(format!(
                "http://127.0.0.1:{}/sessionserver/session/minecraft/profile/00000000000000000000000000000000",
                handle.port
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(unknown_resp.status(), reqwest::StatusCode::NO_CONTENT);

        handle.stop();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn offline_slim_account_without_custom_skin_serves_fallback_slim_texture() {
        let acct = create_offline_account("SlimDefault", SkinVariant::Slim).unwrap();
        let handle = start_local_yggdrasil(&acct).await.unwrap();
        let raw_uuid = acct.uuid.replace('-', "");
        thk_core::downloader::ensure_rustls_crypto_provider();
        let client = reqwest::Client::new();

        let prof: serde_json::Value = client
            .get(format!(
                "http://127.0.0.1:{}/sessionserver/session/minecraft/profile/{raw_uuid}?unsigned=false",
                handle.port
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let b64_tex = prof["properties"][0]["value"].as_str().unwrap();
        let decoded: serde_json::Value =
            serde_json::from_slice(&BASE64.decode(b64_tex).unwrap()).unwrap();
        assert_eq!(decoded["textures"]["SKIN"]["metadata"]["model"], "slim");
        let skin_url = decoded["textures"]["SKIN"]["url"].as_str().unwrap();
        let bytes = client
            .get(skin_url)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let img = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (64, 64));
        handle.stop();
    }
}
