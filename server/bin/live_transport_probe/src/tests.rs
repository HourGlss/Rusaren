use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use game_api::{spawn_dev_server_with_options, DevServerOptions, WebRtcRuntimeConfig};
use game_domain::SkillTree;
use game_net::SkillCatalogEntry;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::planner::build_match_plans;
use crate::{run_probe, ProbeConfig, ProbeMechanicObservation};

fn temp_path(label: &str, suffix: &str) -> PathBuf {
    static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after the unix epoch")
        .as_nanos();
    let counter = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir()
        .join("rusaren-live-transport-probe")
        .join(format!(
            "{label}-{}-{unique}-{counter}.{suffix}",
            std::process::id()
        ));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("temp parent directory should exist");
    }
    path
}

fn temp_record_store_path() -> PathBuf {
    temp_path("records", "tsv")
}

fn temp_combat_log_path() -> PathBuf {
    temp_path("combat-log", "sqlite")
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

fn temp_web_client_root() -> PathBuf {
    let root = temp_path("web-root", "dir");
    fs::create_dir_all(&root).expect("temporary web root should be creatable");
    root
}

fn live_probe_test_mutex() -> &'static Mutex<()> {
    static LIVE_PROBE_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    LIVE_PROBE_TEST_MUTEX.get_or_init(|| Mutex::new(()))
}

async fn start_server_fast() -> (game_api::DevServerHandle, String) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let server = spawn_dev_server_with_options(
        listener,
        DevServerOptions {
            tick_interval: Duration::from_millis(10),
            simulation_step_ms: game_sim::COMBAT_FRAME_MS,
            record_store_path: temp_record_store_path(),
            combat_log_path: temp_combat_log_path(),
            content_root: repo_content_root(),
            web_client_root: temp_web_client_root(),
            observability: None,
            webrtc: WebRtcRuntimeConfig::default(),
            admin_auth: None,
        },
    )
    .await
    .expect("server should spawn");
    let base_url = format!("ws://{}", server.local_addr());
    (server, base_url)
}

async fn run_probe_with_fresh_server(
    config: ProbeConfig,
) -> crate::ProbeResult<crate::ProbeOutcome> {
    let (server, base_url) = start_server_fast().await;
    let outcome = run_probe(ProbeConfig {
        origin: base_url,
        ..config
    })
    .await;
    server.shutdown().await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    outcome
}

