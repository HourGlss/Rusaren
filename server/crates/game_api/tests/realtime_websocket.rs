#![allow(clippy::expect_used, clippy::too_many_lines)]

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use game_api::{
    spawn_dev_server, spawn_dev_server_with_options, DevServerOptions, ServerObservability,
    WebRtcRuntimeConfig,
};
use game_domain::{MatchOutcome, PlayerId, PlayerName, ReadyState, TeamSide};
use game_net::{
    ClientControlCommand, ServerControlEvent, ValidatedInputFrame, BUTTON_CAST, BUTTON_PRIMARY,
};
use game_sim::COMBAT_FRAME_MS;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as ClientMessage;

type ClientStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn player_name(raw: &str) -> PlayerName {
    match PlayerName::new(raw) {
        Ok(player_name) => player_name,
        Err(error) => panic!("valid player name expected: {error}"),
    }
}

async fn recv_event(stream: &mut ClientStream) -> ServerControlEvent {
    while let Some(message_result) = stream.next().await {
        match message_result {
            Ok(ClientMessage::Binary(bytes)) => match ServerControlEvent::decode_packet(&bytes) {
                Ok((_, event)) => return event,
                Err(error) => panic!("server event should decode: {error}"),
            },
            Ok(ClientMessage::Close(_)) => panic!("websocket closed before event arrived"),
            Ok(_) => {}
            Err(error) => panic!("websocket receive should work: {error}"),
        }
    }

    panic!("websocket ended before any event arrived");
}

async fn recv_events_until<F>(
    stream: &mut ClientStream,
    max_events: usize,
    mut predicate: F,
) -> Vec<ServerControlEvent>
where
    F: FnMut(&ServerControlEvent) -> bool,
{
    let mut events = Vec::new();

    for _ in 0..max_events {
        let event = recv_event(stream).await;
        let satisfied = predicate(&event);
        events.push(event);
        if satisfied {
            return events;
        }
    }

    panic!("expected predicate to succeed within {max_events} events, got {events:?}");
}

async fn recv_events_up_to<F>(
    stream: &mut ClientStream,
    max_events: usize,
    mut predicate: F,
) -> (Vec<ServerControlEvent>, bool)
where
    F: FnMut(&ServerControlEvent) -> bool,
{
    let mut events = Vec::new();

    for _ in 0..max_events {
        let event = recv_event(stream).await;
        let satisfied = predicate(&event);
        events.push(event);
        if satisfied {
            return (events, true);
        }
    }

    (events, false)
}

async fn start_server() -> (game_api::DevServerHandle, String) {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) => panic!("listener should bind: {error}"),
    };
    let server = match spawn_dev_server(listener).await {
        Ok(server) => server,
        Err(error) => panic!("server should spawn: {error}"),
    };
    let base_url = format!("ws://{}/ws-dev", server.local_addr());
    (server, base_url)
}

async fn start_server_with_options(
    options: DevServerOptions,
) -> (game_api::DevServerHandle, String) {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) => panic!("listener should bind: {error}"),
    };
    let server = match spawn_dev_server_with_options(listener, options).await {
        Ok(server) => server,
        Err(error) => panic!("server should spawn: {error}"),
    };
    let base_url = format!("ws://{}/ws-dev", server.local_addr());
    (server, base_url)
}

async fn start_server_fast() -> (game_api::DevServerHandle, String) {
    start_server_with_options(DevServerOptions {
        tick_interval: Duration::from_millis(10),
        simulation_step_ms: COMBAT_FRAME_MS,
        record_store_path: temp_record_store_path(),
        content_root: repo_content_root(),
        web_client_root: temp_web_client_root("fast-default", None),
        observability: Some(ServerObservability::new("test-fast")),
        webrtc: WebRtcRuntimeConfig::default(),
    })
    .await
}

