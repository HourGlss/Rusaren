use super::*;
use game_domain::{SkillChoice, SkillTree};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const TEST_TILE_UNITS: u16 = 50;

#[test]
fn bundled_content_loads_all_classes_and_the_ascii_map() {
    let content = GameContent::bundled().expect("bundled content should load");
    for tree_name in ["Paladin", "Ranger", "Bard", "Druid", "Necromancer"] {
        let tree = SkillTree::new(tree_name).expect("authored tree should parse");
        let tier_one = content
            .skills()
            .resolve(&SkillChoice::new(tree.clone(), 1).expect("choice"))
            .unwrap_or_else(|| panic!("{tree_name} tier one should exist"));
        assert_eq!(tier_one.tree, tree);
        assert!(content.skills().melee_for(&tree).is_some());
    }

    let mage_one = content
        .skills()
        .resolve(&SkillChoice::new(SkillTree::Mage, 1).expect("choice"))
        .expect("mage tier one should exist");
    assert!(matches!(
        mage_one.behavior,
        SkillBehavior::Projectile { .. }
    ));
    assert!(content.skills().melee_for(&SkillTree::Warrior).is_some());
    assert_eq!(content.map().map_id, "prototype_arena");
    assert_eq!(content.map().objective_target_ms, 180_000);
    assert!(!content.map().obstacles.is_empty());
    assert_eq!(
        content.training_map().map(|map| map.map_id.as_str()),
        Some("training_arena")
    );
    let training_map = content
        .training_map()
        .expect("bundled content should include the authored training map");
    assert!(!training_map.footprint_mask.is_empty());
    assert!(training_map
        .features
        .iter()
        .any(|feature| matches!(feature.kind, ArenaMapFeatureKind::TrainingDummyResetFull)));
    assert!(training_map
        .features
        .iter()
        .any(|feature| matches!(feature.kind, ArenaMapFeatureKind::TrainingDummyExecute)));
    assert!(content
        .mechanics()
        .behaviors
        .iter()
        .any(|mechanic| mechanic.id == "summon" && mechanic.implemented));
    assert!(content
        .mechanics()
        .behaviors
        .iter()
        .any(|mechanic| mechanic.id == "passive" && mechanic.implemented));
    assert_eq!(content.map().team_a_anchors.len(), 1);
    assert_eq!(content.map().team_b_anchors.len(), 1);
    assert!(
        content.map().team_a_anchors[0].0 < content.map().team_b_anchors[0].0,
        "team A should remain left of team B in the bundled map"
    );
    assert_eq!(
        content.map().team_a_anchors[0].1,
        content.map().team_b_anchors[0].1,
        "the bundled anchors should remain on the same horizontal lane"
    );
    let half_width = i32::from(content.map().width_units) / 2;
    assert!(
        i32::from(content.map().team_a_anchors[0].0).abs() < half_width,
        "team A anchor should stay inside the authored map bounds"
    );
    assert!(
        i32::from(content.map().team_b_anchors[0].0).abs() < half_width,
        "team B anchor should stay inside the authored map bounds"
    );
}

#[test]
fn bundled_content_exposes_authored_cast_times_and_registry_surface() {
    let content = GameContent::bundled().expect("bundled content should load");
    let cleric_minor_heal = content
        .skills()
        .resolve(&SkillChoice::new(SkillTree::Cleric, 1).expect("choice"))
        .expect("cleric tier one should exist");
    assert_eq!(cleric_minor_heal.behavior.cast_time_ms(), 250);

    let druid_dreamseed = content
        .skills()
        .resolve(&SkillChoice::new(SkillTree::new("Druid").expect("tree"), 5).expect("choice"))
        .expect("druid tier five should exist");
    assert_eq!(druid_dreamseed.behavior.cast_time_ms(), 550);

    assert!(content
        .mechanics()
        .behaviors
        .iter()
        .any(|mechanic| mechanic.id == "interrupt" && mechanic.implemented));
    assert!(content
        .mechanics()
        .behaviors
        .iter()
        .any(|mechanic| mechanic.id == "dispel" && mechanic.implemented));
    assert!(content
        .mechanics()
        .statuses
        .iter()
        .any(|mechanic| mechanic.id == "sleep" && mechanic.implemented));
    assert!(content
        .mechanics()
        .statuses
        .iter()
        .any(|mechanic| mechanic.id == "shield" && mechanic.implemented));
}

