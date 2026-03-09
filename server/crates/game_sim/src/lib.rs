//! Fixed-step simulation, arena geometry, and authoritative combat resolution.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::collections::BTreeMap;
use std::fmt;

use game_content::{
    ArenaMapDefinition, ArenaMapObstacle, ArenaMapObstacleKind, CombatValueKind, MeleeDefinition,
    SkillBehavior, SkillDefinition, SkillEffectKind, StatusDefinition, StatusKind,
};
use game_domain::{PlayerId, TeamAssignment, TeamSide};

pub const PLAYER_RADIUS_UNITS: u16 = 28;
pub const COMBAT_FRAME_MS: u16 = 100;
pub const PLAYER_MOVE_SPEED_UNITS_PER_SECOND: u16 = 260;
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
    alive: bool,
    moving: bool,
    movement_intent: MovementIntent,
    queued_primary: bool,
    queued_cast_slot: Option<u8>,
    melee: MeleeDefinition,
    skills: [Option<SkillDefinition>; 5],
    primary_cooldown_remaining_ms: u16,
    slot_cooldown_remaining_ms: [u16; 5],
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
                        alive: true,
                        moving: false,
                        movement_intent: MovementIntent::zero(),
                        queued_primary: false,
                        queued_cast_slot: None,
                        melee: player.melee,
                        skills: player.skills,
                        primary_cooldown_remaining_ms: 0,
                        slot_cooldown_remaining_ms: [0; 5],
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

    fn advance_cooldowns(&mut self, delta_ms: u16) {
        for player in self.players.values_mut() {
            player.primary_cooldown_remaining_ms = player
                .primary_cooldown_remaining_ms
                .saturating_sub(delta_ms);
            for remaining in &mut player.slot_cooldown_remaining_ms {
                *remaining = remaining.saturating_sub(delta_ms);
            }
        }
    }

    fn apply_status_ticks(&mut self, delta_ms: u16, events: &mut Vec<SimulationEvent>) {
        let player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in player_ids {
            if !self
                .players
                .get(&player_id)
                .is_some_and(|player| player.alive)
            {
                continue;
            }

            let mut pending_effects = Vec::new();
            {
                let Some(player) = self.players.get_mut(&player_id) else {
                    continue;
                };
                let mut retained_statuses = Vec::with_capacity(player.statuses.len());
                for mut status in std::mem::take(&mut player.statuses) {
                    status.remaining_ms = status.remaining_ms.saturating_sub(delta_ms);
                    if let Some(interval_ms) = status.tick_interval_ms {
                        status.tick_progress_ms = status.tick_progress_ms.saturating_add(delta_ms);
                        while status.tick_progress_ms >= interval_ms {
                            status.tick_progress_ms =
                                status.tick_progress_ms.saturating_sub(interval_ms);
                            pending_effects.push((
                                status.source,
                                status.kind,
                                status.magnitude.saturating_mul(u16::from(status.stacks)),
                            ));
                        }
                    }
                    if status.remaining_ms > 0 {
                        retained_statuses.push(status);
                    }
                }
                player.statuses = retained_statuses;
            }

            for (source, kind, amount) in pending_effects {
                match kind {
                    StatusKind::Poison => {
                        events.extend(self.apply_damage_internal(source, &[player_id], amount));
                    }
                    StatusKind::Hot => {
                        events.extend(self.apply_healing_internal(source, &[player_id], amount));
                    }
                    StatusKind::Chill | StatusKind::Root => {}
                }
            }
        }
    }

    fn move_players(&mut self, delta_ms: u16, events: &mut Vec<SimulationEvent>) {
        let arena_width_units = self.arena_width_units;
        let arena_height_units = self.arena_height_units;
        let obstacles = self.obstacles.clone();

        for (player_id, player) in &mut self.players {
            if !player.alive {
                continue;
            }

            let rooted = player
                .statuses
                .iter()
                .any(|status| status.kind == StatusKind::Root);
            let movement = if rooted {
                MovementIntent::zero()
            } else {
                player.movement_intent
            };
            player.moving = movement != MovementIntent::zero();
            if !player.moving {
                continue;
            }

            let slow_bps = total_slow_bps(&player.statuses);
            let speed = adjusted_move_speed(delta_ms, slow_bps);
            if speed == 0 {
                continue;
            }

            let (delta_x, delta_y) = movement_delta(movement, speed);
            let next_x = i32::from(player.x) + delta_x;
            let next_y = i32::from(player.y) + delta_y;
            let (resolved_x, resolved_y) = resolve_movement(
                player.x,
                player.y,
                next_x,
                next_y,
                arena_width_units,
                arena_height_units,
                &obstacles,
            );

            if resolved_x != player.x || resolved_y != player.y {
                player.x = resolved_x;
                player.y = resolved_y;
                events.push(SimulationEvent::PlayerMoved {
                    player_id: *player_id,
                    x: player.x,
                    y: player.y,
                });
            }
        }
    }

    fn resolve_queued_actions(&mut self, events: &mut Vec<SimulationEvent>) {
        let player_ids = self.players.keys().copied().collect::<Vec<_>>();
        for player_id in player_ids {
            let Some(snapshot) = self.player_state(player_id) else {
                continue;
            };
            if !snapshot.alive {
                continue;
            }

            let queued_primary = self
                .players
                .get(&player_id)
                .is_some_and(|player| player.queued_primary);
            let queued_cast_slot = self
                .players
                .get(&player_id)
                .and_then(|player| player.queued_cast_slot);

            if queued_primary {
                events.extend(self.resolve_primary_attack(player_id, snapshot));
            }
            if let Some(slot) = queued_cast_slot {
                events.extend(self.resolve_cast(player_id, snapshot, slot));
            }

            if let Some(player) = self.players.get_mut(&player_id) {
                player.queued_primary = false;
                player.queued_cast_slot = None;
            }
        }
    }

    fn resolve_primary_attack(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
    ) -> Vec<SimulationEvent> {
        let Some(player) = self.players.get(&attacker) else {
            return Vec::new();
        };
        if player.primary_cooldown_remaining_ms > 0 {
            return Vec::new();
        }

        let melee = player.melee.clone();
        let target_point = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            melee.range,
        );
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: arena_effect_kind(melee.effect),
                owner: attacker,
                slot: 0,
                x: attacker_state.x,
                y: attacker_state.y,
                target_x: target_point.0,
                target_y: target_point.1,
                radius: melee.radius,
            },
        }];

        if let Some(player) = self.players.get_mut(&attacker) {
            player.primary_cooldown_remaining_ms = melee.cooldown_ms;
        }
        if let Some(target) =
            self.find_closest_player_near_point(attacker, target_point, melee.radius)
        {
            events.extend(self.apply_payload(attacker, 0, &[target], melee.payload));
        }

        events
    }

    #[allow(clippy::too_many_lines)]
    fn resolve_cast(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
    ) -> Vec<SimulationEvent> {
        let slot_index = usize::from(slot - 1);
        let Some(player) = self.players.get(&attacker) else {
            return Vec::new();
        };
        if player.slot_cooldown_remaining_ms[slot_index] > 0 {
            return Vec::new();
        }

        let Some(skill) = player.skills[slot_index].clone() else {
            return Vec::new();
        };
        match skill.behavior {
            SkillBehavior::Projectile {
                cooldown_ms,
                speed,
                range,
                radius,
                effect,
                payload,
            } => {
                if let Some(player) = self.players.get_mut(&attacker) {
                    player.slot_cooldown_remaining_ms[slot_index] = cooldown_ms;
                }
                self.spawn_projectile(
                    attacker,
                    slot,
                    attacker_state,
                    speed,
                    range,
                    radius,
                    arena_effect_kind(effect),
                    payload,
                )
            }
            SkillBehavior::Beam {
                cooldown_ms,
                range,
                radius,
                effect,
                payload,
            } => {
                if let Some(player) = self.players.get_mut(&attacker) {
                    player.slot_cooldown_remaining_ms[slot_index] = cooldown_ms;
                }
                self.cast_beam_skill(
                    attacker,
                    attacker_state,
                    slot,
                    range,
                    radius,
                    arena_effect_kind(effect),
                    payload,
                )
            }
            SkillBehavior::Dash {
                cooldown_ms,
                distance,
                effect,
                impact_radius,
                payload,
            } => {
                if let Some(player) = self.players.get_mut(&attacker) {
                    player.slot_cooldown_remaining_ms[slot_index] = cooldown_ms;
                }
                self.cast_dash_skill(
                    attacker,
                    attacker_state,
                    slot,
                    distance,
                    arena_effect_kind(effect),
                    impact_radius,
                    payload,
                )
            }
            SkillBehavior::Burst {
                cooldown_ms,
                range,
                radius,
                effect,
                payload,
            } => {
                if let Some(player) = self.players.get_mut(&attacker) {
                    player.slot_cooldown_remaining_ms[slot_index] = cooldown_ms;
                }
                self.cast_burst_skill(
                    attacker,
                    attacker_state,
                    slot,
                    range,
                    radius,
                    arena_effect_kind(effect),
                    payload,
                )
            }
            SkillBehavior::Nova {
                cooldown_ms,
                radius,
                effect,
                payload,
            } => {
                if let Some(player) = self.players.get_mut(&attacker) {
                    player.slot_cooldown_remaining_ms[slot_index] = cooldown_ms;
                }
                self.cast_nova_skill(
                    attacker,
                    attacker_state,
                    slot,
                    radius,
                    arena_effect_kind(effect),
                    payload,
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_projectile(
        &mut self,
        attacker: PlayerId,
        slot: u8,
        attacker_state: SimPlayerState,
        speed: u16,
        range: u16,
        radius: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        let direction = normalize_aim(attacker_state.aim_x, attacker_state.aim_y);
        let spawn_distance =
            i16::try_from(i32::from(PLAYER_RADIUS_UNITS) + i32::from(radius)).unwrap_or(i16::MAX);
        let start_x = saturating_i16(
            i32::from(attacker_state.x) + round_f32_to_i32(direction.0 * f32::from(spawn_distance)),
        );
        let start_y = saturating_i16(
            i32::from(attacker_state.y) + round_f32_to_i32(direction.1 * f32::from(spawn_distance)),
        );
        let projectile = ProjectileState {
            owner: attacker,
            slot,
            kind: effect_kind,
            x: start_x,
            y: start_y,
            direction_x: direction.0,
            direction_y: direction.1,
            speed_units_per_second: speed,
            remaining_range_units: i32::from(range),
            radius,
            payload,
        };
        self.projectiles.push(projectile);
        vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: effect_kind,
                owner: attacker,
                slot,
                x: start_x,
                y: start_y,
                target_x: start_x,
                target_y: start_y,
                radius,
            },
        }]
    }

    fn cast_beam_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        range: u16,
        radius: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        let desired_end = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            range,
        );
        let end = truncate_line_to_obstacles(
            (attacker_state.x, attacker_state.y),
            desired_end,
            &self.obstacles,
        );
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: effect_kind,
                owner: attacker,
                slot,
                x: attacker_state.x,
                y: attacker_state.y,
                target_x: end.0,
                target_y: end.1,
                radius,
            },
        }];

        if let Some(target) = self.find_first_player_on_segment(
            attacker,
            (attacker_state.x, attacker_state.y),
            end,
            radius,
        ) {
            events.extend(self.apply_payload(attacker, slot, &[target], payload));
        }
        events
    }

    fn cast_dash_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        distance: u16,
        effect_kind: ArenaEffectKind,
        impact_radius: Option<u16>,
        payload: Option<game_content::EffectPayload>,
    ) -> Vec<SimulationEvent> {
        let desired = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            distance,
        );
        let (resolved_x, resolved_y) = resolve_movement(
            attacker_state.x,
            attacker_state.y,
            i32::from(desired.0),
            i32::from(desired.1),
            self.arena_width_units,
            self.arena_height_units,
            &self.obstacles,
        );
        if let Some(player) = self.players.get_mut(&attacker) {
            player.x = resolved_x;
            player.y = resolved_y;
            player.moving = false;
        }

        let mut events = vec![
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: effect_kind,
                    owner: attacker,
                    slot,
                    x: attacker_state.x,
                    y: attacker_state.y,
                    target_x: resolved_x,
                    target_y: resolved_y,
                    radius: PLAYER_RADIUS_UNITS,
                },
            },
            SimulationEvent::PlayerMoved {
                player_id: attacker,
                x: resolved_x,
                y: resolved_y,
            },
        ];

        if let (Some(radius), Some(payload)) = (impact_radius, payload) {
            let targets =
                self.find_players_in_radius((resolved_x, resolved_y), radius, Some(attacker));
            events.extend(self.apply_payload(attacker, slot, &targets, payload));
        }

        events
    }

    fn cast_burst_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        range: u16,
        radius: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        let desired_center = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            range,
        );
        let center = truncate_line_to_obstacles(
            (attacker_state.x, attacker_state.y),
            desired_center,
            &self.obstacles,
        );
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: effect_kind,
                owner: attacker,
                slot,
                x: center.0,
                y: center.1,
                target_x: center.0,
                target_y: center.1,
                radius,
            },
        }];
        let targets = self.find_players_in_radius(center, radius, None);
        events.extend(self.apply_payload(attacker, slot, &targets, payload));
        events
    }

    fn cast_nova_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        radius: u16,
        effect_kind: ArenaEffectKind,
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        let center = (attacker_state.x, attacker_state.y);
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: effect_kind,
                owner: attacker,
                slot,
                x: center.0,
                y: center.1,
                target_x: center.0,
                target_y: center.1,
                radius,
            },
        }];
        let targets = self.find_players_in_radius(center, radius, None);
        events.extend(self.apply_payload(attacker, slot, &targets, payload));
        events
    }

    fn advance_projectiles(&mut self, delta_ms: u16, events: &mut Vec<SimulationEvent>) {
        let mut next_projectiles = Vec::new();
        let projectiles = std::mem::take(&mut self.projectiles);
        for projectile in projectiles {
            let step_distance = travel_distance_units(projectile.speed_units_per_second, delta_ms);
            if step_distance == 0 {
                next_projectiles.push(projectile);
                continue;
            }

            let desired_x =
                f32::from(projectile.x) + projectile.direction_x * f32::from(step_distance);
            let desired_y =
                f32::from(projectile.y) + projectile.direction_y * f32::from(step_distance);
            let desired_end = (
                saturating_i16(round_f32_to_i32(desired_x)),
                saturating_i16(round_f32_to_i32(desired_y)),
            );
            let clipped_end = truncate_line_to_obstacles(
                (projectile.x, projectile.y),
                desired_end,
                &self.obstacles,
            );
            let target = self.find_first_player_on_segment(
                projectile.owner,
                (projectile.x, projectile.y),
                clipped_end,
                projectile.radius,
            );
            if let Some(target) = target {
                events.extend(self.apply_payload(
                    projectile.owner,
                    projectile.slot,
                    &[target],
                    projectile.payload,
                ));
                events.push(SimulationEvent::EffectSpawned {
                    effect: ArenaEffect {
                        kind: ArenaEffectKind::HitSpark,
                        owner: projectile.owner,
                        slot: projectile.slot,
                        x: clipped_end.0,
                        y: clipped_end.1,
                        target_x: clipped_end.0,
                        target_y: clipped_end.1,
                        radius: projectile.radius.saturating_mul(2),
                    },
                });
                continue;
            }

            let traveled = point_distance_units((projectile.x, projectile.y), clipped_end);
            let remaining_range = projectile
                .remaining_range_units
                .saturating_sub(i32::from(traveled));
            let blocked = clipped_end != desired_end;
            if remaining_range <= 0 || blocked {
                if blocked {
                    events.push(SimulationEvent::EffectSpawned {
                        effect: ArenaEffect {
                            kind: ArenaEffectKind::HitSpark,
                            owner: projectile.owner,
                            slot: projectile.slot,
                            x: clipped_end.0,
                            y: clipped_end.1,
                            target_x: clipped_end.0,
                            target_y: clipped_end.1,
                            radius: projectile.radius.saturating_mul(2),
                        },
                    });
                }
                continue;
            }

            next_projectiles.push(ProjectileState {
                x: clipped_end.0,
                y: clipped_end.1,
                remaining_range_units: remaining_range,
                ..projectile
            });
        }

        self.projectiles = next_projectiles;
    }

    fn apply_payload(
        &mut self,
        source: PlayerId,
        slot: u8,
        targets: &[PlayerId],
        payload: game_content::EffectPayload,
    ) -> Vec<SimulationEvent> {
        if targets.is_empty() {
            return Vec::new();
        }

        let mut events = match payload.kind {
            CombatValueKind::Damage => self.apply_damage_internal(source, targets, payload.amount),
            CombatValueKind::Heal => self.apply_healing_internal(source, targets, payload.amount),
        };

        if let Some(status) = payload.status {
            for target in targets {
                if let Some(event) = self.apply_status(source, *target, slot, status) {
                    events.push(event);
                }
            }
        }

        events
    }

    fn apply_damage_internal(
        &mut self,
        attacker: PlayerId,
        targets: &[PlayerId],
        amount: u16,
    ) -> Vec<SimulationEvent> {
        if amount == 0 {
            return Vec::new();
        }

        let mut events = Vec::new();
        for target in targets {
            let Some(player) = self.players.get_mut(target) else {
                continue;
            };
            if !player.alive {
                continue;
            }

            let damage = amount.min(player.hit_points);
            player.hit_points = player.hit_points.saturating_sub(damage);
            let defeated = player.hit_points == 0;
            if defeated {
                player.alive = false;
                player.moving = false;
                player.movement_intent = MovementIntent::zero();
                player.statuses.clear();
            }

            events.push(SimulationEvent::DamageApplied {
                attacker,
                target: *target,
                amount: damage,
                remaining_hit_points: player.hit_points,
                defeated,
            });
        }

        events
    }

    fn apply_healing_internal(
        &mut self,
        source: PlayerId,
        targets: &[PlayerId],
        amount: u16,
    ) -> Vec<SimulationEvent> {
        if amount == 0 {
            return Vec::new();
        }

        let mut events = Vec::new();
        for target in targets {
            let Some(player) = self.players.get_mut(target) else {
                continue;
            };
            if !player.alive {
                continue;
            }

            let missing = player.max_hit_points.saturating_sub(player.hit_points);
            let healed = amount.min(missing);
            player.hit_points = player.hit_points.saturating_add(healed);
            events.push(SimulationEvent::HealingApplied {
                source,
                target: *target,
                amount: healed,
                resulting_hit_points: player.hit_points,
            });
        }
        events
    }

    fn apply_status(
        &mut self,
        source: PlayerId,
        target: PlayerId,
        slot: u8,
        definition: StatusDefinition,
    ) -> Option<SimulationEvent> {
        let player = self.players.get_mut(&target)?;
        if !player.alive {
            return None;
        }

        let mut stacks_after = 1_u8;
        if let Some(existing) = player.statuses.iter_mut().find(|status| {
            status.source == source && status.slot == slot && status.kind == definition.kind
        }) {
            existing.stacks = existing.stacks.saturating_add(1).min(existing.max_stacks);
            existing.remaining_ms = definition.duration_ms;
            existing.tick_progress_ms = 0;
            existing.magnitude = definition.magnitude;
            existing.trigger_duration_ms = definition.trigger_duration_ms;
            stacks_after = existing.stacks;
        } else {
            player.statuses.push(StatusInstance {
                source,
                slot,
                kind: definition.kind,
                stacks: 1,
                remaining_ms: definition.duration_ms,
                tick_interval_ms: definition.tick_interval_ms,
                tick_progress_ms: 0,
                magnitude: definition.magnitude,
                max_stacks: definition.max_stacks,
                trigger_duration_ms: definition.trigger_duration_ms,
            });
        }

        if definition.kind == StatusKind::Chill
            && stacks_after >= definition.max_stacks
            && definition.trigger_duration_ms.is_some()
        {
            let root_duration = definition.trigger_duration_ms.unwrap_or(0);
            self.apply_status(
                source,
                target,
                slot,
                StatusDefinition {
                    kind: StatusKind::Root,
                    duration_ms: root_duration,
                    tick_interval_ms: None,
                    magnitude: 0,
                    max_stacks: 1,
                    trigger_duration_ms: None,
                },
            );
        }

        Some(SimulationEvent::StatusApplied {
            source,
            target,
            slot,
            kind: definition.kind,
            stacks: stacks_after,
            remaining_ms: definition.duration_ms,
        })
    }

    fn find_closest_player_near_point(
        &self,
        attacker: PlayerId,
        point: (i16, i16),
        radius: u16,
    ) -> Option<PlayerId> {
        let max_distance_sq = i32::from(radius) * i32::from(radius);
        self.players
            .iter()
            .filter(|(player_id, player)| **player_id != attacker && player.alive)
            .filter_map(|(player_id, player)| {
                let distance_sq = point_distance_sq(point, (player.x, player.y));
                (distance_sq <= max_distance_sq).then_some((*player_id, distance_sq))
            })
            .min_by_key(|(_, distance_sq)| *distance_sq)
            .map(|(player_id, _)| player_id)
    }

    fn find_first_player_on_segment(
        &self,
        attacker: PlayerId,
        start: (i16, i16),
        end: (i16, i16),
        radius: u16,
    ) -> Option<PlayerId> {
        let threshold_sq = f32::from(radius) * f32::from(radius);
        self.players
            .iter()
            .filter(|(player_id, player)| **player_id != attacker && player.alive)
            .filter_map(|(player_id, player)| {
                let point = (player.x, player.y);
                let distance_sq = segment_distance_sq(start, end, point);
                (distance_sq <= threshold_sq)
                    .then_some((*player_id, point_distance_sq(start, point)))
            })
            .min_by_key(|(_, distance_sq)| *distance_sq)
            .map(|(player_id, _)| player_id)
    }

    fn find_players_in_radius(
        &self,
        center: (i16, i16),
        radius: u16,
        exclude: Option<PlayerId>,
    ) -> Vec<PlayerId> {
        let max_distance_sq = i32::from(radius) * i32::from(radius);
        self.players
            .iter()
            .filter(|(player_id, player)| Some(**player_id) != exclude && player.alive)
            .filter_map(|(player_id, player)| {
                let distance_sq = point_distance_sq(center, (player.x, player.y));
                (distance_sq <= max_distance_sq).then_some(*player_id)
            })
            .collect()
    }
}