async fn start_server_with_web_root(
    web_client_root: PathBuf,
) -> (game_api::DevServerHandle, String) {
    start_server_with_options(DevServerOptions {
        tick_interval: Duration::from_secs(1),
        simulation_step_ms: COMBAT_FRAME_MS,
        record_store_path: temp_record_store_path(),
        content_root: repo_content_root(),
        web_client_root,
        observability: Some(ServerObservability::new("test-web-root")),
        webrtc: WebRtcRuntimeConfig::default(),
    })
    .await
}

async fn connect_socket(base_url: &str) -> ClientStream {
    match connect_async(base_url).await {
        Ok((stream, _)) => stream,
        Err(error) => panic!("websocket should connect: {error}"),
    }
}

async fn connect_socket_expect_rejection(base_url: &str) -> String {
    match connect_async(base_url).await {
        Ok(_) => panic!("websocket handshake should have been rejected"),
        Err(error) => error.to_string(),
    }
}

async fn send_command(stream: &mut ClientStream, command: ClientControlCommand, seq: u32) {
    let packet = match command.encode_packet(seq, 0) {
        Ok(packet) => packet,
        Err(error) => panic!("command packet should encode: {error}"),
    };
    let _ = stream.send(ClientMessage::Binary(packet.into())).await;
}

async fn send_input(
    stream: &mut ClientStream,
    frame: ValidatedInputFrame,
    seq: u32,
    sim_tick: u32,
) {
    let packet = match frame.encode_packet(seq, sim_tick) {
        Ok(packet) => packet,
        Err(error) => panic!("input frame packet should encode: {error}"),
    };
    let _ = stream.send(ClientMessage::Binary(packet.into())).await;
}

fn slot_one_cast_input(client_input_tick: u32) -> ValidatedInputFrame {
    ValidatedInputFrame::new(client_input_tick, 0, 0, 0, 0, BUTTON_CAST, 1)
        .expect("slot one cast frame should be valid")
}

async fn cast_until_round_won(
    alice: &mut ClientStream,
    bob: &mut ClientStream,
    round: u8,
) -> (Vec<ServerControlEvent>, Vec<ServerControlEvent>) {
    let mut alice_events = Vec::new();
    let mut bob_events = Vec::new();

    for offset in 0_u32..18 {
        let sequence = u32::from(round) * 100 + offset + 1;
        send_input(alice, slot_one_cast_input(sequence), sequence, sequence).await;

        let (alice_batch, alice_finished) = recv_events_up_to(alice, 24, |event| {
            matches!(
                event,
                ServerControlEvent::RoundWon {
                    round: won_round,
                    ..
                } if won_round.get() == round
            )
        })
        .await;
        alice_events.extend(alice_batch);

        let (bob_batch, bob_finished) = recv_events_up_to(bob, 24, |event| {
            matches!(
                event,
                ServerControlEvent::RoundWon {
                    round: won_round,
                    ..
                } if won_round.get() == round
            )
        })
        .await;
        bob_events.extend(bob_batch);

        if alice_finished && bob_finished {
            return (alice_events, bob_events);
        }
    }

    panic!("expected round {round} to end after repeated slot-one casts");
}

async fn connect_player(stream: &mut ClientStream, raw_name: &str) {
    send_command(
        stream,
        ClientControlCommand::Connect {
            player_name: player_name(raw_name),
        },
        1,
    )
    .await;
}

fn connected_player_id(events: &[ServerControlEvent], expected_name: &str) -> PlayerId {
    events
        .iter()
        .find_map(|event| match event {
            ServerControlEvent::Connected {
                player_id,
                player_name,
                ..
            } if player_name.as_str() == expected_name => Some(*player_id),
            _ => None,
        })
        .expect("connect flow should include Connected with the expected player name")
}

fn temp_record_store_path() -> PathBuf {
    let unique = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(error) => panic!("system time should be after the unix epoch: {error}"),
    };
    std::env::temp_dir().join(format!("rusaren-realtime-websocket-{unique}.tsv"))
}

