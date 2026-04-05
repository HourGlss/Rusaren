use std::collections::BTreeMap;

use game_domain::{RoundNumber, SkillTree};

use super::yaml::{
    ClassProfileYaml, ConfigurationsYaml, CrowdControlDiminishingReturnsYaml,
    LobbyConfigurationYaml, MapGenerationConfigurationYaml, MapGenerationStyleYaml,
    MapsConfigurationYaml, MatchConfigurationYaml, MovementModifierCapsYaml, PassiveBonusCapsYaml,
    SimulationConfigurationYaml, TrainingDummyConfigurationYaml,
};
use super::{
    ClassProfile, ContentError, CrowdControlDiminishingReturns, GameConfiguration,
    LobbyConfiguration, MapGenerationConfiguration, MapGenerationStyle, MapsConfiguration,
    MatchConfiguration, MovementModifierCaps, PassiveBonusCaps, SimulationConfiguration,
    TrainingDummyConfiguration,
};

pub fn parse_configuration_yaml(
    source: &str,
    yaml: &str,
) -> Result<GameConfiguration, ContentError> {
    let parsed: ConfigurationsYaml =
        serde_yaml::from_str(yaml).map_err(|error| ContentError::Parse {
            source: String::from(source),
            message: error.to_string(),
        })?;

    Ok(GameConfiguration {
        lobby: parse_lobby_configuration(source, parsed.lobby)?,
        match_flow: parse_match_configuration(source, parsed.match_flow)?,
        maps: parse_maps_configuration(source, parsed.maps)?,
        simulation: parse_simulation_configuration(source, parsed.simulation)?,
        classes: parse_class_profiles(source, parsed.classes)?,
    })
}

fn parse_lobby_configuration(
    source: &str,
    yaml: LobbyConfigurationYaml,
) -> Result<LobbyConfiguration, ContentError> {
    require_positive_u8(
        source,
        "lobby.launch_countdown_seconds",
        yaml.launch_countdown_seconds,
    )?;
    Ok(LobbyConfiguration {
        launch_countdown_seconds: yaml.launch_countdown_seconds,
    })
}

fn parse_match_configuration(
    source: &str,
    yaml: MatchConfigurationYaml,
) -> Result<MatchConfiguration, ContentError> {
    require_valid_round_number(source, "match.total_rounds", yaml.total_rounds)?;
    require_positive_u8(source, "match.skill_pick_seconds", yaml.skill_pick_seconds)?;
    require_positive_u8(source, "match.pre_combat_seconds", yaml.pre_combat_seconds)?;
    Ok(MatchConfiguration {
        total_rounds: yaml.total_rounds,
        skill_pick_seconds: yaml.skill_pick_seconds,
        pre_combat_seconds: yaml.pre_combat_seconds,
    })
}

fn parse_maps_configuration(
    source: &str,
    yaml: MapsConfigurationYaml,
) -> Result<MapsConfiguration, ContentError> {
    require_positive_u16(source, "maps.tile_units", yaml.tile_units)?;
    if yaml.objective_target_ms_by_map.is_empty() {
        return Err(validation_error(
            source,
            "maps.objective_target_ms_by_map must contain at least one entry",
        ));
    }
    for (map_id, objective_target_ms) in &yaml.objective_target_ms_by_map {
        if map_id.trim().is_empty() {
            return Err(validation_error(
                source,
                "maps.objective_target_ms_by_map entries must use non-empty map ids",
            ));
        }
        require_positive_u32(
            source,
            &format!("maps.objective_target_ms_by_map.{map_id}"),
            *objective_target_ms,
        )?;
    }
    Ok(MapsConfiguration {
        tile_units: yaml.tile_units,
        objective_target_ms_by_map: yaml.objective_target_ms_by_map,
        generation: parse_map_generation_configuration(source, yaml.generation)?,
    })
}

