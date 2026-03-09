//! Fixed-tick simulation, arena geometry, and placeholder combat resolution.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::collections::BTreeMap;
use std::fmt;

use game_domain::{PlayerId, TeamAssignment, TeamSide};

pub const ARENA_WIDTH_UNITS: u16 = 1800;
pub const ARENA_HEIGHT_UNITS: u16 = 1200;
pub const PLAYER_RADIUS_UNITS: u16 = 28;
pub const PLAYER_MOVE_SPEED_UNITS: i16 = 18;

const DEFAULT_AIM_X: i16 = 120;
const DEFAULT_AIM_Y: i16 = 0;
const MELEE_RANGE_UNITS: u16 = 110;
const MELEE_HIT_RADIUS_UNITS: u16 = 48;
const MELEE_DAMAGE: u16 = 40;
const SLOT1_RANGE_UNITS: u16 = 1600;
const SLOT1_DAMAGE: u16 = 100;
const SLOT2_DASH_DISTANCE_UNITS: u16 = 180;
const SLOT3_CAST_RANGE_UNITS: u16 = 260;
const SLOT3_RADIUS_UNITS: u16 = 96;
const SLOT3_DAMAGE: u16 = 42;
const SLOT4_RADIUS_UNITS: u16 = 132;
const SLOT4_DAMAGE: u16 = 36;
const SLOT5_RANGE_UNITS: u16 = 1600;
const SLOT5_DAMAGE: u16 = 100;
const PILLAR_HALF_EXTENT_UNITS: u16 = 70;
const SHRUB_HALF_EXTENT_UNITS: u16 = 92;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimPlayerSeed {
    pub assignment: TeamAssignment,
    pub hit_points: u16,
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
    DamageMustBePositive,
    InvalidSkillSlot(u8),
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
            Self::DamageMustBePositive => f.write_str("damage must be positive"),
            Self::InvalidSkillSlot(slot) => {
                write!(f, "skill slot {slot} is outside the supported range 1..=5")
            }
        }
    }
}

impl std::error::Error for SimulationError {}

#[derive(Clone, Debug)]
pub struct SimulationWorld {
    players: BTreeMap<PlayerId, SimPlayer>,
    pending_inputs: BTreeMap<PlayerId, MovementIntent>,
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
}

const ARENA_OBSTACLES: [ArenaObstacle; 8] = [
    obstacle(
        ArenaObstacleKind::Shrub,
        -220,
        -150,
        SHRUB_HALF_EXTENT_UNITS,
        SHRUB_HALF_EXTENT_UNITS,
    ),
    obstacle(
        ArenaObstacleKind::Pillar,
        -220,
        -150,
        PILLAR_HALF_EXTENT_UNITS,
        PILLAR_HALF_EXTENT_UNITS,
    ),
    obstacle(
        ArenaObstacleKind::Shrub,
        220,
        -150,
        SHRUB_HALF_EXTENT_UNITS,
        SHRUB_HALF_EXTENT_UNITS,
    ),
    obstacle(
        ArenaObstacleKind::Pillar,
        220,
        -150,
        PILLAR_HALF_EXTENT_UNITS,
        PILLAR_HALF_EXTENT_UNITS,
    ),
    obstacle(
        ArenaObstacleKind::Shrub,
        -220,
        150,
        SHRUB_HALF_EXTENT_UNITS,
        SHRUB_HALF_EXTENT_UNITS,
    ),
    obstacle(
        ArenaObstacleKind::Pillar,
        -220,
        150,
        PILLAR_HALF_EXTENT_UNITS,
        PILLAR_HALF_EXTENT_UNITS,
    ),
    obstacle(
        ArenaObstacleKind::Shrub,
        220,
        150,
        SHRUB_HALF_EXTENT_UNITS,
        SHRUB_HALF_EXTENT_UNITS,
    ),
    obstacle(
        ArenaObstacleKind::Pillar,
        220,
        150,
        PILLAR_HALF_EXTENT_UNITS,
        PILLAR_HALF_EXTENT_UNITS,
    ),
];