fn spawn_position(team: TeamSide, index: u16, map: &ArenaMapDefinition) -> (i16, i16, i16) {
    let horizontal_offset = i16::try_from(index).unwrap_or(i16::MAX) * SPAWN_SPACING_UNITS;
    match team {
        TeamSide::TeamA => (
            map.team_a_anchor.0,
            map.team_a_anchor.1 + horizontal_offset,
            DEFAULT_AIM_X,
        ),
        TeamSide::TeamB => (
            map.team_b_anchor.0,
            map.team_b_anchor.1 + horizontal_offset,
            -DEFAULT_AIM_X,
        ),
    }
}

fn map_obstacle_to_sim_obstacle(obstacle: &ArenaMapObstacle) -> ArenaObstacle {
    ArenaObstacle {
        kind: match obstacle.kind {
            ArenaMapObstacleKind::Pillar => ArenaObstacleKind::Pillar,
            ArenaMapObstacleKind::Shrub => ArenaObstacleKind::Shrub,
        },
        center_x: obstacle.center_x,
        center_y: obstacle.center_y,
        half_width: obstacle.half_width,
        half_height: obstacle.half_height,
    }
}

fn arena_effect_kind(kind: SkillEffectKind) -> ArenaEffectKind {
    match kind {
        SkillEffectKind::MeleeSwing => ArenaEffectKind::MeleeSwing,
        SkillEffectKind::SkillShot => ArenaEffectKind::SkillShot,
        SkillEffectKind::DashTrail => ArenaEffectKind::DashTrail,
        SkillEffectKind::Burst => ArenaEffectKind::Burst,
        SkillEffectKind::Nova => ArenaEffectKind::Nova,
        SkillEffectKind::Beam => ArenaEffectKind::Beam,
        SkillEffectKind::HitSpark => ArenaEffectKind::HitSpark,
    }
}