fn repo_content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content")
}

fn temp_web_client_root(prefix: &str, index_html: Option<&str>) -> PathBuf {
    let unique = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(error) => panic!("system time should be after the unix epoch: {error}"),
    };
    let root = std::env::temp_dir().join(format!("rusaren-web-root-{prefix}-{unique}"));
    if let Err(error) = fs::create_dir_all(&root) {
        panic!("temporary web client root should be created: {error}");
    }

    if let Some(index_html) = index_html {
        if let Err(error) = fs::write(root.join("index.html"), index_html) {
            panic!("index.html should be written: {error}");
        }
        if let Err(error) = fs::write(root.join("index.js"), "console.log('rusaren shell');") {
            panic!("index.js should be written: {error}");
        }
    }

    root
}

fn http_authority_from_ws(base_url: &str) -> String {
    let without_scheme = base_url
        .strip_prefix("ws://")
        .expect("ws:// prefix expected");
    without_scheme
        .trim_end_matches("/ws-dev")
        .trim_end_matches("/ws")
        .to_string()
}

async fn http_get(base_url: &str, path: &str) -> (u16, String) {
    let authority = http_authority_from_ws(base_url);
    let mut stream = match tokio::net::TcpStream::connect(&authority).await {
        Ok(stream) => stream,
        Err(error) => panic!("http connection should succeed: {error}"),
    };
    let request = format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    if let Err(error) = stream.write_all(request.as_bytes()).await {
        panic!("http request should be written: {error}");
    }

    let mut raw_response = Vec::new();
    if let Err(error) = stream.read_to_end(&mut raw_response).await {
        panic!("http response should be readable: {error}");
    }

    let response = match String::from_utf8(raw_response) {
        Ok(response) => response,
        Err(error) => panic!("http response should be valid utf8 for these tests: {error}"),
    };
    let (head, body) = response
        .split_once("\r\n\r\n")
        .expect("http response should contain a header/body split");
    let status_line = head.lines().next().expect("http status line should exist");
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .expect("http status line should contain a status code")
        .parse::<u16>()
        .expect("http status code should be numeric");

    (status_code, body.to_string())
}

