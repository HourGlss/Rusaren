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
const PROBE_TEST_CONFIG_YAML: &str = r"lobby:
  launch_countdown_seconds: 1

match:
  total_rounds: 1
  skill_pick_seconds: 5
  pre_combat_seconds: 1

maps:
  tile_units: 50
  objective_target_ms_by_map:
    prototype_arena: 30000
    template_arena: 30000
    training_arena: 30000
  generation:
    max_generation_attempts: 1
    protected_tile_buffer_radius_tiles: 0
    obstacle_edge_padding_tiles: 0
    wall_segment_lengths_tiles: [2, 3]
    long_wall_percent: 0
    wall_candidate_skip_percent: 100
    wall_min_spacing_manhattan_tiles: 0
    pillar_candidate_skip_percent: 100
    pillar_min_spacing_manhattan_tiles: 0
    styles:
      - shrub_clusters: 0
        shrub_radius_tiles: 0
        shrub_soft_radius_tiles: 0
        shrub_fill_percent: 0
        wall_segments: 0
        isolated_pillars: 0

simulation:
  combat_frame_ms: 100
  player_radius_units: 28
  vision_radius_units: 2000
  spawn_spacing_units: 80
  default_aim_x_units: 120
  default_aim_y_units: 0
  mana_regen_per_second: 30
  global_projectile_speed_bonus_bps: 0
  teleport_resolution_steps: 48
  movement_audio_step_interval_ms: 240
  movement_audio_radius_units: 520
  stealth_audio_radius_units: 170
  brush_movement_audible_percent: 22
  passive_bonus_caps:
    player_speed_bps: 9000
    projectile_speed_bps: 9000
    cooldown_bps: 9000
    cast_time_bps: 9500
  movement_modifier_caps:
    chill_bps: 8000
    haste_bps: 6000
    status_total_min_bps: -8000
    status_total_max_bps: 6000
    overall_total_min_bps: -8000
    overall_total_max_bps: 9000
    effective_scale_min_bps: 2000
    effective_scale_max_bps: 16000
  crowd_control_diminishing_returns:
    window_ms: 15000
    stages_bps: [10000, 5000, 2500, 0]
  training_dummy:
    base_hit_points: 100
    health_multiplier: 100
    execute_threshold_bps: 500

classes:
  Bard:
    hit_points: 240
    max_mana: 180
    move_speed_units_per_second: 325
  Cleric:
    hit_points: 280
    max_mana: 180
    move_speed_units_per_second: 320
  Druid:
    hit_points: 240
    max_mana: 180
    move_speed_units_per_second: 320
  Mage:
    hit_points: 220
    max_mana: 180
    move_speed_units_per_second: 320
  Rogue:
    hit_points: 220
    max_mana: 140
    move_speed_units_per_second: 330
  Necromancer:
    hit_points: 220
    max_mana: 150
    move_speed_units_per_second: 320
  Warrior:
    hit_points: 260
    max_mana: 120
    move_speed_units_per_second: 310
  Ranger:
    hit_points: 220
    max_mana: 140
    move_speed_units_per_second: 325
";
const SUPPORT_MIX_PROBE_TEST_CONFIG_YAML: &str = r"lobby:
  launch_countdown_seconds: 1

match:
  total_rounds: 1
  skill_pick_seconds: 5
  pre_combat_seconds: 1

maps:
  tile_units: 50
  objective_target_ms_by_map:
    prototype_arena: 30000
    template_arena: 30000
    training_arena: 30000
  generation:
    max_generation_attempts: 1
    protected_tile_buffer_radius_tiles: 0
    obstacle_edge_padding_tiles: 0
    wall_segment_lengths_tiles: [2, 3]
    long_wall_percent: 0
    wall_candidate_skip_percent: 100
    wall_min_spacing_manhattan_tiles: 0
    pillar_candidate_skip_percent: 100
    pillar_min_spacing_manhattan_tiles: 0
    styles:
      - shrub_clusters: 0
        shrub_radius_tiles: 0
        shrub_soft_radius_tiles: 0
        shrub_fill_percent: 0
        wall_segments: 0
        isolated_pillars: 0