const fn obstacle(
    kind: ArenaObstacleKind,
    center_x: i16,
    center_y: i16,
    half_width: u16,
    half_height: u16,
) -> ArenaObstacle {
    ArenaObstacle {
        kind,
        center_x,
        center_y,
        half_width,
        half_height,
    }
}

impl SimulationWorld {
    pub fn new(players: Vec<SimPlayerSeed>) -> Result<Self, SimulationError> {
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
            let (spawn_x, spawn_y, aim_x) = spawn_position(player.assignment.team, spawn_index);

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
            players: world_players,
            pending_inputs: BTreeMap::new(),
        })
    }

    pub fn submit_input(
        &mut self,
        player_id: PlayerId,
        movement: MovementIntent,
    ) -> Result<(), SimulationError> {
        let player = self
            .players
            .get(&player_id)
            .ok_or(SimulationError::PlayerMissing(player_id))?;

        if !player.alive {
            return Err(SimulationError::PlayerAlreadyDefeated(player_id));
        }

        self.pending_inputs.insert(player_id, movement);
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

    pub fn tick(&mut self) -> Vec<SimulationEvent> {
        let mut events = Vec::new();

        for (player_id, player) in &mut self.players {
            if !player.alive {
                continue;
            }

            let movement = self
                .pending_inputs
                .remove(player_id)
                .unwrap_or_else(MovementIntent::zero);
            player.moving = movement != MovementIntent::zero();

            if !player.moving {
                continue;
            }

            let next_x =
                i32::from(player.x) + i32::from(movement.x) * i32::from(PLAYER_MOVE_SPEED_UNITS);
            let next_y =
                i32::from(player.y) + i32::from(movement.y) * i32::from(PLAYER_MOVE_SPEED_UNITS);
            let (resolved_x, resolved_y) = resolve_movement(player.x, player.y, next_x, next_y);

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

        events
    }

    pub fn melee_attack(
        &mut self,
        attacker: PlayerId,
    ) -> Result<Vec<SimulationEvent>, SimulationError> {
        let attacker_state = self.require_live_player(attacker)?;
        let target_point = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            MELEE_RANGE_UNITS,
        );
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: ArenaEffectKind::MeleeSwing,
                owner: attacker,
                slot: 0,
                x: attacker_state.x,
                y: attacker_state.y,
                target_x: target_point.0,
                target_y: target_point.1,
                radius: MELEE_HIT_RADIUS_UNITS,
            },
        }];

        if let Some(target) =
            self.find_closest_player_near_point(attacker, target_point, MELEE_HIT_RADIUS_UNITS)
        {
            events.extend(self.apply_damage_internal(attacker, &[target], MELEE_DAMAGE));
        }

        Ok(events)
    }

    pub fn cast_skill(
        &mut self,
        attacker: PlayerId,
        slot: u8,
    ) -> Result<Vec<SimulationEvent>, SimulationError> {
        let attacker_state = self.require_live_player(attacker)?;

        match slot {
            1 => Ok(self.cast_line_skill(
                attacker,
                attacker_state,
                slot,
                SLOT1_RANGE_UNITS,
                SLOT1_DAMAGE,
                ArenaEffectKind::SkillShot,
            )),
            2 => Ok(self.cast_dash_skill(attacker, attacker_state, slot)),
            3 => Ok(self.cast_burst_skill(attacker, attacker_state, slot)),
            4 => Ok(self.cast_nova_skill(attacker, attacker_state, slot)),
            5 => Ok(self.cast_line_skill(
                attacker,
                attacker_state,
                slot,
                SLOT5_RANGE_UNITS,
                SLOT5_DAMAGE,
                ArenaEffectKind::Beam,
            )),
            other => Err(SimulationError::InvalidSkillSlot(other)),
        }
    }

    #[must_use]
    pub fn player_state(&self, player_id: PlayerId) -> Option<SimPlayerState> {
        self.players.get(&player_id).map(|player| SimPlayerState {
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
    pub fn is_team_defeated(&self, team: TeamSide) -> bool {
        self.players
            .values()
            .filter(|player| player.team == team)
            .all(|player| !player.alive)
    }

    fn cast_line_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
        range: u16,
        damage: u16,
        effect_kind: ArenaEffectKind,
    ) -> Vec<SimulationEvent> {
        let end = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            range,
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
                radius: PLAYER_RADIUS_UNITS,
            },
        }];

        if let Some(target) = self.find_first_player_on_segment(
            attacker,
            (attacker_state.x, attacker_state.y),
            end,
            PLAYER_RADIUS_UNITS,
        ) {
            events.extend(self.apply_damage_internal(attacker, &[target], damage));
        }

        events
    }

    fn cast_dash_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
    ) -> Vec<SimulationEvent> {
        let desired = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            SLOT2_DASH_DISTANCE_UNITS,
        );
        let (resolved_x, resolved_y) = resolve_movement(
            attacker_state.x,
            attacker_state.y,
            i32::from(desired.0),
            i32::from(desired.1),
        );

        if let Some(player) = self.players.get_mut(&attacker) {
            player.x = resolved_x;
            player.y = resolved_y;
            player.moving = false;
        }

        vec![
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: ArenaEffectKind::DashTrail,
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
        ]
    }

    fn cast_burst_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
    ) -> Vec<SimulationEvent> {
        let center = project_from_aim(
            attacker_state.x,
            attacker_state.y,
            attacker_state.aim_x,
            attacker_state.aim_y,
            SLOT3_CAST_RANGE_UNITS,
        );
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: ArenaEffectKind::Burst,
                owner: attacker,
                slot,
                x: center.0,
                y: center.1,
                target_x: center.0,
                target_y: center.1,
                radius: SLOT3_RADIUS_UNITS,
            },
        }];

        let targets = self.find_players_in_radius(center, SLOT3_RADIUS_UNITS, Some(attacker));
        events.extend(self.apply_damage_internal(attacker, &targets, SLOT3_DAMAGE));
        events
    }

    fn cast_nova_skill(
        &mut self,
        attacker: PlayerId,
        attacker_state: SimPlayerState,
        slot: u8,
    ) -> Vec<SimulationEvent> {
        let center = (attacker_state.x, attacker_state.y);
        let mut events = vec![SimulationEvent::EffectSpawned {
            effect: ArenaEffect {
                kind: ArenaEffectKind::Nova,
                owner: attacker,
                slot,
                x: center.0,
                y: center.1,
                target_x: center.0,
                target_y: center.1,
                radius: SLOT4_RADIUS_UNITS,
            },
        }];

        let targets = self.find_players_in_radius(center, SLOT4_RADIUS_UNITS, Some(attacker));
        events.extend(self.apply_damage_internal(attacker, &targets, SLOT4_DAMAGE));
        events
    }

    fn require_live_player(&self, player_id: PlayerId) -> Result<SimPlayerState, SimulationError> {
        let player = self
            .player_state(player_id)
            .ok_or(SimulationError::PlayerMissing(player_id))?;
        if !player.alive {
            return Err(SimulationError::PlayerAlreadyDefeated(player_id));
        }
        Ok(player)
    }

    fn apply_damage_internal(
        &mut self,
        attacker: PlayerId,
        targets: &[PlayerId],
        amount: u16,
    ) -> Vec<SimulationEvent> {
        let mut events = Vec::new();
        for target in targets {
            if let Ok(event) = self.apply_damage(attacker, *target, amount) {
                let (target_x, target_y) = self
                    .players
                    .get(target)
                    .map_or((0, 0), |player| (player.x, player.y));
                events.push(SimulationEvent::EffectSpawned {
                    effect: ArenaEffect {
                        kind: ArenaEffectKind::HitSpark,
                        owner: attacker,
                        slot: 0,
                        x: target_x,
                        y: target_y,
                        target_x,
                        target_y,
                        radius: PLAYER_RADIUS_UNITS,
                    },
                });
                events.push(event);
            }
        }
        events
    }

    fn find_closest_player_near_point(
        &self,
        attacker: PlayerId,
        point: (i16, i16),
        radius: u16,
    ) -> Option<PlayerId> {
        self.players
            .iter()
            .filter(|(player_id, player)| **player_id != attacker && player.alive)
            .map(|(player_id, player)| {
                let dx = i32::from(player.x) - i32::from(point.0);
                let dy = i32::from(player.y) - i32::from(point.1);
                (*player_id, dx * dx + dy * dy)
            })
            .filter(|(_, distance_sq)| *distance_sq <= i32::from(radius) * i32::from(radius))
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
        self.players
            .iter()
            .filter(|(player_id, player)| **player_id != attacker && player.alive)
            .filter_map(|(player_id, player)| {
                let distance = segment_distance_sq(start, end, (player.x, player.y));
                if distance > f32::from(radius) * f32::from(radius) {
                    return None;
                }

                Some((*player_id, point_distance_sq(start, (player.x, player.y))))
            })
            .min_by_key(|(_, distance_sq)| *distance_sq)
            .map(|(player_id, _)| player_id)
    }

    fn find_players_in_radius(
        &self,
        center: (i16, i16),
        radius: u16,
        excluded_player: Option<PlayerId>,
    ) -> Vec<PlayerId> {
        self.players
            .iter()
            .filter(|(player_id, player)| Some(**player_id) != excluded_player && player.alive)
            .filter_map(|(player_id, player)| {
                let distance_sq = point_distance_sq(center, (player.x, player.y));
                let max_distance = i32::from(radius) + i32::from(PLAYER_RADIUS_UNITS);
                if distance_sq <= max_distance * max_distance {
                    Some(*player_id)
                } else {
                    None
                }
            })
            .collect()
    }

    fn apply_damage(
        &mut self,
        attacker: PlayerId,
        target: PlayerId,
        amount: u16,
    ) -> Result<SimulationEvent, SimulationError> {
        if amount == 0 {
            return Err(SimulationError::DamageMustBePositive);
        }

        let attacker_state = self
            .players
            .get(&attacker)
            .ok_or(SimulationError::PlayerMissing(attacker))?;
        if !attacker_state.alive {
            return Err(SimulationError::PlayerAlreadyDefeated(attacker));
        }

        let target_state = self
            .players
            .get_mut(&target)
            .ok_or(SimulationError::PlayerMissing(target))?;
        if !target_state.alive {
            return Err(SimulationError::PlayerAlreadyDefeated(target));
        }

        target_state.hit_points = target_state.hit_points.saturating_sub(amount);
        let defeated = target_state.hit_points == 0;
        if defeated {
            target_state.alive = false;
            target_state.moving = false;
        }

        Ok(SimulationEvent::DamageApplied {
            attacker,
            target,
            amount,
            remaining_hit_points: target_state.hit_points,
            defeated,
        })
    }
}

