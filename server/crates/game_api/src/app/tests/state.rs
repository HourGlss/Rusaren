use super::*;
use crate::combat_log::{
    CombatLogCastCancelReason, CombatLogCastMode, CombatLogEvent, CombatLogMissReason,
    CombatLogStatusRemovedReason, CombatLogTargetKind, CombatLogTriggerReason,
};
use game_content::{CombatValueKind, DispelScope, StatusKind};
use game_sim::{
    SimCastCancelReason, SimCastMode, SimMissReason, SimRemovedStatus, SimStatusRemovedReason,
    SimTargetKind, SimTriggerReason, SimulationEvent,
};

#[test]
fn central_lobby_receives_directory_snapshots_as_lobbies_change() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let mut alice = connect_player(&mut server, &mut transport, 1, "Alice");
    let mut bob = connect_player(&mut server, &mut transport, 2, "Bob");
    let mut charlie = connect_player(&mut server, &mut transport, 3, "Charlie");

    alice
        .create_game_lobby(&mut transport)
        .expect("create lobby");
    server.pump_transport(&mut transport);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice create events");
    let lobby_id = lobby_id_from(&alice_events);
    let bob_events = bob
        .drain_events(&mut transport)
        .expect("bob directory events");
    let charlie_events = charlie
        .drain_events(&mut transport)
        .expect("charlie directory events");
    for events in [&bob_events, &charlie_events] {
        let directory = lobby_directory(events).expect("central players should see lobbies");
        assert_eq!(directory.len(), 1);
        assert_eq!(directory[0].player_count, 1);
    }

    bob.join_game_lobby(&mut transport, lobby_id)
        .expect("join lobby");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice join events");
    let _ = bob.drain_events(&mut transport).expect("bob join events");
    let charlie_events = charlie
        .drain_events(&mut transport)
        .expect("charlie updated directory");
    let directory = lobby_directory(&charlie_events).expect("directory snapshot");
    assert_eq!(directory.len(), 1);
    assert_eq!(directory[0].player_count, 2);

    bob.leave_game_lobby(&mut transport).expect("leave lobby");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice leave events");
    let bob_events = bob.drain_events(&mut transport).expect("bob leave events");
    let charlie_events = charlie
        .drain_events(&mut transport)
        .expect("charlie leave directory");
    assert!(bob_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::ReturnedToCentralLobby { .. })));
    let directory = lobby_directory(&charlie_events).expect("directory snapshot");
    assert_eq!(directory.len(), 1);
    assert_eq!(directory[0].player_count, 1);
}

#[test]
fn app_error_display_and_connected_player_sequences_are_precise() {
    assert_eq!(
        AppError::PlayerMissing(PlayerId::new(7).expect("player id")).to_string(),
        "player 7 is not connected"
    );

    let mut connected = ConnectedPlayer {
        player_name: player_name("Alice"),
        record: PlayerRecord::new(),
        location: PlayerLocation::CentralLobby,
        inbound_control: SequenceTracker::new(),
        inbound_input: SequenceTracker::new(),
        newest_client_input_tick: None,
        next_outbound_seq: 0,
    };
    assert_eq!(connected.next_outbound_seq(), 1);
    assert_eq!(connected.next_outbound_seq(), 2);
}