simulation:
  combat_frame_ms: 100
  player_radius_units: 28
  vision_radius_units: 2000
  spawn_spacing_units: 80
  default_aim_x_units: 120
  default_aim_y_units: 0
  mana_regen_per_second: 30
  global_projectile_speed_bonus_bps: 0
  teleport_resolution_steps: 48
  movement_audio_step_interval_ms: 240
  movement_audio_radius_units: 520
  stealth_audio_radius_units: 170
  brush_movement_audible_percent: 22
  passive_bonus_caps:
    player_speed_bps: 9000
    projectile_speed_bps: 9000
    cooldown_bps: 9000
    cast_time_bps: 9500
  movement_modifier_caps:
    chill_bps: 8000
    haste_bps: 6000
    status_total_min_bps: -8000
    status_total_max_bps: 6000
    overall_total_min_bps: -8000
    overall_total_max_bps: 9000
    effective_scale_min_bps: 2000
    effective_scale_max_bps: 16000
  crowd_control_diminishing_returns:
    window_ms: 15000
    stages_bps: [10000, 5000, 2500, 0]
  training_dummy:
    base_hit_points: 100
    health_multiplier: 100
    execute_threshold_bps: 500

classes:
  Bard:
    hit_points: 600
    max_mana: 180
    move_speed_units_per_second: 325
  Cleric:
    hit_points: 640
    max_mana: 180
    move_speed_units_per_second: 320
  Druid:
    hit_points: 600
    max_mana: 180
    move_speed_units_per_second: 320
  Mage:
    hit_points: 560
    max_mana: 180
    move_speed_units_per_second: 320
  Rogue:
    hit_points: 220
    max_mana: 140
    move_speed_units_per_second: 330
  Necromancer:
    hit_points: 220
    max_mana: 150
    move_speed_units_per_second: 320
  Warrior:
    hit_points: 260
    max_mana: 120
    move_speed_units_per_second: 310
  Ranger:
    hit_points: 220
    max_mana: 140
    move_speed_units_per_second: 325
";
const PROBE_TEST_TEMPLATE_ARENA: &str = r"#############
#A.........B#
#...........#
#....XXX....#
#....XXX....#
#....XXX....#
#...........#
#...........#
#############
";
const RELEASE_PROBE_TEST_TEMPLATE_ARENA: &str = r"#############
#A.........B#
#...........#
#...........#
#.....X.....#
#...........#
#...........#
#...........#
#############
";
const PERIODIC_TEST_CLERIC_YAML: &str = r"tree: Cleric
melee:
  id: cleric_probe_hammer
  name: Probe Hammer
  description: Stable melee for periodic probe integration tests.
  cooldown_ms: 550
  range: 88
  radius: 40
  effect: melee_swing
  payload:
    kind: damage
    amount: 8
skills:
  - tier: 1
    id: cleric_probe_holding_light
    name: Holding Light
    description: Self-heal anchor for periodic probe integration tests.
    behavior:
      kind: nova
      effect: nova
      cooldown_ms: 400
      cast_time_ms: 120
      mana_cost: 8
      radius: 24
      payload:
        kind: heal
        amount: 20
  - tier: 2
    id: cleric_probe_filler_two
    name: Probe Filler Two
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      player_speed_bps: 500
  - tier: 3
    id: cleric_probe_filler_three
    name: Probe Filler Three
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      cast_time_bps: 500
  - tier: 4
    id: cleric_probe_filler_four
    name: Probe Filler Four
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      cooldown_bps: 500
  - tier: 5
    id: cleric_probe_filler_five
    name: Probe Filler Five
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      projectile_speed_bps: 500
";
const PERIODIC_TEST_ROGUE_YAML: &str = r"tree: Rogue
melee:
  id: rogue_probe_knife
  name: Probe Knife
  description: Stable melee for periodic probe integration tests.
  cooldown_ms: 500
  range: 86
  radius: 38
  effect: melee_swing
  payload:
    kind: damage
    amount: 8
skills:
  - tier: 1
    id: rogue_probe_poison_dart
    name: Probe Poison Dart
    description: Reliable poison projectile for periodic probe integration tests.
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 320
      mana_cost: 6
      speed: 420
      range: 1000
      radius: 16
      payload:
        kind: damage
        amount: 4
        status:
          kind: poison
          duration_ms: 5000
          tick_interval_ms: 1000
          magnitude: 2
          max_stacks: 5
  - tier: 2
    id: rogue_probe_filler_two
    name: Probe Filler Two
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      player_speed_bps: 500
  - tier: 3
    id: rogue_probe_filler_three
    name: Probe Filler Three
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      cast_time_bps: 500
  - tier: 4
    id: rogue_probe_filler_four
    name: Probe Filler Four
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      cooldown_bps: 500
  - tier: 5
    id: rogue_probe_filler_five
    name: Probe Filler Five
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      projectile_speed_bps: 500
";
const PERIODIC_TEST_NECROMANCER_YAML: &str = r"tree: Necromancer
melee:
  id: necromancer_probe_knife
  name: Probe Knife
  description: Stable melee for periodic probe integration tests.
  cooldown_ms: 540
  range: 82
  radius: 36
  effect: melee_swing
  payload:
    kind: damage
    amount: 8
