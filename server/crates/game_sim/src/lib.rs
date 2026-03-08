//! Fixed-tick simulation, world updates, and combat resolution.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::collections::BTreeMap;
use std::fmt;

use game_domain::{PlayerId, TeamAssignment, TeamSide};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimPlayerSeed {
    pub assignment: TeamAssignment,
    pub hit_points: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimPlayerState {
    pub player_id: PlayerId,
    pub team: TeamSide,
    pub x: i32,
    pub y: i32,
    pub hit_points: u16,
    pub alive: bool,
    pub moving: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimulationEvent {
    PlayerMoved {
        player_id: PlayerId,
        x: i32,
        y: i32,
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
    x: i32,
    y: i32,
    hit_points: u16,
    alive: bool,
    moving: bool,
}

impl SimulationWorld {
    pub fn new(players: Vec<SimPlayerSeed>) -> Result<Self, SimulationError> {
        let mut world_players = BTreeMap::new();

        for player in players {
            if player.hit_points == 0 {
                return Err(SimulationError::InvalidHitPoints {
                    player_id: player.assignment.player_id,
                    hit_points: player.hit_points,
                });
            }

            if world_players
                .insert(
                    player.assignment.player_id,
                    SimPlayer {
                        team: player.assignment.team,
                        x: 0,
                        y: 0,
                        hit_points: player.hit_points,
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
            player.x += i32::from(movement.x);
            player.y += i32::from(movement.y);

            if player.moving {
                events.push(SimulationEvent::PlayerMoved {
                    player_id: *player_id,
                    x: player.x,
                    y: player.y,
                });
            }
        }

        events
    }

    pub fn apply_damage(
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

    #[must_use]
    pub fn player_state(&self, player_id: PlayerId) -> Option<SimPlayerState> {
        self.players.get(&player_id).map(|player| SimPlayerState {
            player_id,
            team: player.team,
            x: player.x,
            y: player.y,
            hit_points: player.hit_points,
            alive: player.alive,
            moving: player.moving,
        })
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

        world
            .apply_damage(player_id(1), player_id(1), 100)
            .expect("self damage is allowed");
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
                x: 1,
                y: 0,
            }]
        );
        assert_eq!(
            world.player_state(player_id(1)).expect("player exists"),
            SimPlayerState {
                player_id: player_id(1),
                team: TeamSide::TeamA,
                x: 1,
                y: 0,
                hit_points: 100,
                alive: true,
                moving: true,
            }
        );

        assert_eq!(world.tick(), Vec::<SimulationEvent>::new());
        assert_eq!(
            world.player_state(player_id(1)).expect("player exists"),
            SimPlayerState {
                player_id: player_id(1),
                team: TeamSide::TeamA,
                x: 1,
                y: 0,
                hit_points: 100,
                alive: true,
                moving: false,
            }
        );
    }

    #[test]
    fn apply_damage_allows_friendly_fire_and_rejects_invalid_damage_calls() {
        let mut world = SimulationWorld::new(vec![
            seed(1, "Alice", TeamSide::TeamA, 100),
            seed(2, "Bob", TeamSide::TeamA, 100),
        ])
        .expect("world should build");

        assert_eq!(
            world.apply_damage(player_id(1), player_id(2), 0),
            Err(SimulationError::DamageMustBePositive)
        );
        assert_eq!(
            world.apply_damage(player_id(9), player_id(2), 1),
            Err(SimulationError::PlayerMissing(player_id(9)))
        );
        assert_eq!(
            world.apply_damage(player_id(1), player_id(9), 1),
            Err(SimulationError::PlayerMissing(player_id(9)))
        );

        assert_eq!(
            world
                .apply_damage(player_id(1), player_id(2), 25)
                .expect("friendly fire damage should be allowed"),
            SimulationEvent::DamageApplied {
                attacker: player_id(1),
                target: player_id(2),
                amount: 25,
                remaining_hit_points: 75,
                defeated: false,
            }
        );
    }

    #[test]
    fn lethal_damage_marks_defeat_and_team_defeat_queries_reflect_the_state() {
        let mut world = SimulationWorld::new(vec![
            seed(1, "Alice", TeamSide::TeamA, 100),
            seed(2, "Bob", TeamSide::TeamB, 100),
        ])
        .expect("world should build");

        assert_eq!(
            world
                .apply_damage(player_id(1), player_id(2), 100)
                .expect("lethal damage should work"),
            SimulationEvent::DamageApplied {
                attacker: player_id(1),
                target: player_id(2),
                amount: 100,
                remaining_hit_points: 0,
                defeated: true,
            }
        );
        assert!(world.is_team_defeated(TeamSide::TeamB));
        assert!(!world.is_team_defeated(TeamSide::TeamA));
        assert_eq!(
            world.apply_damage(player_id(1), player_id(2), 1),
            Err(SimulationError::PlayerAlreadyDefeated(player_id(2)))
        );
    }
}
