use super::*;

#[test]
fn poison_and_hot_tick_with_expected_stacking_behavior() {
    let content = content();
    let mut world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Rogue,
                [Some(choice(SkillTree::Rogue, 1)), None, None, None, None],
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::new("Druid").expect("druid tree"),
                [
                    None,
                    None,
                    Some(choice(SkillTree::new("Druid").expect("druid tree"), 3)),
                    None,
                    None,
                ],
            ),
        ],
    );
    {
        let alice = world.players.get_mut(&player_id(1)).expect("alice");
        alice.x = -400;
        alice.y = 0;
        alice.aim_x = 100;
        alice.aim_y = 0;
        let bob = world.players.get_mut(&player_id(2)).expect("bob");
        bob.x = -240;
        bob.y = 0;
        bob.aim_x = 0;
        bob.aim_y = 1;
        bob.hit_points = 70;
    }

    world.queue_cast(player_id(1), 1).expect("poison cast");
    let _ = world.tick(COMBAT_FRAME_MS);
    for _ in 0..8 {
        let _ = world.tick(COMBAT_FRAME_MS);
    }
    let poison_statuses = world.statuses_for(player_id(2)).expect("statuses");
    assert!(poison_statuses
        .iter()
        .any(|status| status.kind == StatusKind::Poison));
    let damaged_hit_points = world.player_state(player_id(2)).expect("bob").hit_points;
    assert!(damaged_hit_points < 70);

    let hot_skill = world
        .players
        .get(&player_id(2))
        .and_then(|player| player.skills[2].clone())
        .expect("hot should be equipped");
    let _ = resolve_skill_cast(&mut world, player_id(2), 3, hot_skill.behavior);
    let _ = collect_ticks(&mut world, 10);
    let hot_statuses = world.statuses_for(player_id(2)).expect("statuses");
    assert!(hot_statuses
        .iter()
        .any(|status| status.kind == StatusKind::Hot));
    let bob = world.player_state(player_id(2)).expect("bob");
    assert!(bob.hit_points > damaged_hit_points);
}

#[test]
fn poison_stacks_and_hot_refreshes_from_the_same_source() {
    let content = content();
    let mut poison_world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Rogue,
                [Some(choice(SkillTree::Rogue, 1)), None, None, None, None],
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [Some(choice(SkillTree::Warrior, 1)), None, None, None, None],
            ),
        ],
    );
    {
        let alice = poison_world.players.get_mut(&player_id(1)).expect("alice");
        alice.x = -360;
        alice.y = 0;
        alice.aim_x = 100;
        alice.aim_y = 0;
        let bob = poison_world.players.get_mut(&player_id(2)).expect("bob");
        bob.x = -200;
        bob.y = 0;
    }

    for _ in 0..2 {
        poison_world
            .queue_cast(player_id(1), 1)
            .expect("poison cast");
        let _ = poison_world.tick(COMBAT_FRAME_MS);
        for _ in 0..8 {
            let _ = poison_world.tick(COMBAT_FRAME_MS);
        }
    }

    let poison_statuses = poison_world.statuses_for(player_id(2)).expect("statuses");
    let poison = poison_statuses
        .iter()
        .find(|status| status.kind == StatusKind::Poison)
        .expect("poison should exist");
    assert_eq!(poison.stacks, 2);

    let mut hot_world = world(
        &content,
        vec![seed(
            &content,
            1,
            "Druid",
            TeamSide::TeamA,
            SkillTree::new("Druid").expect("druid tree"),
            [
                None,
                None,
                Some(choice(SkillTree::new("Druid").expect("druid tree"), 3)),
                None,
                None,
            ],
        )],
    );
    {
        let cleric = hot_world.players.get_mut(&player_id(1)).expect("cleric");
        cleric.hit_points = 80;
    }

    let hot_skill = hot_world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[2].clone())
        .expect("hot should be equipped");
    let _ = resolve_skill_cast(&mut hot_world, player_id(1), 3, hot_skill.behavior);
    let _ = collect_ticks(&mut hot_world, 18);
    let hot_before_refresh = hot_world
        .statuses_for(player_id(1))
        .expect("statuses")
        .into_iter()
        .find(|status| status.kind == StatusKind::Hot)
        .expect("hot should exist before refresh");

    for _ in 0..4 {
        let _ = hot_world.tick(COMBAT_FRAME_MS);
    }
    let _ = resolve_skill_cast(&mut hot_world, player_id(1), 3, hot_skill.behavior);
    let hot_after_refresh = hot_world
        .statuses_for(player_id(1))
        .expect("statuses")
        .into_iter()
        .find(|status| status.kind == StatusKind::Hot)
        .expect("hot should exist after refresh");
    assert_eq!(hot_after_refresh.stacks, 1);
    assert!(hot_after_refresh.remaining_ms >= hot_before_refresh.remaining_ms);
}

