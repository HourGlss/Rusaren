#![allow(clippy::expect_used)]

use game_api::{ConnectionId, HeadlessClient, InMemoryTransport, ServerApp};
use game_domain::{LobbyId, PlayerName, ReadyState, SkillChoice, SkillTree, TeamSide};
use game_net::{ServerControlEvent, ValidatedInputFrame, BUTTON_CAST};
use game_sim::COMBAT_FRAME_MS;

fn connection_id(raw: u64) -> ConnectionId {
    ConnectionId::new(raw).expect("valid connection id")
}

fn player_name(raw: &str) -> PlayerName {
    PlayerName::new(raw).expect("valid player name")
}

fn skill(tree: SkillTree, tier: u8) -> SkillChoice {
    SkillChoice::new(tree, tier).expect("valid skill choice")
}

fn slot_one_cast_input(client_input_tick: u32) -> ValidatedInputFrame {
    ValidatedInputFrame::new(client_input_tick, 0, 0, 0, 0, BUTTON_CAST, 1)
        .expect("slot one cast input should be valid")
}

fn connect_player(
    server: &mut ServerApp,
    transport: &mut InMemoryTransport,
    raw_connection_id: u64,
    raw_name: &str,
) -> HeadlessClient {
    let mut client = HeadlessClient::new(connection_id(raw_connection_id), player_name(raw_name));
    client.connect(transport).expect("connect packet");
    server.pump_transport(transport);
    let events = client.drain_events(transport).expect("connect events");
    assert!(events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::Connected { .. })));
    client
}

fn connect_pair(
    server: &mut ServerApp,
    transport: &mut InMemoryTransport,
    offset: u64,
    left_name: &str,
    right_name: &str,
) -> (HeadlessClient, HeadlessClient) {
    let left = connect_player(server, transport, offset, left_name);
    let right = connect_player(server, transport, offset + 1, right_name);
    (left, right)
}

fn lobby_id_from(events: &[ServerControlEvent]) -> LobbyId {
    events
        .iter()
        .find_map(|event| match event {
            ServerControlEvent::GameLobbyCreated { lobby_id } => Some(*lobby_id),
            _ => None,
        })
        .expect("game lobby should exist")
}

fn cast_until_round_won(
    server: &mut ServerApp,
    transport: &mut InMemoryTransport,
    left: &mut HeadlessClient,
    right: &mut HeadlessClient,
    round: u8,
) -> (Vec<ServerControlEvent>, Vec<ServerControlEvent>) {
    let mut left_events = Vec::new();
    let mut right_events = Vec::new();

    for offset in 0_u32..18 {
        let sequence = u32::from(round) * 100 + offset + 1;
        left.send_input(transport, slot_one_cast_input(sequence), sequence)
            .expect("attack packet");
        server.pump_transport(transport);
        left_events.extend(left.drain_events(transport).expect("left input events"));
        right_events.extend(right.drain_events(transport).expect("right input events"));

        for _ in 0..12 {
            server.advance_millis(transport, COMBAT_FRAME_MS);
            left_events.extend(left.drain_events(transport).expect("left combat events"));
            right_events.extend(right.drain_events(transport).expect("right combat events"));
            if left_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::RoundWon { round: won_round, .. } if won_round.get() == round
            )) {
                return (left_events, right_events);
            }
        }
    }

    panic!("expected round {round} to end after repeated slot-one casts");
}

fn launch_match(
    server: &mut ServerApp,
    transport: &mut InMemoryTransport,
    left: &mut HeadlessClient,
    right: &mut HeadlessClient,
) {
    left.create_game_lobby(transport).expect("create lobby");
    server.pump_transport(transport);
    let left_events = left.drain_events(transport).expect("left events");
    let lobby_id = lobby_id_from(&left_events);

    right
        .join_game_lobby(transport, lobby_id)
        .expect("join lobby");
    server.pump_transport(transport);
    let _ = left.drain_events(transport).expect("left join events");
    let _ = right.drain_events(transport).expect("right join events");

    left.select_team(transport, TeamSide::TeamA)
        .expect("left team");
    right
        .select_team(transport, TeamSide::TeamB)
        .expect("right team");
    server.pump_transport(transport);
    let _ = left.drain_events(transport).expect("left team events");
    let _ = right.drain_events(transport).expect("right team events");

    left.set_ready(transport, ReadyState::Ready)
        .expect("left ready");
    right
        .set_ready(transport, ReadyState::Ready)
        .expect("right ready");
    server.pump_transport(transport);
    let _ = left.drain_events(transport).expect("left ready events");
    let _ = right.drain_events(transport).expect("right ready events");

    server.advance_seconds(transport, 5);
    let left_events = left.drain_events(transport).expect("left launch events");
    let right_events = right.drain_events(transport).expect("right launch events");
    assert!(left_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::MatchStarted { .. })));
    assert!(right_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::MatchStarted { .. })));
}

