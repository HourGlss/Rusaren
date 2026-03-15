//! Data loading and validation for authored YAML skills and ASCII maps.
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use game_domain::{SkillChoice, SkillTree};
use serde::Deserialize;

const BUNDLED_SKILL_FILES: [(&str, &str); 4] = [
    (
        "skills/warrior.yaml",
        include_str!("../../../content/skills/warrior.yaml"),
    ),
    (
        "skills/mage.yaml",
        include_str!("../../../content/skills/mage.yaml"),
    ),
    (
        "skills/rogue.yaml",
        include_str!("../../../content/skills/rogue.yaml"),
    ),
    (
        "skills/cleric.yaml",
        include_str!("../../../content/skills/cleric.yaml"),
    ),
];
const BUNDLED_MAP_FILE: (&str, &str) = (
    "maps/prototype_arena.txt",
    include_str!("../../../content/maps/prototype_arena.txt"),
);
const DEFAULT_TILE_UNITS: u16 = 50;
const MAX_MAP_DIMENSION_TILES: usize = 128;
const MAX_SKILL_TEXT_LEN: usize = 120;
const REQUIRED_TIERS: [u8; 5] = [1, 2, 3, 4, 5];

type AnchorPoint = (i16, i16);

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
pub enum StatusKind {
    Poison,
    Hot,
    Chill,
    Root,
    Haste,
    Silence,
    Stun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusDefinition {
    pub kind: StatusKind,
    pub duration_ms: u16,
    pub tick_interval_ms: Option<u16>,
    pub magnitude: u16,
    pub max_stacks: u8,
    pub trigger_duration_ms: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectPayload {
    pub kind: CombatValueKind,
    pub amount: u16,
    pub status: Option<StatusDefinition>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillBehavior {
    Projectile {
        cooldown_ms: u16,
        mana_cost: u16,
        speed: u16,
        range: u16,
        radius: u16,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
    Beam {
        cooldown_ms: u16,
        mana_cost: u16,
        range: u16,
        radius: u16,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
    Dash {
        cooldown_ms: u16,
        mana_cost: u16,
        distance: u16,
        effect: SkillEffectKind,
        impact_radius: Option<u16>,
        payload: Option<EffectPayload>,
    },
    Burst {
        cooldown_ms: u16,
        mana_cost: u16,
        range: u16,
        radius: u16,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
    Nova {
        cooldown_ms: u16,
        mana_cost: u16,
        radius: u16,
        effect: SkillEffectKind,
        payload: EffectPayload,
    },
}

impl SkillBehavior {
    #[must_use]
    pub const fn cooldown_ms(self) -> u16 {
        match self {
            Self::Projectile { cooldown_ms, .. }
            | Self::Beam { cooldown_ms, .. }
            | Self::Dash { cooldown_ms, .. }
            | Self::Burst { cooldown_ms, .. }
            | Self::Nova { cooldown_ms, .. } => cooldown_ms,
        }
    }

    #[must_use]
    pub const fn mana_cost(self) -> u16 {
        match self {
            Self::Projectile { mana_cost, .. }
            | Self::Beam { mana_cost, .. }
            | Self::Dash { mana_cost, .. }
            | Self::Burst { mana_cost, .. }
            | Self::Nova { mana_cost, .. } => mana_cost,
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
    by_choice: BTreeMap<(usize, u8), SkillDefinition>,
    melee_by_tree: BTreeMap<usize, MeleeDefinition>,
}

impl SkillCatalog {
    #[must_use]
    pub fn resolve(&self, choice: SkillChoice) -> Option<&SkillDefinition> {
        self.by_choice.get(&(choice.tree.as_index(), choice.tier))
    }

    #[must_use]
    pub fn melee_for(&self, tree: SkillTree) -> Option<&MeleeDefinition> {
        self.melee_by_tree.get(&tree.as_index())
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
pub struct GameContent {
    skills: SkillCatalog,
    map: ArenaMapDefinition,
}

impl GameContent {
    pub fn bundled() -> Result<Self, ContentError> {
        let skills = load_skill_catalog_from_pairs(&BUNDLED_SKILL_FILES)?;
        let map = parse_ascii_map(BUNDLED_MAP_FILE.0, BUNDLED_MAP_FILE.1)?;
        Ok(Self { skills, map })
    }

    pub fn load_from_root(root: impl AsRef<Path>) -> Result<Self, ContentError> {
        let root = root.as_ref();
        let skills_dir = root.join("skills");
        let mut yaml_paths = fs::read_dir(&skills_dir)
            .map_err(|error| ContentError::Io {
                path: skills_dir.clone(),
                message: error.to_string(),
            })?
            .filter_map(|entry| entry.ok().map(|value| value.path()))
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("yaml"))
            })
            .collect::<Vec<_>>();
        yaml_paths.sort();
        if yaml_paths.is_empty() {
            return Err(ContentError::Validation {
                source: skills_dir.display().to_string(),
                message: String::from("no skill YAML files were found"),
            });
        }

        let mut pairs = Vec::with_capacity(yaml_paths.len());
        for path in yaml_paths {
            let yaml = fs::read_to_string(&path).map_err(|error| ContentError::Io {
                path: path.clone(),
                message: error.to_string(),
            })?;
            pairs.push((path.display().to_string(), yaml));
        }
        let owned_pairs = pairs
            .iter()
            .map(|(source, yaml)| (source.as_str(), yaml.as_str()))
            .collect::<Vec<_>>();
        let skills = load_skill_catalog_from_pairs(&owned_pairs)?;

        let map_path = root.join("maps").join("prototype_arena.txt");
        let map_text = fs::read_to_string(&map_path).map_err(|error| ContentError::Io {
            path: map_path.clone(),
            message: error.to_string(),
        })?;
        let map = parse_ascii_map(&map_path.display().to_string(), &map_text)?;

        Ok(Self { skills, map })
    }

    #[must_use]
    pub const fn skills(&self) -> &SkillCatalog {
        &self.skills
    }

    #[must_use]
    pub const fn map(&self) -> &ArenaMapDefinition {
        &self.map
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentError {
    Io { path: PathBuf, message: String },
    Parse { source: String, message: String },
    Validation { source: String, message: String },
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(
                    f,
                    "failed to read content file {}: {message}",
                    path.display()
                )
            }
            Self::Parse { source, message } => write!(f, "failed to parse {source}: {message}"),
            Self::Validation { source, message } => {
                write!(f, "invalid content in {source}: {message}")
            }
        }
    }
}

impl std::error::Error for ContentError {}

pub fn parse_ascii_map(source: &str, ascii_map: &str) -> Result<ArenaMapDefinition, ContentError> {
    let rows = collect_map_rows(source, ascii_map)?;
    let (width_tiles, height_tiles, width_units, height_units) =
        validate_map_dimensions(source, &rows)?;
    let (team_a_anchor, team_b_anchor, obstacles) =
        parse_map_layout(source, &rows, width_tiles, height_tiles)?;

    Ok(ArenaMapDefinition {
        map_id: map_identifier(source),
        width_tiles,
        height_tiles,
        tile_units: DEFAULT_TILE_UNITS,
        width_units,
        height_units,
        team_a_anchor,
        team_b_anchor,
        obstacles,
    })
}

pub fn parse_skill_yaml(source: &str, yaml: &str) -> Result<ClassDefinition, ContentError> {
    let document =
        serde_yaml::from_str::<SkillFileYaml>(yaml).map_err(|error| ContentError::Parse {
            source: String::from(source),
            message: error.to_string(),
        })?;
    validate_skill_file(source, document)
}

fn load_skill_catalog_from_pairs(pairs: &[(&str, &str)]) -> Result<SkillCatalog, ContentError> {
    let mut by_choice = BTreeMap::new();
    let mut melee_by_tree = BTreeMap::new();
    let mut ids_by_owner = BTreeMap::<String, String>::new();
    for (source, yaml) in pairs {
        let definition = parse_skill_yaml(source, yaml)?;
        if let Some(existing_owner) = ids_by_owner.insert(
            definition.melee.id.clone(),
            format!("{} melee", definition.tree),
        ) {
            return Err(ContentError::Validation {
                source: String::from(*source),
                message: format!(
                    "duplicate authored id '{}' already used by {existing_owner}",
                    definition.melee.id
                ),
            });
        }
        if melee_by_tree
            .insert(definition.tree.as_index(), definition.melee.clone())
            .is_some()
        {
            return Err(ContentError::Validation {
                source: String::from(*source),
                message: format!("duplicate melee definition for {}", definition.tree),
            });
        }

        for skill in definition.skills {
            if let Some(existing_owner) = ids_by_owner.insert(
                skill.id.clone(),
                format!("{} tier {}", skill.tree, skill.tier),
            ) {
                return Err(ContentError::Validation {
                    source: String::from(*source),
                    message: format!(
                        "duplicate authored id '{}' already used by {existing_owner}",
                        skill.id
                    ),
                });
            }
            let key = (skill.tree.as_index(), skill.tier);
            if let Some(existing) = by_choice.insert(key, skill.clone()) {
                return Err(ContentError::Validation {
                    source: String::from(*source),
                    message: format!(
                        "duplicate definition for {} tier {} (existing id {})",
                        existing.tree, existing.tier, existing.id
                    ),
                });
            }
        }
    }

    for tree in SkillTree::ALL {
        if !melee_by_tree.contains_key(&tree.as_index()) {
            return Err(ContentError::Validation {
                source: String::from("skills"),
                message: format!("missing melee definition for {tree}"),
            });
        }

        for tier in REQUIRED_TIERS {
            if !by_choice.contains_key(&(tree.as_index(), tier)) {
                return Err(ContentError::Validation {
                    source: String::from("skills"),
                    message: format!("missing definition for {tree} tier {tier}"),
                });
            }
        }
    }

    Ok(SkillCatalog {
        by_choice,
        melee_by_tree,
    })
}

fn validate_skill_file(
    source: &str,
    document: SkillFileYaml,
) -> Result<ClassDefinition, ContentError> {
    let tree = parse_skill_tree(source, &document.tree)?;
    let melee = parse_melee_definition(source, tree, document.melee)?;
    let mut seen_tiers = BTreeSet::new();
    let mut skills = Vec::with_capacity(document.skills.len());

    for yaml_skill in document.skills {
        if !REQUIRED_TIERS.contains(&yaml_skill.tier) {
            return Err(ContentError::Validation {
                source: String::from(source),
                message: format!(
                    "tier {} is outside the supported range 1..=5",
                    yaml_skill.tier
                ),
            });
        }
        if !seen_tiers.insert(yaml_skill.tier) {
            return Err(ContentError::Validation {
                source: String::from(source),
                message: format!("tier {} is defined more than once", yaml_skill.tier),
            });
        }
        validate_skill_text(source, "id", &yaml_skill.id)?;
        validate_skill_text(source, "name", &yaml_skill.name)?;
        validate_skill_text(source, "description", &yaml_skill.description)?;
        let behavior = parse_skill_behavior(source, &yaml_skill.behavior)?;
        skills.push(SkillDefinition {
            tree,
            tier: yaml_skill.tier,
            id: yaml_skill.id,
            name: yaml_skill.name,
            description: yaml_skill.description,
            behavior,
        });
    }

    if skills.len() != REQUIRED_TIERS.len() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!(
                "expected exactly {} skills, found {}",
                REQUIRED_TIERS.len(),
                skills.len()
            ),
        });
    }

    Ok(ClassDefinition {
        tree,
        melee,
        skills,
    })
}

fn parse_melee_definition(
    source: &str,
    tree: SkillTree,
    yaml: MeleeYaml,
) -> Result<MeleeDefinition, ContentError> {
    validate_skill_text(source, "melee.id", &yaml.id)?;
    validate_skill_text(source, "melee.name", &yaml.name)?;
    validate_skill_text(source, "melee.description", &yaml.description)?;
    Ok(MeleeDefinition {
        tree,
        id: yaml.id,
        name: yaml.name,
        description: yaml.description,
        cooldown_ms: require_positive_u16(source, "melee.cooldown_ms", Some(yaml.cooldown_ms))?,
        range: require_positive_u16(source, "melee.range", Some(yaml.range))?,
        radius: require_positive_u16(source, "melee.radius", Some(yaml.radius))?,
        effect: parse_effect_kind(source, &yaml.effect)?,
        payload: parse_payload(source, Some(yaml.payload), "melee.payload")?,
    })
}

fn validate_skill_text(source: &str, field: &str, value: &str) -> Result<(), ContentError> {
    if value.trim().is_empty() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{field} must not be empty"),
        });
    }
    if value.len() > MAX_SKILL_TEXT_LEN {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!(
                "{field} length {} exceeds maximum {MAX_SKILL_TEXT_LEN}",
                value.len()
            ),
        });
    }
    Ok(())
}

