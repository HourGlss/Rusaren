use super::*;
use game_content::{ProcResetDefinition, ProcTriggerKind, SkillEffectKind};

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
    let cleric_tree = SkillTree::Cleric;
    let bard_tree = SkillTree::new("Bard").expect("bard tree");
    let mage_tree = SkillTree::Mage;
    let baseline_skill = content
        .skills()
        .resolve(&choice(cleric_tree.clone(), 1))
        .expect("cleric cast skill")
        .clone();

    let mut baseline_world = world(
        &content,
        vec![seed_with_slot_one_skill(
            &content,
            1,
            "Baseline",
            TeamSide::TeamA,
            cleric_tree.clone(),
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
            cleric_tree.clone(),
            [
                Some(choice(cleric_tree.clone(), 1)),
                Some(choice(bard_tree.clone(), 2)),
                Some(choice(mage_tree.clone(), 5)),
                None,
                None,
            ],
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

    baseline_world
        .queue_cast(player_id(1), 1)
        .expect("baseline cast");
    passive_world
        .queue_cast(player_id(1), 1)
        .expect("passive cast");
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
            cleric_tree.clone(),
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
            mage_tree.clone(),
            [
                Some(choice(SkillTree::Cleric, 1)),
                Some(choice(bard_tree, 2)),
                Some(choice(mage_tree, 5)),
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
            [
                Some(choice(mage_tree.clone(), 1)),
                None,
                None,
                None,
                Some(choice(mage_tree.clone(), 5)),
            ],
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

    baseline_world
        .queue_cast(player_id(1), 1)
        .expect("baseline cast");
    passive_world
        .queue_cast(player_id(1), 1)
        .expect("passive cast");
    let _ = baseline_world.tick(COMBAT_FRAME_MS);
    let _ = passive_world.tick(COMBAT_FRAME_MS);

    assert_eq!(baseline_world.projectiles.len(), 1);
    assert_eq!(passive_world.projectiles.len(), 1);
    assert!(
        passive_world.projectiles[0].x > baseline_world.projectiles[0].x,
        "projectile-speed passives should advance projectiles farther in the same frame"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn passive_proc_resets_targeted_skills_and_grants_free_instacasts_with_internal_cooldown() {
    let content = content();
    let rogue_tree = SkillTree::Rogue;
    let profile = content
        .class_profile(&rogue_tree)
        .expect("rogue profile should exist");
    let melee = content
        .skills()
        .melee_for(&rogue_tree)
        .expect("rogue melee should exist")
        .clone();

    let trigger_skill = SkillDefinition {
        tree: rogue_tree.clone(),
        tier: 1,
        id: String::from("proc_trigger"),
        name: String::from("Proc Trigger"),
        description: String::from("beam hit"),
        audio_cue_id: None,
        behavior: SkillBehavior::Beam {
            cooldown_ms: 100,
            cast_time_ms: 0,
            mana_cost: 0,
            range: 320,
            radius: 18,
            effect: SkillEffectKind::Beam,
            payload: EffectPayload {
                kind: CombatValueKind::Damage,
                amount: 6,
                amount_max: None,
                crit_chance_bps: 0,
                crit_multiplier_bps: 0,
                status: None,
                interrupt_silence_duration_ms: None,
                dispel: None,
            },
        },
    };
    let reset_skill = SkillDefinition {
        tree: rogue_tree.clone(),
        tier: 2,
        id: String::from("reset_target"),
        name: String::from("Reset Target"),
        description: String::from("teleport"),
        audio_cue_id: None,
        behavior: SkillBehavior::Teleport {
            cooldown_ms: 1200,
            cast_time_ms: 0,
            mana_cost: 0,
            distance: 220,
            effect: SkillEffectKind::DashTrail,
        },
    };
    let instacast_skill = SkillDefinition {
        tree: rogue_tree.clone(),
        tier: 3,
        id: String::from("instacast_target"),
        name: String::from("Instacast Target"),
        description: String::from("slow beam"),
        audio_cue_id: None,
        behavior: SkillBehavior::Beam {
            cooldown_ms: 1300,
            cast_time_ms: 400,
            mana_cost: 20,
            range: 320,
            radius: 18,
            effect: SkillEffectKind::Beam,
            payload: EffectPayload {
                kind: CombatValueKind::Damage,
                amount: 9,
                amount_max: None,
                crit_chance_bps: 0,
                crit_multiplier_bps: 0,
                status: None,
                interrupt_silence_duration_ms: None,
                dispel: None,
            },
        },
    };
    let passive_skill = SkillDefinition {
        tree: rogue_tree.clone(),
        tier: 4,
        id: String::from("proc_passive"),
        name: String::from("Proc Passive"),
        description: String::from("proc reset"),
        audio_cue_id: None,
        behavior: SkillBehavior::Passive {
            player_speed_bps: 0,
            projectile_speed_bps: 0,
            cooldown_bps: 0,
            cast_time_bps: 0,
            proc_reset: Some(ProcResetDefinition {
                trigger: ProcTriggerKind::Hit,
                source_skill_ids: vec![String::from("proc_trigger")],
                reset_skill_ids: vec![String::from("reset_target")],
                instacast_skill_ids: vec![String::from("instacast_target")],
                instacast_costs_mana: false,
                instacast_starts_cooldown: false,
                internal_cooldown_ms: Some(1000),
            }),
        },
    };

    let mut world = world(
        &content,
        vec![
            SimPlayerSeed {
                assignment: assignment(1, "Alice", TeamSide::TeamA),
                hit_points: profile.hit_points,
                max_mana: profile.max_mana,
                move_speed_units_per_second: profile.move_speed_units_per_second,
                melee,
                skills: [
                    Some(trigger_skill.clone()),
                    Some(reset_skill),
                    Some(instacast_skill.clone()),
                    Some(passive_skill),
                    None,
                ],
            },
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
        TEST_ATTACKER_X,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        TEST_ATTACKER_X + 180,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    world
        .players
        .get_mut(&player_id(1))
        .expect("alice")
        .slot_cooldown_remaining_ms[1] = 800;

    let trigger_events = resolve_skill_cast(&mut world, player_id(1), 1, trigger_skill.behavior);
    assert!(
        damage_to(&trigger_events, player_id(2)).is_some(),
        "the trigger spell should land so the passive can fire"
    );
    assert_eq!(
        world
            .player_state(player_id(1))
            .expect("alice")
            .slot_cooldown_remaining_ms[1],
        0,
        "the proc should reset the authored cooldown target"
    );

    {
        let alice = world.players.get_mut(&player_id(1)).expect("alice");
        alice.mana = 0;
        alice.mana_regen_progress = 0;
    }
    world.queue_cast(player_id(1), 3).expect("instacast queue");
    let instacast_events = world.tick(COMBAT_FRAME_MS);
    assert!(
        effect_spawned_by(&instacast_events, player_id(1), 3),
        "the proc should let the authored follow-up spell fire immediately"
    );
    let after_instacast = world.player_state(player_id(1)).expect("alice");
    assert_eq!(
        after_instacast.current_cast_slot, None,
        "the proc should skip the normal cast-time windup"
    );
    assert_eq!(
        after_instacast.mana,
        u16::try_from(
            (u32::from(COMBAT_FRAME_MS) * u32::from(world.configuration.mana_regen_per_second))
                / 1000,
        )
        .unwrap_or(u16::MAX),
        "the proc should preserve the frame's mana regeneration instead of spending mana"
    );
    assert_eq!(
        after_instacast.slot_cooldown_remaining_ms[2], 0,
        "the proc should allow the authored follow-up cast to skip cooldown consumption"
    );

    let _ = world.tick(COMBAT_FRAME_MS);
    world
        .players
        .get_mut(&player_id(1))
        .expect("alice")
        .slot_cooldown_remaining_ms[1] = 600;
    world
        .queue_cast(player_id(1), 1)
        .expect("second trigger queue");
    let second_trigger = world.tick(COMBAT_FRAME_MS);
    assert!(
        damage_to(&second_trigger, player_id(2)).is_some(),
        "the trigger spell should still land while the proc is on internal cooldown"
    );
    assert_eq!(
        world
            .player_state(player_id(1))
            .expect("alice")
            .slot_cooldown_remaining_ms[1],
        500,
        "the internal cooldown should stop the proc from resetting the authored target again"
    );

    world.players.get_mut(&player_id(1)).expect("alice").mana = 0;
    world.queue_cast(player_id(1), 3).expect("post-icd queue");
    let blocked_events = world.tick(COMBAT_FRAME_MS);
    assert!(
        !effect_spawned_by(&blocked_events, player_id(1), 3),
        "without a refreshed proc, the cast should fail on its normal mana and cast rules"
    );
}
