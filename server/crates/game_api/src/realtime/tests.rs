use super::*;
use game_domain::{PlayerId, PlayerName};
use game_net::{ClientControlCommand, ValidatedInputFrame};

fn test_state(
    observability: Option<ServerObservability>,
) -> (DevServerState, mpsc::UnboundedReceiver<IngressEvent>) {
    let (ingress_tx, ingress_rx) = mpsc::unbounded_channel();
    let runtime = Arc::new(Mutex::new(RuntimeState {
        app: ServerApp::new(),
        transport: RealtimeTransport::new(),
        observability: observability.clone(),
    }));
    (
        DevServerState {
            runtime,
            ingress_tx,
            web_client_root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("target")
                .join("test-temp")
                .join(format!("realtime-web-client-root-{}", std::process::id())),
            observability,
            next_connection_id: Arc::new(AtomicU64::new(1)),
            bootstrap_tokens: Arc::new(Mutex::new(SessionBootstrapTokenRegistry::new(
                Duration::from_secs(30),
            ))),
            bootstrap_rate_limiter: Arc::new(Mutex::new(SessionBootstrapRateLimiter::new(
                SESSION_BOOTSTRAP_RATE_LIMIT_WINDOW,
                SESSION_BOOTSTRAP_RATE_LIMIT_MAX_REQUESTS,
            ))),
            webrtc: WebRtcRuntimeConfig::default(),
            admin_auth: None,
        },
        ingress_rx,
    )
}

fn websocket_outbound() -> (ClientOutbound, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (ClientOutbound::WebSocket { outbound: tx }, rx)
}

fn decode_single_event(packet: &[u8]) -> ServerControlEvent {
    ServerControlEvent::decode_packet(packet)
        .expect("packet should decode")
        .1
}

#[test]
fn bootstrap_token_shape_is_url_safe_and_bounded() {
    assert!(is_valid_session_bootstrap_token_shape("abc_DEF-123"));
    assert!(!is_valid_session_bootstrap_token_shape(""));
    assert!(!is_valid_session_bootstrap_token_shape("bad token"));
    assert!(!is_valid_session_bootstrap_token_shape(&"A".repeat(128)));
}

#[test]
fn bootstrap_tokens_are_one_time_use_and_expire() {
    let now = Instant::now();
    let mut registry = SessionBootstrapTokenRegistry::new(Duration::from_millis(50));
    let token = registry.mint(now).expect("token should mint");

    assert_eq!(registry.consume(&token, now), Ok(()));
    assert_eq!(
        registry.consume(&token, now),
        Err("session bootstrap token is missing or already consumed")
    );

    let expired = registry.mint(now).expect("token should mint");
    assert_eq!(
        registry.consume(&expired, now + Duration::from_millis(60)),
        Err("session bootstrap token is missing or already consumed")
    );
}

#[test]
fn bootstrap_tokens_refuse_growth_beyond_the_hard_cap() {
    let now = Instant::now();
    let mut registry = SessionBootstrapTokenRegistry::new(Duration::from_secs(30));
    for _ in 0..MAX_SESSION_BOOTSTRAP_TOKENS {
        registry.mint(now).expect("token should mint below the cap");
    }
    let error = registry
        .mint(now)
        .expect_err("mint should fail once the registry reaches its cap");
    assert!(error.contains("registry is full"));
}

#[test]
fn bootstrap_rate_limiter_caps_requests_per_ip_and_recovers_after_the_window() {
    let ip = "203.0.113.24".parse().expect("valid ip");
    let start = Instant::now();
    let mut limiter = SessionBootstrapRateLimiter::new(Duration::from_secs(10), 2);

    assert_eq!(limiter.check_and_record(ip, start), Ok(()));
    assert_eq!(
        limiter.check_and_record(ip, start + Duration::from_secs(1)),
        Ok(())
    );
    let retry_after = limiter
        .check_and_record(ip, start + Duration::from_secs(2))
        .expect_err("third request in the same window should be limited");
    assert!(retry_after >= Duration::from_secs(7));

    assert_eq!(
        limiter.check_and_record(ip, start + Duration::from_secs(11)),
        Ok(())
    );
}

#[test]
fn realtime_transport_registers_routes_and_unregisters_clients() {
    let connection_id = ConnectionId::new(1).expect("valid connection id");
    let (outbound, mut outbound_rx) = websocket_outbound();
    let mut transport = RealtimeTransport::new();

    assert_eq!(
        transport.register_client(connection_id, outbound.clone()),
        Ok(())
    );
    assert_eq!(
        transport.register_client(connection_id, outbound),
        Err("connection is already registered")
    );

    let packet = ServerControlEvent::CombatStarted
        .encode_packet(1, 2)
        .expect("packet");
    transport.send_to_client(connection_id, packet.clone());
    assert_eq!(outbound_rx.try_recv().expect("packet should send"), packet);

    transport.enqueue(connection_id, vec![1, 2, 3]);
    assert_eq!(
        transport.recv_from_client(),
        Some((connection_id, vec![1, 2, 3]))
    );

    transport.unregister_client(connection_id);
    transport.send_to_client(connection_id, vec![9]);
    assert!(outbound_rx.try_recv().is_err());
}