fn parse_skill_tree(source: &str, raw: &str) -> Result<SkillTree, ContentError> {
    match SkillTree::parse(raw) {
        Some(tree) => Ok(tree),
        None => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("unknown skill tree '{}'", raw.trim()),
        }),
    }
}

fn parse_skill_behavior(
    source: &str,
    yaml: &SkillBehaviorYaml,
) -> Result<SkillBehavior, ContentError> {
    validate_behavior_shape(source, yaml)?;
    let effect = parse_effect_kind(source, &yaml.effect)?;
    let cooldown_ms = require_positive_u16(source, "cooldown_ms", yaml.cooldown_ms)?;
    let mana_cost = yaml.mana_cost.unwrap_or(0);
    match yaml.kind.as_str() {
        "projectile" => Ok(SkillBehavior::Projectile {
            cooldown_ms,
            mana_cost,
            speed: require_positive_u16(source, "speed", yaml.speed)?,
            range: require_positive_u16(source, "range", yaml.range)?,
            radius: require_positive_u16(source, "radius", yaml.radius)?,
            effect,
            payload: parse_payload(source, yaml.payload.clone(), "payload")?,
        }),
        "beam" => Ok(SkillBehavior::Beam {
            cooldown_ms,
            mana_cost,
            range: require_positive_u16(source, "range", yaml.range)?,
            radius: require_positive_u16(source, "radius", yaml.radius)?,
            effect,
            payload: parse_payload(source, yaml.payload.clone(), "payload")?,
        }),
        "dash" => Ok(SkillBehavior::Dash {
            cooldown_ms,
            mana_cost,
            distance: require_positive_u16(source, "distance", yaml.distance)?,
            effect,
            impact_radius: yaml
                .impact_radius
                .map(|value| require_positive_u16(source, "impact_radius", Some(value)))
                .transpose()?,
            payload: match yaml.payload.clone() {
                Some(payload) => Some(parse_payload(source, Some(payload), "payload")?),
                None => None,
            },
        }),
        "burst" => Ok(SkillBehavior::Burst {
            cooldown_ms,
            mana_cost,
            range: require_positive_u16(source, "range", yaml.range)?,
            radius: require_positive_u16(source, "radius", yaml.radius)?,
            effect,
            payload: parse_payload(source, yaml.payload.clone(), "payload")?,
        }),
        "nova" => Ok(SkillBehavior::Nova {
            cooldown_ms,
            mana_cost,
            radius: require_positive_u16(source, "radius", yaml.radius)?,
            effect,
            payload: parse_payload(source, yaml.payload.clone(), "payload")?,
        }),
        other => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("unknown behavior kind '{other}'"),
        }),
    }
}

