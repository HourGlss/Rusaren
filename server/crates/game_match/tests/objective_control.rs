use game_domain::{
    MatchId, PlayerId, PlayerName, PlayerRecord, SkillChoice, SkillTree, TeamAssignment, TeamSide,
};
use game_match::{MatchConfig, MatchEvent, MatchPhase, MatchSession};

const TEST_OBJECTIVE_TARGET_MS: u32 = 180_000;
const TEST_SKILL_PICK_SECONDS: u8 = 25;
const TEST_PRE_COMBAT_SECONDS: u8 = 5;
const TEST_TOTAL_ROUNDS: u8 = 5;

fn player_id(raw: u32) -> PlayerId {
    match PlayerId::new(raw) {
        Ok(value) => value,
        Err(error) => panic!("valid player id: {error}"),
    }
}

fn assignment(raw_id: u32, raw_name: &str, team: TeamSide) -> TeamAssignment {
    TeamAssignment {
        player_id: player_id(raw_id),
        player_name: match PlayerName::new(raw_name) {
            Ok(value) => value,
            Err(error) => panic!("valid player name: {error}"),
        },
        record: PlayerRecord::new(),
        team,
    }
}

fn skill(tree: SkillTree, tier: u8) -> SkillChoice {
    match SkillChoice::new(tree, tier) {
        Ok(value) => value,
        Err(error) => panic!("valid skill choice: {error}"),
    }
}

fn combat_session() -> MatchSession {
    let match_id = match MatchId::new(1) {
        Ok(value) => value,
        Err(error) => panic!("match id: {error}"),
    };
    let config = match MatchConfig::new(
        TEST_TOTAL_ROUNDS,
        TEST_SKILL_PICK_SECONDS,
        TEST_PRE_COMBAT_SECONDS,
        TEST_OBJECTIVE_TARGET_MS,
    ) {
        Ok(value) => value,
        Err(error) => panic!("config: {error}"),
    };
    let mut session = match MatchSession::new(
        match_id,
        vec![
            assignment(1, "Alice", TeamSide::TeamA),
            assignment(2, "Bob", TeamSide::TeamB),
        ],
        config,
    ) {
        Ok(value) => value,
        Err(error) => panic!("session: {error}"),
    };
    if let Err(error) = session.submit_skill_pick(player_id(1), skill(SkillTree::Mage, 1)) {
        panic!("team A pick: {error}");
    }
    if let Err(error) = session.submit_skill_pick(player_id(2), skill(SkillTree::Rogue, 1)) {
        panic!("team B pick: {error}");
    }
    if let Err(error) = session.advance_phase_by(TEST_PRE_COMBAT_SECONDS) {
        panic!("combat should start: {error}");
    }
    session
}

#[test]
fn objective_control_accumulates_for_both_teams_when_they_share_the_center() {
    let mut session = combat_session();

    for _ in 0..3 {
        let events = match session.advance_objective_control(true, true, 60_000) {
            Ok(value) => value,
            Err(error) => panic!("objective tick: {error}"),
        };
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

    let events = match session.advance_objective_control(true, false, 1_000) {
        Ok(value) => value,
        Err(error) => panic!("tie-break objective tick: {error}"),
    };
    let round_one = match game_domain::RoundNumber::new(1) {
        Ok(value) => value,
        Err(error) => panic!("round: {error}"),
    };
    assert_eq!(
        events,
        vec![MatchEvent::RoundWon {
            round: round_one,
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
            seconds_remaining: TEST_SKILL_PICK_SECONDS,
        }
    );
    assert_eq!(session.objective_control_ms(), (0, 0));
}

#[test]
fn objective_control_tracks_each_team_independently_before_a_round_resolves() {
    let mut session = combat_session();

    let events = match session.advance_objective_control(true, false, 15_000) {
        Ok(value) => value,
        Err(error) => panic!("team A control: {error}"),
    };
    assert!(events.is_empty());
    let events = match session.advance_objective_control(false, true, 10_000) {
        Ok(value) => value,
        Err(error) => panic!("team B control: {error}"),
    };
    assert!(events.is_empty());

    assert_eq!(session.objective_control_ms(), (15_000, 10_000));
    assert_eq!(session.phase(), &MatchPhase::Combat);
}