#[test]
fn population_counts_track_runtime_state() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    assert_eq!(server.connected_player_count(), 0);
    assert_eq!(server.bound_connection_count(), 0);
    assert_eq!(server.central_lobby_player_count(), 0);
    assert_eq!(server.active_lobby_count(), 0);
    assert_eq!(server.active_match_count(), 0);

    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    assert_eq!(server.connected_player_count(), 2);
    assert_eq!(server.bound_connection_count(), 2);
    assert_eq!(server.central_lobby_player_count(), 2);
    assert_eq!(server.active_lobby_count(), 0);
    assert_eq!(server.active_match_count(), 0);

    alice
        .create_game_lobby(&mut transport)
        .expect("create lobby");
    server.pump_transport(&mut transport);
    let lobby_id = lobby_id_from(&alice.drain_events(&mut transport).expect("create events"));
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob directory update");
    assert_eq!(server.connected_player_count(), 2);
    assert_eq!(server.bound_connection_count(), 2);
    assert_eq!(server.central_lobby_player_count(), 1);
    assert_eq!(server.active_lobby_count(), 1);
    assert_eq!(server.active_match_count(), 0);

    bob.join_game_lobby(&mut transport, lobby_id)
        .expect("join lobby");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice join events");
    let _ = bob.drain_events(&mut transport).expect("bob join events");
    assert_eq!(server.connected_player_count(), 2);
    assert_eq!(server.bound_connection_count(), 2);
    assert_eq!(server.central_lobby_player_count(), 0);
    assert_eq!(server.active_lobby_count(), 1);
    assert_eq!(server.active_match_count(), 0);

    let mut match_server = ServerApp::new();
    let mut match_transport = InMemoryTransport::new();
    let (mut match_alice, mut match_bob) = connect_pair(&mut match_server, &mut match_transport);
    let _ = launch_match(
        &mut match_server,
        &mut match_transport,
        &mut match_alice,
        &mut match_bob,
    );
    assert_eq!(match_server.connected_player_count(), 2);
    assert_eq!(match_server.bound_connection_count(), 2);
    assert_eq!(match_server.central_lobby_player_count(), 0);
    assert_eq!(match_server.active_lobby_count(), 0);
    assert_eq!(match_server.active_match_count(), 1);
}

#[test]
fn constructors_preserve_custom_content_and_persistence() {
    let (content, content_root) = custom_content();

    let mut ephemeral = ServerApp::new_with_content(content.clone());
    let mut transport = InMemoryTransport::new();
    let mut alice = HeadlessClient::new(connection_id(91), player_name("Alice"));
    alice.connect(&mut transport).expect("connect packet");
    ephemeral.pump_transport(&mut transport);
    let events = alice
        .drain_events(&mut transport)
        .expect("connect events should decode");
    assert!(events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Connected { skill_catalog, .. }
            if skill_catalog.iter().any(|entry| entry.tree.as_str() == "MutationSentinel")
    )));

    let path = temp_path("server-app-custom-persistent");
    remove_if_exists(&path);
    let mut persistent = ServerApp::new_persistent_with_content(content, &path)
        .expect("persistent app should build");
    let mut transport = InMemoryTransport::new();
    let mut persistent_alice = HeadlessClient::new(connection_id(92), player_name("Alice"));
    persistent_alice
        .connect(&mut transport)
        .expect("connect packet");
    persistent.pump_transport(&mut transport);
    let events = persistent_alice
        .drain_events(&mut transport)
        .expect("connect events should decode");
    let player_id = persistent_alice.player_id().expect("player id should bind");
    assert!(events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Connected { skill_catalog, .. }
            if skill_catalog.iter().any(|entry| entry.tree.as_str() == "MutationSentinel")
    )));

    persistent
        .players
        .get_mut(&player_id)
        .expect("connected player should exist")
        .record
        .record_win();
    assert!(persistent.persist_player_record(&mut transport, player_id));
    drop(persistent);

    let mut reloaded = ServerApp::new_persistent(&path).expect("persistent reload");
    let mut transport = InMemoryTransport::new();
    let mut reloaded_alice = HeadlessClient::new(connection_id(93), player_name("Alice"));
    reloaded_alice
        .connect(&mut transport)
        .expect("connect packet");
    reloaded.pump_transport(&mut transport);
    let events = reloaded_alice
        .drain_events(&mut transport)
        .expect("reloaded connect events");
    assert!(events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Connected { record, .. } if record.wins == 1
    )));

    remove_if_exists(&path);
    remove_dir_if_exists(&content_root);
}

#[test]
fn disconnect_connection_removes_bound_players() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let alice = connect_player(&mut server, &mut transport, 1, "Alice");
    let player_id = alice.player_id().expect("alice should connect");
    let connection_id = alice.connection_id();

    server
        .disconnect_connection(&mut transport, connection_id)
        .expect("disconnect should succeed");

    assert!(!server.players.contains_key(&player_id));
    assert!(!server.connections.contains_key(&connection_id));
    assert!(!server.player_connections.contains_key(&player_id));
}

