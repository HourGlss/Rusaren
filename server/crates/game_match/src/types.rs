use std::fmt;

use game_domain::{
    DomainError, LoadoutProgress, MatchOutcome, PlayerId, RoundNumber, SkillChoice, TeamAssignment,
    TeamSide,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchConfig {
    pub total_rounds: RoundNumber,
    pub skill_pick_seconds: u8,
    pub pre_combat_seconds: u8,
    pub objective_target_ms: u32,
}

impl MatchConfig {
    pub fn new(
        total_rounds: u8,
        skill_pick_seconds: u8,
        pre_combat_seconds: u8,
        objective_target_ms: u32,
    ) -> Result<Self, MatchError> {
        if skill_pick_seconds == 0 {
            return Err(MatchError::InvalidConfiguration {
                message: String::from("skill_pick_seconds must be greater than zero"),
            });
        }
        if pre_combat_seconds == 0 {
            return Err(MatchError::InvalidConfiguration {
                message: String::from("pre_combat_seconds must be greater than zero"),
            });
        }
        if objective_target_ms == 0 {
            return Err(MatchError::InvalidConfiguration {
                message: String::from("objective_target_ms must be greater than zero"),
            });
        }
        let total_rounds =
            RoundNumber::new(total_rounds).map_err(|error| MatchError::InvalidConfiguration {
                message: format!("total_rounds is invalid: {error}"),
            })?;
        Ok(Self {
            total_rounds,
            skill_pick_seconds,
            pre_combat_seconds,
            objective_target_ms,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScoreBoard {
    pub team_a: u8,
    pub team_b: u8,
}

impl ScoreBoard {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            team_a: 0,
            team_b: 0,
        }
    }

    pub(crate) fn award_round(&mut self, winner: TeamSide) {
        match winner {
            TeamSide::TeamA => self.team_a = self.team_a.saturating_add(1),
            TeamSide::TeamB => self.team_b = self.team_b.saturating_add(1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchPlayer {
    pub assignment: TeamAssignment,
    pub loadout_progress: LoadoutProgress,
    pub selected_for_round: Option<SkillChoice>,
    pub equipped_slots: [Option<SkillChoice>; 5],
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchPhase {
    SkillPick {
        seconds_remaining: u8,
    },
    PreCombat {
        seconds_remaining: u8,
    },
    Combat,
    MatchEnd {
        outcome: MatchOutcome,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchEvent {
    SkillChosen {
        player_id: PlayerId,
        slot: u8,
        choice: SkillChoice,
    },
    PreCombatStarted {
        seconds_remaining: u8,
    },
    CombatStarted,
    RoundWon {
        round: RoundNumber,
        winning_team: TeamSide,
        score: ScoreBoard,
    },
    MatchEnded {
        outcome: MatchOutcome,
        message: String,
        score: ScoreBoard,
    },
    ManualResolutionRequired {
        reason: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchError {
    InvalidConfiguration {
        message: String,
    },
    DuplicatePlayer(PlayerId),
    MissingTeam(TeamSide),
    PlayerMissing(PlayerId),
    WrongPhase {
        expected: &'static str,
        actual: &'static str,
    },
    SkillAlreadySelected(PlayerId),
    InvalidSkillChoice(DomainError),
    PlayerAlreadyDefeated(PlayerId),
}

impl fmt::Display for MatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration { message } => {
                write!(f, "match configuration is invalid: {message}")
            }
            Self::DuplicatePlayer(player_id) => {
                write!(
                    f,
                    "player {} appears more than once in the match roster",
                    player_id.get()
                )
            }
            Self::MissingTeam(team) => write!(f, "{team} must contain at least one player"),
            Self::PlayerMissing(player_id) => {
                write!(f, "player {} is not part of the match", player_id.get())
            }
            Self::WrongPhase { expected, actual } => {
                write!(
                    f,
                    "match expected phase {expected} but is currently {actual}"
                )
            }
            Self::SkillAlreadySelected(player_id) => {
                write!(
                    f,
                    "player {} already selected a skill this round",
                    player_id.get()
                )
            }
            Self::InvalidSkillChoice(error) => error.fmt(f),
            Self::PlayerAlreadyDefeated(player_id) => {
                write!(f, "player {} is already defeated", player_id.get())
            }
        }
    }
}

impl std::error::Error for MatchError {}
