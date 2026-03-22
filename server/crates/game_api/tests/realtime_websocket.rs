#![allow(clippy::expect_used, clippy::too_many_lines)]

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use game_api::{
    spawn_dev_server, spawn_dev_server_with_options, DevServerOptions, ServerObservability,
    WebRtcRuntimeConfig,
};
use game_domain::{MatchOutcome, PlayerId, PlayerName, ReadyState, SkillTree, TeamSide};
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

async fn recv_event_with_timeout(
    stream: &mut ClientStream,
    timeout: Duration,
) -> Option<ServerControlEvent> {
    tokio::time::timeout(timeout, recv_event(stream)).await.ok()
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

async fn recv_events_until_within<F>(
    stream: &mut ClientStream,
    timeout: Duration,
    max_events: usize,
    mut predicate: F,
) -> Vec<ServerControlEvent>
where
    F: FnMut(&ServerControlEvent) -> bool,
{
    let mut events = Vec::new();
    let start = Instant::now();

    for _ in 0..max_events {
        let Some(remaining) = timeout.checked_sub(start.elapsed()) else {
            break;
        };
        let Some(event) = recv_event_with_timeout(stream, remaining).await else {
            break;
        };
        let satisfied = predicate(&event);
        events.push(event);
        if satisfied {
            return events;
        }
    }

    panic!(
        "expected predicate to succeed within {timeout:?} and {max_events} events, got {events:?}"
    );
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

async fn drain_pending_events(
    stream: &mut ClientStream,
    quiet_period: Duration,
    max_events: usize,
) -> Vec<ServerControlEvent> {
    let mut events = Vec::new();

    for _ in 0..max_events {
        let Some(event) = recv_event_with_timeout(stream, quiet_period).await else {
            break;
        };
        events.push(event);
    }

    events
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
        admin_auth: None,
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
        admin_auth: None,
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
        tokio::time::sleep(Duration::from_millis(250)).await;

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
    if let Ok(server_root) = std::env::var("RARENA_SERVER_ROOT") {
        return PathBuf::from(server_root).join("content");
    }

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
    http_get_with_headers(base_url, path, &[]).await
}

async fn http_get_with_headers(
    base_url: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> (u16, String) {
    let authority = http_authority_from_ws(base_url);
    let mut stream = match tokio::net::TcpStream::connect(&authority).await {
        Ok(stream) => stream,
        Err(error) => panic!("http connection should succeed: {error}"),
    };
    let mut request = format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
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
#[path = "realtime_websocket/gameplay.rs"]
mod gameplay;
#[path = "realtime_websocket/handshake.rs"]
mod handshake;
#[path = "realtime_websocket/http_routes.rs"]
mod http_routes;
#[path = "realtime_websocket/validation.rs"]
mod validation;
