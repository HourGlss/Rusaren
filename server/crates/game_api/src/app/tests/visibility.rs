use super::*;
use std::collections::BTreeMap;

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
    let (visible_tiles, explored_tiles) = build_runtime_visibility_masks(runtime, alice_id, &map);
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
    let expected_top_left = (
        i16::try_from(-i32::from(map.width_units) / 2 + i32::from(map.tile_units) / 2)
            .expect("top-left x fits"),
        i16::try_from(-i32::from(map.height_units) / 2 + i32::from(map.tile_units) / 2)
            .expect("top-left y fits"),
    );
    let expected_bottom_right = (
        i16::try_from(i32::from(map.width_units) / 2 - i32::from(map.tile_units) / 2)
            .expect("bottom-right x fits"),
        i16::try_from(i32::from(map.height_units) / 2 - i32::from(map.tile_units) / 2)
            .expect("bottom-right y fits"),
    );
    assert_eq!(
        ServerApp::tile_center_units(&map, 0, 0),
        expected_top_left,
        "top-left tile center should track the authored map bounds"
    );
    assert_eq!(
        ServerApp::tile_center_units(
            &map,
            usize::from(map.width_tiles) - 1,
            usize::from(map.height_tiles) - 1,
        ),
        expected_bottom_right,
        "bottom-right tile center should track the authored map bounds"
    );
    assert_eq!(
        ServerApp::tile_index_for_point(
            &map,
            i16::try_from(-i32::from(map.width_units) / 2 - 50).expect("x fits"),
            0,
        ),
        None
    );
    assert_eq!(
        ServerApp::tile_index_for_point(
            &map,
            0,
            i16::try_from(-i32::from(map.height_units) / 2 - 50).expect("y fits"),
        ),
        None
    );
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
    let owned_projectiles = runtime_projectiles_snapshot(runtime, alice_id, &map, &visible_tiles);
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
        footprint_mask: vec![0xFF, 0xFF],
        team_a_anchors: vec![(-150, -150)],
        team_b_anchors: vec![(150, 150)],
        obstacles: Vec::new(),
        features: Vec::new(),
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

    assert!(!ServerApp::point_is_visible_to_viewer(
        (0, 0),
        (70, 0),
        &[shared_shrub],
    ));
    assert!(!ServerApp::point_is_visible_to_viewer(
        (20, 0),
        (70, 0),
        &[shared_shrub],
    ));
    assert!(!ServerApp::point_is_visible_to_viewer(
        (70, 0),
        (20, 0),
        &[shared_shrub],
    ));

    let single_tile_top_left = game_sim::ArenaObstacle {
        kind: game_sim::ArenaObstacleKind::Pillar,
        center_x: -150,
        center_y: -150,
        half_width: 49,
        half_height: 49,
    };
    let mut top_left_mask = ServerApp::blank_visibility_mask(&map);
    ServerApp::set_mask_bit(&mut top_left_mask, 0);
    assert!(ServerApp::mask_intersects_obstacle(
        &map,
        &top_left_mask,
        &single_tile_top_left
    ));
    let mut wrong_top_row_mask = ServerApp::blank_visibility_mask(&map);
    ServerApp::set_mask_bit(&mut wrong_top_row_mask, 1);
    assert!(!ServerApp::mask_intersects_obstacle(
        &map,
        &wrong_top_row_mask,
        &single_tile_top_left
    ));
    let mut wrong_left_column_mask = ServerApp::blank_visibility_mask(&map);
    ServerApp::set_mask_bit(&mut wrong_left_column_mask, 4);
    assert!(!ServerApp::mask_intersects_obstacle(
        &map,
        &wrong_left_column_mask,
        &single_tile_top_left
    ));

    let single_tile_bottom_right = game_sim::ArenaObstacle {
        kind: game_sim::ArenaObstacleKind::Pillar,
        center_x: 150,
        center_y: 150,
        half_width: 49,
        half_height: 49,
    };
    let mut bottom_right_mask = ServerApp::blank_visibility_mask(&map);
    ServerApp::set_mask_bit(&mut bottom_right_mask, 15);
    assert!(ServerApp::mask_intersects_obstacle(
        &map,
        &bottom_right_mask,
        &single_tile_bottom_right
    ));
    let mut wrong_bottom_row_mask = ServerApp::blank_visibility_mask(&map);
    ServerApp::set_mask_bit(&mut wrong_bottom_row_mask, 14);
    assert!(!ServerApp::mask_intersects_obstacle(
        &map,
        &wrong_bottom_row_mask,
        &single_tile_bottom_right
    ));
    let mut wrong_right_column_mask = ServerApp::blank_visibility_mask(&map);
    ServerApp::set_mask_bit(&mut wrong_right_column_mask, 11);
    assert!(!ServerApp::mask_intersects_obstacle(
        &map,
        &wrong_right_column_mask,
        &single_tile_bottom_right
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
            build_runtime_visibility_masks(runtime, alice_id, &map);
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
        let projectiles = runtime_projectiles_snapshot(runtime, alice_id, &map, &projectile_mask);
        assert!(projectiles
            .iter()
            .any(|projectile| projectile.owner == bob_id));
        let hidden_projectiles = runtime_projectiles_snapshot(
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

#[test]
fn deployable_snapshots_always_include_owned_and_only_visible_foreign_entities() {
    let content = GameContent::bundled().expect("bundled content");
    let map = content.map().clone();
    let ranger_tree = SkillTree::new("Ranger").expect("ranger tree");
    let alice_id = manual_player_id(1);
    let bob_id = manual_player_id(2);
    let mut runtime = manual_runtime(
        &content,
        vec![
            manual_seed(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                ranger_tree.clone(),
                [None, None, None, Some(skill(ranger_tree.clone(), 4)), None],
            ),
            manual_seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                ranger_tree.clone(),
                [None, None, None, Some(skill(ranger_tree.clone(), 4)), None],
            ),
        ],
    );

    runtime.world.update_aim(alice_id, 1, 0).expect("alice aim");
    runtime.world.queue_cast(alice_id, 4).expect("alice ward");
    let _ = runtime.world.tick(COMBAT_FRAME_MS);

    runtime.world.update_aim(bob_id, -1, 0).expect("bob aim");
    runtime.world.queue_cast(bob_id, 4).expect("bob ward");
    let _ = runtime.world.tick(COMBAT_FRAME_MS);

    let deployables = runtime.world.deployables();
    let alice_ward = deployables
        .iter()
        .copied()
        .find(|deployable| deployable.owner == alice_id)
        .expect("alice ward");
    let bob_ward = deployables
        .iter()
        .copied()
        .find(|deployable| deployable.owner == bob_id)
        .expect("bob ward");

    let hidden_snapshot = runtime_deployables_snapshot(
        &runtime,
        alice_id,
        &map,
        &ServerApp::blank_visibility_mask(&map),
    );
    assert!(
        hidden_snapshot
            .iter()
            .any(|deployable| deployable.owner == alice_id),
        "owners should always receive their own deployables"
    );
    assert!(
        hidden_snapshot
            .iter()
            .all(|deployable| deployable.owner != bob_id),
        "foreign deployables should stay hidden without a visible tile"
    );

    let mut visible_mask = ServerApp::blank_visibility_mask(&map);
    let bob_tile = ServerApp::tile_index_for_point(&map, bob_ward.x, bob_ward.y)
        .expect("bob ward tile should exist");
    ServerApp::set_mask_bit(&mut visible_mask, bob_tile);
    let visible_snapshot = runtime_deployables_snapshot(&runtime, alice_id, &map, &visible_mask);
    assert!(visible_snapshot
        .iter()
        .any(|deployable| deployable.id == alice_ward.id && deployable.owner == alice_id));
    assert!(
        visible_snapshot
            .iter()
            .any(|deployable| deployable.id == bob_ward.id && deployable.owner == bob_id),
        "foreign deployables should appear once their tile is visible"
    );
}

#[test]
fn aura_deployables_are_omitted_from_arena_snapshots() {
    let content = GameContent::bundled().expect("bundled content");
    let map = content.map().clone();
    let paladin_tree = SkillTree::new("Paladin").expect("paladin tree");
    let alice_id = manual_player_id(1);
    let mut runtime = manual_runtime(
        &content,
        vec![
            manual_seed(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                paladin_tree.clone(),
                [None, None, Some(skill(paladin_tree.clone(), 3)), None, None],
            ),
            manual_seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                paladin_tree.clone(),
                [None, None, None, None, None],
            ),
        ],
    );

    runtime.world.queue_cast(alice_id, 3).expect("alice aura");
    let _ = runtime.world.tick(COMBAT_FRAME_MS);

    assert!(
        runtime
            .world
            .deployables()
            .into_iter()
            .any(|deployable| deployable.kind == game_sim::ArenaDeployableKind::Aura),
        "the authored aura skill should still create an aura deployable in the sim"
    );

    let snapshot = runtime_deployables_snapshot(
        &runtime,
        alice_id,
        &map,
        &ServerApp::blank_visibility_mask(&map),
    );
    assert!(
        snapshot
            .iter()
            .all(|deployable| deployable.kind != game_net::ArenaDeployableKind::Aura),
        "auras should stay gameplay-only and should not be surfaced as visible arena deployables"
    );
}

fn manual_player_id(raw: u32) -> PlayerId {
    PlayerId::new(raw).expect("valid player id")
}

fn build_runtime_visibility_masks(
    runtime: &mut MatchRuntime,
    viewer_id: PlayerId,
    map: &game_content::ArenaMapDefinition,
) -> (Vec<u8>, Vec<u8>) {
    ServerApp::build_visibility_masks(&runtime.world, &mut runtime.explored_tiles, viewer_id, map)
        .expect("visibility mask")
}

fn runtime_projectiles_snapshot(
    runtime: &MatchRuntime,
    viewer_id: PlayerId,
    map: &game_content::ArenaMapDefinition,
    visible_tiles: &[u8],
) -> Vec<game_net::ArenaProjectileSnapshot> {
    ServerApp::arena_projectiles_snapshot(&runtime.world, viewer_id, map, visible_tiles)
}

fn runtime_deployables_snapshot(
    runtime: &MatchRuntime,
    viewer_id: PlayerId,
    map: &game_content::ArenaMapDefinition,
    visible_tiles: &[u8],
) -> Vec<game_net::ArenaDeployableSnapshot> {
    ServerApp::arena_deployables_snapshot(&runtime.world, viewer_id, map, visible_tiles)
}

fn manual_assignment(raw_id: u32, raw_name: &str, team: TeamSide) -> TeamAssignment {
    TeamAssignment {
        player_id: manual_player_id(raw_id),
        player_name: player_name(raw_name),
        record: PlayerRecord::new(),
        team,
    }
}

#[allow(clippy::needless_pass_by_value)]
fn manual_seed(
    content: &GameContent,
    raw_id: u32,
    raw_name: &str,
    team: TeamSide,
    primary_tree: SkillTree,
    choices: [Option<SkillChoice>; 5],
) -> game_sim::SimPlayerSeed {
    game_sim::SimPlayerSeed {
        assignment: manual_assignment(raw_id, raw_name, team),
        hit_points: 100,
        melee: content
            .skills()
            .melee_for(&primary_tree)
            .expect("melee should exist")
            .clone(),
        skills: choices
            .map(|choice| choice.and_then(|picked| content.skills().resolve(&picked).cloned())),
    }
}

fn manual_runtime(content: &GameContent, seeds: Vec<game_sim::SimPlayerSeed>) -> MatchRuntime {
    let roster = seeds
        .iter()
        .map(|seed| seed.assignment.clone())
        .collect::<Vec<_>>();
    let participants = roster
        .iter()
        .map(|assignment| assignment.player_id)
        .collect();
    let session = MatchSession::new(
        MatchId::new(1).expect("match id"),
        roster.clone(),
        game_match::MatchConfig::v1(),
    )
    .expect("match session");
    let world = game_sim::SimulationWorld::new(seeds, content.map()).expect("world");
    MatchRuntime {
        roster,
        participants,
        session,
        world,
        explored_tiles: BTreeMap::new(),
        combat_frame_index: 0,
        feedback: MatchCombatFeedback::default(),
    }
}

fn test_visibility_map() -> &'static str {
    "....................\n\
....................\n\
.....A.........B....\n\
....................\n\
....................\n"
}