#[test]
fn poison_and_hot_remove_themselves_after_their_authored_durations() {
    let content = content();

    let mut poison_world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Rogue",
                TeamSide::TeamA,
                SkillTree::Rogue,
                [Some(choice(SkillTree::Rogue, 1)), None, None, None, None],
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
        &mut poison_world,
        player_id(1),
        -360,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut poison_world,
        player_id(2),
        -200,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );
    poison_world
        .queue_cast(player_id(1), 1)
        .expect("poison cast should queue");
    let _ = poison_world.tick(COMBAT_FRAME_MS);
    let _ = collect_ticks(&mut poison_world, 45);
    assert!(
        poison_world
            .statuses_for(player_id(2))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Poison),
        "poison should expire after its duration"
    );

    let mut hot_world = world(
        &content,
        vec![seed(
            &content,
            1,
            "Druid",
            TeamSide::TeamA,
            SkillTree::new("Druid").expect("druid tree"),
            [
                None,
                None,
                Some(choice(SkillTree::new("Druid").expect("druid tree"), 3)),
                None,
                None,
            ],
        )],
    );
    {
        let cleric = hot_world.players.get_mut(&player_id(1)).expect("cleric");
        cleric.hit_points = 70;
    }
    let hot_skill = hot_world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[2].clone())
        .expect("hot should be equipped");
    let _ = resolve_skill_cast(&mut hot_world, player_id(1), 3, hot_skill.behavior);
    let _ = collect_ticks(&mut hot_world, 90);
    assert!(
        hot_world
            .statuses_for(player_id(1))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Hot),
        "hot should expire after its duration"
    );
}

#[test]
fn chill_stacks_slow_then_root_target() {
    let content = content();
    let mut world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Mage,
                [
                    Some(choice(SkillTree::Mage, 1)),
                    Some(choice(SkillTree::Mage, 2)),
                    Some(choice(SkillTree::Mage, 3)),
                    None,
                    None,
                ],
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [Some(choice(SkillTree::Warrior, 1)), None, None, None, None],
            ),
        ],
    );
    {
        let alice = world.players.get_mut(&player_id(1)).expect("alice");
        alice.x = -200;
        alice.y = 0;
        alice.aim_x = 100;
        alice.aim_y = 0;
        let bob = world.players.get_mut(&player_id(2)).expect("bob");
        bob.x = -20;
        bob.y = 0;
    }

    for stack in 1..=3 {
        let burst_skill = world
            .players
            .get(&player_id(1))
            .and_then(|player| player.skills[2].clone())
            .expect("burst should be equipped");
        let _ = resolve_skill_cast(&mut world, player_id(1), 3, burst_skill.behavior);
        let statuses = world.statuses_for(player_id(2)).expect("statuses");
        let chill = statuses
            .iter()
            .find(|status| status.kind == StatusKind::Chill)
            .expect("chill should be active after each burst");
        assert_eq!(chill.stacks, stack);
        let rooted = statuses
            .iter()
            .any(|status| status.kind == StatusKind::Root);
        assert_eq!(rooted, stack == 3);
        for _ in 0..20 {
            let _ = world.tick(COMBAT_FRAME_MS);
        }
    }
}