fn parse_payload(
    source: &str,
    yaml: Option<EffectPayloadYaml>,
    field: &str,
) -> Result<EffectPayload, ContentError> {
    let yaml = yaml.ok_or_else(|| ContentError::Validation {
        source: String::from(source),
        message: format!("{field} is required"),
    })?;

    let kind = parse_payload_kind(source, &yaml.kind)?;
    let amount = yaml.amount.unwrap_or(0);
    if amount == 0 && yaml.status.is_none() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{field} must provide a positive amount or a status"),
        });
    }

    Ok(EffectPayload {
        kind,
        amount,
        status: yaml
            .status
            .as_ref()
            .map(|status| parse_status(source, status))
            .transpose()?,
    })
}

fn parse_payload_kind(source: &str, raw: &str) -> Result<CombatValueKind, ContentError> {
    match raw {
        "damage" => Ok(CombatValueKind::Damage),
        "heal" => Ok(CombatValueKind::Heal),
        other => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("unknown payload kind '{other}'"),
        }),
    }
}

fn parse_status(source: &str, yaml: &StatusYaml) -> Result<StatusDefinition, ContentError> {
    let kind = parse_status_kind(source, &yaml.kind)?;
    let duration_ms = require_positive_u16(source, "status.duration_ms", Some(yaml.duration_ms))?;
    let max_stacks = yaml.max_stacks.unwrap_or(1);
    if max_stacks == 0 {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: String::from("status.max_stacks must be greater than zero"),
        });
    }

    match kind {
        StatusKind::Poison | StatusKind::Hot => {
            let tick_interval_ms =
                require_positive_u16(source, "status.tick_interval_ms", yaml.tick_interval_ms)?;
            Ok(StatusDefinition {
                kind,
                duration_ms,
                tick_interval_ms: Some(tick_interval_ms),
                magnitude: require_positive_u16(source, "status.magnitude", Some(yaml.magnitude))?,
                max_stacks,
                trigger_duration_ms: None,
            })
        }
        StatusKind::Chill | StatusKind::Haste => Ok(StatusDefinition {
            kind,
            duration_ms,
            tick_interval_ms: None,
            magnitude: require_positive_u16(source, "status.magnitude", Some(yaml.magnitude))?,
            max_stacks,
            trigger_duration_ms: if kind == StatusKind::Chill {
                yaml.trigger_duration_ms
                    .map(|value| {
                        require_positive_u16(source, "status.trigger_duration_ms", Some(value))
                    })
                    .transpose()?
            } else {
                forbid_numeric_status_field(
                    source,
                    "status.trigger_duration_ms",
                    yaml.trigger_duration_ms,
                )?;
                None
            },
        }),
        StatusKind::Root | StatusKind::Silence | StatusKind::Stun => {
            if yaml.tick_interval_ms.is_some() {
                return Err(ContentError::Validation {
                    source: String::from(source),
                    message: format!("status.tick_interval_ms is not valid for {kind:?}"),
                });
            }
            if yaml.trigger_duration_ms.is_some() {
                return Err(ContentError::Validation {
                    source: String::from(source),
                    message: format!("status.trigger_duration_ms is not valid for {kind:?}"),
                });
            }
            if yaml.magnitude != 0 {
                return Err(ContentError::Validation {
                    source: String::from(source),
                    message: format!("status.magnitude must be zero for {kind:?}"),
                });
            }
            if yaml.max_stacks.unwrap_or(1) != 1 {
                return Err(ContentError::Validation {
                    source: String::from(source),
                    message: format!("status.max_stacks must be 1 for {kind:?}"),
                });
            }
            Ok(StatusDefinition {
                kind,
                duration_ms,
                tick_interval_ms: None,
                magnitude: 0,
                max_stacks: 1,
                trigger_duration_ms: None,
            })
        }
    }
}

