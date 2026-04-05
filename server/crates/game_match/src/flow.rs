use std::collections::BTreeMap;

use game_domain::{LoadoutProgress, MatchOutcome, SkillChoice, TeamAssignment, TeamSide};

use super::{
    known_round, MatchConfig, MatchError, MatchEvent, MatchId, MatchPhase, MatchPlayer,
    MatchSession, PlayerId, ScoreBoard,
};

impl MatchSession {
    pub fn new(
        match_id: MatchId,
        roster: Vec<TeamAssignment>,
        config: MatchConfig,
    ) -> Result<Self, MatchError> {
        let mut players = BTreeMap::new();
        let mut team_a = 0usize;
        let mut team_b = 0usize;

        for assignment in roster {
            let player_id = assignment.player_id;
            if players.contains_key(&player_id) {
                return Err(MatchError::DuplicatePlayer(player_id));
            }

            match assignment.team {
                TeamSide::TeamA => team_a += 1,
                TeamSide::TeamB => team_b += 1,
            }

            players.insert(
                player_id,
                MatchPlayer {
                    assignment,
                    loadout_progress: LoadoutProgress::new(),
                    selected_for_round: None,
                    equipped_slots: [const { None }; 5],
                    alive: true,
                },
            );
        }

        if team_a == 0 {
            return Err(MatchError::MissingTeam(TeamSide::TeamA));
        }
        if team_b == 0 {
            return Err(MatchError::MissingTeam(TeamSide::TeamB));
        }

        Ok(Self {
            _match_id: match_id,
            config,
            current_round: known_round(1),
            phase: MatchPhase::SkillPick {
                seconds_remaining: config.skill_pick_seconds,
            },
            score: ScoreBoard::new(),
            objective_team_a_ms: 0,
            objective_team_b_ms: 0,
            players,
        })
    }

    pub fn submit_skill_pick(
        &mut self,
        player_id: PlayerId,
        choice: SkillChoice,
    ) -> Result<Vec<MatchEvent>, MatchError> {
        self.expect_phase("SkillPick")?;

        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(MatchError::PlayerMissing(player_id))?;

        if player.selected_for_round.is_some() {
            return Err(MatchError::SkillAlreadySelected(player_id));
        }

        player
            .loadout_progress
            .apply(&choice)
            .map_err(MatchError::InvalidSkillChoice)?;
        player.selected_for_round = Some(choice.clone());
        player.equipped_slots[usize::from(self.current_round.get() - 1)] = Some(choice.clone());

        let mut events = vec![MatchEvent::SkillChosen {
            player_id,
            slot: self.current_round.get(),
            choice,
        }];
        if self
            .players
            .values()
            .all(|entry| entry.selected_for_round.is_some())
        {
            self.phase = MatchPhase::PreCombat {
                seconds_remaining: self.config.pre_combat_seconds,
            };
            events.push(MatchEvent::PreCombatStarted {
                seconds_remaining: self.config.pre_combat_seconds,
            });
        }

        Ok(events)
    }

    pub fn advance_phase_by(&mut self, seconds: u8) -> Result<Vec<MatchEvent>, MatchError> {
        if seconds == 0 {
            return Ok(Vec::new());
        }

        match self.phase.clone() {
            MatchPhase::SkillPick { seconds_remaining } => {
                let selections_complete = self
                    .players
                    .values()
                    .all(|player| player.selected_for_round.is_some());

                if selections_complete {
                    return Err(MatchError::WrongPhase {
                        expected: "manual skill submissions",
                        actual: "SkillPick with all selections complete",
                    });
                }

                let next_remaining = seconds_remaining.saturating_sub(seconds);
                self.phase = MatchPhase::SkillPick {
                    seconds_remaining: next_remaining,
                };

                if next_remaining == 0 {
                    Ok(vec![MatchEvent::ManualResolutionRequired {
                        reason: "skill-pick timeout reached without a timeout resolution policy",
                    }])
                } else {
                    Ok(Vec::new())
                }
            }
            MatchPhase::PreCombat { seconds_remaining } => {
                let next_remaining = seconds_remaining.saturating_sub(seconds);
                if next_remaining == 0 {
                    self.phase = MatchPhase::Combat;
                    Ok(vec![MatchEvent::CombatStarted])
                } else {
                    self.phase = MatchPhase::PreCombat {
                        seconds_remaining: next_remaining,
                    };
                    Ok(Vec::new())
                }
            }
            MatchPhase::Combat => Err(MatchError::WrongPhase {
                expected: "SkillPick or PreCombat",
                actual: "Combat",
            }),
            MatchPhase::MatchEnd { .. } => Err(MatchError::WrongPhase {
                expected: "an active round phase",
                actual: "MatchEnd",
            }),
        }
    }