#[must_use]
pub const fn arena_obstacles() -> &'static [ArenaObstacle] {
    &ARENA_OBSTACLES
}

fn spawn_position(team: TeamSide, ordinal: u16) -> (i16, i16, i16) {
    let y = match ordinal {
        0 => 220,
        1 => -220,
        2 => 320,
        3 => -320,
        _ => 0,
    };

    match team {
        TeamSide::TeamA => (-640, y, DEFAULT_AIM_X),
        TeamSide::TeamB => (640, y, -DEFAULT_AIM_X),
    }
}

fn resolve_movement(current_x: i16, current_y: i16, target_x: i32, target_y: i32) -> (i16, i16) {
    let mut resolved_x = current_x;
    let mut resolved_y = current_y;
    let clamped_x = clamp_to_arena_x(target_x);
    let clamped_y = clamp_to_arena_y(target_y);

    if !position_collides(clamped_x, i32::from(current_y)) {
        resolved_x = saturating_i16(clamped_x);
    }
    if !position_collides(i32::from(resolved_x), clamped_y) {
        resolved_y = saturating_i16(clamped_y);
    }

    (resolved_x, resolved_y)
}

fn clamp_to_arena_x(value: i32) -> i32 {
    let half_width = i32::from(ARENA_WIDTH_UNITS) / 2;
    let radius = i32::from(PLAYER_RADIUS_UNITS);
    value.clamp(-half_width + radius, half_width - radius)
}

