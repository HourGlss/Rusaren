use std::fmt;

use game_domain::{
    DomainError, LoadoutProgress, MatchOutcome, PlayerId, RoundNumber, SkillChoice, TeamAssignment,
    TeamSide,
};

use super::{known_round, PRE_COMBAT_SECONDS, SKILL_PICK_SECONDS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchConfig {
    pub total_rounds: RoundNumber,
    pub skill_pick_seconds: u8,
    pub pre_combat_seconds: u8,
}

impl MatchConfig {
    #[must_use]
    pub fn v1() -> Self {
        Self {
            total_rounds: known_round(5),
            skill_pick_seconds: SKILL_PICK_SECONDS,
            pre_combat_seconds: PRE_COMBAT_SECONDS,
        }
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
