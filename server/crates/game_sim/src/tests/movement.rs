use super::*;

#[test]
fn movement_passes_through_shrubs_but_stops_on_pillars() {
    let content = content();
    let mut world = world(
        &content,
        vec![seed(
            &content,
            1,
            "Alice",
            TeamSide::TeamA,
            SkillTree::Warrior,
            [None, None, None, None, None],
        )],
    );
    let shrub = *world
        .obstacles()
        .iter()
        .find(|obstacle| obstacle.kind == ArenaObstacleKind::Shrub)
        .expect("shrub exists");
    {
        let player = world.players.get_mut(&player_id(1)).expect("player");
        player.x = shrub.center_x
            - i16::try_from(shrub.half_width).expect("fits")
            - i16::try_from(PLAYER_RADIUS_UNITS).expect("fits")
            - 30;
        player.y = shrub.center_y;
    }
    world
        .submit_input(player_id(1), MovementIntent::new(1, 0).expect("intent"))
        .expect("input");
    for _ in 0..10 {
        let _ = world.tick(COMBAT_FRAME_MS);
    }
    let state = world.player_state(player_id(1)).expect("player");
    assert!(
        state.x > shrub.center_x - i16::try_from(shrub.half_width).expect("fits"),
        "shrubs should be traversable and allow the player to enter the bush footprint"
    );

    let pillar = *world
        .obstacles()
        .iter()
        .filter(|obstacle| obstacle.kind == ArenaObstacleKind::Pillar && obstacle.center_x < 0)
        .max_by_key(|obstacle| obstacle.center_x)
        .expect("a right-edge pillar should exist on the left side of the map");
    {
        let player = world.players.get_mut(&player_id(1)).expect("player");
        player.x = pillar.center_x
            - i16::try_from(pillar.half_width).expect("fits")
            - i16::try_from(PLAYER_RADIUS_UNITS).expect("fits")
            - 30;
        player.y = pillar.center_y;
    }
    world
        .submit_input(player_id(1), MovementIntent::new(1, 0).expect("intent"))
        .expect("input");
    for _ in 0..10 {
        let _ = world.tick(COMBAT_FRAME_MS);
    }
    let state = world.player_state(player_id(1)).expect("player");
    assert!(
        state.x
            <= pillar.center_x
                - i16::try_from(pillar.half_width).expect("fits")
                - i16::try_from(PLAYER_RADIUS_UNITS).expect("fits")
    );
}

#[test]
fn projectiles_travel_through_shrubs_but_stop_on_pillars() {
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
                [None, None, None, None, None],
            ),
        ],
    );
    let shrub = *world
        .obstacles()
        .iter()
        .find(|obstacle| obstacle.kind == ArenaObstacleKind::Shrub)
        .expect("shrub exists");
    set_player_pose(
        &mut world,
        player_id(1),
        shrub.center_x - 220,
        shrub.center_y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut world,
        player_id(2),
        shrub.center_x + 220,
        shrub.center_y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    world.queue_cast(player_id(1), 1).expect("projectile cast");
    let mut events = world.tick(COMBAT_FRAME_MS);
    events.extend(collect_ticks(&mut world, 24));
    assert!(
        damage_to(&events, player_id(2)).is_some(),
        "projectiles should travel through shrubs"
    );
}