fn parse_status_kind(source: &str, raw: &str) -> Result<StatusKind, ContentError> {
    match raw {
        "poison" => Ok(StatusKind::Poison),
        "hot" => Ok(StatusKind::Hot),
        "chill" => Ok(StatusKind::Chill),
        "root" => Ok(StatusKind::Root),
        "haste" => Ok(StatusKind::Haste),
        "silence" => Ok(StatusKind::Silence),
        "stun" => Ok(StatusKind::Stun),
        other => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("unknown status kind '{other}'"),
        }),
    }
}

fn validate_behavior_shape(source: &str, yaml: &SkillBehaviorYaml) -> Result<(), ContentError> {
    match yaml.kind.as_str() {
        "projectile" => {
            forbid_behavior_field(source, "distance", yaml.distance)?;
            forbid_behavior_field(source, "impact_radius", yaml.impact_radius)?;
        }
        "beam" | "burst" => {
            forbid_behavior_field(source, "speed", yaml.speed)?;
            forbid_behavior_field(source, "distance", yaml.distance)?;
            forbid_behavior_field(source, "impact_radius", yaml.impact_radius)?;
        }
        "dash" => {
            forbid_behavior_field(source, "speed", yaml.speed)?;
            forbid_behavior_field(source, "range", yaml.range)?;
            forbid_behavior_field(source, "radius", yaml.radius)?;
        }
        "nova" => {
            forbid_behavior_field(source, "speed", yaml.speed)?;
            forbid_behavior_field(source, "distance", yaml.distance)?;
            forbid_behavior_field(source, "range", yaml.range)?;
            forbid_behavior_field(source, "impact_radius", yaml.impact_radius)?;
        }
        _ => {}
    }
    Ok(())
}

