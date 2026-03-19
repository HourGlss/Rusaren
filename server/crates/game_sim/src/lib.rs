//! Fixed-step simulation, arena geometry, and authoritative combat resolution.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

mod actions;
mod effects;
mod geometry;
mod helpers;
mod ticks;

use std::collections::BTreeMap;
use std::fmt;

use game_content::{
    ArenaMapDefinition, CombatValueKind, MeleeDefinition, SkillBehavior, SkillDefinition,
    StatusDefinition, StatusKind,
};
use game_domain::{PlayerId, TeamAssignment, TeamSide};

use geometry::{
    normalize_aim, point_distance_sq, point_distance_units, project_from_aim, round_f32_to_i32,
    saturating_i16, segment_distance_sq, truncate_line_to_obstacles,
};
pub use geometry::{
    obstacle_blocks_movement, obstacle_blocks_projectiles, obstacle_blocks_vision,
    obstacle_contains_point, segment_hits_obstacle,
};
use helpers::{
    adjusted_move_speed, arena_effect_kind, map_obstacle_to_sim_obstacle, movement_delta,
    resolve_movement, spawn_position, total_move_modifier_bps, travel_distance_units,
};

pub const PLAYER_RADIUS_UNITS: u16 = 28;
pub const COMBAT_FRAME_MS: u16 = 100;
pub const PLAYER_MOVE_SPEED_UNITS_PER_SECOND: u16 = 260;
pub const PLAYER_MAX_MANA: u16 = 100;
pub const PLAYER_MANA_REGEN_PER_SECOND: u16 = 12;
pub const VISION_RADIUS_UNITS: u16 = 450;
const SPAWN_SPACING_UNITS: i16 = 120;
const DEFAULT_AIM_X: i16 = 120;
const DEFAULT_AIM_Y: i16 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MovementIntent {
    pub x: i8,
    pub y: i8,
}

impl MovementIntent {
    pub fn new(x: i8, y: i8) -> Result<Self, SimulationError> {
        if !(-1..=1).contains(&x) {
            return Err(SimulationError::MovementComponentOutOfRange {
                axis: "x",
                value: x,
            });
        }
        if !(-1..=1).contains(&y) {
            return Err(SimulationError::MovementComponentOutOfRange {
                axis: "y",
                value: y,
            });
        }
        Ok(Self { x, y })
    }

