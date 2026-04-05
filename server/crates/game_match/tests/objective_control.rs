use game_domain::{
    MatchId, PlayerId, PlayerName, PlayerRecord, SkillChoice, SkillTree, TeamAssignment, TeamSide,
};
use game_match::{MatchConfig, MatchEvent, MatchPhase, MatchSession, SKILL_PICK_SECONDS};

const TEST_OBJECTIVE_TARGET_MS: u32 = 180_000;

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

fn skill(tree: SkillTree, tier: u8) -> SkillChoice {
    SkillChoice::new(tree, tier).expect("valid skill choice")
}

fn combat_session() -> MatchSession {
    let mut session = MatchSession::new(
        MatchId::new(1).expect("match id"),
        vec![
            assignment(1, "Alice", TeamSide::TeamA),
            assignment(2, "Bob", TeamSide::TeamB),
        ],
        MatchConfig::v1(TEST_OBJECTIVE_TARGET_MS),
    )
    .expect("session");
    session
        .submit_skill_pick(player_id(1), skill(SkillTree::Mage, 1))
        .expect("team A pick");
    session
        .submit_skill_pick(player_id(2), skill(SkillTree::Rogue, 1))
        .expect("team B pick");
    session
        .advance_phase_by(game_match::PRE_COMBAT_SECONDS)
        .expect("combat should start");
    session
}

#[test]
fn objective_control_accumulates_for_both_teams_when_they_share_the_center() {
    let mut session = combat_session();

    for _ in 0..3 {
        let events = session
            .advance_objective_control(true, true, 60_000)
            .expect("objective tick");
        assert!(
            events.is_empty(),
            "a tied objective race should not resolve the round"
        );
    }

    assert_eq!(
        session.objective_control_ms(),
        (TEST_OBJECTIVE_TARGET_MS, TEST_OBJECTIVE_TARGET_MS)
    );
    assert_eq!(session.phase(), &MatchPhase::Combat);

    let events = session
        .advance_objective_control(true, false, 1_000)
        .expect("tie-break objective tick");
    assert_eq!(
        events,
        vec![MatchEvent::RoundWon {
            round: game_domain::RoundNumber::new(1).expect("round"),
            winning_team: TeamSide::TeamA,
            score: game_match::ScoreBoard {
                team_a: 1,
                team_b: 0
            },
        }]
    );
    assert_eq!(
        session.phase(),
        &MatchPhase::SkillPick {
            seconds_remaining: SKILL_PICK_SECONDS,
        }
    );
    assert_eq!(session.objective_control_ms(), (0, 0));
}

#[test]
fn objective_control_tracks_each_team_independently_before_a_round_resolves() {
    let mut session = combat_session();

    let events = session
        .advance_objective_control(true, false, 15_000)
        .expect("team A control");
    assert!(events.is_empty());
    let events = session
        .advance_objective_control(false, true, 10_000)
        .expect("team B control");
    assert!(events.is_empty());

    assert_eq!(session.objective_control_ms(), (15_000, 10_000));
    assert_eq!(session.phase(), &MatchPhase::Combat);
}