skills:
  - tier: 1
    id: necromancer_probe_grave_dart
    name: Probe Grave Dart
    description: Reliable poison projectile for periodic probe integration tests.
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 340
      mana_cost: 6
      speed: 400
      range: 1000
      radius: 16
      payload:
        kind: damage
        amount: 3
        status:
          kind: poison
          duration_ms: 5000
          tick_interval_ms: 1000
          magnitude: 2
          max_stacks: 5
  - tier: 2
    id: necromancer_probe_filler_two
    name: Probe Filler Two
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      player_speed_bps: 500
  - tier: 3
    id: necromancer_probe_filler_three
    name: Probe Filler Three
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      cast_time_bps: 500
  - tier: 4
    id: necromancer_probe_filler_four
    name: Probe Filler Four
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      cooldown_bps: 500
  - tier: 5
    id: necromancer_probe_filler_five
    name: Probe Filler Five
    description: Unused filler for periodic probe tests.
    behavior:
      kind: passive
      effect: nova
      projectile_speed_bps: 500
";
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
      cooldown_ms: 350
      cast_time_ms: 120
      mana_cost: 8
      radius: 24
      payload:
        kind: heal
        amount: 8
        dispel:
          scope: negative
          max_statuses: 2
  - tier: 2
    id: cleric_probe_filler_two
    name: Probe Filler Two
    description: Unused filler for dispel probe tests.
    behavior:
      kind: passive
      effect: nova
      player_speed_bps: 500
  - tier: 3
    id: cleric_probe_filler_three
    name: Probe Filler Three
    description: Unused filler for dispel probe tests.
    behavior:
      kind: passive
      effect: nova
      cast_time_bps: 500
  - tier: 4
    id: cleric_probe_filler_four
    name: Probe Filler Four
    description: Unused filler for dispel probe tests.
    behavior:
      kind: passive
      effect: nova
      cooldown_bps: 500
  - tier: 5
    id: cleric_probe_filler_five
    name: Probe Filler Five
    description: Unused filler for dispel probe tests.
    behavior:
      kind: passive
      effect: nova
      projectile_speed_bps: 500
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
    id: mage_probe_taint_bolt
    name: Probe Taint Bolt
    description: Early poison application for dispel probe tests.
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 320
      mana_cost: 6
      speed: 420
      range: 1000
      radius: 16
      payload:
        kind: damage
        amount: 3
        status:
          kind: poison
          duration_ms: 5000
          tick_interval_ms: 1000
          magnitude: 2
          max_stacks: 5
  - tier: 2
    id: mage_probe_filler_two
    name: Probe Filler Two
    description: Unused filler for dispel probe tests.
    behavior:
      kind: passive
      effect: nova
      player_speed_bps: 500
  - tier: 3
    id: mage_probe_filler_three
    name: Probe Filler Three
    description: Unused filler for dispel probe tests.
    behavior:
      kind: passive
      effect: nova
      cast_time_bps: 500
  - tier: 4
    id: mage_probe_filler_four
    name: Probe Filler Four
    description: Unused filler for dispel probe tests.
    behavior:
      kind: passive
      effect: nova
      cooldown_bps: 500
  - tier: 5
    id: mage_probe_filler_five
    name: Probe Filler Five
    description: Unused filler for dispel probe tests.
    behavior:
      kind: passive
      effect: nova
      projectile_speed_bps: 500
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

fn write_content_override(root: &Path, relative_path: &str, contents: &str) {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("content override directory should exist");
    }
    fs::write(path, contents).expect("content override should write");
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
    write_content_override(&root, "config/configurations.yaml", PROBE_TEST_CONFIG_YAML);
    write_content_override(&root, "maps/template_arena.txt", PROBE_TEST_TEMPLATE_ARENA);
    let keep: Vec<&str> = skill_files
        .iter()
        .map(|(file_name, _)| *file_name)
        .collect();
    prune_skill_files(&root, &keep);
    for (file_name, yaml) in skill_files {
        write_skill_override(&root, file_name, yaml);
    }
    root
}

fn shipped_probe_content_root_with_overrides(
    prefix: &str,
    keep: &[&str],
    config_yaml: &str,
    arena_text: &str,
) -> PathBuf {
    let root = temp_content_root(prefix);
    remove_dir_if_exists(&root);
    copy_dir_all(&repo_content_root(), &root);
    write_content_override(&root, "config/configurations.yaml", config_yaml);
    write_content_override(&root, "maps/template_arena.txt", arena_text);
    prune_skill_files(&root, keep);
    root
}