#[test]
fn disconnecting_lobby_members_notifies_only_remaining_players() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    alice
        .create_game_lobby(&mut transport)
        .expect("create lobby");
    server.pump_transport(&mut transport);
    let lobby_id = lobby_id_from(&alice.drain_events(&mut transport).expect("create events"));

    bob.join_game_lobby(&mut transport, lobby_id)
        .expect("join lobby");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice join events");
    let _ = bob.drain_events(&mut transport).expect("bob join events");

    let bob_id = bob.player_id().expect("bob id");
    server
        .disconnect_player(&mut transport, bob_id)
        .expect("disconnect should succeed");
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice disconnect events");
    let bob_events = bob
        .drain_events(&mut transport)
        .expect("bob should not receive disconnect aftermath");

    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::GameLobbyLeft { player_id, .. } if *player_id == bob_id
    )));
    assert!(bob_events.is_empty());

    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    alice
        .create_game_lobby(&mut transport)
        .expect("create lobby");
    server.pump_transport(&mut transport);
    let lobby_id = lobby_id_from(&alice.drain_events(&mut transport).expect("create events"));
    bob.join_game_lobby(&mut transport, lobby_id)
        .expect("join lobby");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice join events");
    let _ = bob.drain_events(&mut transport).expect("bob join events");
    alice
        .select_team(&mut transport, TeamSide::TeamA)
        .expect("alice team");
    bob.select_team(&mut transport, TeamSide::TeamB)
        .expect("bob team");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice team events");
    let _ = bob.drain_events(&mut transport).expect("bob team events");
    alice
        .set_ready(&mut transport, ReadyState::Ready)
        .expect("alice ready");
    bob.set_ready(&mut transport, ReadyState::Ready)
        .expect("bob ready");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice ready events");
    let _ = bob.drain_events(&mut transport).expect("bob ready events");

    let bob_id = bob.player_id().expect("bob id");
    server
        .disconnect_player(&mut transport, bob_id)
        .expect("countdown disconnect should succeed");
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice countdown disconnect events");
    let bob_events = bob
        .drain_events(&mut transport)
        .expect("bob countdown disconnect events");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "Bob has disconnected. Game is over."
    )));
    assert!(bob_events.is_empty());
}

#[test]
fn lobby_directory_and_location_helpers_report_exact_state() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    alice
        .create_game_lobby(&mut transport)
        .expect("create lobby");
    server.pump_transport(&mut transport);
    let lobby_id = lobby_id_from(&alice.drain_events(&mut transport).expect("create events"));
    bob.join_game_lobby(&mut transport, lobby_id)
        .expect("join lobby");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice join events");
    let _ = bob.drain_events(&mut transport).expect("bob join events");

    alice
        .select_team(&mut transport, TeamSide::TeamA)
        .expect("alice team");
    bob.select_team(&mut transport, TeamSide::TeamB)
        .expect("bob team");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice team events");
    let _ = bob.drain_events(&mut transport).expect("bob team events");
    alice
        .set_ready(&mut transport, ReadyState::Ready)
        .expect("alice ready");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice ready events");

    let entries = server.build_lobby_directory_entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0],
        LobbyDirectoryEntry {
            lobby_id,
            player_count: 2,
            team_a_count: 1,
            team_b_count: 1,
            ready_count: 1,
            phase: LobbySnapshotPhase::Open,
        }
    );
    assert_eq!(server.lobby_members(lobby_id).len(), 2);

    let alice_id = alice.player_id().expect("alice id");
    assert!(!server.ensure_location(&mut transport, alice_id, PlayerLocation::CentralLobby,));
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("ensure-location events");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "player is in the wrong state for that command"
    )));

    server.cleanup_empty_lobby(lobby_id);
    assert!(server.game_lobbies.contains_key(&lobby_id));
    bob.leave_game_lobby(&mut transport).expect("leave lobby");
    server.pump_transport(&mut transport);
    let _ = bob.drain_events(&mut transport).expect("bob leave events");
    alice.leave_game_lobby(&mut transport).expect("leave lobby");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice leave events");
    server.cleanup_empty_lobby(lobby_id);
    assert!(!server.game_lobbies.contains_key(&lobby_id));
}

#[test]
fn malformed_packets_reject_unbound_connections_with_direct_errors() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let mut intruder = HeadlessClient::new(connection_id(77), player_name("Intruder"));

    transport.send_from_client(intruder.connection_id(), vec![0xAA, 0xBB, 0xCC]);
    server.pump_transport(&mut transport);

    let events = intruder
        .drain_events(&mut transport)
        .expect("malformed packet response should decode");
    assert!(events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message.contains("minimum header length")
    )));
}

