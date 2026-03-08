//! Pure rules, core types, and game-domain state machines.

#![forbid(unsafe_code)]

use std::fmt;

pub const MAX_PLAYER_NAME_LEN: usize = 24;
pub const MAX_SKILL_TIER: u8 = 5;
pub const MAX_ROUNDS: u8 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    IdMustBeNonZero(&'static str),
    PlayerNameEmpty,
    PlayerNameTooLong {
        len: usize,
        max: usize,
    },
    PlayerNameInvalidCharacter {
        ch: char,
    },
    SkillTierOutOfRange {
        tier: u8,
        min: u8,
        max: u8,
    },
    SkillTierGap {
        tree: SkillTree,
        expected: u8,
        actual: u8,
    },
    RoundOutOfRange {
        round: u8,
        min: u8,
        max: u8,
    },
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdMustBeNonZero(kind) => write!(f, "{kind} must be non-zero"),
            Self::PlayerNameEmpty => write!(f, "player name must not be empty"),
            Self::PlayerNameTooLong { len, max } => {
                write!(f, "player name length {len} exceeds maximum {max}")
            }
            Self::PlayerNameInvalidCharacter { ch } => {
                write!(f, "player name contains invalid character '{ch}'")
            }
            Self::SkillTierOutOfRange { tier, min, max } => {
                write!(
                    f,
                    "skill tier {tier} is outside the allowed range {min}..={max}"
                )
            }
            Self::SkillTierGap {
                tree,
                expected,
                actual,
            } => write!(
                f,
                "skill progression for {tree} expected tier {expected} but received tier {actual}"
            ),
            Self::RoundOutOfRange { round, min, max } => {
                write!(
                    f,
                    "round {round} is outside the allowed range {min}..={max}"
                )
            }
        }
    }
}

impl std::error::Error for DomainError {}

macro_rules! non_zero_id {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            /// # Errors
            ///
            /// Returns [`DomainError::IdMustBeNonZero`] when `value` is zero.
            pub fn new(value: u32) -> Result<Self, DomainError> {
                if value == 0 {
                    return Err(DomainError::IdMustBeNonZero($label));
                }

                Ok(Self(value))
            }

            #[must_use]
            pub const fn get(self) -> u32 {
                self.0
            }
        }
    };
}

non_zero_id!(PlayerId, "player_id");
non_zero_id!(LobbyId, "lobby_id");
non_zero_id!(MatchId, "match_id");
non_zero_id!(EntityId, "entity_id");

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillTree {
    Warrior,
    Rogue,
    Mage,
    Cleric,
}

impl SkillTree {
    #[must_use]
    pub const fn as_index(self) -> usize {
        match self {
            Self::Warrior => 0,
            Self::Rogue => 1,
            Self::Mage => 2,
            Self::Cleric => 3,
        }
    }
}