async fn run_probe_until_mechanic_observed(
    config: ProbeConfig,
    mechanic: ProbeMechanicObservation,
    max_attempts: usize,
) -> crate::ProbeResult<crate::ProbeOutcome> {
    let mut last_outcome = None;
    for attempt in 0..max_attempts {
        let outcome = run_probe_with_fresh_server(config.clone()).await?;
        if outcome.observed_mechanics.contains(&mechanic) {
            return Ok(outcome);
        }
        last_outcome = Some(outcome);
        if attempt + 1 < max_attempts {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    if let Some(outcome) = last_outcome {
        return Err(crate::ProbeError::new(format!(
            "probe did not observe {} after {} attempts; last observed mechanics: {:?}",
            mechanic.as_str(),
            max_attempts,
            outcome
                .observed_mechanics
                .iter()
                .map(|observed| observed.as_str())
                .collect::<Vec<_>>()
        )));
    }

    Err(crate::ProbeError::new(
        "probe retry loop produced no outcome",
    ))
}

fn periodic_probe_config(output_path: PathBuf) -> ProbeConfig {
    ProbeConfig {
        origin: String::new(),
        output_path,
        max_games: Some(1),
        connect_timeout: Duration::from_secs(15),
        stage_timeout: Duration::from_secs(20),
        round_timeout: Duration::from_secs(90),
        match_timeout: Duration::from_secs(120),
        input_cadence: Duration::from_millis(80),
        players_per_match: 3,
        preferred_tree_order: Some(vec![
            String::from("Cleric"),
            String::from("Rogue"),
            String::from("Necromancer"),
        ]),
        max_rounds_per_match: Some(1),
        max_combat_loops_per_round: Some(180),
        required_mechanics: Some(
            [ProbeMechanicObservation::MultiSourcePeriodicStack]
                .into_iter()
                .collect(),
        ),
    }
}

fn dispel_probe_config(output_path: PathBuf) -> ProbeConfig {
    ProbeConfig {
        origin: String::new(),
        output_path,
        max_games: Some(1),
        connect_timeout: Duration::from_secs(15),
        stage_timeout: Duration::from_secs(20),
        round_timeout: Duration::from_secs(90),
        match_timeout: Duration::from_secs(300),
        input_cadence: Duration::from_millis(80),
        players_per_match: 2,
        preferred_tree_order: Some(vec![String::from("Cleric"), String::from("Mage")]),
        max_rounds_per_match: Some(4),
        max_combat_loops_per_round: Some(240),
        required_mechanics: Some(
            [ProbeMechanicObservation::DispelResolved]
                .into_iter()
                .collect(),
        ),
    }
}

fn channel_probe_config(output_path: PathBuf) -> ProbeConfig {
    ProbeConfig {
        origin: String::new(),
        output_path,
        max_games: Some(1),
        connect_timeout: Duration::from_secs(15),
        stage_timeout: Duration::from_secs(20),
        round_timeout: Duration::from_secs(60),
        match_timeout: Duration::from_secs(360),
        input_cadence: Duration::from_millis(80),
        players_per_match: 2,
        preferred_tree_order: Some(vec![String::from("Warrior"), String::from("Ranger")]),
        max_rounds_per_match: Some(5),
        max_combat_loops_per_round: Some(260),
        required_mechanics: Some(
            [ProbeMechanicObservation::ChannelMaintained]
                .into_iter()
                .collect(),
        ),
    }
}

#[test]
fn planner_covers_all_trees_and_fills_the_last_match() {
    let mut catalog = Vec::new();
    for tree_name in [
        "Warrior",
        "Rogue",
        "Mage",
        "Cleric",
        "Bard",
        "Druid",
        "Necromancer",
        "Paladin",
        "Ranger",
    ] {
        let tree = SkillTree::new(tree_name).expect("tree should parse");
        for tier in 1..=5 {
            catalog.push(SkillCatalogEntry {
                tree: tree.clone(),
                tier,
                skill_id: format!("{tree_name}-{tier}"),
                skill_name: format!("{tree_name} {tier}"),
                skill_description: format!("{tree_name} tier {tier}"),
                skill_summary: String::from("Test summary"),
                ui_category: String::from("neutral"),
            });
        }
    }

    let (trees, plans) = build_match_plans(&catalog, 4, None).expect("plan should build");
    assert_eq!(trees.len(), 9);
    assert_eq!(plans.len(), 3);
    assert_eq!(plans[0].players.len(), 4);
    assert_eq!(plans[1].players.len(), 4);
    assert_eq!(plans[2].players.len(), 4);
    assert_eq!(plans[2].players[0].tiers, vec![1, 2, 3, 4, 5]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_probe_completes_one_real_webrtc_match_against_the_dev_server() {
    let _guard = live_probe_test_mutex().lock().await;
    let (server, base_url) = start_server_fast().await;
    let output_path = temp_path("probe-log", "jsonl");
    let outcome = run_probe(ProbeConfig {
        origin: base_url,
        output_path: output_path.clone(),
        max_games: Some(1),
        connect_timeout: Duration::from_secs(30),
        stage_timeout: Duration::from_secs(45),
        round_timeout: Duration::from_secs(90),
        match_timeout: Duration::from_secs(300),
        input_cadence: Duration::from_millis(100),
        players_per_match: 4,
        preferred_tree_order: Some(vec![
            String::from("Warrior"),
            String::from("Rogue"),
            String::from("Mage"),
            String::from("Ranger"),
        ]),
        max_rounds_per_match: Some(1),
        max_combat_loops_per_round: Some(40),
        required_mechanics: None,
    })
    .await
    .expect("probe should complete");

    assert_eq!(outcome.matches_completed, 1);
    assert!(outcome.covered_skills >= 4);
    assert!(fs::metadata(output_path).is_ok());

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_probe_exercises_periodic_stacks_against_the_dev_server() {
    let _guard = live_probe_test_mutex().lock().await;
    let periodic_output_path = temp_path("probe-periodic-log", "jsonl");
    let periodic_outcome = run_probe_until_mechanic_observed(
        periodic_probe_config(periodic_output_path.clone()),
        ProbeMechanicObservation::MultiSourcePeriodicStack,
        3,
    )
    .await
    .expect("probe should exercise multi-source periodic stacking");

    assert_eq!(periodic_outcome.matches_completed, 1);
    assert!(fs::metadata(periodic_output_path).is_ok());
    assert!(periodic_outcome
        .observed_mechanics
        .contains(&ProbeMechanicObservation::MultiSourcePeriodicStack));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_probe_exercises_dispels_against_the_dev_server() {
    let _guard = live_probe_test_mutex().lock().await;
    let dispel_outcome = run_probe_until_mechanic_observed(
        dispel_probe_config(temp_path("probe-dispel-log", "jsonl")),
        ProbeMechanicObservation::DispelResolved,
        4,
    )
    .await
    .expect("probe should exercise dispel resolution");

    assert_eq!(dispel_outcome.matches_completed, 1);
    assert!(dispel_outcome
        .observed_mechanics
        .contains(&ProbeMechanicObservation::DispelResolved));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_probe_exercises_channels_against_the_dev_server() {
    let _guard = live_probe_test_mutex().lock().await;
    let channel_outcome = run_probe_until_mechanic_observed(
        channel_probe_config(temp_path("probe-channel-log", "jsonl")),
        ProbeMechanicObservation::ChannelMaintained,
        4,
    )
    .await
    .expect("probe should exercise channel maintenance");

    assert_eq!(channel_outcome.matches_completed, 1);
    assert!(channel_outcome
        .observed_mechanics
        .contains(&ProbeMechanicObservation::ChannelMaintained));
}
