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
    Barrier,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaDeployableKind {
    Summon,
    Ward,
    Trap,
    Barrier,
    Aura,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaDeployable {
    pub id: u32,
    pub owner: PlayerId,
    pub team: TeamSide,
    pub kind: ArenaDeployableKind,
    pub x: i16,
    pub y: i16,
    pub radius: u16,
    pub hit_points: u16,
    pub max_hit_points: u16,
    pub remaining_ms: u16,
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
    pub current_cast_slot: Option<u8>,
    pub current_cast_remaining_ms: u16,
    pub current_cast_total_ms: u16,
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
    DeployableSpawned {
        deployable_id: u32,
        owner: PlayerId,
        kind: ArenaDeployableKind,
        x: i16,
        y: i16,
        radius: u16,
    },
    DeployableDamaged {
        attacker: PlayerId,
        deployable_id: u32,
        amount: u16,
        remaining_hit_points: u16,
        destroyed: bool,
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
    deployables: Vec<DeployableState>,
    next_deployable_id: u32,
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
    active_cast: Option<PendingCast>,
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
    shield_remaining: u16,
}

#[derive(Clone, Debug)]
struct PendingCast {
    slot: u8,
    slot_index: usize,
    remaining_ms: u16,
    total_ms: u16,
    just_started: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetEntity {
    Player(PlayerId),
    Deployable(u32),
}

#[derive(Clone, Debug)]
struct DeployableState {
    id: u32,
    owner: PlayerId,
    team: TeamSide,
    kind: ArenaDeployableKind,
    x: i16,
    y: i16,
    radius: u16,
    hit_points: u16,
    max_hit_points: u16,
    remaining_ms: u16,
    blocks_movement: bool,
    blocks_projectiles: bool,
    behavior: DeployableBehavior,
}

#[derive(Clone, Copy, Debug)]
enum DeployableBehavior {
    Summon {
        range: u16,
        tick_interval_ms: u16,
        tick_progress_ms: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
    },
    Ward,
    Trap {
        payload: game_content::EffectPayload,
    },
    Barrier,
    Aura {
        tick_interval_ms: u16,
        tick_progress_ms: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
        anchor_player: Option<PlayerId>,
    },
}

#[derive(Clone, Copy, Debug, Default)]
struct PassiveModifiers {
    player_speed: u16,
    projectile_speed: u16,
    cooldown: u16,
    cast_time: u16,
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
                        active_cast: None,
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
            deployables: Vec::new(),
            next_deployable_id: 1,
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
        if player.active_cast.is_some() {
            return Ok(());
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
        if player.active_cast.is_some() {
            return Ok(());
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
        self.advance_active_casts(delta_ms, &mut events);
        self.advance_deployables(delta_ms, &mut events);
        events
    }

    #[must_use]
    pub fn player_state(&self, player_id: PlayerId) -> Option<SimPlayerState> {
        self.players.get(&player_id).map(|player| {
            let slot_cooldown_total_ms = std::array::from_fn(|index| {
                player
                    .skills
                    .get(index)
                    .and_then(|skill| {
                        skill.as_ref().map(|value| {
                            self.effective_skill_cooldown_ms(player_id, value.behavior.cooldown_ms())
                        })
                    })
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
                primary_cooldown_total_ms: self
                    .effective_primary_cooldown_ms(player_id, player.melee.cooldown_ms),
                slot_cooldown_remaining_ms: player.slot_cooldown_remaining_ms,
                slot_cooldown_total_ms,
                current_cast_slot: player.active_cast.as_ref().map(|cast| cast.slot),
                current_cast_remaining_ms: player
                    .active_cast
                    .as_ref()
                    .map_or(0, |cast| cast.remaining_ms),
                current_cast_total_ms: player.active_cast.as_ref().map_or(0, |cast| cast.total_ms),
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
    pub fn deployables(&self) -> Vec<ArenaDeployable> {
        self.deployables
            .iter()
            .map(|deployable| ArenaDeployable {
                id: deployable.id,
                owner: deployable.owner,
                team: deployable.team,
                kind: deployable.kind,
                x: deployable.x,
                y: deployable.y,
                radius: deployable.radius,
                hit_points: deployable.hit_points,
                max_hit_points: deployable.max_hit_points,
                remaining_ms: deployable.remaining_ms,
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

    fn passive_modifiers_for(&self, player_id: PlayerId) -> PassiveModifiers {
        let Some(player) = self.players.get(&player_id) else {
            return PassiveModifiers::default();
        };
        let mut modifiers = PassiveModifiers::default();
        for skill in player.skills.iter().flatten() {
            if let SkillBehavior::Passive {
                player_speed_bps,
                projectile_speed_bps,
                cooldown_bps,
                cast_time_bps,
            } = skill.behavior
            {
                modifiers.player_speed = modifiers
                    .player_speed
                    .saturating_add(player_speed_bps)
                    .min(9_000);
                modifiers.projectile_speed = modifiers
                    .projectile_speed
                    .saturating_add(projectile_speed_bps)
                    .min(9_000);
                modifiers.cooldown = modifiers.cooldown.saturating_add(cooldown_bps).min(9_000);
                modifiers.cast_time = modifiers.cast_time.saturating_add(cast_time_bps).min(9_500);
            }
        }
        modifiers
    }

    fn scale_duration_ms(base_ms: u16, reduction_bps: u16) -> u16 {
        let scale_bps = 10_000_u32.saturating_sub(u32::from(reduction_bps));
        let scaled = u32::from(base_ms).saturating_mul(scale_bps) / 10_000;
        u16::try_from(scaled).unwrap_or(u16::MAX)
    }

    fn scale_speed_units(base_speed: u16, bonus_bps: u16) -> u16 {
        let scaled = u32::from(base_speed).saturating_mul(10_000_u32 + u32::from(bonus_bps)) / 10_000;
        u16::try_from(scaled).unwrap_or(u16::MAX)
    }

    fn effective_primary_cooldown_ms(&self, player_id: PlayerId, base_ms: u16) -> u16 {
        Self::scale_duration_ms(base_ms, self.passive_modifiers_for(player_id).cooldown)
    }

    fn effective_skill_cooldown_ms(&self, player_id: PlayerId, base_ms: u16) -> u16 {
        Self::scale_duration_ms(base_ms, self.passive_modifiers_for(player_id).cooldown)
    }

    fn effective_cast_time_ms(&self, player_id: PlayerId, base_ms: u16) -> u16 {
        Self::scale_duration_ms(base_ms, self.passive_modifiers_for(player_id).cast_time)
    }

    fn effective_projectile_speed(&self, player_id: PlayerId, base_speed: u16) -> u16 {
        Self::scale_speed_units(base_speed, self.passive_modifiers_for(player_id).projectile_speed)
    }

    fn effective_move_modifier_bps(&self, player_id: PlayerId, statuses: &[StatusInstance]) -> i16 {
        let status_modifier = total_move_modifier_bps(statuses);
        let passive_bonus = i16::try_from(self.passive_modifiers_for(player_id).player_speed)
            .unwrap_or(i16::MAX);
        status_modifier.saturating_add(passive_bonus).clamp(-8_000, 9_000)
    }

    fn combat_obstacles(&self) -> Vec<ArenaObstacle> {
        let mut obstacles = self.obstacles.clone();
        for deployable in &self.deployables {
            if deployable.blocks_movement || deployable.blocks_projectiles {
                obstacles.push(ArenaObstacle {
                    kind: ArenaObstacleKind::Barrier,
                    center_x: deployable.x,
                    center_y: deployable.y,
                    half_width: deployable.radius,
                    half_height: deployable.radius,
                });
            }
        }
        obstacles
    }

    fn is_walkable_position(&self, x: i16, y: i16) -> bool {
        let min_x = -i32::from(self.arena_width_units / 2) + i32::from(PLAYER_RADIUS_UNITS);
        let max_x = i32::from(self.arena_width_units / 2) - i32::from(PLAYER_RADIUS_UNITS);
        let min_y = -i32::from(self.arena_height_units / 2) + i32::from(PLAYER_RADIUS_UNITS);
        let max_y = i32::from(self.arena_height_units / 2) - i32::from(PLAYER_RADIUS_UNITS);
        let x_i32 = i32::from(x);
        let y_i32 = i32::from(y);
        if x_i32 < min_x || x_i32 > max_x || y_i32 < min_y || y_i32 > max_y {
            return false;
        }
        !self
            .combat_obstacles()
            .iter()
            .filter(|obstacle| obstacle_blocks_movement(obstacle))
            .any(|obstacle| obstacle_contains_point(x, y, obstacle))
    }

    fn resolve_teleport_destination(
        &self,
        start_x: i16,
        start_y: i16,
        desired_x: i16,
        desired_y: i16,
    ) -> (i16, i16) {
        if self.is_walkable_position(desired_x, desired_y) {
            return (desired_x, desired_y);
        }
        for step in 1_u16..=48 {
            let t = f32::from(step) / 48.0;
            let sample_x = saturating_i16(round_f32_to_i32(
                f32::from(desired_x) + (f32::from(start_x) - f32::from(desired_x)) * t,
            ));
            let sample_y = saturating_i16(round_f32_to_i32(
                f32::from(desired_y) + (f32::from(start_y) - f32::from(desired_y)) * t,
            ));
            if self.is_walkable_position(sample_x, sample_y) {
                return (sample_x, sample_y);
            }
        }
        (start_x, start_y)
    }

    fn break_stealth(&mut self, player_id: PlayerId) {
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        player
            .statuses
            .retain(|status| status.kind != StatusKind::Stealth);
    }

    fn player_has_status(&self, player_id: PlayerId, kind: StatusKind) -> bool {
        self.players
            .get(&player_id)
            .is_some_and(|player| player.statuses.iter().any(|status| status.kind == kind))
    }

    fn can_enemy_target_player(&self, attacker: PlayerId, target: PlayerId) -> bool {
        let Some(attacker_player) = self.players.get(&attacker) else {
            return false;
        };
        let Some(target_player) = self.players.get(&target) else {
            return false;
        };
        if !target_player.alive {
            return false;
        }
        if attacker_player.team == target_player.team {
            return true;
        }
        !self.player_has_status(target, StatusKind::Stealth)
            || self.player_has_status(target, StatusKind::Reveal)
    }

    fn next_deployable_id(&mut self) -> u32 {
        let id = self.next_deployable_id;
        self.next_deployable_id = self.next_deployable_id.saturating_add(1);
        id
    }
}

#[cfg(test)]
mod tests;
