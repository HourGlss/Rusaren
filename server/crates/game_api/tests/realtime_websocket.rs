#![allow(clippy::expect_used, clippy::too_many_lines)]

use futures_util::{SinkExt, StreamExt};
use game_api::spawn_dev_server;
use game_domain::{PlayerId, PlayerName, ReadyState, TeamSide};
use game_net::{ClientControlCommand, ServerControlEvent};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as ClientMessage;

type ClientStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn player_id(raw: u32) -> PlayerId {
    match PlayerId::new(raw) {
        Ok(player_id) => player_id,
        Err(error) => panic!("valid player id expected: {error}"),
    }
}

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

async fn start_server() -> (game_api::DevServerHandle, String) {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) => panic!("listener should bind: {error}"),
    };
    let server = match spawn_dev_server(listener).await {
        Ok(server) => server,
        Err(error) => panic!("server should spawn: {error}"),
    };
    let base_url = format!("ws://{}/ws", server.local_addr());
    (server, base_url)
}

async fn connect_socket(base_url: &str) -> ClientStream {
    match connect_async(base_url).await {
        Ok((stream, _)) => stream,
        Err(error) => panic!("websocket should connect: {error}"),
    }
}

async fn send_command(stream: &mut ClientStream, command: ClientControlCommand, seq: u32) {
    let packet = match command.encode_packet(seq, 0) {
        Ok(packet) => packet,
        Err(error) => panic!("command packet should encode: {error}"),
    };
    let _ = stream.send(ClientMessage::Binary(packet.into())).await;
}

async fn connect_player(stream: &mut ClientStream, raw_id: u32, raw_name: &str) {
    send_command(
        stream,
        ClientControlCommand::Connect {
            player_id: player_id(raw_id),
            player_name: player_name(raw_name),
        },
        1,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_accepts_binary_commands_and_broadcasts_events() {
    let (server, base_url) = start_server().await;
    let mut alice = connect_socket(&base_url).await;
    let mut bob = connect_socket(&base_url).await;

    connect_player(&mut alice, 1, "Alice").await;
    connect_player(&mut bob, 2, "Bob").await;

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
            if *current == lobby_id && *joined_player == player_id(1)
    )));

    send_command(&mut bob, ClientControlCommand::JoinGameLobby { lobby_id }, 2).await;
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
            if *current == lobby_id && *joined_player == player_id(2)
    )));
    assert!(bob_join_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::GameLobbyJoined { lobby_id: current, player_id: joined_player }
            if *current == lobby_id && *joined_player == player_id(2)
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
async fn websocket_adapter_rejects_text_messages() {
    let (server, base_url) = start_server().await;
    let mut socket = connect_socket(&base_url).await;

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
    let mut socket = connect_socket(&base_url).await;

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
    let mut socket = connect_socket(&base_url).await;

    connect_player(&mut socket, 1, "Alice").await;
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
            player_id: player_id(2),
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
