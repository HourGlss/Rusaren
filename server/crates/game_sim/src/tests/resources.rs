use super::*;

#[test]
fn skill_cooldown_state_counts_down_before_a_second_cast_can_land() {
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
                [Some(choice(SkillTree::Mage, 1)), None, None, None, None],
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
        alice.x = -400;
        alice.y = 0;
        alice.aim_x = 100;
        alice.aim_y = 0;
        let bob = world.players.get_mut(&player_id(2)).expect("bob");
        bob.x = -200;
        bob.y = 0;
    }

    world.queue_cast(player_id(1), 1).expect("first cast");
    let _ = world.tick(COMBAT_FRAME_MS);
    let after_cast = world.player_state(player_id(1)).expect("alice");
    assert!(after_cast.slot_cooldown_remaining_ms[0] > 0);
    assert_eq!(after_cast.slot_cooldown_total_ms[0], 700);

    world
        .queue_cast(player_id(1), 1)
        .expect("second cast queue");
    let blocked_events = world.tick(COMBAT_FRAME_MS);
    assert!(!blocked_events.iter().any(|event| matches!(
        event,
        SimulationEvent::EffectSpawned { effect }
            if effect.owner == player_id(1) && effect.slot == 1
    )));

    for _ in 0..7 {
        let _ = world.tick(COMBAT_FRAME_MS);
    }
    let cooled_down = world.player_state(player_id(1)).expect("alice");
    assert_eq!(cooled_down.slot_cooldown_remaining_ms[0], 0);
}

#[test]
fn skills_consume_mana_and_regenerate_over_time() {
    let content = content();
    let mut world = world(
        &content,
        vec![seed(
            &content,
            1,
            "Alice",
            TeamSide::TeamA,
            SkillTree::Mage,
            [Some(choice(SkillTree::Mage, 1)), None, None, None, None],
        )],
    );
    {
        let alice = world.players.get_mut(&player_id(1)).expect("alice");
        alice.mana = 20;
        alice.x = -500;
        alice.y = 0;
        alice.aim_x = 100;
        alice.aim_y = 0;
    }

    world.queue_cast(player_id(1), 1).expect("cast");
    let cast_events = world.tick(COMBAT_FRAME_MS);
    assert!(cast_events.iter().any(|event| matches!(
        event,
        SimulationEvent::EffectSpawned { effect }
            if effect.owner == player_id(1) && effect.slot == 1
    )));
    let after_cast = world.player_state(player_id(1)).expect("alice");
    assert_eq!(after_cast.mana, 5);

    for _ in 0..20 {
        let _ = world.tick(COMBAT_FRAME_MS);
    }
    let regenerated = world.player_state(player_id(1)).expect("alice");
    assert!(regenerated.mana > after_cast.mana);
}

#[test]
fn casts_do_not_fire_when_mana_is_below_cost() {
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
                [Some(choice(SkillTree::Mage, 1)), None, None, None, None],
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
        alice.mana = 10;
        alice.x = -500;
        alice.y = 0;
        alice.aim_x = 100;
        alice.aim_y = 0;
        let bob = world.players.get_mut(&player_id(2)).expect("bob");
        bob.x = -250;
        bob.y = 0;
    }

    world.queue_cast(player_id(1), 1).expect("cast");
    let blocked_events = world.tick(COMBAT_FRAME_MS);
    assert!(!blocked_events.iter().any(|event| matches!(
        event,
        SimulationEvent::EffectSpawned { effect }
            if effect.owner == player_id(1) && effect.slot == 1
    )));
    let alice = world.player_state(player_id(1)).expect("alice");
    assert_eq!(alice.mana, 11);
    assert_eq!(alice.slot_cooldown_remaining_ms[0], 0);
}

