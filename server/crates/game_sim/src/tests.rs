use super::*;
use game_content::{ArenaMapObstacleKind, GameContent};
use game_domain::{PlayerName, PlayerRecord, SkillChoice, SkillTree};

const COMBAT_FRAME_MS: u16 = 100;
const PLAYER_MOVE_SPEED_UNITS_PER_SECOND: u16 = 280;
const PLAYER_RADIUS_UNITS: u16 = 28;
const SPAWN_SPACING_UNITS: i16 = 120;
const DEFAULT_AIM_X: i16 = 120;
const DEFAULT_AIM_Y: i16 = 0;
const TEST_ATTACKER_X: i16 = -620;
const TEST_OPEN_LANE_Y: i16 = 0;
const TEST_AIM_X: i16 = 120;
const TEST_AIM_Y: i16 = 0;

fn player_id(raw: u32) -> PlayerId {
    PlayerId::new(raw).expect("valid player id")
}

fn assignment(raw_id: u32, raw_name: &str, team: TeamSide) -> TeamAssignment {
    TeamAssignment {
        player_id: player_id(raw_id),
        player_name: PlayerName::new(raw_name).expect("valid player name"),
        record: PlayerRecord::new(),
        team,
    }
}

fn content() -> GameContent {
    GameContent::bundled().expect("bundled content should load")
}

fn choice(tree: SkillTree, tier: u8) -> SkillChoice {
    SkillChoice::new(tree, tier).expect("valid choice")
}

#[allow(clippy::needless_pass_by_value)]
fn seed(
    content: &GameContent,
    raw_id: u32,
    raw_name: &str,
    team: TeamSide,
    primary_tree: SkillTree,
    choices: [Option<SkillChoice>; 5],
) -> SimPlayerSeed {
    let profile = content
        .class_profile(&primary_tree)
        .expect("class profile should exist");
    SimPlayerSeed {
        assignment: assignment(raw_id, raw_name, team),
        hit_points: profile.hit_points,
        max_mana: profile.max_mana,
        move_speed_units_per_second: profile.move_speed_units_per_second,
        melee: content
            .skills()
            .melee_for(&primary_tree)
            .expect("melee should exist")
            .clone(),
        skills: choices.map(|value| {
            value.and_then(|skill_choice| content.skills().resolve(&skill_choice).cloned())
        }),
    }
}

fn world(content: &GameContent, seeds: Vec<SimPlayerSeed>) -> SimulationWorld {
    SimulationWorld::new(seeds, content.map(), content.configuration().simulation)
        .expect("world should build")
}

fn spawn_position(
    team: TeamSide,
    index: u16,
    map: &game_content::ArenaMapDefinition,
) -> (i16, i16, i16) {
    super::spawn_position(team, index, map, &content().configuration().simulation)
}

#[allow(clippy::too_many_arguments)]
fn resolve_movement(
    start_x: i16,
    start_y: i16,
    desired_x: i32,
    desired_y: i32,
    arena_width_units: u16,
    arena_height_units: u16,
    width_tiles: u16,
    height_tiles: u16,
    tile_units: u16,
    footprint_mask: &[u8],
    obstacles: &[ArenaObstacle],
) -> (i16, i16) {
    super::resolve_movement(
        start_x,
        start_y,
        desired_x,
        desired_y,
        PLAYER_RADIUS_UNITS,
        arena_width_units,
        arena_height_units,
        width_tiles,
        height_tiles,
        tile_units,
        footprint_mask,
        obstacles,
    )
}

#[allow(clippy::needless_pass_by_value)]
fn seed_with_slot_one_skill(
    content: &GameContent,
    raw_id: u32,
    raw_name: &str,
    team: TeamSide,
    tree: SkillTree,
    skill: &SkillDefinition,
) -> SimPlayerSeed {
    let profile = content
        .class_profile(&tree)
        .expect("class profile should exist");
    SimPlayerSeed {
        assignment: assignment(raw_id, raw_name, team),
        hit_points: profile.hit_points,
        max_mana: profile.max_mana,
        move_speed_units_per_second: profile.move_speed_units_per_second,
        melee: content
            .skills()
            .melee_for(&tree)
            .expect("melee should exist")
            .clone(),
        skills: [Some(skill.clone()), None, None, None, None],
    }
}

fn set_player_pose(
    world: &mut SimulationWorld,
    player_id: PlayerId,
    x: i16,
    y: i16,
    aim_x: i16,
    aim_y: i16,
) {
    let player = world
        .players
        .get_mut(&player_id)
        .expect("player should exist");
    player.x = x;
    player.y = y;
    player.aim_x = aim_x;
    player.aim_y = aim_y;
}

fn collect_ticks(world: &mut SimulationWorld, frames: usize) -> Vec<SimulationEvent> {
    let mut events = Vec::new();
    for _ in 0..frames {
        events.extend(world.tick(COMBAT_FRAME_MS));
    }
    events
}

fn status_expiration_frames(duration_ms: u16) -> usize {
    usize::from(duration_ms.div_ceil(COMBAT_FRAME_MS)) + 1
}

fn class_hit_points(content: &GameContent, tree: &SkillTree) -> u16 {
    content
        .class_profile(tree)
        .expect("class profile should exist")
        .hit_points
}

fn dr_scaled_duration_ms(content: &GameContent, base_duration_ms: u16, stage_index: usize) -> u16 {
    let scale_bps = u32::from(
        content
            .configuration()
            .simulation
            .crowd_control_diminishing_returns
            .stages_bps[stage_index],
    );
    let scaled = u32::from(base_duration_ms).saturating_mul(scale_bps);
    let rounded = scaled.div_ceil(10_000);
    u16::try_from(rounded).unwrap_or(u16::MAX)
}