#[test]
fn client_outbound_webrtc_routes_control_and_snapshot_packets() {
    let (control_tx, mut control_rx) = mpsc::unbounded_channel();
    let (snapshot_tx, mut snapshot_rx) = mpsc::unbounded_channel();
    let outbound = ClientOutbound::WebRtc {
        control: control_tx,
        snapshot: snapshot_tx,
    };

    let control_packet = ServerControlEvent::Error {
        message: String::from("bad"),
    }
    .encode_packet(0, 0)
    .expect("control packet");
    outbound.send_packet(control_packet.clone());
    assert_eq!(
        control_rx.try_recv().expect("control packet should route"),
        control_packet
    );

    let snapshot_packet = ServerControlEvent::ArenaEffectBatch { effects: vec![] }
        .encode_packet(0, 0)
        .expect("snapshot packet");
    outbound.send_packet(snapshot_packet.clone());
    assert_eq!(
        snapshot_rx
            .try_recv()
            .expect("snapshot packet should route"),
        snapshot_packet
    );

    let input_packet = ValidatedInputFrame::new(1, 0, 0, 0, 0, 0, 0)
        .expect("input frame")
        .encode_packet(1, 1)
        .expect("input packet");
    outbound.send_packet(input_packet);
    assert!(control_rx.try_recv().is_err());
    assert!(snapshot_rx.try_recv().is_err());

    outbound.send_packet(vec![1, 2, 3]);
}

#[test]
fn reject_client_session_sends_error_packet_and_records_observability() {
    let observability = ServerObservability::new("test");
    let (outbound, mut outbound_rx) = websocket_outbound();

    sessions::reject_client_session(&outbound, Some(&observability), "bad packet");

    let packet = outbound_rx.try_recv().expect("error packet should send");
    assert!(matches!(
        decode_single_event(&packet),
        ServerControlEvent::Error { message } if message == "bad packet"
    ));
    let metrics = observability.render_prometheus();
    assert!(metrics.contains("rarena_websocket_rejections_total 1"));
    assert!(metrics.contains("rarena_ingress_packets_total{result=\"rejected\"} 1"));
}

#[test]
fn allocate_connection_id_returns_monotonic_non_zero_ids() {
    let next = AtomicU64::new(1);
    let first = sessions::allocate_connection_id(&next);
    let second = sessions::allocate_connection_id(&next);

    assert_eq!(first.get(), 1);
    assert_eq!(second.get(), 2);
}

#[tokio::test]
async fn bind_initial_player_marks_the_session_bound_on_accept() {
    let observability = ServerObservability::new("test");
    let (state, mut ingress_rx) = test_state(Some(observability.clone()));
    let (outbound, _) = websocket_outbound();
    let connection_id = ConnectionId::new(3).expect("valid connection id");
    let player_id = PlayerId::new(41).expect("valid player id");
    let mut guard = NetworkSessionGuard::new();
    let mut bound_player = None;
    let packet = ClientControlCommand::Connect {
        player_name: PlayerName::new("Alice").expect("player name"),
    }
    .encode_packet(1, 0)
    .expect("connect packet");

    let responder = tokio::spawn(async move {
        match ingress_rx.recv().await.expect("ingress event") {
            IngressEvent::Connect {
                connection_id: got,
                ack,
                ..
            } => {
                assert_eq!(got, connection_id);
                ack.send(Ok(player_id)).expect("ack should send");
            }
            IngressEvent::Packet { .. } => panic!("unexpected packet ingress event"),
            IngressEvent::Disconnect { .. } => panic!("unexpected disconnect ingress event"),
        }
    });

    let keep_open = sessions::bind_initial_player(
        &state,
        connection_id,
        &outbound,
        &mut guard,
        &mut bound_player,
        packet,
    )
    .await;

    responder.await.expect("responder should finish");
    assert!(keep_open);
    assert!(guard.is_bound());
    assert_eq!(bound_player, Some(player_id));
    let metrics = observability.render_prometheus();
    assert!(metrics.contains("rarena_websocket_sessions_bound_total 1"));
}

