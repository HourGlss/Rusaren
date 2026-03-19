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