fn shipped_probe_content_root(prefix: &str, keep: &[&str]) -> PathBuf {
    shipped_probe_content_root_with_overrides(
        prefix,
        keep,
        PROBE_TEST_CONFIG_YAML,
        RELEASE_PROBE_TEST_TEMPLATE_ARENA,
    )
}

fn support_mix_probe_content_root(prefix: &str, keep: &[&str]) -> PathBuf {
    shipped_probe_content_root_with_overrides(
        prefix,
        keep,
        SUPPORT_MIX_PROBE_TEST_CONFIG_YAML,
        RELEASE_PROBE_TEST_TEMPLATE_ARENA,
    )
}

fn periodic_probe_content_root() -> PathBuf {
    mechanic_probe_content_root(
        "probe-periodic",
        &[
            ("cleric.yaml", PERIODIC_TEST_CLERIC_YAML),
            ("rogue.yaml", PERIODIC_TEST_ROGUE_YAML),
            ("necromancer.yaml", PERIODIC_TEST_NECROMANCER_YAML),
            (
                "warrior.yaml",
                include_str!("../../../content/skills/warrior.yaml"),
            ),
        ],
    )
}

fn dispel_probe_content_root() -> PathBuf {
    mechanic_probe_content_root(
        "probe-dispel",
        &[
            ("cleric.yaml", DISPEL_TEST_CLERIC_YAML),
            ("mage.yaml", DISPEL_TEST_MAGE_YAML),
            (
                "warrior.yaml",
                include_str!("../../../content/skills/warrior.yaml"),
            ),
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

async fn run_probe_with_fresh_server(
    config: ProbeConfig,
) -> crate::ProbeResult<crate::ProbeOutcome> {
    let content_root = config
        .content_root
        .clone()
        .unwrap_or_else(repo_content_root);
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
        content_root: Some(periodic_probe_content_root()),
        max_games: Some(1),
        connect_timeout: Duration::from_secs(15),
        stage_timeout: Duration::from_secs(20),
        round_timeout: Duration::from_secs(30),
        match_timeout: Duration::from_secs(45),
        input_cadence: Duration::from_millis(60),
        players_per_match: 3,
        preferred_tree_order: Some(vec![
            String::from("Cleric"),
            String::from("Rogue"),
            String::from("Necromancer"),
        ]),
        max_rounds_per_match: Some(1),
        max_combat_loops_per_round: Some(120),
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
        round_timeout: Duration::from_secs(25),
        match_timeout: Duration::from_secs(40),
        input_cadence: Duration::from_millis(60),
        players_per_match: 2,
        preferred_tree_order: Some(vec![String::from("Cleric"), String::from("Mage")]),
        max_rounds_per_match: Some(1),
        max_combat_loops_per_round: Some(120),
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
    let content_root = shipped_probe_content_root(
        "probe-release-smoke",
        &["warrior.yaml", "rogue.yaml", "mage.yaml", "ranger.yaml"],
    );
    let (server, base_url) = start_server_fast_with_content_root(content_root.clone()).await;
    let output_path = temp_path("probe-log", "jsonl");
    let outcome = run_probe(ProbeConfig {
        origin: base_url,
        output_path: output_path.clone(),
        content_root: Some(content_root),
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
    assert!(fs::metadata(output_path).is_ok());

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_probe_verifies_mixed_bard_cleric_druid_mage_round_against_the_dev_server() {
    if skip_live_probe_tests() {
        return;
    }
    let _guard = live_probe_test_mutex().lock().await;
    let content_root = support_mix_probe_content_root(
        "probe-release-support-mix",
        &[
            "bard.yaml",
            "cleric.yaml",
            "druid.yaml",
            "mage.yaml",
            "warrior.yaml",
        ],
    );
    let (server, base_url) = start_server_fast_with_content_root(content_root.clone()).await;
    let output_path = temp_path("probe-support-mix-log", "jsonl");
    let outcome = run_probe(ProbeConfig {
        origin: base_url,
        output_path: output_path.clone(),
        content_root: Some(content_root),
        max_games: Some(1),
        connect_timeout: Duration::from_secs(30),
        stage_timeout: Duration::from_secs(45),
        round_timeout: Duration::from_secs(90),
        match_timeout: Duration::from_secs(180),
        input_cadence: Duration::from_millis(100),
        players_per_match: 4,
        preferred_tree_order: Some(vec![
            String::from("Bard"),
            String::from("Cleric"),
            String::from("Druid"),
            String::from("Mage"),
        ]),
        max_rounds_per_match: Some(1),
        max_combat_loops_per_round: None,
        required_mechanics: None,
    })
    .await
    .expect("probe should verify the first mixed support/damage round");

    assert_eq!(outcome.matches_completed, 1);
    assert_eq!(outcome.covered_skills, 4);
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
