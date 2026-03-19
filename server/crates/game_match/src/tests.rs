use super::*;
use game_domain::{
    DomainError, MatchOutcome, PlayerName, PlayerRecord, SkillChoice, SkillTree, TeamAssignment,
    TeamSide,
};

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