#[test]
fn bundled_content_covers_the_implemented_registry_surface() {
    let content = GameContent::bundled().expect("bundled content should load");
    let coverage = collect_authored_registry_surface(&content);

    for mechanic in content
        .mechanics()
        .behaviors
        .iter()
        .filter(|mechanic| mechanic.implemented)
    {
        assert!(
            coverage.behaviors.contains(mechanic.id.as_str()),
            "implemented behavior {} should appear in authored skills",
            mechanic.id
        );
    }
    for mechanic in content
        .mechanics()
        .statuses
        .iter()
        .filter(|mechanic| mechanic.implemented)
    {
        assert!(
            coverage.statuses.contains(mechanic.id.as_str()),
            "implemented status {} should appear in authored skills",
            mechanic.id
        );
    }

    assert!(coverage.passive_fields.contains("player_speed_bps"));
    assert!(coverage.passive_fields.contains("projectile_speed_bps"));
    assert!(coverage.passive_fields.contains("cooldown_bps"));
    assert!(coverage.passive_fields.contains("cast_time_bps"));
}

struct AuthoredRegistryCoverage {
    behaviors: BTreeSet<&'static str>,
    statuses: BTreeSet<&'static str>,
    passive_fields: BTreeSet<&'static str>,
}

fn collect_authored_registry_surface(content: &GameContent) -> AuthoredRegistryCoverage {
    let mut coverage = AuthoredRegistryCoverage {
        behaviors: BTreeSet::new(),
        statuses: BTreeSet::new(),
        passive_fields: BTreeSet::new(),
    };

    for skill in content.skills().all() {
        collect_behavior_registry_coverage(&skill.behavior, &mut coverage);
    }

    coverage
}

#[allow(clippy::too_many_lines)]
fn collect_behavior_registry_coverage(
    behavior: &SkillBehavior,
    coverage: &mut AuthoredRegistryCoverage,
) {
    match behavior {
        SkillBehavior::Projectile { payload, .. } => {
            coverage.behaviors.insert("projectile");
            collect_payload_registry_coverage(
                payload,
                &mut coverage.behaviors,
                &mut coverage.statuses,
            );
        }
        SkillBehavior::Beam { payload, .. } => {
            coverage.behaviors.insert("beam");
            collect_payload_registry_coverage(
                payload,
                &mut coverage.behaviors,
                &mut coverage.statuses,
            );
        }
        SkillBehavior::Dash { payload, .. } => {
            coverage.behaviors.insert("dash");
            if let Some(payload) = payload {
                collect_payload_registry_coverage(
                    payload,
                    &mut coverage.behaviors,
                    &mut coverage.statuses,
                );
            }
        }
        SkillBehavior::Burst { payload, .. } => {
            coverage.behaviors.insert("burst");
            collect_payload_registry_coverage(
                payload,
                &mut coverage.behaviors,
                &mut coverage.statuses,
            );
        }
        SkillBehavior::Nova { payload, .. } => {
            coverage.behaviors.insert("nova");
            collect_payload_registry_coverage(
                payload,
                &mut coverage.behaviors,
                &mut coverage.statuses,
            );
        }
        SkillBehavior::Teleport { .. } => {
            coverage.behaviors.insert("teleport");
        }
        SkillBehavior::Channel { payload, .. } => {
            coverage.behaviors.insert("channel");
            collect_payload_registry_coverage(
                payload,
                &mut coverage.behaviors,
                &mut coverage.statuses,
            );
        }
        SkillBehavior::Passive {
            player_speed_bps,
            projectile_speed_bps,
            cooldown_bps,
            cast_time_bps,
            proc_reset,
        } => {
            coverage.behaviors.insert("passive");
            if *player_speed_bps > 0 {
                coverage.passive_fields.insert("player_speed_bps");
            }
            if *projectile_speed_bps > 0 {
                coverage.passive_fields.insert("projectile_speed_bps");
            }
            if *cooldown_bps > 0 {
                coverage.passive_fields.insert("cooldown_bps");
            }
            if *cast_time_bps > 0 {
                coverage.passive_fields.insert("cast_time_bps");
            }
            if proc_reset.is_some() {
                coverage.behaviors.insert("proc_reset");
            }
        }
        SkillBehavior::Summon { payload, .. } => {
            coverage.behaviors.insert("summon");
            collect_payload_registry_coverage(
                payload,
                &mut coverage.behaviors,
                &mut coverage.statuses,
            );
        }
        SkillBehavior::Ward { .. } => {
            coverage.behaviors.insert("ward");
        }
        SkillBehavior::Trap { payload, .. } => {
            coverage.behaviors.insert("trap");
            collect_payload_registry_coverage(
                payload,
                &mut coverage.behaviors,
                &mut coverage.statuses,
            );
        }
        SkillBehavior::Barrier { .. } => {
            coverage.behaviors.insert("barrier");
        }
        SkillBehavior::Aura { payload, .. } => {
            coverage.behaviors.insert("aura");
            collect_payload_registry_coverage(
                payload,
                &mut coverage.behaviors,
                &mut coverage.statuses,
            );
        }
    }
}

