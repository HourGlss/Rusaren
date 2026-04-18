#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use game_api::{
    CombatLogEntry, CombatLogEvent, CombatLogPhase, CombatLogStore, ConnectionId, HeadlessClient,
    InMemoryTransport, ServerApp,
};
use game_content::GameContent;
use game_domain::{LobbyId, MatchId, PlayerName, ReadyState, SkillChoice, SkillTree, TeamSide};
use game_net::ServerControlEvent;

const MATCH_PAIR_COUNT: usize = 10;
const IDLE_CLIENT_COUNT: usize = 100;
const COMBAT_FRAME_MS: u16 = 100;

#[derive(Default)]
struct DurationSamples {
    samples_ms: Vec<f64>,
}

impl DurationSamples {
    fn record(&mut self, duration: Duration) {
        self.samples_ms.push(duration.as_secs_f64() * 1000.0);
    }

    fn p95_ms(&self) -> f64 {
        percentile_ms(&self.samples_ms, 95, 100)
    }

    fn p99_ms(&self) -> f64 {
        percentile_ms(&self.samples_ms, 99, 100)
    }

    fn max_ms(&self) -> f64 {
        self.samples_ms.iter().copied().fold(0.0, f64::max)
    }
}

fn percentile_ms(samples: &[f64], numerator: usize, denominator: usize) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let last_index = sorted.len().saturating_sub(1);
    let index = last_index
        .saturating_mul(numerator)
        .saturating_add(denominator / 2)
        / denominator;
    sorted[index]
}

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
    command_latencies: &mut DurationSamples,
) -> HeadlessClient {
    let mut client = HeadlessClient::new(connection_id(raw_connection_id), player_name(raw_name));
    let started_at = Instant::now();
    client.connect(transport).expect("connect packet");
    server.pump_transport(transport);
    let events = client.drain_events(transport).expect("connect events");
    command_latencies.record(started_at.elapsed());
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
        .expect("match should exist")
}

fn launch_match_pair(
    server: &mut ServerApp,
    transport: &mut InMemoryTransport,
    left: &mut HeadlessClient,
    right: &mut HeadlessClient,
    command_latencies: &mut DurationSamples,
) -> MatchId {
    let started_at = Instant::now();
    left.create_game_lobby(transport).expect("create lobby");
    server.pump_transport(transport);
    let left_events = left.drain_events(transport).expect("left create events");
    command_latencies.record(started_at.elapsed());
    let lobby_id = lobby_id_from(&left_events);

    let started_at = Instant::now();
    right
        .join_game_lobby(transport, lobby_id)
        .expect("join lobby");
    server.pump_transport(transport);
    command_latencies.record(started_at.elapsed());
    let _ = left.drain_events(transport).expect("left join events");
    let _ = right.drain_events(transport).expect("right join events");

    let started_at = Instant::now();
    left.select_team(transport, TeamSide::TeamA)
        .expect("left team");
    right
        .select_team(transport, TeamSide::TeamB)
        .expect("right team");
    server.pump_transport(transport);
    command_latencies.record(started_at.elapsed());
    let _ = left.drain_events(transport).expect("left team events");
    let _ = right.drain_events(transport).expect("right team events");

    let started_at = Instant::now();
    left.set_ready(transport, ReadyState::Ready)
        .expect("left ready");
    right
        .set_ready(transport, ReadyState::Ready)
        .expect("right ready");
    server.pump_transport(transport);
    command_latencies.record(started_at.elapsed());
    let _ = left.drain_events(transport).expect("left ready events");
    let _ = right.drain_events(transport).expect("right ready events");

    server.advance_seconds(transport, 5);
    let left_events = left.drain_events(transport).expect("left launch events");
    let _ = right.drain_events(transport).expect("right launch events");
    match_id_from(&left_events)
}

