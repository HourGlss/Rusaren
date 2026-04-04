use std::collections::{BTreeSet, VecDeque};

use super::{
    ArenaMapDefinition, ArenaMapFeatureKind, ArenaMapObstacle, ArenaMapObstacleKind, ContentError,
};

const MAX_GENERATION_ATTEMPTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GenerationStyle {
    shrub_clusters: usize,
    shrub_radius_tiles: i32,
    shrub_soft_radius_tiles: i32,
    shrub_fill_percent: u8,
    pillar_pairs: usize,
}

pub fn generate_template_match_map(
    template: &ArenaMapDefinition,
    map_id: impl Into<String>,
    seed: u64,
) -> Result<ArenaMapDefinition, ContentError> {
    let map_id = map_id.into();
    let source = format!("generated map {map_id}");
    if count_set_bits(&template.objective_mask) == 0 {
        return Err(ContentError::Validation {
            source,
            message: String::from("template map must contain at least one objective tile 'X'"),
        });
    }

    let protected_tiles = protected_tiles(template)?;
    let candidate_orbits = collect_candidate_orbits(template, &protected_tiles);
    if candidate_orbits.is_empty() {
        return Err(ContentError::Validation {
            source,
            message: String::from("template map does not leave any tiles available for generation"),
        });
    }

    let mut rng = MapRng::new(seed);
    let style = pick_generation_style(&mut rng);
    for _attempt in 0..MAX_GENERATION_ATTEMPTS {
        let mut orbit_order = candidate_orbits.clone();
        shuffle(&mut orbit_order, &mut rng);
        let layout = build_obstacle_layout(template, &orbit_order, style, &mut rng);
        if all_spawn_anchors_reach_objective(template, &layout) {
            return Ok(ArenaMapDefinition {
                map_id: map_id.clone(),
                width_tiles: template.width_tiles,
                height_tiles: template.height_tiles,
                tile_units: template.tile_units,
                width_units: template.width_units,
                height_units: template.height_units,
                footprint_mask: template.footprint_mask.clone(),
                objective_mask: template.objective_mask.clone(),
                team_a_anchors: template.team_a_anchors.clone(),
                team_b_anchors: template.team_b_anchors.clone(),
                obstacles: obstacle_layout_to_map_obstacles(template, &layout),
                features: template.features.clone(),
            });
        }
    }

    Err(ContentError::Validation {
        source,
        message: String::from(
            "generator could not find a symmetric obstacle layout that preserves center access",
        ),
    })
}