#[test]
fn malformed_packets_reject_bound_connections_with_error_events() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let mut alice = connect_player(&mut server, &mut transport, 1, "Alice");

    transport.send_from_client(alice.connection_id(), vec![0xAA, 0xBB, 0xCC]);
    server.pump_transport(&mut transport);

    let events = alice
        .drain_events(&mut transport)
        .expect("malformed packet response should decode");
    assert!(events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message.contains("minimum header length")
    )));
}

#[test]
fn lobby_directory_entries_count_only_assigned_members_and_specific_lobbies() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let mut alice = connect_player(&mut server, &mut transport, 1, "Alice");
    let mut bob = connect_player(&mut server, &mut transport, 2, "Bob");
    let mut charlie = connect_player(&mut server, &mut transport, 3, "Charlie");
    let mut dylan = connect_player(&mut server, &mut transport, 4, "Dylan");

    alice
        .create_game_lobby(&mut transport)
        .expect("create first lobby");
    server.pump_transport(&mut transport);
    let lobby_one = lobby_id_from(&alice.drain_events(&mut transport).expect("create events"));
    bob.join_game_lobby(&mut transport, lobby_one)
        .expect("bob join first lobby");
    charlie
        .join_game_lobby(&mut transport, lobby_one)
        .expect("charlie join first lobby");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice lobby-one join events");
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob lobby-one join events");
    let _ = charlie
        .drain_events(&mut transport)
        .expect("charlie lobby-one join events");

    dylan
        .create_game_lobby(&mut transport)
        .expect("create second lobby");
    server.pump_transport(&mut transport);
    let lobby_two = lobby_id_from(&dylan.drain_events(&mut transport).expect("create events"));

    alice
        .select_team(&mut transport, TeamSide::TeamA)
        .expect("alice team");
    bob.select_team(&mut transport, TeamSide::TeamB)
        .expect("bob team");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice team events");
    let _ = bob.drain_events(&mut transport).expect("bob team events");
    let _ = charlie
        .drain_events(&mut transport)
        .expect("charlie team events");
    alice
        .set_ready(&mut transport, ReadyState::Ready)
        .expect("alice ready");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice ready events");
    let _ = bob.drain_events(&mut transport).expect("bob ready events");
    let _ = charlie
        .drain_events(&mut transport)
        .expect("charlie ready events");

    let entries = server.build_lobby_directory_entries();
    assert_eq!(entries.len(), 2);
    let first_entry = entries
        .iter()
        .find(|entry| entry.lobby_id == lobby_one)
        .cloned()
        .expect("first lobby should appear");
    assert_eq!(
        first_entry,
        LobbyDirectoryEntry {
            lobby_id: lobby_one,
            player_count: 3,
            team_a_count: 1,
            team_b_count: 1,
            ready_count: 1,
            phase: LobbySnapshotPhase::Open,
        }
    );
    let second_entry = entries
        .iter()
        .find(|entry| entry.lobby_id == lobby_two)
        .cloned()
        .expect("second lobby should appear");
    assert_eq!(
        second_entry,
        LobbyDirectoryEntry {
            lobby_id: lobby_two,
            player_count: 1,
            team_a_count: 0,
            team_b_count: 0,
            ready_count: 0,
            phase: LobbySnapshotPhase::Open,
        }
    );

    let alice_id = alice.player_id().expect("alice id");
    let bob_id = bob.player_id().expect("bob id");
    let charlie_id = charlie.player_id().expect("charlie id");
    let dylan_id = dylan.player_id().expect("dylan id");
    let mut lobby_one_members = server.lobby_members(lobby_one);
    lobby_one_members.sort_unstable();
    let mut expected_lobby_one_members = vec![alice_id, bob_id, charlie_id];
    expected_lobby_one_members.sort_unstable();
    assert_eq!(lobby_one_members, expected_lobby_one_members);
    assert_eq!(server.lobby_members(lobby_two), vec![dylan_id]);
}