#[tokio::test]
async fn bind_initial_player_rejects_when_connect_is_denied_or_unacknowledged() {
    let observability = ServerObservability::new("test");
    let (state, mut ingress_rx) = test_state(Some(observability.clone()));
    let (outbound, mut outbound_rx) = websocket_outbound();
    let connection_id = ConnectionId::new(4).expect("valid connection id");
    let mut guard = NetworkSessionGuard::new();
    let mut bound_player = None;
    let packet = ClientControlCommand::Connect {
        player_name: PlayerName::new("Bob").expect("player name"),
    }
    .encode_packet(1, 0)
    .expect("connect packet");

    let responder = tokio::spawn(async move {
        match ingress_rx.recv().await.expect("ingress event") {
            IngressEvent::Connect { ack, .. } => {
                ack.send(Err(String::from("denied")))
                    .expect("ack should send");
            }
            IngressEvent::Packet { .. } => panic!("unexpected packet ingress event"),
            IngressEvent::Disconnect { .. } => panic!("unexpected disconnect ingress event"),
        }
    });

    let keep_open = sessions::bind_initial_player(
        &state,
        connection_id,
        &outbound,
        &mut guard,
        &mut bound_player,
        packet.clone(),
    )
    .await;
    responder.await.expect("responder should finish");

    assert!(!keep_open);
    assert!(!guard.is_bound());
    assert_eq!(bound_player, None);
    let packet = outbound_rx.try_recv().expect("denial should send an error");
    assert!(matches!(
        decode_single_event(&packet),
        ServerControlEvent::Error { message } if message == "denied"
    ));

    let (state, ingress_rx) = test_state(None);
    drop(ingress_rx);
    let (outbound, mut outbound_rx) = websocket_outbound();
    let mut shutdown_guard = NetworkSessionGuard::new();
    let mut shutdown_player = None;
    let keep_open = sessions::bind_initial_player(
        &state,
        connection_id,
        &outbound,
        &mut shutdown_guard,
        &mut shutdown_player,
        packet,
    )
    .await;
    assert!(!keep_open);
    let packet = outbound_rx
        .try_recv()
        .expect("closed ingress path should send an error");
    assert!(matches!(
        decode_single_event(&packet),
        ServerControlEvent::Error { message } if message == "server is shutting down"
    ));
}

#[test]
fn forward_bound_packet_reports_closed_ingress_channels() {
    let observability = ServerObservability::new("test");
    let (state, mut ingress_rx) = test_state(Some(observability.clone()));
    let connection_id = ConnectionId::new(9).expect("valid connection id");
    let packet = vec![7, 8, 9];

    assert!(sessions::forward_bound_packet(
        &state,
        connection_id,
        packet.clone()
    ));
    match ingress_rx.try_recv().expect("packet should forward") {
        IngressEvent::Packet {
            connection_id: forwarded_id,
            packet: forwarded_packet,
        } => {
            assert_eq!(forwarded_id, connection_id);
            assert_eq!(forwarded_packet, packet);
        }
        IngressEvent::Connect { .. } => panic!("unexpected connect ingress event"),
        IngressEvent::Disconnect { .. } => panic!("unexpected disconnect ingress event"),
    }

    drop(ingress_rx);
    assert!(!sessions::forward_bound_packet(
        &state,
        connection_id,
        vec![1]
    ));
    let metrics = observability.render_prometheus();
    assert!(metrics.contains("rarena_websocket_rejections_total 1"));
    assert!(metrics.contains("rarena_ingress_packets_total{result=\"rejected\"} 1"));
}

#[test]
fn validate_webrtc_packet_channel_enforces_channel_and_kind_rules() {
    let control_packet = ClientControlCommand::CreateGameLobby
        .encode_packet(1, 0)
        .expect("control packet");
    assert_eq!(
        signaling::validate_webrtc_packet_channel(&control_packet, ChannelId::Control),
        Ok(())
    );
    let wrong_control =
        signaling::validate_webrtc_packet_channel(&control_packet, ChannelId::Input)
            .expect_err("control packet should be rejected on input channel");
    assert!(wrong_control.contains("does not match the Input data channel"));

    let input_packet = ValidatedInputFrame::new(1, 0, 0, 0, 0, 0, 0)
        .expect("input frame")
        .encode_packet(1, 0)
        .expect("input packet");
    assert_eq!(
        signaling::validate_webrtc_packet_channel(&input_packet, ChannelId::Input),
        Ok(())
    );
    let wrong_channel =
        signaling::validate_webrtc_packet_channel(&input_packet, ChannelId::Control)
            .expect_err("input packet should be rejected on control channel");
    assert!(wrong_channel.contains("does not match the Control data channel"));

    let snapshot_error =
        signaling::validate_webrtc_packet_channel(&input_packet, ChannelId::Snapshot)
            .expect_err("snapshot channel should always reject client packets");
    assert_eq!(
        snapshot_error,
        String::from("packet header channel Input does not match the Snapshot data channel")
    );
}
