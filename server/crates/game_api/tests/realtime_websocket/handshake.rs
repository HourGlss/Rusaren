use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_accepts_binary_commands_and_broadcasts_events() {
    let (server, base_url) = start_server().await;
    let mut alice = connect_socket(&bootstrap_signal_url(&base_url).await).await;
    let mut bob = connect_socket(&bootstrap_signal_url(&base_url).await).await;

    connect_player(&mut alice, "Alice").await;
    connect_player(&mut bob, "Bob").await;

    let alice_connect_events = recv_events_until(&mut alice, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    let bob_connect_events = recv_events_until(&mut bob, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    assert!(alice_connect_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::Connected { .. })));
    assert!(bob_connect_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::Connected { .. })));
    let alice_id = connected_player_id(&alice_connect_events, "Alice");
    let bob_id = connected_player_id(&bob_connect_events, "Bob");

    send_command(&mut alice, ClientControlCommand::CreateGameLobby, 2).await;
    let created_events = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbyCreated { .. })
    })
    .await;
    let lobby_id = created_events
        .into_iter()
        .find_map(|event| match event {
            ServerControlEvent::GameLobbyCreated { lobby_id } => Some(lobby_id),
            _ => None,
        })
        .expect("create flow should include GameLobbyCreated");

    let alice_post_create = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;
    assert!(alice_post_create.iter().any(|event| matches!(
        event,
        ServerControlEvent::GameLobbyJoined { lobby_id: current, player_id: joined_player }
            if *current == lobby_id && *joined_player == alice_id
    )));

    send_command(
        &mut bob,
        ClientControlCommand::JoinGameLobby { lobby_id },
        2,
    )
    .await;
    let alice_join_events = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;
    let bob_join_events = recv_events_until(&mut bob, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;
    assert!(alice_join_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::GameLobbyJoined { lobby_id: current, player_id: joined_player }
            if *current == lobby_id && *joined_player == bob_id
    )));
    assert!(bob_join_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::GameLobbyJoined { lobby_id: current, player_id: joined_player }
            if *current == lobby_id && *joined_player == bob_id
    )));

    send_command(
        &mut alice,
        ClientControlCommand::SelectTeam {
            team: TeamSide::TeamA,
        },
        3,
    )
    .await;
    send_command(
        &mut bob,
        ClientControlCommand::SelectTeam {
            team: TeamSide::TeamB,
        },
        3,
    )
    .await;
    let _ = recv_event(&mut alice).await;
    let _ = recv_event(&mut alice).await;
    let _ = recv_event(&mut bob).await;
    let _ = recv_event(&mut bob).await;

    send_command(
        &mut alice,
        ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        },
        4,
    )
    .await;
    send_command(
        &mut bob,
        ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        },
        4,
    )
    .await;

    let alice_events = recv_events_until(&mut alice, 6, |event| {
        matches!(
            event,
            ServerControlEvent::LaunchCountdownStarted {
                lobby_id: current, ..
            } if *current == lobby_id
        )
    })
    .await;
    let bob_events = recv_events_until(&mut bob, 6, |event| {
        matches!(
            event,
            ServerControlEvent::LaunchCountdownStarted {
                lobby_id: current, ..
            } if *current == lobby_id
        )
    })
    .await;
    assert_eq!(
        alice_events
            .iter()
            .filter(|event| matches!(event, ServerControlEvent::ReadyChanged { .. }))
            .count(),
        2
    );
    assert_eq!(
        bob_events
            .iter()
            .filter(|event| matches!(event, ServerControlEvent::ReadyChanged { .. }))
            .count(),
        2
    );
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::LaunchCountdownStarted {
            lobby_id: current,
            seconds_remaining: 5,
            roster_size: 2,
        } if *current == lobby_id
    )));
    assert!(bob_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::LaunchCountdownStarted {
            lobby_id: current,
            seconds_remaining: 5,
            roster_size: 2,
        } if *current == lobby_id
    )));

    let _ = alice.close(None).await;
    let _ = bob.close(None).await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_upgrade_requires_one_time_bootstrap_tokens() {
    let (server, base_url) = start_server().await;

    let missing_token_error = connect_socket_expect_rejection(&base_url).await;
    assert!(
        missing_token_error.contains("401"),
        "missing bootstrap token should fail with HTTP 401, got: {missing_token_error}"
    );

    let tokenized_url = bootstrap_signal_url(&base_url).await;
    let stream = connect_socket(&tokenized_url).await;
    drop(stream);

    let reused_token_error = connect_socket_expect_rejection(&tokenized_url).await;
    assert!(
        reused_token_error.contains("401"),
        "reused bootstrap token should fail with HTTP 401, got: {reused_token_error}"
    );

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_rejects_text_messages() {
    let (server, base_url) = start_server().await;
    let mut socket = connect_socket(&bootstrap_signal_url(&base_url).await).await;

    let _ = socket.send(ClientMessage::Text("hello".into())).await;
    assert!(matches!(
        recv_event(&mut socket).await,
        ServerControlEvent::Error { message } if message == "text websocket messages are not accepted"
    ));

    let _ = socket.close(None).await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_rejects_non_connect_binary_first_packets() {
    let (server, base_url) = start_server().await;
    let mut socket = connect_socket(&bootstrap_signal_url(&base_url).await).await;

    send_command(
        &mut socket,
        ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        },
        1,
    )
    .await;

    assert!(matches!(
        recv_event(&mut socket).await,
        ServerControlEvent::Error { message }
            if message == "the first packet on a network session must be a connect command"
    ));

    let _ = socket.close(None).await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_rejects_connect_after_session_binding() {
    let (server, base_url) = start_server().await;
    let mut socket = connect_socket(&bootstrap_signal_url(&base_url).await).await;

    connect_player(&mut socket, "Alice").await;
    let connect_events = recv_events_until(&mut socket, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    assert!(connect_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::Connected { .. })));

    send_command(
        &mut socket,
        ClientControlCommand::Connect {
            player_name: player_name("Mallory"),
        },
        2,
    )
    .await;

    assert!(matches!(
        recv_event(&mut socket).await,
        ServerControlEvent::Error { message }
            if message == "connect commands are not accepted after a network session is bound"
    ));

    let _ = socket.close(None).await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_rejects_zero_tick_intervals() {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) => panic!("listener should bind: {error}"),
    };

    let result = spawn_dev_server_with_options(
        listener,
        DevServerOptions {
            tick_interval: Duration::ZERO,
            simulation_step_ms: COMBAT_FRAME_MS,
            record_store_path: temp_record_store_path(),
            content_root: repo_content_root(),
            web_client_root: temp_web_client_root("zero-tick", None),
            observability: Some(ServerObservability::new("test-zero-tick")),
            webrtc: WebRtcRuntimeConfig::default(),
        },
    )
    .await;

    assert!(matches!(result, Err(error) if error.kind() == std::io::ErrorKind::InvalidInput));
}