fn total_slow_bps(statuses: &[StatusInstance]) -> u16 {
    statuses
        .iter()
        .filter(|status| status.kind == StatusKind::Chill)
        .fold(0_u16, |accumulator, status| {
            let slow = status
                .magnitude
                .saturating_mul(u16::from(status.stacks))
                .min(8_000);
            accumulator.saturating_add(slow).min(8_000)
        })
}

fn adjusted_move_speed(delta_ms: u16, slow_bps: u16) -> u16 {
    let effective_speed = u32::from(PLAYER_MOVE_SPEED_UNITS_PER_SECOND)
        .saturating_mul(u32::from(10_000_u16.saturating_sub(slow_bps)))
        / 10_000;
    let distance = effective_speed.saturating_mul(u32::from(delta_ms)) / 1000;
    u16::try_from(distance).unwrap_or(u16::MAX)
}

fn travel_distance_units(speed_units_per_second: u16, delta_ms: u16) -> u16 {
    let distance = u32::from(speed_units_per_second).saturating_mul(u32::from(delta_ms)) / 1000;
    u16::try_from(distance).unwrap_or(u16::MAX)
}

fn movement_delta(intent: MovementIntent, speed: u16) -> (i32, i32) {
    let mut x = f32::from(intent.x);
    let mut y = f32::from(intent.y);
    let length = (x * x + y * y).sqrt();
    if length > f32::EPSILON {
        x /= length;
        y /= length;
    }

    (
        round_f32_to_i32(x * f32::from(speed)),
        round_f32_to_i32(y * f32::from(speed)),
    )
}

