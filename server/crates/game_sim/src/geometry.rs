use crate::{ArenaObstacle, ArenaObstacleKind};

#[must_use]
pub const fn obstacle_blocks_movement(obstacle: &ArenaObstacle) -> bool {
    matches!(
        obstacle.kind,
        ArenaObstacleKind::Pillar | ArenaObstacleKind::Barrier
    )
}

#[must_use]
pub const fn obstacle_blocks_projectiles(obstacle: &ArenaObstacle) -> bool {
    matches!(
        obstacle.kind,
        ArenaObstacleKind::Pillar | ArenaObstacleKind::Barrier
    )
}

#[must_use]
pub const fn obstacle_blocks_vision(obstacle: &ArenaObstacle) -> bool {
    matches!(
        obstacle.kind,
        ArenaObstacleKind::Pillar | ArenaObstacleKind::Shrub
    )
}

#[must_use]
pub(crate) fn circle_intersects_rect(
    x: i16,
    y: i16,
    radius: u16,
    obstacle: &ArenaObstacle,
) -> bool {
    let (left, right, top, bottom) = rect_bounds(obstacle);
    let closest_x = x.clamp(left, right);
    let closest_y = y.clamp(top, bottom);
    let dx = i32::from(x - closest_x);
    let dy = i32::from(y - closest_y);
    dx * dx + dy * dy <= i32::from(radius) * i32::from(radius)
}

#[must_use]
pub fn obstacle_contains_point(x: i16, y: i16, obstacle: &ArenaObstacle) -> bool {
    let (left, right, top, bottom) = rect_bounds(obstacle);
    x >= left && x <= right && y >= top && y <= bottom
}

#[must_use]
pub fn segment_hits_obstacle(start: (i16, i16), end: (i16, i16), obstacle: &ArenaObstacle) -> bool {
    segment_rect_intersection_t(start, end, obstacle).is_some()
}

#[must_use]
pub(crate) fn truncate_line_to_obstacles(
    start: (i16, i16),
    end: (i16, i16),
    obstacles: &[ArenaObstacle],
) -> (i16, i16) {
    let mut closest_t = 1.0_f32;
    for obstacle in obstacles
        .iter()
        .filter(|obstacle| obstacle_blocks_projectiles(obstacle))
    {
        if let Some(intersection_t) = segment_rect_intersection_t(start, end, obstacle) {
            if intersection_t < closest_t {
                closest_t = intersection_t;
            }
        }
    }

    if closest_t >= 1.0 {
        return end;
    }

    let start_x = f32::from(start.0);
    let start_y = f32::from(start.1);
    let delta_x = f32::from(end.0) - start_x;
    let delta_y = f32::from(end.1) - start_y;
    let clipped_t = (closest_t - 0.01).clamp(0.0, 1.0);
    (
        saturating_i16(round_f32_to_i32(start_x + delta_x * clipped_t)),
        saturating_i16(round_f32_to_i32(start_y + delta_y * clipped_t)),
    )
}

#[must_use]
pub(crate) fn segment_rect_intersection_t(
    start: (i16, i16),
    end: (i16, i16),
    obstacle: &ArenaObstacle,
) -> Option<f32> {
    let start_x = f32::from(start.0);
    let start_y = f32::from(start.1);
    let delta_x = f32::from(end.0 - start.0);
    let delta_y = f32::from(end.1 - start.1);
    let (left, right, top, bottom) = rect_bounds(obstacle);
    let min_x = f32::from(left);
    let max_x = f32::from(right);
    let min_y = f32::from(top);
    let max_y = f32::from(bottom);
    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    if !update_segment_slab(start_x, delta_x, min_x, max_x, &mut t_min, &mut t_max) {
        return None;
    }
    if !update_segment_slab(start_y, delta_y, min_y, max_y, &mut t_min, &mut t_max) {
        return None;
    }

    if t_min <= 0.0 || t_min > 1.0 {
        None
    } else {
        Some(t_min)
    }
}