#[test]
fn movement_helpers_return_exact_values() {
    assert_eq!(
        movement_delta(MovementIntent::zero(), 100),
        (0, 0),
        "zero intent should not move"
    );
    assert_eq!(
        movement_delta(MovementIntent::new(1, 0).expect("intent"), 100),
        (100, 0),
        "full horizontal intent should move exactly one speed unit horizontally"
    );
    assert_eq!(
        movement_delta(MovementIntent::new(1, 1).expect("intent"), 100),
        (71, 71),
        "diagonal movement should normalize before scaling"
    );
    assert_eq!(
        movement_delta(MovementIntent::new(1, -1).expect("intent"), 100),
        (71, -71),
        "diagonal movement should preserve signed axes after normalization"
    );
    assert_eq!(
        movement_delta(MovementIntent::new(0, 1).expect("intent"), 100),
        (0, 100),
        "axis-aligned vertical intent should preserve a full-speed y component"
    );
    assert_eq!(
        movement_delta(MovementIntent::new(-1, 0).expect("intent"), 100),
        (-100, 0),
        "axis-aligned horizontal intent should preserve a full-speed signed x component"
    );

    let pillar = ArenaObstacle {
        kind: ArenaObstacleKind::Pillar,
        center_x: 0,
        center_y: 0,
        half_width: 50,
        half_height: 50,
    };

    assert_eq!(
        resolve_movement(0, 0, 500, 0, 500, 500, &[]),
        (222, 0),
        "movement should clamp to the arena edge while respecting player radius"
    );
    assert_eq!(
        resolve_movement(0, 0, -500, -500, 500, 500, &[]),
        (-222, -222)
    );
    assert_eq!(
        resolve_movement(0, 0, 100, -100, 500, 500, &[]),
        (100, -100)
    );
    assert_eq!(
        resolve_movement(-100, 0, 0, 0, 500, 500, &[pillar]),
        (-100, 0),
        "movement into a blocking pillar should revert to the starting point"
    );
    assert_eq!(
        resolve_movement(0, 0, 260, 260, 500, 500, &[]),
        (222, 222),
        "movement should clamp independently on both axes at the arena edge"
    );
}

#[test]
fn movement_ticks_update_vertical_position_only_when_resolution_changes() {
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

    let start = world.player_state(player_id(1)).expect("alice start");
    world
        .submit_input(player_id(1), MovementIntent::new(0, 1).expect("intent"))
        .expect("movement input should apply");
    let events = world.tick(COMBAT_FRAME_MS);
    let moved = world.player_state(player_id(1)).expect("alice moved");
    assert_eq!(moved.x, start.x);
    assert!(moved.y > start.y);
    assert!(events.iter().any(|event| matches!(
        event,
        SimulationEvent::PlayerMoved {
            player_id: moved_player_id,
            x,
            y,
        } if *moved_player_id == player_id(1) && *x == moved.x && *y == moved.y
    )));

    world
        .players
        .get_mut(&player_id(1))
        .expect("alice")
        .statuses
        .push(StatusInstance {
            source: player_id(1),
            slot: 1,
            kind: StatusKind::Root,
            stacks: 1,
            remaining_ms: 500,
            tick_interval_ms: None,
            tick_progress_ms: 0,
            magnitude: 0,
            max_stacks: 1,
            trigger_duration_ms: None,
            shield_remaining: 0,
            expire_payload: None,
            dispel_payload: None,
        });
    world
        .submit_input(player_id(1), MovementIntent::new(0, 1).expect("intent"))
        .expect("movement input should apply");
    let rooted_events = world.tick(COMBAT_FRAME_MS);
    let rooted = world.player_state(player_id(1)).expect("alice rooted");
    assert_eq!(rooted.x, moved.x);
    assert_eq!(rooted.y, moved.y);
    assert!(
        rooted_events.iter().all(|event| !matches!(
            event,
            SimulationEvent::PlayerMoved { player_id: moved_player_id, .. }
                if *moved_player_id == player_id(1)
        )),
        "rooted players should not emit movement events when their resolved position is unchanged"
    );
}