fn collect_payload_registry_coverage(
    payload: &EffectPayload,
    behaviors: &mut BTreeSet<&'static str>,
    statuses: &mut BTreeSet<&'static str>,
) {
    if payload.interrupt_silence_duration_ms.is_some() {
        behaviors.insert("interrupt");
    }
    if payload.dispel.is_some() {
        behaviors.insert("dispel");
    }
    if payload.has_amount_range() {
        behaviors.insert("damage_range");
    }
    if payload.can_crit() {
        behaviors.insert("critical_strike");
    }
    if let Some(status) = &payload.status {
        statuses.insert(match status.kind {
            StatusKind::Poison => "poison",
            StatusKind::Hot => "hot",
            StatusKind::Chill => "chill",
            StatusKind::Root => "root",
            StatusKind::Haste => "haste",
            StatusKind::Silence => "silence",
            StatusKind::Stun => "stun",
            StatusKind::Sleep => "sleep",
            StatusKind::Shield => "shield",
            StatusKind::Stealth => "stealth",
            StatusKind::Reveal => "reveal",
            StatusKind::Fear => "fear",
            StatusKind::HealingReduction => "healing_reduction",
        });
    }
}

#[test]
fn content_error_display_variants_are_precise() {
    assert_eq!(
        ContentError::Io {
            path: PathBuf::from("content/skills/mage.yaml"),
            message: String::from("permission denied"),
        }
        .to_string(),
        "failed to read content file content/skills/mage.yaml: permission denied"
    );
    assert_eq!(
        ContentError::Parse {
            source: String::from("skills/mage.yaml"),
            message: String::from("invalid yaml"),
        }
        .to_string(),
        "failed to parse skills/mage.yaml: invalid yaml"
    );
    assert_eq!(
        ContentError::Validation {
            source: String::from("skills/mage.yaml"),
            message: String::from("tier 1 is duplicated"),
        }
        .to_string(),
        "invalid content in skills/mage.yaml: tier 1 is duplicated"
    );
}

