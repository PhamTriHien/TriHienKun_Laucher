#![deny(unsafe_code)]

pub mod direct_host;
pub mod discord_rpc;
pub mod lan_tunnel;
pub mod ping;

pub use direct_host::{
    detect_local_network_snapshot, detect_local_network_snapshot_with_bridge,
    detect_primary_global_ipv6, detect_primary_routed_ipv4, fetch_public_ipv4_address,
    format_lan_announcement, is_cgnat_or_non_public_ipv4, is_global_unicast_ipv6,
    is_usable_lan_ipv4, probe_connected_network_snapshot, start_direct_port_host,
    ConnectedNetworkSnapshot, DirectHostActiveInfo, DirectHostConfig, DirectHostStatus,
    DirectPortHostHandle, NetworkInterfaceEntry, UpnpPortStatus,
};
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
    fn lan_scan_from_ui_thread_uses_explicit_runtime_and_releases_socket() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (coordinator, mut events) = LanShareCoordinator::new();
        assert!(tokio::runtime::Handle::try_current().is_err());
        coordinator.start_lan_scan(runtime.handle(), false).unwrap();
        assert!(coordinator.is_scanning());
        assert!(matches!(
            events.try_recv().unwrap(),
            LanTunnelEvent::ScannerStateChanged { active: true }
        ));
        coordinator.stop_scan();
        assert!(!coordinator.is_scanning());
        runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                assert!(matches!(
                    events.recv().await.unwrap(),
                    LanTunnelEvent::ScannerStateChanged { active: false }
                ));
                assert!(matches!(
                    events.recv().await.unwrap(),
                    LanTunnelEvent::ScannerStateChanged { active: false }
                ));
            })
            .await
            .unwrap();
        });
        coordinator.start_lan_scan(runtime.handle(), false).unwrap();
        coordinator.stop_scan();
        assert!(matches!(coordinator.tunnel_status(), TunnelStatus::Idle));
    }

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

        assert!(
            parse_lan_announcement("[MOTD]Bad[/MOTD][AD]0[/AD]", Ipv4Addr::LOCALHOST).is_none()
        );
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

    #[test]
    fn ipv6_host_port_hex_motd_and_discord_truncation_work() {
        assert_eq!(
            parse_host_and_port("[::1]:25565"),
            ("::1".to_string(), Some(25565))
        );
        assert_eq!(
            parse_host_and_port("[2001:db8::1]"),
            ("2001:db8::1".to_string(), None)
        );
        assert_eq!(parse_host_and_port("::1"), ("::1".to_string(), None));
        assert_eq!(
            parse_host_and_port("play.example.com:25577"),
            ("play.example.com".to_string(), Some(25577))
        );

        let hex_spans = parse_legacy_section_str("§x§0§0§f§f§8§8Neon §lServer");
        assert_eq!(hex_spans.len(), 2);
        assert_eq!(hex_spans[0].color_hex.as_deref(), Some("#00FF88"));
        assert!(!hex_spans[0].bold);
        assert_eq!(hex_spans[1].color_hex.as_deref(), Some("#00FF88"));
        assert!(hex_spans[1].bold);

        let tr_comp = json!({
            "translate": "Welcome to %s!",
            "with": [{ "text": "TriHienKun", "color": "aqua" }]
        });
        let tr_spans = parse_chat_component_to_spans(&tr_comp);
        let joined: String = tr_spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "Welcome to TriHienKun!");

        let long_vi = "Đang chơi gói Modpack siêu nặng ".repeat(6);
        let truncated = crate::discord_rpc::truncate_discord_field(&long_vi, 128);
        assert!(truncated.len() <= 128);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn tunnel_status_public_endpoint_never_duplicates_port() {
        let canonical = TunnelStatus::Running {
            local_port: 25565,
            remote_addr: "bore.pub".to_string(),
            remote_port: 25435,
        };
        assert_eq!(
            canonical.public_endpoint().as_deref(),
            Some("bore.pub:25435")
        );

        let preformatted = TunnelStatus::Running {
            local_port: 25565,
            remote_addr: "bore.pub:25435".to_string(),
            remote_port: 25435,
        };
        assert_eq!(
            preformatted.public_endpoint().as_deref(),
            Some("bore.pub:25435")
        );
        assert_eq!(TunnelStatus::Idle.public_endpoint(), None);
    }

    #[test]
    fn ip_classification_and_lan_frame_formatting_are_accurate() {
        use std::net::Ipv6Addr;

        assert!(is_global_unicast_ipv6(
            "2001:ee0:524e:3170:419:937:1f0c:2f30"
                .parse::<Ipv6Addr>()
                .unwrap()
        ));
        assert!(!is_global_unicast_ipv6(
            "fe80::d4b0:a65b:2c42:915d".parse::<Ipv6Addr>().unwrap()
        ));
        assert!(!is_global_unicast_ipv6(
            "2001:db8::1".parse::<Ipv6Addr>().unwrap()
        ));
        assert!(!is_global_unicast_ipv6(Ipv6Addr::LOCALHOST));

        assert!(is_cgnat_or_non_public_ipv4(Ipv4Addr::new(100, 64, 12, 5)));
        assert!(is_cgnat_or_non_public_ipv4(Ipv4Addr::new(
            100, 127, 255, 254
        )));
        assert!(!is_cgnat_or_non_public_ipv4(Ipv4Addr::new(100, 128, 0, 1)));
        assert!(is_cgnat_or_non_public_ipv4(Ipv4Addr::new(192, 168, 1, 5)));
        assert!(!is_cgnat_or_non_public_ipv4(Ipv4Addr::new(
            14, 234, 190, 58
        )));

        let frame = format_lan_announcement("§aTriHienKun[/MOTD] §bServer", 25565);
        assert_eq!(frame, "[MOTD]TriHienKun Server[/MOTD][AD]25565[/AD]");
        let parsed = parse_lan_announcement(&frame, Ipv4Addr::new(192, 168, 1, 5)).unwrap();
        assert_eq!(parsed.motd, "TriHienKun Server");
        assert_eq!(parsed.port, 25565);
    }

    #[tokio::test]
    async fn direct_port_host_bridges_tcp_traffic_and_stops_cleanly() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::{TcpListener, TcpStream};

        let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let upstream_port = upstream.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = upstream.accept().await {
                tokio::spawn(async move {
                    let mut req = [0u8; 4];
                    if s.read_exact(&mut req).await.is_ok() && &req == b"PING" {
                        let _ = s.write_all(b"PONG").await;
                    }
                });
            }
        });

        let probe_free = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let host_port = probe_free.local_addr().unwrap().port();
        drop(probe_free);

        let handle = start_direct_port_host(
            DirectHostConfig {
                local_target_port: upstream_port,
                host_port,
                bind_ipv4: None,
                motd: "Unit Test Host".into(),
                announce_on_lan: false,
                try_upnp: false,
            },
            None,
        )
        .await
        .expect("direct host must start");

        assert!(matches!(
            handle.current_status(),
            DirectHostStatus::Running(_)
        ));

        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, host_port))
            .await
            .expect("client must connect to hosted port");
        client.write_all(b"PING").await.unwrap();
        let mut resp = [0u8; 4];
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(&resp, b"PONG");
        drop(client);

        handle.stop();
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert!(matches!(handle.current_status(), DirectHostStatus::Stopped));
    }
}