fn projectile_frame_budget(speed: u16, range: u16) -> usize {
    let travel_per_frame = usize::from(travel_distance_units(speed, COMBAT_FRAME_MS).max(1));
    usize::from(range).div_ceil(travel_per_frame) + 3
}

fn cast_resolution_frame_budget(behavior: &SkillBehavior) -> usize {
    1 + usize::from(behavior.cast_time_ms().div_ceil(COMBAT_FRAME_MS))
}

fn activate_skill_cast(
    world: &mut SimulationWorld,
    attacker_id: PlayerId,
    slot: u8,
    behavior: &SkillBehavior,
) -> Vec<SimulationEvent> {
    activate_skill_cast_with_mode(world, attacker_id, slot, behavior, false)
}

fn activate_skill_cast_with_mode(
    world: &mut SimulationWorld,
    attacker_id: PlayerId,
    slot: u8,
    behavior: &SkillBehavior,
    self_target: bool,
) -> Vec<SimulationEvent> {
    world
        .queue_cast_with_mode(attacker_id, slot, self_target)
        .expect("cast should queue successfully");
    let mut events = world.tick(COMBAT_FRAME_MS);
    let extra_cast_frames = cast_resolution_frame_budget(behavior).saturating_sub(1);
    if extra_cast_frames > 0 {
        events.extend(collect_ticks(world, extra_cast_frames));
    }
    events
}

#[allow(clippy::needless_pass_by_value)]
fn resolve_skill_cast(
    world: &mut SimulationWorld,
    attacker_id: PlayerId,
    slot: u8,
    behavior: SkillBehavior,
) -> Vec<SimulationEvent> {
    resolve_skill_cast_with_mode(world, attacker_id, slot, behavior, false)
}

#[allow(clippy::needless_pass_by_value)]
fn resolve_skill_cast_with_mode(
    world: &mut SimulationWorld,
    attacker_id: PlayerId,
    slot: u8,
    behavior: SkillBehavior,
    self_target: bool,
) -> Vec<SimulationEvent> {
    let mut events =
        activate_skill_cast_with_mode(world, attacker_id, slot, &behavior, self_target);
    match behavior {
        SkillBehavior::Projectile { speed, range, .. } => {
            events.extend(collect_ticks(world, projectile_frame_budget(speed, range)));
        }
        SkillBehavior::Channel {
            tick_interval_ms, ..
        } => {
            events.extend(collect_ticks(
                world,
                usize::from(tick_interval_ms / COMBAT_FRAME_MS + 2),
            ));
        }
        _ => {}
    }
    events
}

fn remaining_status_ms(
    world: &SimulationWorld,
    player_id: PlayerId,
    kind: StatusKind,
) -> Option<u16> {
    world
        .statuses_for(player_id)?
        .into_iter()
        .find(|status| status.kind == kind)
        .map(|status| status.remaining_ms)
}

fn effect_spawned_by(events: &[SimulationEvent], owner: PlayerId, slot: u8) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            SimulationEvent::EffectSpawned { effect }
                if effect.owner == owner && effect.slot == slot
        )
    })
}

fn moved_player(events: &[SimulationEvent], player_id: PlayerId) -> Option<(i16, i16)> {
    events.iter().find_map(|event| match event {
        SimulationEvent::PlayerMoved {
            player_id: moved,
            x,
            y,
        } if *moved == player_id => Some((*x, *y)),
        _ => None,
    })
}

fn damage_to(events: &[SimulationEvent], target: PlayerId) -> Option<u16> {
    events.iter().find_map(|event| match event {
        SimulationEvent::DamageApplied {
            target: damaged,
            amount,
            ..
        } if *damaged == target => Some(*amount),
        _ => None,
    })
}

fn healing_to(events: &[SimulationEvent], target: PlayerId) -> Option<u16> {
    events.iter().find_map(|event| match event {
        SimulationEvent::HealingApplied {
            target: healed,
            amount,
            ..
        } if *healed == target => Some(*amount),
        _ => None,
    })
}

fn status_applied_to(events: &[SimulationEvent], target: PlayerId, kind: StatusKind) -> Option<u8> {
    events.iter().find_map(|event| match event {
        SimulationEvent::StatusApplied {
            target: applied,
            kind: applied_kind,
            stacks,
            ..
        } if *applied == target && *applied_kind == kind => Some(*stacks),
        _ => None,
    })
}

fn player_has_status(world: &SimulationWorld, player_id: PlayerId, kind: StatusKind) -> bool {
    world
        .statuses_for(player_id)
        .unwrap_or_default()
        .iter()
        .any(|status| status.kind == kind)
}

fn behavior_payload(behavior: &SkillBehavior) -> Option<game_content::EffectPayload> {
    match behavior {
        SkillBehavior::Projectile { payload, .. }
        | SkillBehavior::Beam { payload, .. }
        | SkillBehavior::Burst { payload, .. }
        | SkillBehavior::Nova { payload, .. }
        | SkillBehavior::Channel { payload, .. }
        | SkillBehavior::Summon { payload, .. }
        | SkillBehavior::Trap { payload, .. }
        | SkillBehavior::Aura { payload, .. } => Some(payload.clone()),
        SkillBehavior::Dash { payload, .. } => payload.clone(),
        SkillBehavior::Teleport { .. }
        | SkillBehavior::Passive { .. }
        | SkillBehavior::Ward { .. }
        | SkillBehavior::Barrier { .. } => None,
    }
}

fn miss_offset_units(radius: u16) -> i16 {
    i16::try_from(u32::from(radius) + u32::from(PLAYER_RADIUS_UNITS) + 80).unwrap_or(i16::MAX)
}

mod casting;
mod combat;
mod movement;
mod resources;
mod state;
mod statuses;
