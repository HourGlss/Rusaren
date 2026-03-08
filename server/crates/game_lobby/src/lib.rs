//! Lobby, ready-check, and match-launch orchestration.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::collections::BTreeMap;
use std::fmt;

use game_domain::{
    LobbyId, PlayerId, PlayerName, PlayerRecord, ReadyState, TeamAssignment, TeamSide,
};

pub const LAUNCH_COUNTDOWN_SECONDS: u8 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbyPlayer {
    pub player_id: PlayerId,
    pub player_name: PlayerName,
    pub record: PlayerRecord,
    pub team: Option<TeamSide>,
    pub ready_state: ReadyState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LobbyPhase {
    Open,
    LaunchCountdown {
        seconds_remaining: u8,
        locked_roster: Vec<TeamAssignment>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LobbyEvent {
    PlayerJoined {
        player_id: PlayerId,
    },
    PlayerLeft {
        player_id: PlayerId,
    },
    TeamSelected {
        player_id: PlayerId,
        team: TeamSide,
        ready_reset: bool,
    },
    ReadyChanged {
        player_id: PlayerId,
        ready: ReadyState,
    },
    LaunchCountdownStarted {
        seconds_remaining: u8,
        roster: Vec<TeamAssignment>,
    },
    LaunchCountdownTick {
        seconds_remaining: u8,
    },
    MatchLaunchReady {
        roster: Vec<TeamAssignment>,
    },
    MatchAborted {
        player_id: PlayerId,
        player_name: PlayerName,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LobbyError {
    DuplicatePlayer(PlayerId),
    PlayerMissing(PlayerId),
    TeamRequiredForReady(PlayerId),
    LobbyLocked,
    CountdownNotRunning,
}

impl fmt::Display for LobbyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePlayer(player_id) => {
                write!(f, "player {} is already in the lobby", player_id.get())
            }
            Self::PlayerMissing(player_id) => {
                write!(f, "player {} is not in the lobby", player_id.get())
            }
            Self::TeamRequiredForReady(player_id) => write!(
                f,
                "player {} must join a team before toggling ready",
                player_id.get()
            ),
            Self::LobbyLocked => f.write_str("lobby roster is locked"),
            Self::CountdownNotRunning => f.write_str("launch countdown is not running"),
        }
    }
}

impl std::error::Error for LobbyError {}

#[derive(Clone, Debug)]
pub struct Lobby {
    _lobby_id: LobbyId,
    players: BTreeMap<PlayerId, LobbyPlayer>,
    phase: LobbyPhase,
}

impl Lobby {
    #[must_use]
    pub fn new(lobby_id: LobbyId) -> Self {
        Self {
            _lobby_id: lobby_id,
            players: BTreeMap::new(),
            phase: LobbyPhase::Open,
        }
    }

    pub fn add_player(
        &mut self,
        player_id: PlayerId,
        player_name: PlayerName,
        record: PlayerRecord,
    ) -> Result<LobbyEvent, LobbyError> {
        self.ensure_open()?;

        if self.players.contains_key(&player_id) {
            return Err(LobbyError::DuplicatePlayer(player_id));
        }

        self.players.insert(
            player_id,
            LobbyPlayer {
                player_id,
                player_name,
                record,
                team: None,
                ready_state: ReadyState::NotReady,
            },
        );

        Ok(LobbyEvent::PlayerJoined { player_id })
    }

    pub fn select_team(
        &mut self,
        player_id: PlayerId,
        team: TeamSide,
    ) -> Result<Vec<LobbyEvent>, LobbyError> {
        self.ensure_open()?;

        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(LobbyError::PlayerMissing(player_id))?;

        let ready_reset = player.ready_state.is_ready();
        player.team = Some(team);
        player.ready_state = ReadyState::NotReady;

        let mut events = vec![LobbyEvent::TeamSelected {
            player_id,
            team,
            ready_reset,
        }];
        events.extend(self.maybe_start_countdown());
        Ok(events)
    }

    pub fn set_ready(
        &mut self,
        player_id: PlayerId,
        ready: ReadyState,
    ) -> Result<Vec<LobbyEvent>, LobbyError> {
        self.ensure_open()?;

        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(LobbyError::PlayerMissing(player_id))?;

        if player.team.is_none() {
            return Err(LobbyError::TeamRequiredForReady(player_id));
        }

        player.ready_state = ready;

        let mut events = vec![LobbyEvent::ReadyChanged {
            player_id,
            ready: player.ready_state,
        }];
        events.extend(self.maybe_start_countdown());
        Ok(events)
    }

    pub fn leave_or_disconnect_player(
        &mut self,
        player_id: PlayerId,
    ) -> Result<LobbyEvent, LobbyError> {
        match &self.phase {
            LobbyPhase::Open => {
                self.players
                    .remove(&player_id)
                    .ok_or(LobbyError::PlayerMissing(player_id))?;

                Ok(LobbyEvent::PlayerLeft { player_id })
            }
            LobbyPhase::LaunchCountdown { .. } => {
                let removed = self
                    .players
                    .remove(&player_id)
                    .ok_or(LobbyError::PlayerMissing(player_id))?;

                for survivor in self.players.values_mut() {
                    survivor.ready_state = ReadyState::NotReady;
                }
                self.phase = LobbyPhase::Open;

                Ok(LobbyEvent::MatchAborted {
                    player_id,
                    player_name: removed.player_name.clone(),
                    message: format!("{} has disconnected. Game is over.", removed.player_name),
                })
            }
        }
    }

    pub fn advance_countdown(&mut self) -> Result<LobbyEvent, LobbyError> {
        let (seconds_remaining, roster) = match &self.phase {
            LobbyPhase::LaunchCountdown {
                seconds_remaining,
                locked_roster,
            } => (*seconds_remaining, locked_roster.clone()),
            LobbyPhase::Open => return Err(LobbyError::CountdownNotRunning),
        };

        if seconds_remaining > 1 {
            self.phase = LobbyPhase::LaunchCountdown {
                seconds_remaining: seconds_remaining - 1,
                locked_roster: roster,
            };

            Ok(LobbyEvent::LaunchCountdownTick {
                seconds_remaining: seconds_remaining - 1,
            })
        } else {
            for entry in &roster {
                let _ = self.players.remove(&entry.player_id);
            }

            self.phase = LobbyPhase::Open;
            Ok(LobbyEvent::MatchLaunchReady { roster })
        }
    }

    #[must_use]
    pub fn phase(&self) -> &LobbyPhase {
        &self.phase
    }

    #[must_use]
    pub fn player(&self, player_id: PlayerId) -> Option<&LobbyPlayer> {
        self.players.get(&player_id)
    }

    #[must_use]
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    fn ensure_open(&self) -> Result<(), LobbyError> {
        if matches!(self.phase, LobbyPhase::Open) {
            Ok(())
        } else {
            Err(LobbyError::LobbyLocked)
        }
    }

    fn maybe_start_countdown(&mut self) -> Vec<LobbyEvent> {
        if !matches!(self.phase, LobbyPhase::Open) || !self.can_start_countdown() {
            return Vec::new();
        }

        let roster = self.locked_roster();
        self.phase = LobbyPhase::LaunchCountdown {
            seconds_remaining: LAUNCH_COUNTDOWN_SECONDS,
            locked_roster: roster.clone(),
        };

        vec![LobbyEvent::LaunchCountdownStarted {
            seconds_remaining: LAUNCH_COUNTDOWN_SECONDS,
            roster,
        }]
    }

    fn can_start_countdown(&self) -> bool {
        let mut team_a_count = 0usize;
        let mut team_b_count = 0usize;

        for player in self.players.values() {
            if !player.ready_state.is_ready() {
                return false;
            }

            match player.team {
                Some(TeamSide::TeamA) => team_a_count += 1,
                Some(TeamSide::TeamB) => team_b_count += 1,
                None => return false,
            }
        }

        team_a_count > 0 && team_b_count > 0
    }

    fn locked_roster(&self) -> Vec<TeamAssignment> {
        self.players
            .values()
            .filter_map(|player| {
                player.team.map(|team| TeamAssignment {
                    player_id: player.player_id,
                    player_name: player.player_name.clone(),
                    record: player.record,
                    team,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn player_id(raw: u32) -> PlayerId {
        PlayerId::new(raw).expect("valid player id")
    }

    fn player_name(raw: &str) -> PlayerName {
        PlayerName::new(raw).expect("valid player name")
    }

    fn lobby() -> Lobby {
        Lobby::new(LobbyId::new(1).expect("valid lobby id"))
    }

    #[test]
    fn add_player_accepts_unique_players_and_rejects_duplicates() {
        let mut lobby = lobby();
        assert_eq!(
            lobby.add_player(player_id(1), player_name("Alice"), PlayerRecord::new()),
            Ok(LobbyEvent::PlayerJoined {
                player_id: player_id(1),
            })
        );

        assert_eq!(
            lobby.add_player(player_id(1), player_name("Alice"), PlayerRecord::new()),
            Err(LobbyError::DuplicatePlayer(player_id(1)))
        );
    }

    #[test]
    fn add_player_starts_in_not_ready_without_a_team() {
        let mut lobby = lobby();
        lobby
            .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
            .expect("player should join");

        let player = lobby.player(player_id(1)).expect("player should exist");
        assert_eq!(player.team, None);
        assert_eq!(player.ready_state, ReadyState::NotReady);
    }

    #[test]
    fn select_team_assigns_team_and_resets_ready_state() {
        let mut lobby = lobby();
        lobby
            .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
            .expect("player should join");
        lobby
            .select_team(player_id(1), TeamSide::TeamA)
            .expect("team selection should succeed");
        lobby
            .set_ready(player_id(1), ReadyState::Ready)
            .expect("ready should succeed");

        let events = lobby
            .select_team(player_id(1), TeamSide::TeamB)
            .expect("changing teams should succeed");

        assert_eq!(
            events,
            vec![LobbyEvent::TeamSelected {
                player_id: player_id(1),
                team: TeamSide::TeamB,
                ready_reset: true,
            }]
        );
        assert_eq!(
            lobby
                .player(player_id(1))
                .expect("player should exist")
                .ready_state,
            ReadyState::NotReady
        );
    }

    #[test]
    fn select_team_rejects_missing_players_and_locked_rosters() {
        let mut lobby = lobby();
        assert_eq!(
            lobby.select_team(player_id(7), TeamSide::TeamA),
            Err(LobbyError::PlayerMissing(player_id(7)))
        );

        lobby
            .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
            .expect("player should join");
        lobby
            .add_player(player_id(2), player_name("Bob"), PlayerRecord::new())
            .expect("player should join");
        lobby
            .select_team(player_id(1), TeamSide::TeamA)
            .expect("team selection should work");
        lobby
            .select_team(player_id(2), TeamSide::TeamB)
            .expect("team selection should work");
        lobby
            .set_ready(player_id(1), ReadyState::Ready)
            .expect("ready should work");
        lobby
            .set_ready(player_id(2), ReadyState::Ready)
            .expect("ready should start countdown");

        assert_eq!(
            lobby.select_team(player_id(1), TeamSide::TeamA),
            Err(LobbyError::LobbyLocked)
        );
    }

    #[test]
    fn set_ready_requires_a_team_and_a_real_player() {
        let mut lobby = lobby();
        assert_eq!(
            lobby.set_ready(player_id(9), ReadyState::Ready),
            Err(LobbyError::PlayerMissing(player_id(9)))
        );

        lobby
            .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
            .expect("player should join");
        assert_eq!(
            lobby.set_ready(player_id(1), ReadyState::Ready),
            Err(LobbyError::TeamRequiredForReady(player_id(1)))
        );
    }

    #[test]
    fn set_ready_starts_the_countdown_once_both_teams_are_ready() {
        let mut lobby = lobby();
        lobby
            .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
            .expect("player should join");
        lobby
            .add_player(player_id(2), player_name("Bob"), PlayerRecord::new())
            .expect("player should join");
        lobby
            .select_team(player_id(1), TeamSide::TeamA)
            .expect("team selection should work");
        lobby
            .select_team(player_id(2), TeamSide::TeamB)
            .expect("team selection should work");

        assert_eq!(
            lobby
                .set_ready(player_id(1), ReadyState::Ready)
                .expect("ready should succeed"),
            vec![LobbyEvent::ReadyChanged {
                player_id: player_id(1),
                ready: ReadyState::Ready,
            }]
        );

        let events = lobby
            .set_ready(player_id(2), ReadyState::Ready)
            .expect("second ready should start countdown");

        assert_eq!(
            events,
            vec![
                LobbyEvent::ReadyChanged {
                    player_id: player_id(2),
                    ready: ReadyState::Ready,
                },
                LobbyEvent::LaunchCountdownStarted {
                    seconds_remaining: LAUNCH_COUNTDOWN_SECONDS,
                    roster: vec![
                        TeamAssignment {
                            player_id: player_id(1),
                            player_name: player_name("Alice"),
                            record: PlayerRecord::new(),
                            team: TeamSide::TeamA,
                        },
                        TeamAssignment {
                            player_id: player_id(2),
                            player_name: player_name("Bob"),
                            record: PlayerRecord::new(),
                            team: TeamSide::TeamB,
                        },
                    ],
                },
            ]
        );
        assert!(matches!(lobby.phase(), LobbyPhase::LaunchCountdown { .. }));
    }

    #[test]
    fn leaving_during_open_removes_the_player() {
        let mut lobby = lobby();
        lobby
            .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
            .expect("player should join");

        assert_eq!(
            lobby.leave_or_disconnect_player(player_id(1)),
            Ok(LobbyEvent::PlayerLeft {
                player_id: player_id(1),
            })
        );
        assert_eq!(lobby.player_count(), 0);
    }

    #[test]
    fn disconnecting_during_countdown_aborts_the_match_and_unlocks_the_lobby() {
        let mut lobby = lobby();
        lobby
            .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
            .expect("player should join");
        lobby
            .add_player(player_id(2), player_name("Bob"), PlayerRecord::new())
            .expect("player should join");
        lobby
            .select_team(player_id(1), TeamSide::TeamA)
            .expect("team selection should work");
        lobby
            .select_team(player_id(2), TeamSide::TeamB)
            .expect("team selection should work");
        lobby
            .set_ready(player_id(1), ReadyState::Ready)
            .expect("ready should work");
        lobby
            .set_ready(player_id(2), ReadyState::Ready)
            .expect("ready should start countdown");

        assert_eq!(
            lobby.leave_or_disconnect_player(player_id(2)),
            Ok(LobbyEvent::MatchAborted {
                player_id: player_id(2),
                player_name: player_name("Bob"),
                message: String::from("Bob has disconnected. Game is over."),
            })
        );
        assert!(matches!(lobby.phase(), LobbyPhase::Open));
        assert_eq!(
            lobby
                .player(player_id(1))
                .expect("remaining player should exist")
                .ready_state,
            ReadyState::NotReady
        );
    }

    #[test]
    fn advance_countdown_ticks_and_then_launches_the_locked_roster() {
        let mut lobby = lobby();
        lobby
            .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
            .expect("player should join");
        lobby
            .add_player(player_id(2), player_name("Bob"), PlayerRecord::new())
            .expect("player should join");
        lobby
            .select_team(player_id(1), TeamSide::TeamA)
            .expect("team selection should work");
        lobby
            .select_team(player_id(2), TeamSide::TeamB)
            .expect("team selection should work");
        lobby
            .set_ready(player_id(1), ReadyState::Ready)
            .expect("ready should work");
        lobby
            .set_ready(player_id(2), ReadyState::Ready)
            .expect("ready should start countdown");

        for remaining in (2..=LAUNCH_COUNTDOWN_SECONDS).rev() {
            assert_eq!(
                lobby.advance_countdown(),
                Ok(LobbyEvent::LaunchCountdownTick {
                    seconds_remaining: remaining - 1,
                })
            );
        }

        match lobby.advance_countdown() {
            Ok(LobbyEvent::MatchLaunchReady { roster }) => {
                assert_eq!(roster.len(), 2);
                assert_eq!(lobby.player_count(), 0);
            }
            other => panic!("unexpected launch result: {other:?}"),
        }
    }

    #[test]
    fn advance_countdown_rejects_calls_when_no_countdown_is_running() {
        let mut lobby = lobby();
        assert_eq!(
            lobby.advance_countdown(),
            Err(LobbyError::CountdownNotRunning)
        );
    }

    fn maybe_team() -> impl Strategy<Value = Option<TeamSide>> {
        prop_oneof![
            Just(None),
            Just(Some(TeamSide::TeamA)),
            Just(Some(TeamSide::TeamB)),
        ]
    }

    proptest! {
        #[test]
        fn prop_launch_countdown_requires_two_ready_players_on_opposing_teams(
            team_one in maybe_team(),
            team_two in maybe_team(),
            ready_one in any::<bool>(),
            ready_two in any::<bool>(),
        ) {
            let mut lobby = lobby();
            lobby
                .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
                .expect("player should join");
            lobby
                .add_player(player_id(2), player_name("Bob"), PlayerRecord::new())
                .expect("player should join");

            if let Some(team) = team_one {
                lobby
                    .select_team(player_id(1), team)
                    .expect("team selection should work");
                if ready_one {
                    lobby
                        .set_ready(player_id(1), ReadyState::Ready)
                        .expect("ready should work with a team");
                }
            }

            if let Some(team) = team_two {
                lobby
                    .select_team(player_id(2), team)
                    .expect("team selection should work");
                if ready_two {
                    lobby
                        .set_ready(player_id(2), ReadyState::Ready)
                        .expect("ready should work with a team");
                }
            }

            let should_start = ready_one
                && ready_two
                && matches!(
                    (team_one, team_two),
                    (Some(TeamSide::TeamA), Some(TeamSide::TeamB))
                        | (Some(TeamSide::TeamB), Some(TeamSide::TeamA))
                );

            prop_assert_eq!(
                matches!(lobby.phase(), LobbyPhase::LaunchCountdown { .. }),
                should_start
            );
        }
    }
}
