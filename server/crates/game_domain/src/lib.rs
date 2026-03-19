//! Pure rules, core types, and game-domain state machines.

#![forbid(unsafe_code)]

mod error;
mod ids;
mod player;
mod round;
mod skill;

pub use error::DomainError;
pub use ids::{EntityId, LobbyId, MatchId, PlayerId};
pub use player::{MatchOutcome, PlayerName, PlayerRecord, ReadyState, TeamAssignment, TeamSide};
pub use round::RoundNumber;
pub use skill::{KnownSkillTree, LoadoutProgress, SkillChoice, SkillTree};

pub const MAX_PLAYER_NAME_LEN: usize = 24;
pub const MAX_SKILL_TREE_NAME_LEN: usize = 32;
pub const MAX_SKILL_TIER: u8 = 5;
pub const MAX_ROUNDS: u8 = 5;

#[cfg(test)]
mod tests;