pub fn render_ascii_map(map: &ArenaMapDefinition) -> Result<String, ContentError> {
    let width = usize::from(map.width_tiles);
    let height = usize::from(map.height_tiles);
    let mut glyphs = vec![vec![' '; width]; height];

    for row in 0..height {
        for column in 0..width {
            let index = tile_index(width, column, row);
            if mask_has_tile(&map.footprint_mask, index) {
                glyphs[row][column] = '.';
            }
        }
    }

    for obstacle in &map.obstacles {
        let index = tile_index_for_center(map, obstacle.center_x, obstacle.center_y)?;
        let row = index / width;
        let column = index % width;
        glyphs[row][column] = match obstacle.kind {
            ArenaMapObstacleKind::Pillar => '#',
            ArenaMapObstacleKind::Shrub => '+',
        };
    }

    for feature in &map.features {
        let index = tile_index_for_center(map, feature.center_x, feature.center_y)?;
        let row = index / width;
        let column = index % width;
        glyphs[row][column] = match feature.kind {
            ArenaMapFeatureKind::TrainingDummyResetFull => 'd',
            ArenaMapFeatureKind::TrainingDummyExecute => 'D',
        };
    }

    for index in objective_indices(map) {
        let row = index / width;
        let column = index % width;
        glyphs[row][column] = 'X';
    }

    for &(center_x, center_y) in &map.team_a_anchors {
        let index = tile_index_for_center(map, center_x, center_y)?;
        let row = index / width;
        let column = index % width;
        glyphs[row][column] = 'A';
    }
    for &(center_x, center_y) in &map.team_b_anchors {
        let index = tile_index_for_center(map, center_x, center_y)?;
        let row = index / width;
        let column = index % width;
        glyphs[row][column] = 'B';
    }

    Ok(glyphs
        .into_iter()
        .map(|row| row.into_iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n"))
}

fn pick_generation_style(rng: &mut MapRng) -> GenerationStyle {
    match rng.next_u32() % 5 {
        0 => GenerationStyle {
            shrub_clusters: 1,
            shrub_radius_tiles: 1,
            shrub_soft_radius_tiles: 2,
            shrub_fill_percent: 16,
            pillar_pairs: 2,
        },
        1 => GenerationStyle {
            shrub_clusters: 2,
            shrub_radius_tiles: 2,
            shrub_soft_radius_tiles: 3,
            shrub_fill_percent: 24,
            pillar_pairs: 3,
        },
        2 => GenerationStyle {
            shrub_clusters: 3,
            shrub_radius_tiles: 2,
            shrub_soft_radius_tiles: 4,
            shrub_fill_percent: 38,
            pillar_pairs: 3,
        },
        3 => GenerationStyle {
            shrub_clusters: 3,
            shrub_radius_tiles: 3,
            shrub_soft_radius_tiles: 5,
            shrub_fill_percent: 50,
            pillar_pairs: 4,
        },
        _ => GenerationStyle {
            shrub_clusters: 4,
            shrub_radius_tiles: 3,
            shrub_soft_radius_tiles: 5,
            shrub_fill_percent: 62,
            pillar_pairs: 5,
        },
    }
}

fn build_obstacle_layout(
    template: &ArenaMapDefinition,
    orbit_order: &[Vec<usize>],
    style: GenerationStyle,
    rng: &mut MapRng,
) -> Vec<Option<ArenaMapObstacleKind>> {
    let tile_count = tile_count(template);
    let mut layout = vec![None; tile_count];
    let cluster_count = style.shrub_clusters.min(orbit_order.len());
    let cluster_centers = orbit_order
        .iter()
        .take(cluster_count)
        .map(|orbit| index_to_coord(template, orbit[0]))
        .collect::<Vec<_>>();

    let mut pillar_representatives = Vec::new();
    let mut pillar_pairs_remaining = style.pillar_pairs;
    for orbit in orbit_order {
        if pillar_pairs_remaining == 0 {
            break;
        }
        let representative = index_to_coord(template, orbit[0]);
        if !orbit_is_pillar_safe(template, orbit)
            || pillar_representatives
                .iter()
                .any(|existing| manhattan_distance(representative, *existing) <= 3)
            || rng.roll_percent(55)
        {
            continue;
        }
        for &index in orbit {
            layout[index] = Some(ArenaMapObstacleKind::Pillar);
        }
        pillar_representatives.push(representative);
        pillar_pairs_remaining = pillar_pairs_remaining.saturating_sub(1);
    }

    for orbit in orbit_order {
        if orbit
            .iter()
            .any(|&index| matches!(layout[index], Some(ArenaMapObstacleKind::Pillar)))
        {
            continue;
        }
        let representative = index_to_coord(template, orbit[0]);
        let shrub_probability = cluster_centers
            .iter()
            .map(|center| manhattan_distance(representative, *center))
            .min()
            .map(|distance| {
                if distance <= style.shrub_radius_tiles {
                    90
                } else if distance <= style.shrub_soft_radius_tiles {
                    style.shrub_fill_percent
                } else {
                    style.shrub_fill_percent / 4
                }
            })
            .unwrap_or(style.shrub_fill_percent / 4);
        if !rng.roll_percent(shrub_probability) {
            continue;
        }
        for &index in orbit {
            layout[index] = Some(ArenaMapObstacleKind::Shrub);
        }
    }

    layout
}

fn collect_candidate_orbits(
    template: &ArenaMapDefinition,
    protected_tiles: &[bool],
) -> Vec<Vec<usize>> {
    let width = usize::from(template.width_tiles);
    let height = usize::from(template.height_tiles);
    let mut seen = BTreeSet::new();
    let mut orbits = Vec::new();
    for row in 0..height {
        for column in 0..width {
            let index = tile_index(width, column, row);
            if seen.contains(&index)
                || !mask_has_tile(&template.footprint_mask, index)
                || protected_tiles[index]
            {
                continue;
            }

            let orbit = mirror_orbit(template, column, row);
            for orbit_index in &orbit {
                seen.insert(*orbit_index);
            }
            if orbit.iter().all(|&orbit_index| {
                mask_has_tile(&template.footprint_mask, orbit_index)
                    && !protected_tiles[orbit_index]
            }) {
                orbits.push(orbit);
            }
        }
    }
    orbits
}

fn protected_tiles(template: &ArenaMapDefinition) -> Result<Vec<bool>, ContentError> {
    let mut protected = vec![false; tile_count(template)];
    for index in objective_indices(template) {
        mark_with_buffer(template, &mut protected, index, 1);
    }
    for &(center_x, center_y) in template
        .team_a_anchors
        .iter()
        .chain(template.team_b_anchors.iter())
    {
        let index = tile_index_for_center(template, center_x, center_y)?;
        mark_with_buffer(template, &mut protected, index, 1);
    }
    for feature in &template.features {
        let index = tile_index_for_center(template, feature.center_x, feature.center_y)?;
        mark_with_buffer(template, &mut protected, index, 1);
    }
    Ok(protected)
}

fn objective_indices(template: &ArenaMapDefinition) -> Vec<usize> {
    (0..tile_count(template))
        .filter(|index| mask_has_tile(&template.objective_mask, *index))
        .collect()
}

fn mark_with_buffer(
    template: &ArenaMapDefinition,
    protected: &mut [bool],
    center_index: usize,
    radius: i32,
) {
    let (center_column, center_row) = index_to_coord(template, center_index);
    let width = i32::from(template.width_tiles);
    let height = i32::from(template.height_tiles);
    for row in (center_row - radius)..=(center_row + radius) {
        for column in (center_column - radius)..=(center_column + radius) {
            if row < 0 || column < 0 || row >= height || column >= width {
                continue;
            }
            let index = tile_index(
                usize::from(template.width_tiles),
                usize::try_from(column).unwrap_or(0),
                usize::try_from(row).unwrap_or(0),
            );
            protected[index] = true;
        }
    }
}

fn mirror_orbit(template: &ArenaMapDefinition, column: usize, row: usize) -> Vec<usize> {
    let mut orbit = BTreeSet::new();
    let mut current = (column, row);
    for _ in 0..8 {
        let index = tile_index(usize::from(template.width_tiles), current.0, current.1);
        if !orbit.insert(index) {
            break;
        }
        current = mirror_coordinate_across_diagonal(template, current.0, current.1);
    }
    orbit.into_iter().collect()
}

fn mirror_coordinate_across_diagonal(
    template: &ArenaMapDefinition,
    column: usize,
    row: usize,
) -> (usize, usize) {
    let width = f64::from(template.width_tiles);
    let height = f64::from(template.height_tiles);
    let normalized_x = (column as f64 + 0.5) / width;
    let normalized_y = (row as f64 + 0.5) / height;
    let mirrored_x = 1.0 - normalized_y;
    let mirrored_y = 1.0 - normalized_x;
    let mirrored_column =
        (((mirrored_x * width) - 0.5).round() as i32).clamp(0, i32::from(template.width_tiles) - 1);
    let mirrored_row = (((mirrored_y * height) - 0.5).round() as i32)
        .clamp(0, i32::from(template.height_tiles) - 1);
    (
        usize::try_from(mirrored_column).unwrap_or(0),
        usize::try_from(mirrored_row).unwrap_or(0),
    )
}

fn orbit_is_pillar_safe(template: &ArenaMapDefinition, orbit: &[usize]) -> bool {
    orbit.iter().all(|&index| {
        let (column, row) = index_to_coord(template, index);
        let edge_distance = i32::min(
            i32::min(column, i32::from(template.width_tiles) - 1 - column),
            i32::min(row, i32::from(template.height_tiles) - 1 - row),
        );
        edge_distance >= 1
    })
}

fn all_spawn_anchors_reach_objective(
    template: &ArenaMapDefinition,
    layout: &[Option<ArenaMapObstacleKind>],
) -> bool {
    let Some(objective_goal) = first_objective_index(template) else {
        return false;
    };
    template
        .team_a_anchors
        .iter()
        .chain(template.team_b_anchors.iter())
        .copied()
        .all(|anchor| {
            tile_index_for_center(template, anchor.0, anchor.1)
                .ok()
                .is_some_and(|start_index| {
                    path_exists_to_objective(template, layout, start_index, objective_goal)
                })
        })
}

fn first_objective_index(template: &ArenaMapDefinition) -> Option<usize> {
    (0..tile_count(template)).find(|index| mask_has_tile(&template.objective_mask, *index))
}

fn path_exists_to_objective(
    template: &ArenaMapDefinition,
    layout: &[Option<ArenaMapObstacleKind>],
    start_index: usize,
    _goal_hint: usize,
) -> bool {
    let width = usize::from(template.width_tiles);
    let height = usize::from(template.height_tiles);
    let mut visited = vec![false; tile_count(template)];
    let mut queue = VecDeque::from([start_index]);
    visited[start_index] = true;

    while let Some(index) = queue.pop_front() {
        if mask_has_tile(&template.objective_mask, index) {
            return true;
        }
        let row = index / width;
        let column = index % width;
        for (next_column, next_row) in neighbors(column, row, width, height) {
            let next_index = tile_index(width, next_column, next_row);
            if visited[next_index]
                || !mask_has_tile(&template.footprint_mask, next_index)
                || matches!(layout[next_index], Some(ArenaMapObstacleKind::Pillar))
            {
                continue;
            }
            visited[next_index] = true;
            queue.push_back(next_index);
        }
    }

    false
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

fn obstacle_layout_to_map_obstacles(
    template: &ArenaMapDefinition,
    layout: &[Option<ArenaMapObstacleKind>],
) -> Vec<ArenaMapObstacle> {
    layout
        .iter()
        .enumerate()
        .filter_map(|(index, kind)| {
            kind.map(|obstacle_kind| {
                let (column, row) = index_to_coord(template, index);
                let (center_x, center_y) = tile_center_units(template, column, row);
                ArenaMapObstacle {
                    kind: obstacle_kind,
                    center_x,
                    center_y,
                    half_width: template.tile_units / 2,
                    half_height: template.tile_units / 2,
                }
            })
        })
        .collect()
}

fn tile_index_for_center(
    map: &ArenaMapDefinition,
    center_x: i16,
    center_y: i16,
) -> Result<usize, ContentError> {
    let tile_units = i32::from(map.tile_units);
    let x = i32::from(center_x) + i32::from(map.width_units) / 2 - tile_units / 2;
    let y = i32::from(center_y) + i32::from(map.height_units) / 2 - tile_units / 2;
    if x < 0
        || y < 0
        || x % tile_units != 0
        || y % tile_units != 0
        || x >= i32::from(map.width_units)
        || y >= i32::from(map.height_units)
    {
        return Err(ContentError::Validation {
            source: map.map_id.clone(),
            message: format!("coordinate ({center_x}, {center_y}) does not land on a map tile"),
        });
    }
    let column = usize::try_from(x / tile_units).unwrap_or(0);
    let row = usize::try_from(y / tile_units).unwrap_or(0);
    Ok(tile_index(usize::from(map.width_tiles), column, row))
}

fn tile_center_units(map: &ArenaMapDefinition, column: i32, row: i32) -> (i16, i16) {
    let tile_units = i32::from(map.tile_units);
    let center_x = -i32::from(map.width_units) / 2 + column * tile_units + tile_units / 2;
    let center_y = -i32::from(map.height_units) / 2 + row * tile_units + tile_units / 2;
    (
        i16::try_from(center_x).unwrap_or(i16::MAX),
        i16::try_from(center_y).unwrap_or(i16::MAX),
    )
}

fn index_to_coord(map: &ArenaMapDefinition, index: usize) -> (i32, i32) {
    let width = usize::from(map.width_tiles);
    (
        i32::try_from(index % width).unwrap_or(0),
        i32::try_from(index / width).unwrap_or(0),
    )
}

fn manhattan_distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

fn tile_count(map: &ArenaMapDefinition) -> usize {
    usize::from(map.width_tiles) * usize::from(map.height_tiles)
}

fn tile_index(width: usize, column: usize, row: usize) -> usize {
    row * width + column
}

fn count_set_bits(mask: &[u8]) -> usize {
    mask.iter().map(|byte| byte.count_ones() as usize).sum()
}

fn mask_has_tile(mask: &[u8], index: usize) -> bool {
    let byte_index = index / 8;
    let bit_index = index % 8;
    mask.get(byte_index)
        .is_some_and(|byte| (byte & (1_u8 << bit_index)) != 0)
}

struct MapRng {
    state: u64,
}

impl MapRng {
    fn new(seed: u64) -> Self {
        let state = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self { state }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    fn roll_percent(&mut self, percent: u8) -> bool {
        (self.next_u32() % 100) < u32::from(percent)
    }
}

fn shuffle<T>(values: &mut [T], rng: &mut MapRng) {
    for index in (1..values.len()).rev() {
        let swap_index =
            usize::try_from(rng.next_u64() % u64::try_from(index + 1).unwrap_or(1)).unwrap_or(0);
        values.swap(index, swap_index);
    }
}