#[test]
fn mana_regen_and_exact_skill_cost_boundaries_are_precise() {
    let content = content();
    let mut world = world(
        &content,
        vec![seed(
            &content,
            1,
            "Alice",
            TeamSide::TeamA,
            SkillTree::Mage,
            [None, None, None, None, None],
        )],
    );

    let max_mana = world.players.get(&player_id(1)).expect("alice").max_mana;
    {
        let alice = world.players.get_mut(&player_id(1)).expect("alice");
        alice.mana = max_mana.saturating_sub(3);
        alice.mana_regen_progress = 950;
    }
    world.advance_mana(100);
    let alice = world.players.get(&player_id(1)).expect("alice");
    assert_eq!(alice.mana, max_mana - 1);
    assert_eq!(alice.mana_regen_progress, 150);

    {
        let alice = world.players.get_mut(&player_id(1)).expect("alice");
        alice.mana = alice.max_mana;
        alice.mana_regen_progress = 777;
    }
    world.advance_mana(100);
    assert_eq!(
        world
            .players
            .get(&player_id(1))
            .expect("alice")
            .mana_regen_progress,
        0,
        "fully regenerated players should not accumulate stale regen remainder"
    );

    {
        let alice = world.players.get_mut(&player_id(1)).expect("alice");
        alice.mana = 12;
    }
    assert!(
        world.consume_skill_mana(player_id(1), 12),
        "exact mana should be enough to pay a skill cost"
    );
    assert_eq!(world.players.get(&player_id(1)).expect("alice").mana, 0);
    assert!(
        !world.consume_skill_mana(player_id(1), 1),
        "spending more mana than remains should fail"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn passive_skills_reduce_cast_times_cooldowns_and_improve_move_speed() {
    let content = content();
    let warrior_tree = SkillTree::Warrior;
    let baseline_skill = content
        .skills()
        .resolve(&choice(warrior_tree.clone(), 3))
        .expect("warrior cast skill")
        .clone();
    let _passive_skill = content
        .skills()
        .resolve(&choice(warrior_tree.clone(), 5))
        .expect("warrior passive skill")
        .clone();

    let mut baseline_world = world(
        &content,
        vec![seed_with_slot_one_skill(
            &content,
            1,
            "Baseline",
            TeamSide::TeamA,
            warrior_tree.clone(),
            &baseline_skill,
        )],
    );
    let mut passive_world = world(
        &content,
        vec![seed(
            &content,
            1,
            "Passive",
            TeamSide::TeamA,
            warrior_tree.clone(),
            [Some(choice(warrior_tree.clone(), 3)), Some(choice(warrior_tree.clone(), 5)), None, None, None],
        )],
    );

    for world in [&mut baseline_world, &mut passive_world] {
        set_player_pose(
            world,
            player_id(1),
            TEST_ATTACKER_X,
            TEST_OPEN_LANE_Y,
            TEST_AIM_X,
            TEST_AIM_Y,
        );
    }

    baseline_world.queue_cast(player_id(1), 1).expect("baseline cast");
    passive_world.queue_cast(player_id(1), 1).expect("passive cast");
    let _ = baseline_world.tick(COMBAT_FRAME_MS);
    let _ = passive_world.tick(COMBAT_FRAME_MS);

    let baseline_cast = baseline_world.player_state(player_id(1)).expect("baseline");
    let passive_cast = passive_world.player_state(player_id(1)).expect("passive");
    assert!(
        passive_cast.current_cast_total_ms < baseline_cast.current_cast_total_ms,
        "cast-time passives should reduce the exported total cast time"
    );

    let _ = collect_ticks(&mut baseline_world, 4);
    let _ = collect_ticks(&mut passive_world, 4);
    let baseline_after = baseline_world.player_state(player_id(1)).expect("baseline");
    let passive_after = passive_world.player_state(player_id(1)).expect("passive");
    assert!(
        passive_after.slot_cooldown_total_ms[0] < baseline_after.slot_cooldown_total_ms[0],
        "cooldown passives should reduce the exported cooldown total"
    );
    assert!(
        passive_after.slot_cooldown_remaining_ms[0] < baseline_after.slot_cooldown_remaining_ms[0],
        "cooldown passives should reduce the live cooldown state too"
    );

    let mut baseline_move_world = world(
        &content,
        vec![seed_with_slot_one_skill(
            &content,
            1,
            "Baseline",
            TeamSide::TeamA,
            warrior_tree.clone(),
            &baseline_skill,
        )],
    );
    let mut passive_move_world = world(
        &content,
        vec![seed(
            &content,
            1,
            "Passive",
            TeamSide::TeamA,
            warrior_tree.clone(),
            [
                Some(choice(warrior_tree.clone(), 3)),
                Some(choice(warrior_tree.clone(), 5)),
                None,
                None,
                None,
            ],
        )],
    );
    for world in [&mut baseline_move_world, &mut passive_move_world] {
        set_player_pose(
            world,
            player_id(1),
            TEST_ATTACKER_X,
            TEST_OPEN_LANE_Y,
            TEST_AIM_X,
            TEST_AIM_Y,
        );
    }

    baseline_move_world
        .submit_input(player_id(1), MovementIntent::new(1, 0).expect("intent"))
        .expect("movement");
    passive_move_world
        .submit_input(player_id(1), MovementIntent::new(1, 0).expect("intent"))
        .expect("movement");
    let _ = collect_ticks(&mut baseline_move_world, 5);
    let _ = collect_ticks(&mut passive_move_world, 5);
    let baseline_move = baseline_move_world
        .player_state(player_id(1))
        .expect("baseline");
    let passive_move = passive_move_world
        .player_state(player_id(1))
        .expect("passive");
    assert!(
        passive_move.x - TEST_ATTACKER_X > baseline_move.x - TEST_ATTACKER_X,
        "speed passives should increase distance moved per frame"
    );
}

#[test]
fn passive_skills_increase_projectile_speed() {
    let content = content();
    let mage_tree = SkillTree::Mage;
    let projectile_skill = content
        .skills()
        .resolve(&choice(mage_tree.clone(), 1))
        .expect("mage projectile")
        .clone();

    let mut baseline_world = world(
        &content,
        vec![seed_with_slot_one_skill(
            &content,
            1,
            "Baseline",
            TeamSide::TeamA,
            mage_tree.clone(),
            &projectile_skill,
        )],
    );
    let mut passive_world = world(
        &content,
        vec![seed(
            &content,
            1,
            "Passive",
            TeamSide::TeamA,
            mage_tree.clone(),
            [Some(choice(mage_tree.clone(), 1)), None, None, None, Some(choice(mage_tree.clone(), 5))],
        )],
    );

    for world in [&mut baseline_world, &mut passive_world] {
        set_player_pose(
            world,
            player_id(1),
            TEST_ATTACKER_X,
            TEST_OPEN_LANE_Y,
            TEST_AIM_X,
            TEST_AIM_Y,
        );
    }

    baseline_world.queue_cast(player_id(1), 1).expect("baseline cast");
    passive_world.queue_cast(player_id(1), 1).expect("passive cast");
    let _ = baseline_world.tick(COMBAT_FRAME_MS);
    let _ = passive_world.tick(COMBAT_FRAME_MS);

    assert_eq!(baseline_world.projectiles.len(), 1);
    assert_eq!(passive_world.projectiles.len(), 1);
    assert!(
        passive_world.projectiles[0].x > baseline_world.projectiles[0].x,
        "projectile-speed passives should advance projectiles farther in the same frame"
    );
}
