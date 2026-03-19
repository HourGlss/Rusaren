use std::fmt;

use crate::{DomainError, PlayerId, MAX_PLAYER_NAME_LEN};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerName(String);

impl PlayerName {
    /// # Errors
    ///
    /// Returns a [`DomainError`] when the trimmed name is empty, too long, or
    /// contains characters outside `[A-Za-z0-9_-]`.
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let raw = raw.into();
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            return Err(DomainError::PlayerNameEmpty);
        }

        if trimmed.len() > MAX_PLAYER_NAME_LEN {
            return Err(DomainError::PlayerNameTooLong {
                len: trimmed.len(),
                max: MAX_PLAYER_NAME_LEN,
            });
        }

        if let Some(ch) = trimmed
            .chars()
            .find(|ch| !ch.is_ascii_alphanumeric() && *ch != '_' && *ch != '-')
        {
            return Err(DomainError::PlayerNameInvalidCharacter { ch });
        }

        Ok(Self(trimmed.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PlayerName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamSide {
    TeamA,
    TeamB,
}

impl TeamSide {
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::TeamA => Self::TeamB,
            Self::TeamB => Self::TeamA,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TeamA => "Team A",
            Self::TeamB => "Team B",
        }
    }
}

impl fmt::Display for TeamSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadyState {
    Ready,
    NotReady,
}

impl ReadyState {
    #[must_use]
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerRecord {
    pub wins: u16,
    pub losses: u16,
    pub no_contests: u16,
}

impl PlayerRecord {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            wins: 0,
            losses: 0,
            no_contests: 0,
        }
    }

    pub fn record_win(&mut self) {
        self.wins = self.wins.saturating_add(1);
    }

    pub fn record_loss(&mut self) {
        self.losses = self.losses.saturating_add(1);
    }

    pub fn record_no_contest(&mut self) {
        self.no_contests = self.no_contests.saturating_add(1);
    }

    #[must_use]
    pub const fn total_games(self) -> u16 {
        self.wins + self.losses + self.no_contests
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchOutcome {
    TeamAWin,
    TeamBWin,
    NoContest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamAssignment {
    pub player_id: PlayerId,
    pub player_name: PlayerName,
    pub record: PlayerRecord,
    pub team: TeamSide,
}