fn choose_round_one_skills(
    server: &mut ServerApp,
    transport: &mut InMemoryTransport,
    left: &mut HeadlessClient,
    right: &mut HeadlessClient,
    command_latencies: &mut DurationSamples,
) {
    let started_at = Instant::now();
    left.choose_skill(transport, skill(SkillTree::Mage, 1))
        .expect("left skill");
    right
        .choose_skill(transport, skill(SkillTree::Rogue, 1))
        .expect("right skill");
    server.pump_transport(transport);
    command_latencies.record(started_at.elapsed());
    let _ = left.drain_events(transport).expect("left skill events");
    let _ = right.drain_events(transport).expect("right skill events");

    server.advance_seconds(transport, 5);
    let _ = left
        .drain_events(transport)
        .expect("left pre-combat events");
    let _ = right
        .drain_events(transport)
        .expect("right pre-combat events");
}

fn drain_active_match_clients(clients: &mut [HeadlessClient], transport: &mut InMemoryTransport) {
    for client in clients {
        let _ = client
            .drain_events(transport)
            .expect("combat events should remain decodable");
    }
}

fn assert_command_latency_budgets(command_latencies: &DurationSamples) {
    assert!(
        command_latencies.p95_ms() <= 50.0,
        "command latency p95 exceeded budget: {:.3} ms",
        command_latencies.p95_ms()
    );
    assert!(
        command_latencies.p99_ms() <= 100.0,
        "command latency p99 exceeded budget: {:.3} ms",
        command_latencies.p99_ms()
    );
}

fn assert_tick_latency_budgets(tick_latencies: &DurationSamples) {
    assert!(
        tick_latencies.p95_ms() <= 12.0,
        "tick latency p95 exceeded budget: {:.3} ms",
        tick_latencies.p95_ms()
    );
    assert!(
        tick_latencies.p99_ms() <= 20.0,
        "tick latency p99 exceeded budget: {:.3} ms",
        tick_latencies.p99_ms()
    );
    assert!(
        tick_latencies.max_ms() <= 32.0,
        "tick latency max exceeded budget: {:.3} ms",
        tick_latencies.max_ms()
    );
}

