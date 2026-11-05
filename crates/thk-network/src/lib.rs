#![deny(unsafe_code)]

pub mod discord_rpc;
pub mod lan_tunnel;
pub mod ping;

pub use discord_rpc::{DiscordPresenceState, DiscordRpcService, DEFAULT_DISCORD_APP_ID};
pub use lan_tunnel::{
    is_local_interface_ipv4, parse_lan_announcement, start_bore_tunnel, BoreTunnelHandle,
    LanShareCoordinator, LanTunnelEvent, LanWorldAnnouncement, TunnelStatus, DEFAULT_BORE_SERVER,
    MC_LAN_MULTICAST_GROUP, MC_LAN_MULTICAST_PORT,
};
pub use ping::{
    decode_varint_slice, detect_primary_lan_ipv4, encode_varint, parse_chat_component_to_spans,
    parse_host_and_port, parse_legacy_section_str, ping_minecraft_server, resolve_minecraft_srv,
    strip_section_codes, MotdSpan, ServerStatusPing,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::net::Ipv4Addr;

    #[test]
    fn varint_roundtrip_handles_positive_and_protocol_ids() {
        for sample in [0, 1, 127, 128, 255, 767, 25565, 2_097_151] {
            let mut buf = Vec::new();
            encode_varint(sample, &mut buf);
            let (decoded, used) = decode_varint_slice(&buf).expect("valid varint");
            assert_eq!(decoded, sample);
            assert_eq!(used, buf.len());
        }
    }

    #[test]
    fn lan_announcement_parser_extracts_motd_and_port() {
        let pkt = "[MOTD]§aTriHienKun §lSurvival[/MOTD][AD]54321[/AD]";
        let parsed =
            parse_lan_announcement(pkt, Ipv4Addr::LOCALHOST).expect("valid LAN announcement");
        assert_eq!(parsed.motd, "TriHienKun Survival");
        assert_eq!(parsed.port, 54321);
        assert_eq!(parsed.sender_ip, Ipv4Addr::LOCALHOST);

        assert!(parse_lan_announcement("[MOTD]Bad[/MOTD][AD]0[/AD]", Ipv4Addr::LOCALHOST).is_none());
    }

    #[test]
    fn motd_component_and_section_parser_preserves_colors_and_styles() {
        let comp = json!({
            "text": "",
            "extra": [
                { "text": "TriHienKun ", "color": "#55FF55", "bold": true },
                "§eLaucher §rOnline"
            ]
        });
        let spans = parse_chat_component_to_spans(&comp);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].text, "TriHienKun ");
        assert_eq!(spans[0].color_hex.as_deref(), Some("#55FF55"));
        assert!(spans[0].bold);
        assert_eq!(spans[1].text, "Laucher ");
        assert_eq!(spans[1].color_hex.as_deref(), Some("#FFFF55"));
        assert_eq!(spans[2].text, "Online");
        assert!(spans[2].color_hex.is_none());
    }

    #[test]
    fn local_interface_verifier_accepts_loopback_and_rejects_public_ip() {
        assert!(is_local_interface_ipv4(Ipv4Addr::LOCALHOST));
        assert!(!is_local_interface_ipv4(Ipv4Addr::new(198, 51, 100, 42)));
    }
}
