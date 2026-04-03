use game_content::{
    ArenaMapDefinition, ArenaMapObstacle, ArenaMapObstacleKind, SkillEffectKind, StatusKind,
};
use game_domain::TeamSide;

use crate::geometry::{circle_intersects_rect, round_f32_to_i32, saturating_i16};
use crate::{
    obstacle_blocks_movement, ArenaEffectKind, ArenaObstacle, ArenaObstacleKind, MovementIntent,
};

use super::{
    StatusInstance, DEFAULT_AIM_X, PLAYER_MOVE_SPEED_UNITS_PER_SECOND, PLAYER_RADIUS_UNITS,
    SPAWN_SPACING_UNITS,
};

pub(crate) fn spawn_position(
    team: TeamSide,
    index: u16,
    map: &ArenaMapDefinition,
) -> (i16, i16, i16) {
    let anchors = match team {
        TeamSide::TeamA => &map.team_a_anchors,
        TeamSide::TeamB => &map.team_b_anchors,
    };
    let anchor_count = u16::try_from(anchors.len()).unwrap_or(1).max(1);
    let anchor_index = usize::from(index % anchor_count);
    let lane_index = index / anchor_count;
    let vertical_offset = i16::try_from(lane_index).unwrap_or(i16::MAX) * SPAWN_SPACING_UNITS;
    let anchor = anchors[anchor_index];
    match team {
        TeamSide::TeamA => (anchor.0, anchor.1 + vertical_offset, DEFAULT_AIM_X),
        TeamSide::TeamB => (anchor.0, anchor.1 + vertical_offset, -DEFAULT_AIM_X),
    }
}

pub(crate) fn map_obstacle_to_sim_obstacle(obstacle: &ArenaMapObstacle) -> ArenaObstacle {
    ArenaObstacle {
        kind: match obstacle.kind {
            ArenaMapObstacleKind::Pillar => ArenaObstacleKind::Pillar,
            ArenaMapObstacleKind::Shrub => ArenaObstacleKind::Shrub,
        },
        center_x: obstacle.center_x,
        center_y: obstacle.center_y,
        half_width: obstacle.half_width,
        half_height: obstacle.half_height,
    }
}

pub(crate) fn arena_effect_kind(kind: SkillEffectKind) -> ArenaEffectKind {
    match kind {
        SkillEffectKind::MeleeSwing => ArenaEffectKind::MeleeSwing,
        SkillEffectKind::SkillShot => ArenaEffectKind::SkillShot,
        SkillEffectKind::DashTrail => ArenaEffectKind::DashTrail,
        SkillEffectKind::Burst => ArenaEffectKind::Burst,
        SkillEffectKind::Nova => ArenaEffectKind::Nova,
        SkillEffectKind::Beam => ArenaEffectKind::Beam,
        SkillEffectKind::HitSpark => ArenaEffectKind::HitSpark,
    }
}

pub(crate) fn total_slow_bps(statuses: &[StatusInstance]) -> u16 {
    statuses
        .iter()
        .filter(|status| status.kind == StatusKind::Chill)
        .fold(0_u16, |accumulator, status| {
            let slow = status
                .magnitude
                .saturating_mul(u16::from(status.stacks))
                .min(8_000);
            accumulator.saturating_add(slow).min(8_000)
        })
}

fn total_haste_bps(statuses: &[StatusInstance]) -> u16 {
    statuses
        .iter()
        .filter(|status| status.kind == StatusKind::Haste)
        .fold(0_u16, |accumulator, status| {
            let haste = status
                .magnitude
                .saturating_mul(u16::from(status.stacks))
                .min(6_000);
            accumulator.saturating_add(haste).min(6_000)
        })
}

pub(crate) fn total_move_modifier_bps(statuses: &[StatusInstance]) -> i16 {
    let haste = i32::from(total_haste_bps(statuses));
    let slow = i32::from(total_slow_bps(statuses));
    i16::try_from((haste - slow).clamp(-8_000, 6_000)).unwrap_or(0)
}

pub(crate) fn adjusted_move_speed(delta_ms: u16, move_modifier_bps: i16) -> u16 {
    let scale_bps = (10_000_i32 + i32::from(move_modifier_bps))
        .clamp(2_000, 16_000)
        .cast_unsigned();
    let effective_speed =
        u32::from(PLAYER_MOVE_SPEED_UNITS_PER_SECOND).saturating_mul(scale_bps) / 10_000;
    let distance = effective_speed.saturating_mul(u32::from(delta_ms)) / 1000;
    u16::try_from(distance).unwrap_or(u16::MAX)
}

pub(crate) fn travel_distance_units(speed_units_per_second: u16, delta_ms: u16) -> u16 {
    let distance = u32::from(speed_units_per_second).saturating_mul(u32::from(delta_ms)) / 1000;
    u16::try_from(distance).unwrap_or(u16::MAX)
}

