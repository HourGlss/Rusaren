use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use game_domain::{SkillChoice, SkillTree};

use super::{
    load_skill_catalog_from_pairs_with_mechanics, parse_ascii_map, parse_configuration_yaml,
    parse_mechanics_yaml, read_skill_file_pairs, workspace_content_root, ContentError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillEffectKind {
    MeleeSwing,
    SkillShot,
    DashTrail,
    Burst,
    Nova,
    Beam,
    HitSpark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatValueKind {
    Damage,
    Heal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MechanicCategory {
    Behavior,
    Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericFieldRule {
    Required,
    Optional,
    NonNegative,
    Zero,
    Forbidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadFieldRule {
    Required,
    Optional,
    Forbidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackRule {
    Positive,
    One,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorSchema {
    pub numeric_fields: BTreeMap<String, NumericFieldRule>,
    pub payload: PayloadFieldRule,
    pub cast_start_payload: PayloadFieldRule,
    pub cast_end_payload: PayloadFieldRule,
    pub allowed_effects: Vec<SkillEffectKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusSchema {
    pub numeric_fields: BTreeMap<String, NumericFieldRule>,
    pub max_stacks: StackRule,
    pub expire_payload: PayloadFieldRule,
    pub dispel_payload: PayloadFieldRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Poison,
    Hot,
    Chill,
    Root,
    Haste,
    Silence,
    Stun,
    Sleep,
    Shield,
    Stealth,
    Reveal,
    Fear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispelScope {
    Positive,
    Negative,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispelDefinition {
    pub scope: DispelScope,
    pub max_statuses: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusDefinition {
    pub kind: StatusKind,
    pub duration_ms: u16,
    pub tick_interval_ms: Option<u16>,
    pub magnitude: u16,
    pub max_stacks: u8,
    pub trigger_duration_ms: Option<u16>,
    pub expire_payload: Option<Box<EffectPayload>>,
    pub dispel_payload: Option<Box<EffectPayload>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectPayload {
    pub kind: CombatValueKind,
    pub amount: u16,
    pub status: Option<StatusDefinition>,
    pub interrupt_silence_duration_ms: Option<u16>,
    pub dispel: Option<DispelDefinition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeleeDefinition {
    pub tree: SkillTree,
    pub id: String,
    pub name: String,
    pub description: String,
    pub audio_cue_id: Option<String>,
    pub cooldown_ms: u16,
    pub range: u16,
    pub radius: u16,
    pub effect: SkillEffectKind,
    pub payload: EffectPayload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillBehavior {
    Projectile {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        speed: u16,
        range: u16,
        radius: u16,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
    Beam {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        range: u16,
        radius: u16,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
    Dash {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        distance: u16,
        effect: SkillEffectKind,
        impact_radius: Option<u16>,
        payload: Option<EffectPayload>,
    },
    Burst {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        range: u16,
        radius: u16,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
    Nova {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        radius: u16,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
    Teleport {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        distance: u16,
        effect: SkillEffectKind,
    },
    Channel {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        range: u16,
        radius: u16,
        duration_ms: u16,
        tick_interval_ms: u16,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
    Passive {
        player_speed_bps: u16,
        projectile_speed_bps: u16,
        cooldown_bps: u16,
        cast_time_bps: u16,
    },
    Summon {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: u16,
        range: u16,
        tick_interval_ms: u16,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
    Ward {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: u16,
        effect: SkillEffectKind,
    },
    Trap {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: u16,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
    Barrier {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: u16,
        effect: SkillEffectKind,
    },
    Aura {
        cooldown_ms: u16,
        cast_time_ms: u16,
        mana_cost: u16,
        distance: u16,
        radius: u16,
        duration_ms: u16,
        hit_points: Option<u16>,
        toggleable: bool,
        tick_interval_ms: u16,
        cast_start_payload: Option<EffectPayload>,
        cast_end_payload: Option<EffectPayload>,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
}

impl SkillBehavior {
    #[must_use]
    pub const fn cooldown_ms(&self) -> u16 {
        match self {
            Self::Projectile { cooldown_ms, .. }
            | Self::Beam { cooldown_ms, .. }
            | Self::Dash { cooldown_ms, .. }
            | Self::Burst { cooldown_ms, .. }
            | Self::Nova { cooldown_ms, .. }
            | Self::Teleport { cooldown_ms, .. }
            | Self::Channel { cooldown_ms, .. }
            | Self::Summon { cooldown_ms, .. }
            | Self::Ward { cooldown_ms, .. }
            | Self::Trap { cooldown_ms, .. }
            | Self::Barrier { cooldown_ms, .. }
            | Self::Aura { cooldown_ms, .. } => *cooldown_ms,
            Self::Passive { .. } => 0,
        }
    }

    #[must_use]
    pub const fn cast_time_ms(&self) -> u16 {
        match self {
            Self::Projectile { cast_time_ms, .. }
            | Self::Beam { cast_time_ms, .. }
            | Self::Dash { cast_time_ms, .. }
            | Self::Burst { cast_time_ms, .. }
            | Self::Nova { cast_time_ms, .. }
            | Self::Teleport { cast_time_ms, .. }
            | Self::Channel { cast_time_ms, .. }
            | Self::Summon { cast_time_ms, .. }
            | Self::Ward { cast_time_ms, .. }
            | Self::Trap { cast_time_ms, .. }
            | Self::Barrier { cast_time_ms, .. }
            | Self::Aura { cast_time_ms, .. } => *cast_time_ms,
            Self::Passive { .. } => 0,
        }
    }

    #[must_use]
    pub const fn mana_cost(&self) -> u16 {
        match self {
            Self::Projectile { mana_cost, .. }
            | Self::Beam { mana_cost, .. }
            | Self::Dash { mana_cost, .. }
            | Self::Burst { mana_cost, .. }
            | Self::Nova { mana_cost, .. }
            | Self::Teleport { mana_cost, .. }
            | Self::Channel { mana_cost, .. }
            | Self::Summon { mana_cost, .. }
            | Self::Ward { mana_cost, .. }
            | Self::Trap { mana_cost, .. }
            | Self::Barrier { mana_cost, .. }
            | Self::Aura { mana_cost, .. } => *mana_cost,
            Self::Passive { .. } => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillDefinition {
    pub tree: SkillTree,
    pub tier: u8,
    pub id: String,
    pub name: String,
    pub description: String,
    pub audio_cue_id: Option<String>,
    pub behavior: SkillBehavior,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDefinition {
    pub tree: SkillTree,
    pub melee: MeleeDefinition,
    pub skills: Vec<SkillDefinition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillCatalog {
    by_choice: BTreeMap<(SkillTree, u8), SkillDefinition>,
    melee_by_tree: BTreeMap<SkillTree, MeleeDefinition>,
}

impl SkillCatalog {
    pub(super) fn new(
        by_choice: BTreeMap<(SkillTree, u8), SkillDefinition>,
        melee_by_tree: BTreeMap<SkillTree, MeleeDefinition>,
    ) -> Self {
        Self {
            by_choice,
            melee_by_tree,
        }
    }

    #[must_use]
    pub fn resolve(&self, choice: &SkillChoice) -> Option<&SkillDefinition> {
        self.by_choice.get(&(choice.tree.clone(), choice.tier))
    }

    #[must_use]
    pub fn melee_for(&self, tree: &SkillTree) -> Option<&MeleeDefinition> {
        self.melee_by_tree.get(tree)
    }

    pub fn all(&self) -> impl Iterator<Item = &SkillDefinition> {
        self.by_choice.values()
    }

    pub fn trees(&self) -> impl Iterator<Item = &SkillTree> {
        self.melee_by_tree.keys()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaMapObstacleKind {
    Pillar,
    Shrub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaMapFeatureKind {
    TrainingDummyResetFull,
    TrainingDummyExecute,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaMapObstacle {
    pub kind: ArenaMapObstacleKind,
    pub center_x: i16,
    pub center_y: i16,
    pub half_width: u16,
    pub half_height: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaMapFeature {
    pub kind: ArenaMapFeatureKind,
    pub center_x: i16,
    pub center_y: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaMapDefinition {
    pub map_id: String,
    pub width_tiles: u16,
    pub height_tiles: u16,
    pub tile_units: u16,
    pub width_units: u16,
    pub height_units: u16,
    pub objective_target_ms: u32,
    pub footprint_mask: Vec<u8>,
    pub objective_mask: Vec<u8>,
    pub team_a_anchors: Vec<(i16, i16)>,
    pub team_b_anchors: Vec<(i16, i16)>,
    pub obstacles: Vec<ArenaMapObstacle>,
    pub features: Vec<ArenaMapFeature>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LobbyConfiguration {
    pub launch_countdown_seconds: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchConfiguration {
    pub total_rounds: u8,
    pub skill_pick_seconds: u8,
    pub pre_combat_seconds: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapGenerationStyle {
    pub shrub_clusters: usize,
    pub shrub_radius_tiles: i32,
    pub shrub_soft_radius_tiles: i32,
    pub shrub_fill_percent: u8,
    pub wall_segments: usize,
    pub isolated_pillars: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapGenerationConfiguration {
    pub max_generation_attempts: usize,
    pub protected_tile_buffer_radius_tiles: i32,
    pub obstacle_edge_padding_tiles: i32,
    pub wall_segment_lengths_tiles: [i32; 2],
    pub long_wall_percent: u8,
    pub wall_candidate_skip_percent: u8,
    pub wall_min_spacing_manhattan_tiles: i32,
    pub pillar_candidate_skip_percent: u8,
    pub pillar_min_spacing_manhattan_tiles: i32,
    pub styles: Vec<MapGenerationStyle>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapsConfiguration {
    pub tile_units: u16,
    pub objective_target_ms_by_map: BTreeMap<String, u32>,
    pub generation: MapGenerationConfiguration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::struct_field_names)]
pub struct PassiveBonusCaps {
    pub player_speed_bps: u16,
    pub projectile_speed_bps: u16,
    pub cooldown_bps: u16,
    pub cast_time_bps: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::struct_field_names)]
pub struct MovementModifierCaps {
    pub chill_bps: u16,
    pub haste_bps: u16,
    pub status_total_min_bps: i16,
    pub status_total_max_bps: i16,
    pub overall_total_min_bps: i16,
    pub overall_total_max_bps: i16,
    pub effective_scale_min_bps: u16,
    pub effective_scale_max_bps: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrowdControlDiminishingReturns {
    pub window_ms: u16,
    pub stages_bps: [u16; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainingDummyConfiguration {
    pub base_hit_points: u16,
    pub health_multiplier: u16,
    pub execute_threshold_bps: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimulationConfiguration {
    pub combat_frame_ms: u16,
    pub player_radius_units: u16,
    pub vision_radius_units: u16,
    pub spawn_spacing_units: i16,
    pub default_aim_x_units: i16,
    pub default_aim_y_units: i16,
    pub mana_regen_per_second: u16,
    pub global_projectile_speed_bonus_bps: u16,
    pub teleport_resolution_steps: u16,
    pub movement_audio_step_interval_ms: u16,
    pub movement_audio_radius_units: u16,
    pub stealth_audio_radius_units: u16,
    pub brush_movement_audible_percent: u8,
    pub passive_bonus_caps: PassiveBonusCaps,
    pub movement_modifier_caps: MovementModifierCaps,
    pub crowd_control_diminishing_returns: CrowdControlDiminishingReturns,
    pub training_dummy: TrainingDummyConfiguration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassProfile {
    pub hit_points: u16,
    pub max_mana: u16,
    pub move_speed_units_per_second: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameConfiguration {
    pub lobby: LobbyConfiguration,
    pub match_flow: MatchConfiguration,
    pub maps: MapsConfiguration,
    pub simulation: SimulationConfiguration,
    pub classes: BTreeMap<SkillTree, ClassProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MechanicDefinition {
    pub id: String,
    pub label: String,
    pub category: MechanicCategory,
    pub implemented: bool,
    pub inspiration: String,
    pub notes: String,
    pub behavior_schema: Option<BehaviorSchema>,
    pub status_schema: Option<StatusSchema>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MechanicCatalog {
    pub behaviors: Vec<MechanicDefinition>,
    pub statuses: Vec<MechanicDefinition>,
}

impl MechanicCatalog {
    #[must_use]
    pub fn behavior(&self, id: &str) -> Option<&MechanicDefinition> {
        self.behaviors.iter().find(|mechanic| mechanic.id == id)
    }

    #[must_use]
    pub fn status(&self, id: &str) -> Option<&MechanicDefinition> {
        self.statuses.iter().find(|mechanic| mechanic.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameContent {
    skills: SkillCatalog,
    maps: BTreeMap<String, ArenaMapDefinition>,
    default_map_id: String,
    training_map_id: Option<String>,
    mechanics: MechanicCatalog,
    configuration: GameConfiguration,
}

impl GameContent {
    pub fn bundled() -> Result<Self, ContentError> {
        Self::load_from_root(workspace_content_root())
    }

    #[allow(clippy::too_many_lines)]
    pub fn load_from_root(root: impl AsRef<Path>) -> Result<Self, ContentError> {
        let root = root.as_ref();
        let mechanics_path = root.join("mechanics").join("registry.yaml");
        let mechanics_yaml =
            fs::read_to_string(&mechanics_path).map_err(|error| ContentError::Io {
                path: mechanics_path.clone(),
                message: error.to_string(),
            })?;
        let mechanics =
            parse_mechanics_yaml(&mechanics_path.display().to_string(), &mechanics_yaml)?;

        let configuration_path = root.join("config").join("configurations.yaml");
        let configuration_yaml =
            fs::read_to_string(&configuration_path).map_err(|error| ContentError::Io {
                path: configuration_path.clone(),
                message: error.to_string(),
            })?;
        let configuration = parse_configuration_yaml(
            &configuration_path.display().to_string(),
            &configuration_yaml,
        )?;

        let pairs = read_skill_file_pairs(root)?;
        let owned_pairs = pairs
            .iter()
            .map(|(source, yaml)| (source.as_str(), yaml.as_str()))
            .collect::<Vec<_>>();
        let skills = load_skill_catalog_from_pairs_with_mechanics(&owned_pairs, &mechanics)?;
        for tree in skills.trees() {
            if !configuration.classes.contains_key(tree) {
                return Err(ContentError::Validation {
                    source: configuration_path.display().to_string(),
                    message: format!(
                        "classes is missing a profile for authored tree '{}'",
                        tree.as_str()
                    ),
                });
            }
        }

        let maps_dir = root.join("maps");
        let mut map_paths = fs::read_dir(&maps_dir)
            .map_err(|error| ContentError::Io {
                path: maps_dir.clone(),
                message: error.to_string(),
            })?
            .filter_map(|entry| entry.ok().map(|value| value.path()))
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
            })
            .collect::<Vec<_>>();
        map_paths.sort();
        if map_paths.is_empty() {
            return Err(ContentError::Validation {
                source: maps_dir.display().to_string(),
                message: String::from("no ASCII map files were found"),
            });
        }

        let mut maps = BTreeMap::new();
        for path in map_paths {
            let map_text = fs::read_to_string(&path).map_err(|error| ContentError::Io {
                path: path.clone(),
                message: error.to_string(),
            })?;
            let mut map = parse_ascii_map(
                &path.display().to_string(),
                &map_text,
                configuration.maps.tile_units,
            )?;
            let Some(objective_target_ms) = configuration
                .maps
                .objective_target_ms_by_map
                .get(&map.map_id)
                .copied()
            else {
                return Err(ContentError::Validation {
                    source: String::from("config/configurations.yaml"),
                    message: format!(
                        "map registry is missing objective_target_ms for '{}'",
                        map.map_id
                    ),
                });
            };
            map.objective_target_ms = objective_target_ms;
            if maps.insert(map.map_id.clone(), map).is_some() {
                return Err(ContentError::Validation {
                    source: path.display().to_string(),
                    message: String::from("duplicate map id"),
                });
            }
        }
        if !maps.contains_key("prototype_arena") {
            return Err(ContentError::Validation {
                source: maps_dir.display().to_string(),
                message: String::from("maps must include prototype_arena.txt"),
            });
        }
        for map_id in configuration.maps.objective_target_ms_by_map.keys() {
            if !maps.contains_key(map_id) {
                return Err(ContentError::Validation {
                    source: configuration_path.display().to_string(),
                    message: format!(
                        "maps.objective_target_ms_by_map contains unknown map '{map_id}'"
                    ),
                });
            }
        }
        let training_map_id = maps
            .contains_key("training_arena")
            .then_some(String::from("training_arena"));

        Ok(Self {
            skills,
            maps,
            default_map_id: String::from("prototype_arena"),
            training_map_id,
            mechanics,
            configuration,
        })
    }

    #[must_use]
    pub const fn skills(&self) -> &SkillCatalog {
        &self.skills
    }

    #[must_use]
    pub fn map(&self) -> &ArenaMapDefinition {
        let Some(map) = self.maps.get(&self.default_map_id) else {
            panic!("default map id should always exist");
        };
        map
    }

    #[must_use]
    pub fn training_map(&self) -> Option<&ArenaMapDefinition> {
        self.training_map_id
            .as_ref()
            .and_then(|map_id| self.maps.get(map_id))
    }

    #[must_use]
    pub fn map_by_id(&self, map_id: &str) -> Option<&ArenaMapDefinition> {
        self.maps.get(map_id)
    }

    pub fn maps(&self) -> impl Iterator<Item = &ArenaMapDefinition> {
        self.maps.values()
    }

    #[must_use]
    pub const fn mechanics(&self) -> &MechanicCatalog {
        &self.mechanics
    }

    #[must_use]
    pub const fn configuration(&self) -> &GameConfiguration {
        &self.configuration
    }

    #[must_use]
    pub fn class_profile(&self, tree: &SkillTree) -> Option<&ClassProfile> {
        self.configuration.classes.get(tree)
    }
}
