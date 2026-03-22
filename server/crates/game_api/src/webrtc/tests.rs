use super::*;

fn config_with_turn() -> WebRtcRuntimeConfig {
    WebRtcRuntimeConfig {
        stun_urls: vec![String::from("stun:turn.example.com:3478")],
        turn_urls: vec![
            String::from("turn:turn.example.com:3478?transport=udp"),
            String::from("turn:turn.example.com:3478?transport=tcp"),
        ],
        turn_shared_secret: Some(String::from("shared-secret")),
        turn_ttl: Duration::from_secs(600),
    }
}

#[test]
fn signaling_decoder_rejects_answers_from_clients_and_unknown_fields() {
    let answer = r#"{"type":"session_description","description":{"type":"answer","sdp":"v=0"}}"#;
    assert_eq!(
        decode_client_signal_message(answer),
        Err(String::from(
            "clients may only submit offer session descriptions"
        ))
    );

    let unknown_field = r#"{"type":"bye","extra":true}"#;
    let result = decode_client_signal_message(unknown_field);
    assert!(matches!(
        result,
        Err(message) if message.starts_with("invalid signaling JSON:")
    ));
}

#[test]
fn signaling_decoder_accepts_offers_and_candidates() {
    let offer = r#"{"type":"session_description","description":{"type":"offer","sdp":"v=0\r\n"}}"#;
    assert!(matches!(
        decode_client_signal_message(offer),
        Ok(ClientSignalMessage::SessionDescription { .. })
    ));

    let candidate = r#"{"type":"ice_candidate","candidate":{"candidate":"candidate:0 1 UDP 2122252543 127.0.0.1 5000 typ host","sdp_mid":"0","sdp_mline_index":0}}"#;
    assert!(matches!(
        decode_client_signal_message(candidate),
        Ok(ClientSignalMessage::IceCandidate { .. })
    ));
}

#[test]
fn runtime_config_generates_ephemeral_turn_credentials() {
    let connection_id = ConnectionId::new(7).expect("valid connection id");
    let ice_servers = config_with_turn()
        .ice_servers_for_connection(connection_id, UNIX_EPOCH + Duration::from_secs(1_000))
        .expect("ICE servers should build");

    assert_eq!(ice_servers.len(), 2);
    assert_eq!(
        ice_servers[0].urls,
        vec![String::from("stun:turn.example.com:3478")]
    );
    assert_eq!(ice_servers[1].urls.len(), 2);
    assert!(ice_servers[1].username.starts_with("1600:conn-7"));
    assert!(!ice_servers[1].credential.is_empty());
}

#[test]
fn runtime_config_requires_secret_for_turn_urls() {
    let config = WebRtcRuntimeConfig {
        stun_urls: Vec::new(),
        turn_urls: vec![String::from("turn:turn.example.com:3478?transport=udp")],
        turn_shared_secret: None,
        turn_ttl: Duration::from_secs(300),
    };

    assert_eq!(
        config.validate(),
        Err(String::from(
            "TURN URLs require RARENA_WEBRTC_TURN_SECRET to be configured"
        ))
    );
}

#[test]
fn runtime_config_rejects_blank_urls_and_zero_ttl() {
    let blank_stun = WebRtcRuntimeConfig {
        stun_urls: vec![
            String::from("stun:turn.example.com:3478"),
            String::from("  "),
        ],
        turn_urls: Vec::new(),
        turn_shared_secret: None,
        turn_ttl: Duration::from_secs(300),
    };
    assert_eq!(
        blank_stun.validate(),
        Err(String::from("STUN URLs must not contain blank entries"))
    );

    let blank_turn = WebRtcRuntimeConfig {
        stun_urls: Vec::new(),
        turn_urls: vec![String::from("turn:turn.example.com:3478"), String::new()],
        turn_shared_secret: Some(String::from("shared-secret")),
        turn_ttl: Duration::from_secs(300),
    };
    assert_eq!(
        blank_turn.validate(),
        Err(String::from("TURN URLs must not contain blank entries"))
    );

    let zero_ttl = WebRtcRuntimeConfig {
        stun_urls: Vec::new(),
        turn_urls: Vec::new(),
        turn_shared_secret: None,
        turn_ttl: Duration::ZERO,
    };
    assert_eq!(
        zero_ttl.validate(),
        Err(String::from(
            "TURN credential TTL must be greater than zero"
        ))
    );
}

#[test]
fn ice_server_config_validates_and_converts_cleanly() {
    let server = WebRtcIceServerConfig {
        urls: vec![String::from("stun:turn.example.com:3478")],
        username: String::from("user"),
        credential: String::from("secret"),
    };
    let rtc = server.to_rtc_ice_server();
    assert_eq!(rtc.urls, server.urls);
    assert_eq!(rtc.username, server.username);
    assert_eq!(rtc.credential, server.credential);

    let empty = WebRtcIceServerConfig {
        urls: Vec::new(),
        username: String::new(),
        credential: String::new(),
    };
    assert_eq!(
        empty.validate(),
        Err(String::from(
            "ICE server configuration requires at least one URL"
        ))
    );

    let blank = WebRtcIceServerConfig {
        urls: vec![String::from("   ")],
        username: String::new(),
        credential: String::new(),
    };
    assert_eq!(
        blank.validate(),
        Err(String::from("ICE server URLs must not be blank"))
    );
}

#[test]
fn runtime_config_reports_clock_and_url_validation_failures_from_ice_servers() {
    let connection_id = ConnectionId::new(9).expect("valid connection id");
    let before_epoch = config_with_turn()
        .ice_servers_for_connection(connection_id, UNIX_EPOCH - Duration::from_secs(1));
    assert!(matches!(
        before_epoch,
        Err(message) if message.starts_with("system clock is before the unix epoch:")
    ));

    let invalid_url = WebRtcRuntimeConfig {
        stun_urls: vec![String::from(" ")],
        turn_urls: Vec::new(),
        turn_shared_secret: None,
        turn_ttl: Duration::from_secs(60),
    };
    assert_eq!(
        invalid_url.ice_servers_for_connection(connection_id, UNIX_EPOCH),
        Err(String::from("STUN URLs must not contain blank entries"))
    );
}
