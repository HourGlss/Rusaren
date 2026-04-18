use super::*;
use game_content::{CombatValueKind, EffectPayload, SkillEffectKind, StatusDefinition};

fn poison_impact_frames(speed: u16, distance_units: u16) -> usize {
    let travel_per_frame = usize::from(travel_distance_units(speed, COMBAT_FRAME_MS).max(1));
    usize::from(distance_units).div_ceil(travel_per_frame) + 1
}

#[test]
#[allow(clippy::too_many_lines)]
fn poison_and_hot_tick_with_expected_stacking_behavior() {
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
                [None, None, None, None, None],
            ),
        ],
    );
    {
        let alice = poison_world.players.get_mut(&player_id(1)).expect("alice");
        alice.x = -400;
        alice.y = 0;
        alice.aim_x = 100;
        alice.aim_y = 0;
        let bob = poison_world.players.get_mut(&player_id(2)).expect("bob");
        bob.x = -240;
        bob.y = 0;
    }

    let poison_skill = poison_world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[0].clone())
        .expect("poison should be equipped");
    let SkillBehavior::Projectile {
        speed: poison_speed,
        ..
    } = poison_skill.behavior
    else {
        panic!("rogue tier one should remain a projectile");
    };
    let _ = activate_skill_cast(&mut poison_world, player_id(1), 1, &poison_skill.behavior);
    let _ = collect_ticks(&mut poison_world, poison_impact_frames(poison_speed, 160));
    let poison_statuses = poison_world.statuses_for(player_id(2)).expect("statuses");
    assert!(poison_statuses
        .iter()
        .any(|status| status.kind == StatusKind::Poison));
    let damaged_hit_points = poison_world
        .player_state(player_id(2))
        .expect("bob")
        .hit_points;
    assert!(damaged_hit_points < class_hit_points(&content, &SkillTree::Warrior));

    let druid_tree = SkillTree::new("Druid").expect("druid tree");
    let mut hot_world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Briar",
                TeamSide::TeamA,
                druid_tree.clone(),
                [None, None, Some(choice(druid_tree, 3)), None, None],
            ),
            seed(
                &content,
                2,
                "Ally",
                TeamSide::TeamA,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );
    {
        let briar = hot_world.players.get_mut(&player_id(1)).expect("briar");
        briar.x = -420;
        briar.y = 0;
        briar.aim_x = 100;
        briar.aim_y = 0;
        let ally = hot_world.players.get_mut(&player_id(2)).expect("ally");
        ally.x = -300;
        ally.y = 0;
        ally.hit_points = 70;
    }

    let hot_skill = hot_world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[2].clone())
        .expect("hot should be equipped");
    let _ = resolve_skill_cast(&mut hot_world, player_id(1), 3, hot_skill.behavior);
    let _ = collect_ticks(&mut hot_world, 10);
    let hot_statuses = hot_world.statuses_for(player_id(2)).expect("statuses");
    assert!(hot_statuses
        .iter()
        .any(|status| status.kind == StatusKind::Hot));
    let ally = hot_world.player_state(player_id(2)).expect("ally");
    assert!(ally.hit_points > 70);
}