fn parse_map_generation_configuration(
    source: &str,
    yaml: MapGenerationConfigurationYaml,
) -> Result<MapGenerationConfiguration, ContentError> {
    if yaml.max_generation_attempts == 0 {
        return Err(validation_error(
            source,
            "maps.generation.max_generation_attempts must be greater than zero",
        ));
    }
    if yaml.protected_tile_buffer_radius_tiles < 0 {
        return Err(validation_error(
            source,
            "maps.generation.protected_tile_buffer_radius_tiles must be non-negative",
        ));
    }
    if yaml.obstacle_edge_padding_tiles < 0 {
        return Err(validation_error(
            source,
            "maps.generation.obstacle_edge_padding_tiles must be non-negative",
        ));
    }
    if yaml.wall_segment_lengths_tiles.len() != 2 {
        return Err(validation_error(
            source,
            "maps.generation.wall_segment_lengths_tiles must contain exactly two lengths",
        ));
    }
    let short_length = yaml.wall_segment_lengths_tiles[0];
    let long_length = yaml.wall_segment_lengths_tiles[1];
    if short_length <= 0 || long_length <= 0 || long_length < short_length {
        return Err(validation_error(
            source,
            "maps.generation.wall_segment_lengths_tiles must be positive and ascending",
        ));
    }
    validate_percent(
        source,
        "maps.generation.long_wall_percent",
        yaml.long_wall_percent,
    )?;
    validate_percent(
        source,
        "maps.generation.wall_candidate_skip_percent",
        yaml.wall_candidate_skip_percent,
    )?;
    validate_percent(
        source,
        "maps.generation.pillar_candidate_skip_percent",
        yaml.pillar_candidate_skip_percent,
    )?;
    if yaml.wall_min_spacing_manhattan_tiles < 0 {
        return Err(validation_error(
            source,
            "maps.generation.wall_min_spacing_manhattan_tiles must be non-negative",
        ));
    }
    if yaml.pillar_min_spacing_manhattan_tiles < 0 {
        return Err(validation_error(
            source,
            "maps.generation.pillar_min_spacing_manhattan_tiles must be non-negative",
        ));
    }
    if yaml.styles.is_empty() {
        return Err(validation_error(
            source,
            "maps.generation.styles must contain at least one style",
        ));
    }

    let styles = yaml
        .styles
        .into_iter()
        .enumerate()
        .map(|(index, style)| parse_map_generation_style(source, index, style))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(MapGenerationConfiguration {
        max_generation_attempts: yaml.max_generation_attempts,
        protected_tile_buffer_radius_tiles: yaml.protected_tile_buffer_radius_tiles,
        obstacle_edge_padding_tiles: yaml.obstacle_edge_padding_tiles,
        wall_segment_lengths_tiles: [short_length, long_length],
        long_wall_percent: yaml.long_wall_percent,
        wall_candidate_skip_percent: yaml.wall_candidate_skip_percent,
        wall_min_spacing_manhattan_tiles: yaml.wall_min_spacing_manhattan_tiles,
        pillar_candidate_skip_percent: yaml.pillar_candidate_skip_percent,
        pillar_min_spacing_manhattan_tiles: yaml.pillar_min_spacing_manhattan_tiles,
        styles,
    })
}

fn parse_map_generation_style(
    source: &str,
    index: usize,
    yaml: MapGenerationStyleYaml,
) -> Result<MapGenerationStyle, ContentError> {
    let prefix = format!("maps.generation.styles[{index}]");
    if yaml.shrub_radius_tiles < 0 || yaml.shrub_soft_radius_tiles < 0 {
        return Err(validation_error(
            source,
            format!("{prefix} shrub radii must be non-negative"),
        ));
    }
    if yaml.shrub_soft_radius_tiles < yaml.shrub_radius_tiles {
        return Err(validation_error(
            source,
            format!("{prefix}.shrub_soft_radius_tiles must be >= shrub_radius_tiles"),
        ));
    }
    validate_percent(
        source,
        &format!("{prefix}.shrub_fill_percent"),
        yaml.shrub_fill_percent,
    )?;
    Ok(MapGenerationStyle {
        shrub_clusters: yaml.shrub_clusters,
        shrub_radius_tiles: yaml.shrub_radius_tiles,
        shrub_soft_radius_tiles: yaml.shrub_soft_radius_tiles,
        shrub_fill_percent: yaml.shrub_fill_percent,
        wall_segments: yaml.wall_segments,
        isolated_pillars: yaml.isolated_pillars,
    })
}

