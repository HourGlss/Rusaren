# `configurations.yaml`

`server/content/config/configurations.yaml` is the authored balance and runtime-tuning file for the server content bundle.

The authoritative schema is the Rust loader:

- `server/crates/game_content/src/yaml.rs`
- `server/crates/game_content/src/config.rs`
- `server/crates/game_content/src/model.rs`

This README describes the fields that loader accepts today and the validation rules it enforces.

## Top-Level Structure

```yaml
lobby:
  ...

match:
  ...

maps:
  ...

simulation:
  ...

classes:
  ...
```

## `lobby`

```yaml
lobby:
  launch_countdown_seconds: 5
```

- `launch_countdown_seconds: u8`
  - The ready-check countdown before an open lobby launches into a match.
  - Must be greater than `0`.

## `match`

```yaml
match:
  total_rounds: 5
  skill_pick_seconds: 25
  pre_combat_seconds: 5
```

- `total_rounds: u8`
  - Number of rounds in the match.
  - Parsed through `RoundNumber::new`, so it must be within the domain’s valid round range.
- `skill_pick_seconds: u8`
  - Length of the skill-pick phase for each round.
  - Must be greater than `0`.
- `pre_combat_seconds: u8`
  - Countdown between the end of picks and the start of combat.
  - Must be greater than `0`.

## `maps`

```yaml
maps:
  tile_units: 50
  objective_target_ms_by_map:
    template_arena: 30000
    prototype_arena: 180000
    training_arena: 180000
  generation:
    ...
```

- `tile_units: u16`
  - Tile size used when ASCII maps are parsed into arena coordinates.
  - Must be greater than `0`.
- `objective_target_ms_by_map: { map_id -> u32 }`
  - Per-authored-map objective timer target in milliseconds.
  - Every key must be a non-empty map id.
  - Every value must be greater than `0`.
  - Every configured map id must exist in `server/content/maps`.

### `maps.generation`

These values drive the generated match maps created from `template_arena`.

```yaml
maps:
  generation:
    max_generation_attempts: 256
    protected_tile_buffer_radius_tiles: 1
    obstacle_edge_padding_tiles: 1
    wall_segment_lengths_tiles: [2, 3]
    long_wall_percent: 42
    wall_candidate_skip_percent: 35
    wall_min_spacing_manhattan_tiles: 4
    pillar_candidate_skip_percent: 55
    pillar_min_spacing_manhattan_tiles: 3
    styles:
      - shrub_clusters: 1
        shrub_radius_tiles: 1
        shrub_soft_radius_tiles: 2
        shrub_fill_percent: 16
        wall_segments: 1
        isolated_pillars: 2
```

- `max_generation_attempts: usize`
  - How many symmetric layouts the generator may try before it gives up.
  - Must be greater than `0`.
- `protected_tile_buffer_radius_tiles: i32`
  - Buffer radius around objective tiles, spawn anchors, and map features where generation cannot place obstacles.
  - Must be non-negative.
- `obstacle_edge_padding_tiles: i32`
  - Minimum distance from the map edge for generated pillars and wall tiles.
  - Must be non-negative.
- `wall_segment_lengths_tiles: [i32; 2]`
  - Exactly two values: `[short_length, long_length]`.
  - Both must be positive.
  - The second value must be greater than or equal to the first.
- `long_wall_percent: u8`
  - Percentage chance that a candidate wall uses the longer segment length instead of the shorter one.
  - Must be between `0` and `100`.
- `wall_candidate_skip_percent: u8`
  - Percentage chance that a valid wall candidate is skipped to keep layouts varied.
  - Must be between `0` and `100`.
- `wall_min_spacing_manhattan_tiles: i32`
  - Minimum Manhattan spacing between wall representatives.
  - Must be non-negative.
- `pillar_candidate_skip_percent: u8`
  - Percentage chance that an isolated pillar candidate is skipped.
  - Must be between `0` and `100`.
- `pillar_min_spacing_manhattan_tiles: i32`
  - Minimum Manhattan spacing between isolated pillar representatives.
  - Must be non-negative.
- `styles: [MapGenerationStyle]`
  - At least one style must be present.
  - One style is chosen randomly per generated map.

#### `maps.generation.styles[]`

```yaml
styles:
  - shrub_clusters: 3
    shrub_radius_tiles: 2
    shrub_soft_radius_tiles: 4
    shrub_fill_percent: 38
    wall_segments: 2
    isolated_pillars: 3
```

- `shrub_clusters: usize`
  - Number of cluster seeds used for shrub placement.
- `shrub_radius_tiles: i32`
  - Radius where shrub placement is heavily favored.
  - Must be non-negative.
- `shrub_soft_radius_tiles: i32`
  - Outer radius where shrub placement falls back to the style’s fill percentage.
  - Must be non-negative.
  - Must be greater than or equal to `shrub_radius_tiles`.
- `shrub_fill_percent: u8`
  - Percentage used for softer shrub fill.
  - Must be between `0` and `100`.
- `wall_segments: usize`
  - Number of short or long wall segments to place for this style.
- `isolated_pillars: usize`
  - Number of additional non-wall pillar groups to try to place for this style.

## `simulation`

```yaml
simulation:
  combat_frame_ms: 100
  player_radius_units: 28
  vision_radius_units: 450
  spawn_spacing_units: 120
  default_aim_x_units: 120
  default_aim_y_units: 0
  mana_regen_per_second: 12
  global_projectile_speed_bonus_bps: 2000
  teleport_resolution_steps: 48
  passive_bonus_caps:
    ...
  movement_modifier_caps:
    ...
  crowd_control_diminishing_returns:
    ...
  training_dummy:
    ...
```