fn resolve_movement(
    start_x: i16,
    start_y: i16,
    desired_x: i32,
    desired_y: i32,
    arena_width_units: u16,
    arena_height_units: u16,
    obstacles: &[ArenaObstacle],
) -> (i16, i16) {
    let min_x = -i32::from(arena_width_units / 2);
    let max_x = i32::from(arena_width_units / 2);
    let min_y = -i32::from(arena_height_units / 2);
    let max_y = i32::from(arena_height_units / 2);
    let candidate_x = desired_x.clamp(
        min_x + i32::from(PLAYER_RADIUS_UNITS),
        max_x - i32::from(PLAYER_RADIUS_UNITS),
    );
    let candidate_y = desired_y.clamp(
        min_y + i32::from(PLAYER_RADIUS_UNITS),
        max_y - i32::from(PLAYER_RADIUS_UNITS),
    );

    let mut resolved_x = saturating_i16(candidate_x);
    let mut resolved_y = saturating_i16(candidate_y);
    if obstacles.iter().any(|obstacle| {
        circle_intersects_rect(resolved_x, resolved_y, PLAYER_RADIUS_UNITS, obstacle)
    }) {
        resolved_x = start_x;
        resolved_y = start_y;
    }

    (resolved_x, resolved_y)
}

fn circle_intersects_rect(x: i16, y: i16, radius: u16, obstacle: &ArenaObstacle) -> bool {
    let left = obstacle.center_x - i16::try_from(obstacle.half_width).unwrap_or(i16::MAX);
    let right = obstacle.center_x + i16::try_from(obstacle.half_width).unwrap_or(i16::MAX);
    let top = obstacle.center_y - i16::try_from(obstacle.half_height).unwrap_or(i16::MAX);
    let bottom = obstacle.center_y + i16::try_from(obstacle.half_height).unwrap_or(i16::MAX);
    let closest_x = x.clamp(left, right);
    let closest_y = y.clamp(top, bottom);
    let dx = i32::from(x - closest_x);
    let dy = i32::from(y - closest_y);
    dx * dx + dy * dy <= i32::from(radius) * i32::from(radius)
}