fn complete_match(
    server: &mut ServerApp,
    transport: &mut InMemoryTransport,
    left: &mut HeadlessClient,
    right: &mut HeadlessClient,
) {
    for round in 1..=5 {
        left.choose_skill(transport, skill(SkillTree::Mage, round))
            .expect("left skill");
        right
            .choose_skill(transport, skill(SkillTree::Rogue, round))
            .expect("right skill");
        server.pump_transport(transport);
        let _ = left.drain_events(transport).expect("left skill events");
        let _ = right.drain_events(transport).expect("right skill events");

        server.advance_seconds(transport, 5);
        let _ = left
            .drain_events(transport)
            .expect("left pre-combat events");
        let _ = right
            .drain_events(transport)
            .expect("right pre-combat events");

        let (left_events, right_events) =
            cast_until_round_won(server, transport, left, right, round);
        assert!(left_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::RoundWon { round: won_round, .. } if won_round.get() == round
        )));
        assert!(right_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::RoundWon { round: won_round, .. } if won_round.get() == round
        )));
    }
}

#[test]
fn repeated_match_sessions_complete_without_state_leaks() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut left, mut right) = connect_pair(&mut server, &mut transport, 1, "Alice", "Bob");

    for _ in 0..3 {
        launch_match(&mut server, &mut transport, &mut left, &mut right);
        complete_match(&mut server, &mut transport, &mut left, &mut right);

        left.quit_to_central_lobby(&mut transport)
            .expect("left quit");
        right
            .quit_to_central_lobby(&mut transport)
            .expect("right quit");
        server.pump_transport(&mut transport);
        let left_events = left
            .drain_events(&mut transport)
            .expect("left return events");
        let right_events = right
            .drain_events(&mut transport)
            .expect("right return events");
        assert!(left_events
            .iter()
            .any(|event| matches!(event, ServerControlEvent::ReturnedToCentralLobby { .. })));
        assert!(right_events
            .iter()
            .any(|event| matches!(event, ServerControlEvent::ReturnedToCentralLobby { .. })));
    }
}

#[test]
fn lobby_directory_handles_parallel_lobby_churn() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let mut clients = vec![
        connect_player(&mut server, &mut transport, 1, "Alice"),
        connect_player(&mut server, &mut transport, 2, "Bob"),
        connect_player(&mut server, &mut transport, 3, "Cara"),
        connect_player(&mut server, &mut transport, 4, "Dax"),
        connect_player(&mut server, &mut transport, 5, "Eve"),
        connect_player(&mut server, &mut transport, 6, "Finn"),
    ];

    clients[0]
        .create_game_lobby(&mut transport)
        .expect("alice create");
    clients[2]
        .create_game_lobby(&mut transport)
        .expect("cara create");
    clients[4]
        .create_game_lobby(&mut transport)
        .expect("eve create");
    server.pump_transport(&mut transport);

    let first_lobby = lobby_id_from(&clients[0].drain_events(&mut transport).expect("alice"));
    let second_lobby = lobby_id_from(&clients[2].drain_events(&mut transport).expect("cara"));
    let third_lobby = lobby_id_from(&clients[4].drain_events(&mut transport).expect("eve"));

    clients[1]
        .join_game_lobby(&mut transport, first_lobby)
        .expect("bob join");
    clients[3]
        .join_game_lobby(&mut transport, second_lobby)
        .expect("dax join");
    clients[5]
        .join_game_lobby(&mut transport, third_lobby)
        .expect("finn join");
    server.pump_transport(&mut transport);

    let central_events = clients[0]
        .drain_events(&mut transport)
        .expect("alice events");
    let _ = clients[1].drain_events(&mut transport).expect("bob events");
    let _ = clients[2]
        .drain_events(&mut transport)
        .expect("cara events");
    let _ = clients[3].drain_events(&mut transport).expect("dax events");
    let _ = clients[4].drain_events(&mut transport).expect("eve events");
    let _ = clients[5]
        .drain_events(&mut transport)
        .expect("finn events");

    assert!(central_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::GameLobbySnapshot { .. })));

    clients[1]
        .leave_game_lobby(&mut transport)
        .expect("bob leave");
    clients[3]
        .leave_game_lobby(&mut transport)
        .expect("dax leave");
    clients[5]
        .leave_game_lobby(&mut transport)
        .expect("finn leave");
    server.pump_transport(&mut transport);

    for client in &mut clients {
        let _ = client
            .drain_events(&mut transport)
            .expect("drain leave events");
    }
}