    pub fn mark_player_defeated(
        &mut self,
        player_id: PlayerId,
    ) -> Result<Vec<MatchEvent>, MatchError> {
        self.expect_phase("Combat")?;

        let defeated_team = {
            let player = self
                .players
                .get_mut(&player_id)
                .ok_or(MatchError::PlayerMissing(player_id))?;

            if !player.alive {
                return Err(MatchError::PlayerAlreadyDefeated(player_id));
            }

            player.alive = false;
            player.assignment.team
        };

        let team_is_defeated = self
            .players
            .values()
            .filter(|player| player.assignment.team == defeated_team)
            .all(|player| !player.alive);

        if !team_is_defeated {
            return Ok(Vec::new());
        }

        let winning_team = defeated_team.other();
        Ok(self.finish_round(winning_team))
    }

    pub fn advance_objective_control(
        &mut self,
        team_a_present: bool,
        team_b_present: bool,
        delta_ms: u16,
    ) -> Result<Vec<MatchEvent>, MatchError> {
        self.expect_phase("Combat")?;

        if team_a_present {
            self.objective_team_a_ms = self
                .objective_team_a_ms
                .saturating_add(u32::from(delta_ms))
                .min(self.config.objective_target_ms.saturating_mul(2));
        }
        if team_b_present {
            self.objective_team_b_ms = self
                .objective_team_b_ms
                .saturating_add(u32::from(delta_ms))
                .min(self.config.objective_target_ms.saturating_mul(2));
        }

        let winner = if self.objective_team_a_ms >= self.config.objective_target_ms
            && self.objective_team_b_ms >= self.config.objective_target_ms
        {
            match self.objective_team_a_ms.cmp(&self.objective_team_b_ms) {
                std::cmp::Ordering::Greater => Some(TeamSide::TeamA),
                std::cmp::Ordering::Less => Some(TeamSide::TeamB),
                std::cmp::Ordering::Equal => None,
            }
        } else if self.objective_team_a_ms >= self.config.objective_target_ms {
            Some(TeamSide::TeamA)
        } else if self.objective_team_b_ms >= self.config.objective_target_ms {
            Some(TeamSide::TeamB)
        } else {
            None
        };

        Ok(winner.map_or_else(Vec::new, |team| self.finish_round(team)))
    }

    pub fn disconnect_player(&mut self, player_id: PlayerId) -> Result<MatchEvent, MatchError> {
        let player_name = self
            .players
            .get(&player_id)
            .ok_or(MatchError::PlayerMissing(player_id))?
            .assignment
            .player_name
            .clone();

        let message = format!("{player_name} has disconnected. Game is over.");
        self.phase = MatchPhase::MatchEnd {
            outcome: MatchOutcome::NoContest,
            message: message.clone(),
        };

        Ok(MatchEvent::MatchEnded {
            outcome: MatchOutcome::NoContest,
            message,
            score: self.score.clone(),
        })
    }

    fn finish_round(&mut self, winning_team: TeamSide) -> Vec<MatchEvent> {
        self.score.award_round(winning_team);

        let round_event = MatchEvent::RoundWon {
            round: self.current_round,
            winning_team,
            score: self.score.clone(),
        };

        if self.current_round == self.config.total_rounds {
            let outcome = match winning_team {
                TeamSide::TeamA => MatchOutcome::TeamAWin,
                TeamSide::TeamB => MatchOutcome::TeamBWin,
            };
            let message = format!(
                "{} wins {}-{} after round {}.",
                winning_team,
                self.score.team_a,
                self.score.team_b,
                self.current_round.get()
            );
            self.phase = MatchPhase::MatchEnd {
                outcome,
                message: message.clone(),
            };
            return vec![
                round_event,
                MatchEvent::MatchEnded {
                    outcome,
                    message,
                    score: self.score.clone(),
                },
            ];
        }

        self.current_round = match self.current_round.next() {
            Some(next_round) => next_round,
            None => panic!("internal invariant violated: non-final round should have a successor"),
        };
        self.reset_for_next_round();
        vec![round_event]
    }
}
