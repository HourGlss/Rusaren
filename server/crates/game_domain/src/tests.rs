use super::*;
use proptest::prelude::*;
use std::fmt;

fn assert_ok<T, E: fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected Ok(..), got Err({error:?})"),
    }
}

fn valid_player_name_strategy() -> impl Strategy<Value = String> {
    let alphabet: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-"
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
    assert_eq!(
        PlayerName::new("Alice\t2"),
        Err(DomainError::PlayerNameInvalidCharacter { ch: '\t' })
    );
    assert_eq!(
        PlayerName::new("Alice\n2"),
        Err(DomainError::PlayerNameInvalidCharacter { ch: '\n' })
    );
    assert_eq!(
        PlayerName::new("=cmd"),
        Err(DomainError::PlayerNameInvalidCharacter { ch: '=' })
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
        progress.can_apply(&rogue_two),
        Err(DomainError::SkillTierGap {
            tree: SkillTree::Rogue,
            expected: 1,
            actual: 2,
        })
    );

    let mut progress = LoadoutProgress::new();
    assert_eq!(progress.apply(&rogue_one), Ok(()));
    assert_eq!(progress.tier_for(&SkillTree::Rogue), 1);
    assert_eq!(progress.apply(&rogue_two), Ok(()));
    assert_eq!(progress.tier_for(&SkillTree::Rogue), 2);
    assert_eq!(
        progress.can_apply(&rogue_one),
        Err(DomainError::SkillTierGap {
            tree: SkillTree::Rogue,
            expected: 3,
            actual: 1,
        })
    );
    assert_eq!(progress.can_apply(&rogue_three), Ok(()));
}

#[test]
fn skill_tree_accepts_custom_classes_and_rejects_bad_names() {
    let druid = assert_ok(SkillTree::new("Druid"));
    assert_eq!(druid.as_str(), "Druid");
    assert_ne!(druid, SkillTree::Mage);

    assert_eq!(SkillTree::new("   "), Err(DomainError::SkillTreeNameEmpty));
    assert_eq!(
        SkillTree::new("Beast@Master"),
        Err(DomainError::SkillTreeNameInvalidCharacter { ch: '@' })
    );
    assert_eq!(
        SkillTree::new("A".repeat(MAX_SKILL_TREE_NAME_LEN + 1)),
        Err(DomainError::SkillTreeNameTooLong {
            len: MAX_SKILL_TREE_NAME_LEN + 1,
            max: MAX_SKILL_TREE_NAME_LEN,
        })
    );
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