#[test]
fn haste_increases_movement_speed_and_expires_cleanly() {
    let content = content();
    let mut world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Bard",
                TeamSide::TeamA,
                SkillTree::new("Bard").expect("bard tree"),
                [None, None, None, Some(choice(SkillTree::new("Bard").expect("bard tree"), 4)), None],
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
        &mut world,
        player_id(1),
        -700,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        -460,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    let haste_skill = world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[3].clone())
        .expect("haste skill");
    let cast_events = resolve_skill_cast(&mut world, player_id(1), 4, haste_skill.behavior);
    assert!(status_applied_to(&cast_events, player_id(2), StatusKind::Haste).is_some());

    let bob_before = world.player_state(player_id(2)).expect("bob");
    world
        .submit_input(player_id(2), MovementIntent::new(1, 0).expect("intent"))
        .expect("movement input");
    let move_events = world.tick(COMBAT_FRAME_MS);
    let bob_after_haste = world.player_state(player_id(2)).expect("bob");
    assert!(moved_player(&move_events, player_id(2)).is_some());
    let hasted_distance = bob_after_haste.x - bob_before.x;
    assert!(
        hasted_distance
            > i16::try_from(travel_distance_units(
                PLAYER_MOVE_SPEED_UNITS_PER_SECOND,
                COMBAT_FRAME_MS
            ))
            .unwrap_or(i16::MAX),
        "haste should increase movement distance per frame"
    );

    world
        .submit_input(player_id(2), MovementIntent::zero())
        .expect("stop movement");
    let _ = collect_ticks(&mut world, 25);
    assert!(
        world
            .statuses_for(player_id(2))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Haste),
        "haste should expire after its duration"
    );

    let bob_before_normal = world.player_state(player_id(2)).expect("bob");
    world
        .submit_input(player_id(2), MovementIntent::new(1, 0).expect("intent"))
        .expect("movement input");
    let _ = world.tick(COMBAT_FRAME_MS);
    let bob_after_normal = world.player_state(player_id(2)).expect("bob");
    let normal_distance = bob_after_normal.x - bob_before_normal.x;
    assert_eq!(
        normal_distance,
        i16::try_from(travel_distance_units(
            PLAYER_MOVE_SPEED_UNITS_PER_SECOND,
            COMBAT_FRAME_MS
        ))
        .unwrap_or(i16::MAX),
        "movement should return to the baseline speed after haste expires"
    );
}

#[test]
fn silence_blocks_skill_casts_but_not_primary_attacks_until_it_expires() {
    let content = content();
    let mut world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::new("Bard").expect("bard tree"),
                content
                    .skills()
                    .resolve(&choice(SkillTree::new("Bard").expect("bard tree"), 3))
                    .expect("bard silence skill"),
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Mage,
                [Some(choice(SkillTree::Mage, 1)), None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut world,
        player_id(1),
        -220,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        -140,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    let silence_skill = world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[0].clone())
        .expect("silence skill");
    let cast_events = resolve_skill_cast(&mut world, player_id(1), 1, silence_skill.behavior);
    assert!(status_applied_to(&cast_events, player_id(2), StatusKind::Silence).is_some());

    world
        .queue_primary_attack(player_id(2))
        .expect("primary queue");
    world.queue_cast(player_id(2), 1).expect("cast queue");
    let blocked_events = world.tick(COMBAT_FRAME_MS);
    assert!(
        damage_to(&blocked_events, player_id(1)).is_some(),
        "silenced players should still be able to melee"
    );
    assert!(
        !effect_spawned_by(&blocked_events, player_id(2), 1),
        "silenced players should not cast skills"
    );
    let bob_state = world.player_state(player_id(2)).expect("bob");
    assert_eq!(
        bob_state.slot_cooldown_remaining_ms[0], 0,
        "blocked casts should not start cooldowns"
    );

    let _ = collect_ticks(&mut world, 13);
    assert!(
        world
            .statuses_for(player_id(2))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Silence),
        "silence should expire"
    );

    let mage_skill = world
        .players
        .get(&player_id(2))
        .and_then(|player| player.skills[0].clone())
        .expect("mage skill");
    let post_events = resolve_skill_cast(&mut world, player_id(2), 1, mage_skill.behavior);
    assert!(
        effect_spawned_by(&post_events, player_id(2), 1),
        "casts should resume after silence expires"
    );
}