fn truncate_line_to_obstacles(
    start: (i16, i16),
    end: (i16, i16),
    obstacles: &[ArenaObstacle],
) -> (i16, i16) {
    let mut closest_t = 1.0_f32;
    for obstacle in obstacles {
        if let Some(intersection_t) = segment_rect_intersection_t(start, end, obstacle) {
            if intersection_t < closest_t {
                closest_t = intersection_t;
            }
        }
    }

    if closest_t >= 1.0 {
        return end;
    }

    let start_x = f32::from(start.0);
    let start_y = f32::from(start.1);
    let delta_x = f32::from(end.0) - start_x;
    let delta_y = f32::from(end.1) - start_y;
    let clipped_t = (closest_t - 0.01).clamp(0.0, 1.0);
    (
        saturating_i16(round_f32_to_i32(start_x + delta_x * clipped_t)),
        saturating_i16(round_f32_to_i32(start_y + delta_y * clipped_t)),
    )
}

fn segment_rect_intersection_t(
    start: (i16, i16),
    end: (i16, i16),
    obstacle: &ArenaObstacle,
) -> Option<f32> {
    let start_x = f32::from(start.0);
    let start_y = f32::from(start.1);
    let delta_x = f32::from(end.0 - start.0);
    let delta_y = f32::from(end.1 - start.1);
    let min_x = f32::from(obstacle.center_x) - f32::from(obstacle.half_width);
    let max_x = f32::from(obstacle.center_x) + f32::from(obstacle.half_width);
    let min_y = f32::from(obstacle.center_y) - f32::from(obstacle.half_height);
    let max_y = f32::from(obstacle.center_y) + f32::from(obstacle.half_height);
    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    if !update_segment_slab(start_x, delta_x, min_x, max_x, &mut t_min, &mut t_max) {
        return None;
    }
    if !update_segment_slab(start_y, delta_y, min_y, max_y, &mut t_min, &mut t_max) {
        return None;
    }

    if t_min <= 0.0 || t_min > 1.0 {
        None
    } else {
        Some(t_min)
    }
}

