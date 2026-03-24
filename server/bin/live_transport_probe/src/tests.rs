use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use game_api::{spawn_dev_server_with_options, DevServerOptions, WebRtcRuntimeConfig};
use game_domain::SkillTree;
use game_net::SkillCatalogEntry;
use tokio::net::TcpListener;

use crate::planner::build_match_plans;
use crate::{run_probe, ProbeConfig};

fn temp_path(label: &str, suffix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after the unix epoch")
        .as_nanos();
    let path = std::env::temp_dir()
        .join("rusaren-live-transport-probe")
        .join(format!("{label}-{unique}.{suffix}"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("temp parent directory should exist");
    }
    path
}

fn temp_record_store_path() -> PathBuf {
    temp_path("records", "tsv")
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
    let (server, base_url) = start_server_fast().await;
    let output_path = temp_path("probe-log", "jsonl");
    let outcome = run_probe(ProbeConfig {
        origin: base_url,
        output_path: output_path.clone(),
        max_games: Some(1),
        connect_timeout: Duration::from_secs(15),
        stage_timeout: Duration::from_secs(20),
        round_timeout: Duration::from_secs(60),
        match_timeout: Duration::from_secs(180),
        input_cadence: Duration::from_millis(100),
        players_per_match: 4,
        preferred_tree_order: Some(vec![
            String::from("Warrior"),
            String::from("Rogue"),
            String::from("Mage"),
            String::from("Ranger"),
        ]),
        max_rounds_per_match: Some(1),
        max_combat_loops_per_round: Some(25),
    })
    .await
    .expect("probe should complete");

    assert_eq!(outcome.matches_completed, 1);
    assert!(outcome.covered_skills >= 4);
    assert!(fs::metadata(output_path).is_ok());

    server.shutdown().await;
}