#[test]
fn stun_blocks_movement_and_all_actions_until_it_expires() {
    let content = content();
    let mut world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::new("Paladin").expect("paladin tree"),
                content
                    .skills()
                    .resolve(&choice(SkillTree::new("Paladin").expect("paladin tree"), 2))
                    .expect("paladin stun skill"),
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Rogue,
                [Some(choice(SkillTree::Rogue, 1)), None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut world,
        player_id(1),
        -700,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        -515,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    let stun_skill = world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[0].clone())
        .expect("stun skill");
    let cast_events = resolve_skill_cast(&mut world, player_id(1), 1, stun_skill.behavior);
    assert!(status_applied_to(&cast_events, player_id(2), StatusKind::Stun).is_some());

    let bob_before = world.player_state(player_id(2)).expect("bob");
    world
        .submit_input(player_id(2), MovementIntent::new(1, 0).expect("intent"))
        .expect("movement queue");
    world
        .queue_primary_attack(player_id(2))
        .expect("primary queue");
    world.queue_cast(player_id(2), 1).expect("cast queue");
    let blocked_events = world.tick(COMBAT_FRAME_MS);
    let bob_after = world.player_state(player_id(2)).expect("bob");
    assert_eq!(bob_after.x, bob_before.x, "stun should block movement");
    assert!(
        moved_player(&blocked_events, player_id(2)).is_none(),
        "stun should not emit movement events for the stunned player"
    );
    assert!(
        !effect_spawned_by(&blocked_events, player_id(2), 0)
            && !effect_spawned_by(&blocked_events, player_id(2), 1),
        "stun should block both melee and skill actions"
    );
    assert_eq!(
        bob_after.slot_cooldown_remaining_ms[0], 0,
        "blocked stunned casts should not start cooldowns"
    );

    let _ = collect_ticks(&mut world, 8);
    assert!(
        world
            .statuses_for(player_id(2))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Stun),
        "stun should expire"
    );

    world.queue_cast(player_id(2), 1).expect("post-stun cast");
    let mut post_events = world.tick(COMBAT_FRAME_MS);
    post_events.extend(collect_ticks(&mut world, 10));
    assert!(
        effect_spawned_by(&post_events, player_id(2), 1),
        "stunned players should be able to cast again once the stun ends"
    );
}

