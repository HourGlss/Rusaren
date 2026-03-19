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

    #[must_use]
    pub fn players(&self) -> Vec<LobbyPlayer> {
        self.players.values().cloned().collect()
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
mod tests;
