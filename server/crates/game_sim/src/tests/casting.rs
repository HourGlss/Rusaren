use super::*;

#[test]
fn cast_time_delays_skill_resolution_until_the_windup_finishes() {
    let content = content();
    let skill = content
        .skills()
        .resolve(&choice(SkillTree::Warrior, 3))
        .expect("warrior tier three should exist")
        .clone();
    assert_eq!(skill.behavior.cast_time_ms(), 250);

    let mut world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Warrior,
                &skill,
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Mage,
                [const { None }; 5],
            ),
        ],
    );
    set_player_pose(
        &mut world,
        player_id(1),
        TEST_ATTACKER_X,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        TEST_ATTACKER_X + 220,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    world.queue_cast(player_id(1), 1).expect("cast");

    let start_events = world.tick(COMBAT_FRAME_MS);
    assert!(
        damage_to(&start_events, player_id(2)).is_none(),
        "casts should not resolve on the start tick"
    );
    let start_state = world.player_state(player_id(1)).expect("attacker state");
    assert_eq!(start_state.current_cast_slot, Some(1));
    assert_eq!(start_state.current_cast_remaining_ms, 250);

    let second_tick = world.tick(COMBAT_FRAME_MS);
    assert!(damage_to(&second_tick, player_id(2)).is_none());
    assert_eq!(
        world
            .player_state(player_id(1))
            .expect("attacker state")
            .current_cast_remaining_ms,
        150
    );

    let third_tick = world.tick(COMBAT_FRAME_MS);
    assert!(damage_to(&third_tick, player_id(2)).is_none());

    let fourth_tick = world.tick(COMBAT_FRAME_MS);
    assert!(
        damage_to(&fourth_tick, player_id(2)).is_some(),
        "casts should resolve once the cast timer reaches zero"
    );
    assert_eq!(
        world
            .player_state(player_id(1))
            .expect("attacker state")
            .current_cast_slot,
        None
    );
}

#[test]
fn movement_input_cancels_active_casts_before_the_skill_fires() {
    let content = content();
    let skill = content
        .skills()
        .resolve(&choice(SkillTree::Warrior, 3))
        .expect("warrior tier three should exist")
        .clone();

    let mut world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Warrior,
                &skill,
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Mage,
                [const { None }; 5],
            ),
        ],
    );
    set_player_pose(
        &mut world,
        player_id(1),
        TEST_ATTACKER_X,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        TEST_ATTACKER_X + 220,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    world.queue_cast(player_id(1), 1).expect("cast");
    let _ = world.tick(COMBAT_FRAME_MS);
    assert_eq!(
        world
            .player_state(player_id(1))
            .expect("attacker state")
            .current_cast_slot,
        Some(1)
    );

    world
        .submit_input(
            player_id(1),
            MovementIntent::new(1, 0).expect("movement input should be valid"),
        )
        .expect("movement");
    let cancel_tick = world.tick(COMBAT_FRAME_MS);
    assert!(
        moved_player(&cancel_tick, player_id(1)).is_some(),
        "movement input should move the player while canceling the cast"
    );
    assert_eq!(
        world
            .player_state(player_id(1))
            .expect("attacker state")
            .current_cast_slot,
        None
    );

    let remaining_events = collect_ticks(&mut world, 4);
    assert!(
        damage_to(&remaining_events, player_id(2)).is_none(),
        "canceled casts should never apply their payload"
    );
}
