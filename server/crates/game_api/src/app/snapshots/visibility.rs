use game_content::ArenaMapDefinition;
use game_domain::PlayerId;
use game_sim::{
    obstacle_blocks_vision, obstacle_contains_point, segment_hits_obstacle, ArenaObstacle,
    ArenaObstacleKind as SimArenaObstacleKind, VISION_RADIUS_UNITS,
};

use super::super::{MatchRuntime, ServerApp};

impl ServerApp {
    pub(in super::super) fn build_visibility_masks(
        runtime: &mut MatchRuntime,
        viewer_id: PlayerId,
        map: &ArenaMapDefinition,
    ) -> Option<(Vec<u8>, Vec<u8>)> {
        let viewer_state = runtime.world.player_state(viewer_id)?;
        let mut visible_tiles = Self::blank_visibility_mask(map);
        let viewer_position = (viewer_state.x, viewer_state.y);
        let mut vision_sources = vec![(viewer_position, VISION_RADIUS_UNITS)];
        for deployable in runtime.world.deployables() {
            if deployable.team == viewer_state.team
                && deployable.kind == game_sim::ArenaDeployableKind::Ward
            {
                vision_sources.push(((deployable.x, deployable.y), deployable.radius));
            }
        }
        if let Some(tile_index) = Self::tile_index_for_point(map, viewer_state.x, viewer_state.y) {
            Self::set_mask_bit(&mut visible_tiles, tile_index);
        }

        for row in 0..usize::from(map.height_tiles) {
            for column in 0..usize::from(map.width_tiles) {
                let tile_center = Self::tile_center_units(map, column, row);
                if vision_sources
                    .iter()
                    .any(|(source_position, vision_radius)| {
                        Self::point_is_visible_from_source(
                            *source_position,
                            tile_center,
                            runtime.world.obstacles(),
                            *vision_radius,
                        )
                    })
                {
                    let tile_index = row * usize::from(map.width_tiles) + column;
                    Self::set_mask_bit(&mut visible_tiles, tile_index);
                }
            }
        }

        let explored_tiles = runtime
            .explored_tiles
            .entry(viewer_id)
            .or_insert_with(|| Self::blank_visibility_mask(map));
        if explored_tiles.len() != visible_tiles.len() {
            *explored_tiles = Self::blank_visibility_mask(map);
        }
        for (explored, visible) in explored_tiles.iter_mut().zip(&visible_tiles) {
            *explored |= *visible;
        }

        Some((visible_tiles, explored_tiles.clone()))
    }

    fn point_is_visible_from_source(
        viewer_position: (i16, i16),
        target_position: (i16, i16),
        obstacles: &[ArenaObstacle],
        vision_radius_units: u16,
    ) -> bool {
        let delta_x = i32::from(target_position.0) - i32::from(viewer_position.0);
        let delta_y = i32::from(target_position.1) - i32::from(viewer_position.1);
        let radius_sq = i32::from(vision_radius_units) * i32::from(vision_radius_units);
        if delta_x.saturating_mul(delta_x) + delta_y.saturating_mul(delta_y) > radius_sq {
            return false;
        }

        let viewer_shrub = Self::containing_shrub(obstacles, viewer_position.0, viewer_position.1);
        let target_shrub = Self::containing_shrub(obstacles, target_position.0, target_position.1);
        if (viewer_shrub.is_some() || target_shrub.is_some()) && viewer_shrub != target_shrub {
            return false;
        }

        for obstacle in obstacles
            .iter()
            .filter(|obstacle| obstacle_blocks_vision(obstacle))
        {
            if viewer_shrub.is_some()
                && viewer_shrub == target_shrub
                && Some(*obstacle) == viewer_shrub
            {
                continue;
            }
            if segment_hits_obstacle(viewer_position, target_position, obstacle) {
                return false;
            }
        }

        true
    }

    #[cfg(test)]
    pub(in super::super) fn point_is_visible_to_viewer(
        viewer_position: (i16, i16),
        target_position: (i16, i16),
        obstacles: &[ArenaObstacle],
    ) -> bool {
        Self::point_is_visible_from_source(
            viewer_position,
            target_position,
            obstacles,
            VISION_RADIUS_UNITS,
        )
    }

    pub(in super::super) fn containing_shrub(
        obstacles: &[ArenaObstacle],
        x: i16,
        y: i16,
    ) -> Option<ArenaObstacle> {
        obstacles.iter().copied().find(|obstacle| {
            obstacle.kind == SimArenaObstacleKind::Shrub && obstacle_contains_point(x, y, obstacle)
        })
    }