#[test]
#[allow(clippy::too_many_lines)]
fn poison_and_hot_stack_from_the_same_source() {
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

    let poison_skill = poison_world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[0].clone())
        .expect("poison should be equipped");
    let SkillBehavior::Projectile {
        speed: poison_speed,
        ..
    } = poison_skill.behavior
    else {
        panic!("rogue tier one should remain a projectile");
    };
    for _ in 0..2 {
        let _ = activate_skill_cast(&mut poison_world, player_id(1), 1, &poison_skill.behavior);
        let _ = collect_ticks(&mut poison_world, poison_impact_frames(poison_speed, 160));
    }

    let poison_statuses = poison_world.statuses_for(player_id(2)).expect("statuses");
    let poison = poison_statuses
        .iter()
        .find(|status| status.kind == StatusKind::Poison)
        .expect("poison should exist");
    assert_eq!(poison.stacks, 2);

    let druid_tree = SkillTree::new("Druid").expect("druid tree");
    let mut hot_world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Druid",
                TeamSide::TeamA,
                druid_tree.clone(),
                [None, None, Some(choice(druid_tree, 3)), None, None],
            ),
            seed(
                &content,
                2,
                "Ally",
                TeamSide::TeamA,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );
    {
        let druid = hot_world.players.get_mut(&player_id(1)).expect("druid");
        druid.x = -420;
        druid.y = 0;
        druid.aim_x = 100;
        druid.aim_y = 0;
        let ally = hot_world.players.get_mut(&player_id(2)).expect("ally");
        ally.x = -300;
        ally.y = 0;
        ally.hit_points = 80;
    }

    let hot_skill = hot_world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[2].clone())
        .expect("hot should be equipped");
    let _ = resolve_skill_cast(&mut hot_world, player_id(1), 3, hot_skill.behavior.clone());
    let _ = collect_ticks(&mut hot_world, 18);
    let hot_before_refresh = hot_world
        .statuses_for(player_id(2))
        .expect("statuses")
        .into_iter()
        .find(|status| status.kind == StatusKind::Hot)
        .expect("hot should exist before refresh");

    for _ in 0..4 {
        let _ = hot_world.tick(COMBAT_FRAME_MS);
    }
    let _ = resolve_skill_cast(&mut hot_world, player_id(1), 3, hot_skill.behavior.clone());
    let hot_after_refresh = hot_world
        .statuses_for(player_id(2))
        .expect("statuses")
        .into_iter()
        .find(|status| status.kind == StatusKind::Hot)
        .expect("hot should exist after refresh");
    assert_eq!(hot_after_refresh.stacks, 2);
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

    let druid_tree = SkillTree::new("Druid").expect("druid tree");
    let mut hot_world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Druid",
                TeamSide::TeamA,
                druid_tree.clone(),
                [None, None, Some(choice(druid_tree, 3)), None, None],
            ),
            seed(
                &content,
                2,
                "Ally",
                TeamSide::TeamA,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );
    {
        let druid = hot_world.players.get_mut(&player_id(1)).expect("druid");
        druid.x = -420;
        druid.y = 0;
        druid.aim_x = 100;
        druid.aim_y = 0;
        let ally = hot_world.players.get_mut(&player_id(2)).expect("ally");
        ally.x = -300;
        ally.y = 0;
        ally.hit_points = 70;
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
            .statuses_for(player_id(2))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Hot),
        "hot should expire after its duration"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn multi_source_hot_and_chill_coexist_without_collapsing_into_one_shared_stack() {
    let content = content();
    let druid_tree = SkillTree::new("Druid").expect("druid tree");
    let mut hot_world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "DruidOne",
                TeamSide::TeamA,
                druid_tree.clone(),
                [None, None, Some(choice(druid_tree.clone(), 3)), None, None],
            ),
            seed(
                &content,
                2,
                "DruidTwo",
                TeamSide::TeamA,
                druid_tree.clone(),
                [None, None, Some(choice(druid_tree, 3)), None, None],
            ),
            seed(
                &content,
                3,
                "Ally",
                TeamSide::TeamA,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );
    set_player_pose(&mut hot_world, player_id(1), -500, -60, 200, 60);
    set_player_pose(&mut hot_world, player_id(2), -500, 60, 200, -60);
    set_player_pose(
        &mut hot_world,
        player_id(3),
        -300,
        0,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    hot_world
        .players
        .get_mut(&player_id(3))
        .expect("ally")
        .hit_points = 70;

    let hot_one = hot_world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[2].clone())
        .expect("first hot");
    let hot_two = hot_world
        .players
        .get(&player_id(2))
        .and_then(|player| player.skills[2].clone())
        .expect("second hot");
    let _ = resolve_skill_cast(&mut hot_world, player_id(1), 3, hot_one.behavior);
    let _ = resolve_skill_cast(&mut hot_world, player_id(2), 3, hot_two.behavior);

    let hot_statuses = hot_world
        .statuses_for(player_id(3))
        .expect("statuses")
        .into_iter()
        .filter(|status| status.kind == StatusKind::Hot)
        .collect::<Vec<_>>();
    assert_eq!(hot_statuses.len(), 2);
    assert_ne!(hot_statuses[0].source, hot_statuses[1].source);
    assert!(hot_statuses.iter().all(|status| status.stacks == 1));

    let mut chill_world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "MageOne",
                TeamSide::TeamA,
                SkillTree::Mage,
                [None, None, Some(choice(SkillTree::Mage, 3)), None, None],
            ),
            seed(
                &content,
                2,
                "MageTwo",
                TeamSide::TeamA,
                SkillTree::Mage,
                [None, None, Some(choice(SkillTree::Mage, 3)), None, None],
            ),
            seed(
                &content,
                3,
                "Enemy",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );
    set_player_pose(&mut chill_world, player_id(1), -340, -40, 120, 40);
    set_player_pose(&mut chill_world, player_id(2), -340, 40, 120, -40);
    set_player_pose(
        &mut chill_world,
        player_id(3),
        -80,
        0,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    let chill_one = chill_world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[2].clone())
        .expect("first chill");
    let chill_two = chill_world
        .players
        .get(&player_id(2))
        .and_then(|player| player.skills[2].clone())
        .expect("second chill");
    let _ = resolve_skill_cast(&mut chill_world, player_id(1), 3, chill_one.behavior);
    let _ = resolve_skill_cast(&mut chill_world, player_id(2), 3, chill_two.behavior);

    let chill_statuses = chill_world
        .statuses_for(player_id(3))
        .expect("statuses")
        .into_iter()
        .filter(|status| status.kind == StatusKind::Chill)
        .collect::<Vec<_>>();
    assert_eq!(chill_statuses.len(), 2);
    assert_ne!(chill_statuses[0].source, chill_statuses[1].source);
    assert!(
        chill_world
            .statuses_for(player_id(3))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Root),
        "separate chill sources should coexist without prematurely forcing the shared root trigger"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn dispel_removes_negative_effects_and_lifebloom_blooms_on_dispel_or_expire() {
    let content = content();
    let druid_tree = SkillTree::new("Druid").expect("druid tree");
    let mut cleanse_world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Rogue",
                TeamSide::TeamB,
                SkillTree::Rogue,
                content
                    .skills()
                    .resolve(&choice(SkillTree::Rogue, 1))
                    .expect("rogue poison"),
            ),
            seed_with_slot_one_skill(
                &content,
                2,
                "Cleric",
                TeamSide::TeamA,
                SkillTree::Cleric,
                content
                    .skills()
                    .resolve(&choice(SkillTree::Cleric, 4))
                    .expect("cleric cleanse"),
            ),
            seed(
                &content,
                3,
                "Ally",
                TeamSide::TeamA,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut cleanse_world,
        player_id(1),
        -500,
        0,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(&mut cleanse_world, player_id(2), -420, 80, 120, -80);
    set_player_pose(
        &mut cleanse_world,
        player_id(3),
        -300,
        0,
        TEST_AIM_X,
        TEST_AIM_Y,
    );

    let poison_skill = cleanse_world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[0].clone())
        .expect("poison");
    let SkillBehavior::Projectile {
        speed: poison_speed,
        ..
    } = poison_skill.behavior
    else {
        panic!("rogue poison should remain a projectile");
    };
    let _ = activate_skill_cast(&mut cleanse_world, player_id(1), 1, &poison_skill.behavior);
    let _ = collect_ticks(&mut cleanse_world, poison_impact_frames(poison_speed, 200));
    assert!(
        cleanse_world
            .statuses_for(player_id(3))
            .expect("statuses")
            .iter()
            .any(|status| status.kind == StatusKind::Poison),
        "the ally should be poisoned before the cleanse lands"
    );

    set_player_pose(&mut cleanse_world, player_id(2), -420, 80, 120, -80);
    let cleanse_skill = cleanse_world
        .players
        .get(&player_id(2))
        .and_then(|player| player.skills[0].clone())
        .expect("cleanse");
    let cleanse_events =
        resolve_skill_cast(&mut cleanse_world, player_id(2), 1, cleanse_skill.behavior);
    assert!(
        healing_to(&cleanse_events, player_id(3)).is_some(),
        "cleanse should still count as a heal"
    );
    assert!(
        cleanse_world
            .statuses_for(player_id(3))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Poison),
        "the negative dispel should remove poison from the ally"
    );

    let mut lifebloom_world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Druid",
                TeamSide::TeamA,
                druid_tree.clone(),
                [None, None, Some(choice(druid_tree.clone(), 3)), None, None],
            ),
            seed(
                &content,
                2,
                "Ally",
                TeamSide::TeamA,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut lifebloom_world,
        player_id(1),
        -420,
        0,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut lifebloom_world,
        player_id(2),
        -300,
        0,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    lifebloom_world
        .players
        .get_mut(&player_id(2))
        .expect("ally")
        .hit_points = 60;

    let lifebloom_skill = lifebloom_world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[2].clone())
        .expect("lifebloom");
    let _ = resolve_skill_cast(
        &mut lifebloom_world,
        player_id(1),
        3,
        lifebloom_skill.behavior.clone(),
    );
    let dispel_events = lifebloom_world.apply_payload(
        player_id(1),
        4,
        &[TargetEntity::Player(player_id(2))],
        game_content::EffectPayload {
            kind: CombatValueKind::Heal,
            amount: 0,
            amount_max: None,
            crit_chance_bps: 0,
            crit_multiplier_bps: 0,
            status: None,
            interrupt_silence_duration_ms: None,
            dispel: Some(game_content::DispelDefinition {
                scope: game_content::DispelScope::Positive,
                max_statuses: 1,
            }),
        },
    );
    assert!(
        dispel_events.iter().any(|event| matches!(
            event,
            SimulationEvent::HealingApplied { target, amount, .. }
                if *target == player_id(2) && *amount == 12
        )),
        "dispel-triggered blooms should emit their authored heal payload"
    );
    assert!(
        lifebloom_world
            .statuses_for(player_id(2))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Hot),
        "the positive dispel should remove the hot it just bloomed"
    );

    let mut expire_world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Druid",
                TeamSide::TeamA,
                druid_tree,
                [
                    None,
                    None,
                    Some(choice(SkillTree::new("Druid").expect("druid tree"), 3)),
                    None,
                    None,
                ],
            ),
            seed(
                &content,
                2,
                "Ally",
                TeamSide::TeamA,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut expire_world,
        player_id(1),
        -420,
        0,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut expire_world,
        player_id(2),
        -300,
        0,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    expire_world
        .players
        .get_mut(&player_id(2))
        .expect("ally")
        .hit_points = 60;
    let expire_skill = expire_world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[2].clone())
        .expect("lifebloom");
    let _ = resolve_skill_cast(&mut expire_world, player_id(1), 3, expire_skill.behavior);
    let expire_events = collect_ticks(&mut expire_world, 40);
    assert!(
        expire_events.iter().any(|event| matches!(
            event,
            SimulationEvent::HealingApplied { target, amount, .. }
                if *target == player_id(2) && *amount == 12
        )),
        "expiration-triggered blooms should emit their authored heal payload"
    );
    assert!(
        expire_world
            .statuses_for(player_id(2))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Hot),
        "the hot should be gone after its bloom-on-expire fires"
    );
}