#[test]
fn projectile_spawn_and_advance_follow_authored_math() {
    let content = content();
    let mage_skill = content
        .skills()
        .resolve(&choice(SkillTree::Mage, 1))
        .expect("mage tier one should exist")
        .clone();
    let SkillBehavior::Projectile { speed, range, .. } = mage_skill.behavior else {
        panic!("mage tier one should remain a projectile");
    };
    let mut world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Mage,
                &mage_skill,
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
        TEST_ATTACKER_X,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    {
        let alice = world.players.get_mut(&player_id(1)).expect("alice");
        alice.mana = mage_skill.behavior.mana_cost();
    }

    world.queue_cast(player_id(1), 1).expect("projectile cast");
    let cast_events = world.tick(COMBAT_FRAME_MS);
    assert_eq!(world.projectiles.len(), 1);
    let projectile = world.projectiles[0].clone();
    let spawn_x = cast_events
        .iter()
        .find_map(|event| match event {
            SimulationEvent::EffectSpawned { effect }
                if effect.owner == player_id(1) && effect.slot == 1 =>
            {
                Some(effect.x)
            }
            _ => None,
        })
        .expect("projectile cast should emit a spawn effect");
    assert_eq!(
        spawn_x,
        TEST_ATTACKER_X
            + i16::try_from(u32::from(PLAYER_RADIUS_UNITS) + u32::from(projectile.radius))
                .unwrap_or(i16::MAX)
    );
    assert_eq!(projectile.y, TEST_OPEN_LANE_Y);
    assert!(
        world.player_state(player_id(1)).expect("alice").mana < mage_skill.behavior.mana_cost(),
        "casts at exact mana cost should still succeed and consume mana"
    );

    let step = i16::try_from(travel_distance_units(speed, COMBAT_FRAME_MS)).unwrap_or(i16::MAX);
    assert_eq!(
        projectile.x,
        spawn_x + step,
        "projectiles should advance during the frame that spawned them"
    );
    let _ = world.tick(COMBAT_FRAME_MS);
    assert_eq!(world.projectiles.len(), 1);
    assert_eq!(world.projectiles[0].x, spawn_x + step.saturating_mul(2));
    assert!(
        world.projectiles[0].remaining_range_units < i32::from(range),
        "projectiles should spend range as they travel"
    );
}

#[test]
fn projectile_helpers_preserve_vertical_motion_and_expire_at_range() {
    let content = content();
    let mage_skill = content
        .skills()
        .resolve(&choice(SkillTree::Mage, 1))
        .expect("mage tier one should exist")
        .clone();
    let SkillBehavior::Projectile {
        speed,
        range,
        radius,
        effect,
        ref payload,
        ..
    } = mage_skill.behavior
    else {
        panic!("mage tier one should remain a projectile");
    };
    let mut world = world(
        &content,
        vec![seed_with_slot_one_skill(
            &content,
            1,
            "Alice",
            TeamSide::TeamA,
            SkillTree::Mage,
            &mage_skill,
        )],
    );

    set_player_pose(
        &mut world,
        player_id(1),
        TEST_ATTACKER_X,
        TEST_OPEN_LANE_Y,
        0,
        120,
    );
    let attacker_state = world.player_state(player_id(1)).expect("alice state");
    let spawn_events = world.spawn_projectile(
        player_id(1),
        1,
        attacker_state,
        speed,
        range,
        radius,
        arena_effect_kind(effect),
        payload.clone(),
    );
    let spawn_effect = spawn_events
        .into_iter()
        .find_map(|event| match event {
            SimulationEvent::EffectSpawned { effect } => Some(effect),
            _ => None,
        })
        .expect("spawning should emit an effect");
    assert_eq!(spawn_effect.x, TEST_ATTACKER_X);
    assert_eq!(
        spawn_effect.y,
        TEST_OPEN_LANE_Y
            + i16::try_from(u32::from(PLAYER_RADIUS_UNITS) + u32::from(radius)).unwrap_or(i16::MAX)
    );

    world.projectiles.clear();
    world.projectiles.push(ProjectileState {
        owner: player_id(1),
        slot: 1,
        kind: ArenaEffectKind::SkillShot,
        x: 10,
        y: 20,
        direction_x: 0.0,
        direction_y: 1.0,
        speed_units_per_second: 100,
        remaining_range_units: 50,
        radius: 12,
        payload: payload.clone(),
    });
    let mut events = Vec::new();
    world.advance_projectiles(100, &mut events);
    assert!(events.is_empty());
    assert_eq!(world.projectiles.len(), 1);
    assert_eq!(world.projectiles[0].x, 10);
    assert_eq!(world.projectiles[0].y, 30);
    assert_eq!(world.projectiles[0].remaining_range_units, 40);

    world.projectiles[0].remaining_range_units = 10;
    world.advance_projectiles(100, &mut events);
    assert!(
        world.projectiles.is_empty(),
        "projectiles should disappear once they exhaust their authored range"
    );
}

