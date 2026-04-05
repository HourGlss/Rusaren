#![allow(
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

use std::collections::{BTreeSet, VecDeque};

use game_content::{
    generate_template_match_map, parse_ascii_map, render_ascii_map, ArenaMapDefinition,
    ArenaMapObstacleKind, GameContent,
};

#[test]
fn generated_maps_preserve_the_template_contract_and_roundtrip_to_ascii() {
    let content = GameContent::bundled().expect("bundled content should load");
    let template = content
        .map_by_id("template_arena")
        .expect("template arena should exist");
    let generated = generate_template_match_map(
        template,
        &content.configuration().maps.generation,
        "sample_001",
        0x00C0_FFEE_u64,
    )
    .expect("map");

    assert_eq!(generated.width_tiles, template.width_tiles);
    assert_eq!(generated.height_tiles, template.height_tiles);
    assert_eq!(generated.tile_units, template.tile_units);
    assert_eq!(generated.objective_target_ms, template.objective_target_ms);
    assert_eq!(generated.footprint_mask, template.footprint_mask);
    assert_eq!(generated.objective_mask, template.objective_mask);
    assert_eq!(generated.team_a_anchors, template.team_a_anchors);
    assert_eq!(generated.team_b_anchors, template.team_b_anchors);
    assert_eq!(generated.features, template.features);
    assert_eq!(
        count_mask_bits(&generated.objective_mask),
        count_mask_bits(&template.objective_mask)
    );

    let rendered = render_ascii_map(&generated).expect("generated map should render");
    let mut reparsed = parse_ascii_map("sample_001.txt", &rendered, generated.tile_units)
        .expect("rendered map should parse");
    reparsed.objective_target_ms = generated.objective_target_ms;

    assert_eq!(reparsed.width_tiles, generated.width_tiles);
    assert_eq!(reparsed.height_tiles, generated.height_tiles);
    assert_eq!(reparsed.objective_target_ms, generated.objective_target_ms);
    assert_eq!(reparsed.footprint_mask, generated.footprint_mask);
    assert_eq!(reparsed.objective_mask, generated.objective_mask);
    assert_eq!(reparsed.team_a_anchors, generated.team_a_anchors);
    assert_eq!(reparsed.team_b_anchors, generated.team_b_anchors);
    assert_eq!(
        obstacle_signatures(&reparsed),
        obstacle_signatures(&generated)
    );
}

#[test]
fn generated_maps_keep_spawn_paths_open_and_preserve_diagonal_symmetry() {
    let content = GameContent::bundled().expect("bundled content should load");
    let template = content
        .map_by_id("template_arena")
        .expect("template arena should exist");

    for seed in 1..=24_u64 {
        let generated = generate_template_match_map(
            template,
            &content.configuration().maps.generation,
            format!("seed_{seed:03}"),
            seed,
        )
        .expect("map");
        assert!(
            all_anchors_reach_objective(&generated),
            "all anchors should have a route to the center on seed {seed}"
        );
        assert!(
            obstacles_are_diagonally_symmetric(&generated),
            "generated obstacle layout should remain diagonally symmetric on seed {seed}"
        );
    }
}

#[test]
fn generated_maps_include_short_contiguous_pillar_walls() {
    let content = GameContent::bundled().expect("bundled content should load");
    let template = content
        .map_by_id("template_arena")
        .expect("template arena should exist");

    for seed in 1..=24_u64 {
        let generated = generate_template_match_map(
            template,
            &content.configuration().maps.generation,
            format!("wall_seed_{seed:03}"),
            seed,
        )
        .expect("map");
        assert!(
            has_contiguous_pillar_wall(&generated),
            "generated map should include at least one 2-3 tile contiguous pillar wall on seed {seed}"
        );
    }
}

fn obstacle_signatures(map: &ArenaMapDefinition) -> BTreeSet<(u8, i16, i16)> {
    map.obstacles
        .iter()
        .map(|obstacle| {
            let kind = match obstacle.kind {
                ArenaMapObstacleKind::Pillar => 1_u8,
                ArenaMapObstacleKind::Shrub => 2_u8,
            };
            (kind, obstacle.center_x, obstacle.center_y)
        })
        .collect()
}

fn count_mask_bits(mask: &[u8]) -> u32 {
    mask.iter().map(|byte| byte.count_ones()).sum()
}

fn obstacles_are_diagonally_symmetric(map: &ArenaMapDefinition) -> bool {
    let obstacle_tiles = map
        .obstacles
        .iter()
        .map(|obstacle| {
            (
                match obstacle.kind {
                    ArenaMapObstacleKind::Pillar => 1_u8,
                    ArenaMapObstacleKind::Shrub => 2_u8,
                },
                tile_index_for_center(map, obstacle.center_x, obstacle.center_y),
            )
        })
        .collect::<BTreeSet<_>>();

    obstacle_tiles.iter().all(|(kind, index)| {
        let (column, row) = index_to_coord(map, *index);
        let (mirror_column, mirror_row) = mirror_coordinate_across_diagonal(map, column, row);
        let mirror_index = tile_index(map, mirror_column, mirror_row);
        obstacle_tiles.contains(&(*kind, mirror_index))
    })
}