fn clamp_to_arena_y(value: i32) -> i32 {
    let half_height = i32::from(ARENA_HEIGHT_UNITS) / 2;
    let radius = i32::from(PLAYER_RADIUS_UNITS);
    value.clamp(-half_height + radius, half_height - radius)
}

fn position_collides(x: i32, y: i32) -> bool {
    arena_obstacles()
        .iter()
        .any(|obstacle| circle_intersects_rect(x, y, i32::from(PLAYER_RADIUS_UNITS), obstacle))
}

fn circle_intersects_rect(x: i32, y: i32, radius: i32, obstacle: &ArenaObstacle) -> bool {
    let left = i32::from(obstacle.center_x) - i32::from(obstacle.half_width);
    let right = i32::from(obstacle.center_x) + i32::from(obstacle.half_width);
    let top = i32::from(obstacle.center_y) - i32::from(obstacle.half_height);
    let bottom = i32::from(obstacle.center_y) + i32::from(obstacle.half_height);
    let closest_x = x.clamp(left, right);
    let closest_y = y.clamp(top, bottom);
    let dx = x - closest_x;
    let dy = y - closest_y;
    dx * dx + dy * dy <= radius * radius
}

#[allow(clippy::cast_possible_truncation)]
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
        saturating_i16(projected_x.round() as i32),
        saturating_i16(projected_y.round() as i32),
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