#[test]
fn apply_match_outcome_and_cleanup_helpers_track_match_lifecycle() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    let match_id = enter_combat(
        &mut server,
        &mut transport,
        &mut alice,
        &mut bob,
        skill(SkillTree::Mage, 1),
        skill(SkillTree::Rogue, 1),
    );
    let alice_id = alice.player_id().expect("alice id");
    let bob_id = bob.player_id().expect("bob id");

    server.apply_match_outcome(&mut transport, match_id, MatchOutcome::TeamAWin);
    assert_eq!(server.players[&alice_id].record.wins, 1);
    assert_eq!(server.players[&bob_id].record.losses, 1);
    assert!(matches!(
        server.players[&alice_id].location,
        PlayerLocation::Results(current) if current == match_id
    ));

    server.cleanup_finished_match(match_id);
    assert!(server.matches.contains_key(&match_id));
    server
        .players
        .get_mut(&alice_id)
        .expect("alice should exist")
        .location = PlayerLocation::CentralLobby;
    server
        .players
        .get_mut(&bob_id)
        .expect("bob should exist")
        .location = PlayerLocation::CentralLobby;
    server.cleanup_finished_match(match_id);
    assert!(!server.matches.contains_key(&match_id));
}

#[test]
fn team_b_match_outcomes_record_wins_for_team_b_players() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    let match_id = enter_combat(
        &mut server,
        &mut transport,
        &mut alice,
        &mut bob,
        skill(SkillTree::Mage, 1),
        skill(SkillTree::Rogue, 1),
    );
    let alice_id = alice.player_id().expect("alice id");
    let bob_id = bob.player_id().expect("bob id");

    server.apply_match_outcome(&mut transport, match_id, MatchOutcome::TeamBWin);
    assert_eq!(server.players[&alice_id].record.losses, 1);
    assert_eq!(server.players[&bob_id].record.wins, 1);
    assert!(matches!(
        server.players[&bob_id].location,
        PlayerLocation::Results(current) if current == match_id
    ));
}

#[test]
fn send_direct_error_delivers_a_decodable_error_packet() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let connection_id = connection_id(77);

    server.send_direct_error(&mut transport, connection_id, "bad packet");

    let packets = transport.drain_client_packets(connection_id);
    assert_eq!(packets.len(), 1);
    let (_, event) = ServerControlEvent::decode_packet(&packets[0]).expect("packet should decode");
    assert_eq!(
        event,
        ServerControlEvent::Error {
            message: String::from("bad packet"),
        }
    );
}

#[test]
fn persistent_player_records_survive_reconnect() {
    let path = temp_path("server-app-records");
    remove_if_exists(&path);

    let mut server = ServerApp::new_persistent(&path).expect("persistent server should start");
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

    for tier in 1..=5 {
        alice
            .choose_skill(&mut transport, skill(SkillTree::Mage, tier))
            .expect("alice skill");
        bob.choose_skill(&mut transport, skill(SkillTree::Rogue, tier))
            .expect("bob skill");
        server.pump_transport(&mut transport);
        let _ = alice
            .drain_events(&mut transport)
            .expect("alice skill events");
        let _ = bob.drain_events(&mut transport).expect("bob skill events");
        server.advance_seconds(&mut transport, 5);
        let _ = alice
            .drain_events(&mut transport)
            .expect("alice pre-combat events");
        let _ = bob
            .drain_events(&mut transport)
            .expect("bob pre-combat events");
        let _ = cast_until_round_won(&mut server, &mut transport, &mut alice, &mut bob, tier);
    }

    alice
        .quit_to_central_lobby(&mut transport)
        .expect("alice quit");
    bob.quit_to_central_lobby(&mut transport).expect("bob quit");
    server.pump_transport(&mut transport);
    let _ = alice.drain_events(&mut transport).expect("alice return");
    let _ = bob.drain_events(&mut transport).expect("bob return");
    server
        .disconnect_player(
            &mut transport,
            alice
                .player_id()
                .expect("alice should be connected before disconnect"),
        )
        .expect("alice disconnect");
    server
        .disconnect_player(
            &mut transport,
            bob.player_id()
                .expect("bob should be connected before disconnect"),
        )
        .expect("bob disconnect");

    let mut reloaded = ServerApp::new_persistent(&path).expect("persistent server should reload");
    let mut transport = InMemoryTransport::new();
    let mut alice = HeadlessClient::new(connection_id(9), player_name("Alice"));
    alice.connect(&mut transport).expect("connect packet");
    reloaded.pump_transport(&mut transport);

    let events = alice
        .drain_events(&mut transport)
        .expect("alice reconnect events");
    assert!(events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Connected { record, .. }
            if record.wins == 1 && record.losses == 0 && record.no_contests == 0
    )));

    remove_if_exists(&path);
}

