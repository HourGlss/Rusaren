use super::{MatchError, MatchPhase, MatchPlayer, MatchSession, PlayerId, RoundNumber, ScoreBoard};
use game_domain::SkillChoice;

impl MatchSession {
    #[must_use]
    pub fn phase(&self) -> &MatchPhase {
        &self.phase
    }

    #[must_use]
    pub fn current_round(&self) -> RoundNumber {
        self.current_round
    }

    #[must_use]
    pub fn score(&self) -> &ScoreBoard {
        &self.score
    }

    #[must_use]
    pub fn player(&self, player_id: PlayerId) -> Option<&MatchPlayer> {
        self.players.get(&player_id)
    }

    #[must_use]
    pub fn objective_control_ms(&self) -> (u32, u32) {
        (self.objective_team_a_ms, self.objective_team_b_ms)
    }

    #[must_use]
    pub fn equipped_choice(&self, player_id: PlayerId, slot: u8) -> Option<SkillChoice> {
        if !(1..=5).contains(&slot) {
            return None;
        }

        self.players
            .get(&player_id)
            .and_then(|player| player.equipped_slots[usize::from(slot - 1)].clone())
    }

    fn phase_name(&self) -> &'static str {
        match self.phase {
            MatchPhase::SkillPick { .. } => "SkillPick",
            MatchPhase::PreCombat { .. } => "PreCombat",
            MatchPhase::Combat => "Combat",
            MatchPhase::MatchEnd { .. } => "MatchEnd",
        }
    }

    pub(crate) fn expect_phase(&self, expected: &'static str) -> Result<(), MatchError> {
        if self.phase_name() == expected {
            Ok(())
        } else {
            Err(MatchError::WrongPhase {
                expected,
                actual: self.phase_name(),
            })
        }
    }

    pub(crate) fn reset_for_next_round(&mut self) {
        for player in self.players.values_mut() {
            player.alive = true;
            player.selected_for_round = None;
        }
        self.phase = MatchPhase::SkillPick {
            seconds_remaining: self.config.skill_pick_seconds,
        };
        self.objective_team_a_ms = 0;
        self.objective_team_b_ms = 0;
    }
}
