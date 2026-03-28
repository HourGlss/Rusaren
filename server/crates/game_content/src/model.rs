use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use game_domain::{SkillChoice, SkillTree};

use super::{
    load_skill_catalog_from_pairs_with_mechanics, parse_ascii_map, parse_mechanics_yaml,
    read_skill_file_pairs, workspace_content_root, ContentError,
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
        tick_interval_ms: u16,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaMapObstacleKind {
    Pillar,
    Shrub,
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
pub struct ArenaMapDefinition {
    pub map_id: String,
    pub width_tiles: u16,
    pub height_tiles: u16,
    pub tile_units: u16,
    pub width_units: u16,
    pub height_units: u16,
    pub team_a_anchor: (i16, i16),
    pub team_b_anchor: (i16, i16),
    pub obstacles: Vec<ArenaMapObstacle>,
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
    map: ArenaMapDefinition,
    mechanics: MechanicCatalog,
}

impl GameContent {
    pub fn bundled() -> Result<Self, ContentError> {
        Self::load_from_root(workspace_content_root())
    }

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

        let pairs = read_skill_file_pairs(root)?;
        let owned_pairs = pairs
            .iter()
            .map(|(source, yaml)| (source.as_str(), yaml.as_str()))
            .collect::<Vec<_>>();
        let skills = load_skill_catalog_from_pairs_with_mechanics(&owned_pairs, &mechanics)?;

        let map_path = root.join("maps").join("prototype_arena.txt");
        let map_text = fs::read_to_string(&map_path).map_err(|error| ContentError::Io {
            path: map_path.clone(),
            message: error.to_string(),
        })?;
        let map = parse_ascii_map(&map_path.display().to_string(), &map_text)?;

        Ok(Self {
            skills,
            map,
            mechanics,
        })
    }

    #[must_use]
    pub const fn skills(&self) -> &SkillCatalog {
        &self.skills
    }

    #[must_use]
    pub const fn map(&self) -> &ArenaMapDefinition {
        &self.map
    }

    #[must_use]
    pub const fn mechanics(&self) -> &MechanicCatalog {
        &self.mechanics
    }
}