#[test]
fn chill_reduces_movement_speed_before_root_expires_cleanly() {
    let content = content();
    let mut world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Mage",
                TeamSide::TeamA,
                SkillTree::Mage,
                [None, None, Some(choice(SkillTree::Mage, 3)), None, None],
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
        &mut world,
        player_id(1),
        -260,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        0,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    let frost_burst = world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[2].clone())
        .expect("frost burst");
    let cast_events = resolve_skill_cast(&mut world, player_id(1), 3, frost_burst.behavior);
    assert_eq!(
        status_applied_to(&cast_events, player_id(2), StatusKind::Chill),
        Some(1)
    );

    let bob_before = world.player_state(player_id(2)).expect("bob");
    world
        .submit_input(player_id(2), MovementIntent::new(1, 0).expect("intent"))
        .expect("movement queue");
    let _ = world.tick(COMBAT_FRAME_MS);
    let bob_after = world.player_state(player_id(2)).expect("bob");
    let chilled_distance = bob_after.x - bob_before.x;
    assert!(
        chilled_distance
            < i16::try_from(travel_distance_units(
                PLAYER_MOVE_SPEED_UNITS_PER_SECOND,
                COMBAT_FRAME_MS
            ))
            .unwrap_or(i16::MAX),
        "chill should slow movement before rooting"
    );

    let _ = collect_ticks(&mut world, 25);
    assert!(
        world
            .statuses_for(player_id(2))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Chill && status.kind != StatusKind::Root),
        "chill and its derived root should both expire"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn sleep_blocks_actions_and_breaks_on_damage_from_any_source() {
    let content = content();
    let druid_tree = SkillTree::new("Druid").expect("druid tree");
    let mut world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                druid_tree.clone(),
                content
                    .skills()
                    .resolve(&choice(druid_tree.clone(), 5))
                    .expect("druid sleep skill"),
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Mage,
                [Some(choice(SkillTree::Mage, 1)), None, None, None, None],
            ),
            seed(
                &content,
                3,
                "Charlie",
                TeamSide::TeamB,
                SkillTree::Mage,
                [Some(choice(SkillTree::Mage, 1)), None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut world,
        player_id(1),
        -260,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        -40,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(3),
        -220,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );

    let sleep_skill = world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[0].clone())
        .expect("sleep skill");
    let cast_events = resolve_skill_cast(&mut world, player_id(1), 1, sleep_skill.behavior);
    assert!(status_applied_to(&cast_events, player_id(2), StatusKind::Sleep).is_some());

    let bob_before = world.player_state(player_id(2)).expect("bob");
    world
        .submit_input(player_id(2), MovementIntent::new(-1, 0).expect("intent"))
        .expect("movement queue");
    world
        .queue_primary_attack(player_id(2))
        .expect("primary queue");
    world.queue_cast(player_id(2), 1).expect("cast queue");
    let blocked_events = world.tick(COMBAT_FRAME_MS);
    let bob_after = world.player_state(player_id(2)).expect("bob");
    assert_eq!(bob_after.x, bob_before.x, "sleep should block movement");
    assert!(
        !effect_spawned_by(&blocked_events, player_id(2), 0)
            && !effect_spawned_by(&blocked_events, player_id(2), 1),
        "sleep should block both primary and skill actions"
    );

    let ally_projectile_skill = world
        .players
        .get(&player_id(3))
        .and_then(|player| player.skills[0].clone())
        .expect("ally projectile skill");
    let projectile_events =
        resolve_skill_cast(&mut world, player_id(3), 1, ally_projectile_skill.behavior);
    assert!(
        damage_to(&projectile_events, player_id(2)).is_some(),
        "ally damage should still land on a sleeping player"
    );
    assert!(
        world
            .statuses_for(player_id(2))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Sleep),
        "sleep should break on damage from any source"
    );

    world
        .queue_primary_attack(player_id(2))
        .expect("primary queue");
    let wake_events = world.tick(COMBAT_FRAME_MS);
    assert!(
        effect_spawned_by(&wake_events, player_id(2), 0),
        "players should act again once sleep is broken"
    );
}

#[test]
fn shield_stacks_and_absorbs_damage_before_hit_points() {
    let content = content();
    let mut world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Cleric",
                TeamSide::TeamA,
                SkillTree::Cleric,
                content
                    .skills()
                    .resolve(&choice(SkillTree::Cleric, 3))
                    .expect("cleric shield skill"),
            ),
            seed(
                &content,
                2,
                "Warrior",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [Some(choice(SkillTree::Warrior, 1)), None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut world,
        player_id(1),
        -420,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        -260,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    let shield_skill = world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[0].clone())
        .expect("shield skill");
    let first_cast = resolve_skill_cast(&mut world, player_id(1), 1, shield_skill.behavior);
    assert!(status_applied_to(&first_cast, player_id(1), StatusKind::Shield).is_some());
    let _ = collect_ticks(&mut world, 20);
    let second_cast = resolve_skill_cast(&mut world, player_id(1), 1, shield_skill.behavior);
    assert_eq!(
        status_applied_to(&second_cast, player_id(1), StatusKind::Shield),
        Some(2)
    );

    let beam_skill = world
        .players
        .get(&player_id(2))
        .and_then(|player| player.skills[0].clone())
        .expect("warrior beam");
    let first_hit = resolve_skill_cast(&mut world, player_id(2), 1, beam_skill.behavior);
    assert_eq!(
        world.player_state(player_id(1)).expect("cleric").hit_points,
        100,
        "stacked shields should absorb the first hit before HP changes"
    );
    assert!(
        damage_to(&first_hit, player_id(1)).is_none()
            || damage_to(&first_hit, player_id(1)) == Some(0),
        "fully absorbed hits should not reduce HP"
    );
    assert!(
        world
            .statuses_for(player_id(1))
            .expect("statuses")
            .iter()
            .any(|status| status.kind == StatusKind::Shield),
        "some shield should remain after the first absorbed hit"
    );

    let _ = collect_ticks(&mut world, 11);
    let second_hit = resolve_skill_cast(&mut world, player_id(2), 1, beam_skill.behavior);
    assert!(
        damage_to(&second_hit, player_id(1)).is_some(),
        "later hits should spill through once the stacked shields are exhausted"
    );
    assert!(
        world.player_state(player_id(1)).expect("cleric").hit_points < 100,
        "shield overflow should eventually damage hit points"
    );
}