impl fmt::Display for SkillTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Warrior => "Warrior",
            Self::Rogue => "Rogue",
            Self::Mage => "Mage",
            Self::Cleric => "Cleric",
        };

        f.write_str(name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillChoice {
    pub tree: SkillTree,
    pub tier: u8,
}

impl SkillChoice {
    /// # Errors
    ///
    /// Returns [`DomainError::SkillTierOutOfRange`] when `tier` is outside
    /// `1..=MAX_SKILL_TIER`.
    pub fn new(tree: SkillTree, tier: u8) -> Result<Self, DomainError> {
        if !(1..=MAX_SKILL_TIER).contains(&tier) {
            return Err(DomainError::SkillTierOutOfRange {
                tier,
                min: 1,
                max: MAX_SKILL_TIER,
            });
        }

        Ok(Self { tree, tier })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadoutProgress {
    unlocked_tiers: [u8; 4],
}

impl LoadoutProgress {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            unlocked_tiers: [0; 4],
        }
    }

    #[must_use]
    pub const fn tier_for(self, tree: SkillTree) -> u8 {
        self.unlocked_tiers[tree.as_index()]
    }

    /// # Errors
    ///
    /// Returns [`DomainError::SkillTierGap`] when `choice` does not continue the
    /// caller's current progression for that tree.
    pub fn can_apply(self, choice: SkillChoice) -> Result<(), DomainError> {
        let current = self.tier_for(choice.tree);
        let expected = if current == 0 { 1 } else { current + 1 };

        if choice.tier != expected {
            return Err(DomainError::SkillTierGap {
                tree: choice.tree,
                expected,
                actual: choice.tier,
            });
        }

        Ok(())
    }

    /// # Errors
    ///
    /// Returns the same errors as [`LoadoutProgress::can_apply`] when `choice`
    /// is not the next valid skill tier for its tree.
    pub fn apply(&mut self, choice: SkillChoice) -> Result<(), DomainError> {
        self.can_apply(choice)?;
        self.unlocked_tiers[choice.tree.as_index()] = choice.tier;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RoundNumber(u8);

impl RoundNumber {
    /// # Errors
    ///
    /// Returns [`DomainError::RoundOutOfRange`] when `value` is outside
    /// `1..=MAX_ROUNDS`.
    pub fn new(value: u8) -> Result<Self, DomainError> {
        if !(1..=MAX_ROUNDS).contains(&value) {
            return Err(DomainError::RoundOutOfRange {
                round: value,
                min: 1,
                max: MAX_ROUNDS,
            });
        }

        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    #[must_use]
    pub fn next(self) -> Option<Self> {
        if self.0 == MAX_ROUNDS {
            None
        } else {
            Some(Self(self.0 + 1))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamAssignment {
    pub player_id: PlayerId,
    pub player_name: PlayerName,
    pub record: PlayerRecord,
    pub team: TeamSide,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn assert_ok<T, E: fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("expected Ok(..), got Err({error:?})"),
        }
    }

    fn valid_player_name_strategy() -> impl Strategy<Value = String> {
        let alphabet: Vec<char> =
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-"
                .chars()
                .collect();

        proptest::collection::vec(proptest::sample::select(alphabet), 1..=MAX_PLAYER_NAME_LEN)
            .prop_map(|chars| chars.into_iter().collect())
    }

    #[test]
    fn ids_reject_zero_and_accept_positive_values() {
        assert_eq!(
            PlayerId::new(0),
            Err(DomainError::IdMustBeNonZero("player_id"))
        );
        assert_eq!(PlayerId::new(1).map(PlayerId::get), Ok(1));
        assert_eq!(PlayerId::new(u32::MAX).map(PlayerId::get), Ok(u32::MAX));

        assert_eq!(
            LobbyId::new(0),
            Err(DomainError::IdMustBeNonZero("lobby_id"))
        );
        assert_eq!(LobbyId::new(1).map(LobbyId::get), Ok(1));

        assert_eq!(
            MatchId::new(0),
            Err(DomainError::IdMustBeNonZero("match_id"))
        );
        assert_eq!(MatchId::new(1).map(MatchId::get), Ok(1));

        assert_eq!(
            EntityId::new(0),
            Err(DomainError::IdMustBeNonZero("entity_id"))
        );
        assert_eq!(EntityId::new(1).map(EntityId::get), Ok(1));
    }

    #[test]
    fn player_name_accepts_trimmed_ascii_identifiers_and_rejects_bad_values() {
        let name = assert_ok(PlayerName::new("  Alice-1_2  "));
        assert_eq!(name.as_str(), "Alice-1_2");

        assert_eq!(PlayerName::new("   "), Err(DomainError::PlayerNameEmpty));

        let long_name = "A".repeat(MAX_PLAYER_NAME_LEN + 1);
        assert_eq!(
            PlayerName::new(long_name),
            Err(DomainError::PlayerNameTooLong {
                len: MAX_PLAYER_NAME_LEN + 1,
                max: MAX_PLAYER_NAME_LEN,
            })
        );

        assert_eq!(
            PlayerName::new("bad name"),
            Err(DomainError::PlayerNameInvalidCharacter { ch: ' ' })
        );
    }

    #[test]
    fn team_side_round_trips_and_player_record_accumulates_outcomes() {
        assert_eq!(TeamSide::TeamA.other(), TeamSide::TeamB);
        assert_eq!(TeamSide::TeamB.other(), TeamSide::TeamA);

        let mut record = PlayerRecord::new();
        assert_eq!(record.total_games(), 0);
        record.record_win();
        record.record_loss();
        record.record_no_contest();

        assert_eq!(
            record,
            PlayerRecord {
                wins: 1,
                losses: 1,
                no_contests: 1,
            }
        );
        assert_eq!(record.total_games(), 3);
    }

    #[test]
    fn skill_choice_and_progression_enforce_boundaries() {
        assert_eq!(
            SkillChoice::new(SkillTree::Mage, 0),
            Err(DomainError::SkillTierOutOfRange {
                tier: 0,
                min: 1,
                max: MAX_SKILL_TIER,
            })
        );
        assert_eq!(
            SkillChoice::new(SkillTree::Mage, 6),
            Err(DomainError::SkillTierOutOfRange {
                tier: 6,
                min: 1,
                max: MAX_SKILL_TIER,
            })
        );

        let rogue_one = assert_ok(SkillChoice::new(SkillTree::Rogue, 1));
        let rogue_two = assert_ok(SkillChoice::new(SkillTree::Rogue, 2));
        let rogue_three = assert_ok(SkillChoice::new(SkillTree::Rogue, 3));

        let progress = LoadoutProgress::new();
        assert_eq!(
            progress.can_apply(rogue_two),
            Err(DomainError::SkillTierGap {
                tree: SkillTree::Rogue,
                expected: 1,
                actual: 2,
            })
        );

        let mut progress = LoadoutProgress::new();
        assert_eq!(progress.apply(rogue_one), Ok(()));
        assert_eq!(progress.tier_for(SkillTree::Rogue), 1);
        assert_eq!(progress.apply(rogue_two), Ok(()));
        assert_eq!(progress.tier_for(SkillTree::Rogue), 2);
        assert_eq!(
            progress.can_apply(rogue_one),
            Err(DomainError::SkillTierGap {
                tree: SkillTree::Rogue,
                expected: 3,
                actual: 1,
            })
        );
        assert_eq!(progress.can_apply(rogue_three), Ok(()));
    }

    #[test]
    fn round_number_accepts_valid_bounds_and_rejects_out_of_range_values() {
        assert_eq!(
            RoundNumber::new(0),
            Err(DomainError::RoundOutOfRange {
                round: 0,
                min: 1,
                max: MAX_ROUNDS,
            })
        );
        assert_eq!(RoundNumber::new(1).map(RoundNumber::get), Ok(1));
        assert_eq!(
            RoundNumber::new(MAX_ROUNDS).map(RoundNumber::get),
            Ok(MAX_ROUNDS)
        );
        assert_eq!(
            RoundNumber::new(MAX_ROUNDS + 1),
            Err(DomainError::RoundOutOfRange {
                round: MAX_ROUNDS + 1,
                min: 1,
                max: MAX_ROUNDS,
            })
        );
        assert_eq!(assert_ok(RoundNumber::new(MAX_ROUNDS)).next(), None);
    }

    proptest! {
        #[test]
        fn prop_player_name_accepts_all_valid_ascii_identifiers(
            raw in valid_player_name_strategy()
        ) {
            let name = PlayerName::new(raw.clone());
            prop_assert_eq!(name.as_ref().map(PlayerName::as_str), Ok(raw.as_str()));
        }

        #[test]
        fn prop_player_id_accepts_all_positive_values(raw in 1_u32..) {
            prop_assert_eq!(PlayerId::new(raw).map(PlayerId::get), Ok(raw));
        }
    }
}