#[test]
fn spawn_positions_and_obstacle_blocking_rules_stay_stable() {
    let content = content();
    let map = content.map();

    assert_eq!(
        spawn_position(TeamSide::TeamA, 0, map),
        (map.team_a_anchor.0, map.team_a_anchor.1, DEFAULT_AIM_X)
    );
    assert_eq!(
        spawn_position(TeamSide::TeamB, 1, map),
        (
            map.team_b_anchor.0,
            map.team_b_anchor.1 + SPAWN_SPACING_UNITS,
            -DEFAULT_AIM_X,
        )
    );
    assert_eq!(
        spawn_position(TeamSide::TeamA, 2, map),
        (
            map.team_a_anchor.0,
            map.team_a_anchor.1 + SPAWN_SPACING_UNITS.saturating_mul(2),
            DEFAULT_AIM_X,
        )
    );

    let pillar = map
        .obstacles
        .iter()
        .find(|obstacle| obstacle.kind == ArenaMapObstacleKind::Pillar)
        .expect("pillar should exist");
    let mapped = map_obstacle_to_sim_obstacle(pillar);
    assert!(obstacle_blocks_movement(&mapped));
    assert!(obstacle_blocks_projectiles(&mapped));
    assert!(obstacle_blocks_vision(&mapped));
}