pub(crate) fn movement_delta(intent: MovementIntent, speed: u16) -> (i32, i32) {
    let mut x = f32::from(intent.x);
    let mut y = f32::from(intent.y);
    let length = (x * x + y * y).sqrt();
    if length > f32::EPSILON {
        x /= length;
        y /= length;
    }

    (
        round_f32_to_i32(x * f32::from(speed)),
        round_f32_to_i32(y * f32::from(speed)),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_movement(
    start_x: i16,
    start_y: i16,
    desired_x: i32,
    desired_y: i32,
    arena_width_units: u16,
    arena_height_units: u16,
    width_tiles: u16,
    height_tiles: u16,
    tile_units: u16,
    footprint_mask: &[u8],
    obstacles: &[ArenaObstacle],
) -> (i16, i16) {
    let min_x = -i32::from(arena_width_units / 2);
    let max_x = i32::from(arena_width_units / 2);
    let min_y = -i32::from(arena_height_units / 2);
    let max_y = i32::from(arena_height_units / 2);
    let candidate_x = desired_x.clamp(
        min_x + i32::from(PLAYER_RADIUS_UNITS),
        max_x - i32::from(PLAYER_RADIUS_UNITS),
    );
    let candidate_y = desired_y.clamp(
        min_y + i32::from(PLAYER_RADIUS_UNITS),
        max_y - i32::from(PLAYER_RADIUS_UNITS),
    );

    let mut resolved_x = saturating_i16(candidate_x);
    let mut resolved_y = saturating_i16(candidate_y);
    if !circle_fits_map_footprint(
        resolved_x,
        resolved_y,
        PLAYER_RADIUS_UNITS,
        arena_width_units,
        arena_height_units,
        width_tiles,
        height_tiles,
        tile_units,
        footprint_mask,
    ) {
        resolved_x = start_x;
        resolved_y = start_y;
    }
    if obstacles
        .iter()
        .filter(|obstacle| obstacle_blocks_movement(obstacle))
        .any(|obstacle| {
            circle_intersects_rect(resolved_x, resolved_y, PLAYER_RADIUS_UNITS, obstacle)
        })
    {
        resolved_x = start_x;
        resolved_y = start_y;
    }

    (resolved_x, resolved_y)
}

pub(crate) fn map_mask_has_tile(mask: &[u8], index: usize) -> bool {
    let byte_index = index / 8;
    let bit_index = index % 8;
    mask.get(byte_index)
        .is_some_and(|byte| (byte & (1_u8 << bit_index)) != 0)
}

pub(crate) fn point_in_map_footprint(
    x: i16,
    y: i16,
    arena_width_units: u16,
    arena_height_units: u16,
    width_tiles: u16,
    height_tiles: u16,
    tile_units: u16,
    footprint_mask: &[u8],
) -> bool {
    let Some(index) = tile_index_for_point(
        x,
        y,
        arena_width_units,
        arena_height_units,
        width_tiles,
        height_tiles,
        tile_units,
    ) else {
        return false;
    };
    map_mask_has_tile(footprint_mask, index)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn circle_fits_map_footprint(
    x: i16,
    y: i16,
    radius: u16,
    arena_width_units: u16,
    arena_height_units: u16,
    width_tiles: u16,
    height_tiles: u16,
    tile_units: u16,
    footprint_mask: &[u8],
) -> bool {
    let diagonal_offset = i16::try_from(u32::from(radius) * 707 / 1000).unwrap_or(i16::MAX);
    let radius_i16 = i16::try_from(radius).unwrap_or(i16::MAX);
    [
        (x, y),
        (x.saturating_add(radius_i16), y),
        (x.saturating_sub(radius_i16), y),
        (x, y.saturating_add(radius_i16)),
        (x, y.saturating_sub(radius_i16)),
        (
            x.saturating_add(diagonal_offset),
            y.saturating_add(diagonal_offset),
        ),
        (
            x.saturating_add(diagonal_offset),
            y.saturating_sub(diagonal_offset),
        ),
        (
            x.saturating_sub(diagonal_offset),
            y.saturating_add(diagonal_offset),
        ),
        (
            x.saturating_sub(diagonal_offset),
            y.saturating_sub(diagonal_offset),
        ),
    ]
    .into_iter()
    .all(|(sample_x, sample_y)| {
        point_in_map_footprint(
            sample_x,
            sample_y,
            arena_width_units,
            arena_height_units,
            width_tiles,
            height_tiles,
            tile_units,
            footprint_mask,
        )
    })
}

fn tile_index_for_point(
    x: i16,
    y: i16,
    arena_width_units: u16,
    arena_height_units: u16,
    width_tiles: u16,
    height_tiles: u16,
    tile_units: u16,
) -> Option<usize> {
    let tile_units_i32 = i32::from(tile_units);
    if tile_units_i32 <= 0 {
        return None;
    }
    let mut relative_x = i32::from(x) + i32::from(arena_width_units) / 2;
    let mut relative_y = i32::from(y) + i32::from(arena_height_units) / 2;
    let arena_width_units_i32 = i32::from(arena_width_units);
    let arena_height_units_i32 = i32::from(arena_height_units);
    if relative_x == arena_width_units_i32 {
        relative_x = arena_width_units_i32.saturating_sub(1);
    }
    if relative_y == arena_height_units_i32 {
        relative_y = arena_height_units_i32.saturating_sub(1);
    }
    if relative_x < 0
        || relative_y < 0
        || relative_x >= arena_width_units_i32
        || relative_y >= arena_height_units_i32
    {
        return None;
    }
    let column = usize::try_from(relative_x / tile_units_i32).ok()?;
    let row = usize::try_from(relative_y / tile_units_i32).ok()?;
    if column >= usize::from(width_tiles) || row >= usize::from(height_tiles) {
        return None;
    }
    Some(row * usize::from(width_tiles) + column)
}