fn parse_simulation_configuration(
    source: &str,
    yaml: SimulationConfigurationYaml,
) -> Result<SimulationConfiguration, ContentError> {
    require_positive_u16(source, "simulation.combat_frame_ms", yaml.combat_frame_ms)?;
    require_positive_u16(
        source,
        "simulation.player_radius_units",
        yaml.player_radius_units,
    )?;
    require_positive_u16(
        source,
        "simulation.vision_radius_units",
        yaml.vision_radius_units,
    )?;
    if yaml.default_aim_x_units == 0 && yaml.default_aim_y_units == 0 {
        return Err(validation_error(
            source,
            "simulation default aim vector must not be zero on both axes",
        ));
    }
    require_positive_u16(
        source,
        "simulation.mana_regen_per_second",
        yaml.mana_regen_per_second,
    )?;
    require_positive_u16(
        source,
        "simulation.teleport_resolution_steps",
        yaml.teleport_resolution_steps,
    )?;
    require_positive_u16(
        source,
        "simulation.movement_audio_step_interval_ms",
        yaml.movement_audio_step_interval_ms,
    )?;
    require_positive_u16(
        source,
        "simulation.movement_audio_radius_units",
        yaml.movement_audio_radius_units,
    )?;
    require_positive_u16(
        source,
        "simulation.stealth_audio_radius_units",
        yaml.stealth_audio_radius_units,
    )?;
    validate_percent(
        source,
        "simulation.brush_movement_audible_percent",
        yaml.brush_movement_audible_percent,
    )?;

    Ok(SimulationConfiguration {
        combat_frame_ms: yaml.combat_frame_ms,
        player_radius_units: yaml.player_radius_units,
        vision_radius_units: yaml.vision_radius_units,
        spawn_spacing_units: yaml.spawn_spacing_units,
        default_aim_x_units: yaml.default_aim_x_units,
        default_aim_y_units: yaml.default_aim_y_units,
        mana_regen_per_second: yaml.mana_regen_per_second,
        global_projectile_speed_bonus_bps: yaml.global_projectile_speed_bonus_bps,
        teleport_resolution_steps: yaml.teleport_resolution_steps,
        movement_audio_step_interval_ms: yaml.movement_audio_step_interval_ms,
        movement_audio_radius_units: yaml.movement_audio_radius_units,
        stealth_audio_radius_units: yaml.stealth_audio_radius_units,
        brush_movement_audible_percent: yaml.brush_movement_audible_percent,
        passive_bonus_caps: parse_passive_bonus_caps(source, yaml.passive_bonus_caps)?,
        movement_modifier_caps: parse_movement_modifier_caps(source, yaml.movement_modifier_caps)?,
        crowd_control_diminishing_returns: parse_crowd_control_diminishing_returns(
            source,
            yaml.crowd_control_diminishing_returns,
        )?,
        training_dummy: parse_training_dummy_configuration(source, yaml.training_dummy)?,
    })
}