async fn bootstrap_signal_url(base_url: &str) -> String {
    let (status_code, body) = http_get(base_url, "/session/bootstrap").await;
    assert_eq!(status_code, 200, "session bootstrap should return HTTP 200");
    let payload = serde_json::from_str::<Value>(&body).expect("bootstrap JSON should decode");
    let token = payload
        .get("token")
        .and_then(Value::as_str)
        .expect("bootstrap JSON should include a token");
    assert!(!token.is_empty(), "bootstrap token should not be blank");
    format!("{base_url}?token={token}")
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hosted_root_serves_the_exported_web_shell_and_keeps_websocket_routes_alive() {
    let web_client_root = temp_web_client_root(
        "hosted-shell",
        Some(
            "<!doctype html><html><head><title>Rusaren Control Shell</title></head><body><script src=\"index.js\"></script></body></html>",
        ),
    );
    let (server, base_url) = start_server_with_web_root(web_client_root).await;

    let (status_code, index_body) = http_get(&base_url, "/").await;
    assert_eq!(status_code, 200);
    assert!(index_body.contains("Rusaren Control Shell"));

    let (asset_status_code, asset_body) = http_get(&base_url, "/index.js").await;
    assert_eq!(asset_status_code, 200);
    assert!(asset_body.contains("rusaren shell"));

    let mut socket = connect_socket(&bootstrap_signal_url(&base_url).await).await;
    connect_player(&mut socket, "Alice").await;
    let connect_events = recv_events_until(&mut socket, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    assert!(connect_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::Connected { .. })));

    let _ = socket.close(None).await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hosted_root_returns_a_clear_placeholder_when_the_web_bundle_is_missing() {
    let web_client_root = temp_web_client_root("missing-shell", None);
    let (server, base_url) = start_server_with_web_root(web_client_root).await;

    let (status_code, body) = http_get(&base_url, "/").await;
    assert_eq!(status_code, 503);
    assert!(body.contains("Rusaren web client is not built yet."));
    assert!(body.contains("export-web-client.ps1"));

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn healthcheck_and_metrics_routes_report_expected_status_and_prometheus_text() {
    let observability = ServerObservability::new("test-metrics");
    let web_client_root = temp_web_client_root(
        "metrics-shell",
        Some("<!doctype html><html><body>metrics shell</body></html>"),
    );
    let (server, base_url) = start_server_with_options(DevServerOptions {
        tick_interval: Duration::from_millis(10),
        simulation_step_ms: COMBAT_FRAME_MS,
        record_store_path: temp_record_store_path(),
        content_root: repo_content_root(),
        web_client_root,
        observability: Some(observability.clone()),
        webrtc: WebRtcRuntimeConfig::default(),
    })
    .await;

    let (health_status, health_body) = http_get(&base_url, "/healthz").await;
    assert_eq!(health_status, 200);
    assert_eq!(health_body, "ok");

    let (root_status, root_body) = http_get(&base_url, "/").await;
    assert_eq!(root_status, 200);
    assert!(root_body.contains("metrics shell"));

    let mut socket = connect_socket(&bootstrap_signal_url(&base_url).await).await;
    connect_player(&mut socket, "Alice").await;
    let _ = recv_events_until(&mut socket, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    let _ = socket.close(None).await;

    tokio::time::sleep(Duration::from_millis(20)).await;

    let (metrics_status, metrics_body) = http_get(&base_url, "/metrics").await;
    assert_eq!(metrics_status, 200);
    assert!(metrics_body.contains("rarena_http_requests_total{route=\"healthz\"} 1"));
    assert!(metrics_body.contains("rarena_http_requests_total{route=\"root\"} 1"));
    assert!(metrics_body.contains("rarena_http_requests_total{route=\"metrics\"} 1"));
    assert!(metrics_body.contains("rarena_websocket_upgrade_attempts_total 1"));
    assert!(metrics_body.contains("rarena_websocket_sessions_bound_total 1"));
    assert!(metrics_body.contains("rarena_websocket_disconnects_total 1"));
    assert!(metrics_body.contains("rarena_build_info{version=\"test-metrics\"} 1"));

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metrics_route_returns_service_unavailable_when_observability_is_disabled() {
    let web_client_root = temp_web_client_root(
        "metrics-disabled",
        Some("<!doctype html><html><body>metrics disabled shell</body></html>"),
    );
    let (server, base_url) = start_server_with_options(DevServerOptions {
        tick_interval: Duration::from_secs(1),
        simulation_step_ms: COMBAT_FRAME_MS,
        record_store_path: temp_record_store_path(),
        content_root: repo_content_root(),
        web_client_root,
        observability: None,
        webrtc: WebRtcRuntimeConfig::default(),
    })
    .await;

    let (status_code, body) = http_get(&base_url, "/metrics").await;
    assert_eq!(status_code, 503);
    assert!(body.contains("Rusaren metrics are disabled"));

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_finishes_a_full_match_loop_via_live_input_frames() {
    let (server, base_url) = start_server_fast().await;
    let mut alice = connect_socket(&bootstrap_signal_url(&base_url).await).await;
    let mut bob = connect_socket(&bootstrap_signal_url(&base_url).await).await;

    connect_player(&mut alice, "Alice").await;
    connect_player(&mut bob, "Bob").await;
    let _ = recv_events_until(&mut alice, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;

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
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

    send_command(
        &mut bob,
        ClientControlCommand::JoinGameLobby { lobby_id },
        2,
    )
    .await;
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

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

    let alice_launch_events = recv_events_until(&mut alice, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;
    let bob_launch_events = recv_events_until(&mut bob, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;
    assert!(alice_launch_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::LaunchCountdownStarted { .. })));
    assert!(bob_launch_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::LaunchCountdownStarted { .. })));
    let alice_snapshot_events = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::ArenaStateSnapshot { .. })
    })
    .await;
    assert!(alice_snapshot_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaStateSnapshot { snapshot }
            if snapshot.players.len() == 2
                && snapshot.obstacles.len() == 100
                && snapshot.projectiles.is_empty()
    )));

    for round in 1..=5 {
        send_command(
            &mut alice,
            ClientControlCommand::ChooseSkill {
                tree: game_domain::SkillTree::Mage,
                tier: round,
            },
            4 + u32::from(round),
        )
        .await;
        send_command(
            &mut bob,
            ClientControlCommand::ChooseSkill {
                tree: game_domain::SkillTree::Rogue,
                tier: round,
            },
            4 + u32::from(round),
        )
        .await;

        let alice_skill_events = recv_events_until(&mut alice, 10, |event| {
            matches!(event, ServerControlEvent::PreCombatStarted { .. })
        })
        .await;
        let bob_skill_events = recv_events_until(&mut bob, 10, |event| {
            matches!(event, ServerControlEvent::PreCombatStarted { .. })
        })
        .await;
        assert!(alice_skill_events
            .iter()
            .any(|event| matches!(event, ServerControlEvent::SkillChosen { .. })));
        assert!(bob_skill_events
            .iter()
            .any(|event| matches!(event, ServerControlEvent::SkillChosen { .. })));

        let _ = recv_events_until(&mut alice, 8, |event| {
            matches!(event, ServerControlEvent::CombatStarted)
        })
        .await;
        let _ = recv_events_until(&mut bob, 8, |event| {
            matches!(event, ServerControlEvent::CombatStarted)
        })
        .await;

        if round < 5 {
            let (alice_round_events, bob_round_events) =
                cast_until_round_won(&mut alice, &mut bob, round).await;
            assert!(alice_round_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::ArenaEffectBatch { effects }
                    if effects.iter().any(|effect| effect.slot == 1)
            )));
            assert!(alice_round_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::RoundWon {
                    round: won,
                    winning_team: TeamSide::TeamA,
                    score_a,
                    score_b,
                } if won.get() == round && *score_a == round && *score_b == 0
            )));
            assert!(bob_round_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::RoundWon {
                    round: won,
                    winning_team: TeamSide::TeamA,
                    score_a,
                    score_b,
                } if won.get() == round && *score_a == round && *score_b == 0
            )));
        } else {
            let (mut alice_match_events, mut bob_match_events) =
                cast_until_round_won(&mut alice, &mut bob, round).await;
            alice_match_events.extend(
                recv_events_until(&mut alice, 8, |event| {
                    matches!(event, ServerControlEvent::MatchEnded { .. })
                })
                .await,
            );
            bob_match_events.extend(
                recv_events_until(&mut bob, 8, |event| {
                    matches!(event, ServerControlEvent::MatchEnded { .. })
                })
                .await,
            );
            assert!(alice_match_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::RoundWon {
                    round: won,
                    winning_team: TeamSide::TeamA,
                    score_a,
                    score_b,
                } if won.get() == round && *score_a == 5 && *score_b == 0
            )));
            assert!(alice_match_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::MatchEnded {
                    outcome: MatchOutcome::TeamAWin,
                    score_a: 5,
                    score_b: 0,
                    ..
                }
            )));
            assert!(bob_match_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::MatchEnded {
                    outcome: MatchOutcome::TeamAWin,
                    score_a: 5,
                    score_b: 0,
                    ..
                }
            )));
        }
    }

    send_command(&mut alice, ClientControlCommand::QuitToCentralLobby, 10).await;
    send_command(&mut bob, ClientControlCommand::QuitToCentralLobby, 10).await;
    let alice_return_events = recv_events_until(&mut alice, 3, |event| {
        matches!(event, ServerControlEvent::ReturnedToCentralLobby { .. })
    })
    .await;
    let bob_return_events = recv_events_until(&mut bob, 3, |event| {
        matches!(event, ServerControlEvent::ReturnedToCentralLobby { .. })
    })
    .await;
    assert!(alice_return_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ReturnedToCentralLobby { record }
            if record.wins == 1 && record.losses == 0 && record.no_contests == 0
    )));
    assert!(bob_return_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ReturnedToCentralLobby { record }
            if record.wins == 0 && record.losses == 1 && record.no_contests == 0
    )));

    let _ = alice.close(None).await;
    let _ = bob.close(None).await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_rejects_skill_tier_skips_and_accepts_the_next_valid_pick() {
    let (server, base_url) = start_server_fast().await;
    let mut alice = connect_socket(&bootstrap_signal_url(&base_url).await).await;
    let mut bob = connect_socket(&bootstrap_signal_url(&base_url).await).await;

    connect_player(&mut alice, "Alice").await;
    connect_player(&mut bob, "Bob").await;
    let _ = recv_events_until(&mut alice, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;

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
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

    send_command(
        &mut bob,
        ClientControlCommand::JoinGameLobby { lobby_id },
        2,
    )
    .await;
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

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
    let _ = recv_events_until(&mut alice, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;

    send_command(
        &mut alice,
        ClientControlCommand::ChooseSkill {
            tree: game_domain::SkillTree::Mage,
            tier: 5,
        },
        5,
    )
    .await;
    let alice_invalid_events = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::Error { .. })
    })
    .await;
    assert!(alice_invalid_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "skill progression for Mage expected tier 1 but received tier 5"
    )));

    send_command(
        &mut alice,
        ClientControlCommand::ChooseSkill {
            tree: game_domain::SkillTree::Mage,
            tier: 1,
        },
        6,
    )
    .await;
    send_command(
        &mut bob,
        ClientControlCommand::ChooseSkill {
            tree: game_domain::SkillTree::Rogue,
            tier: 1,
        },
        5,
    )
    .await;

    let alice_events = recv_events_until(&mut alice, 10, |event| {
        matches!(event, ServerControlEvent::PreCombatStarted { .. })
    })
    .await;
    let bob_events = recv_events_until(&mut bob, 10, |event| {
        matches!(event, ServerControlEvent::PreCombatStarted { .. })
    })
    .await;
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::SkillChosen {
            tree: game_domain::SkillTree::Mage,
            tier: 1,
            ..
        }
    )));
    assert!(bob_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::SkillChosen {
            tree: game_domain::SkillTree::Rogue,
            tier: 1,
            ..
        }
    )));

    let _ = alice.close(None).await;
    let _ = bob.close(None).await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_rejects_input_frames_before_combat() {
    let (server, base_url) = start_server_fast().await;
    let mut alice = connect_socket(&bootstrap_signal_url(&base_url).await).await;
    let mut bob = connect_socket(&bootstrap_signal_url(&base_url).await).await;

    connect_player(&mut alice, "Alice").await;
    connect_player(&mut bob, "Bob").await;
    let _ = recv_events_until(&mut alice, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;

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
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

    send_command(
        &mut bob,
        ClientControlCommand::JoinGameLobby { lobby_id },
        2,
    )
    .await;
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

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
    let _ = recv_events_until(&mut alice, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;

    send_input(
        &mut alice,
        ValidatedInputFrame::new(1, 0, 0, 0, 0, BUTTON_PRIMARY, 0)
            .expect("primary attack frame should be valid"),
        1,
        1,
    )
    .await;

    let alice_error_events = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::Error { .. })
    })
    .await;
    assert!(alice_error_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "input frames are only accepted during combat"
    )));

    let _ = alice.close(None).await;
    let _ = bob.close(None).await;
    server.shutdown().await;
}