#[test]
fn teleport_passes_through_pillars_and_clamps_to_valid_space() {
    let content = content();
    let teleport_skill = content
        .skills()
        .resolve(&choice(SkillTree::Mage, 2))
        .expect("mage teleport should exist")
        .clone();
    let mut world = world(
        &content,
        vec![seed_with_slot_one_skill(
            &content,
            1,
            "Alice",
            TeamSide::TeamA,
            SkillTree::Mage,
            &teleport_skill,
        )],
    );
    world.obstacles = vec![ArenaObstacle {
        kind: ArenaObstacleKind::Pillar,
        center_x: -240,
        center_y: TEST_OPEN_LANE_Y,
        half_width: 25,
        half_height: 25,
    }];
    let pillar = world.obstacles[0];

    let SkillBehavior::Teleport { distance, .. } = &teleport_skill.behavior else {
        panic!("mage tier two should remain a teleport");
    };
    let desired_x = pillar.center_x
        + i16::try_from(pillar.half_width).expect("fits")
        + i16::try_from(PLAYER_RADIUS_UNITS).expect("fits")
        + 40;
    set_player_pose(
        &mut world,
        player_id(1),
        desired_x - i16::try_from(*distance).expect("fits"),
        pillar.center_y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    let teleport_events =
        resolve_skill_cast(&mut world, player_id(1), 1, teleport_skill.behavior.clone());
    let teleported = world.player_state(player_id(1)).expect("alice");
    assert!(
        teleported.x > pillar.center_x + i16::try_from(pillar.half_width).expect("fits"),
        "teleports should pass through intervening pillars when the destination is valid"
    );
    assert!(moved_player(&teleport_events, player_id(1)).is_some());

    let arena_edge_x = i16::try_from(world.arena_width_units() / 2).unwrap_or(i16::MAX)
        - i16::try_from(PLAYER_RADIUS_UNITS).expect("fits");
    set_player_pose(
        &mut world,
        player_id(1),
        arena_edge_x - 30,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    let _ = collect_ticks(&mut world, 17);
    let _ = resolve_skill_cast(&mut world, player_id(1), 1, teleport_skill.behavior.clone());
    let clamped = world.player_state(player_id(1)).expect("alice");
    assert!(
        clamped.x <= arena_edge_x,
        "teleports should clamp to the nearest valid in-bounds destination"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn barriers_block_movement_and_projectiles() {
    let content = content();
    let barrier_skill = content
        .skills()
        .resolve(&choice(SkillTree::Warrior, 2))
        .expect("warrior barrier should exist")
        .clone();
    let mut movement_world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Warrior,
                &barrier_skill,
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
        &mut movement_world,
        player_id(1),
        -300,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    let barrier_events = resolve_skill_cast(
        &mut movement_world,
        player_id(1),
        1,
        barrier_skill.behavior.clone(),
    );
    let barrier = movement_world
        .deployables()
        .into_iter()
        .find(|deployable| deployable.kind == ArenaDeployableKind::Barrier)
        .expect("barrier should spawn");
    assert!(barrier_events.iter().any(|event| matches!(
        event,
        SimulationEvent::DeployableSpawned { deployable_id, .. } if *deployable_id == barrier.id
    )));

    let blocker_x = barrier.x
        - i16::try_from(barrier.radius).expect("fits")
        - i16::try_from(PLAYER_RADIUS_UNITS).expect("fits");
    set_player_pose(
        &mut movement_world,
        player_id(2),
        blocker_x - 20,
        barrier.y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    movement_world
        .submit_input(player_id(2), MovementIntent::new(1, 0).expect("intent"))
        .expect("movement");
    for _ in 0..8 {
        let _ = movement_world.tick(COMBAT_FRAME_MS);
    }
    let moved = movement_world.player_state(player_id(2)).expect("bob");
    assert!(
        moved.x <= blocker_x,
        "barriers should block player movement like a temporary obstacle"
    );

    let projectile_skill = content
        .skills()
        .resolve(&choice(SkillTree::Mage, 1))
        .expect("mage projectile should exist")
        .clone();
    let mut projectile_world = world(
        &content,
        vec![
            seed_with_slot_one_skill(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Warrior,
                &barrier_skill,
            ),
            seed_with_slot_one_skill(
                &content,
                2,
                "Mage",
                TeamSide::TeamB,
                SkillTree::Mage,
                &projectile_skill,
            ),
            seed(
                &content,
                3,
                "Target",
                TeamSide::TeamA,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );
    set_player_pose(
        &mut projectile_world,
        player_id(1),
        -300,
        TEST_OPEN_LANE_Y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    let _ = resolve_skill_cast(
        &mut projectile_world,
        player_id(1),
        1,
        barrier_skill.behavior.clone(),
    );
    let barrier = projectile_world
        .deployables()
        .into_iter()
        .find(|deployable| deployable.kind == ArenaDeployableKind::Barrier)
        .expect("barrier should spawn");
    set_player_pose(
        &mut projectile_world,
        player_id(2),
        barrier.x - 180,
        barrier.y,
        TEST_AIM_X,
        TEST_AIM_Y,
    );
    set_player_pose(
        &mut projectile_world,
        player_id(3),
        barrier.x + 180,
        barrier.y,
        -TEST_AIM_X,
        TEST_AIM_Y,
    );

    let projectile_events = resolve_skill_cast(
        &mut projectile_world,
        player_id(2),
        1,
        projectile_skill.behavior.clone(),
    );
    assert!(
        damage_to(&projectile_events, player_id(3)).is_none(),
        "barriers should block projectiles fired through them"
    );
    assert!(
        projectile_world.projectiles.is_empty(),
        "blocked projectiles should be consumed by the barrier lane"
    );
}

#[test]
fn combat_obstacles_walkability_and_teleport_resolution_stay_precise() {
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

    let walkable_x = TEST_ATTACKER_X;
    let walkable_y = TEST_OPEN_LANE_Y;
    let min_x = -i16::try_from(world.arena_width_units() / 2).unwrap_or(i16::MAX)
        + i16::try_from(PLAYER_RADIUS_UNITS).unwrap_or(i16::MAX);
    let max_x = i16::try_from(world.arena_width_units() / 2).unwrap_or(i16::MAX)
        - i16::try_from(PLAYER_RADIUS_UNITS).unwrap_or(i16::MAX);
    assert!(world.is_walkable_position(walkable_x, walkable_y));
    assert!(world.is_walkable_position(min_x, walkable_y));
    assert!(world.is_walkable_position(max_x, walkable_y));
    assert!(!world.is_walkable_position(min_x.saturating_sub(1), walkable_y));
    assert!(!world.is_walkable_position(max_x.saturating_add(1), walkable_y));

    let pillar = *world
        .obstacles()
        .iter()
        .find(|obstacle| obstacle.kind == ArenaObstacleKind::Pillar)
        .expect("pillar exists");
    assert!(!world.is_walkable_position(pillar.center_x, pillar.center_y));

    let projectile_only = DeployableState {
        id: 900,
        owner: player_id(1),
        team: TeamSide::TeamA,
        kind: ArenaDeployableKind::Ward,
        x: walkable_x + 80,
        y: walkable_y,
        radius: 18,
        hit_points: 1,
        max_hit_points: 1,
        remaining_ms: 1_000,
        blocks_movement: false,
        blocks_projectiles: true,
        behavior: DeployableBehavior::Ward,
    };
    let movement_only = DeployableState {
        id: 901,
        owner: player_id(1),
        team: TeamSide::TeamA,
        kind: ArenaDeployableKind::Barrier,
        x: walkable_x + 160,
        y: walkable_y,
        radius: 18,
        hit_points: 1,
        max_hit_points: 1,
        remaining_ms: 1_000,
        blocks_movement: true,
        blocks_projectiles: false,
        behavior: DeployableBehavior::Barrier,
    };
    world.deployables.push(projectile_only.clone());
    world.deployables.push(movement_only.clone());

    let combat_obstacles = world.combat_obstacles();
    assert!(combat_obstacles.iter().any(|obstacle| {
        obstacle.center_x == projectile_only.x && obstacle.center_y == projectile_only.y
    }));
    assert!(combat_obstacles.iter().any(|obstacle| {
        obstacle.center_x == movement_only.x && obstacle.center_y == movement_only.y
    }));
    assert!(!world.is_walkable_position(projectile_only.x, projectile_only.y));
    assert!(!world.is_walkable_position(movement_only.x, movement_only.y));

    let exact_boundary_destination = world.resolve_teleport_destination(
        max_x.saturating_sub(35),
        walkable_y,
        max_x + 1,
        walkable_y,
    );
    assert_eq!(exact_boundary_destination, (max_x, walkable_y));

    world.obstacles = vec![ArenaObstacle {
        kind: ArenaObstacleKind::Pillar,
        center_x: 0,
        center_y: 0,
        half_width: 212,
        half_height: 200,
    }];
    let no_valid_path = world.resolve_teleport_destination(-220, 0, 180, 0);
    assert_eq!(
        no_valid_path,
        (-220, 0),
        "teleports should fall back to the start when no sampled destination is walkable"
    );
}

#[test]
fn deployable_advancement_target_positions_and_enemy_lookup_stay_precise() {
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
    set_player_pose(&mut world, player_id(1), 0, 0, TEST_AIM_X, TEST_AIM_Y);
    set_player_pose(&mut world, player_id(2), 25, 0, TEST_AIM_X, TEST_AIM_Y);
    set_player_pose(&mut world, player_id(3), 90, 0, TEST_AIM_X, TEST_AIM_Y);

    world.deployables.push(DeployableState {
        id: 451,
        owner: player_id(1),
        team: TeamSide::TeamA,
        kind: ArenaDeployableKind::Trap,
        x: 0,
        y: 0,
        radius: 10,
        hit_points: 20,
        max_hit_points: 20,
        remaining_ms: 1_100,
        blocks_movement: false,
        blocks_projectiles: false,
        behavior: DeployableBehavior::Trap {
            payload: game_content::EffectPayload {
                kind: CombatValueKind::Damage,
                amount: 7,
                status: None,
                interrupt_silence_duration_ms: None,
                dispel: None,
            },
        },
    });

    assert_eq!(
        world.test_target_position(TargetEntity::Player(player_id(2))),
        (25, 0)
    );
    assert_eq!(
        world.test_target_position(TargetEntity::Deployable(451)),
        (0, 0)
    );
    assert_eq!(
        world.test_target_position(TargetEntity::Deployable(999)),
        (0, 0)
    );
    assert_eq!(
        world.test_find_enemy_player_near_point(player_id(1), (0, 0), 10),
        Some(player_id(2)),
        "enemy lookup should include players at the exact overlap threshold"
    );

    let mut events = Vec::new();
    world.advance_deployables(100, &mut events);
    assert!(events.iter().any(|event| matches!(
        event,
        SimulationEvent::EffectSpawned { effect }
            if effect.owner == player_id(1) && effect.target_x == 25 && effect.target_y == 0
    )));
    assert_eq!(damage_to(&events, player_id(2)), Some(7));
    assert!(
        world.deployables.is_empty(),
        "triggered traps should expire after hitting a target"
    );
}