fn content_with_map(label: &str, map_text: &str) -> (GameContent, std::path::PathBuf) {
    let root = temp_dir(label);
    remove_dir_if_exists(&root);
    copy_dir_all(&workspace_content_root(), &root);
    fs::write(root.join("maps").join("prototype_arena.txt"), map_text).expect("map override");
    let content = GameContent::load_from_root(&root).expect("custom content");
    (content, root)
}

fn content_with_reveal_only_field(
    label: &str,
    map_text: &str,
) -> (GameContent, std::path::PathBuf) {
    let root = temp_dir(label);
    remove_dir_if_exists(&root);
    copy_dir_all(&workspace_content_root(), &root);
    fs::write(root.join("maps").join("prototype_arena.txt"), map_text).expect("map override");
    let mage_path = root.join("skills").join("mage.yaml");
    let mage_yaml = fs::read_to_string(&mage_path).expect("mage yaml");
    let patched = mage_yaml
        .replace(
            "amount: 4\r\n        status:",
            "amount: 0\r\n        status:",
        )
        .replace("amount: 4\n        status:", "amount: 0\n        status:");
    assert_ne!(
        patched, mage_yaml,
        "test mage reveal field patch should apply"
    );
    fs::write(&mage_path, patched).expect("patched mage yaml");
    let content = GameContent::load_from_root(&root).expect("custom content");
    (content, root)
}

