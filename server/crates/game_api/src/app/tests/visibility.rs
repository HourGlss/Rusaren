use super::*;

#[test]
fn shrubs_block_vision_for_outside_observers() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    let match_id = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

    let runtime = server.matches.get(&match_id).expect("match runtime");
    let alice_state = runtime
        .world
        .player_state(alice.player_id().expect("alice id"))
        .expect("alice state");
    let shrub = *runtime
        .world
        .obstacles()
        .iter()
        .find(|obstacle| obstacle.kind == game_sim::ArenaObstacleKind::Shrub)
        .expect("shrub obstacle");

    assert!(ServerApp::point_is_visible_to_viewer(
        (alice_state.x, alice_state.y),
        (alice_state.x + 100, alice_state.y),
        runtime.world.obstacles(),
    ));
    assert!(!ServerApp::point_is_visible_to_viewer(
        (alice_state.x, alice_state.y),
        (shrub.center_x, shrub.center_y),
        runtime.world.obstacles(),
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn visibility_masks_tiles_players_projectiles_and_effects_are_precise() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    let match_id = enter_combat(
        &mut server,
        &mut transport,
        &mut alice,
        &mut bob,
        skill(SkillTree::Mage, 1),
        skill(SkillTree::Rogue, 1),
    );
    let alice_id = alice.player_id().expect("alice id");
    let bob_id = bob.player_id().expect("bob id");
    let map = server.content.map().clone();

    let runtime = server.matches.get_mut(&match_id).expect("match runtime");
    let (visible_tiles, explored_tiles) =
        ServerApp::build_visibility_masks(runtime, alice_id, &map).expect("visibility masks");
    let alice_state = runtime.world.player_state(alice_id).expect("alice state");
    let close_shrub = *runtime
        .world
        .obstacles()
        .iter()
        .filter(|obstacle| obstacle.kind == game_sim::ArenaObstacleKind::Shrub)
        .min_by_key(|obstacle| {
            (i32::from(obstacle.center_x) - i32::from(alice_state.x)).abs()
                + (i32::from(obstacle.center_y) - i32::from(alice_state.y)).abs()
        })
        .expect("close shrub should exist");
    let far_shrub = *runtime
        .world
        .obstacles()
        .iter()
        .filter(|obstacle| obstacle.kind == game_sim::ArenaObstacleKind::Shrub)
        .max_by_key(|obstacle| {
            (i32::from(obstacle.center_x) - i32::from(alice_state.x)).abs()
                + (i32::from(obstacle.center_y) - i32::from(alice_state.y)).abs()
        })
        .expect("far shrub should exist");

    assert!(ServerApp::mask_contains_point(
        &map,
        &visible_tiles,
        alice_state.x,
        alice_state.y,
    ));
    assert!(ServerApp::mask_contains_point(
        &map,
        &explored_tiles,
        alice_state.x,
        alice_state.y,
    ));
    assert!(ServerApp::mask_intersects_obstacle(
        &map,
        &explored_tiles,
        &close_shrub
    ));
    assert!(!ServerApp::mask_intersects_obstacle(
        &map,
        &visible_tiles,
        &far_shrub
    ));
    assert_eq!(
        ServerApp::tile_center_units(&map, 0, 0),
        (-750, -450),
        "top-left tile center should stay stable"
    );
    assert_eq!(
        ServerApp::tile_center_units(&map, 30, 18),
        (750, 450),
        "bottom-right tile center should stay stable"
    );
    assert_eq!(ServerApp::tile_index_for_point(&map, -800, 0), None);
    assert_eq!(ServerApp::tile_index_for_point(&map, 0, -500), None);
    assert!(ServerApp::containing_shrub(
        runtime.world.obstacles(),
        close_shrub.center_x,
        close_shrub.center_y
    )
    .is_some());
    assert!(
        ServerApp::containing_shrub(runtime.world.obstacles(), alice_state.x, alice_state.y)
            .is_none()
    );
    assert!(ServerApp::point_is_visible_to_viewer(
        (alice_state.x, alice_state.y),
        (alice_state.x + 100, alice_state.y),
        runtime.world.obstacles(),
    ));
    assert!(!ServerApp::point_is_visible_to_viewer(
        (alice_state.x, alice_state.y),
        (
            alice_state.x + i16::try_from(VISION_RADIUS_UNITS).unwrap_or(i16::MAX) + 10,
            alice_state.y
        ),
        runtime.world.obstacles(),
    ));
    assert!(!ServerApp::point_is_visible_to_viewer(
        (alice_state.x, alice_state.y),
        (close_shrub.center_x, close_shrub.center_y),
        runtime.world.obstacles(),
    ));

    let hidden_player_snapshot =
        ServerApp::arena_players_snapshot(runtime, alice_id, &map, &visible_tiles);
    assert!(hidden_player_snapshot
        .iter()
        .any(|player| player.player_id == alice_id));
    assert!(
        hidden_player_snapshot
            .iter()
            .all(|player| player.player_id != bob_id),
        "hidden enemies should not appear in player snapshots"
    );

    runtime
        .world
        .queue_cast(alice_id, 1)
        .expect("projectile skill should queue");
    let _ = runtime.world.tick(COMBAT_FRAME_MS);
    let owned_projectiles =
        ServerApp::arena_projectiles_snapshot(runtime, alice_id, &map, &visible_tiles);
    assert!(
        !owned_projectiles.is_empty(),
        "owners should receive their own projectiles even when enemies cannot"
    );

    let effects = vec![
        game_net::ArenaEffectSnapshot {
            kind: game_net::ArenaEffectKind::SkillShot,
            owner: alice_id,
            slot: 1,
            x: alice_state.x,
            y: alice_state.y,
            target_x: alice_state.x + 80,
            target_y: alice_state.y,
            radius: 24,
        },
        game_net::ArenaEffectSnapshot {
            kind: game_net::ArenaEffectKind::SkillShot,
            owner: bob_id,
            slot: 1,
            x: far_shrub.center_x,
            y: far_shrub.center_y,
            target_x: far_shrub.center_x,
            target_y: far_shrub.center_y,
            radius: 24,
        },
    ];
    let filtered = server.filter_arena_effects(match_id, alice_id, &effects, &map);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].owner, alice_id);
}

