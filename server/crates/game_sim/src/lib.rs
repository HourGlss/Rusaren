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
    ArenaMapDefinition, ArenaMapFeatureKind, CombatValueKind, CrowdControlDiminishingReturns,
    DispelScope, EffectPayload, MeleeDefinition, SimulationConfiguration, SkillBehavior,
    SkillDefinition, StatusDefinition, StatusKind,
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
    adjusted_move_speed, arena_effect_kind, circle_fits_map_footprint,
    map_obstacle_to_sim_obstacle, movement_delta, resolve_movement, spawn_position,
    total_move_modifier_bps, travel_distance_units,
};

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
    Footstep,
    BrushRustle,
    StealthFootstep,
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
    TrainingDummyResetFull,
    TrainingDummyExecute,
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
    pub max_mana: u16,
    pub move_speed_units_per_second: u16,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimCastMode {
    Windup,
    Channel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimCastCancelReason {
    Manual,
    Movement,
    ControlLoss,
    Defeat,
    Interrupt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimMissReason {
    NoTarget,
    Blocked,
    Expired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimTargetKind {
    Player,
    Deployable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimStatusRemovedReason {
    Expired,
    Dispelled,
    DamageBroken,
    Defeat,
    ShieldConsumed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimTriggerReason {
    Expire,
    Dispel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimRemovedStatus {
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
        slot: u8,
        amount: u16,
        critical: bool,
        remaining_hit_points: u16,
        defeated: bool,
        status_kind: Option<StatusKind>,
        trigger: Option<SimTriggerReason>,
    },
    HealingApplied {
        source: PlayerId,
        target: PlayerId,
        slot: u8,
        amount: u16,
        critical: bool,
        resulting_hit_points: u16,
        status_kind: Option<StatusKind>,
        trigger: Option<SimTriggerReason>,
    },
    StatusApplied {
        source: PlayerId,
        target: PlayerId,
        slot: u8,
        kind: StatusKind,
        stacks: u8,
        stack_delta: u8,
        remaining_ms: u16,
    },
    StatusRemoved {
        source: PlayerId,
        target: PlayerId,
        slot: u8,
        kind: StatusKind,
        stacks: u8,
        remaining_ms: u16,
        reason: SimStatusRemovedReason,
    },
    CastStarted {
        player_id: PlayerId,
        slot: u8,
        behavior: &'static str,
        mode: SimCastMode,
        total_ms: u16,
    },
    CastCompleted {
        player_id: PlayerId,
        slot: u8,
        behavior: &'static str,
    },
    CastCanceled {
        player_id: PlayerId,
        slot: u8,
        reason: SimCastCancelReason,
    },
    ChannelTick {
        player_id: PlayerId,
        slot: u8,
        tick_index: u16,
        behavior: &'static str,
    },
    ImpactHit {
        source: PlayerId,
        slot: u8,
        target_kind: SimTargetKind,
        target_id: u32,
    },
    ImpactMiss {
        source: PlayerId,
        slot: u8,
        reason: SimMissReason,
    },
    DispelCast {
        source: PlayerId,
        slot: u8,
        scope: DispelScope,
        max_statuses: u8,
    },
    DispelResult {
        source: PlayerId,
        slot: u8,
        target: PlayerId,
        removed_statuses: Vec<SimRemovedStatus>,
        triggered_payload_count: u8,
    },
    TriggerResolved {
        source: PlayerId,
        slot: u8,
        status_kind: StatusKind,
        trigger: SimTriggerReason,
        target_kind: SimTargetKind,
        target_id: u32,
        payload_kind: CombatValueKind,
        amount: u16,
    },
    Defeat {
        attacker: Option<PlayerId>,
        target: PlayerId,
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
    configuration: SimulationConfiguration,
    arena_width_units: u16,
    arena_height_units: u16,
    arena_width_tiles: u16,
    arena_height_tiles: u16,
    arena_tile_units: u16,
    footprint_mask: Vec<u8>,
    obstacles: Vec<ArenaObstacle>,
    players: BTreeMap<PlayerId, SimPlayer>,
    projectiles: Vec<ProjectileState>,
    deployables: Vec<DeployableState>,
    next_deployable_id: u32,
    elapsed_ms: u32,
    initial_roll_state: u64,
    roll_state: u64,
}

#[derive(Clone, Debug)]
struct SimPlayer {
    team: TeamSide,
    spawn_x: i16,
    spawn_y: i16,
    spawn_aim_x: i16,
    spawn_aim_y: i16,
    x: i16,
    y: i16,
    aim_x: i16,
    aim_y: i16,
    hit_points: u16,
    max_hit_points: u16,
    mana: u16,
    max_mana: u16,
    move_speed_units_per_second: u16,
    alive: bool,
    moving: bool,
    movement_intent: MovementIntent,
    queued_actions: QueuedActions,
    active_cast: Option<PendingCast>,
    melee: MeleeDefinition,
    skills: [Option<SkillDefinition>; 5],
    primary_cooldown_remaining_ms: u16,
    slot_cooldown_remaining_ms: [u16; 5],
    proc_cooldown_remaining_ms: [u16; 5],
    mana_regen_progress: u16,
    movement_audio_progress_ms: u16,
    statuses: Vec<StatusInstance>,
    next_cast_procs: Vec<NextCastProc>,
    hard_cc_dr: CrowdControlDrState,
    movement_cc_dr: CrowdControlDrState,
    cast_cc_dr: CrowdControlDrState,
}

#[derive(Clone, Copy, Debug, Default)]
struct QueuedActions {
    primary: bool,
    cast_slot: Option<u8>,
    cast_self_target: bool,
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
    expire_payload: Option<Box<EffectPayload>>,
    dispel_payload: Option<Box<EffectPayload>>,
}

#[derive(Clone, Debug)]
struct PendingCast {
    slot: u8,
    slot_index: usize,
    self_target: bool,
    remaining_ms: u16,
    total_ms: u16,
    just_started: bool,
    mode: ActiveCastMode,
}

#[derive(Clone, Debug)]
struct NextCastProc {
    passive_slot: u8,
    skill_ids: Vec<String>,
    costs_mana: bool,
    starts_cooldown: bool,
}

#[derive(Clone, Debug)]
enum ActiveCastMode {
    Windup,
    Channel {
        self_target: bool,
        range: u16,
        radius: u16,
        tick_interval_ms: u16,
        tick_progress_ms: u16,
        effect_kind: ArenaEffectKind,
        payload: EffectPayload,
    },
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
    slot: u8,
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

#[derive(Clone, Debug)]
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
        cast_start_payload: Option<game_content::EffectPayload>,
        cast_end_payload: Option<game_content::EffectPayload>,
        anchor_player: Option<PlayerId>,
        toggleable: bool,
    },
    TrainingDummyResetFull,
    TrainingDummyExecute,
}

#[derive(Clone, Copy, Debug, Default)]
struct PassiveModifiers {
    player_speed: u16,
    projectile_speed: u16,
    cooldown: u16,
    cast_time: u16,
}

#[derive(Clone, Copy, Debug, Default)]
struct CrowdControlDrState {
    stage: u8,
    remaining_ms: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrowdControlDrBucket {
    Hard,
    Movement,
    Cast,
}

impl SimulationWorld {
    pub fn new(
        players: Vec<SimPlayerSeed>,
        map: &ArenaMapDefinition,
        configuration: SimulationConfiguration,
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
                spawn_position(player.assignment.team, spawn_index, map, &configuration);

            if world_players
                .insert(
                    player.assignment.player_id,
                    SimPlayer {
                        team: player.assignment.team,
                        spawn_x,
                        spawn_y,
                        spawn_aim_x: aim_x,
                        spawn_aim_y: configuration.default_aim_y_units,
                        x: spawn_x,
                        y: spawn_y,
                        aim_x,
                        aim_y: configuration.default_aim_y_units,
                        hit_points: player.hit_points,
                        max_hit_points: player.hit_points,
                        mana: player.max_mana,
                        max_mana: player.max_mana,
                        move_speed_units_per_second: player.move_speed_units_per_second,
                        alive: true,
                        moving: false,
                        movement_intent: MovementIntent::zero(),
                        queued_actions: QueuedActions::default(),
                        active_cast: None,
                        melee: player.melee,
                        skills: player.skills,
                        primary_cooldown_remaining_ms: 0,
                        slot_cooldown_remaining_ms: [0; 5],
                        proc_cooldown_remaining_ms: [0; 5],
                        mana_regen_progress: 0,
                        movement_audio_progress_ms: 0,
                        statuses: Vec::new(),
                        next_cast_procs: Vec::new(),
                        hard_cc_dr: CrowdControlDrState::default(),
                        movement_cc_dr: CrowdControlDrState::default(),
                        cast_cc_dr: CrowdControlDrState::default(),
                    },
                )
                .is_some()
            {
                return Err(SimulationError::DuplicatePlayer(
                    player.assignment.player_id,
                ));
            }
        }

        let mut world = Self {
            configuration,
            arena_width_units: map.width_units,
            arena_height_units: map.height_units,
            arena_width_tiles: map.width_tiles,
            arena_height_tiles: map.height_tiles,
            arena_tile_units: map.tile_units,
            footprint_mask: map.footprint_mask.clone(),
            obstacles: map
                .obstacles
                .iter()
                .map(map_obstacle_to_sim_obstacle)
                .collect(),
            players: world_players,
            projectiles: Vec::new(),
            deployables: Vec::new(),
            next_deployable_id: 1,
            elapsed_ms: 0,
            initial_roll_state: 0,
            roll_state: 0,
        };
        world.initial_roll_state = world.compute_initial_roll_state();
        world.roll_state = world.initial_roll_state;
        if let Some(owner) = world.players.keys().next().copied() {
            let owner_team = world
                .players
                .get(&owner)
                .map_or(TeamSide::TeamA, |player| player.team);
            world.spawn_map_features(owner, owner_team, map);
        }
        Ok(world)
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
        player.queued_actions.primary = true;
        Ok(())
    }

    pub fn queue_cast(&mut self, player_id: PlayerId, slot: u8) -> Result<(), SimulationError> {
        self.queue_cast_with_mode(player_id, slot, false)
    }

    pub fn queue_cast_with_mode(
        &mut self,
        player_id: PlayerId,
        slot: u8,
        self_target: bool,
    ) -> Result<(), SimulationError> {
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
        player.queued_actions.cast_slot = Some(slot);
        player.queued_actions.cast_self_target = self_target;
        Ok(())
    }

    pub fn cancel_active_cast(&mut self, player_id: PlayerId) -> Result<bool, SimulationError> {
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(SimulationError::PlayerMissing(player_id))?;
        if !player.alive {
            return Err(SimulationError::PlayerAlreadyDefeated(player_id));
        }
        Ok(player.active_cast.take().is_some())
    }

    pub fn reset_training_session(&mut self) {
        self.projectiles.clear();
        self.elapsed_ms = 0;
        self.roll_state = self.initial_roll_state;
        self.deployables.retain(|deployable| {
            matches!(
                deployable.behavior,
                DeployableBehavior::TrainingDummyResetFull
                    | DeployableBehavior::TrainingDummyExecute
            )
        });
        for deployable in &mut self.deployables {
            deployable.remaining_ms = u16::MAX;
            deployable.hit_points = match deployable.behavior {
                DeployableBehavior::TrainingDummyResetFull => deployable.max_hit_points,
                DeployableBehavior::TrainingDummyExecute => {
                    Self::training_dummy_execute_hit_points(
                        deployable.max_hit_points,
                        self.configuration.training_dummy.execute_threshold_bps,
                    )
                }
                _ => deployable.hit_points,
            };
        }
        for player in self.players.values_mut() {
            player.x = player.spawn_x;
            player.y = player.spawn_y;
            player.aim_x = player.spawn_aim_x;
            player.aim_y = player.spawn_aim_y;
            player.hit_points = player.max_hit_points;
            player.mana = player.max_mana;
            player.alive = true;
            player.moving = false;
            player.movement_intent = MovementIntent::zero();
            player.queued_actions = QueuedActions::default();
            player.active_cast = None;
            player.primary_cooldown_remaining_ms = 0;
            player.slot_cooldown_remaining_ms = [0; 5];
            player.proc_cooldown_remaining_ms = [0; 5];
            player.mana_regen_progress = 0;
            player.movement_audio_progress_ms = 0;
            player.statuses.clear();
            player.next_cast_procs.clear();
            player.hard_cc_dr = CrowdControlDrState::default();
            player.movement_cc_dr = CrowdControlDrState::default();
            player.cast_cc_dr = CrowdControlDrState::default();
        }
    }

    pub fn tick(&mut self, delta_ms: u16) -> Vec<SimulationEvent> {
        let mut events = Vec::new();
        self.elapsed_ms = self.elapsed_ms.saturating_add(u32::from(delta_ms));
        self.advance_cooldowns(delta_ms);
        self.advance_mana(delta_ms);
        self.advance_crowd_control_diminishing_returns(delta_ms);
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
                            self.effective_skill_cooldown_ms(
                                player_id,
                                value.behavior.cooldown_ms(),
                            )
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
    pub fn effect_audio_cue_id(
        &self,
        owner: PlayerId,
        slot: u8,
        kind: ArenaEffectKind,
    ) -> Option<String> {
        match kind {
            ArenaEffectKind::Footstep => Some(String::from("movement_footstep")),
            ArenaEffectKind::BrushRustle => Some(String::from("movement_brush_rustle")),
            ArenaEffectKind::StealthFootstep => Some(String::from("movement_stealth_step")),
            _ => self.player_audio_cue_id_for_slot(owner, slot),
        }
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

    fn player_audio_cue_id_for_slot(&self, player_id: PlayerId, slot: u8) -> Option<String> {
        let player = self.players.get(&player_id)?;
        if slot == 0 {
            return Some(
                player
                    .melee
                    .audio_cue_id
                    .clone()
                    .unwrap_or_else(|| player.melee.id.clone()),
            );
        }
        let slot_index = usize::from(slot.saturating_sub(1));
        let skill = player.skills.get(slot_index)?.as_ref()?;
        Some(
            skill
                .audio_cue_id
                .clone()
                .unwrap_or_else(|| skill.id.clone()),
        )
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
    pub fn footprint_mask(&self) -> &[u8] {
        &self.footprint_mask
    }

    #[must_use]
    pub const fn configuration(&self) -> &SimulationConfiguration {
        &self.configuration
    }

    #[must_use]
    pub const fn vision_radius_units(&self) -> u16 {
        self.configuration.vision_radius_units
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
                proc_reset: _,
            } = skill.behavior
            {
                let caps = self.configuration.passive_bonus_caps;
                modifiers.player_speed = modifiers
                    .player_speed
                    .saturating_add(player_speed_bps)
                    .min(caps.player_speed_bps);
                modifiers.projectile_speed = modifiers
                    .projectile_speed
                    .saturating_add(projectile_speed_bps)
                    .min(caps.projectile_speed_bps);
                modifiers.cooldown = modifiers
                    .cooldown
                    .saturating_add(cooldown_bps)
                    .min(caps.cooldown_bps);
                modifiers.cast_time = modifiers
                    .cast_time
                    .saturating_add(cast_time_bps)
                    .min(caps.cast_time_bps);
            }
        }
        modifiers
    }

    fn scale_duration_ms(base_ms: u16, reduction_bps: u16) -> u16 {
        let scale_bps = 10_000_u32.saturating_sub(u32::from(reduction_bps));
        let scaled = u32::from(base_ms).saturating_mul(scale_bps) / 10_000;
        u16::try_from(scaled).unwrap_or(u16::MAX)
    }

    fn scale_duration_with_bps(base_ms: u16, scale_bps: u16) -> u16 {
        let scaled = u32::from(base_ms).saturating_mul(u32::from(scale_bps));
        let rounded = scaled.div_ceil(10_000);
        u16::try_from(rounded).unwrap_or(u16::MAX)
    }

    fn scale_speed_units(base_speed: u16, bonus_bps: u16) -> u16 {
        let scaled =
            u32::from(base_speed).saturating_mul(10_000_u32 + u32::from(bonus_bps)) / 10_000;
        u16::try_from(scaled).unwrap_or(u16::MAX)
    }

    fn compute_initial_roll_state(&self) -> u64 {
        let mut seed = 0x9E37_79B9_7F4A_7C15_u64
            ^ u64::from(self.arena_width_units)
            ^ (u64::from(self.arena_height_units) << 16)
            ^ (u64::from(self.configuration.combat_frame_ms) << 32);
        for player_id in self.players.keys() {
            seed = seed
                .wrapping_mul(0xBF58_476D_1CE4_E5B9)
                .wrapping_add(u64::from(player_id.get()) << 1);
        }
        seed
    }

    fn next_roll_u64(&mut self) -> u64 {
        self.roll_state = self.roll_state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.roll_state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    fn roll_u16_inclusive(&mut self, min: u16, max: u16) -> u16 {
        if max <= min {
            return min;
        }
        let span = u32::from(max) - u32::from(min) + 1;
        let offset = u32::try_from(self.next_roll_u64() % u64::from(span)).unwrap_or(0);
        min.saturating_add(u16::try_from(offset).unwrap_or(0))
    }

    fn roll_bps(&mut self, chance_bps: u16) -> bool {
        if chance_bps == 0 {
            return false;
        }
        if chance_bps >= 10_000 {
            return true;
        }
        (self.next_roll_u64() % 10_000) < u64::from(chance_bps)
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
        Self::scale_speed_units(
            base_speed,
            self.passive_modifiers_for(player_id)
                .projectile_speed
                .saturating_add(self.configuration.global_projectile_speed_bonus_bps),
        )
    }

    fn effective_move_modifier_bps(&self, player_id: PlayerId, statuses: &[StatusInstance]) -> i16 {
        let status_modifier =
            total_move_modifier_bps(statuses, &self.configuration.movement_modifier_caps);
        let passive_bonus =
            i16::try_from(self.passive_modifiers_for(player_id).player_speed).unwrap_or(i16::MAX);
        status_modifier.saturating_add(passive_bonus).clamp(
            self.configuration
                .movement_modifier_caps
                .overall_total_min_bps,
            self.configuration
                .movement_modifier_caps
                .overall_total_max_bps,
        )
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
        let player_radius_units = self.configuration.player_radius_units;
        let min_x = -i32::from(self.arena_width_units / 2) + i32::from(player_radius_units);
        let max_x = i32::from(self.arena_width_units / 2) - i32::from(player_radius_units);
        let min_y = -i32::from(self.arena_height_units / 2) + i32::from(player_radius_units);
        let max_y = i32::from(self.arena_height_units / 2) - i32::from(player_radius_units);
        let x_i32 = i32::from(x);
        let y_i32 = i32::from(y);
        if x_i32 < min_x || x_i32 > max_x || y_i32 < min_y || y_i32 > max_y {
            return false;
        }
        if !circle_fits_map_footprint(
            x,
            y,
            player_radius_units,
            self.arena_width_units,
            self.arena_height_units,
            self.arena_width_tiles,
            self.arena_height_tiles,
            self.arena_tile_units,
            &self.footprint_mask,
        ) {
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
        for step in 1_u16..=self.configuration.teleport_resolution_steps {
            let t = f32::from(step) / f32::from(self.configuration.teleport_resolution_steps);
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
        self.cancel_toggleable_stealth_auras(player_id);
    }

    fn cancel_toggleable_stealth_auras(&mut self, player_id: PlayerId) {
        self.deployables.retain(|deployable| {
            deployable.owner != player_id || !Self::deployable_tracks_toggleable_stealth(deployable)
        });
    }

    fn deployable_tracks_toggleable_stealth(deployable: &DeployableState) -> bool {
        match &deployable.behavior {
            DeployableBehavior::Aura {
                payload,
                cast_start_payload,
                toggleable,
                ..
            } => {
                *toggleable
                    && (Self::payload_applies_status_kind(payload, StatusKind::Stealth)
                        || cast_start_payload.as_ref().is_some_and(|payload| {
                            Self::payload_applies_status_kind(payload, StatusKind::Stealth)
                        }))
            }
            _ => false,
        }
    }

    fn payload_applies_status_kind(
        payload: &game_content::EffectPayload,
        kind: StatusKind,
    ) -> bool {
        payload
            .status
            .as_ref()
            .is_some_and(|status| status.kind == kind)
    }

    fn payload_hides_aura_visuals(payload: &game_content::EffectPayload) -> bool {
        Self::payload_applies_status_kind(payload, StatusKind::Stealth)
            && payload.amount == 0
            && payload.amount_max.is_none()
            && payload.interrupt_silence_duration_ms.is_none()
            && payload.dispel.is_none()
    }

    fn crowd_control_bucket(kind: StatusKind) -> Option<CrowdControlDrBucket> {
        match kind {
            StatusKind::Stun | StatusKind::Sleep | StatusKind::Fear => {
                Some(CrowdControlDrBucket::Hard)
            }
            StatusKind::Root => Some(CrowdControlDrBucket::Movement),
            StatusKind::Silence => Some(CrowdControlDrBucket::Cast),
            StatusKind::Poison
            | StatusKind::Hot
            | StatusKind::Chill
            | StatusKind::Haste
            | StatusKind::Shield
            | StatusKind::Stealth
            | StatusKind::Reveal
            | StatusKind::HealingReduction => None,
        }
    }

    fn crowd_control_dr_state_mut(
        player: &mut SimPlayer,
        bucket: CrowdControlDrBucket,
    ) -> &mut CrowdControlDrState {
        match bucket {
            CrowdControlDrBucket::Hard => &mut player.hard_cc_dr,
            CrowdControlDrBucket::Movement => &mut player.movement_cc_dr,
            CrowdControlDrBucket::Cast => &mut player.cast_cc_dr,
        }
    }

    fn crowd_control_dr_scale_bps(stage: u8, dr: CrowdControlDiminishingReturns) -> u16 {
        dr.stages_bps.get(usize::from(stage)).copied().unwrap_or(0)
    }

    fn apply_crowd_control_dr(
        player: &mut SimPlayer,
        kind: StatusKind,
        duration_ms: u16,
        dr: CrowdControlDiminishingReturns,
    ) -> Option<u16> {
        let Some(bucket) = Self::crowd_control_bucket(kind) else {
            return Some(duration_ms);
        };
        let state = Self::crowd_control_dr_state_mut(player, bucket);
        if state.remaining_ms == 0 {
            state.stage = 0;
        }
        let scale_bps = Self::crowd_control_dr_scale_bps(state.stage, dr);
        state.stage = state.stage.saturating_add(1).min(3);
        state.remaining_ms = dr.window_ms;
        if scale_bps == 0 {
            return None;
        }
        Some(Self::scale_duration_with_bps(duration_ms, scale_bps))
    }

    fn player_has_status(&self, player_id: PlayerId, kind: StatusKind) -> bool {
        self.players
            .get(&player_id)
            .is_some_and(|player| player.statuses.iter().any(|status| status.kind == kind))
    }

    const fn status_matches_dispel(kind: StatusKind, scope: DispelScope) -> bool {
        match scope {
            DispelScope::Positive => matches!(
                kind,
                StatusKind::Hot | StatusKind::Haste | StatusKind::Shield | StatusKind::Stealth
            ),
            DispelScope::Negative => matches!(
                kind,
                StatusKind::Poison
                    | StatusKind::Chill
                    | StatusKind::Root
                    | StatusKind::Silence
                    | StatusKind::Stun
                    | StatusKind::Sleep
                    | StatusKind::Reveal
                    | StatusKind::Fear
                    | StatusKind::HealingReduction
            ),
            DispelScope::All => true,
        }
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

    fn spawn_map_features(
        &mut self,
        owner: PlayerId,
        owner_team: TeamSide,
        map: &ArenaMapDefinition,
    ) {
        let dummy_team = match owner_team {
            TeamSide::TeamA => TeamSide::TeamB,
            TeamSide::TeamB => TeamSide::TeamA,
        };
        let hit_points = self
            .configuration
            .training_dummy
            .base_hit_points
            .saturating_mul(self.configuration.training_dummy.health_multiplier);
        for feature in &map.features {
            let (kind, behavior) = match feature.kind {
                ArenaMapFeatureKind::TrainingDummyResetFull => (
                    ArenaDeployableKind::TrainingDummyResetFull,
                    DeployableBehavior::TrainingDummyResetFull,
                ),
                ArenaMapFeatureKind::TrainingDummyExecute => (
                    ArenaDeployableKind::TrainingDummyExecute,
                    DeployableBehavior::TrainingDummyExecute,
                ),
            };
            let deployable_id = self.next_deployable_id();
            self.deployables.push(DeployableState {
                id: deployable_id,
                owner,
                slot: 0,
                team: dummy_team,
                kind,
                x: feature.center_x,
                y: feature.center_y,
                radius: self.configuration.player_radius_units,
                hit_points,
                max_hit_points: hit_points,
                remaining_ms: u16::MAX,
                blocks_movement: false,
                blocks_projectiles: false,
                behavior,
            });
        }
    }

    fn training_dummy_execute_hit_points(max_hit_points: u16, execute_threshold_bps: u16) -> u16 {
        let threshold = (u32::from(max_hit_points) * u32::from(execute_threshold_bps)) / 10_000;
        u16::try_from(threshold.max(1)).unwrap_or(1)
    }
}

#[cfg(test)]
mod tests;
