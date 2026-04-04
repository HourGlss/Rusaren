use super::*;
use game_content::{ArenaMapObstacleKind, GameContent};
use game_domain::{PlayerName, PlayerRecord, SkillChoice, SkillTree};

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
    SimPlayerSeed {
        assignment: assignment(raw_id, raw_name, team),
        hit_points: 100,
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
    SimulationWorld::new(seeds, content.map()).expect("world should build")
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
    SimPlayerSeed {
        assignment: assignment(raw_id, raw_name, team),
        hit_points: 100,
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