fn forbid_behavior_field(
    source: &str,
    field: &str,
    value: Option<u16>,
) -> Result<(), ContentError> {
    if value.is_some() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{field} is not valid for this behavior kind"),
        });
    }
    Ok(())
}

fn forbid_numeric_status_field(
    source: &str,
    field: &str,
    value: Option<u16>,
) -> Result<(), ContentError> {
    if value.is_some() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{field} is not valid for this status kind"),
        });
    }
    Ok(())
}

fn parse_effect_kind(source: &str, raw: &str) -> Result<SkillEffectKind, ContentError> {
    match raw {
        "melee_swing" => Ok(SkillEffectKind::MeleeSwing),
        "skill_shot" => Ok(SkillEffectKind::SkillShot),
        "dash_trail" => Ok(SkillEffectKind::DashTrail),
        "burst" => Ok(SkillEffectKind::Burst),
        "nova" => Ok(SkillEffectKind::Nova),
        "beam" => Ok(SkillEffectKind::Beam),
        "hit_spark" => Ok(SkillEffectKind::HitSpark),
        other => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("unknown effect kind '{other}'"),
        }),
    }
}

fn require_positive_u16(
    source: &str,
    field: &str,
    value: Option<u16>,
) -> Result<u16, ContentError> {
    match value {
        Some(0) => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{field} must be greater than zero"),
        }),
        Some(value) => Ok(value),
        None => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{field} is required"),
        }),
    }
}

fn collect_map_rows<'a>(source: &str, ascii_map: &'a str) -> Result<Vec<&'a str>, ContentError> {
    let rows = ascii_map
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: String::from("map must contain at least one non-empty row"),
        });
    }
    Ok(rows)
}

fn validate_map_dimensions(
    source: &str,
    rows: &[&str],
) -> Result<(u16, u16, u16, u16), ContentError> {
    let width = rows[0].chars().count();
    if width == 0 || width > MAX_MAP_DIMENSION_TILES {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!(
                "map width {width} is outside the supported range 1..={MAX_MAP_DIMENSION_TILES}"
            ),
        });
    }
    if rows.len() > MAX_MAP_DIMENSION_TILES {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!(
                "map height {} is outside the supported range 1..={MAX_MAP_DIMENSION_TILES}",
                rows.len()
            ),
        });
    }

    let width_tiles = u16::try_from(width).map_err(|_| ContentError::Validation {
        source: String::from(source),
        message: format!("map width {width} does not fit into u16"),
    })?;
    let height_tiles = u16::try_from(rows.len()).map_err(|_| ContentError::Validation {
        source: String::from(source),
        message: format!("map height {} does not fit into u16", rows.len()),
    })?;
    let width_units =
        width_tiles
            .checked_mul(DEFAULT_TILE_UNITS)
            .ok_or_else(|| ContentError::Validation {
                source: String::from(source),
                message: String::from("map width in world units overflowed u16"),
            })?;
    let height_units = height_tiles
        .checked_mul(DEFAULT_TILE_UNITS)
        .ok_or_else(|| ContentError::Validation {
            source: String::from(source),
            message: String::from("map height in world units overflowed u16"),
        })?;
    Ok((width_tiles, height_tiles, width_units, height_units))
}

