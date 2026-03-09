//! Match and round flow orchestration.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::collections::BTreeMap;
use std::fmt;

use game_domain::{
    DomainError, LoadoutProgress, MatchId, MatchOutcome, PlayerId, RoundNumber, SkillChoice,
    TeamAssignment, TeamSide,
};

pub const SKILL_PICK_SECONDS: u8 = 25;
pub const PRE_COMBAT_SECONDS: u8 = 5;

fn known_round(value: u8) -> RoundNumber {
    match RoundNumber::new(value) {
        Ok(round) => round,
        Err(error) => panic!("internal invariant violated: round {value} should be valid: {error}"),
    }
}

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

    fn award_round(&mut self, winner: TeamSide) {
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

#[derive(Clone, Debug)]
pub struct MatchSession {
    _match_id: MatchId,
    config: MatchConfig,
    current_round: RoundNumber,
    phase: MatchPhase,
    score: ScoreBoard,
    players: BTreeMap<PlayerId, MatchPlayer>,
}

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
                    equipped_slots: [None; 5],
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
            .apply(choice)
            .map_err(MatchError::InvalidSkillChoice)?;
        player.selected_for_round = Some(choice);
        player.equipped_slots[usize::from(self.current_round.get() - 1)] = Some(choice);

        let mut events = vec![MatchEvent::SkillChosen { player_id, choice }];
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
            return Ok(vec![
                round_event,
                MatchEvent::MatchEnded {
                    outcome,
                    message,
                    score: self.score.clone(),
                },
            ]);
        }

        self.current_round = match self.current_round.next() {
            Some(next_round) => next_round,
            None => panic!("internal invariant violated: non-final round should have a successor"),
        };
        self.reset_for_next_round();
        Ok(vec![round_event])
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
    pub fn equipped_choice(&self, player_id: PlayerId, slot: u8) -> Option<SkillChoice> {
        if !(1..=5).contains(&slot) {
            return None;
        }

        self.players
            .get(&player_id)
            .and_then(|player| player.equipped_slots[usize::from(slot - 1)])
    }

    fn phase_name(&self) -> &'static str {
        match self.phase {
            MatchPhase::SkillPick { .. } => "SkillPick",
            MatchPhase::PreCombat { .. } => "PreCombat",
            MatchPhase::Combat => "Combat",
            MatchPhase::MatchEnd { .. } => "MatchEnd",
        }
    }

    fn expect_phase(&self, expected: &'static str) -> Result<(), MatchError> {
        if self.phase_name() == expected {
            Ok(())
        } else {
            Err(MatchError::WrongPhase {
                expected,
                actual: self.phase_name(),
            })
        }
    }

    fn reset_for_next_round(&mut self) {
        for player in self.players.values_mut() {
            player.alive = true;
            player.selected_for_round = None;
        }
        self.phase = MatchPhase::SkillPick {
            seconds_remaining: self.config.skill_pick_seconds,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_domain::{PlayerName, PlayerRecord, SkillTree};

    fn player_id(raw: u32) -> PlayerId {
        PlayerId::new(raw).expect("valid player id")
    }

    fn assignment(raw_id: u32, raw_name: &str, team: TeamSide) -> TeamAssignment {
        TeamAssignment {
            player_id: player_id(raw_id),
            player_name: PlayerName::new(raw_name).expect("valid player name"),
            record: PlayerRecord::new(),
            team,
        }
    }

    fn session() -> MatchSession {
        MatchSession::new(
            MatchId::new(1).expect("valid match id"),
            vec![
                assignment(1, "Alice", TeamSide::TeamA),
                assignment(2, "Bob", TeamSide::TeamB),
            ],
            MatchConfig::v1(),
        )
        .expect("match session should build")
    }

    fn skill(tree: SkillTree, tier: u8) -> SkillChoice {
        SkillChoice::new(tree, tier).expect("skill choice should be valid")
    }

    #[test]
    fn match_new_requires_players_on_both_teams_and_unique_ids() {
        let config = MatchConfig::v1();
        let match_id = MatchId::new(1).expect("valid match id");

        assert!(matches!(
            MatchSession::new(
                match_id,
                vec![assignment(1, "Alice", TeamSide::TeamA)],
                config,
            ),
            Err(MatchError::MissingTeam(TeamSide::TeamB))
        ));

        assert!(matches!(
            MatchSession::new(
                match_id,
                vec![
                    assignment(1, "Alice", TeamSide::TeamA),
                    assignment(1, "AliceClone", TeamSide::TeamB),
                ],
                config,
            ),
            Err(MatchError::DuplicatePlayer(player)) if player == player_id(1)
        ));
    }

    #[test]
    fn submit_skill_pick_accepts_valid_progression_and_rejects_invalid_inputs() {
        let mut session = session();

        assert_eq!(
            session.submit_skill_pick(player_id(9), skill(SkillTree::Mage, 1)),
            Err(MatchError::PlayerMissing(player_id(9)))
        );

        assert_eq!(
            session.submit_skill_pick(player_id(1), skill(SkillTree::Mage, 2)),
            Err(MatchError::InvalidSkillChoice(DomainError::SkillTierGap {
                tree: SkillTree::Mage,
                expected: 1,
                actual: 2,
            }))
        );

        assert_eq!(
            session.submit_skill_pick(player_id(1), skill(SkillTree::Mage, 1)),
            Ok(vec![MatchEvent::SkillChosen {
                player_id: player_id(1),
                choice: skill(SkillTree::Mage, 1),
            }])
        );

        assert_eq!(
            session.submit_skill_pick(player_id(1), skill(SkillTree::Mage, 2)),
            Err(MatchError::SkillAlreadySelected(player_id(1)))
        );
    }

    #[test]
    fn all_skill_choices_transition_the_match_into_pre_combat() {
        let mut session = session();

        session
            .submit_skill_pick(player_id(1), skill(SkillTree::Mage, 1))
            .expect("first pick should work");

        let events = session
            .submit_skill_pick(player_id(2), skill(SkillTree::Rogue, 1))
            .expect("second pick should work");

        assert_eq!(
            events,
            vec![
                MatchEvent::SkillChosen {
                    player_id: player_id(2),
                    choice: skill(SkillTree::Rogue, 1),
                },
                MatchEvent::PreCombatStarted {
                    seconds_remaining: PRE_COMBAT_SECONDS,
                },
            ]
        );
        assert_eq!(
            session.phase(),
            &MatchPhase::PreCombat {
                seconds_remaining: PRE_COMBAT_SECONDS,
            }
        );
    }

    #[test]
    fn pre_combat_countdown_transitions_into_combat() {
        let mut session = session();
        session
            .submit_skill_pick(player_id(1), skill(SkillTree::Mage, 1))
            .expect("first pick should work");
        session
            .submit_skill_pick(player_id(2), skill(SkillTree::Rogue, 1))
            .expect("second pick should work");

        assert_eq!(
            session
                .advance_phase_by(4)
                .expect("countdown tick should work"),
            Vec::<MatchEvent>::new()
        );
        assert_eq!(
            session.phase(),
            &MatchPhase::PreCombat {
                seconds_remaining: 1,
            }
        );

        assert_eq!(
            session.advance_phase_by(1).expect("combat should start"),
            vec![MatchEvent::CombatStarted]
        );
        assert_eq!(session.phase(), &MatchPhase::Combat);
    }

    #[test]
    fn skill_pick_timeout_requires_manual_resolution_until_policy_is_defined() {
        let mut session = session();
        assert_eq!(
            session.advance_phase_by(SKILL_PICK_SECONDS),
            Ok(vec![MatchEvent::ManualResolutionRequired {
                reason: "skill-pick timeout reached without a timeout resolution policy",
            }])
        );
        assert_eq!(
            session.phase(),
            &MatchPhase::SkillPick {
                seconds_remaining: 0,
            }
        );
    }

    #[test]
    fn mark_player_defeated_requires_combat_and_rejects_invalid_targets() {
        let mut session = session();
        assert_eq!(
            session.mark_player_defeated(player_id(1)),
            Err(MatchError::WrongPhase {
                expected: "Combat",
                actual: "SkillPick",
            })
        );

        session
            .submit_skill_pick(player_id(1), skill(SkillTree::Mage, 1))
            .expect("first pick should work");
        session
            .submit_skill_pick(player_id(2), skill(SkillTree::Rogue, 1))
            .expect("second pick should work");
        session
            .advance_phase_by(PRE_COMBAT_SECONDS)
            .expect("combat should start");

        assert_eq!(
            session.mark_player_defeated(player_id(9)),
            Err(MatchError::PlayerMissing(player_id(9)))
        );
    }

    #[test]
    fn defeating_the_last_player_on_a_team_awards_the_round_and_resets_the_next_round() {
        let mut session = session();
        session
            .submit_skill_pick(player_id(1), skill(SkillTree::Mage, 1))
            .expect("first pick should work");
        session
            .submit_skill_pick(player_id(2), skill(SkillTree::Rogue, 1))
            .expect("second pick should work");
        session
            .advance_phase_by(PRE_COMBAT_SECONDS)
            .expect("combat should start");

        let events = session
            .mark_player_defeated(player_id(2))
            .expect("defeat should end the round");

        assert_eq!(
            events,
            vec![MatchEvent::RoundWon {
                round: RoundNumber::new(1).expect("round one should be valid"),
                winning_team: TeamSide::TeamA,
                score: ScoreBoard {
                    team_a: 1,
                    team_b: 0,
                },
            }]
        );
        assert_eq!(session.current_round().get(), 2);
        assert_eq!(
            session.phase(),
            &MatchPhase::SkillPick {
                seconds_remaining: SKILL_PICK_SECONDS,
            }
        );
        assert!(session.player(player_id(1)).expect("alice exists").alive);
        assert!(session.player(player_id(2)).expect("bob exists").alive);
        assert_eq!(session.score().team_a, 1);
    }

    #[test]
    fn chosen_skills_are_bound_to_round_slots_and_persist_across_rounds() {
        let mut session = session();
        session
            .submit_skill_pick(player_id(1), skill(SkillTree::Mage, 1))
            .expect("alice round one pick");
        session
            .submit_skill_pick(player_id(2), skill(SkillTree::Rogue, 1))
            .expect("bob round one pick");
        session
            .advance_phase_by(PRE_COMBAT_SECONDS)
            .expect("combat should start");
        session
            .mark_player_defeated(player_id(2))
            .expect("round one should end");

        assert_eq!(
            session.equipped_choice(player_id(1), 1),
            Some(skill(SkillTree::Mage, 1))
        );
        assert_eq!(session.equipped_choice(player_id(1), 2), None);

        session
            .submit_skill_pick(player_id(1), skill(SkillTree::Warrior, 1))
            .expect("alice round two pick");
        session
            .submit_skill_pick(player_id(2), skill(SkillTree::Cleric, 1))
            .expect("bob round two pick");

        assert_eq!(
            session.equipped_choice(player_id(1), 1),
            Some(skill(SkillTree::Mage, 1))
        );
        assert_eq!(
            session.equipped_choice(player_id(1), 2),
            Some(skill(SkillTree::Warrior, 1))
        );
    }

    #[test]
    fn fifth_round_completes_the_match_instead_of_ending_early() {
        let mut session = session();

        for round in 1..=5 {
            session
                .submit_skill_pick(player_id(1), skill(SkillTree::Mage, round))
                .expect("alice should progress each round");
            session
                .submit_skill_pick(player_id(2), skill(SkillTree::Rogue, round))
                .expect("bob should progress each round");
            session
                .advance_phase_by(PRE_COMBAT_SECONDS)
                .expect("combat should start");
            let events = session
                .mark_player_defeated(player_id(2))
                .expect("bob defeat should be valid");

            if round < 5 {
                assert_eq!(events.len(), 1);
                assert_eq!(session.current_round().get(), round + 1);
            } else {
                assert_eq!(events.len(), 2);
                assert!(matches!(
                    session.phase(),
                    MatchPhase::MatchEnd {
                        outcome: MatchOutcome::TeamAWin,
                        ..
                    }
                ));
            }
        }
    }

    #[test]
    fn disconnecting_any_player_ends_the_match_as_no_contest() {
        let mut session = session();
        let event = session
            .disconnect_player(player_id(2))
            .expect("disconnect should end the match");

        assert_eq!(
            event,
            MatchEvent::MatchEnded {
                outcome: MatchOutcome::NoContest,
                message: String::from("Bob has disconnected. Game is over."),
                score: ScoreBoard::new(),
            }
        );
        assert_eq!(
            session.phase(),
            &MatchPhase::MatchEnd {
                outcome: MatchOutcome::NoContest,
                message: String::from("Bob has disconnected. Game is over."),
            }
        );
    }

    #[test]
    fn defeat_rejects_double_kills_on_the_same_player() {
        let mut session = session();
        session
            .submit_skill_pick(player_id(1), skill(SkillTree::Mage, 1))
            .expect("first pick should work");
        session
            .submit_skill_pick(player_id(2), skill(SkillTree::Rogue, 1))
            .expect("second pick should work");
        session
            .advance_phase_by(PRE_COMBAT_SECONDS)
            .expect("combat should start");

        session
            .mark_player_defeated(player_id(2))
            .expect("first defeat should work");
        assert_eq!(
            session.mark_player_defeated(player_id(2)),
            Err(MatchError::WrongPhase {
                expected: "Combat",
                actual: "SkillPick",
            })
        );
    }
}