#[test]
fn healing_reduction_status_reduces_direct_healing_by_the_strongest_effect() {
    let content = content();
    let mut world = world(
        &content,
        vec![seed(
            &content,
            1,
            "Alice",
            TeamSide::TeamA,
            SkillTree::Cleric,
            [None, None, None, None, None],
        )],
    );
    let max_hit_points = class_hit_points(&content, &SkillTree::Cleric);
    world
        .players
        .get_mut(&player_id(1))
        .expect("alice")
        .hit_points = max_hit_points - 30;

    let weaker = world.apply_status(
        player_id(2),
        player_id(1),
        1,
        StatusDefinition {
            kind: StatusKind::HealingReduction,
            duration_ms: 3000,
            tick_interval_ms: None,
            magnitude: 2500,
            max_stacks: 1,
            trigger_duration_ms: None,
            expire_payload: None,
            dispel_payload: None,
        },
    );
    assert!(weaker.is_some(), "weaker healing reduction should apply");
    let stronger = world.apply_status(
        player_id(3),
        player_id(1),
        1,
        StatusDefinition {
            kind: StatusKind::HealingReduction,
            duration_ms: 3000,
            tick_interval_ms: None,
            magnitude: 5000,
            max_stacks: 1,
            trigger_duration_ms: None,
            expire_payload: None,
            dispel_payload: None,
        },
    );
    assert!(
        stronger.is_some(),
        "stronger healing reduction should apply"
    );

    let events = world.apply_payload(
        player_id(1),
        1,
        &[TargetEntity::Player(player_id(1))],
        EffectPayload {
            kind: CombatValueKind::Heal,
            amount: 20,
            amount_max: None,
            crit_chance_bps: 0,
            crit_multiplier_bps: 0,
            status: None,
            interrupt_silence_duration_ms: None,
            dispel: None,
        },
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            SimulationEvent::HealingApplied {
                target,
                amount,
                critical,
                ..
            } if *target == player_id(1) && *amount == 10 && !*critical
        )
    }));
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
                [
                    None,
                    None,
                    None,
                    Some(choice(SkillTree::new("Bard").expect("bard tree"), 4)),
                    None,
                ],
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
    let haste_duration_ms =
        remaining_status_ms(&world, player_id(2), StatusKind::Haste).expect("haste");

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
    let _ = collect_ticks(&mut world, status_expiration_frames(haste_duration_ms));
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
    let silence_duration_ms =
        remaining_status_ms(&world, player_id(2), StatusKind::Silence).expect("silence");

    let _ = collect_ticks(&mut world, status_expiration_frames(silence_duration_ms));
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
    let stun_duration_ms =
        remaining_status_ms(&world, player_id(2), StatusKind::Stun).expect("stun");

    let _ = collect_ticks(&mut world, status_expiration_frames(stun_duration_ms));
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

    let chill_expiration_frames = world
        .statuses_for(player_id(2))
        .expect("statuses")
        .iter()
        .filter(|status| matches!(status.kind, StatusKind::Chill | StatusKind::Root))
        .map(|status| status_expiration_frames(status.remaining_ms))
        .max()
        .unwrap_or(0);
    let _ = collect_ticks(&mut world, chill_expiration_frames);
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
    let first_cast = resolve_skill_cast(&mut world, player_id(1), 1, shield_skill.behavior.clone());
    assert!(status_applied_to(&first_cast, player_id(1), StatusKind::Shield).is_some());
    let _ = collect_ticks(&mut world, 20);
    let second_cast =
        resolve_skill_cast(&mut world, player_id(1), 1, shield_skill.behavior.clone());
    assert_eq!(
        status_applied_to(&second_cast, player_id(1), StatusKind::Shield),
        Some(2)
    );

    let beam_skill = world
        .players
        .get(&player_id(2))
        .and_then(|player| player.skills[0].clone())
        .expect("warrior beam");
    let first_hit = resolve_skill_cast(&mut world, player_id(2), 1, beam_skill.behavior.clone());
    assert_eq!(
        world.player_state(player_id(1)).expect("cleric").hit_points,
        class_hit_points(&content, &SkillTree::Cleric),
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
    let second_hit = resolve_skill_cast(&mut world, player_id(2), 1, beam_skill.behavior.clone());
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
    let cast_events = resolve_skill_cast(&mut world, player_id(1), 1, stealth_skill.behavior);
    assert!(
        player_has_status(&world, player_id(1), StatusKind::Stealth),
        "the rogue should enter stealth immediately when Nightcloak starts"
    );
    assert!(
        !effect_spawned_by(&cast_events, player_id(1), 1),
        "Nightcloak should not advertise stealth with a visible aura pulse"
    );
    let pulse_events = collect_ticks(&mut world, 10);
    assert!(
        !effect_spawned_by(&pulse_events, player_id(1), 1),
        "Nightcloak refreshes should stay visually hidden while stealth is active"
    );

    let projectile_skill = world
        .players
        .get(&player_id(2))
        .and_then(|player| player.skills[0].clone())
        .expect("projectile skill");
    let projectile_events =
        resolve_skill_cast(&mut world, player_id(2), 1, projectile_skill.behavior);
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
        !player_has_status(&world, player_id(1), StatusKind::Stealth),
        "taking an action should immediately break stealth"
    );
    let _ = collect_ticks(&mut world, 20);
    assert!(
        !player_has_status(&world, player_id(1), StatusKind::Stealth),
        "breaking stealth should also cancel the toggle aura so it does not silently reapply"
    );
}