fn advance_world(world: &mut game_sim::SimulationWorld, frames: usize) {
    for _ in 0..frames {
        let _ = world.tick(COMBAT_FRAME_MS);
    }
}

#[test]
fn allied_wards_extend_visibility_masks_and_enemy_snapshots() {
    let (content, root) = content_with_map("ward-visibility", test_visibility_map());
    let alice_id = manual_player_id(1);
    let bob_id = manual_player_id(2);
    let ranger_tree = SkillTree::new("Ranger").expect("ranger tree");
    let map = content.map().clone();
    let mut runtime = manual_runtime(
        &content,
        vec![
            manual_seed(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                ranger_tree.clone(),
                [None, None, None, Some(skill(ranger_tree.clone(), 4)), None],
            ),
            manual_seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Mage,
                [Some(skill(SkillTree::Mage, 1)), None, None, None, None],
            ),
        ],
    );

    let bob_state = runtime.world.player_state(bob_id).expect("bob state");
    let (visible_before, _) = build_runtime_visibility_masks(&mut runtime, alice_id, &map);
    assert!(
        !ServerApp::mask_contains_point(&map, &visible_before, bob_state.x, bob_state.y),
        "bob should begin outside alice's base vision radius on the custom map"
    );
    assert!(
        ServerApp::arena_players_snapshot(&runtime, alice_id, &map, &visible_before)
            .iter()
            .all(|player| player.player_id != bob_id),
        "enemy players outside the visibility mask should not appear in snapshots"
    );

    runtime
        .world
        .update_aim(alice_id, 1, 0)
        .expect("ward aim should update");
    runtime
        .world
        .queue_cast(alice_id, 4)
        .expect("ward should queue");
    let _ = runtime.world.tick(COMBAT_FRAME_MS);
    assert!(
        runtime.world.deployables().into_iter().any(|deployable| {
            deployable.owner == alice_id && deployable.kind == game_sim::ArenaDeployableKind::Ward
        }),
        "the ward cast should create a deployable vision source"
    );

    let (visible_after, _) = build_runtime_visibility_masks(&mut runtime, alice_id, &map);
    assert!(
        ServerApp::mask_contains_point(&map, &visible_after, bob_state.x, bob_state.y),
        "the allied ward should extend visibility to bob's location"
    );
    assert!(
        ServerApp::arena_players_snapshot(&runtime, alice_id, &map, &visible_after)
            .iter()
            .any(|player| player.player_id == bob_id),
        "once a ward sees bob, the enemy should appear in the arena player snapshot"
    );

    remove_dir_if_exists(&root);
}

