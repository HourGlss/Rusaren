use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::transport::{ConnectionId, HeadlessClient, InMemoryTransport};
use game_content::GameContent;
use game_domain::{PlayerName, SkillTree};
use game_net::{
    ArenaStateSnapshot, LobbyDirectoryEntry, LobbySnapshotPlayer, ServerControlEvent,
    ValidatedInputFrame, BUTTON_CAST,
};
use game_sim::PLAYER_MOVE_SPEED_UNITS_PER_SECOND;

fn connection_id(raw: u64) -> ConnectionId {
    ConnectionId::new(raw).expect("valid connection id")
}

fn player_name(raw: &str) -> PlayerName {
    PlayerName::new(raw).expect("valid player name")
}

fn skill(tree: SkillTree, tier: u8) -> SkillChoice {
    SkillChoice::new(tree, tier).expect("valid skill choice")
}

fn temp_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should move forward")
        .as_nanos();
    std::env::temp_dir()
        .join("rusaren-tests")
        .join(format!("{label}-{}-{unique}.tsv", std::process::id()))
}

fn remove_if_exists(path: &PathBuf) {
    if path.exists() {
        fs::remove_file(path).expect("temp file should be removable");
    }
    if let Some(parent) = path.parent() {
        if parent.exists() {
            let _ = fs::remove_dir(parent);
        }
    }
}

fn temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should move forward")
        .as_nanos();
    std::env::temp_dir()
        .join("rusaren-tests")
        .join(format!("{label}-{}-{unique}", std::process::id()))
}

fn remove_dir_if_exists(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).expect("temp directory should be removable");
    }
}

fn copy_dir_all(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("destination directory should be creatable");
    for entry in fs::read_dir(source).expect("source directory should be readable") {
        let entry = entry.expect("directory entry should load");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().expect("file type should load");
        if file_type.is_dir() {
            copy_dir_all(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path).expect("file copy should succeed");
        }
    }
}

fn workspace_content_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content")
        .canonicalize()
        .expect("workspace content root should exist")
}

fn custom_class_yaml() -> &'static str {
    r"tree: MutationSentinel
melee:
  id: sentinel_probe
  name: Sentinel Probe
  description: A short-range sentinel jab used only by tests.
  cooldown_ms: 500
  range: 82
  radius: 36
  effect: melee_swing
  payload:
    kind: damage
    amount: 9
skills:
  - tier: 1
    id: sentinel_ping
    name: Sentinel Ping
    description: A narrow beam used to prove custom content is live.
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 900
      mana_cost: 6
      range: 180
      radius: 28
      payload:
        kind: damage
        amount: 12
  - tier: 2
    id: sentinel_burst
    name: Sentinel Burst
    description: A short area pulse.
    behavior:
      kind: nova
      effect: nova
      cooldown_ms: 1300
      mana_cost: 10
      radius: 110
      payload:
        kind: damage
        amount: 10
  - tier: 3
    id: sentinel_orb
    name: Sentinel Orb
    description: A small projectile.
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 1400
      mana_cost: 14
      speed: 360
      range: 260
      radius: 24
      payload:
        kind: damage
        amount: 14
  - tier: 4
    id: sentinel_dash
    name: Sentinel Dash
    description: A quick relocation test skill.
    behavior:
      kind: dash
      effect: dash_trail
      cooldown_ms: 1800
      mana_cost: 18
      distance: 180
  - tier: 5
    id: sentinel_nova
    name: Sentinel Nova
    description: A stronger area pulse.
    behavior:
      kind: nova
      effect: nova
      cooldown_ms: 2600
      mana_cost: 28
      radius: 150
      payload:
        kind: damage
        amount: 18
"
}

fn custom_content() -> (GameContent, PathBuf) {
    let root = temp_dir("server-app-custom-content");
    remove_dir_if_exists(&root);
    copy_dir_all(&workspace_content_root(), &root);
    fs::write(
        root.join("skills").join("mutation_sentinel.yaml"),
        custom_class_yaml(),
    )
    .expect("custom class file should write");
    let content =
        GameContent::load_from_root(&root).expect("custom content root should load cleanly");
    (content, root)
}

fn assert_connected(events: &[ServerControlEvent], player_id: PlayerId, player_name: &str) {
    assert!(events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Connected {
            player_id: connected_id,
            player_name: connected_name,
            skill_catalog,
            ..
        } if *connected_id == player_id
            && connected_name.as_str() == player_name
            && !skill_catalog.is_empty()
            && skill_catalog.iter().all(|entry| !entry.skill_name.is_empty())
    )));
}

