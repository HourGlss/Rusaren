use super::*;

#[test]
fn cast_time_delays_skill_resolution_until_the_windup_finishes() {
    let content = content();
    let skill = content
        .skills()
        .resolve(&choice(SkillTree::Warrior, 3))
        .expect("warrior tier three should exist")
        .clone();
    let cast_time_ms = skill.behavior.cast_time_ms();
    assert!(cast_time_ms > 0);

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
        TEST_ATTACKER_X + 100,
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
    assert_eq!(start_state.current_cast_remaining_ms, cast_time_ms);

    let ticks_until_resolution = usize::from(cast_time_ms.div_ceil(COMBAT_FRAME_MS));
    for elapsed_ticks in 1..ticks_until_resolution {
        let tick_events = world.tick(COMBAT_FRAME_MS);
        assert!(
            damage_to(&tick_events, player_id(2)).is_none(),
            "casts should not resolve before the final windup tick"
        );
        let expected_remaining = cast_time_ms
            .saturating_sub(u16::try_from(elapsed_ticks).unwrap_or(u16::MAX) * COMBAT_FRAME_MS);
        assert_eq!(
            world
                .player_state(player_id(1))
                .expect("attacker state")
                .current_cast_remaining_ms,
            expected_remaining
        );
    }

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
        TEST_ATTACKER_X + 100,
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

#[test]
fn start_pending_cast_requires_stillness_mana_and_sets_active_cast_state() {
    let content = content();
    let skill = content
        .skills()
        .resolve(&choice(SkillTree::Warrior, 3))
        .expect("warrior tier three should exist")
        .clone();
    let mut world = world(
        &content,
        vec![seed_with_slot_one_skill(
            &content,
            1,
            "Alice",
            TeamSide::TeamA,
            SkillTree::Warrior,
            &skill,
        )],
    );
    set_player_pose(
        &mut world,
        player_id(1),
        TEST_ATTACKER_X,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );

    let attacker_state = world.player_state(player_id(1)).expect("attacker");
    assert!(world.test_start_pending_cast(
        player_id(1),
        attacker_state,
        1,
        0,
        skill.behavior.clone(),
    ));
    let active_cast = world.players[&player_id(1)]
        .active_cast
        .as_ref()
        .expect("pending cast should be active");
    assert_eq!(active_cast.slot, 1);
    assert!(active_cast.total_ms > 0);

    world
        .players
        .get_mut(&player_id(1))
        .expect("player")
        .active_cast = None;
    world.players.get_mut(&player_id(1)).expect("player").moving = true;
    let moving_state = world.player_state(player_id(1)).expect("moving attacker");
    assert!(!world.test_start_pending_cast(
        player_id(1),
        moving_state,
        1,
        0,
        skill.behavior.clone(),
    ));

    world.players.get_mut(&player_id(1)).expect("player").moving = false;
    world.players.get_mut(&player_id(1)).expect("player").mana =
        skill.behavior.mana_cost().saturating_sub(1);
    let low_mana_state = world.player_state(player_id(1)).expect("low mana attacker");
    assert!(!world.test_start_pending_cast(
        player_id(1),
        low_mana_state,
        1,
        0,
        skill.behavior.clone(),
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn beam_dash_burst_and_nova_resolvers_emit_effects_and_apply_payloads() {
    let content = content();

    let beam_skill = content
        .skills()
        .resolve(&choice(SkillTree::Warrior, 1))
        .expect("warrior beam")
        .clone();
    assert!(matches!(beam_skill.behavior, SkillBehavior::Beam { .. }));
    let mut beam_world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Warrior,
                &beam_skill,
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Mage,
                [None, None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut beam_world,
        player_id(1),
        TEST_ATTACKER_X,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut beam_world,
        player_id(2),
        TEST_ATTACKER_X + 120,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );
    let beam_events = resolve_skill_cast(&mut beam_world, player_id(1), 1, beam_skill.behavior);
    assert!(effect_spawned_by(&beam_events, player_id(1), 1));
    assert!(damage_to(&beam_events, player_id(2)).is_some());

    let dash_skill = content
        .skills()
        .resolve(&choice(SkillTree::new("Paladin").expect("paladin tree"), 2))
        .expect("paladin dash")
        .clone();
    assert!(matches!(dash_skill.behavior, SkillBehavior::Dash { .. }));
    let mut dash_world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::new("Paladin").expect("paladin tree"),
                &dash_skill,
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Mage,
                [None, None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut dash_world,
        player_id(1),
        TEST_ATTACKER_X,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut dash_world,
        player_id(2),
        TEST_ATTACKER_X + 185,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );
    let dash_events = resolve_skill_cast(&mut dash_world, player_id(1), 1, dash_skill.behavior);
    assert!(effect_spawned_by(&dash_events, player_id(1), 1));
    assert!(moved_player(&dash_events, player_id(1)).is_some());
    assert!(damage_to(&dash_events, player_id(2)).is_some());

    let burst_skill = content
        .skills()
        .resolve(&choice(SkillTree::Mage, 3))
        .expect("mage burst")
        .clone();
    assert!(matches!(burst_skill.behavior, SkillBehavior::Burst { .. }));
    let mut burst_world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Mage,
                &burst_skill,
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut burst_world,
        player_id(1),
        TEST_ATTACKER_X,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut burst_world,
        player_id(2),
        TEST_ATTACKER_X + 260,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );
    let burst_events = resolve_skill_cast(&mut burst_world, player_id(1), 1, burst_skill.behavior);
    assert!(effect_spawned_by(&burst_events, player_id(1), 1));
    assert!(damage_to(&burst_events, player_id(2)).is_some());
    assert!(status_applied_to(&burst_events, player_id(2), StatusKind::Chill).is_some());

    let nova_skill = content
        .skills()
        .resolve(&choice(SkillTree::Warrior, 3))
        .expect("warrior nova")
        .clone();
    assert!(matches!(nova_skill.behavior, SkillBehavior::Nova { .. }));
    let mut nova_world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Warrior,
                &nova_skill,
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Mage,
                [None, None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut nova_world,
        player_id(1),
        TEST_ATTACKER_X,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut nova_world,
        player_id(2),
        TEST_ATTACKER_X + 60,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );
    let nova_events = resolve_skill_cast(&mut nova_world, player_id(1), 1, nova_skill.behavior);
    assert!(effect_spawned_by(&nova_events, player_id(1), 1));
    assert!(damage_to(&nova_events, player_id(2)).is_some());
    assert!(status_applied_to(&nova_events, player_id(2), StatusKind::Fear).is_some());
}

#[test]
fn channels_tick_while_maintained_and_manual_cancel_stops_future_ticks() {
    let content = content();
    let skill = content
        .skills()
        .resolve(&choice(SkillTree::Warrior, 5))
        .expect("warrior channel should exist")
        .clone();
    let SkillBehavior::Channel {
        tick_interval_ms,
        duration_ms,
        ..
    } = skill.behavior
    else {
        panic!("warrior tier five should remain a channel");
    };

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
                [None, None, None, None, None],
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
        TEST_ATTACKER_X + 80,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    let start_events = activate_skill_cast(&mut world, player_id(1), 1, &skill.behavior);
    assert!(
        damage_to(&start_events, player_id(2)).is_none(),
        "channel windups should not deal damage before the first maintained tick"
    );
    let started_channel = world.player_state(player_id(1)).expect("attacker");
    assert_eq!(started_channel.current_cast_slot, Some(1));
    assert_eq!(started_channel.current_cast_total_ms, duration_ms);

    let first_tick = collect_ticks(
        &mut world,
        usize::from(tick_interval_ms / COMBAT_FRAME_MS + 1),
    );
    assert!(
        damage_to(&first_tick, player_id(2)).is_some(),
        "channels should repeatedly apply their payload while maintained"
    );
    assert_eq!(
        world
            .player_state(player_id(1))
            .expect("attacker")
            .current_cast_slot,
        Some(1),
        "the caster should still be channeling after the first tick"
    );

    assert!(
        world
            .cancel_active_cast(player_id(1))
            .expect("cancel should succeed"),
        "manual cancel should report that it stopped an active channel"
    );
    assert_eq!(
        world
            .player_state(player_id(1))
            .expect("attacker")
            .current_cast_slot,
        None
    );
    let post_cancel_events =
        collect_ticks(&mut world, usize::from(duration_ms / COMBAT_FRAME_MS + 2));
    assert!(
        damage_to(&post_cancel_events, player_id(2)).is_none(),
        "manual cancel should stop future channel ticks"
    );
}

#[test]
fn movement_input_cancels_active_channels_mid_stream() {
    let content = content();
    let skill = content
        .skills()
        .resolve(&choice(SkillTree::Warrior, 5))
        .expect("warrior channel should exist")
        .clone();
    let SkillBehavior::Channel {
        tick_interval_ms, ..
    } = skill.behavior
    else {
        panic!("warrior tier five should remain a channel");
    };

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
                [None, None, None, None, None],
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
        TEST_ATTACKER_X + 80,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    let _ = activate_skill_cast(&mut world, player_id(1), 1, &skill.behavior);
    let _ = collect_ticks(
        &mut world,
        usize::from(tick_interval_ms / COMBAT_FRAME_MS + 1),
    );
    assert_eq!(
        world
            .player_state(player_id(1))
            .expect("attacker")
            .current_cast_slot,
        Some(1),
        "the channel should be active before the movement cancel"
    );

    world
        .submit_input(player_id(1), MovementIntent::new(1, 0).expect("movement"))
        .expect("movement should queue");
    let cancel_tick = world.tick(COMBAT_FRAME_MS);
    assert!(
        moved_player(&cancel_tick, player_id(1)).is_some(),
        "movement input should still move the player"
    );
    assert_eq!(
        world
            .player_state(player_id(1))
            .expect("attacker")
            .current_cast_slot,
        None,
        "moving should immediately cancel an active channel"
    );
}