#[test]
fn stealthed_players_stay_hidden_until_a_reveal_effect_lands() {
    let (content, root) =
        content_with_reveal_only_field("stealth-reveal-visibility", test_visibility_map());
    let alice_id = manual_player_id(1);
    let bob_id = manual_player_id(2);
    let map = content.map().clone();
    let mut runtime = manual_runtime(
        &content,
        vec![
            manual_seed(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Rogue,
                [None, None, None, Some(skill(SkillTree::Rogue, 4)), None],
            ),
            manual_seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Mage,
                [None, None, None, Some(skill(SkillTree::Mage, 4)), None],
            ),
        ],
    );

    runtime
        .world
        .queue_cast(alice_id, 4)
        .expect("stealth aura should queue");
    let _ = runtime.world.tick(COMBAT_FRAME_MS);
    advance_world(&mut runtime.world, 10);
    assert!(
        runtime
            .world
            .statuses_for(alice_id)
            .unwrap_or_default()
            .iter()
            .any(|status| status.kind == game_content::StatusKind::Stealth),
        "the rogue should gain stealth after the aura pulses"
    );

    runtime.world.update_aim(bob_id, -1, 0).expect("bob aim");
    runtime
        .world
        .submit_input(
            bob_id,
            game_sim::MovementIntent::new(-1, 0).expect("movement intent"),
        )
        .expect("bob movement");
    advance_world(&mut runtime.world, 5);
    runtime
        .world
        .submit_input(bob_id, game_sim::MovementIntent::zero())
        .expect("stop movement");

    let alice_state = runtime.world.player_state(alice_id).expect("alice state");
    let (visible_while_stealthed, _) = build_runtime_visibility_masks(&mut runtime, bob_id, &map);
    assert!(
        ServerApp::mask_contains_point(
            &map,
            &visible_while_stealthed,
            alice_state.x,
            alice_state.y,
        ),
        "bob should have ordinary line-of-sight to alice by now, so stealth is the reason she is hidden"
    );
    assert!(
        ServerApp::arena_players_snapshot(&runtime, bob_id, &map, &visible_while_stealthed)
            .iter()
            .all(|player| player.player_id != alice_id),
        "stealth should hide alice from enemy snapshots even when she is otherwise visible"
    );

    runtime
        .world
        .queue_cast(bob_id, 4)
        .expect("reveal field should queue");
    let _ = runtime.world.tick(COMBAT_FRAME_MS);
    advance_world(&mut runtime.world, 10);
    assert!(
        runtime
            .world
            .statuses_for(alice_id)
            .unwrap_or_default()
            .iter()
            .any(|status| status.kind == game_content::StatusKind::Reveal),
        "the reveal field should apply reveal without relying on damage to break stealth in this test"
    );

    let (visible_after_reveal, _) = build_runtime_visibility_masks(&mut runtime, bob_id, &map);
    assert!(
        ServerApp::arena_players_snapshot(&runtime, bob_id, &map, &visible_after_reveal)
            .iter()
            .any(|player| player.player_id == alice_id),
        "revealed stealthed players should reappear in enemy arena snapshots"
    );

    remove_dir_if_exists(&root);
}