fn parse_map_layout(
    source: &str,
    rows: &[&str],
    width_tiles: u16,
    height_tiles: u16,
) -> Result<(AnchorPoint, AnchorPoint, Vec<ArenaMapObstacle>), ContentError> {
    let mut team_a_anchor = None;
    let mut team_b_anchor = None;
    let mut obstacles = Vec::new();
    let expected_width = usize::from(width_tiles);

    for (row_index, row) in rows.iter().enumerate() {
        validate_map_row_width(source, row, row_index, expected_width)?;

        for (column_index, glyph) in row.chars().enumerate() {
            let (center_x, center_y) = map_cell_center(
                width_tiles,
                height_tiles,
                DEFAULT_TILE_UNITS,
                column_index,
                row_index,
            )?;
            parse_map_glyph(
                source,
                glyph,
                row_index,
                column_index,
                center_x,
                center_y,
                &mut team_a_anchor,
                &mut team_b_anchor,
                &mut obstacles,
            )?;
        }
    }

    let team_a_anchor = team_a_anchor.ok_or_else(|| ContentError::Validation {
        source: String::from(source),
        message: String::from("map must contain one Team A anchor 'A'"),
    })?;
    let team_b_anchor = team_b_anchor.ok_or_else(|| ContentError::Validation {
        source: String::from(source),
        message: String::from("map must contain one Team B anchor 'B'"),
    })?;
    Ok((team_a_anchor, team_b_anchor, obstacles))
}

fn validate_map_row_width(
    source: &str,
    row: &str,
    row_index: usize,
    expected_width: usize,
) -> Result<(), ContentError> {
    let row_width = row.chars().count();
    if row_width != expected_width {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!(
                "row {} has width {} but expected {}",
                row_index + 1,
                row_width,
                expected_width
            ),
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn parse_map_glyph(
    source: &str,
    glyph: char,
    row_index: usize,
    column_index: usize,
    center_x: i16,
    center_y: i16,
    team_a_anchor: &mut Option<AnchorPoint>,
    team_b_anchor: &mut Option<AnchorPoint>,
    obstacles: &mut Vec<ArenaMapObstacle>,
) -> Result<(), ContentError> {
    match glyph {
        '.' | ' ' => Ok(()),
        'A' => set_team_anchor(source, "A", team_a_anchor, center_x, center_y),
        'B' => set_team_anchor(source, "B", team_b_anchor, center_x, center_y),
        '#' => {
            obstacles.push(map_obstacle(
                ArenaMapObstacleKind::Pillar,
                center_x,
                center_y,
            ));
            Ok(())
        }
        '+' => {
            obstacles.push(map_obstacle(
                ArenaMapObstacleKind::Shrub,
                center_x,
                center_y,
            ));
            Ok(())
        }
        other => Err(ContentError::Validation {
            source: String::from(source),
            message: format!(
                "unsupported map glyph '{other}' at row {}, column {}",
                row_index + 1,
                column_index + 1
            ),
        }),
    }
}

fn set_team_anchor(
    source: &str,
    label: &str,
    anchor: &mut Option<AnchorPoint>,
    center_x: i16,
    center_y: i16,
) -> Result<(), ContentError> {
    if anchor.replace((center_x, center_y)).is_some() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!("map must contain exactly one Team {label} anchor"),
        });
    }
    Ok(())
}

fn map_obstacle(kind: ArenaMapObstacleKind, center_x: i16, center_y: i16) -> ArenaMapObstacle {
    ArenaMapObstacle {
        kind,
        center_x,
        center_y,
        half_width: DEFAULT_TILE_UNITS / 2,
        half_height: DEFAULT_TILE_UNITS / 2,
    }
}

fn map_identifier(source: &str) -> String {
    Path::new(source)
        .file_stem()
        .and_then(|value| value.to_str())
        .map_or_else(|| String::from("arena"), String::from)
}