fn assert_directory_lobby_count(events: &[ServerControlEvent], expected_count: usize) {
    assert!(events.iter().any(|event| matches!(
        event,
        ServerControlEvent::LobbyDirectorySnapshot { lobbies }
            if lobbies.len() == expected_count
    )));
}

fn lobby_directory(entries: &[ServerControlEvent]) -> Option<&[LobbyDirectoryEntry]> {
    entries.iter().rev().find_map(|event| match event {
        ServerControlEvent::LobbyDirectorySnapshot { lobbies } => Some(lobbies.as_slice()),
        _ => None,
    })
}

fn slot_one_cast_input(client_input_tick: u32) -> ValidatedInputFrame {
    ValidatedInputFrame::new(client_input_tick, 0, 0, 0, 0, BUTTON_CAST, 1)
        .expect("slot one cast input should be valid")
}

fn movement_input_frame(client_input_tick: u32, move_x: i16, move_y: i16) -> ValidatedInputFrame {
    ValidatedInputFrame::new(client_input_tick, move_x, move_y, 0, 0, 0, 0)
        .expect("movement input should be valid")
}

fn slot_cast_input(client_input_tick: u32, slot: u16) -> ValidatedInputFrame {
    ValidatedInputFrame::new(client_input_tick, 0, 0, 0, 0, BUTTON_CAST, slot)
        .expect("cast input should be valid")
}

fn player_x(server: &ServerApp, match_id: MatchId, player_id: PlayerId) -> i16 {
    server.matches[&match_id]
        .world
        .player_state(player_id)
        .expect("player state should exist")
        .x
}

fn cast_until_round_won(
    server: &mut ServerApp,
    transport: &mut InMemoryTransport,
    alice: &mut HeadlessClient,
    bob: &mut HeadlessClient,
    round: u8,
) -> (Vec<ServerControlEvent>, Vec<ServerControlEvent>) {
    let mut alice_events = Vec::new();
    let mut bob_events = Vec::new();

    for offset in 0_u32..18 {
        let sequence = u32::from(round) * 100 + offset + 1;
        alice
            .send_input(transport, slot_one_cast_input(sequence), sequence)
            .expect("attack packet");
        server.pump_transport(transport);
        alice_events.extend(alice.drain_events(transport).expect("alice input events"));
        bob_events.extend(bob.drain_events(transport).expect("bob input events"));

        for _ in 0..12 {
            server.advance_millis(transport, COMBAT_FRAME_MS);
            alice_events.extend(alice.drain_events(transport).expect("alice combat events"));
            bob_events.extend(bob.drain_events(transport).expect("bob combat events"));
            if alice_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::RoundWon { round: won_round, .. } if won_round.get() == round
            )) {
                return (alice_events, bob_events);
            }
        }
    }

    panic!("expected round {round} to end after repeated slot-one casts");
}

fn lobby_snapshot_players(entries: &[ServerControlEvent]) -> Option<&[LobbySnapshotPlayer]> {
    entries.iter().rev().find_map(|event| match event {
        ServerControlEvent::GameLobbySnapshot { players, .. } => Some(players.as_slice()),
        _ => None,
    })
}

fn arena_state_snapshot(entries: &[ServerControlEvent]) -> Option<&ArenaStateSnapshot> {
    entries.iter().rev().find_map(|event| match event {
        ServerControlEvent::ArenaStateSnapshot { snapshot } => Some(snapshot),
        _ => None,
    })
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
    assert_connected(
        &events,
        client
            .player_id()
            .expect("headless client should receive Connected before assertions"),
        raw_name,
    );
    assert_directory_lobby_count(&events, 0);
    client
}