#[test]
fn stealth_blocks_targeting_and_breaks_on_actions() {
    let content = content();
    let rogue_tree = SkillTree::Rogue;
    let mut world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Rogue",
                TeamSide::TeamA,
                rogue_tree.clone(),
                content
                    .skills()
                    .resolve(&choice(rogue_tree.clone(), 4))
                    .expect("rogue stealth skill"),
            ),
            seed(
                &content,
                2,
                "Mage",
                TeamSide::TeamB,
                SkillTree::Mage,
                [Some(choice(SkillTree::Mage, 1)), None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut world,
        player_id(1),
        -260,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        -40,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    let stealth_skill = world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[0].clone())
        .expect("stealth skill");
    let _ = resolve_skill_cast(&mut world, player_id(1), 1, stealth_skill.behavior);
    let _ = collect_ticks(&mut world, 10);
    assert!(
        world
            .statuses_for(player_id(1))
            .expect("statuses")
            .iter()
            .any(|status| status.kind == StatusKind::Stealth),
        "the rogue should enter stealth after the aura pulses"
    );

    let projectile_skill = world
        .players
        .get(&player_id(2))
        .and_then(|player| player.skills[0].clone())
        .expect("projectile skill");
    let projectile_events = resolve_skill_cast(&mut world, player_id(2), 1, projectile_skill.behavior);
    assert!(
        damage_to(&projectile_events, player_id(1)).is_none(),
        "stealthed players should not be hittable by ordinary targeted projectiles"
    );

    world
        .queue_primary_attack(player_id(1))
        .expect("primary queue");
    let action_events = world.tick(COMBAT_FRAME_MS);
    assert!(
        effect_spawned_by(&action_events, player_id(1), 0),
        "the rogue should still be able to act while stealthed"
    );
    assert!(
        world
            .statuses_for(player_id(1))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Stealth),
        "taking an action should immediately break stealth"
    );
}

#[test]
fn fear_forces_movement_away_and_blocks_actions() {
    let content = content();
    let mut world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Warrior",
                TeamSide::TeamA,
                SkillTree::Warrior,
                content
                    .skills()
                    .resolve(&choice(SkillTree::Warrior, 3))
                    .expect("warrior fear skill"),
            ),
            seed(
                &content,
                2,
                "Mage",
                TeamSide::TeamB,
                SkillTree::Mage,
                [Some(choice(SkillTree::Mage, 1)), None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut world,
        player_id(1),
        -260,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        -160,
        TEST_OPEN_LANE_Y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    let fear_skill = world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[0].clone())
        .expect("fear skill");
    let cast_events = resolve_skill_cast(&mut world, player_id(1), 1, fear_skill.behavior);
    assert!(status_applied_to(&cast_events, player_id(2), StatusKind::Fear).is_some());

    let bob_before = world.player_state(player_id(2)).expect("bob");
    world
        .submit_input(player_id(2), MovementIntent::new(-1, 0).expect("intent"))
        .expect("movement queue");
    world
        .queue_primary_attack(player_id(2))
        .expect("primary queue");
    world.queue_cast(player_id(2), 1).expect("cast queue");
    let blocked_events = world.tick(COMBAT_FRAME_MS);
    let bob_after = world.player_state(player_id(2)).expect("bob");
    assert!(
        bob_after.x > bob_before.x,
        "fear should force movement away from the applier"
    );
    assert!(
        !effect_spawned_by(&blocked_events, player_id(2), 0)
            && !effect_spawned_by(&blocked_events, player_id(2), 1),
        "fear should block all player actions while it is active"
    );
}