pub(crate) fn update_segment_slab(
    start: f32,
    delta: f32,
    min_bound: f32,
    max_bound: f32,
    t_min: &mut f32,
    t_max: &mut f32,
) -> bool {
    if delta.abs() <= f32::EPSILON {
        return start >= min_bound && start <= max_bound;
    }

    let inverse_delta = 1.0 / delta;
    let mut entry = (min_bound - start) * inverse_delta;
    let mut exit = (max_bound - start) * inverse_delta;
    if entry > exit {
        std::mem::swap(&mut entry, &mut exit);
    }
    *t_min = (*t_min).max(entry);
    *t_max = (*t_max).min(exit);
    *t_min <= *t_max
}

#[must_use]
pub(crate) fn project_from_aim(
    origin_x: i16,
    origin_y: i16,
    aim_x: i16,
    aim_y: i16,
    distance: u16,
) -> (i16, i16) {
    let direction = normalize_aim(aim_x, aim_y);
    let projected_x = f32::from(origin_x) + direction.0 * f32::from(distance);
    let projected_y = f32::from(origin_y) + direction.1 * f32::from(distance);
    (
        saturating_i16(round_f32_to_i32(projected_x)),
        saturating_i16(round_f32_to_i32(projected_y)),
    )
}

#[must_use]
pub(crate) fn normalize_aim(aim_x: i16, aim_y: i16) -> (f32, f32) {
    let raw_x = f32::from(aim_x);
    let raw_y = f32::from(aim_y);
    let length = (raw_x * raw_x + raw_y * raw_y).sqrt();
    if length <= f32::EPSILON {
        return (1.0, 0.0);
    }
    (raw_x / length, raw_y / length)
}

#[must_use]
pub(crate) fn point_distance_sq(a: (i16, i16), b: (i16, i16)) -> i32 {
    let dx = i32::from(a.0) - i32::from(b.0);
    let dy = i32::from(a.1) - i32::from(b.1);
    dx * dx + dy * dy
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
#[must_use]
pub(crate) fn point_distance_units(a: (i16, i16), b: (i16, i16)) -> u16 {
    let distance = ((point_distance_sq(a, b)) as f32).sqrt().round();
    u16::try_from(distance as i32).unwrap_or(u16::MAX)
}

#[must_use]
pub(crate) fn segment_distance_sq(start: (i16, i16), end: (i16, i16), point: (i16, i16)) -> f32 {
    let ax = f32::from(start.0);
    let ay = f32::from(start.1);
    let bx = f32::from(end.0);
    let by = f32::from(end.1);
    let px = f32::from(point.0);
    let py = f32::from(point.1);
    let ab_x = bx - ax;
    let ab_y = by - ay;
    let ab_len_sq = ab_x * ab_x + ab_y * ab_y;
    if ab_len_sq <= f32::EPSILON {
        let dx = px - ax;
        let dy = py - ay;
        return dx * dx + dy * dy;
    }

    let ap_x = px - ax;
    let ap_y = py - ay;
    let t = ((ap_x * ab_x + ap_y * ab_y) / ab_len_sq).clamp(0.0, 1.0);
    let nearest_x = ax + ab_x * t;
    let nearest_y = ay + ab_y * t;
    let dx = px - nearest_x;
    let dy = py - nearest_y;
    dx * dx + dy * dy
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
#[must_use]
pub(crate) fn round_f32_to_i32(value: f32) -> i32 {
    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

#[must_use]
pub(crate) fn saturating_i16(value: i32) -> i16 {
    let clamped = value.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
    i16::try_from(clamped).unwrap_or(if clamped < 0 { i16::MIN } else { i16::MAX })
}

fn rect_bounds(obstacle: &ArenaObstacle) -> (i16, i16, i16, i16) {
    let left = obstacle.center_x - i16::try_from(obstacle.half_width).unwrap_or(i16::MAX);
    let right = obstacle.center_x + i16::try_from(obstacle.half_width).unwrap_or(i16::MAX);
    let top = obstacle.center_y - i16::try_from(obstacle.half_height).unwrap_or(i16::MAX);
    let bottom = obstacle.center_y + i16::try_from(obstacle.half_height).unwrap_or(i16::MAX);
    (left, right, top, bottom)
}

#[cfg(test)]
mod tests;
