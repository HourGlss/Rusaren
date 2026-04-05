use super::*;

#[test]
fn melee_uses_class_stats_and_respects_cooldown() {
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
                SkillTree::Warrior,
                [Some(choice(SkillTree::Warrior, 1)), None, None, None, None],
            ),
        ],
    );
    let alice = world.player_state(player_id(1)).expect("alice");
    let expected_damage = world
        .players
        .get(&player_id(1))
        .expect("alice player")
        .melee
        .payload
        .amount;
    {
        let bob = world.players.get_mut(&player_id(2)).expect("bob");
        bob.x = alice.x + 60;
        bob.y = alice.y;
    }

    world
        .queue_primary_attack(player_id(1))
        .expect("melee queue");
    let events = world.tick(COMBAT_FRAME_MS);
    assert!(events.iter().any(|event| matches!(event, SimulationEvent::DamageApplied { attacker, target, amount, .. } if *attacker == player_id(1) && *target == player_id(2) && *amount == expected_damage)));

    world
        .queue_primary_attack(player_id(1))
        .expect("cooldown queue");
    let events = world.tick(COMBAT_FRAME_MS);
    assert!(!events.iter().any(|event| matches!(event, SimulationEvent::DamageApplied { target, .. } if *target == player_id(2))));
}

#[test]
fn projectiles_hit_and_miss_based_on_geometry() {
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
        alice.x = -500;
        alice.y = 0;
        alice.aim_x = 100;
        alice.aim_y = 0;
        let bob = world.players.get_mut(&player_id(2)).expect("bob");
        bob.x = -250;
        bob.y = 0;
    }

    world.queue_cast(player_id(1), 1).expect("cast");
    let _ = world.tick(COMBAT_FRAME_MS);
    for _ in 0..10 {
        let events = world.tick(COMBAT_FRAME_MS);
        if events.iter().any(|event| matches!(event, SimulationEvent::DamageApplied { target, .. } if *target == player_id(2))) {
            return;
        }
    }
    panic!("projectile should hit bob in open space");
}

#[test]
fn projectile_overlap_with_player_body_counts_as_a_hit() {
    let content = content();
    let skill = content
        .skills()
        .resolve(&choice(SkillTree::Mage, 1))
        .expect("mage tier one should exist");
    let SkillBehavior::Projectile {
        radius,
        speed,
        range,
        ..
    } = skill.behavior
    else {
        panic!("mage tier one should remain a projectile");
    };

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
    let overlap_offset =
        i16::try_from(u32::from(radius) + u32::from(PLAYER_RADIUS_UNITS) - 2).unwrap_or(i16::MAX);
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
        TEST_ATTACKER_X + i16::try_from(range.min(220)).unwrap_or(220),
        TEST_OPEN_LANE_Y + overlap_offset,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    world.queue_cast(player_id(1), 1).expect("cast");
    let mut events = world.tick(COMBAT_FRAME_MS);
    events.extend(collect_ticks(
        &mut world,
        projectile_frame_budget(speed, range),
    ));

    assert!(
        damage_to(&events, player_id(2)).is_some(),
        "projectiles should hit once their radius overlaps the player's collision body"
    );
}

#[test]
fn healing_can_affect_enemy_players_and_caps_at_max_hp() {
    let content = content();
    let mut world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Cleric,
                [Some(choice(SkillTree::Cleric, 1)), None, None, None, None],
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
        bob.x = -80;
        bob.y = 0;
        bob.hit_points = 60;
    }

    let skill = world
        .players
        .get(&player_id(1))
        .and_then(|player| player.skills[0].clone())
        .expect("heal skill should be equipped");
    let events = resolve_skill_cast(&mut world, player_id(1), 1, skill.behavior);
    assert!(events.iter().any(|event| matches!(event, SimulationEvent::HealingApplied { target, .. } if *target == player_id(2))));
    let bob = world.player_state(player_id(2)).expect("bob");
    assert!(bob.hit_points > 60);
    assert!(bob.hit_points <= bob.max_hit_points);
}

