#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use game_api::{ConnectionId, HeadlessClient, InMemoryTransport, ServerApp};
use game_content::GameContent;
use game_domain::{LobbyId, MatchId, PlayerName, ReadyState, SkillChoice, SkillTree, TeamSide};
use game_net::ServerControlEvent;

fn temp_path(stem: &str, extension: &str) -> PathBuf {
    static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("test-temp")
        .join(format!(
            "rusaren-{stem}-{}-{counter}.{extension}",
            std::process::id()
        ))
}

fn repo_content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content")
}

fn connection_id(raw: u64) -> ConnectionId {
    ConnectionId::new(raw).expect("valid connection id")
}

fn player_name(raw: &str) -> PlayerName {
    PlayerName::new(raw).expect("valid player name")
}

fn skill(tree: SkillTree, tier: u8) -> SkillChoice {
    SkillChoice::new(tree, tier).expect("valid skill choice")
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

fn lobby_id_from(events: &[ServerControlEvent]) -> LobbyId {
    events
        .iter()
        .find_map(|event| match event {
            ServerControlEvent::GameLobbyCreated { lobby_id } => Some(*lobby_id),
            _ => None,
        })
        .expect("game lobby should exist")
}

fn match_id_from(events: &[ServerControlEvent]) -> MatchId {
    events
        .iter()
        .find_map(|event| match event {
            ServerControlEvent::MatchStarted { match_id, .. } => Some(*match_id),
            _ => None,
        })
        .expect("match should start")
}

#[test]
fn recent_combat_log_views_surface_latest_matches_and_entries() {
    let content = GameContent::load_from_root(repo_content_root()).expect("content should load");
    let record_store_path = temp_path("combat-log-views-records", "tsv");
    let combat_log_path = temp_path("combat-log-views-combat", "sqlite");
    let mut server = ServerApp::new_persistent_with_content_and_log(
        content,
        &record_store_path,
        &combat_log_path,
    )
    .expect("persistent server app should initialize");
    let mut transport = InMemoryTransport::new();

    let mut alice = connect_player(&mut server, &mut transport, 1, "Alice");
    let mut bob = connect_player(&mut server, &mut transport, 2, "Bob");

    alice
        .create_game_lobby(&mut transport)
        .expect("create lobby");
    server.pump_transport(&mut transport);
    let alice_events = alice.drain_events(&mut transport).expect("alice events");
    let lobby_id = lobby_id_from(&alice_events);

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

    server.advance_seconds(&mut transport, 5);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice launch events");
    let _ = bob.drain_events(&mut transport).expect("bob launch events");
    let match_id = match_id_from(&alice_events);

    alice
        .choose_skill(&mut transport, skill(SkillTree::Mage, 1))
        .expect("alice skill");
    bob.choose_skill(&mut transport, skill(SkillTree::Rogue, 1))
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

    let recent_matches = server
        .recent_combat_log_matches(4)
        .expect("recent combat log matches should load");
    assert_eq!(recent_matches.len(), 1);
    let summary = &recent_matches[0];
    assert_eq!(summary.match_id, match_id.get());
    assert!(summary.event_count > 0);
    assert!(!summary.last_event_kind.is_empty());

    let recent_entries = server
        .combat_log_entries_limit(match_id, 10)
        .expect("recent combat log entries should load");
    assert!(!recent_entries.is_empty());
    assert!(recent_entries.len() <= 10);
    assert!(recent_entries
        .windows(2)
        .all(|window| window[0].sequence <= window[1].sequence));
    assert!(recent_entries
        .iter()
        .any(|entry| matches!(entry.event, game_api::CombatLogEvent::SkillPicked { .. })));
}