fn map_cell_center(
    width_tiles: u16,
    height_tiles: u16,
    tile_units: u16,
    column: usize,
    row: usize,
) -> Result<(i16, i16), ContentError> {
    let width_units = i32::from(width_tiles) * i32::from(tile_units);
    let height_units = i32::from(height_tiles) * i32::from(tile_units);
    let origin_x = -width_units / 2;
    let origin_y = -height_units / 2;
    let center_x = origin_x
        + i32::try_from(column).unwrap_or(i32::MAX) * i32::from(tile_units)
        + i32::from(tile_units / 2);
    let center_y = origin_y
        + i32::try_from(row).unwrap_or(i32::MAX) * i32::from(tile_units)
        + i32::from(tile_units / 2);

    let x = i16::try_from(center_x).map_err(|_| ContentError::Validation {
        source: String::from("map"),
        message: format!("map column {column} overflowed i16 coordinates"),
    })?;
    let y = i16::try_from(center_y).map_err(|_| ContentError::Validation {
        source: String::from("map"),
        message: format!("map row {row} overflowed i16 coordinates"),
    })?;
    Ok((x, y))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillFileYaml {
    tree: String,
    melee: MeleeYaml,
    skills: Vec<SkillYaml>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MeleeYaml {
    id: String,
    name: String,
    description: String,
    cooldown_ms: u16,
    range: u16,
    radius: u16,
    effect: String,
    payload: EffectPayloadYaml,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillYaml {
    tier: u8,
    id: String,
    name: String,
    description: String,
    behavior: SkillBehaviorYaml,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectPayloadYaml {
    kind: String,
    amount: Option<u16>,
    status: Option<StatusYaml>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusYaml {
    kind: String,
    duration_ms: u16,
    tick_interval_ms: Option<u16>,
    magnitude: u16,
    max_stacks: Option<u8>,
    trigger_duration_ms: Option<u16>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillBehaviorYaml {
    kind: String,
    effect: String,
    cooldown_ms: Option<u16>,
    mana_cost: Option<u16>,
    range: Option<u16>,
    radius: Option<u16>,
    distance: Option<u16>,
    speed: Option<u16>,
    impact_radius: Option<u16>,
    payload: Option<EffectPayloadYaml>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn bundled_content_loads_all_classes_and_the_ascii_map() {
        let content = GameContent::bundled().expect("bundled content should load");

        let mage_one = content
            .skills()
            .resolve(SkillChoice::new(SkillTree::Mage, 1).expect("choice"))
            .expect("mage tier one should exist");
        assert!(matches!(
            mage_one.behavior,
            SkillBehavior::Projectile { .. }
        ));
        assert!(content.skills().melee_for(SkillTree::Warrior).is_some());
        assert_eq!(content.map().map_id, "prototype_arena");
        assert!(!content.map().obstacles.is_empty());
        assert_eq!(content.map().team_a_anchor.0, -650);
        assert_eq!(content.map().team_b_anchor.0, 650);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn parse_skill_yaml_rejects_unknown_trees_duplicate_tiers_and_invalid_field_shapes() {
        let unknown_tree = r"
tree: Druid
melee:
  id: druid_claw
  name: Claw
  description: nope
  cooldown_ms: 100
  range: 50
  radius: 20
  effect: melee_swing
  payload:
    kind: damage
    amount: 10
skills:
  - tier: 1
    id: druid_sprout
    name: Sprout
    description: nope
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 100
      speed: 100
      range: 100
      radius: 10
      payload:
        kind: damage
        amount: 1
";
        assert!(matches!(
            parse_skill_yaml("skills/druid.yaml", unknown_tree),
            Err(ContentError::Validation { .. })
        ));

        let duplicate_tier = r"
tree: Mage
melee:
  id: mage_staff
  name: Staff
  description: bonk
  cooldown_ms: 100
  range: 50
  radius: 20
  effect: melee_swing
  payload:
    kind: damage
    amount: 10
skills:
  - tier: 1
    id: mage_a
    name: A
    description: A
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 100
      speed: 100
      range: 10
      radius: 10
      payload:
        kind: damage
        amount: 1
  - tier: 1
    id: mage_b
    name: B
    description: B
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
";
        assert!(matches!(
            parse_skill_yaml("skills/mage.yaml", duplicate_tier),
            Err(ContentError::Validation { .. })
        ));

        let invalid_dash_shape = r"
tree: Rogue
melee:
  id: rogue_blade
  name: Blade
  description: blade
  cooldown_ms: 100
  range: 50
  radius: 20
  effect: melee_swing
  payload:
    kind: damage
    amount: 10
skills:
  - tier: 1
    id: rogue_dash
    name: Dash
    description: dash
    behavior:
      kind: dash
      effect: dash_trail
      cooldown_ms: 100
      distance: 120
      range: 40
  - tier: 2
    id: rogue_two
    name: Two
    description: Two
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 3
    id: rogue_three
    name: Three
    description: Three
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 4
    id: rogue_four
    name: Four
    description: Four
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
  - tier: 5
    id: rogue_five
    name: Five
    description: Five
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 100
      range: 20
      radius: 10
      payload:
        kind: damage
        amount: 2
";
        assert!(matches!(
            parse_skill_yaml("skills/rogue.yaml", invalid_dash_shape),
            Err(ContentError::Validation { .. })
        ));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn parse_skill_yaml_accepts_status_payloads_and_melee_definitions() {
        let yaml = r"
tree: Cleric
melee:
  id: cleric_mace
  name: Mace
  description: bonk
  cooldown_ms: 550
  range: 80
  radius: 30
  effect: melee_swing
  payload:
    kind: damage
    amount: 12
skills:
  - tier: 1
    id: cleric_minor_heal
    name: Minor Heal
    description: heal
    behavior:
      kind: beam
      effect: beam
      cooldown_ms: 900
      range: 250
      radius: 20
      payload:
        kind: heal
        amount: 14
  - tier: 2
    id: cleric_flash
    name: Flash
    description: flash
    behavior:
      kind: dash
      effect: dash_trail
      cooldown_ms: 1200
      distance: 120
  - tier: 3
    id: cleric_hot
    name: Hot
    description: hot
    behavior:
      kind: nova
      effect: nova
      cooldown_ms: 1200
      radius: 120
      payload:
        kind: heal
        amount: 0
        status:
          kind: hot
          duration_ms: 3000
          tick_interval_ms: 1000
          magnitude: 4
          max_stacks: 1
  - tier: 4
    id: cleric_burst
    name: Burst
    description: burst
    behavior:
      kind: burst
      effect: burst
      cooldown_ms: 1400
      range: 200
      radius: 80
      payload:
        kind: damage
        amount: 10
  - tier: 5
    id: cleric_root
    name: Root
    description: root
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 1600
      speed: 100
      range: 300
      radius: 12
      payload:
        kind: damage
        amount: 5
        status:
          kind: root
          duration_ms: 500
          magnitude: 0
";
        let parsed = parse_skill_yaml("skills/cleric.yaml", yaml).expect("yaml should parse");
        assert_eq!(parsed.melee.cooldown_ms, 550);
        assert!(matches!(
            parsed.skills[2].behavior,
            SkillBehavior::Nova {
                mana_cost: 0,
                payload: EffectPayload {
                    kind: CombatValueKind::Heal,
                    status: Some(StatusDefinition {
                        kind: StatusKind::Hot,
                        ..
                    }),
                    ..
                },
                ..
            }
        ));
        assert_eq!(parsed.skills[0].behavior.mana_cost(), 0);
    }

    #[test]
    fn parse_ascii_map_rejects_ragged_rows_bad_glyphs_and_missing_anchors() {
        let ragged = "A..\n..\n";
        assert!(matches!(
            parse_ascii_map("maps/ragged.txt", ragged),
            Err(ContentError::Validation { .. })
        ));

        let invalid_glyph = "A..\n.@.\n..B\n";
        assert!(matches!(
            parse_ascii_map("maps/invalid.txt", invalid_glyph),
            Err(ContentError::Validation { .. })
        ));

        let missing_anchor = "...\n.#.\n...\n";
        assert!(matches!(
            parse_ascii_map("maps/missing.txt", missing_anchor),
            Err(ContentError::Validation { .. })
        ));
    }

    #[test]
    fn load_skill_catalog_rejects_duplicate_authored_ids_across_files() {
        let duplicate_mage = BUNDLED_SKILL_FILES[1]
            .1
            .replace("mage_arc_bolt", "warrior_sweeping_slash");
        let pairs = vec![
            BUNDLED_SKILL_FILES[0],
            ("skills/mage.yaml", duplicate_mage.as_str()),
            BUNDLED_SKILL_FILES[2],
            BUNDLED_SKILL_FILES[3],
        ];
        assert!(matches!(
            load_skill_catalog_from_pairs(&pairs),
            Err(ContentError::Validation { .. })
        ));
    }

    #[test]
    fn load_from_root_fails_cleanly_for_invalid_yaml_and_map_content() {
        let root = temp_content_root("invalid-content");
        let skills_dir = root.join("skills");
        let maps_dir = root.join("maps");
        fs::create_dir_all(&skills_dir).expect("skills dir");
        fs::create_dir_all(&maps_dir).expect("maps dir");

        for (source, yaml) in BUNDLED_SKILL_FILES {
            let path = skills_dir.join(
                Path::new(source)
                    .file_name()
                    .expect("bundled skill file should have a file name"),
            );
            let text = if source.ends_with("rogue.yaml") {
                yaml.replacen("kind: projectile", "kind: dash", 1)
            } else {
                String::from(yaml)
            };
            fs::write(path, text).expect("skill file");
        }
        fs::write(maps_dir.join("prototype_arena.txt"), "A..\n..B\n").expect("map file");

        let error = GameContent::load_from_root(&root).expect_err("invalid content should fail");
        assert!(matches!(error, ContentError::Validation { .. }));
    }

    fn temp_content_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("current time should be after unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rarena-{label}-{nonce}"));
        if root.exists() {
            fs::remove_dir_all(&root).expect("existing temp dir should be removable");
        }
        root
    }
}
