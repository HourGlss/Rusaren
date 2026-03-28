use super::*;

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.000_1,
        "expected {expected}, got {actual}"
    );
}

fn pillar() -> ArenaObstacle {
    ArenaObstacle {
        kind: ArenaObstacleKind::Pillar,
        center_x: 0,
        center_y: 0,
        half_width: 50,
        half_height: 50,
    }
}

fn shrub() -> ArenaObstacle {
    ArenaObstacle {
        kind: ArenaObstacleKind::Shrub,
        ..pillar()
    }
}

fn barrier() -> ArenaObstacle {
    ArenaObstacle {
        kind: ArenaObstacleKind::Barrier,
        ..pillar()
    }
}

#[test]
fn obstacle_kind_rules_and_point_containment_are_exact() {
    let pillar = pillar();
    let shrub = shrub();
    let barrier = barrier();

    assert!(obstacle_blocks_movement(&pillar));
    assert!(obstacle_blocks_projectiles(&pillar));
    assert!(obstacle_blocks_vision(&pillar));
    assert!(!obstacle_blocks_movement(&shrub));
    assert!(!obstacle_blocks_projectiles(&shrub));
    assert!(obstacle_blocks_vision(&shrub));
    assert!(obstacle_blocks_movement(&barrier));
    assert!(obstacle_blocks_projectiles(&barrier));
    assert!(!obstacle_blocks_vision(&barrier));

    assert!(obstacle_contains_point(0, 0, &pillar));
    assert!(obstacle_contains_point(-50, 0, &pillar));
    assert!(obstacle_contains_point(50, 0, &pillar));
    assert!(obstacle_contains_point(0, -50, &pillar));
    assert!(obstacle_contains_point(0, 50, &pillar));
    assert!(!obstacle_contains_point(-51, 0, &pillar));
    assert!(!obstacle_contains_point(51, 0, &pillar));
    assert!(!obstacle_contains_point(0, -51, &pillar));
    assert!(!obstacle_contains_point(0, 51, &pillar));
}

#[test]
fn circle_and_segment_geometry_handle_edges_reverse_paths_and_inside_cases() {
    let pillar = pillar();

    assert!(circle_intersects_rect(80, 0, 30, &pillar));
    assert!(!circle_intersects_rect(81, 0, 30, &pillar));
    assert!(circle_intersects_rect(0, 80, 30, &pillar));
    assert!(!circle_intersects_rect(0, 81, 30, &pillar));
    assert!(circle_intersects_rect(80, 80, 43, &pillar));
    assert!(!circle_intersects_rect(80, 80, 42, &pillar));

    assert_eq!(
        segment_rect_intersection_t((-100, 0), (100, 0), &pillar),
        Some(0.25)
    );
    assert_eq!(
        segment_rect_intersection_t((100, 0), (-100, 0), &pillar),
        Some(0.25)
    );
    assert_eq!(
        segment_rect_intersection_t((0, -100), (0, 100), &pillar),
        Some(0.25)
    );
    assert_eq!(
        segment_rect_intersection_t((-100, 120), (100, 120), &pillar),
        None
    );
    assert_eq!(segment_rect_intersection_t((0, 0), (100, 0), &pillar), None);

    assert!(segment_hits_obstacle((-100, 0), (100, 0), &pillar));
    assert!(segment_hits_obstacle((100, 0), (-100, 0), &pillar));
    assert!(!segment_hits_obstacle((-100, 120), (100, 120), &pillar));
}

#[test]
fn truncation_chooses_the_nearest_blocker_and_ignores_shrubs() {
    let near_pillar = pillar();
    let far_pillar = ArenaObstacle {
        center_x: 180,
        ..pillar()
    };
    let shrub = shrub();

    assert_eq!(
        truncate_line_to_obstacles((-100, 0), (100, 0), &[near_pillar]),
        (-52, 0)
    );
    assert_eq!(
        truncate_line_to_obstacles((-100, 120), (100, 120), &[near_pillar]),
        (100, 120)
    );
    assert_eq!(
        truncate_line_to_obstacles((-100, 0), (100, 0), &[shrub]),
        (100, 0)
    );
    assert_eq!(
        truncate_line_to_obstacles((-200, 0), (250, 0), &[far_pillar, near_pillar]),
        (-54, 0)
    );
    assert_eq!(
        truncate_line_to_obstacles((0, -100), (0, 100), &[near_pillar]),
        (0, -52)
    );
}

#[test]
fn segment_slab_updates_cover_positive_negative_and_parallel_segments() {
    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;
    assert!(update_segment_slab(
        -100.0, 200.0, -50.0, 50.0, &mut t_min, &mut t_max
    ));
    assert_close(t_min, 0.25);
    assert_close(t_max, 0.75);

    let mut reverse_t_min = 0.0_f32;
    let mut reverse_t_max = 1.0_f32;
    assert!(update_segment_slab(
        100.0,
        -200.0,
        -50.0,
        50.0,
        &mut reverse_t_min,
        &mut reverse_t_max,
    ));
    assert_close(reverse_t_min, 0.25);
    assert_close(reverse_t_max, 0.75);

    let mut parallel_t_min = 0.0_f32;
    let mut parallel_t_max = 1.0_f32;
    assert!(update_segment_slab(
        0.0,
        0.0,
        -50.0,
        50.0,
        &mut parallel_t_min,
        &mut parallel_t_max,
    ));
    assert_close(parallel_t_min, 0.0);
    assert_close(parallel_t_max, 1.0);

    let mut miss_t_min = 0.0_f32;
    let mut miss_t_max = 1.0_f32;
    assert!(!update_segment_slab(
        -100.0,
        0.0,
        -50.0,
        50.0,
        &mut miss_t_min,
        &mut miss_t_max,
    ));
}

#[test]
fn aim_projection_and_distance_helpers_return_exact_values() {
    assert_eq!(project_from_aim(10, 20, 0, 0, 30), (40, 20));
    assert_eq!(project_from_aim(0, 0, 3, 4, 50), (30, 40));

    let direction = normalize_aim(3, 4);
    assert_close(direction.0, 0.6);
    assert_close(direction.1, 0.8);

    let zero_direction = normalize_aim(0, 0);
    assert_close(zero_direction.0, 1.0);
    assert_close(zero_direction.1, 0.0);

    assert_eq!(point_distance_sq((0, 0), (3, 4)), 25);
    assert_eq!(point_distance_units((0, 0), (3, 4)), 5);
    assert_eq!(point_distance_units((0, 0), (0, 0)), 0);

    assert_close(segment_distance_sq((0, 0), (10, 0), (5, 5)), 25.0);
    assert_close(segment_distance_sq((0, 0), (10, 0), (-5, 0)), 25.0);
    assert_close(segment_distance_sq((0, 0), (10, 0), (15, 0)), 25.0);
    assert_close(segment_distance_sq((0, 0), (0, 0), (3, 4)), 25.0);
    assert_close(segment_distance_sq((0, 0), (0, 10), (5, 5)), 25.0);
    assert_close(segment_distance_sq((0, 0), (10, 10), (10, 0)), 50.0);
}

#[test]
fn rounding_and_saturation_helpers_clamp_safely() {
    assert_eq!(round_f32_to_i32(1.49), 1);
    assert_eq!(round_f32_to_i32(1.5), 2);
    assert_eq!(round_f32_to_i32(-1.5), -2);

    assert_eq!(saturating_i16(123), 123);
    assert_eq!(saturating_i16(i32::from(i16::MAX) + 1), i16::MAX);
    assert_eq!(saturating_i16(i32::from(i16::MIN) - 1), i16::MIN);
}