#[cfg(target_os = "linux")]
fn current_rss_mib() -> Option<f64> {
    let contents = std::fs::read_to_string("/proc/self/status").ok()?;
    let raw_line = contents
        .lines()
        .find(|line| line.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?;
    let kib = raw_line.parse::<f64>().ok()?;
    Some(kib / 1024.0)
}

#[cfg(not(target_os = "linux"))]
fn current_rss_mib() -> Option<f64> {
    None
}

#[test]
fn reference_load_scenarios_hold_tick_command_memory_and_capacity_budgets() {
    if !cfg!(target_os = "linux") {
        eprintln!("skipping fixed-reference performance gate outside linux");
        return;
    }
    if cfg!(debug_assertions) {
        eprintln!("skipping fixed-reference performance gate in debug builds");
        return;
    }

    let content = GameContent::load_from_root(repo_content_root()).expect("content should load");
    let record_store_path = temp_path("perf-gate-records", "tsv");
    let combat_log_path = temp_path("perf-gate-combat", "sqlite");
    let mut server = ServerApp::new_persistent_with_content_and_log(
        content,
        &record_store_path,
        &combat_log_path,
    )
    .expect("persistent server app should initialize");
    let mut transport = InMemoryTransport::new();
    let mut command_latencies = DurationSamples::default();
    let mut clients = Vec::new();

    for raw_connection_id in 1..=u64::try_from(IDLE_CLIENT_COUNT).expect("count should fit") {
        clients.push(connect_player(
            &mut server,
            &mut transport,
            raw_connection_id,
            &format!("Player{raw_connection_id:03}"),
            &mut command_latencies,
        ));
    }

    assert_eq!(server.connected_player_count(), IDLE_CLIENT_COUNT);

    let mut launched_match_ids = Vec::new();
    for pair_index in 0..MATCH_PAIR_COUNT {
        let left_index = pair_index * 2;
        let (_, right_slice) = clients.split_at_mut(left_index);
        let (left_slice, right_slice) = right_slice.split_at_mut(1);
        let left_client = &mut left_slice[0];
        let right_client = &mut right_slice[0];
        let match_id = launch_match_pair(
            &mut server,
            &mut transport,
            left_client,
            right_client,
            &mut command_latencies,
        );
        launched_match_ids.push(match_id);
        choose_round_one_skills(
            &mut server,
            &mut transport,
            left_client,
            right_client,
            &mut command_latencies,
        );
    }

    assert_eq!(server.active_match_count(), MATCH_PAIR_COUNT);
    assert_eq!(launched_match_ids.len(), MATCH_PAIR_COUNT);

    for _ in 0..30 {
        server.advance_millis(&mut transport, COMBAT_FRAME_MS);
        drain_active_match_clients(&mut clients[..MATCH_PAIR_COUNT * 2], &mut transport);
    }

    let mut tick_latencies = DurationSamples::default();
    for _ in 0..120 {
        let started_at = Instant::now();
        server.advance_millis(&mut transport, COMBAT_FRAME_MS);
        tick_latencies.record(started_at.elapsed());

        drain_active_match_clients(&mut clients[..MATCH_PAIR_COUNT * 2], &mut transport);
    }

    assert_command_latency_budgets(&command_latencies);
    assert_tick_latency_budgets(&tick_latencies);
    if let Some(rss_mib) = current_rss_mib() {
        assert!(
            rss_mib <= 350.0,
            "backend RSS exceeded budget: {rss_mib:.1} MiB"
        );
    }
}

#[test]
fn reference_sqlite_logging_holds_append_and_query_budgets() {
    if !cfg!(target_os = "linux") {
        eprintln!("skipping fixed-reference sqlite gate outside linux");
        return;
    }
    if cfg!(debug_assertions) {
        eprintln!("skipping fixed-reference sqlite gate in debug builds");
        return;
    }

    let store_path = temp_path("perf-gate-sqlite", "sqlite");
    let mut store = CombatLogStore::new_persistent(&store_path)
        .expect("persistent combat log store should initialize");
    let mut append_latencies = DurationSamples::default();
    let mut query_latencies = DurationSamples::default();

    for match_raw in 1..=10_u32 {
        let match_id = MatchId::new(match_raw).expect("match id");
        for frame_index in 0..64_u32 {
            let entry = CombatLogEntry::new(
                match_id,
                1,
                CombatLogPhase::Combat,
                frame_index,
                CombatLogEvent::CombatStarted,
            );
            let started_at = Instant::now();
            store.append(&entry).expect("append should succeed");
            append_latencies.record(started_at.elapsed());
        }
    }

    let mut counted_entries = 0_u64;
    for match_raw in 1..=10_u32 {
        let match_id = MatchId::new(match_raw).expect("match id");
        let started_at = Instant::now();
        let entries = store
            .events_for_match_limit(match_id, 32)
            .expect("bounded query should succeed");
        query_latencies.record(started_at.elapsed());
        counted_entries += u64::try_from(entries.len()).unwrap_or(0);
        assert_eq!(entries.len(), 32);
    }

    let started_at = Instant::now();
    let summaries = store
        .recent_matches(10)
        .expect("recent matches should load");
    query_latencies.record(started_at.elapsed());
    assert_eq!(summaries.len(), 10);

    assert_eq!(counted_entries, 320);
    let started_at = Instant::now();
    let full_entries = store
        .events_for_match(MatchId::new(1).expect("match id"))
        .expect("full query should succeed");
    query_latencies.record(started_at.elapsed());
    assert_eq!(full_entries.len(), 64);

    assert!(
        append_latencies.p95_ms() <= 10.0,
        "sqlite append p95 exceeded budget: {:.3} ms",
        append_latencies.p95_ms()
    );
    assert!(
        append_latencies.p99_ms() <= 25.0,
        "sqlite append p99 exceeded budget: {:.3} ms",
        append_latencies.p99_ms()
    );
    assert!(
        query_latencies.p95_ms() <= 10.0,
        "sqlite query p95 exceeded budget: {:.3} ms",
        query_latencies.p95_ms()
    );
    assert!(
        query_latencies.p99_ms() <= 25.0,
        "sqlite query p99 exceeded budget: {:.3} ms",
        query_latencies.p99_ms()
    );

    let _ = std::fs::remove_file(store_path);
}
