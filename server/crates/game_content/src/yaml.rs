use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SkillFileYaml {
    pub(super) tree: String,
    pub(super) melee: MeleeYaml,
    pub(super) skills: Vec<SkillYaml>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MechanicsFileYaml {
    pub(super) behaviors: Vec<MechanicYaml>,
    pub(super) statuses: Vec<MechanicYaml>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MechanicYaml {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) implemented: bool,
    pub(super) inspiration: String,
    pub(super) notes: String,
    pub(super) schema: Option<MechanicSchemaYaml>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MechanicSchemaYaml {
    #[serde(default)]
    pub(super) numeric_fields: BTreeMap<String, NumericRuleYaml>,
    #[serde(default)]
    pub(super) payload: PayloadRuleYaml,
    #[serde(default)]
    pub(super) cast_start_payload: PayloadRuleYaml,
    #[serde(default)]
    pub(super) cast_end_payload: PayloadRuleYaml,
    #[serde(default)]
    pub(super) expire_payload: PayloadRuleYaml,
    #[serde(default)]
    pub(super) dispel_payload: PayloadRuleYaml,
    #[serde(default)]
    pub(super) allowed_effects: Vec<String>,
    #[serde(default)]
    pub(super) max_stacks: StackRuleYaml,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NumericRuleYaml {
    Required,
    Optional,
    NonNegative,
    Zero,
    #[default]
    Forbidden,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PayloadRuleYaml {
    Required,
    Optional,
    #[default]
    Forbidden,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StackRuleYaml {
    Positive,
    #[default]
    One,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MeleeYaml {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) audio_cue_id: Option<String>,
    pub(super) cooldown_ms: u16,
    pub(super) range: u16,
    pub(super) radius: u16,
    pub(super) effect: String,
    pub(super) payload: EffectPayloadYaml,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SkillYaml {
    pub(super) tier: u8,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) audio_cue_id: Option<String>,
    pub(super) behavior: SkillBehaviorYaml,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EffectPayloadYaml {
    pub(super) kind: String,
    pub(super) amount: Option<u16>,
    pub(super) amount_min: Option<u16>,
    pub(super) amount_max: Option<u16>,
    pub(super) crit_chance_bps: Option<u16>,
    pub(super) crit_multiplier_bps: Option<u16>,
    pub(super) status: Option<StatusYaml>,
    pub(super) interrupt_silence_duration_ms: Option<u16>,
    pub(super) dispel: Option<DispelYaml>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DispelYaml {
    pub(super) scope: String,
    pub(super) max_statuses: Option<u8>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StatusYaml {
    pub(super) kind: String,
    pub(super) duration_ms: u16,
    pub(super) tick_interval_ms: Option<u16>,
    pub(super) magnitude: u16,
    pub(super) max_stacks: Option<u8>,
    pub(super) trigger_duration_ms: Option<u16>,
    pub(super) expire_payload: Option<Box<EffectPayloadYaml>>,
    pub(super) dispel_payload: Option<Box<EffectPayloadYaml>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SkillBehaviorYaml {
    pub(super) kind: String,
    pub(super) effect: String,
    pub(super) cooldown_ms: Option<u16>,
    pub(super) cast_time_ms: Option<u16>,
    pub(super) mana_cost: Option<u16>,
    pub(super) range: Option<u16>,
    pub(super) radius: Option<u16>,
    pub(super) distance: Option<u16>,
    pub(super) speed: Option<u16>,
    pub(super) impact_radius: Option<u16>,
    pub(super) duration_ms: Option<u16>,
    pub(super) hit_points: Option<u16>,
    pub(super) tick_interval_ms: Option<u16>,
    pub(super) player_speed_bps: Option<u16>,
    pub(super) projectile_speed_bps: Option<u16>,
    pub(super) cooldown_bps: Option<u16>,
    pub(super) cast_time_bps: Option<u16>,
    pub(super) proc_reset: Option<ProcResetYaml>,
    pub(super) toggleable: Option<bool>,
    pub(super) cast_start_payload: Option<EffectPayloadYaml>,
    pub(super) cast_end_payload: Option<EffectPayloadYaml>,
    pub(super) payload: Option<EffectPayloadYaml>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProcResetYaml {
    pub(super) trigger: String,
    pub(super) source_skill_ids: Option<Vec<String>>,
    pub(super) reset_skill_ids: Option<Vec<String>>,
    pub(super) instacast_skill_ids: Option<Vec<String>>,
    pub(super) instacast_costs_mana: Option<bool>,
    pub(super) instacast_starts_cooldown: Option<bool>,
    pub(super) internal_cooldown_ms: Option<u16>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConfigurationsYaml {
    pub(super) lobby: LobbyConfigurationYaml,
    #[serde(rename = "match")]
    pub(super) match_flow: MatchConfigurationYaml,
    pub(super) maps: MapsConfigurationYaml,
    pub(super) simulation: SimulationConfigurationYaml,
    pub(super) classes: BTreeMap<String, ClassProfileYaml>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LobbyConfigurationYaml {
    pub(super) launch_countdown_seconds: u8,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MatchConfigurationYaml {
    pub(super) total_rounds: u8,
    pub(super) skill_pick_seconds: u8,
    pub(super) pre_combat_seconds: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MapsConfigurationYaml {
    pub(super) tile_units: u16,
    pub(super) objective_target_ms_by_map: BTreeMap<String, u32>,
    pub(super) generation: MapGenerationConfigurationYaml,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MapGenerationConfigurationYaml {
    pub(super) max_generation_attempts: usize,
    pub(super) protected_tile_buffer_radius_tiles: i32,
    pub(super) obstacle_edge_padding_tiles: i32,
    pub(super) wall_segment_lengths_tiles: Vec<i32>,
    pub(super) long_wall_percent: u8,
    pub(super) wall_candidate_skip_percent: u8,
    pub(super) wall_min_spacing_manhattan_tiles: i32,
    pub(super) pillar_candidate_skip_percent: u8,
    pub(super) pillar_min_spacing_manhattan_tiles: i32,
    pub(super) styles: Vec<MapGenerationStyleYaml>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MapGenerationStyleYaml {
    pub(super) shrub_clusters: usize,
    pub(super) shrub_radius_tiles: i32,
    pub(super) shrub_soft_radius_tiles: i32,
    pub(super) shrub_fill_percent: u8,
    pub(super) wall_segments: usize,
    pub(super) isolated_pillars: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SimulationConfigurationYaml {
    pub(super) combat_frame_ms: u16,
    pub(super) player_radius_units: u16,
    pub(super) vision_radius_units: u16,
    pub(super) spawn_spacing_units: i16,
    pub(super) default_aim_x_units: i16,
    pub(super) default_aim_y_units: i16,
    pub(super) mana_regen_per_second: u16,
    pub(super) global_projectile_speed_bonus_bps: u16,
    pub(super) teleport_resolution_steps: u16,
    pub(super) movement_audio_step_interval_ms: u16,
    pub(super) movement_audio_radius_units: u16,
    pub(super) stealth_audio_radius_units: u16,
    pub(super) brush_movement_audible_percent: u8,
    pub(super) passive_bonus_caps: PassiveBonusCapsYaml,
    pub(super) movement_modifier_caps: MovementModifierCapsYaml,
    pub(super) crowd_control_diminishing_returns: CrowdControlDiminishingReturnsYaml,
    pub(super) training_dummy: TrainingDummyConfigurationYaml,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)]
pub(super) struct PassiveBonusCapsYaml {
    pub(super) player_speed_bps: u16,
    pub(super) projectile_speed_bps: u16,
    pub(super) cooldown_bps: u16,
    pub(super) cast_time_bps: u16,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)]
pub(super) struct MovementModifierCapsYaml {
    pub(super) chill_bps: u16,
    pub(super) haste_bps: u16,
    pub(super) status_total_min_bps: i16,
    pub(super) status_total_max_bps: i16,
    pub(super) overall_total_min_bps: i16,
    pub(super) overall_total_max_bps: i16,
    pub(super) effective_scale_min_bps: u16,
    pub(super) effective_scale_max_bps: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CrowdControlDiminishingReturnsYaml {
    pub(super) window_ms: u16,
    pub(super) stages_bps: Vec<u16>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrainingDummyConfigurationYaml {
    pub(super) base_hit_points: u16,
    pub(super) health_multiplier: u16,
    pub(super) execute_threshold_bps: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClassProfileYaml {
    pub(super) hit_points: u16,
    pub(super) max_mana: u16,
    pub(super) move_speed_units_per_second: u16,
}
