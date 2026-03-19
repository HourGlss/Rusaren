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