- `combat_frame_ms: u16`
  - Simulation step length in milliseconds.
  - Used by match advancement, server runtime stepping, and several diagnostics.
  - Must be greater than `0`.
- `player_radius_units: u16`
  - Collision and footprint radius for players.
  - Must be greater than `0`.
- `vision_radius_units: u16`
  - Base line-of-sight radius used by visibility snapshots.
  - Must be greater than `0`.
- `spawn_spacing_units: i16`
  - Vertical spacing between same-team spawn placements that share the same authored anchor lane.
- `default_aim_x_units: i16`
- `default_aim_y_units: i16`
  - Default aim vector used when an input frame does not provide a direction.
  - The loader rejects the configuration if both axes are `0`.
- `mana_regen_per_second: u16`
  - Mana restored per second during combat.
  - Must be greater than `0`.
- `global_projectile_speed_bonus_bps: u16`
  - Global projectile speed multiplier expressed in basis points.
  - `10000` means no change.
  - `12000` means `+20%`.
  - `8000` means `-20%`.
- `teleport_resolution_steps: u16`
  - Number of interpolation steps used when validating teleport paths.
  - Must be greater than `0`.

### `simulation.passive_bonus_caps`

```yaml
passive_bonus_caps:
  player_speed_bps: 9000
  projectile_speed_bps: 9000
  cooldown_bps: 9000
  cast_time_bps: 9500
```

Every field in this block is a `u16` basis-point cap and must be between `0` and `10000`.

- `player_speed_bps`
- `projectile_speed_bps`
- `cooldown_bps`
- `cast_time_bps`

These cap the total passive bonus that can be accumulated from skill trees and effects for the corresponding stat.

### `simulation.movement_modifier_caps`

```yaml
movement_modifier_caps:
  chill_bps: 8000
  haste_bps: 6000
  status_total_min_bps: -8000
  status_total_max_bps: 6000
  overall_total_min_bps: -8000
  overall_total_max_bps: 9000
  effective_scale_min_bps: 2000
  effective_scale_max_bps: 16000
```

- `chill_bps: u16`
- `haste_bps: u16`
  - Basis-point magnitudes for the built-in generic chill and haste handling.
  - Each must be between `0` and `10000`.
- `status_total_min_bps: i16`
- `status_total_max_bps: i16`
  - Clamp range for the total status-derived movement modifier.
  - `min` must be less than or equal to `max`.
- `overall_total_min_bps: i16`
- `overall_total_max_bps: i16`
  - Clamp range after status modifiers and passives are combined.
  - `min` must be less than or equal to `max`.
- `effective_scale_min_bps: u16`
- `effective_scale_max_bps: u16`
  - Final effective movement-speed multiplier clamp.
  - Must be positive and ascending.

### `simulation.crowd_control_diminishing_returns`

```yaml
crowd_control_diminishing_returns:
  window_ms: 15000
  stages_bps: [10000, 5000, 2500, 0]
```

- `window_ms: u16`
  - Rolling window in milliseconds for DR tracking.
  - Must be greater than `0`.
- `stages_bps: [u16; 4]`
  - Exactly four basis-point entries.
  - Each value must be between `0` and `10000`.
  - Example:
    - `10000` = full duration
    - `5000` = half duration
    - `2500` = quarter duration
    - `0` = immune within the DR window

### `simulation.training_dummy`

```yaml
training_dummy:
  base_hit_points: 100
  health_multiplier: 100
  execute_threshold_bps: 500
```

- `base_hit_points: u16`
  - Base HP budget before scaling.
  - Must be greater than `0`.
- `health_multiplier: u16`
  - Multiplier applied to the base HP budget.
  - Must be greater than `0`.
- `execute_threshold_bps: u16`
  - Execute threshold in basis points.
  - Must be between `0` and `10000`.
  - `500` means `5%`.

## `classes`

```yaml
classes:
  Cleric:
    hit_points: 105
    max_mana: 120
    move_speed_units_per_second: 275
```

This block defines the baseline class profile for each authored `SkillTree`.

- Each key must parse as a valid `SkillTree`.
- A profile is required for every authored class tree in `server/content/skills`.
- Each numeric value must be greater than `0`.

Supported fields per class:

- `hit_points: u16`
- `max_mana: u16`
- `move_speed_units_per_second: u16`

### First-Pick Rule

Class stats are determined by the first class pick in the player’s round loadout.

If a player picks:

1. `Cleric` tier 1
2. `Warrior` tier 2
3. `Warrior` tier 3
4. `Warrior` tier 4
5. `Warrior` tier 5

then that player still uses the `Cleric` class profile for:

- base hit points
- base mana
- base movement speed

The later picks affect the skill bar, but not the base class profile for that round.

## Validation Summary

The loader rejects `configurations.yaml` when:

- a required top-level section is missing
- a positive-only field is `0`
- a percent exceeds `100`
- a basis-point field exceeds `10000`
- a range is reversed
- a wall-length pair is not exactly two ascending positive values
- DR stages do not contain exactly four entries
- a class profile is missing or invalid
- an objective target is configured for an unknown map

## Practical Editing Advice

- Treat this file as the home for authored gameplay and pacing numbers.
- Prefer changing values here before changing Rust, especially for:
  - match pacing
  - class baseline stats
  - visibility radius
  - projectile/movement tuning
  - generated map density and style
- After changes, rerun the relevant content and gameplay tests:

```powershell
cargo test -p game_content
cargo test -p game_match
cargo test -p game_lobby
cargo test -p game_sim --lib
cargo test -p game_api --lib
```