fn all_anchors_reach_objective(map: &ArenaMapDefinition) -> bool {
    map.team_a_anchors
        .iter()
        .chain(map.team_b_anchors.iter())
        .all(|&(center_x, center_y)| {
            path_exists_to_objective(map, tile_index_for_center(map, center_x, center_y))
        })
}

fn path_exists_to_objective(map: &ArenaMapDefinition, start_index: usize) -> bool {
    let width = usize::from(map.width_tiles);
    let height = usize::from(map.height_tiles);
    let blocked = blocked_tiles(map);
    let mut visited = vec![false; width * height];
    let mut queue = VecDeque::from([start_index]);
    visited[start_index] = true;

    while let Some(index) = queue.pop_front() {
        if mask_has_tile(&map.objective_mask, index) {
            return true;
        }
        let row = index / width;
        let column = index % width;
        for (next_column, next_row) in neighbors(column, row, width, height) {
            let next_index = tile_index(map, next_column, next_row);
            if visited[next_index]
                || !mask_has_tile(&map.footprint_mask, next_index)
                || blocked.contains(&next_index)
            {
                continue;
            }
            visited[next_index] = true;
            queue.push_back(next_index);
        }
    }

    false
}

fn blocked_tiles(map: &ArenaMapDefinition) -> BTreeSet<usize> {
    map.obstacles
        .iter()
        .filter(|obstacle| obstacle.kind == ArenaMapObstacleKind::Pillar)
        .map(|obstacle| tile_index_for_center(map, obstacle.center_x, obstacle.center_y))
        .collect()
}

fn has_contiguous_pillar_wall(map: &ArenaMapDefinition) -> bool {
    let blocked = blocked_tiles(map);
    blocked.iter().any(|index| {
        let (column, row) = index_to_coord(map, *index);
        neighbors(
            column,
            row,
            usize::from(map.width_tiles),
            usize::from(map.height_tiles),
        )
        .into_iter()
        .any(|(next_column, next_row)| blocked.contains(&tile_index(map, next_column, next_row)))
    })
}

fn neighbors(column: usize, row: usize, width: usize, height: usize) -> Vec<(usize, usize)> {
    let mut result = Vec::with_capacity(4);
    if column > 0 {
        result.push((column - 1, row));
    }
    if column + 1 < width {
        result.push((column + 1, row));
    }
    if row > 0 {
        result.push((column, row - 1));
    }
    if row + 1 < height {
        result.push((column, row + 1));
    }
    result
}

fn mirror_coordinate_across_diagonal(
    map: &ArenaMapDefinition,
    column: usize,
    row: usize,
) -> (usize, usize) {
    let width = f64::from(map.width_tiles);
    let height = f64::from(map.height_tiles);
    let normalized_x = (column as f64 + 0.5) / width;
    let normalized_y = (row as f64 + 0.5) / height;
    let mirrored_x = 1.0 - normalized_y;
    let mirrored_y = 1.0 - normalized_x;
    let mirrored_column =
        (((mirrored_x * width) - 0.5).round() as i32).clamp(0, i32::from(map.width_tiles) - 1);
    let mirrored_row =
        (((mirrored_y * height) - 0.5).round() as i32).clamp(0, i32::from(map.height_tiles) - 1);
    (
        usize::try_from(mirrored_column).expect("column should remain in bounds"),
        usize::try_from(mirrored_row).expect("row should remain in bounds"),
    )
}

fn tile_index(map: &ArenaMapDefinition, column: usize, row: usize) -> usize {
    row * usize::from(map.width_tiles) + column
}

fn index_to_coord(map: &ArenaMapDefinition, index: usize) -> (usize, usize) {
    let width = usize::from(map.width_tiles);
    (index % width, index / width)
}

fn mask_has_tile(mask: &[u8], index: usize) -> bool {
    let byte_index = index / 8;
    let bit_index = index % 8;
    mask.get(byte_index)
        .is_some_and(|byte| (byte & (1_u8 << bit_index)) != 0)
}

fn tile_index_for_center(map: &ArenaMapDefinition, center_x: i16, center_y: i16) -> usize {
    let tile_units = i32::from(map.tile_units);
    let x = i32::from(center_x) + i32::from(map.width_units) / 2 - tile_units / 2;
    let y = i32::from(center_y) + i32::from(map.height_units) / 2 - tile_units / 2;
    let column = usize::try_from(x / tile_units).expect("column should stay in bounds");
    let row = usize::try_from(y / tile_units).expect("row should stay in bounds");
    tile_index(map, column, row)
}