#[test]
fn nightcloak_recast_toggles_stealth_off_without_spending_more_resources() {
    let content = content();
    let rogue_skill = content
        .skills()
        .resolve(&choice(SkillTree::Rogue, 4))
        .expect("rogue stealth skill")
        .clone();
    let mut world = world(
        &content,
        vec![seed_with_slot_one_skill(
            &content,
            1,
            "Rogue",
            TeamSide::TeamA,
            SkillTree::Rogue,
            &rogue_skill,
        )],
    );

    let _ = resolve_skill_cast(&mut world, player_id(1), 1, rogue_skill.behavior.clone());
    let after_first_cast = world.player_state(player_id(1)).expect("rogue state");
    let cooldown_after_first_cast = after_first_cast.slot_cooldown_remaining_ms[0];
    assert!(
        world
            .statuses_for(player_id(1))
            .expect("statuses")
            .iter()
            .any(|status| status.kind == StatusKind::Stealth),
        "the rogue should be stealthed after the first toggle"
    );

    {
        let rogue = world.players.get_mut(&player_id(1)).expect("rogue");
        rogue.mana = 0;
        rogue.mana_regen_progress = 0;
    }

    let _ = activate_skill_cast(&mut world, player_id(1), 1, &rogue_skill.behavior);
    let after_second_cast = world.player_state(player_id(1)).expect("rogue state");
    assert!(
        world
            .statuses_for(player_id(1))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Stealth),
        "recasting Nightcloak should toggle the stealth aura off"
    );
    assert_eq!(
        after_second_cast.mana,
        1,
        "toggling Nightcloak off should bypass mana spending and only reflect the next tick of natural regen"
    );
    assert_eq!(
        after_second_cast.slot_cooldown_remaining_ms[0],
        cooldown_after_first_cast.saturating_sub(COMBAT_FRAME_MS),
        "toggling Nightcloak off should let the existing cooldown keep ticking instead of restarting it"
    );
}

