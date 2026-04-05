use std::fs;
use std::path::{Path, PathBuf};
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

const COMBAT_FRAME_MS: u16 = 100;
const DISPEL_TEST_CLERIC_YAML: &str = r"tree: Cleric
melee:
  id: cleric_probe_mace
  name: Probe Mace
  description: Reliable melee for probe integration tests.
  cooldown_ms: 550
  range: 88
  radius: 40
  effect: melee_swing
  payload:
    kind: damage
    amount: 12
skills:
  - tier: 1
    id: cleric_probe_cleanse
    name: Probe Cleanse
    description: Early self-cleanse for live transport probe tests.
    behavior:
      kind: nova
      effect: nova
      cooldown_ms: 700
      cast_time_ms: 150
      mana_cost: 10
      radius: 24
      payload:
        kind: heal
        amount: 6
        dispel:
          scope: negative
          max_statuses: 1
  - tier: 2
    id: cleric_probe_minor_heal
    name: Probe Minor Heal
    description: Filler heal for later rounds.
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 900
      cast_time_ms: 200
      mana_cost: 12
      range: 280
      radius: 28
      payload:
        kind: heal
        amount: 14
  - tier: 3
    id: cleric_probe_aegis
    name: Probe Aegis
    description: Filler shield for later rounds.
    behavior:
      kind: nova
      effect: nova
      cooldown_ms: 1400
      cast_time_ms: 250
      mana_cost: 18
      radius: 18
      payload:
        kind: heal
        amount: 0
        status:
          kind: shield
          duration_ms: 4000
          magnitude: 12
          max_stacks: 2
  - tier: 4
    id: cleric_probe_ward
    name: Probe Ward
    description: Filler ward for later rounds.
    behavior:
      kind: ward
      effect: nova
      cooldown_ms: 1400
      mana_cost: 18
      distance: 220
      radius: 160
      duration_ms: 0
      hit_points: 30
  - tier: 5
    id: cleric_probe_hymn
    name: Probe Hymn
    description: Filler aura for later rounds.
    behavior:
      kind: aura
      effect: nova
      cooldown_ms: 2200
      cast_time_ms: 500
      mana_cost: 26
      radius: 120
      duration_ms: 2500
      tick_interval_ms: 500
      payload:
        kind: heal
        amount: 4
";
const DISPEL_TEST_MAGE_YAML: &str = r"tree: Mage
melee:
  id: mage_probe_staff
  name: Probe Staff
  description: Reliable melee for probe integration tests.
  cooldown_ms: 600
  range: 84
  radius: 36
  effect: melee_swing
  payload:
    kind: damage
    amount: 10
skills:
  - tier: 1
    id: mage_probe_frost_burst
    name: Probe Frost Burst
    description: Early chill application for dispel probe tests.
    behavior:
      kind: burst
      effect: burst
      cooldown_ms: 700
      mana_cost: 10
      range: 260
      radius: 100
      payload:
        kind: damage
        amount: 8
        status:
          kind: chill
          duration_ms: 4000
          magnitude: 1200
          max_stacks: 1
          trigger_duration_ms: 700
  - tier: 2
    id: mage_probe_arc_bolt
    name: Probe Arc Bolt
    description: Filler projectile for later rounds.
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 700
      mana_cost: 12
      speed: 320
      range: 1200
      radius: 18
      payload:
        kind: damage
        amount: 16
  - tier: 3
    id: mage_probe_blink
    name: Probe Blink
    description: Filler teleport for later rounds.
    behavior:
      kind: teleport
      effect: dash_trail
      cooldown_ms: 1500
      mana_cost: 18
      distance: 180
  - tier: 4
    id: mage_probe_scrying
    name: Probe Scrying
    description: Filler aura for later rounds.
    behavior:
      kind: aura
      effect: nova
      cooldown_ms: 2000
      mana_cost: 24
      distance: 240
      radius: 100
      duration_ms: 3000
      hit_points: 30
      tick_interval_ms: 750
      payload:
        kind: damage
        amount: 3
        status:
          kind: reveal
          duration_ms: 1000
          magnitude: 0
  - tier: 5
    id: mage_probe_focus
    name: Probe Focus
    description: Filler passive for later rounds.
    behavior:
      kind: passive
      effect: nova
      projectile_speed_bps: 2500
      cast_time_bps: 1000
