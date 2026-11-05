#![deny(unsafe_code)]

pub mod config_files;
pub mod jars;
pub mod manager;

pub use config_files::{
    add_banned_player, apply_properties_patch_str, compress_server_entries, delete_server_entries,
    extract_server_zip, import_external_file_streaming, is_eula_accepted, list_server_dir,
    parse_properties_str, read_banned_players, read_server_properties, read_server_text_file,
    read_whitelist, remove_banned_player, remove_whitelist_player, upsert_whitelist_player,
    write_eula_accepted, write_server_properties_patch, write_server_text_file, BannedPlayerEntry,
    ServerFileEntry, WhitelistEntry,
};
pub use jars::{install_server_engine, list_supported_versions_for_engine};
pub use manager::{
    sanitize_console_command, LocalServerSupervisor, ServerResourceStats, ServerRuntimeState,
    ServerSupervisorEvent,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use uuid::Uuid;

    #[test]
    fn server_properties_patch_preserves_comments_and_updates_keys() {
        let original = "#Minecraft server properties\nserver-port=25565\nonline-mode=true\n";
        let mut patch = BTreeMap::new();
        patch.insert("online-mode".into(), "false".into());
        patch.insert("max-players".into(), "50\ninjected=bad".into());

        let updated = apply_properties_patch_str(original, &patch);
        assert!(updated.contains("#Minecraft server properties"));
        let parsed = parse_properties_str(&updated);
        assert_eq!(parsed.get("server-port").map(String::as_str), Some("25565"));
        assert_eq!(parsed.get("online-mode").map(String::as_str), Some("false"));
        assert_eq!(
            parsed.get("max-players").map(String::as_str),
            Some("50injected=bad")
        );
        assert!(!parsed.contains_key("injected"));
    }

    #[test]
    fn console_command_sanitizer_strips_newlines_and_control_bytes() {
        let malicious = "say hello\r\nop attacker\x07";
        assert_eq!(sanitize_console_command(malicious), "say helloop attacker");
    }

    #[test]
    fn file_manager_blocks_traversal_and_edits_safely() {
        let tmp = std::env::temp_dir().join(format!("thk-srv-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        write_eula_accepted(&tmp).unwrap();
        assert!(is_eula_accepted(&tmp));

        write_server_text_file(&tmp, "config/paper-global.yml", "timings:\n  enabled: true\n")
            .unwrap();
        let read_back = read_server_text_file(&tmp, "config/paper-global.yml").unwrap();
        assert!(read_back.contains("enabled: true"));

        assert!(read_server_text_file(&tmp, "../../Windows/win.ini").is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