fn connect_pair(
    server: &mut ServerApp,
    transport: &mut InMemoryTransport,
) -> (HeadlessClient, HeadlessClient) {
    let alice = connect_player(server, transport, 1, "Alice");
    let bob = connect_player(server, transport, 2, "Bob");
    assert_ne!(
        alice
            .player_id()
            .expect("alice should be connected before uniqueness checks"),
        bob.player_id()
            .expect("bob should be connected before uniqueness checks"),
        "server-assigned player ids should be unique across live clients"
    );
    (alice, bob)
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

#[allow(clippy::too_many_lines)]
fn launch_match(
    server: &mut ServerApp,
    transport: &mut InMemoryTransport,
    alice: &mut HeadlessClient,
    bob: &mut HeadlessClient,
) -> MatchId {
    alice.create_game_lobby(transport).expect("create lobby");
    server.pump_transport(transport);
    let alice_events = alice.drain_events(transport).expect("alice events");
    let lobby_id = lobby_id_from(&alice_events);
    assert_eq!(
        lobby_snapshot_players(&alice_events)
            .expect("creator should receive a full lobby snapshot")
            .len(),
        1
    );

    bob.join_game_lobby(transport, lobby_id)
        .expect("join lobby");
    server.pump_transport(transport);
    let alice_join_events = alice.drain_events(transport).expect("alice join events");
    let bob_join_events = bob.drain_events(transport).expect("bob join events");
    assert_eq!(
        lobby_snapshot_players(&alice_join_events)
            .expect("existing member should receive updated snapshot")
            .len(),
        2
    );
    assert_eq!(
        lobby_snapshot_players(&bob_join_events)
            .expect("late joiner should receive a full lobby snapshot")
            .len(),
        2
    );

    alice
        .select_team(transport, TeamSide::TeamA)
        .expect("alice team");
    bob.select_team(transport, TeamSide::TeamB)
        .expect("bob team");
    server.pump_transport(transport);
    let _ = alice.drain_events(transport).expect("alice select events");
    let _ = bob.drain_events(transport).expect("bob select events");

    alice
        .set_ready(transport, ReadyState::Ready)
        .expect("alice ready");
    bob.set_ready(transport, ReadyState::Ready)
        .expect("bob ready");
    server.pump_transport(transport);
    let alice_events = alice.drain_events(transport).expect("alice ready events");
    let bob_events = bob.drain_events(transport).expect("bob ready events");
    assert!(alice_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::LaunchCountdownStarted { .. })));
    assert!(bob_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::LaunchCountdownStarted { .. })));

    server.advance_seconds(transport, 5);
    let alice_events = alice
        .drain_events(transport)
        .expect("alice countdown events");
    let bob_events = bob.drain_events(transport).expect("bob countdown events");

    let match_id = alice_events
        .iter()
        .find_map(|event| match event {
            ServerControlEvent::MatchStarted { match_id, .. } => Some(*match_id),
            _ => None,
        })
        .expect("match should start");
    let alice_player_id = alice.player_id().expect("alice id");
    let bob_player_id = bob.player_id().expect("bob id");
    let alice_snapshot =
        arena_state_snapshot(&alice_events).expect("alice should receive an arena snapshot");
    let bob_snapshot =
        arena_state_snapshot(&bob_events).expect("bob should receive an arena snapshot");
    assert!(
        alice_snapshot
            .players
            .iter()
            .any(|player| player.player_id == alice_player_id),
        "alice should see her own actor in the first snapshot"
    );
    assert!(
        !alice_snapshot
            .players
            .iter()
            .any(|player| player.player_id == bob_player_id),
        "alice should not receive hidden opposing actors in the first snapshot"
    );
    assert!(
        bob_snapshot
            .players
            .iter()
            .any(|player| player.player_id == bob_player_id),
        "bob should see his own actor in the first snapshot"
    );
    assert!(
        !bob_snapshot
            .players
            .iter()
            .any(|player| player.player_id == alice_player_id),
        "bob should not receive hidden opposing actors in the first snapshot"
    );
    assert_eq!(alice_snapshot.tile_units, server.content.map().tile_units);
    assert_eq!(bob_snapshot.tile_units, server.content.map().tile_units);
    assert!(
        !alice_snapshot.visible_tiles.is_empty() && !bob_snapshot.visible_tiles.is_empty(),
        "initial arena snapshots should include visibility masks"
    );
    assert!(
        alice_snapshot.obstacles.len() < server.content.map().obstacles.len(),
        "alice should not receive the full terrain layout before exploring it"
    );
    assert!(
        bob_snapshot.obstacles.len() < server.content.map().obstacles.len(),
        "bob should not receive the full terrain layout before exploring it"
    );
    assert!(bob_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::MatchStarted { match_id: other, .. } if *other == match_id
    )));

    match_id
}

fn enter_combat(
    server: &mut ServerApp,
    transport: &mut InMemoryTransport,
    alice: &mut HeadlessClient,
    bob: &mut HeadlessClient,
    alice_choice: SkillChoice,
    bob_choice: SkillChoice,
) -> MatchId {
    let match_id = launch_match(server, transport, alice, bob);
    alice
        .choose_skill(transport, alice_choice)
        .expect("alice skill");
    bob.choose_skill(transport, bob_choice).expect("bob skill");
    server.pump_transport(transport);
    let _ = alice.drain_events(transport).expect("alice skill events");
    let _ = bob.drain_events(transport).expect("bob skill events");
    server.advance_seconds(transport, 5);
    let _ = alice
        .drain_events(transport)
        .expect("alice pre-combat events");
    let _ = bob.drain_events(transport).expect("bob pre-combat events");
    match_id
}

mod end_to_end;
mod state;
mod visibility;