#[allow(clippy::cast_possible_truncation)]
fn saturating_i16(value: i32) -> i16 {
    let clamped = value.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
    match i16::try_from(clamped) {
        Ok(value) => value,
        Err(error) => panic!("clamped i32 should fit inside i16: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_domain::{PlayerName, PlayerRecord};

    fn player_id(raw: u32) -> PlayerId {
        PlayerId::new(raw).expect("valid player id")
    }

    fn seed(raw_id: u32, raw_name: &str, team: TeamSide, hit_points: u16) -> SimPlayerSeed {
        SimPlayerSeed {
            assignment: TeamAssignment {
                player_id: player_id(raw_id),
                player_name: PlayerName::new(raw_name).expect("valid player name"),
                record: PlayerRecord::new(),
                team,
            },
            hit_points,
        }
    }

    #[test]
    fn movement_intent_accepts_unit_inputs_and_rejects_out_of_range_values() {
        assert_eq!(
            MovementIntent::new(-2, 0),
            Err(SimulationError::MovementComponentOutOfRange {
                axis: "x",
                value: -2,
            })
        );
        assert_eq!(
            MovementIntent::new(-1, 1),
            Ok(MovementIntent { x: -1, y: 1 })
        );
        assert_eq!(
            MovementIntent::new(0, 2),
            Err(SimulationError::MovementComponentOutOfRange {
                axis: "y",
                value: 2,
            })
        );
    }

    #[test]
    fn simulation_new_rejects_duplicate_players_and_zero_hit_points() {
        assert!(matches!(
            SimulationWorld::new(vec![
                seed(1, "Alice", TeamSide::TeamA, 100),
                seed(1, "Bob", TeamSide::TeamB, 100),
            ]),
            Err(SimulationError::DuplicatePlayer(player)) if player == player_id(1)
        ));

        assert!(matches!(
            SimulationWorld::new(vec![seed(1, "Alice", TeamSide::TeamA, 0)]),
            Err(SimulationError::InvalidHitPoints { player_id: player, hit_points: 0 })
                if player == player_id(1)
        ));
    }

    #[test]
    fn submit_input_requires_a_live_known_player() {
        let mut world = SimulationWorld::new(vec![seed(1, "Alice", TeamSide::TeamA, 100)])
            .expect("world should build");

        assert_eq!(
            world.submit_input(
                player_id(9),
                MovementIntent::new(1, 0).expect("valid intent")
            ),
            Err(SimulationError::PlayerMissing(player_id(9)))
        );

        let _ = world
            .apply_damage_internal(player_id(1), &[player_id(1)], 100)
            .pop()
            .expect("damage event");
        assert_eq!(
            world.submit_input(
                player_id(1),
                MovementIntent::new(1, 0).expect("valid intent")
            ),
            Err(SimulationError::PlayerAlreadyDefeated(player_id(1)))
        );
    }

    #[test]
    fn tick_moves_players_and_stops_them_immediately_without_new_input() {
        let mut world = SimulationWorld::new(vec![seed(1, "Alice", TeamSide::TeamA, 100)])
            .expect("world should build");
        let starting_state = world.player_state(player_id(1)).expect("player exists");

        world
            .submit_input(
                player_id(1),
                MovementIntent::new(1, 0).expect("valid intent"),
            )
            .expect("input should be accepted");
        assert_eq!(
            world.tick(),
            vec![SimulationEvent::PlayerMoved {
                player_id: player_id(1),
                x: starting_state.x + PLAYER_MOVE_SPEED_UNITS,
                y: starting_state.y,
            }]
        );

        assert_eq!(world.tick(), Vec::<SimulationEvent>::new());
        assert!(
            !world
                .player_state(player_id(1))
                .expect("player exists")
                .moving
        );
    }

    #[test]
    fn movement_collides_with_the_center_pillars_and_shrubs() {
        let mut world = SimulationWorld::new(vec![seed(1, "Alice", TeamSide::TeamA, 100)])
            .expect("world should build");
        {
            let player = world.players.get_mut(&player_id(1)).expect("player");
            player.x = -330;
            player.y = -150;
        }

        for _ in 0..10 {
            world
                .submit_input(
                    player_id(1),
                    MovementIntent::new(1, 0).expect("valid intent"),
                )
                .expect("input should be accepted");
            let _ = world.tick();
        }

        let state = world.player_state(player_id(1)).expect("player exists");
        assert!(state.x < -220 - i16::try_from(SHRUB_HALF_EXTENT_UNITS).expect("fits"));
    }

    #[test]
    fn update_aim_tracks_non_zero_mouse_deltas() {
        let mut world = SimulationWorld::new(vec![seed(1, "Alice", TeamSide::TeamA, 100)])
            .expect("world should build");

        assert_eq!(world.update_aim(player_id(1), 240, -120), Ok(true));
        assert_eq!(world.update_aim(player_id(1), 240, -120), Ok(false));
        assert_eq!(world.update_aim(player_id(1), 0, 0), Ok(false));

        let state = world.player_state(player_id(1)).expect("player exists");
        assert_eq!(state.aim_x, 240);
        assert_eq!(state.aim_y, -120);
    }

    #[test]
    fn melee_and_skill_slots_apply_damage_and_validate_slot_numbers() {
        let mut world = SimulationWorld::new(vec![
            seed(1, "Alice", TeamSide::TeamA, 100),
            seed(2, "Bob", TeamSide::TeamB, 100),
        ])
        .expect("world should build");

        {
            let bob = world.players.get_mut(&player_id(2)).expect("bob exists");
            bob.x = -540;
            bob.y = 220;
        }

        let melee_events = world.melee_attack(player_id(1)).expect("melee should work");
        assert!(melee_events.iter().any(|event| matches!(
            event,
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: ArenaEffectKind::MeleeSwing,
                    ..
                }
            }
        )));
        assert!(melee_events.iter().any(|event| matches!(
            event,
            SimulationEvent::DamageApplied {
                attacker,
                target,
                amount: MELEE_DAMAGE,
                ..
            } if *attacker == player_id(1) && *target == player_id(2)
        )));

        let slot_one_events = world
            .cast_skill(player_id(1), 1)
            .expect("slot one should work");
        assert!(slot_one_events.iter().any(|event| matches!(
            event,
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: ArenaEffectKind::SkillShot,
                    slot: 1,
                    ..
                }
            }
        )));
        assert_eq!(
            world.cast_skill(player_id(1), 9),
            Err(SimulationError::InvalidSkillSlot(9))
        );
    }

    #[test]
    fn dash_and_area_skills_spawn_effects() {
        let mut world = SimulationWorld::new(vec![
            seed(1, "Alice", TeamSide::TeamA, 100),
            seed(2, "Bob", TeamSide::TeamB, 100),
        ])
        .expect("world should build");

        let slot_two_events = world.cast_skill(player_id(1), 2).expect("dash should work");
        assert!(slot_two_events.iter().any(|event| matches!(
            event,
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: ArenaEffectKind::DashTrail,
                    slot: 2,
                    ..
                }
            }
        )));
        assert!(slot_two_events.iter().any(|event| matches!(
            event,
            SimulationEvent::PlayerMoved { player_id: moved_player_id, .. }
                if *moved_player_id == player_id(1)
        )));

        let slot_three_events = world
            .cast_skill(player_id(1), 3)
            .expect("burst should work");
        assert!(slot_three_events.iter().any(|event| matches!(
            event,
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: ArenaEffectKind::Burst,
                    slot: 3,
                    ..
                }
            }
        )));

        let slot_four_events = world.cast_skill(player_id(1), 4).expect("nova should work");
        assert!(slot_four_events.iter().any(|event| matches!(
            event,
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: ArenaEffectKind::Nova,
                    slot: 4,
                    ..
                }
            }
        )));

        let slot_five_events = world.cast_skill(player_id(1), 5).expect("beam should work");
        assert!(slot_five_events.iter().any(|event| matches!(
            event,
            SimulationEvent::EffectSpawned {
                effect: ArenaEffect {
                    kind: ArenaEffectKind::Beam,
                    slot: 5,
                    ..
                }
            }
        )));
    }

    #[test]
    fn apply_damage_allows_friendly_fire_and_rejects_invalid_damage_calls() {
        let mut world = SimulationWorld::new(vec![
            seed(1, "Alice", TeamSide::TeamA, 100),
            seed(2, "Ally", TeamSide::TeamA, 100),
        ])
        .expect("world should build");

        {
            let ally = world.players.get_mut(&player_id(2)).expect("ally exists");
            ally.x = -540;
            ally.y = 220;
        }

        let events = world
            .melee_attack(player_id(1))
            .expect("friendly fire is allowed");
        assert!(events.iter().any(|event| matches!(
            event,
            SimulationEvent::DamageApplied {
                attacker,
                target,
                ..
            } if *attacker == player_id(1) && *target == player_id(2)
        )));
    }

    #[test]
    fn lethal_damage_marks_defeat_and_team_defeat_queries_reflect_the_state() {
        let mut world = SimulationWorld::new(vec![
            seed(1, "Alice", TeamSide::TeamA, 100),
            seed(2, "Bob", TeamSide::TeamB, 10),
        ])
        .expect("world should build");

        {
            let bob = world.players.get_mut(&player_id(2)).expect("bob exists");
            bob.x = -540;
            bob.y = 220;
        }

        let events = world.cast_skill(player_id(1), 5).expect("beam should work");
        assert!(events.iter().any(|event| matches!(
            event,
            SimulationEvent::DamageApplied {
                target,
                defeated: true,
                ..
            } if *target == player_id(2)
        )));
        assert!(world.is_team_defeated(TeamSide::TeamB));
    }
}