fn update_segment_slab(
    start: f32,
    delta: f32,
    min_bound: f32,
    max_bound: f32,
    t_min: &mut f32,
    t_max: &mut f32,
) -> bool {
    if delta.abs() <= f32::EPSILON {
        return start >= min_bound && start <= max_bound;
    }

    let inverse_delta = 1.0 / delta;
    let mut entry = (min_bound - start) * inverse_delta;
    let mut exit = (max_bound - start) * inverse_delta;
    if entry > exit {
        std::mem::swap(&mut entry, &mut exit);
    }
    *t_min = (*t_min).max(entry);
    *t_max = (*t_max).min(exit);
    *t_min <= *t_max
}

fn project_from_aim(
    origin_x: i16,
    origin_y: i16,
    aim_x: i16,
    aim_y: i16,
    distance: u16,
) -> (i16, i16) {
    let direction = normalize_aim(aim_x, aim_y);
    let projected_x = f32::from(origin_x) + direction.0 * f32::from(distance);
    let projected_y = f32::from(origin_y) + direction.1 * f32::from(distance);
    (
        saturating_i16(round_f32_to_i32(projected_x)),
        saturating_i16(round_f32_to_i32(projected_y)),
    )
}

fn normalize_aim(aim_x: i16, aim_y: i16) -> (f32, f32) {
    let raw_x = f32::from(aim_x);
    let raw_y = f32::from(aim_y);
    let length = (raw_x * raw_x + raw_y * raw_y).sqrt();
    if length <= f32::EPSILON {
        return (1.0, 0.0);
    }
    (raw_x / length, raw_y / length)
}