#[test]
#[allow(clippy::too_many_lines)]
fn training_sessions_reset_metrics_without_restarting_the_map() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let mut alice = connect_player(&mut server, &mut transport, 41, "Alice");

    alice
        .start_training(&mut transport)
        .expect("start training");
    server.pump_transport(&mut transport);
    let start_events = alice
        .drain_events(&mut transport)
        .expect("training start events should decode");
    let training_id = start_events
        .iter()
        .find_map(|event| match event {
            ServerControlEvent::TrainingStarted { training_id } => Some(*training_id),
            _ => None,
        })
        .expect("training start event should be emitted");
    let start_snapshot = arena_state_snapshot(&start_events)
        .expect("training should immediately broadcast a snapshot");
    assert_eq!(start_snapshot.mode, game_net::ArenaSessionMode::Training);
    assert_eq!(start_snapshot.phase, game_net::ArenaMatchPhase::Combat);
    assert_eq!(
        start_snapshot.training_metrics,
        Some(game_net::TrainingMetricsSnapshot {
            damage_done: 0,
            healing_done: 0,
            elapsed_ms: 0,
        })
    );

    let player_id = alice.player_id().expect("training player id");
    let ranger_tree = SkillTree::new("Ranger").expect("ranger tree");
    alice
        .choose_skill(&mut transport, skill(ranger_tree.clone(), 4))
        .expect("training ward skill");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("skill choice events should decode");

    let (spawn_x, spawn_y, preserved_explored) = {
        let runtime = server
            .training_sessions
            .get_mut(&training_id)
            .expect("training runtime should exist");
        let start_state = runtime
            .world
            .player_state(player_id)
            .expect("training player state");
        let mut explored = runtime
            .explored_tiles
            .get(&player_id)
            .expect("training explored mask should exist")
            .clone();
        if let Some(first_byte) = explored.first_mut() {
            *first_byte |= 0x03;
        }
        runtime.explored_tiles.insert(player_id, explored.clone());
        runtime.metrics = TrainingMetrics {
            damage_done: 900,
            healing_done: 250,
            elapsed_ms: 7000,
        };
        runtime.combat_frame_index = 77;
        runtime
            .world
            .update_aim(player_id, 1, 0)
            .expect("aim should update");
        runtime
            .world
            .submit_input(
                player_id,
                game_sim::MovementIntent::new(1, 0).expect("movement intent"),
            )
            .expect("movement input should apply");
        let _ = runtime.world.tick(game_sim::COMBAT_FRAME_MS);
        runtime
            .world
            .submit_input(player_id, game_sim::MovementIntent::zero())
            .expect("stop input should apply");
        runtime
            .world
            .queue_cast(player_id, 4)
            .expect("ward cast should queue");
        let _ = runtime.world.tick(game_sim::COMBAT_FRAME_MS);
        assert!(
            runtime
                .world
                .deployables()
                .into_iter()
                .any(|deployable| deployable.kind == game_sim::ArenaDeployableKind::Ward),
            "training should allow spawning temporary authored deployables before reset"
        );
        (start_state.x, start_state.y, explored)
    };

    alice
        .reset_training_session(&mut transport)
        .expect("reset training");
    server.pump_transport(&mut transport);
    let reset_events = alice
        .drain_events(&mut transport)
        .expect("training reset events should decode");
    let reset_snapshot = arena_state_snapshot(&reset_events)
        .expect("reset should rebroadcast the training snapshot");
    assert_eq!(reset_snapshot.mode, game_net::ArenaSessionMode::Training);
    assert_eq!(
        reset_snapshot.training_metrics,
        Some(game_net::TrainingMetricsSnapshot {
            damage_done: 0,
            healing_done: 0,
            elapsed_ms: 0,
        })
    );

    let runtime = server
        .training_sessions
        .get(&training_id)
        .expect("training runtime should survive reset");
    assert_eq!(runtime.metrics, TrainingMetrics::default());
    assert_eq!(
        runtime.combat_frame_index, 77,
        "resetting training should clear metrics and state without rewinding the session frame index"
    );
    assert_eq!(
        runtime.explored_tiles.get(&player_id),
        Some(&preserved_explored),
        "resetting training should preserve the explored training footprint"
    );
    let reset_state = runtime
        .world
        .player_state(player_id)
        .expect("training player should still exist");
    assert_eq!(
        (reset_state.x, reset_state.y),
        (spawn_x, spawn_y),
        "training reset should return the player to the authored spawn"
    );
    let deployables = runtime.world.deployables();
    assert_eq!(
        deployables.len(),
        2,
        "training reset should clear temporary deployables without rebuilding away the dummies"
    );
    let reset_full_dummy = deployables
        .iter()
        .find(|deployable| deployable.kind == game_sim::ArenaDeployableKind::TrainingDummyResetFull)
        .expect("reset-full dummy should remain");
    assert_eq!(reset_full_dummy.hit_points, reset_full_dummy.max_hit_points);
    let execute_dummy = deployables
        .iter()
        .find(|deployable| deployable.kind == game_sim::ArenaDeployableKind::TrainingDummyExecute)
        .expect("execute dummy should remain");
    assert!(
        execute_dummy.hit_points > 0 && execute_dummy.hit_points < execute_dummy.max_hit_points,
        "execute dummy should return to its low-health execute state after a reset"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn durable_combat_logs_capture_advanced_mechanics_event_detail() {
    let path = temp_path("server-app-advanced-combat-log");
    let combat_log_path = companion_combat_log_path(&path);
    remove_if_exists(&path);
    remove_if_exists(&combat_log_path);

    let mut server = ServerApp::new_persistent(&path).expect("persistent server should start");
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    let match_id = enter_combat(
        &mut server,
        &mut transport,
        &mut alice,
        &mut bob,
        skill(SkillTree::Warrior, 1),
        skill(SkillTree::Cleric, 1),
    );
    let alice_id = alice.player_id().expect("alice id");
    let bob_id = bob.player_id().expect("bob id");
    let base_len = server
        .combat_log_entries(match_id)
        .expect("baseline log query should succeed")
        .len();

    server
        .matches
        .get_mut(&match_id)
        .expect("match runtime should exist")
        .combat_frame_index = 7;

    let synthetic_events = vec![
        SimulationEvent::CastStarted {
            player_id: alice_id,
            slot: 5,
            behavior: "channel",
            mode: SimCastMode::Channel,
            total_ms: 2400,
        },
        SimulationEvent::ChannelTick {
            player_id: alice_id,
            slot: 5,
            tick_index: 2,
            behavior: "channel",
        },
        SimulationEvent::CastCanceled {
            player_id: alice_id,
            slot: 5,
            reason: SimCastCancelReason::Manual,
        },
        SimulationEvent::DispelCast {
            source: bob_id,
            slot: 4,
            scope: DispelScope::Negative,
            max_statuses: 1,
        },
        SimulationEvent::StatusApplied {
            source: alice_id,
            target: bob_id,
            slot: 3,
            kind: StatusKind::Hot,
            stacks: 3,
            stack_delta: 2,
            remaining_ms: 1800,
        },
        SimulationEvent::DispelResult {
            source: bob_id,
            slot: 4,
            target: alice_id,
            removed_statuses: vec![SimRemovedStatus {
                source: alice_id,
                slot: 3,
                kind: StatusKind::Hot,
                stacks: 3,
                remaining_ms: 600,
            }],
            triggered_payload_count: 1,
        },
        SimulationEvent::StatusRemoved {
            source: alice_id,
            target: bob_id,
            slot: 3,
            kind: StatusKind::Hot,
            stacks: 3,
            remaining_ms: 600,
            reason: SimStatusRemovedReason::Dispelled,
        },
        SimulationEvent::TriggerResolved {
            source: alice_id,
            slot: 3,
            status_kind: StatusKind::Hot,
            trigger: SimTriggerReason::Dispel,
            target_kind: SimTargetKind::Player,
            target_id: bob_id.get(),
            payload_kind: CombatValueKind::Heal,
            amount: 12,
        },
        SimulationEvent::ImpactHit {
            source: alice_id,
            slot: 5,
            target_kind: SimTargetKind::Player,
            target_id: bob_id.get(),
        },
        SimulationEvent::ImpactMiss {
            source: bob_id,
            slot: 4,
            reason: SimMissReason::Blocked,
        },
        SimulationEvent::Defeat {
            attacker: Some(alice_id),
            target: bob_id,
        },
    ];

    server
        .append_simulation_logs(match_id, &synthetic_events)
        .expect("synthetic simulation logs should persist");

    let entries = server
        .combat_log_entries(match_id)
        .expect("combat logs should be queryable");
    let suffix = &entries[base_len..];
    assert_eq!(suffix.len(), synthetic_events.len());
    assert!(
        suffix
            .iter()
            .all(|entry| entry.frame_index == 7 && entry.round == 1),
        "all appended rows should keep the combat frame and round metadata from the current runtime"
    );

    let expected_events = vec![
        CombatLogEvent::CastStarted {
            player_id: alice_id.get(),
            slot: 5,
            behavior: String::from("channel"),
            mode: CombatLogCastMode::Channel,
            total_ms: 2400,
        },
        CombatLogEvent::ChannelTick {
            player_id: alice_id.get(),
            slot: 5,
            tick_index: 2,
            behavior: String::from("channel"),
        },
        CombatLogEvent::CastCanceled {
            player_id: alice_id.get(),
            slot: 5,
            reason: CombatLogCastCancelReason::Manual,
        },
        CombatLogEvent::DispelCast {
            source_player_id: bob_id.get(),
            slot: 4,
            scope: String::from("negative"),
            max_statuses: 1,
        },
        CombatLogEvent::StatusApplied {
            source_player_id: alice_id.get(),
            target_player_id: bob_id.get(),
            slot: 3,
            status_kind: String::from("hot"),
            stacks: 3,
            stack_delta: 2,
            remaining_ms: 1800,
        },
        CombatLogEvent::DispelResult {
            source_player_id: bob_id.get(),
            slot: 4,
            target_player_id: alice_id.get(),
            removed_statuses: vec![crate::combat_log::CombatLogRemovedStatus {
                source_player_id: alice_id.get(),
                slot: 3,
                status_kind: String::from("hot"),
                stacks: 3,
                remaining_ms: 600,
            }],
            triggered_payload_count: 1,
        },
        CombatLogEvent::StatusRemoved {
            source_player_id: alice_id.get(),
            target_player_id: bob_id.get(),
            slot: 3,
            status_kind: String::from("hot"),
            stacks: 3,
            remaining_ms: 600,
            reason: CombatLogStatusRemovedReason::Dispelled,
        },
        CombatLogEvent::TriggerResolved {
            source_player_id: alice_id.get(),
            target_kind: CombatLogTargetKind::Player,
            target_id: bob_id.get(),
            slot: 3,
            status_kind: String::from("hot"),
            trigger: CombatLogTriggerReason::Dispel,
            payload_kind: String::from("heal"),
            amount: 12,
        },
        CombatLogEvent::ImpactHit {
            source_player_id: alice_id.get(),
            slot: 5,
            target_kind: CombatLogTargetKind::Player,
            target_id: bob_id.get(),
        },
        CombatLogEvent::ImpactMiss {
            source_player_id: bob_id.get(),
            slot: 4,
            reason: CombatLogMissReason::Blocked,
        },
        CombatLogEvent::Defeat {
            source_player_id: Some(alice_id.get()),
            target_player_id: bob_id.get(),
        },
    ];

    assert_eq!(
        suffix
            .iter()
            .map(|entry| &entry.event)
            .collect::<Vec<_>>(),
        expected_events.iter().collect::<Vec<_>>(),
        "the durable combat log should preserve the advanced event payloads exactly for later UI and admin consumers"
    );

    let reloaded = ServerApp::new_persistent(&path).expect("persistent server should reload");
    let reloaded_entries = reloaded
        .combat_log_entries(match_id)
        .expect("reloaded combat logs should be queryable");
    assert_eq!(
        reloaded_entries[base_len..]
            .iter()
            .map(|entry| &entry.event)
            .collect::<Vec<_>>(),
        expected_events.iter().collect::<Vec<_>>(),
        "advanced combat event detail should survive process reload"
    );

    drop(reloaded);
    drop(server);
    remove_if_exists(&path);
    remove_if_exists(&combat_log_path);
}