    #[must_use]
    pub const fn zero() -> Self {
        Self { x: 0, y: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaObstacleKind {
    Pillar,
    Shrub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaObstacle {
    pub kind: ArenaObstacleKind,
    pub center_x: i16,
    pub center_y: i16,
    pub half_width: u16,
    pub half_height: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaEffectKind {
    MeleeSwing,
    SkillShot,
    DashTrail,
    Burst,
    Nova,
    Beam,
    HitSpark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaEffect {
    pub kind: ArenaEffectKind,
    pub owner: PlayerId,
    pub slot: u8,
    pub x: i16,
    pub y: i16,
    pub target_x: i16,
    pub target_y: i16,
    pub radius: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaProjectile {
    pub owner: PlayerId,
    pub slot: u8,
    pub kind: ArenaEffectKind,
    pub x: i16,
    pub y: i16,
    pub radius: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimPlayerSeed {
    pub assignment: TeamAssignment,
    pub hit_points: u16,
    pub melee: MeleeDefinition,
    pub skills: [Option<SkillDefinition>; 5],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimPlayerState {
    pub player_id: PlayerId,
    pub team: TeamSide,
    pub x: i16,
    pub y: i16,
    pub aim_x: i16,
    pub aim_y: i16,
    pub hit_points: u16,
    pub max_hit_points: u16,
    pub mana: u16,
    pub max_mana: u16,
    pub alive: bool,
    pub moving: bool,
    pub primary_cooldown_remaining_ms: u16,
    pub primary_cooldown_total_ms: u16,
    pub slot_cooldown_remaining_ms: [u16; 5],
    pub slot_cooldown_total_ms: [u16; 5],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimStatusState {
    pub source: PlayerId,
    pub slot: u8,
    pub kind: StatusKind,
    pub stacks: u8,
    pub remaining_ms: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimulationEvent {
    PlayerMoved {
        player_id: PlayerId,
        x: i16,
        y: i16,
    },
    EffectSpawned {
        effect: ArenaEffect,
    },
    DamageApplied {
        attacker: PlayerId,
        target: PlayerId,
        amount: u16,
        remaining_hit_points: u16,
        defeated: bool,
    },
    HealingApplied {
        source: PlayerId,
        target: PlayerId,
        amount: u16,
        resulting_hit_points: u16,
    },
    StatusApplied {
        source: PlayerId,
        target: PlayerId,
        slot: u8,
        kind: StatusKind,
        stacks: u8,
        remaining_ms: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulationError {
    DuplicatePlayer(PlayerId),
    PlayerMissing(PlayerId),
    PlayerAlreadyDefeated(PlayerId),
    InvalidHitPoints {
        player_id: PlayerId,
        hit_points: u16,
    },
    MovementComponentOutOfRange {
        axis: &'static str,
        value: i8,
    },
    InvalidSkillSlot(u8),
    SkillSlotEmpty(u8),
}

impl fmt::Display for SimulationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePlayer(player_id) => {
                write!(
                    f,
                    "player {} appears more than once in the simulation",
                    player_id.get()
                )
            }
            Self::PlayerMissing(player_id) => {
                write!(
                    f,
                    "player {} is not part of the simulation",
                    player_id.get()
                )
            }
            Self::PlayerAlreadyDefeated(player_id) => {
                write!(f, "player {} is already defeated", player_id.get())
            }
            Self::InvalidHitPoints {
                player_id,
                hit_points,
            } => write!(
                f,
                "player {} must start with positive hit points, got {hit_points}",
                player_id.get()
            ),
            Self::MovementComponentOutOfRange { axis, value } => {
                write!(f, "movement component {axis}={value} is outside -1..=1")
            }
            Self::InvalidSkillSlot(slot) => {
                write!(f, "skill slot {slot} is outside the supported range 1..=5")
            }
            Self::SkillSlotEmpty(slot) => write!(f, "skill slot {slot} is not equipped"),
        }
    }
}

impl std::error::Error for SimulationError {}

#[derive(Clone, Debug)]
pub struct SimulationWorld {
    arena_width_units: u16,
    arena_height_units: u16,
    obstacles: Vec<ArenaObstacle>,
    players: BTreeMap<PlayerId, SimPlayer>,
    projectiles: Vec<ProjectileState>,
}

#[derive(Clone, Debug)]
struct SimPlayer {
    team: TeamSide,
    x: i16,
    y: i16,
    aim_x: i16,
    aim_y: i16,
    hit_points: u16,
    max_hit_points: u16,
    mana: u16,
    max_mana: u16,
    alive: bool,
    moving: bool,
    movement_intent: MovementIntent,
    queued_primary: bool,
    queued_cast_slot: Option<u8>,
    melee: MeleeDefinition,
    skills: [Option<SkillDefinition>; 5],
    primary_cooldown_remaining_ms: u16,
    slot_cooldown_remaining_ms: [u16; 5],
    mana_regen_progress: u16,
    statuses: Vec<StatusInstance>,
}

#[derive(Clone, Debug)]
struct StatusInstance {
    source: PlayerId,
    slot: u8,
    kind: StatusKind,
    stacks: u8,
    remaining_ms: u16,
    tick_interval_ms: Option<u16>,
    tick_progress_ms: u16,
    magnitude: u16,
    max_stacks: u8,
    trigger_duration_ms: Option<u16>,
}

#[derive(Clone, Debug)]
struct ProjectileState {
    owner: PlayerId,
    slot: u8,
    kind: ArenaEffectKind,
    x: i16,
    y: i16,
    direction_x: f32,
    direction_y: f32,
    speed_units_per_second: u16,
    remaining_range_units: i32,
    radius: u16,
    payload: game_content::EffectPayload,
}

impl SimulationWorld {
    pub fn new(
        players: Vec<SimPlayerSeed>,
        map: &ArenaMapDefinition,
    ) -> Result<Self, SimulationError> {
        let mut world_players = BTreeMap::new();
        let mut team_a_index = 0_u16;
        let mut team_b_index = 0_u16;

        for player in players {
            if player.hit_points == 0 {
                return Err(SimulationError::InvalidHitPoints {
                    player_id: player.assignment.player_id,
                    hit_points: player.hit_points,
                });
            }

            let spawn_index = match player.assignment.team {
                TeamSide::TeamA => {
                    let current = team_a_index;
                    team_a_index = team_a_index.saturating_add(1);
                    current
                }
                TeamSide::TeamB => {
                    let current = team_b_index;
                    team_b_index = team_b_index.saturating_add(1);
                    current
                }
            };
            let (spawn_x, spawn_y, aim_x) =
                spawn_position(player.assignment.team, spawn_index, map);

            if world_players
                .insert(
                    player.assignment.player_id,
                    SimPlayer {
                        team: player.assignment.team,
                        x: spawn_x,
                        y: spawn_y,
                        aim_x,
                        aim_y: DEFAULT_AIM_Y,
                        hit_points: player.hit_points,
                        max_hit_points: player.hit_points,
                        mana: PLAYER_MAX_MANA,
                        max_mana: PLAYER_MAX_MANA,
                        alive: true,
                        moving: false,
                        movement_intent: MovementIntent::zero(),
                        queued_primary: false,
                        queued_cast_slot: None,
                        melee: player.melee,
                        skills: player.skills,
                        primary_cooldown_remaining_ms: 0,
                        slot_cooldown_remaining_ms: [0; 5],
                        mana_regen_progress: 0,
                        statuses: Vec::new(),
                    },
                )
                .is_some()
            {
                return Err(SimulationError::DuplicatePlayer(
                    player.assignment.player_id,
                ));
            }
        }

        Ok(Self {
            arena_width_units: map.width_units,
            arena_height_units: map.height_units,
            obstacles: map
                .obstacles
                .iter()
                .map(map_obstacle_to_sim_obstacle)
                .collect(),
            players: world_players,
            projectiles: Vec::new(),
        })
    }

    pub fn submit_input(
        &mut self,
        player_id: PlayerId,
        movement: MovementIntent,
    ) -> Result<(), SimulationError> {
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(SimulationError::PlayerMissing(player_id))?;
        if !player.alive {
            return Err(SimulationError::PlayerAlreadyDefeated(player_id));
        }
        player.movement_intent = movement;
        Ok(())
    }

    pub fn update_aim(
        &mut self,
        player_id: PlayerId,
        aim_x: i16,
        aim_y: i16,
    ) -> Result<bool, SimulationError> {
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(SimulationError::PlayerMissing(player_id))?;
        if !player.alive {
            return Err(SimulationError::PlayerAlreadyDefeated(player_id));
        }
        if aim_x == 0 && aim_y == 0 {
            return Ok(false);
        }
        let changed = player.aim_x != aim_x || player.aim_y != aim_y;
        player.aim_x = aim_x;
        player.aim_y = aim_y;
        Ok(changed)
    }

    pub fn queue_primary_attack(&mut self, player_id: PlayerId) -> Result<(), SimulationError> {
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(SimulationError::PlayerMissing(player_id))?;
        if !player.alive {
            return Err(SimulationError::PlayerAlreadyDefeated(player_id));
        }
        player.queued_primary = true;
        Ok(())
    }

    pub fn queue_cast(&mut self, player_id: PlayerId, slot: u8) -> Result<(), SimulationError> {
        if !(1..=5).contains(&slot) {
            return Err(SimulationError::InvalidSkillSlot(slot));
        }

        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(SimulationError::PlayerMissing(player_id))?;
        if !player.alive {
            return Err(SimulationError::PlayerAlreadyDefeated(player_id));
        }
        if player.skills[usize::from(slot - 1)].is_none() {
            return Err(SimulationError::SkillSlotEmpty(slot));
        }
        player.queued_cast_slot = Some(slot);
        Ok(())
    }

    pub fn tick(&mut self, delta_ms: u16) -> Vec<SimulationEvent> {
        let mut events = Vec::new();
        self.advance_cooldowns(delta_ms);
        self.advance_mana(delta_ms);
        self.apply_status_ticks(delta_ms, &mut events);
        self.move_players(delta_ms, &mut events);
        self.resolve_queued_actions(&mut events);
        self.advance_projectiles(delta_ms, &mut events);
        events
    }

    #[must_use]
    pub fn player_state(&self, player_id: PlayerId) -> Option<SimPlayerState> {
        self.players.get(&player_id).map(|player| {
            let slot_cooldown_total_ms = std::array::from_fn(|index| {
                player
                    .skills
                    .get(index)
                    .and_then(|skill| skill.as_ref().map(|value| value.behavior.cooldown_ms()))
                    .unwrap_or(0)
            });
            SimPlayerState {
                player_id,
                team: player.team,
                x: player.x,
                y: player.y,
                aim_x: player.aim_x,
                aim_y: player.aim_y,
                hit_points: player.hit_points,
                max_hit_points: player.max_hit_points,
                mana: player.mana,
                max_mana: player.max_mana,
                alive: player.alive,
                moving: player.moving,
                primary_cooldown_remaining_ms: player.primary_cooldown_remaining_ms,
                primary_cooldown_total_ms: player.melee.cooldown_ms,
                slot_cooldown_remaining_ms: player.slot_cooldown_remaining_ms,
                slot_cooldown_total_ms,
            }
        })
    }

    #[must_use]
    pub fn players(&self) -> Vec<SimPlayerState> {
        self.players
            .keys()
            .copied()
            .filter_map(|player_id| self.player_state(player_id))
            .collect()
    }

    #[must_use]
    pub fn statuses_for(&self, player_id: PlayerId) -> Option<Vec<SimStatusState>> {
        self.players.get(&player_id).map(|player| {
            player
                .statuses
                .iter()
                .map(|status| SimStatusState {
                    source: status.source,
                    slot: status.slot,
                    kind: status.kind,
                    stacks: status.stacks,
                    remaining_ms: status.remaining_ms,
                })
                .collect()
        })
    }

    #[must_use]
    pub const fn arena_width_units(&self) -> u16 {
        self.arena_width_units
    }

    #[must_use]
    pub const fn arena_height_units(&self) -> u16 {
        self.arena_height_units
    }

    #[must_use]
    pub fn obstacles(&self) -> &[ArenaObstacle] {
        &self.obstacles
    }

    #[must_use]
    pub fn projectiles(&self) -> Vec<ArenaProjectile> {
        self.projectiles
            .iter()
            .map(|projectile| ArenaProjectile {
                owner: projectile.owner,
                slot: projectile.slot,
                kind: projectile.kind,
                x: projectile.x,
                y: projectile.y,
                radius: projectile.radius,
            })
            .collect()
    }

    #[must_use]
    pub fn is_team_defeated(&self, team: TeamSide) -> bool {
        self.players
            .values()
            .filter(|player| player.team == team)
            .all(|player| !player.alive)
    }
}

#[cfg(test)]
mod tests;