fn point_distance_sq(a: (i16, i16), b: (i16, i16)) -> i32 {
    let dx = i32::from(a.0) - i32::from(b.0);
    let dy = i32::from(a.1) - i32::from(b.1);
    dx * dx + dy * dy
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn point_distance_units(a: (i16, i16), b: (i16, i16)) -> u16 {
    let distance = ((point_distance_sq(a, b)) as f32).sqrt().round();
    u16::try_from(distance as i32).unwrap_or(u16::MAX)
}

fn segment_distance_sq(start: (i16, i16), end: (i16, i16), point: (i16, i16)) -> f32 {
    let ax = f32::from(start.0);
    let ay = f32::from(start.1);
    let bx = f32::from(end.0);
    let by = f32::from(end.1);
    let px = f32::from(point.0);
    let py = f32::from(point.1);
    let ab_x = bx - ax;
    let ab_y = by - ay;
    let ab_len_sq = ab_x * ab_x + ab_y * ab_y;
    if ab_len_sq <= f32::EPSILON {
        let dx = px - ax;
        let dy = py - ay;
        return dx * dx + dy * dy;
    }

    let ap_x = px - ax;
    let ap_y = py - ay;
    let t = ((ap_x * ab_x + ap_y * ab_y) / ab_len_sq).clamp(0.0, 1.0);
    let nearest_x = ax + ab_x * t;
    let nearest_y = ay + ab_y * t;
    let dx = px - nearest_x;
    let dy = py - nearest_y;
    dx * dx + dy * dy
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn round_f32_to_i32(value: f32) -> i32 {
    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

fn saturating_i16(value: i32) -> i16 {
    let clamped = value.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
    i16::try_from(clamped).unwrap_or(if clamped < 0 { i16::MIN } else { i16::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_content::GameContent;
    use game_domain::{PlayerName, PlayerRecord, SkillChoice, SkillTree};

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
                .melee_for(primary_tree)
                .expect("melee should exist")
                .clone(),
            skills: choices.map(|value| {
                value.and_then(|skill_choice| content.skills().resolve(skill_choice).cloned())
            }),
        }
    }

    fn world(content: &GameContent, seeds: Vec<SimPlayerSeed>) -> SimulationWorld {
        SimulationWorld::new(seeds, content.map()).expect("world should build")
    }

    #[test]
    fn movement_stops_on_pillars_and_players_are_circles() {
        let content = content();
        let mut world = world(
            &content,
            vec![seed(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Warrior,
                [None, None, None, None, None],
            )],
        );
        let shrub = *world
            .obstacles()
            .iter()
            .find(|obstacle| obstacle.kind == ArenaObstacleKind::Shrub)
            .expect("shrub exists");
        {
            let player = world.players.get_mut(&player_id(1)).expect("player");
            player.x = shrub.center_x
                - i16::try_from(shrub.half_width).expect("fits")
                - i16::try_from(PLAYER_RADIUS_UNITS).expect("fits")
                - 30;
            player.y = shrub.center_y;
        }
        world
            .submit_input(player_id(1), MovementIntent::new(1, 0).expect("intent"))
            .expect("input");
        for _ in 0..10 {
            let _ = world.tick(COMBAT_FRAME_MS);
        }
        let state = world.player_state(player_id(1)).expect("player");
        assert!(
            state.x
                <= shrub.center_x
                    - i16::try_from(shrub.half_width).expect("fits")
                    - i16::try_from(PLAYER_RADIUS_UNITS).expect("fits")
        );
    }

    #[test]
    fn melee_uses_class_stats_and_respects_cooldown() {
        let content = content();
        let mut world = world(
            &content,
            vec![
                seed(
                    &content,
                    1,
                    "Alice",
                    TeamSide::TeamA,
                    SkillTree::Rogue,
                    [Some(choice(SkillTree::Rogue, 1)), None, None, None, None],
                ),
                seed(
                    &content,
                    2,
                    "Bob",
                    TeamSide::TeamB,
                    SkillTree::Warrior,
                    [Some(choice(SkillTree::Warrior, 1)), None, None, None, None],
                ),
            ],
        );
        let alice = world.player_state(player_id(1)).expect("alice");
        {
            let bob = world.players.get_mut(&player_id(2)).expect("bob");
            bob.x = alice.x + 60;
            bob.y = alice.y;
        }

        world
            .queue_primary_attack(player_id(1))
            .expect("melee queue");
        let events = world.tick(COMBAT_FRAME_MS);
        assert!(events.iter().any(|event| matches!(event, SimulationEvent::DamageApplied { attacker, target, amount: 14, .. } if *attacker == player_id(1) && *target == player_id(2))));

        world
            .queue_primary_attack(player_id(1))
            .expect("cooldown queue");
        let events = world.tick(COMBAT_FRAME_MS);
        assert!(!events.iter().any(|event| matches!(event, SimulationEvent::DamageApplied { target, .. } if *target == player_id(2))));
    }

    #[test]
    fn skill_cooldown_state_counts_down_before_a_second_cast_can_land() {
        let content = content();
        let mut world = world(
            &content,
            vec![
                seed(
                    &content,
                    1,
                    "Alice",
                    TeamSide::TeamA,
                    SkillTree::Mage,
                    [Some(choice(SkillTree::Mage, 1)), None, None, None, None],
                ),
                seed(
                    &content,
                    2,
                    "Bob",
                    TeamSide::TeamB,
                    SkillTree::Warrior,
                    [Some(choice(SkillTree::Warrior, 1)), None, None, None, None],
                ),
            ],
        );
        {
            let alice = world.players.get_mut(&player_id(1)).expect("alice");
            alice.x = -400;
            alice.y = 0;
            alice.aim_x = 100;
            alice.aim_y = 0;
            let bob = world.players.get_mut(&player_id(2)).expect("bob");
            bob.x = -200;
            bob.y = 0;
        }

        world.queue_cast(player_id(1), 1).expect("first cast");
        let _ = world.tick(COMBAT_FRAME_MS);
        let after_cast = world.player_state(player_id(1)).expect("alice");
        assert!(after_cast.slot_cooldown_remaining_ms[0] > 0);
        assert_eq!(after_cast.slot_cooldown_total_ms[0], 700);

        world
            .queue_cast(player_id(1), 1)
            .expect("second cast queue");
        let blocked_events = world.tick(COMBAT_FRAME_MS);
        assert!(!blocked_events.iter().any(|event| matches!(
            event,
            SimulationEvent::EffectSpawned { effect }
                if effect.owner == player_id(1) && effect.slot == 1
        )));

        for _ in 0..7 {
            let _ = world.tick(COMBAT_FRAME_MS);
        }
        let cooled_down = world.player_state(player_id(1)).expect("alice");
        assert_eq!(cooled_down.slot_cooldown_remaining_ms[0], 0);
    }

    #[test]
    fn projectiles_hit_and_miss_based_on_geometry() {
        let content = content();
        let mut world = world(
            &content,
            vec![
                seed(
                    &content,
                    1,
                    "Alice",
                    TeamSide::TeamA,
                    SkillTree::Mage,
                    [Some(choice(SkillTree::Mage, 1)), None, None, None, None],
                ),
                seed(
                    &content,
                    2,
                    "Bob",
                    TeamSide::TeamB,
                    SkillTree::Warrior,
                    [Some(choice(SkillTree::Warrior, 1)), None, None, None, None],
                ),
            ],
        );
        {
            let alice = world.players.get_mut(&player_id(1)).expect("alice");
            alice.x = -500;
            alice.y = 0;
            alice.aim_x = 100;
            alice.aim_y = 0;
            let bob = world.players.get_mut(&player_id(2)).expect("bob");
            bob.x = -250;
            bob.y = 0;
        }

        world.queue_cast(player_id(1), 1).expect("cast");
        let _ = world.tick(COMBAT_FRAME_MS);
        for _ in 0..10 {
            let events = world.tick(COMBAT_FRAME_MS);
            if events.iter().any(|event| matches!(event, SimulationEvent::DamageApplied { target, .. } if *target == player_id(2))) {
                return;
            }
        }
        panic!("projectile should hit bob in open space");
    }

    #[test]
    fn poison_and_hot_tick_with_expected_stacking_behavior() {
        let content = content();
        let mut world = world(
            &content,
            vec![
                seed(
                    &content,
                    1,
                    "Alice",
                    TeamSide::TeamA,
                    SkillTree::Rogue,
                    [Some(choice(SkillTree::Rogue, 1)), None, None, None, None],
                ),
                seed(
                    &content,
                    2,
                    "Bob",
                    TeamSide::TeamB,
                    SkillTree::Cleric,
                    [
                        Some(choice(SkillTree::Cleric, 1)),
                        Some(choice(SkillTree::Cleric, 2)),
                        Some(choice(SkillTree::Cleric, 3)),
                        None,
                        None,
                    ],
                ),
            ],
        );
        {
            let alice = world.players.get_mut(&player_id(1)).expect("alice");
            alice.x = -400;
            alice.y = 0;
            alice.aim_x = 100;
            alice.aim_y = 0;
            let bob = world.players.get_mut(&player_id(2)).expect("bob");
            bob.x = -240;
            bob.y = 0;
            bob.aim_x = 0;
            bob.aim_y = 1;
            bob.hit_points = 70;
        }

        world.queue_cast(player_id(1), 1).expect("poison cast");
        let _ = world.tick(COMBAT_FRAME_MS);
        for _ in 0..8 {
            let _ = world.tick(COMBAT_FRAME_MS);
        }
        let poison_statuses = world.statuses_for(player_id(2)).expect("statuses");
        assert!(poison_statuses
            .iter()
            .any(|status| status.kind == StatusKind::Poison));
        let damaged_hit_points = world.player_state(player_id(2)).expect("bob").hit_points;
        assert!(damaged_hit_points < 70);

        world.queue_cast(player_id(2), 3).expect("hot cast");
        let _ = world.tick(COMBAT_FRAME_MS);
        for _ in 0..10 {
            let _ = world.tick(COMBAT_FRAME_MS);
        }
        let hot_statuses = world.statuses_for(player_id(2)).expect("statuses");
        assert!(hot_statuses
            .iter()
            .any(|status| status.kind == StatusKind::Hot));
        let bob = world.player_state(player_id(2)).expect("bob");
        assert!(bob.hit_points > damaged_hit_points);
    }

    #[test]
    fn poison_stacks_and_hot_refreshes_from_the_same_source() {
        let content = content();
        let mut poison_world = world(
            &content,
            vec![
                seed(
                    &content,
                    1,
                    "Alice",
                    TeamSide::TeamA,
                    SkillTree::Rogue,
                    [Some(choice(SkillTree::Rogue, 1)), None, None, None, None],
                ),
                seed(
                    &content,
                    2,
                    "Bob",
                    TeamSide::TeamB,
                    SkillTree::Warrior,
                    [Some(choice(SkillTree::Warrior, 1)), None, None, None, None],
                ),
            ],
        );
        {
            let alice = poison_world.players.get_mut(&player_id(1)).expect("alice");
            alice.x = -360;
            alice.y = 0;
            alice.aim_x = 100;
            alice.aim_y = 0;
            let bob = poison_world.players.get_mut(&player_id(2)).expect("bob");
            bob.x = -200;
            bob.y = 0;
        }

        for _ in 0..2 {
            poison_world
                .queue_cast(player_id(1), 1)
                .expect("poison cast");
            let _ = poison_world.tick(COMBAT_FRAME_MS);
            for _ in 0..8 {
                let _ = poison_world.tick(COMBAT_FRAME_MS);
            }
        }

        let poison_statuses = poison_world.statuses_for(player_id(2)).expect("statuses");
        let poison = poison_statuses
            .iter()
            .find(|status| status.kind == StatusKind::Poison)
            .expect("poison should exist");
        assert_eq!(poison.stacks, 2);

        let mut hot_world = world(
            &content,
            vec![seed(
                &content,
                1,
                "Cleric",
                TeamSide::TeamA,
                SkillTree::Cleric,
                [None, None, Some(choice(SkillTree::Cleric, 3)), None, None],
            )],
        );
        {
            let cleric = hot_world.players.get_mut(&player_id(1)).expect("cleric");
            cleric.hit_points = 80;
        }

        hot_world.queue_cast(player_id(1), 3).expect("first hot");
        let _ = hot_world.tick(COMBAT_FRAME_MS);
        for _ in 0..18 {
            let _ = hot_world.tick(COMBAT_FRAME_MS);
        }
        let hot_before_refresh = hot_world
            .statuses_for(player_id(1))
            .expect("statuses")
            .into_iter()
            .find(|status| status.kind == StatusKind::Hot)
            .expect("hot should exist before refresh");

        for _ in 0..4 {
            let _ = hot_world.tick(COMBAT_FRAME_MS);
        }
        hot_world.queue_cast(player_id(1), 3).expect("refresh hot");
        let _ = hot_world.tick(COMBAT_FRAME_MS);
        let hot_after_refresh = hot_world
            .statuses_for(player_id(1))
            .expect("statuses")
            .into_iter()
            .find(|status| status.kind == StatusKind::Hot)
            .expect("hot should exist after refresh");
        assert_eq!(hot_after_refresh.stacks, 1);
        assert!(hot_after_refresh.remaining_ms >= hot_before_refresh.remaining_ms);
    }

    #[test]
    fn chill_stacks_slow_then_root_target() {
        let content = content();
        let mut world = world(
            &content,
            vec![
                seed(
                    &content,
                    1,
                    "Alice",
                    TeamSide::TeamA,
                    SkillTree::Mage,
                    [
                        Some(choice(SkillTree::Mage, 1)),
                        Some(choice(SkillTree::Mage, 2)),
                        Some(choice(SkillTree::Mage, 3)),
                        None,
                        None,
                    ],
                ),
                seed(
                    &content,
                    2,
                    "Bob",
                    TeamSide::TeamB,
                    SkillTree::Warrior,
                    [Some(choice(SkillTree::Warrior, 1)), None, None, None, None],
                ),
            ],
        );
        {
            let alice = world.players.get_mut(&player_id(1)).expect("alice");
            alice.x = -200;
            alice.y = 0;
            alice.aim_x = 100;
            alice.aim_y = 0;
            let bob = world.players.get_mut(&player_id(2)).expect("bob");
            bob.x = -20;
            bob.y = 0;
        }

        for stack in 1..=3 {
            world.queue_cast(player_id(1), 3).expect("burst");
            let _ = world.tick(COMBAT_FRAME_MS);
            let statuses = world.statuses_for(player_id(2)).expect("statuses");
            let chill = statuses
                .iter()
                .find(|status| status.kind == StatusKind::Chill)
                .expect("chill should be active after each burst");
            assert_eq!(chill.stacks, stack);
            let rooted = statuses
                .iter()
                .any(|status| status.kind == StatusKind::Root);
            assert_eq!(rooted, stack == 3);
            for _ in 0..20 {
                let _ = world.tick(COMBAT_FRAME_MS);
            }
        }
    }

    #[test]
    fn healing_can_affect_enemy_players_and_caps_at_max_hp() {
        let content = content();
        let mut world = world(
            &content,
            vec![
                seed(
                    &content,
                    1,
                    "Alice",
                    TeamSide::TeamA,
                    SkillTree::Cleric,
                    [Some(choice(SkillTree::Cleric, 1)), None, None, None, None],
                ),
                seed(
                    &content,
                    2,
                    "Bob",
                    TeamSide::TeamB,
                    SkillTree::Warrior,
                    [Some(choice(SkillTree::Warrior, 1)), None, None, None, None],
                ),
            ],
        );
        {
            let alice = world.players.get_mut(&player_id(1)).expect("alice");
            alice.x = -200;
            alice.y = 0;
            alice.aim_x = 100;
            alice.aim_y = 0;
            let bob = world.players.get_mut(&player_id(2)).expect("bob");
            bob.x = -80;
            bob.y = 0;
            bob.hit_points = 60;
        }

        world.queue_cast(player_id(1), 1).expect("heal");
        let events = world.tick(COMBAT_FRAME_MS);
        assert!(events.iter().any(|event| matches!(event, SimulationEvent::HealingApplied { target, .. } if *target == player_id(2))));
        let bob = world.player_state(player_id(2)).expect("bob");
        assert!(bob.hit_points > 60);
        assert!(bob.hit_points <= bob.max_hit_points);
    }
}