";
const CHANNEL_TEST_WARRIOR_YAML: &str = r"tree: Warrior
melee:
  id: warrior_probe_broadswing
  name: Probe Broadswing
  description: Reliable melee for probe integration tests.
  cooldown_ms: 650
  range: 92
  radius: 42
  effect: melee_swing
  payload:
    kind: damage
    amount: 18
skills:
  - tier: 1
    id: warrior_probe_channel
    name: Probe Earthshatter
    description: Early channel for live transport probe tests.
    behavior:
      kind: channel
      effect: nova
      cooldown_ms: 1200
      cast_time_ms: 200
      mana_cost: 14
      radius: 130
      duration_ms: 1800
      tick_interval_ms: 300
      payload:
        kind: damage
        amount: 5
        status:
          kind: chill
          duration_ms: 1200
          magnitude: 800
          max_stacks: 2
          trigger_duration_ms: 500
  - tier: 2
    id: warrior_probe_beam
    name: Probe Slash
    description: Filler beam for later rounds.
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 1000
      mana_cost: 12
      range: 170
      radius: 40
      payload:
        kind: damage
        amount: 18
  - tier: 3
    id: warrior_probe_roar
    name: Probe Roar
    description: Filler nova for later rounds.
    behavior:
      kind: nova
      effect: nova
      cooldown_ms: 1500
      cast_time_ms: 250
      mana_cost: 18
      radius: 120
      payload:
        kind: damage
        amount: 10
        status:
          kind: fear
          duration_ms: 1500
          magnitude: 0
  - tier: 4
    id: warrior_probe_spear
    name: Probe Spear
    description: Filler interrupt projectile for later rounds.
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 1600
      cast_time_ms: 150
      mana_cost: 18
      speed: 360
      range: 920
      radius: 18
      payload:
        kind: damage
        amount: 12
        interrupt_silence_duration_ms: 1200
  - tier: 5
    id: warrior_probe_wall
    name: Probe Wall
    description: Filler barrier for later rounds.
    behavior:
      kind: barrier
      effect: burst
      cooldown_ms: 1800
      mana_cost: 18
      distance: 130
      radius: 48
      duration_ms: 1800
      hit_points: 60
";
const CHANNEL_TEST_RANGER_YAML: &str = r"tree: Ranger
melee:
  id: ranger_probe_knife
  name: Probe Knife
  description: Reliable melee for probe integration tests.
  cooldown_ms: 600
  range: 86
  radius: 38
  effect: melee_swing
  payload:
    kind: damage
    amount: 12
skills:
  - tier: 1
    id: ranger_probe_shot
    name: Probe Shot
    description: Reliable projectile target for probe integration tests.
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 700
      mana_cost: 10
      speed: 340
      range: 1300
      radius: 18
      payload:
        kind: damage
        amount: 15
  - tier: 2
    id: ranger_probe_dash
    name: Probe Dash
    description: Filler dash for later rounds.
    behavior:
      kind: dash
      effect: dash_trail
      cooldown_ms: 1200
      mana_cost: 14
      distance: 160
  - tier: 3
    id: ranger_probe_burst
    name: Probe Burst
    description: Filler burst for later rounds.
    behavior:
      kind: burst
      effect: burst
      cooldown_ms: 1200
      mana_cost: 16
      range: 200
      radius: 90
      payload:
        kind: damage
        amount: 10
  - tier: 4
    id: ranger_probe_ward
    name: Probe Ward
    description: Filler ward for later rounds.
    behavior:
      kind: ward
      effect: nova
      cooldown_ms: 1500
      mana_cost: 18
      distance: 240
      radius: 160
      duration_ms: 0
      hit_points: 30
  - tier: 5
    id: ranger_probe_volley
    name: Probe Volley
    description: Filler beam for later rounds.
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 1300
      mana_cost: 18
      range: 260
      radius: 30
      payload:
        kind: damage
        amount: 16
";

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

fn remove_dir_if_exists(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).expect("temporary directory should be removable");
    }
}

fn copy_dir_all(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("destination directory should be creatable");
    for entry in fs::read_dir(source).expect("source directory should be readable") {
        let entry = entry.expect("directory entry should be readable");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir_all(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path).expect("file copy should succeed");
        }
    }
}

fn temp_content_root(prefix: &str) -> PathBuf {
    temp_path(&format!("content-root-{prefix}"), "dir")
}

fn write_skill_override(root: &Path, file_name: &str, yaml: &str) {
    fs::write(root.join("skills").join(file_name), yaml).expect("skill override should write");
}