#[test]
fn aura_cast_end_payload_applies_when_a_toggle_aura_is_canceled() {
    let content = content();
    let mut stealth_skill = content
        .skills()
        .resolve(&choice(SkillTree::Rogue, 4))
        .expect("rogue stealth skill")
        .clone();
    stealth_skill.behavior = SkillBehavior::Aura {
        cooldown_ms: 2400,
        cast_time_ms: 0,
        mana_cost: 10,
        distance: 0,
        radius: 24,
        duration_ms: 30000,
        hit_points: None,
        toggleable: true,
        tick_interval_ms: 1000,
        cast_start_payload: Some(EffectPayload {
            kind: CombatValueKind::Heal,
            amount: 0,
            amount_max: None,
            crit_chance_bps: 0,
            crit_multiplier_bps: 0,
            status: Some(StatusDefinition {
                kind: StatusKind::Stealth,
                duration_ms: 1200,
                tick_interval_ms: None,
                magnitude: 0,
                max_stacks: 1,
                trigger_duration_ms: None,
                expire_payload: None,
                dispel_payload: None,
            }),
            interrupt_silence_duration_ms: None,
            dispel: None,
        }),
        cast_end_payload: Some(EffectPayload {
            kind: CombatValueKind::Heal,
            amount: 0,
            amount_max: None,
            crit_chance_bps: 0,
            crit_multiplier_bps: 0,
            status: Some(StatusDefinition {
                kind: StatusKind::Haste,
                duration_ms: 1500,
                tick_interval_ms: None,
                magnitude: 1200,
                max_stacks: 1,
                trigger_duration_ms: None,
                expire_payload: None,
                dispel_payload: None,
            }),
            interrupt_silence_duration_ms: None,
            dispel: None,
        }),
        effect: SkillEffectKind::Nova,
        payload: EffectPayload {
            kind: CombatValueKind::Heal,
            amount: 0,
            amount_max: None,
            crit_chance_bps: 0,
            crit_multiplier_bps: 0,
            status: Some(StatusDefinition {
                kind: StatusKind::Stealth,
                duration_ms: 1200,
                tick_interval_ms: None,
                magnitude: 0,
                max_stacks: 1,
                trigger_duration_ms: None,
                expire_payload: None,
                dispel_payload: None,
            }),
            interrupt_silence_duration_ms: None,
            dispel: None,
        },
    };
    let mut world = world(
        &content,
        vec![seed_with_slot_one_skill(
            &content,
            1,
            "Rogue",
            TeamSide::TeamA,
            SkillTree::Rogue,
            &stealth_skill,
        )],
    );

    let _ = resolve_skill_cast(&mut world, player_id(1), 1, stealth_skill.behavior.clone());
    assert!(
        world
            .statuses_for(player_id(1))
            .expect("statuses")
            .iter()
            .any(|status| status.kind == StatusKind::Stealth),
        "the toggle aura should apply its cast-start stealth payload"
    );

    let toggle_off_events = resolve_skill_cast(&mut world, player_id(1), 1, stealth_skill.behavior);
    assert!(
        status_applied_to(&toggle_off_events, player_id(1), StatusKind::Haste).is_some(),
        "canceling a toggle aura should apply its cast-end payload"
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

fn authored_status_definition(
    content: &GameContent,
    tree: SkillTree,
    tier: u8,
) -> game_content::StatusDefinition {
    let skill = content
        .skills()
        .resolve(&choice(tree, tier))
        .expect("authored skill should exist");
    behavior_payload(&skill.behavior)
        .and_then(|payload| payload.status)
        .expect("authored skill should carry a status payload")
}

#[test]
fn crowd_control_diminishing_returns_scale_hard_cc_and_reset_after_the_window() {
    let content = content();
    let stun = authored_status_definition(
        &content,
        SkillTree::new("Paladin").expect("paladin tree"),
        2,
    );
    assert_eq!(stun.kind, StatusKind::Stun);
    let stun_duration_ms = stun.duration_ms;

    let mut world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Paladin",
                TeamSide::TeamA,
                SkillTree::new("Paladin").expect("paladin tree"),
                [None, None, None, None, None],
            ),
            seed(
                &content,
                2,
                "Target",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );

    for expected_remaining_ms in
        [0_usize, 1, 2].map(|stage| dr_scaled_duration_ms(&content, stun.duration_ms, stage))
    {
        assert!(
            world
                .apply_status(player_id(1), player_id(2), 2, stun.clone())
                .is_some(),
            "the first three hard crowd-control applications should still land"
        );
        assert_eq!(
            remaining_status_ms(&world, player_id(2), StatusKind::Stun),
            Some(expected_remaining_ms)
        );
        world
            .players
            .get_mut(&player_id(2))
            .expect("target")
            .statuses
            .clear();
    }

    assert!(
        world
            .apply_status(player_id(1), player_id(2), 2, stun.clone())
            .is_none(),
        "the fourth hard crowd-control application inside the DR window should be immune"
    );
    assert_eq!(
        remaining_status_ms(&world, player_id(2), StatusKind::Stun),
        None
    );

    let reset_frames = status_expiration_frames(
        content
            .configuration()
            .simulation
            .crowd_control_diminishing_returns
            .window_ms,
    );
    let _ = collect_ticks(&mut world, reset_frames);
    assert!(
        world
            .apply_status(player_id(1), player_id(2), 2, stun)
            .is_some(),
        "hard crowd-control should reset to full duration after the DR window expires"
    );
    assert_eq!(
        remaining_status_ms(&world, player_id(2), StatusKind::Stun),
        Some(dr_scaled_duration_ms(&content, stun_duration_ms, 0))
    );
}

#[test]
fn crowd_control_diminishing_returns_use_independent_buckets() {
    let content = content();
    let fear = authored_status_definition(&content, SkillTree::Warrior, 3);
    let silence =
        authored_status_definition(&content, SkillTree::new("Bard").expect("bard tree"), 3);
    let root =
        authored_status_definition(&content, SkillTree::new("Druid").expect("druid tree"), 4);
    let fear_duration_ms = fear.duration_ms;
    let silence_duration_ms = silence.duration_ms;
    let root_duration_ms = root.duration_ms;

    let mut world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Caster",
                TeamSide::TeamA,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
            seed(
                &content,
                2,
                "Target",
                TeamSide::TeamB,
                SkillTree::Mage,
                [None, None, None, None, None],
            ),
        ],
    );

    assert!(world
        .apply_status(player_id(1), player_id(2), 3, fear.clone())
        .is_some());
    world
        .players
        .get_mut(&player_id(2))
        .expect("target")
        .statuses
        .clear();
    assert!(world
        .apply_status(player_id(1), player_id(2), 3, fear)
        .is_some());
    assert_eq!(
        remaining_status_ms(&world, player_id(2), StatusKind::Fear),
        Some(dr_scaled_duration_ms(&content, fear_duration_ms, 1))
    );
    world
        .players
        .get_mut(&player_id(2))
        .expect("target")
        .statuses
        .clear();

    assert!(world
        .apply_status(player_id(1), player_id(2), 3, silence)
        .is_some());
    assert_eq!(
        remaining_status_ms(&world, player_id(2), StatusKind::Silence),
        Some(dr_scaled_duration_ms(&content, silence_duration_ms, 0)),
        "cast-control DR should not inherit the reduced hard-CC stage"
    );
    world
        .players
        .get_mut(&player_id(2))
        .expect("target")
        .statuses
        .clear();

    assert!(world
        .apply_status(player_id(1), player_id(2), 4, root)
        .is_some());
    assert_eq!(
        remaining_status_ms(&world, player_id(2), StatusKind::Root),
        Some(dr_scaled_duration_ms(&content, root_duration_ms, 0)),
        "movement-control DR should stay independent from hard-CC DR"
    );
}