fn parse_passive_bonus_caps(
    source: &str,
    yaml: PassiveBonusCapsYaml,
) -> Result<PassiveBonusCaps, ContentError> {
    validate_basis_points(
        source,
        "simulation.passive_bonus_caps.player_speed_bps",
        yaml.player_speed_bps,
    )?;
    validate_basis_points(
        source,
        "simulation.passive_bonus_caps.projectile_speed_bps",
        yaml.projectile_speed_bps,
    )?;
    validate_basis_points(
        source,
        "simulation.passive_bonus_caps.cooldown_bps",
        yaml.cooldown_bps,
    )?;
    validate_basis_points(
        source,
        "simulation.passive_bonus_caps.cast_time_bps",
        yaml.cast_time_bps,
    )?;
    Ok(PassiveBonusCaps {
        player_speed_bps: yaml.player_speed_bps,
        projectile_speed_bps: yaml.projectile_speed_bps,
        cooldown_bps: yaml.cooldown_bps,
        cast_time_bps: yaml.cast_time_bps,
    })
}

fn parse_movement_modifier_caps(
    source: &str,
    yaml: MovementModifierCapsYaml,
) -> Result<MovementModifierCaps, ContentError> {
    validate_basis_points(
        source,
        "simulation.movement_modifier_caps.chill_bps",
        yaml.chill_bps,
    )?;
    validate_basis_points(
        source,
        "simulation.movement_modifier_caps.haste_bps",
        yaml.haste_bps,
    )?;
    validate_ordered_i16_range(
        source,
        "simulation.movement_modifier_caps.status_total",
        yaml.status_total_min_bps,
        yaml.status_total_max_bps,
    )?;
    validate_ordered_i16_range(
        source,
        "simulation.movement_modifier_caps.overall_total",
        yaml.overall_total_min_bps,
        yaml.overall_total_max_bps,
    )?;
    validate_ordered_u16_range(
        source,
        "simulation.movement_modifier_caps.effective_scale",
        yaml.effective_scale_min_bps,
        yaml.effective_scale_max_bps,
    )?;
    Ok(MovementModifierCaps {
        chill_bps: yaml.chill_bps,
        haste_bps: yaml.haste_bps,
        status_total_min_bps: yaml.status_total_min_bps,
        status_total_max_bps: yaml.status_total_max_bps,
        overall_total_min_bps: yaml.overall_total_min_bps,
        overall_total_max_bps: yaml.overall_total_max_bps,
        effective_scale_min_bps: yaml.effective_scale_min_bps,
        effective_scale_max_bps: yaml.effective_scale_max_bps,
    })
}

fn parse_crowd_control_diminishing_returns(
    source: &str,
    yaml: CrowdControlDiminishingReturnsYaml,
) -> Result<CrowdControlDiminishingReturns, ContentError> {
    require_positive_u16(
        source,
        "simulation.crowd_control_diminishing_returns.window_ms",
        yaml.window_ms,
    )?;
    if yaml.stages_bps.len() != 4 {
        return Err(validation_error(
            source,
            "simulation.crowd_control_diminishing_returns.stages_bps must contain exactly four entries",
        ));
    }
    let mut stages = [0_u16; 4];
    for (index, value) in yaml.stages_bps.into_iter().enumerate() {
        validate_basis_points(
            source,
            &format!("simulation.crowd_control_diminishing_returns.stages_bps[{index}]"),
            value,
        )?;
        stages[index] = value;
    }
    Ok(CrowdControlDiminishingReturns {
        window_ms: yaml.window_ms,
        stages_bps: stages,
    })
}

fn parse_training_dummy_configuration(
    source: &str,
    yaml: TrainingDummyConfigurationYaml,
) -> Result<TrainingDummyConfiguration, ContentError> {
    require_positive_u16(
        source,
        "simulation.training_dummy.base_hit_points",
        yaml.base_hit_points,
    )?;
    require_positive_u16(
        source,
        "simulation.training_dummy.health_multiplier",
        yaml.health_multiplier,
    )?;
    validate_basis_points(
        source,
        "simulation.training_dummy.execute_threshold_bps",
        yaml.execute_threshold_bps,
    )?;
    Ok(TrainingDummyConfiguration {
        base_hit_points: yaml.base_hit_points,
        health_multiplier: yaml.health_multiplier,
        execute_threshold_bps: yaml.execute_threshold_bps,
    })
}