fn prune_skill_files(root: &Path, keep: &[&str]) {
    let skills_dir = root.join("skills");
    let keep: std::collections::BTreeSet<&str> = keep.iter().copied().collect();
    for entry in fs::read_dir(&skills_dir).expect("skills directory should be readable") {
        let entry = entry.expect("skill entry should be readable");
        if entry.file_type().expect("skill file type").is_file() {
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if !keep.contains(file_name.as_ref()) {
                fs::remove_file(entry.path()).expect("unneeded skill file should be removable");
            }
        }
    }
}

fn mechanic_probe_content_root(prefix: &str, skill_files: &[(&str, &str)]) -> PathBuf {
    let root = temp_content_root(prefix);
    remove_dir_if_exists(&root);
    copy_dir_all(&repo_content_root(), &root);
    let keep: Vec<&str> = skill_files.iter().map(|(file_name, _)| *file_name).collect();
    prune_skill_files(&root, &keep);
    for (file_name, yaml) in skill_files {
        write_skill_override(&root, file_name, yaml);
    }
    root
}

fn dispel_probe_content_root() -> PathBuf {
    mechanic_probe_content_root(
        "probe-dispel",
        &[
            ("cleric.yaml", DISPEL_TEST_CLERIC_YAML),
            ("mage.yaml", DISPEL_TEST_MAGE_YAML),
            ("warrior.yaml", include_str!("../../../content/skills/warrior.yaml")),
        ],
    )
}

fn channel_probe_content_root() -> PathBuf {
    mechanic_probe_content_root(
        "probe-channel",
        &[
            ("warrior.yaml", CHANNEL_TEST_WARRIOR_YAML),
            ("ranger.yaml", CHANNEL_TEST_RANGER_YAML),
        ],
    )
}

fn skip_live_probe_tests() -> bool {
    std::env::var_os("RARENA_SKIP_LIVE_PROBE_TESTS").is_some()
}

async fn start_server_fast_with_content_root(
    content_root: PathBuf,
) -> (game_api::DevServerHandle, String) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let server = spawn_dev_server_with_options(
        listener,
        DevServerOptions {
            tick_interval: Duration::from_millis(10),
            simulation_step_ms: COMBAT_FRAME_MS,
            record_store_path: temp_record_store_path(),
            combat_log_path: temp_combat_log_path(),
            content_root,
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

async fn start_server_fast() -> (game_api::DevServerHandle, String) {
    start_server_fast_with_content_root(repo_content_root()).await
}

async fn run_probe_with_fresh_server(
    config: ProbeConfig,
) -> crate::ProbeResult<crate::ProbeOutcome> {
    let content_root = config.content_root.clone().unwrap_or_else(repo_content_root);
    let (server, base_url) = start_server_fast_with_content_root(content_root).await;
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
        content_root: None,
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
        content_root: Some(dispel_probe_content_root()),
        max_games: Some(1),
        connect_timeout: Duration::from_secs(15),
        stage_timeout: Duration::from_secs(20),
        round_timeout: Duration::from_secs(40),
        match_timeout: Duration::from_secs(60),
        input_cadence: Duration::from_millis(60),
        players_per_match: 2,
        preferred_tree_order: Some(vec![String::from("Cleric"), String::from("Mage")]),
        max_rounds_per_match: Some(1),
        max_combat_loops_per_round: Some(180),
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
        content_root: Some(channel_probe_content_root()),
        max_games: Some(1),
        connect_timeout: Duration::from_secs(15),
        stage_timeout: Duration::from_secs(20),
        round_timeout: Duration::from_secs(30),
        match_timeout: Duration::from_secs(60),
        input_cadence: Duration::from_millis(80),
        players_per_match: 2,
        preferred_tree_order: Some(vec![String::from("Warrior"), String::from("Ranger")]),
        max_rounds_per_match: Some(1),
        max_combat_loops_per_round: Some(120),
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
                audio_cue_id: String::new(),
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
    if skip_live_probe_tests() {
        return;
    }
    let _guard = live_probe_test_mutex().lock().await;
    let (server, base_url) = start_server_fast().await;
    let output_path = temp_path("probe-log", "jsonl");
    let outcome = run_probe(ProbeConfig {
        origin: base_url,
        output_path: output_path.clone(),
        content_root: None,
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
    if skip_live_probe_tests() {
        return;
    }
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
    if skip_live_probe_tests() {
        return;
    }
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
    if skip_live_probe_tests() {
        return;
    }
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
