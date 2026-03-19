//! Match and round flow orchestration.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::collections::BTreeMap;

use game_domain::{MatchId, PlayerId, RoundNumber};

pub const SKILL_PICK_SECONDS: u8 = 25;
pub const PRE_COMBAT_SECONDS: u8 = 5;

fn known_round(value: u8) -> RoundNumber {
    match RoundNumber::new(value) {
        Ok(round) => round,
        Err(error) => panic!("internal invariant violated: round {value} should be valid: {error}"),
    }
}

mod accessors;
mod flow;
mod types;

pub use types::{MatchConfig, MatchError, MatchEvent, MatchPhase, MatchPlayer, ScoreBoard};

#[derive(Clone, Debug)]
pub struct MatchSession {
    _match_id: MatchId,
    config: MatchConfig,
    current_round: RoundNumber,
    phase: MatchPhase,
    score: ScoreBoard,
    players: BTreeMap<PlayerId, MatchPlayer>,
}

#[cfg(test)]
mod tests;