fn parse_class_profiles(
    source: &str,
    classes: BTreeMap<String, ClassProfileYaml>,
) -> Result<BTreeMap<SkillTree, ClassProfile>, ContentError> {
    if classes.is_empty() {
        return Err(validation_error(
            source,
            "classes must contain at least one authored class profile",
        ));
    }

    let mut parsed = BTreeMap::new();
    for (tree_name, profile) in classes {
        let tree = SkillTree::new(tree_name.clone()).map_err(|error| ContentError::Validation {
            source: String::from(source),
            message: format!("classes.{tree_name}: {error}"),
        })?;
        require_positive_u16(
            source,
            &format!("classes.{tree_name}.hit_points"),
            profile.hit_points,
        )?;
        require_positive_u16(
            source,
            &format!("classes.{tree_name}.max_mana"),
            profile.max_mana,
        )?;
        require_positive_u16(
            source,
            &format!("classes.{tree_name}.move_speed_units_per_second"),
            profile.move_speed_units_per_second,
        )?;
        if parsed
            .insert(
                tree,
                ClassProfile {
                    hit_points: profile.hit_points,
                    max_mana: profile.max_mana,
                    move_speed_units_per_second: profile.move_speed_units_per_second,
                },
            )
            .is_some()
        {
            return Err(validation_error(
                source,
                format!("classes contains a duplicate tree profile '{tree_name}'"),
            ));
        }
    }

    Ok(parsed)
}

fn require_valid_round_number(source: &str, field: &str, value: u8) -> Result<(), ContentError> {
    RoundNumber::new(value)
        .map(|_| ())
        .map_err(|error| ContentError::Validation {
            source: String::from(source),
            message: format!("{field}: {error}"),
        })
}

fn require_positive_u8(source: &str, field: &str, value: u8) -> Result<(), ContentError> {
    if value == 0 {
        return Err(validation_error(
            source,
            format!("{field} must be greater than zero"),
        ));
    }
    Ok(())
}

fn require_positive_u16(source: &str, field: &str, value: u16) -> Result<(), ContentError> {
    if value == 0 {
        return Err(validation_error(
            source,
            format!("{field} must be greater than zero"),
        ));
    }
    Ok(())
}

fn require_positive_u32(source: &str, field: &str, value: u32) -> Result<(), ContentError> {
    if value == 0 {
        return Err(validation_error(
            source,
            format!("{field} must be greater than zero"),
        ));
    }
    Ok(())
}

fn validate_percent(source: &str, field: &str, value: u8) -> Result<(), ContentError> {
    if value > 100 {
        return Err(validation_error(
            source,
            format!("{field} must be between 0 and 100"),
        ));
    }
    Ok(())
}

fn validate_basis_points(source: &str, field: &str, value: u16) -> Result<(), ContentError> {
    if value > 10_000 {
        return Err(validation_error(
            source,
            format!("{field} must be between 0 and 10000 basis points"),
        ));
    }
    Ok(())
}

fn validate_ordered_i16_range(
    source: &str,
    field: &str,
    min_value: i16,
    max_value: i16,
) -> Result<(), ContentError> {
    if min_value > max_value {
        return Err(validation_error(
            source,
            format!("{field} min must be <= max"),
        ));
    }
    Ok(())
}

fn validate_ordered_u16_range(
    source: &str,
    field: &str,
    min_value: u16,
    max_value: u16,
) -> Result<(), ContentError> {
    if min_value == 0 || min_value > max_value {
        return Err(validation_error(
            source,
            format!("{field} must be positive and ascending"),
        ));
    }
    Ok(())
}

fn validation_error(source: &str, message: impl Into<String>) -> ContentError {
    ContentError::Validation {
        source: String::from(source),
        message: message.into(),
    }
}