#[test]
#[allow(clippy::too_many_lines)]
fn visibility_helper_boundaries_and_shared_shrubs_are_precise() {
    let map = game_content::ArenaMapDefinition {
        map_id: String::from("mutation-mini"),
        width_tiles: 4,
        height_tiles: 4,
        tile_units: 100,
        width_units: 400,
        height_units: 400,
        team_a_anchor: (-150, -150),
        team_b_anchor: (150, 150),
        obstacles: Vec::new(),
    };

    assert_eq!(ServerApp::tile_index_for_point(&map, -200, -200), Some(0));
    assert_eq!(ServerApp::tile_index_for_point(&map, 199, 199), Some(15));
    assert_eq!(ServerApp::tile_index_for_point(&map, 200, 0), None);
    assert_eq!(ServerApp::tile_index_for_point(&map, 0, 200), None);
    assert_eq!(ServerApp::tile_center_units(&map, 1, 2), (-50, 50));

    let mut mask = ServerApp::blank_visibility_mask(&map);
    ServerApp::set_mask_bit(&mut mask, 5);
    let obstacle = game_sim::ArenaObstacle {
        kind: game_sim::ArenaObstacleKind::Pillar,
        center_x: -50,
        center_y: -50,
        half_width: 40,
        half_height: 40,
    };
    assert!(ServerApp::mask_intersects_obstacle(&map, &mask, &obstacle));
    assert!(!ServerApp::mask_intersects_obstacle(
        &map,
        &ServerApp::blank_visibility_mask(&map),
        &obstacle
    ));
    let edge_obstacle = game_sim::ArenaObstacle {
        kind: game_sim::ArenaObstacleKind::Pillar,
        center_x: 125,
        center_y: 75,
        half_width: 60,
        half_height: 40,
    };
    let mut edge_mask = ServerApp::blank_visibility_mask(&map);
    ServerApp::set_mask_bit(&mut edge_mask, 15);
    assert!(ServerApp::mask_intersects_obstacle(
        &map,
        &edge_mask,
        &edge_obstacle
    ));
    let mut outside_mask = ServerApp::blank_visibility_mask(&map);
    ServerApp::set_mask_bit(&mut outside_mask, 9);
    assert!(!ServerApp::mask_intersects_obstacle(
        &map,
        &outside_mask,
        &edge_obstacle
    ));

    assert!(ServerApp::point_is_visible_to_viewer(
        (0, 0),
        (i16::try_from(VISION_RADIUS_UNITS).unwrap_or(i16::MAX), 0),
        &[],
    ));
    assert!(ServerApp::point_is_visible_to_viewer(
        (0, -450),
        (0, -10),
        &[]
    ));
    assert!(!ServerApp::point_is_visible_to_viewer(
        (0, 0),
        (400, 300),
        &[]
    ));
    let shared_shrub = game_sim::ArenaObstacle {
        kind: game_sim::ArenaObstacleKind::Shrub,
        center_x: 0,
        center_y: 0,
        half_width: 40,
        half_height: 40,
    };
    assert!(ServerApp::point_is_visible_to_viewer(
        (0, 0),
        (20, 0),
        &[shared_shrub],
    ));
    let far_shrub = game_sim::ArenaObstacle {
        kind: game_sim::ArenaObstacleKind::Shrub,
        center_x: 150,
        center_y: 0,
        half_width: 40,
        half_height: 40,
    };
    assert!(!ServerApp::point_is_visible_to_viewer(
        (0, 0),
        (150, 0),
        &[shared_shrub, far_shrub],
    ));
    assert!(!ServerApp::point_is_visible_to_viewer(
        (0, 0),
        (70, 0),
        &[shared_shrub],
    ));
    let pillar = game_sim::ArenaObstacle {
        kind: game_sim::ArenaObstacleKind::Pillar,
        center_x: 75,
        center_y: 0,
        half_width: 10,
        half_height: 40,
    };
    assert!(!ServerApp::point_is_visible_to_viewer(
        (0, 0),
        (150, 0),
        &[pillar],
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn snapshot_filters_include_visible_non_owned_entities_and_repair_explored_masks() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    let match_id = enter_combat(
        &mut server,
        &mut transport,
        &mut alice,
        &mut bob,
        skill(SkillTree::Mage, 1),
        skill(SkillTree::Mage, 1),
    );
    let alice_id = alice.player_id().expect("alice id");
    let bob_id = bob.player_id().expect("bob id");
    let map = server.content.map().clone();

    let (alice_position, hidden_position) = {
        let runtime = server.matches.get_mut(&match_id).expect("match runtime");
        runtime.explored_tiles.insert(alice_id, vec![0xFF]);
        let (visible_tiles, explored_tiles) =
            ServerApp::build_visibility_masks(runtime, alice_id, &map).expect("visibility");
        assert_eq!(explored_tiles.len(), visible_tiles.len());
        assert_ne!(explored_tiles, vec![0xFF]);

        let alice_state = runtime.world.player_state(alice_id).expect("alice state");
        let bob_state = runtime.world.player_state(bob_id).expect("bob state");
        let mut player_mask = ServerApp::blank_visibility_mask(&map);
        let bob_tile = ServerApp::tile_index_for_point(&map, bob_state.x, bob_state.y)
            .expect("bob tile should be on the map");
        ServerApp::set_mask_bit(&mut player_mask, bob_tile);
        let players = ServerApp::arena_players_snapshot(runtime, alice_id, &map, &player_mask);
        assert!(players.iter().any(|player| player.player_id == alice_id));
        assert!(players.iter().any(|player| player.player_id == bob_id));

        runtime
            .world
            .queue_cast(bob_id, 1)
            .expect("bob projectile should queue");
        let _ = runtime.world.tick(COMBAT_FRAME_MS);
        let bob_projectile = runtime
            .world
            .projectiles()
            .into_iter()
            .find(|projectile| projectile.owner == bob_id)
            .expect("bob projectile should exist");
        let mut projectile_mask = ServerApp::blank_visibility_mask(&map);
        let projectile_tile =
            ServerApp::tile_index_for_point(&map, bob_projectile.x, bob_projectile.y)
                .expect("projectile tile should be on the map");
        ServerApp::set_mask_bit(&mut projectile_mask, projectile_tile);
        let projectiles =
            ServerApp::arena_projectiles_snapshot(runtime, alice_id, &map, &projectile_mask);
        assert!(projectiles
            .iter()
            .any(|projectile| projectile.owner == bob_id));
        let hidden_projectiles = ServerApp::arena_projectiles_snapshot(
            runtime,
            alice_id,
            &map,
            &ServerApp::blank_visibility_mask(&map),
        );
        assert!(
            hidden_projectiles.is_empty(),
            "non-owned projectiles should disappear when they are outside the visible mask"
        );

        let far_shrub = *runtime
            .world
            .obstacles()
            .iter()
            .filter(|obstacle| obstacle.kind == game_sim::ArenaObstacleKind::Shrub)
            .max_by_key(|obstacle| {
                (i32::from(obstacle.center_x) - i32::from(alice_state.x)).abs()
                    + (i32::from(obstacle.center_y) - i32::from(alice_state.y)).abs()
            })
            .expect("far shrub should exist");
        (
            (alice_state.x, alice_state.y),
            (far_shrub.center_x, far_shrub.center_y),
        )
    };

    let effects = vec![
        game_net::ArenaEffectSnapshot {
            kind: game_net::ArenaEffectKind::SkillShot,
            owner: bob_id,
            slot: 1,
            x: alice_position.0,
            y: alice_position.1,
            target_x: hidden_position.0,
            target_y: hidden_position.1,
            radius: 24,
        },
        game_net::ArenaEffectSnapshot {
            kind: game_net::ArenaEffectKind::SkillShot,
            owner: bob_id,
            slot: 1,
            x: hidden_position.0,
            y: hidden_position.1,
            target_x: alice_position.0,
            target_y: alice_position.1,
            radius: 24,
        },
        game_net::ArenaEffectSnapshot {
            kind: game_net::ArenaEffectKind::SkillShot,
            owner: bob_id,
            slot: 1,
            x: hidden_position.0,
            y: hidden_position.1,
            target_x: hidden_position.0,
            target_y: hidden_position.1,
            radius: 24,
        },
    ];
    let filtered = server.filter_arena_effects(match_id, alice_id, &effects, &map);
    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().all(|effect| effect.owner == bob_id));
    assert!(filtered.iter().any(|effect| {
        (effect.x, effect.y) == alice_position
            && (effect.target_x, effect.target_y) == hidden_position
    }));
    assert!(filtered.iter().any(|effect| {
        (effect.x, effect.y) == hidden_position
            && (effect.target_x, effect.target_y) == alice_position
    }));
}
