use std::fmt;

use crate::SkillTree;

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
    SkillTreeNameEmpty,
    SkillTreeNameTooLong {
        len: usize,
        max: usize,
    },
    SkillTreeNameInvalidCharacter {
        ch: char,
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
            Self::SkillTreeNameEmpty => write!(f, "skill tree name must not be empty"),
            Self::SkillTreeNameTooLong { len, max } => {
                write!(f, "skill tree name length {len} exceeds maximum {max}")
            }
            Self::SkillTreeNameInvalidCharacter { ch } => {
                write!(f, "skill tree name contains invalid character '{ch}'")
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