#[test]
fn read_skill_file_pairs_requires_existing_skill_yaml_files() {
    let root = temp_content_root("skill-file-pairs");
    let missing = read_skill_file_pairs(&root).expect_err("missing skills dir should fail");
    assert!(matches!(missing, ContentError::Io { .. }));

    let skills_dir = root.join("skills");
    fs::create_dir_all(&skills_dir).expect("skills dir");
    let empty = read_skill_file_pairs(&root).expect_err("empty skills dir should fail");
    assert!(matches!(
        empty,
        ContentError::Validation { message, .. }
            if message == "no skill YAML files were found"
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn parse_skill_yaml_rejects_unknown_trees_duplicate_tiers_and_invalid_field_shapes() {
    let unknown_tree = r"
tree: Chronomancer
melee:
  id: chrono_claw
  name: Claw
  description: nope
  cooldown_ms: 100
  range: 50
  radius: 20
  effect: melee_swing
  payload:
    kind: damage
    amount: 10
skills:
  - tier: 1
    id: chrono_sprout
    name: Sprout
    description: nope
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 100
      speed: 100
      range: 100
      radius: 10
      payload:
        kind: damage
        amount: 1
";
    assert!(matches!(
        parse_skill_yaml("skills/druid.yaml", unknown_tree),
        Err(ContentError::Validation { .. })
    ));

    let duplicate_tier = r"
tree: Mage
melee:
  id: mage_staff
  name: Staff
  description: bonk
  cooldown_ms: 100
  range: 50
  radius: 20
  effect: melee_swing
  payload:
    kind: damage
    amount: 10
skills:
  - tier: 1
    id: mage_a
    name: A
    description: A
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 100
      speed: 100
      range: 10
      radius: 10
      payload:
        kind: damage
        amount: 1
  - tier: 1
    id: mage_b
    name: B
    description: B
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
";
    assert!(matches!(
        parse_skill_yaml("skills/mage.yaml", duplicate_tier),
        Err(ContentError::Validation { .. })
    ));

    let invalid_dash_shape = r"
tree: Rogue
melee:
  id: rogue_blade
  name: Blade
  description: blade
  cooldown_ms: 100
  range: 50
  radius: 20
  effect: melee_swing
  payload:
    kind: damage
    amount: 10
skills:
  - tier: 1
    id: rogue_dash
    name: Dash
    description: dash
    behavior:
      kind: dash
      effect: dash_trail
      cooldown_ms: 100
      distance: 120
      range: 40
  - tier: 2
    id: rogue_two
    name: Two
    description: Two
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 3
    id: rogue_three
    name: Three
    description: Three
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 4
    id: rogue_four
    name: Four
    description: Four
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 5
    id: rogue_five
    name: Five
    description: Five
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
";
    assert!(matches!(
        parse_skill_yaml("skills/rogue.yaml", invalid_dash_shape),
        Err(ContentError::Validation { .. })
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn parse_skill_yaml_accepts_new_behaviors_and_rejects_unknown_effects_and_invalid_status_rules() {
    let summon = r"
tree: Mage
melee:
  id: mage_staff
  name: Staff
  description: bonk
  cooldown_ms: 100
  range: 50
  radius: 20
  effect: melee_swing
  payload:
    kind: damage
    amount: 10
skills:
  - tier: 1
    id: mage_summon
    name: Summon
    description: summons
    behavior:
      kind: summon
      effect: skill_shot
      cooldown_ms: 1000
      mana_cost: 20
      distance: 120
      radius: 24
      duration_ms: 4000
      hit_points: 50
      range: 180
      tick_interval_ms: 1000
      payload:
        kind: damage
        amount: 4
  - tier: 2
    id: mage_two
    name: Two
    description: Two
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 3
    id: mage_three
    name: Three
    description: Three
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 4
    id: mage_four
    name: Four
    description: Four
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 5
    id: mage_five
    name: Five
    description: Five
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
";
    let parsed =
        parse_skill_yaml("skills/mage.yaml", summon).expect("summon behavior should parse");
    assert!(matches!(
        parsed.skills[0].behavior,
        SkillBehavior::Summon {
            duration_ms: 4000,
            hit_points: 50,
            range: 180,
            tick_interval_ms: 1000,
            ..
        }
    ));

    let unknown_effect = summon.replace("kind: summon", "kind: projectile");
    let unknown_effect = unknown_effect.replace("effect: skill_shot", "effect: mystery");
    assert!(matches!(
        parse_skill_yaml("skills/mage.yaml", unknown_effect.as_str()),
        Err(ContentError::Validation { message, .. })
            if message == "unknown effect kind 'mystery'"
    ));

    let invalid_hot = r"
tree: Cleric
melee:
  id: cleric_mace
  name: Mace
  description: bonk
  cooldown_ms: 100
  range: 50
  radius: 20
  effect: melee_swing
  payload:
    kind: damage
    amount: 10
skills:
  - tier: 1
    id: cleric_hot
    name: Hot
    description: hot
    behavior:
      kind: nova
      effect: nova
      cooldown_ms: 1200
      radius: 120
      payload:
        kind: heal
        amount: 0
        status:
          kind: hot
          duration_ms: 3000
          tick_interval_ms: 1000
          magnitude: 4
          max_stacks: 0
  - tier: 2
    id: cleric_two
    name: Two
    description: Two
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 3
    id: cleric_three
    name: Three
    description: Three
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 4
    id: cleric_four
    name: Four
    description: Four
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 5
    id: cleric_five
    name: Five
    description: Five
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
";
    assert!(matches!(
        parse_skill_yaml("skills/cleric.yaml", invalid_hot),
        Err(ContentError::Validation { message, .. })
            if message == "status 'Hot' max_stacks must be greater than zero"
    ));

    let invalid_root = r"
tree: Cleric
melee:
  id: cleric_mace
  name: Mace
  description: bonk
  cooldown_ms: 100
  range: 50
  radius: 20
  effect: melee_swing
  payload:
    kind: damage
    amount: 10
skills:
  - tier: 1
    id: cleric_root
    name: Root
    description: root
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 1200
      speed: 180
      range: 220
      radius: 14
      payload:
        kind: damage
        amount: 5
        status:
          kind: root
          duration_ms: 3000
          magnitude: 1
          max_stacks: 1
  - tier: 2
    id: cleric_two
    name: Two
    description: Two
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 3
    id: cleric_three
    name: Three
    description: Three
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 4
    id: cleric_four
    name: Four
    description: Four
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 5
    id: cleric_five
    name: Five
    description: Five
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
";
    assert!(matches!(
        parse_skill_yaml("skills/cleric.yaml", invalid_root),
        Err(ContentError::Validation { message, .. })
            if message == "status.magnitude must be zero for this mechanic"
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn parse_skill_yaml_accepts_status_payloads_and_melee_definitions() {
    let yaml = r"
tree: Cleric
melee:
  id: cleric_mace
  name: Mace
  description: bonk
  cooldown_ms: 550
  range: 80
  radius: 30
  effect: melee_swing
  payload:
    kind: damage
    amount: 12
skills:
  - tier: 1
    id: cleric_minor_heal
    name: Minor Heal
    description: heal
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 900
      range: 250
      radius: 20
      payload:
        kind: heal
        amount: 14
  - tier: 2
    id: cleric_flash
    name: Flash
    description: flash
    behavior:
      kind: dash
      effect: dash_trail
      cooldown_ms: 1200
      distance: 120
  - tier: 3
    id: cleric_hot
    name: Hot
    description: hot
    behavior:
      kind: nova
      effect: nova
      cooldown_ms: 1200
      radius: 120
      payload:
        kind: heal
        amount: 0
        status:
          kind: hot
          duration_ms: 3000
          tick_interval_ms: 1000
          magnitude: 4
          max_stacks: 1
  - tier: 4
    id: cleric_burst
    name: Burst
    description: burst
    behavior:
      kind: burst
      effect: burst
      cooldown_ms: 1400
      range: 200
      radius: 80
      payload:
        kind: damage
        amount: 10
  - tier: 5
    id: cleric_root
    name: Root
    description: root
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 1600
      speed: 100
      range: 300
      radius: 12
      payload:
        kind: damage
        amount: 5
        status:
          kind: root
          duration_ms: 500
          magnitude: 0
";
    let parsed = parse_skill_yaml("skills/cleric.yaml", yaml).expect("yaml should parse");
    assert_eq!(parsed.melee.cooldown_ms, 550);
    assert!(matches!(
        parsed.skills[2].behavior,
        SkillBehavior::Nova {
            mana_cost: 0,
            payload: EffectPayload {
                kind: CombatValueKind::Heal,
                status: Some(StatusDefinition {
                    kind: StatusKind::Hot,
                    ..
                }),
                ..
            },
            ..
        }
    ));
    assert_eq!(parsed.skills[0].behavior.mana_cost(), 0);
}

const TOGGLEABLE_AURA_SKILL_YAML: &str = r"
tree: Rogue
melee:
  id: rogue_dual_cut
  name: Dual Cut
  description: quick slash
  cooldown_ms: 450
  range: 86
  radius: 38
  effect: melee_swing
  payload:
    kind: damage
    amount: 22
skills:
  - tier: 1
    id: rogue_venom_shiv
    name: Venom Shiv
    description: projectile opener
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 650
      mana_cost: 14
      speed: 360
      range: 1500
      radius: 14
      payload:
        kind: damage
        amount: 10
  - tier: 2
    id: rogue_lullwire_trap
    name: Lullwire Trap
    description: trap
    behavior:
      kind: trap
      effect: burst
      cooldown_ms: 2200
      mana_cost: 24
      distance: 220
      radius: 42
      duration_ms: 6000
      hit_points: 40
      payload:
        kind: damage
        amount: 6
  - tier: 3
    id: rogue_veil_step
    name: Veil Step
    description: teleport
    behavior:
      kind: teleport
      effect: dash_trail
      cooldown_ms: 1500
      mana_cost: 20
      distance: 240
  - tier: 4
    id: rogue_nightcloak
    name: Nightcloak
    description: stealth toggle
    behavior:
      kind: aura
      effect: nova
      cooldown_ms: 2400
      mana_cost: 10
      toggleable: true
      radius: 12
      duration_ms: 30000
      tick_interval_ms: 1000
      cast_start_payload:
        kind: heal
        amount: 0
        status:
          kind: stealth
          duration_ms: 1200
          magnitude: 0
      cast_end_payload:
        kind: heal
        amount: 0
        status:
          kind: haste
          duration_ms: 1500
          magnitude: 1200
      payload:
        kind: heal
        amount: 0
        status:
          kind: stealth
          duration_ms: 1200
          magnitude: 0
  - tier: 5
    id: rogue_assassins_tempo
    name: Assassin's Tempo
    description: passive speed
    behavior:
      kind: passive
      effect: nova
      player_speed_bps: 1600
";

#[test]
fn parse_skill_yaml_accepts_aura_cast_start_and_end_payloads() {
    let parsed = parse_skill_yaml("skills/rogue.yaml", TOGGLEABLE_AURA_SKILL_YAML)
        .expect("yaml should parse");
    assert!(matches!(
        parsed.skills[3].behavior,
        SkillBehavior::Aura {
            toggleable: true,
            cast_start_payload: Some(EffectPayload {
                status: Some(StatusDefinition {
                    kind: StatusKind::Stealth,
                    ..
                }),
                ..
            }),
            cast_end_payload: Some(EffectPayload {
                status: Some(StatusDefinition {
                    kind: StatusKind::Haste,
                    ..
                }),
                ..
            }),
            payload: EffectPayload {
                status: Some(StatusDefinition {
                    kind: StatusKind::Stealth,
                    ..
                }),
                ..
            },
            ..
        }
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn parse_skill_yaml_accepts_payload_crit_range_and_proc_reset_fields() {
    let yaml = r"
tree: Rogue
melee:
  id: rogue_dual_cut
  name: Dual Cut
  description: slash
  cooldown_ms: 450
  range: 86
  radius: 38
  effect: melee_swing
  payload:
    kind: damage
    amount: 22
skills:
  - tier: 1
    id: rogue_spike
    name: Spike
    description: ranged opener
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 650
      mana_cost: 14
      speed: 360
      range: 1500
      radius: 14
      payload:
        kind: damage
        amount_min: 8
        amount_max: 12
        crit_chance_bps: 2500
        crit_multiplier_bps: 17500
  - tier: 2
    id: rogue_trap
    name: Trap
    description: trap
    behavior:
      kind: trap
      effect: burst
      cooldown_ms: 2200
      mana_cost: 24
      distance: 220
      radius: 42
      duration_ms: 6000
      hit_points: 40
      payload:
        kind: damage
        amount: 6
  - tier: 3
    id: rogue_teleport
    name: Teleport
    description: blink
    behavior:
      kind: teleport
      effect: dash_trail
      cooldown_ms: 1500
      mana_cost: 20
      distance: 240
  - tier: 4
    id: rogue_proc_talent
    name: Proc Talent
    description: proc
    behavior:
      kind: passive
      effect: nova
      proc_reset:
        trigger: on_hit
        source_skill_ids:
          - rogue_spike
        reset_skill_ids:
          - rogue_teleport
        instacast_skill_ids:
          - rogue_trap
        instacast_costs_mana: false
        instacast_starts_cooldown: false
        internal_cooldown_ms: 9000
  - tier: 5
    id: rogue_last
    name: Last
    description: filler
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 900
      range: 200
      radius: 16
      payload:
        kind: damage
        amount: 4
";
    let parsed = parse_skill_yaml("skills/rogue.yaml", yaml).expect("yaml should parse");
    assert!(matches!(
        parsed.skills[0].behavior,
        SkillBehavior::Projectile {
            payload: EffectPayload {
                amount: 8,
                amount_max: Some(12),
                crit_chance_bps: 2500,
                crit_multiplier_bps: 17500,
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        parsed.skills[3].behavior,
        SkillBehavior::Passive {
            proc_reset: Some(ProcResetDefinition {
                trigger: ProcTriggerKind::Hit,
                instacast_costs_mana: false,
                instacast_starts_cooldown: false,
                internal_cooldown_ms: Some(9000),
                ..
            }),
            ..
        }
    ));
}

#[test]
fn parse_ascii_map_accepts_ragged_rows_and_rejects_bad_glyphs_and_missing_anchors() {
    let ragged = " A.d\nA..B\n  D B\n";
    let parsed = parse_ascii_map("maps/ragged.txt", ragged, TEST_TILE_UNITS)
        .expect("ragged rows should parse");
    assert_eq!(parsed.width_tiles, 5);
    assert_eq!(parsed.height_tiles, 3);
    assert_eq!(parsed.team_a_anchors.len(), 2);
    assert_eq!(parsed.team_b_anchors.len(), 2);
    assert!(
        !parsed.footprint_mask.is_empty(),
        "ragged maps should still produce a valid footprint mask"
    );
    assert_eq!(parsed.features.len(), 2);
    assert!(parsed
        .features
        .iter()
        .any(|feature| matches!(feature.kind, ArenaMapFeatureKind::TrainingDummyResetFull)));
    assert!(parsed
        .features
        .iter()
        .any(|feature| matches!(feature.kind, ArenaMapFeatureKind::TrainingDummyExecute)));

    let invalid_glyph = "A..\n.@.\n..B\n";
    assert!(matches!(
        parse_ascii_map("maps/invalid.txt", invalid_glyph, TEST_TILE_UNITS),
        Err(ContentError::Validation { .. })
    ));

    let missing_anchor = "...\n.#.\n...\n";
    assert!(matches!(
        parse_ascii_map("maps/missing.txt", missing_anchor, TEST_TILE_UNITS),
        Err(ContentError::Validation { .. })
    ));

    let too_many_anchors = "AAAB\nA..B\n...B\n";
    assert!(matches!(
        parse_ascii_map("maps/too-many.txt", too_many_anchors, TEST_TILE_UNITS),
        Err(ContentError::Validation { .. })
    ));
}

#[test]
fn parse_skill_yaml_accepts_audio_cue_ids_and_rejects_invalid_ones() {
    let valid = r"
tree: Mage
melee:
  id: mage_staff
  name: Staff
  description: bonk
  cooldown_ms: 100
  range: 50
  radius: 20
  effect: melee_swing
  audio_cue_id: melee_staff_bonk
  payload:
    kind: damage
    amount: 10
skills:
  - tier: 1
    id: mage_arc_bolt
    name: Arc Bolt
    description: damage
    audio_cue_id: mage_arc_bolt
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 100
      speed: 100
      range: 100
      radius: 10
      payload:
        kind: damage
        amount: 1
  - tier: 2
    id: mage_two
    name: Two
    description: two
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 3
    id: mage_three
    name: Three
    description: three
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 4
    id: mage_four
    name: Four
    description: four
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 5
    id: mage_five
    name: Five
    description: five
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
";
    let parsed = parse_skill_yaml("skills/mage.yaml", valid).expect("audio cue ids should parse");
    assert_eq!(
        parsed.melee.audio_cue_id.as_deref(),
        Some("melee_staff_bonk")
    );
    assert_eq!(
        parsed.skills[0].audio_cue_id.as_deref(),
        Some("mage_arc_bolt")
    );

    let invalid = valid.replace("mage_arc_bolt", "Mage Arc Bolt");
    assert!(matches!(
        parse_skill_yaml("skills/mage.yaml", invalid.as_str()),
        Err(ContentError::Validation { message, .. })
            if message.contains("audio_cue_id")
    ));
}

#[test]
fn parse_mechanics_yaml_accepts_registry_entries_and_rejects_duplicates() {
    let yaml = r"
behaviors:
  - id: summon
    label: Summon
    implemented: false
    inspiration: WoW pets
    notes: Summons a pet
statuses:
  - id: shield
    label: Shield
    implemented: false
    inspiration: Priest bubbles
    notes: Absorbs damage
";
    let mechanics =
        parse_mechanics_yaml("mechanics/registry.yaml", yaml).expect("mechanics yaml should parse");
    assert_eq!(mechanics.behaviors[0].category, MechanicCategory::Behavior);
    assert_eq!(mechanics.statuses[0].category, MechanicCategory::Status);

    let duplicate = r"
behaviors:
  - id: summon
    label: Summon
    implemented: false
    inspiration: WoW pets
    notes: Summons a pet
  - id: summon
    label: Duplicate
    implemented: false
    inspiration: League summons
    notes: duplicate
statuses:
  - id: shield
    label: Shield
    implemented: false
    inspiration: Priest bubbles
    notes: Absorbs damage
";
    assert!(matches!(
        parse_mechanics_yaml("mechanics/registry.yaml", duplicate),
        Err(ContentError::Validation { .. })
    ));
}

#[test]
fn load_skill_catalog_rejects_duplicate_authored_ids_across_files() {
    let bundled_skill_files = workspace_skill_pairs();
    let duplicate_mage = bundled_skill_files
        .iter()
        .find(|(source, _)| source.ends_with("mage.yaml"))
        .expect("mage yaml should exist")
        .1
        .replace("mage_arc_bolt", "warrior_sweeping_slash");
    let pairs = vec![
        to_pair(&bundled_skill_files, "warrior.yaml"),
        ("skills/mage.yaml", duplicate_mage.as_str()),
        to_pair(&bundled_skill_files, "rogue.yaml"),
        to_pair(&bundled_skill_files, "cleric.yaml"),
    ];
    assert!(matches!(
        load_skill_catalog_from_pairs(&pairs),
        Err(ContentError::Validation { .. })
    ));
}

#[test]
fn load_skill_catalog_requires_at_least_one_class_file() {
    assert!(matches!(
        load_skill_catalog_from_pairs(&[]),
        Err(ContentError::Validation { source, message })
            if source == "skills" && message == "at least one class skill file is required"
    ));
}

#[test]
fn load_from_root_fails_cleanly_for_invalid_yaml_and_map_content() {
    let root = temp_content_root("invalid-content");
    let (skills_dir, maps_dir, mechanics_dir, config_dir) = create_content_root_dirs(&root);

    for (source, yaml) in workspace_skill_pairs() {
        let path = skills_dir.join(
            Path::new(&source)
                .file_name()
                .expect("bundled skill file should have a file name"),
        );
        let text = if source.ends_with("rogue.yaml") {
            yaml.replacen("kind: projectile", "kind: dash", 1)
        } else {
            yaml
        };
        fs::write(path, text).expect("skill file");
    }
    fs::write(maps_dir.join("prototype_arena.txt"), "A..\n..B\n").expect("map file");
    write_workspace_mechanics_registry(&mechanics_dir);
    write_workspace_configuration(&config_dir);

    let error = GameContent::load_from_root(&root).expect_err("invalid content should fail");
    assert!(matches!(error, ContentError::Validation { .. }));
}

#[test]
fn load_from_root_accepts_custom_class_files_without_rust_registry_changes() {
    let root = temp_content_root("custom-class");
    let (skills_dir, maps_dir, mechanics_dir, config_dir) = create_content_root_dirs(&root);
    write_workspace_skill_files(&skills_dir);
    fs::write(skills_dir.join("druid.yaml"), druid_yaml()).expect("custom class file");
    write_workspace_map_file(&maps_dir);
    write_workspace_mechanics_registry(&mechanics_dir);
    write_workspace_configuration(&config_dir);
    append_class_profile_to_workspace_configuration(&config_dir, "Druid", 102, 115, 290);

    let content = GameContent::load_from_root(&root).expect("custom class content should load");
    let druid = SkillTree::new("Druid").expect("custom tree");
    let druid_tier_one = content
        .skills()
        .resolve(&SkillChoice::new(druid.clone(), 1).expect("choice"))
        .expect("druid skill should exist");
    assert_eq!(druid_tier_one.name, "Bramble Shot");
    assert!(content.skills().melee_for(&druid).is_some());
}

fn create_content_root_dirs(root: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let skills_dir = root.join("skills");
    let maps_dir = root.join("maps");
    let mechanics_dir = root.join("mechanics");
    let config_dir = root.join("config");
    fs::create_dir_all(&skills_dir).expect("skills dir");
    fs::create_dir_all(&maps_dir).expect("maps dir");
    fs::create_dir_all(&mechanics_dir).expect("mechanics dir");
    fs::create_dir_all(&config_dir).expect("config dir");
    write_workspace_map_registry(&maps_dir);
    (skills_dir, maps_dir, mechanics_dir, config_dir)
}

fn write_workspace_skill_files(skills_dir: &Path) {
    for (source, yaml) in workspace_skill_pairs() {
        let path = skills_dir.join(
            Path::new(&source)
                .file_name()
                .expect("bundled skill file should have a file name"),
        );
        fs::write(path, yaml).expect("skill file");
    }
}

fn write_workspace_map_file(maps_dir: &Path) {
    for entry in fs::read_dir(workspace_content_root().join("maps")).expect("workspace maps dir") {
        let entry = entry.expect("workspace map entry");
        let path = entry.path();
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
        {
            fs::write(
                maps_dir.join(path.file_name().expect("map file name")),
                fs::read_to_string(&path).expect("workspace map"),
            )
            .expect("map file");
        }
    }
}

fn write_workspace_map_registry(maps_dir: &Path) {
    fs::write(
        maps_dir.join("registry.yaml"),
        fs::read_to_string(workspace_content_root().join("maps").join("registry.yaml"))
            .expect("workspace map registry"),
    )
    .expect("map registry");
}

fn write_workspace_mechanics_registry(mechanics_dir: &Path) {
    fs::write(
        mechanics_dir.join("registry.yaml"),
        fs::read_to_string(
            workspace_content_root()
                .join("mechanics")
                .join("registry.yaml"),
        )
        .expect("workspace mechanics registry"),
    )
    .expect("mechanics registry");
}

fn write_workspace_configuration(config_dir: &Path) {
    fs::write(
        config_dir.join("configurations.yaml"),
        fs::read_to_string(
            workspace_content_root()
                .join("config")
                .join("configurations.yaml"),
        )
        .expect("workspace configuration"),
    )
    .expect("configuration");
}

fn append_class_profile_to_workspace_configuration(
    config_dir: &Path,
    tree_name: &str,
    hit_points: u16,
    max_mana: u16,
    move_speed_units_per_second: u16,
) {
    let path = config_dir.join("configurations.yaml");
    let mut configuration = fs::read_to_string(&path).expect("configuration");
    let _ = write!(
        configuration,
        "\n  {tree_name}:\n    hit_points: {hit_points}\n    max_mana: {max_mana}\n    move_speed_units_per_second: {move_speed_units_per_second}\n"
    );
    fs::write(path, configuration).expect("configuration");
}

fn druid_yaml() -> &'static str {
    r"
tree: Druid
melee:
  id: druid_claw
  name: Claw
  description: A quick beast-form slash.
  cooldown_ms: 500
  range: 84
  radius: 38
  effect: melee_swing
  payload:
    kind: damage
    amount: 11
skills:
  - tier: 1
    id: druid_bramble_shot
    name: Bramble Shot
    description: A thorn projectile.
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 800
      speed: 260
      range: 1200
      radius: 18
      payload:
        kind: damage
        amount: 14
  - tier: 2
    id: druid_feral_step
    name: Feral Step
    description: A short pounce.
    behavior:
      kind: dash
      effect: dash_trail
      cooldown_ms: 1400
      distance: 180
  - tier: 3
    id: druid_bloom
    name: Bloom
    description: A healing pulse.
    behavior:
      kind: nova
      effect: nova
      cooldown_ms: 1800
      radius: 120
      payload:
        kind: heal
        amount: 10
  - tier: 4
    id: druid_root_snare
    name: Root Snare
    description: A targeted root burst.
    behavior:
      kind: burst
      effect: burst
      cooldown_ms: 2200
      range: 220
      radius: 90
      payload:
        kind: damage
        amount: 6
        status:
          kind: root
          duration_ms: 700
          magnitude: 0
  - tier: 5
    id: druid_vine_lash
    name: Vine Lash
    description: A line strike.
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 2600
      range: 240
      radius: 28
      payload:
        kind: damage
        amount: 24
"
}

fn temp_content_root(label: &str) -> PathBuf {
    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("test-temp")
        .join(format!("rarena-{label}-{}-{counter}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).expect("existing temp dir should be removable");
    }
    root
}

fn workspace_skill_pairs() -> Vec<(String, String)> {
    read_skill_file_pairs(&workspace_content_root()).expect("workspace content should load")
}

fn to_pair<'a>(pairs: &'a [(String, String)], suffix: &str) -> (&'a str, &'a str) {
    let (source, yaml) = pairs
        .iter()
        .find(|(source, _)| source.ends_with(suffix))
        .unwrap_or_else(|| panic!("expected skill file ending with {suffix}"));
    (source.as_str(), yaml.as_str())
}
use std::fmt::Write as _;