    pub(in super::super) fn blank_visibility_mask(map: &ArenaMapDefinition) -> Vec<u8> {
        vec![0_u8; Self::visibility_mask_len(map)]
    }

    pub(in super::super) fn visibility_mask_len(map: &ArenaMapDefinition) -> usize {
        (usize::from(map.width_tiles) * usize::from(map.height_tiles)).div_ceil(8)
    }

    pub(in super::super) fn set_mask_bit(mask: &mut [u8], index: usize) {
        let byte_index = index / 8;
        let bit_index = index % 8;
        if let Some(byte) = mask.get_mut(byte_index) {
            *byte |= 1_u8 << bit_index;
        }
    }

    pub(in super::super) fn mask_has_tile(mask: &[u8], index: usize) -> bool {
        let byte_index = index / 8;
        let bit_index = index % 8;
        mask.get(byte_index)
            .is_some_and(|byte| (byte & (1_u8 << bit_index)) != 0)
    }

    pub(in super::super) fn mask_contains_point(
        map: &ArenaMapDefinition,
        mask: &[u8],
        x: i16,
        y: i16,
    ) -> bool {
        let Some(index) = Self::tile_index_for_point(map, x, y) else {
            return false;
        };
        Self::mask_has_tile(mask, index)
    }

    pub(in super::super) fn mask_intersects_obstacle(
        map: &ArenaMapDefinition,
        mask: &[u8],
        obstacle: &ArenaObstacle,
    ) -> bool {
        let tile_units = i32::from(map.tile_units);
        if tile_units <= 0 {
            return false;
        }

        let half_map_width = i32::from(map.width_units) / 2;
        let half_map_height = i32::from(map.height_units) / 2;
        let width_tiles = i32::from(map.width_tiles);
        let height_tiles = i32::from(map.height_tiles);
        let min_world_x = i32::from(obstacle.center_x) - i32::from(obstacle.half_width);
        let max_world_x = i32::from(obstacle.center_x) + i32::from(obstacle.half_width);
        let min_world_y = i32::from(obstacle.center_y) - i32::from(obstacle.half_height);
        let max_world_y = i32::from(obstacle.center_y) + i32::from(obstacle.half_height);
        let min_column = ((min_world_x + half_map_width) / tile_units).clamp(0, width_tiles - 1);
        let max_column = ((max_world_x + half_map_width) / tile_units).clamp(0, width_tiles - 1);
        let min_row = ((min_world_y + half_map_height) / tile_units).clamp(0, height_tiles - 1);
        let max_row = ((max_world_y + half_map_height) / tile_units).clamp(0, height_tiles - 1);

        for row in min_row..=max_row {
            for column in min_column..=max_column {
                let tile_index = usize::try_from(row * width_tiles + column).unwrap_or(usize::MAX);
                if tile_index != usize::MAX && Self::mask_has_tile(mask, tile_index) {
                    return true;
                }
            }
        }

        false
    }

    pub(in super::super) fn tile_index_for_point(
        map: &ArenaMapDefinition,
        x: i16,
        y: i16,
    ) -> Option<usize> {
        let tile_units = i32::from(map.tile_units);
        if tile_units <= 0 {
            return None;
        }
        let half_width_units = i32::from(map.width_units) / 2;
        let half_height_units = i32::from(map.height_units) / 2;
        let relative_x = i32::from(x) + half_width_units;
        let relative_y = i32::from(y) + half_height_units;
        if relative_x < 0
            || relative_y < 0
            || relative_x >= i32::from(map.width_units)
            || relative_y >= i32::from(map.height_units)
        {
            return None;
        }
        let column = usize::try_from(relative_x / tile_units).ok()?;
        let row = usize::try_from(relative_y / tile_units).ok()?;
        Some(row * usize::from(map.width_tiles) + column)
    }

    pub(in super::super) fn tile_center_units(
        map: &ArenaMapDefinition,
        column: usize,
        row: usize,
    ) -> (i16, i16) {
        let half_width_units = i32::from(map.width_units) / 2;
        let half_height_units = i32::from(map.height_units) / 2;
        let tile_units = i32::from(map.tile_units);
        let center_x = -half_width_units
            + i32::try_from(column).unwrap_or(i32::MAX) * tile_units
            + tile_units / 2;
        let center_y = -half_height_units
            + i32::try_from(row).unwrap_or(i32::MAX) * tile_units
            + tile_units / 2;
        (
            i16::try_from(center_x).unwrap_or(i16::MAX),
            i16::try_from(center_y).unwrap_or(i16::MAX),
        )
    }
}