#[test]
#[allow(clippy::too_many_lines)]
fn every_authored_melee_hits_when_targets_are_in_range_and_misses_when_not() {
    let content = content();
    let classes = [
        SkillTree::Warrior,
        SkillTree::Rogue,
        SkillTree::Mage,
        SkillTree::Cleric,
    ];

    for tree in classes {
        let melee = content
            .skills()
            .melee_for(&tree)
            .expect("melee should exist")
            .clone();

        let mut hit_world = world(
            &content,
            vec![
                seed(
                    &content,
                    1,
                    "Alice",
                    TeamSide::TeamA,
                    tree.clone(),
                    [const { None }; 5],
                ),
                seed(
                    &content,
                    2,
                    "Bob",
                    TeamSide::TeamB,
                    SkillTree::Warrior,
                    [const { None }; 5],
                ),
            ],
        );
        let target_point = project_from_aim(
            TEST_ATTACKER_X,
            TEST_OPEN_LANE_Y,
            TEST_AIM_X,
            TEST_AIM_Y,
            melee.range,
        );
        set_player_pose(
            &mut hit_world,
            player_id(1),
            TEST_ATTACKER_X,
            TEST_OPEN_LANE_Y,
            TEST_AIM_X,
            TEST_AIM_Y,
        );
        set_player_pose(
            &mut hit_world,
            player_id(2),
            target_point.0,
            target_point.1,
            -TEST_AIM_X,
            TEST_AIM_Y,
        );
        hit_world
            .queue_primary_attack(player_id(1))
            .expect("melee queue should succeed");
        let hit_events = hit_world.tick(COMBAT_FRAME_MS);
        assert!(
            effect_spawned_by(&hit_events, player_id(1), 0),
            "{} melee should spawn an effect",
            melee.id
        );
        match melee.payload.kind {
            CombatValueKind::Damage => assert_eq!(
                damage_to(&hit_events, player_id(2)),
                Some(melee.payload.amount),
                "{} melee should damage targets in range",
                melee.id
            ),
            CombatValueKind::Heal => assert!(
                healing_to(&hit_events, player_id(2)).is_some(),
                "{} melee should heal targets in range",
                melee.id
            ),
        }

        let mut miss_world = world(
            &content,
            vec![
                seed(
                    &content,
                    1,
                    "Alice",
                    TeamSide::TeamA,
                    tree.clone(),
                    [const { None }; 5],
                ),
                seed(
                    &content,
                    2,
                    "Bob",
                    TeamSide::TeamB,
                    SkillTree::Warrior,
                    [const { None }; 5],
                ),
            ],
        );
        let miss_offset = miss_offset_units(melee.radius);
        set_player_pose(
            &mut miss_world,
            player_id(1),
            TEST_ATTACKER_X,
            TEST_OPEN_LANE_Y,
            TEST_AIM_X,
            TEST_AIM_Y,
        );
        set_player_pose(
            &mut miss_world,
            player_id(2),
            target_point.0,
            target_point.1 + miss_offset,
            -TEST_AIM_X,
            TEST_AIM_Y,
        );
        miss_world
            .queue_primary_attack(player_id(1))
            .expect("melee queue should succeed");
        let miss_events = miss_world.tick(COMBAT_FRAME_MS);
        assert!(
            damage_to(&miss_events, player_id(2)).is_none()
                && healing_to(&miss_events, player_id(2)).is_none(),
            "{} melee should miss targets outside its radius",
            melee.id
        );
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn every_authored_skill_hits_with_valid_geometry_and_resources() {
    let content = content();

    for skill in content.skills().all() {
        if matches!(&skill.behavior, SkillBehavior::Passive { .. }) {
            continue;
        }
        let mut world = world(
            &content,
            vec![
                seed_with_slot_one_skill(
                    &content,
                    1,
                    "Alice",
                    TeamSide::TeamA,
                    skill.tree.clone(),
                    skill,
                ),
                seed(
                    &content,
                    2,
                    "Bob",
                    TeamSide::TeamB,
                    SkillTree::Warrior,
                    [const { None }; 5],
                ),
            ],
        );
        let payload = behavior_payload(&skill.behavior);
        let attacker_id = player_id(1);
        let target_id = player_id(2);

        set_player_pose(
            &mut world,
            attacker_id,
            TEST_ATTACKER_X,
            TEST_OPEN_LANE_Y,
            TEST_AIM_X,
            TEST_AIM_Y,
        );

        match &skill.behavior {
            SkillBehavior::Projectile { range, .. } | SkillBehavior::Beam { range, .. } => {
                let distance = i16::try_from((*range).min(240)).unwrap_or(240);
                set_player_pose(
                    &mut world,
                    target_id,
                    TEST_ATTACKER_X + distance,
                    TEST_OPEN_LANE_Y,
                    -TEST_AIM_X,
                    TEST_AIM_Y,
                );
            }
            SkillBehavior::Burst { range, .. } => {
                let center = project_from_aim(
                    TEST_ATTACKER_X,
                    TEST_OPEN_LANE_Y,
                    TEST_AIM_X,
                    TEST_AIM_Y,
                    *range,
                );
                set_player_pose(
                    &mut world,
                    target_id,
                    center.0,
                    center.1,
                    -TEST_AIM_X,
                    TEST_AIM_Y,
                );
            }
            SkillBehavior::Nova { radius, .. } => {
                let radius_offset = i16::try_from((*radius / 2).max(40)).unwrap_or(40);
                set_player_pose(
                    &mut world,
                    target_id,
                    TEST_ATTACKER_X + radius_offset,
                    TEST_OPEN_LANE_Y,
                    -TEST_AIM_X,
                    TEST_AIM_Y,
                );
            }
            SkillBehavior::Dash { distance, .. }
            | SkillBehavior::Summon { distance, .. }
            | SkillBehavior::Trap { distance, .. } => {
                let target_point = project_from_aim(
                    TEST_ATTACKER_X,
                    TEST_OPEN_LANE_Y,
                    TEST_AIM_X,
                    TEST_AIM_Y,
                    *distance,
                );
                set_player_pose(
                    &mut world,
                    target_id,
                    target_point.0,
                    target_point.1,
                    -TEST_AIM_X,
                    TEST_AIM_Y,
                );
            }
            SkillBehavior::Teleport { .. }
            | SkillBehavior::Ward { .. }
            | SkillBehavior::Barrier { .. } => {}
            SkillBehavior::Aura {
                distance,
                hit_points,
                radius,
                ..
            } => {
                if hit_points.is_some() {
                    let aura_point = project_from_aim(
                        TEST_ATTACKER_X,
                        TEST_OPEN_LANE_Y,
                        TEST_AIM_X,
                        TEST_AIM_Y,
                        *distance,
                    );
                    set_player_pose(
                        &mut world,
                        target_id,
                        aura_point.0,
                        aura_point.1,
                        -TEST_AIM_X,
                        TEST_AIM_Y,
                    );
                } else {
                    let radius_offset = i16::try_from((*radius / 2).max(40)).unwrap_or(40);
                    set_player_pose(
                        &mut world,
                        target_id,
                        TEST_ATTACKER_X + radius_offset,
                        TEST_OPEN_LANE_Y,
                        -TEST_AIM_X,
                        TEST_AIM_Y,
                    );
                }
            }
            SkillBehavior::Channel { range, radius, .. } => {
                let center = if *range == 0 {
                    (TEST_ATTACKER_X, TEST_OPEN_LANE_Y)
                } else {
                    project_from_aim(
                        TEST_ATTACKER_X,
                        TEST_OPEN_LANE_Y,
                        TEST_AIM_X,
                        TEST_AIM_Y,
                        *range,
                    )
                };
                let radius_offset = i16::try_from((*radius / 2).max(40)).unwrap_or(40);
                set_player_pose(
                    &mut world,
                    target_id,
                    center.0 + radius_offset,
                    center.1,
                    -TEST_AIM_X,
                    TEST_AIM_Y,
                );
            }
            SkillBehavior::Passive { .. } => unreachable!("passives are skipped above"),
        }

        if let Some(payload) = &payload {
            if payload.kind == CombatValueKind::Heal {
                let target = world
                    .players
                    .get_mut(&target_id)
                    .expect("target should exist");
                target.hit_points = 60;
            }
        }

        let mut events = activate_skill_cast(&mut world, attacker_id, 1, &skill.behavior);
        let after_cast = world.player_state(attacker_id).expect("attacker");
        assert_eq!(
            after_cast.mana,
            after_cast
                .max_mana
                .saturating_sub(skill.behavior.mana_cost()),
            "{} should consume the authored mana cost",
            skill.id
        );
        assert_eq!(
            after_cast.slot_cooldown_total_ms[0],
            skill.behavior.cooldown_ms(),
            "{} should expose the authored cooldown",
            skill.id
        );
        assert!(
            after_cast.slot_cooldown_remaining_ms[0] > 0,
            "{} should start cooling down after a valid cast",
            skill.id
        );

        if let SkillBehavior::Projectile { speed, range, .. } = &skill.behavior {
            events.extend(collect_ticks(
                &mut world,
                projectile_frame_budget(*speed, *range),
            ));
        }
        if let SkillBehavior::Summon {
            tick_interval_ms,
            range: summon_range,
            ..
        } = &skill.behavior
        {
            let _ = summon_range;
            events.extend(collect_ticks(
                &mut world,
                usize::from(*tick_interval_ms / COMBAT_FRAME_MS + 2),
            ));
        }
        if let SkillBehavior::Trap { .. } = &skill.behavior {
            events.extend(collect_ticks(&mut world, 3));
        }
        if let SkillBehavior::Aura {
            tick_interval_ms, ..
        } = &skill.behavior
        {
            events.extend(collect_ticks(
                &mut world,
                usize::from(*tick_interval_ms / COMBAT_FRAME_MS + 2),
            ));
        }
        if let SkillBehavior::Channel {
            tick_interval_ms, ..
        } = &skill.behavior
        {
            events.extend(collect_ticks(
                &mut world,
                usize::from(*tick_interval_ms / COMBAT_FRAME_MS + 2),
            ));
        }

        let should_spawn_visible_effect = !matches!(
            &skill.behavior,
            SkillBehavior::Aura { payload, .. }
                if payload.status.as_ref().is_some_and(|status| status.kind == StatusKind::Stealth)
        );
        if should_spawn_visible_effect {
            assert!(
                effect_spawned_by(&events, attacker_id, 1),
                "{} should spawn a visible effect",
                skill.id
            );
        }

        if matches!(
            &skill.behavior,
            SkillBehavior::Summon { .. }
                | SkillBehavior::Ward { .. }
                | SkillBehavior::Trap { .. }
                | SkillBehavior::Barrier { .. }
                | SkillBehavior::Aura { .. }
        ) {
            assert!(
                events.iter().any(|event| matches!(
                    event,
                    SimulationEvent::DeployableSpawned {
                        owner,
                        ..
                    } if *owner == attacker_id
                )),
                "{} should spawn a deployable",
                skill.id
            );
        }

        if let Some(payload) = payload {
            match payload.kind {
                CombatValueKind::Damage => assert!(
                    damage_to(&events, target_id).is_some(),
                    "{} should damage a target inside its geometry",
                    skill.id
                ),
                CombatValueKind::Heal => {
                    if payload.amount > 0 {
                        assert!(
                            healing_to(&events, target_id).is_some(),
                            "{} should heal a target inside its geometry",
                            skill.id
                        );
                    }
                }
            }

            if let Some(status) = payload.status {
                assert!(
                    status_applied_to(&events, target_id, status.kind).is_some(),
                    "{} should apply its authored status",
                    skill.id
                );
            }
        }

        if matches!(
            skill.behavior,
            SkillBehavior::Dash { .. } | SkillBehavior::Teleport { .. }
        ) {
            let moved = moved_player(&events, attacker_id)
                .unwrap_or_else(|| panic!("{} should move the caster", skill.id));
            assert_ne!(
                moved.0, TEST_ATTACKER_X,
                "{} should change x position",
                skill.id
            );
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn every_authored_skill_misses_targets_outside_its_effective_geometry() {
    let content = content();

    for skill in content.skills().all() {
        if matches!(
            skill.behavior,
            SkillBehavior::Teleport { .. }
                | SkillBehavior::Passive { .. }
                | SkillBehavior::Summon { .. }
                | SkillBehavior::Ward { .. }
                | SkillBehavior::Trap { .. }
                | SkillBehavior::Barrier { .. }
                | SkillBehavior::Aura { .. }
        ) {
            continue;
        }
        let mut world = world(
            &content,
            vec![
                seed_with_slot_one_skill(
                    &content,
                    1,
                    "Alice",
                    TeamSide::TeamA,
                    skill.tree.clone(),
                    skill,
                ),
                seed(
                    &content,
                    2,
                    "Bob",
                    TeamSide::TeamB,
                    SkillTree::Warrior,
                    [const { None }; 5],
                ),
            ],
        );
        let payload = behavior_payload(&skill.behavior);
        let attacker_id = player_id(1);
        let target_id = player_id(2);
        let starting_hit_points = match payload {
            Some(effect_payload) if effect_payload.kind == CombatValueKind::Heal => 60,
            _ => 100,
        };

        set_player_pose(
            &mut world,
            attacker_id,
            TEST_ATTACKER_X,
            TEST_OPEN_LANE_Y,
            TEST_AIM_X,
            TEST_AIM_Y,
        );
        {
            let target = world
                .players
                .get_mut(&target_id)
                .expect("target should exist");
            target.hit_points = starting_hit_points;
        }

        match &skill.behavior {
            SkillBehavior::Projectile { radius, range, .. }
            | SkillBehavior::Beam { radius, range, .. } => {
                set_player_pose(
                    &mut world,
                    target_id,
                    TEST_ATTACKER_X + i16::try_from((*range).min(240)).unwrap_or(240),
                    TEST_OPEN_LANE_Y + miss_offset_units(*radius),
                    -TEST_AIM_X,
                    TEST_AIM_Y,
                );
            }
            SkillBehavior::Burst { range, radius, .. } => {
                let center = project_from_aim(
                    TEST_ATTACKER_X,
                    TEST_OPEN_LANE_Y,
                    TEST_AIM_X,
                    TEST_AIM_Y,
                    *range,
                );
                set_player_pose(
                    &mut world,
                    target_id,
                    center.0,
                    center.1 + miss_offset_units(*radius),
                    -TEST_AIM_X,
                    TEST_AIM_Y,
                );
            }
            SkillBehavior::Nova { radius, .. } => {
                set_player_pose(
                    &mut world,
                    target_id,
                    TEST_ATTACKER_X,
                    TEST_OPEN_LANE_Y + miss_offset_units(*radius),
                    -TEST_AIM_X,
                    TEST_AIM_Y,
                );
            }
            SkillBehavior::Dash {
                distance,
                impact_radius,
                ..
            } => {
                let dash_end = project_from_aim(
                    TEST_ATTACKER_X,
                    TEST_OPEN_LANE_Y,
                    TEST_AIM_X,
                    TEST_AIM_Y,
                    *distance,
                );
                let radius = (*impact_radius).unwrap_or(PLAYER_RADIUS_UNITS);
                set_player_pose(
                    &mut world,
                    target_id,
                    dash_end.0,
                    dash_end.1 + miss_offset_units(radius),
                    -TEST_AIM_X,
                    TEST_AIM_Y,
                );
            }
            SkillBehavior::Channel { range, radius, .. } => {
                let center = if *range == 0 {
                    (TEST_ATTACKER_X, TEST_OPEN_LANE_Y)
                } else {
                    project_from_aim(
                        TEST_ATTACKER_X,
                        TEST_OPEN_LANE_Y,
                        TEST_AIM_X,
                        TEST_AIM_Y,
                        *range,
                    )
                };
                set_player_pose(
                    &mut world,
                    target_id,
                    center.0,
                    center.1 + miss_offset_units(*radius),
                    -TEST_AIM_X,
                    TEST_AIM_Y,
                );
            }
            SkillBehavior::Teleport { .. }
            | SkillBehavior::Passive { .. }
            | SkillBehavior::Summon { .. }
            | SkillBehavior::Ward { .. }
            | SkillBehavior::Trap { .. }
            | SkillBehavior::Barrier { .. }
            | SkillBehavior::Aura { .. } => {
                unreachable!("non-combat utility skills are skipped above")
            }
        }

        world.queue_cast(attacker_id, 1).expect("cast should queue");
        let mut events = world.tick(COMBAT_FRAME_MS);
        if let SkillBehavior::Projectile { speed, range, .. } = &skill.behavior {
            events.extend(collect_ticks(
                &mut world,
                projectile_frame_budget(*speed, *range),
            ));
        }
        if let SkillBehavior::Channel {
            tick_interval_ms, ..
        } = &skill.behavior
        {
            events.extend(collect_ticks(
                &mut world,
                usize::from(*tick_interval_ms / COMBAT_FRAME_MS + 2),
            ));
        }

        assert!(
            damage_to(&events, target_id).is_none(),
            "{} should not damage a target outside its geometry",
            skill.id
        );
        assert!(
            healing_to(&events, target_id).is_none(),
            "{} should not heal a target outside its geometry",
            skill.id
        );
        let target_state = world.player_state(target_id).expect("target");
        assert_eq!(
            target_state.hit_points, starting_hit_points,
            "{} should leave target hit points untouched on a miss",
            skill.id
        );
        assert!(
            world
                .statuses_for(target_id)
                .expect("target statuses should exist")
                .is_empty(),
            "{} should not apply statuses outside its geometry",
            skill.id
        );
    }
}

#[test]
fn targeting_helpers_ignore_attackers_dead_players_and_exclusions() {
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
                [None, None, None, None, None],
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
            seed(
                &content,
                3,
                "Cara",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
            seed(
                &content,
                4,
                "Drew",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );

    set_player_pose(&mut world, player_id(1), 0, 0, TEST_AIM_X, TEST_AIM_Y);
    set_player_pose(&mut world, player_id(2), 40, 0, TEST_AIM_X, TEST_AIM_Y);
    set_player_pose(&mut world, player_id(3), 50, 8, TEST_AIM_X, TEST_AIM_Y);
    set_player_pose(&mut world, player_id(4), 5, 0, TEST_AIM_X, TEST_AIM_Y);
    world.players.get_mut(&player_id(4)).expect("drew").alive = false;

    assert_eq!(
        world.find_closest_player_near_point(player_id(1), (0, 0), 100),
        Some(player_id(2))
    );
    assert_eq!(
        world.find_first_player_on_segment(player_id(1), (0, 0), (100, 0), 30),
        Some(player_id(2))
    );
    assert_eq!(
        world.find_players_in_radius((50, 0), 20, Some(player_id(2))),
        vec![player_id(3)]
    );
    set_player_pose(&mut world, player_id(2), 80, 40, TEST_AIM_X, TEST_AIM_Y);
    assert_eq!(
        world.find_first_player_on_segment(player_id(1), (0, 0), (100, 0), 10),
        Some(player_id(3)),
        "off-axis targets within the stated radius should still count as beam or projectile hits"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn damage_breaks_sleep_and_stealth_and_shields_absorb_precisely() {
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
                [None, None, None, None, None],
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
    set_player_pose(&mut world, player_id(1), 0, 0, TEST_AIM_X, TEST_AIM_Y);
    set_player_pose(&mut world, player_id(2), 40, 0, TEST_AIM_X, TEST_AIM_Y);
    {
        let bob = world.players.get_mut(&player_id(2)).expect("bob");
        bob.statuses.push(StatusInstance {
            source: player_id(1),
            slot: 1,
            kind: StatusKind::Sleep,
            stacks: 1,
            remaining_ms: 1_000,
            tick_interval_ms: None,
            tick_progress_ms: 0,
            magnitude: 0,
            max_stacks: 1,
            trigger_duration_ms: None,
            shield_remaining: 0,
            expire_payload: None,
            dispel_payload: None,
        });
        bob.statuses.push(StatusInstance {
            source: player_id(1),
            slot: 2,
            kind: StatusKind::Stealth,
            stacks: 1,
            remaining_ms: 1_000,
            tick_interval_ms: None,
            tick_progress_ms: 0,
            magnitude: 0,
            max_stacks: 1,
            trigger_duration_ms: None,
            shield_remaining: 0,
            expire_payload: None,
            dispel_payload: None,
        });
        bob.statuses.push(StatusInstance {
            source: player_id(1),
            slot: 3,
            kind: StatusKind::Shield,
            stacks: 2,
            remaining_ms: 1_000,
            tick_interval_ms: None,
            tick_progress_ms: 0,
            magnitude: 10,
            max_stacks: 5,
            trigger_duration_ms: None,
            shield_remaining: 20,
            expire_payload: None,
            dispel_payload: None,
        });
    }

    let absorbed_events =
        world.apply_damage_internal(player_id(1), &[TargetEntity::Player(player_id(2))], 10);
    assert!(
        damage_to(&absorbed_events, player_id(2)).is_none(),
        "fully absorbed hits should not report any applied damage"
    );
    let bob = world.player_state(player_id(2)).expect("bob");
    assert_eq!(
        bob.hit_points,
        class_hit_points(&content, SkillTree::Warrior)
    );
    let statuses = world.statuses_for(player_id(2)).expect("statuses");
    assert!(statuses
        .iter()
        .all(|status| status.kind != StatusKind::Sleep));
    assert!(statuses
        .iter()
        .all(|status| status.kind != StatusKind::Stealth));
    let shield = world.players[&player_id(2)]
        .statuses
        .iter()
        .find(|status| status.kind == StatusKind::Shield)
        .expect("shield should remain");
    assert_eq!(shield.shield_remaining, 10);
    assert_eq!(shield.stacks, 1);

    let remaining_damage = world.test_consume_shields(player_id(2), 15);
    assert_eq!(remaining_damage, 5);
    assert!(
        world
            .statuses_for(player_id(2))
            .expect("statuses")
            .iter()
            .all(|status| status.kind != StatusKind::Shield),
        "shield should be removed once it is fully consumed"
    );

    let lethal_events =
        world.apply_damage_internal(player_id(1), &[TargetEntity::Player(player_id(2))], 200);
    assert!(lethal_events.iter().any(|event| matches!(
        event,
        SimulationEvent::DamageApplied { target, defeated, .. }
            if *target == player_id(2) && *defeated
    )));
    let defeated = world.player_state(player_id(2)).expect("defeated bob");
    assert!(!defeated.alive);
    assert_eq!(defeated.hit_points, 0);
    assert!(
        world
            .statuses_for(player_id(2))
            .expect("statuses")
            .is_empty(),
        "defeat should clear all remaining statuses"
    );
}

#[test]
fn deployable_overlap_radius_and_target_helpers_include_exact_thresholds() {
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
                [None, None, None, None, None],
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
            seed(
                &content,
                3,
                "Cara",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );
    assert_eq!(SimulationWorld::test_deployable_overlap_radius(10, 20), 30);

    set_player_pose(&mut world, player_id(1), 0, 0, TEST_AIM_X, TEST_AIM_Y);
    set_player_pose(&mut world, player_id(2), 25, 0, TEST_AIM_X, TEST_AIM_Y);
    set_player_pose(&mut world, player_id(3), 90, 0, TEST_AIM_X, TEST_AIM_Y);
    world.deployables.push(DeployableState {
        id: 777,
        owner: player_id(1),
        slot: 0,
        team: TeamSide::TeamA,
        kind: ArenaDeployableKind::Summon,
        x: 30,
        y: 0,
        radius: 20,
        hit_points: 30,
        max_hit_points: 30,
        remaining_ms: 1_000,
        blocks_movement: false,
        blocks_projectiles: false,
        behavior: DeployableBehavior::Ward,
    });

    assert_eq!(
        world.find_closest_target_near_point(player_id(1), (0, 0), 10, true),
        Some(TargetEntity::Player(player_id(2))),
        "players at the exact overlap threshold should still win nearest-target selection"
    );
    assert_eq!(
        world.find_first_target_on_segment(player_id(1), (0, 0), (100, 0), 10, true),
        Some(TargetEntity::Player(player_id(2))),
        "segment targeting should prefer the nearest qualifying hit along the line"
    );
    assert_eq!(
        world.find_targets_in_radius((0, 0), 10, Some(player_id(1)), true),
        vec![
            TargetEntity::Player(player_id(2)),
            TargetEntity::Deployable(777)
        ],
        "radius targeting should include both players and deployables at the overlap threshold"
    );
}
