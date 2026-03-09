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
    SkillShot,
    DashTrail,
    Burst,
    Nova,
    Beam,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillBehavior {
    Line {
        range: u16,
        damage: u16,
        effect: SkillEffectKind,
    },
    Dash {
        distance: u16,
        effect: SkillEffectKind,
    },
    Burst {
        range: u16,
        radius: u16,
        damage: u16,
        effect: SkillEffectKind,
    },
    Nova {
        radius: u16,
        damage: u16,
        effect: SkillEffectKind,
    },
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
pub struct SkillCatalog {
    by_choice: BTreeMap<(usize, u8), SkillDefinition>,
}

impl SkillCatalog {
    #[must_use]
    pub fn resolve(&self, choice: SkillChoice) -> Option<&SkillDefinition> {
        self.by_choice.get(&(choice.tree.as_index(), choice.tier))
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

pub fn parse_skill_yaml(source: &str, yaml: &str) -> Result<Vec<SkillDefinition>, ContentError> {
    let document =
        serde_yaml::from_str::<SkillFileYaml>(yaml).map_err(|error| ContentError::Parse {
            source: String::from(source),
            message: error.to_string(),
        })?;
    validate_skill_file(source, document)
}

fn load_skill_catalog_from_pairs(pairs: &[(&str, &str)]) -> Result<SkillCatalog, ContentError> {
    let mut by_choice = BTreeMap::new();
    for (source, yaml) in pairs {
        for definition in parse_skill_yaml(source, yaml)? {
            let key = (definition.tree.as_index(), definition.tier);
            if let Some(existing) = by_choice.insert(key, definition.clone()) {
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

    for tree in [
        SkillTree::Warrior,
        SkillTree::Rogue,
        SkillTree::Mage,
        SkillTree::Cleric,
    ] {
        for tier in REQUIRED_TIERS {
            if !by_choice.contains_key(&(tree.as_index(), tier)) {
                return Err(ContentError::Validation {
                    source: String::from("skills"),
                    message: format!("missing definition for {tree} tier {tier}"),
                });
            }
        }
    }

    Ok(SkillCatalog { by_choice })
}

fn validate_skill_file(
    source: &str,
    document: SkillFileYaml,
) -> Result<Vec<SkillDefinition>, ContentError> {
    let tree = parse_skill_tree(source, &document.tree)?;
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

    Ok(skills)
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
    match raw {
        "Warrior" => Ok(SkillTree::Warrior),
        "Rogue" => Ok(SkillTree::Rogue),
        "Mage" => Ok(SkillTree::Mage),
        "Cleric" => Ok(SkillTree::Cleric),
        other => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("unknown skill tree '{other}'"),
        }),
    }
}

fn parse_skill_behavior(
    source: &str,
    yaml: &SkillBehaviorYaml,
) -> Result<SkillBehavior, ContentError> {
    let effect = parse_effect_kind(source, &yaml.effect)?;
    match yaml.kind.as_str() {
        "line" => Ok(SkillBehavior::Line {
            range: require_positive_u16(source, "range", yaml.range)?,
            damage: require_positive_u16(source, "damage", yaml.damage)?,
            effect,
        }),
        "dash" => Ok(SkillBehavior::Dash {
            distance: require_positive_u16(source, "distance", yaml.distance)?,
            effect,
        }),
        "burst" => Ok(SkillBehavior::Burst {
            range: require_positive_u16(source, "range", yaml.range)?,
            radius: require_positive_u16(source, "radius", yaml.radius)?,
            damage: require_positive_u16(source, "damage", yaml.damage)?,
            effect,
        }),
        "nova" => Ok(SkillBehavior::Nova {
            radius: require_positive_u16(source, "radius", yaml.radius)?,
            damage: require_positive_u16(source, "damage", yaml.damage)?,
            effect,
        }),
        other => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("unknown behavior kind '{other}'"),
        }),
    }
}

fn parse_effect_kind(source: &str, raw: &str) -> Result<SkillEffectKind, ContentError> {
    match raw {
        "skill_shot" => Ok(SkillEffectKind::SkillShot),
        "dash_trail" => Ok(SkillEffectKind::DashTrail),
        "burst" => Ok(SkillEffectKind::Burst),
        "nova" => Ok(SkillEffectKind::Nova),
        "beam" => Ok(SkillEffectKind::Beam),
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
    skills: Vec<SkillYaml>,
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillBehaviorYaml {
    kind: String,
    effect: String,
    range: Option<u16>,
    damage: Option<u16>,
    radius: Option<u16>,
    distance: Option<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_content_loads_all_classes_and_the_ascii_map() {
        let content = GameContent::bundled().expect("bundled content should load");

        assert!(content
            .skills()
            .resolve(SkillChoice::new(SkillTree::Mage, 1).expect("choice"))
            .is_some());
        assert_eq!(content.map().map_id, "prototype_arena");
        assert!(!content.map().obstacles.is_empty());
        assert_eq!(content.map().team_a_anchor.0, -650);
        assert_eq!(content.map().team_b_anchor.0, 650);
    }

    #[test]
    fn parse_skill_yaml_rejects_unknown_trees_and_duplicate_tiers() {
        let unknown_tree = r"
tree: Druid
skills:
  - tier: 1
    id: druid_sprout
    name: Sprout
    description: nope
    behavior:
      kind: line
      effect: skill_shot
      range: 10
      damage: 1
";
        assert!(matches!(
            parse_skill_yaml("skills/druid.yaml", unknown_tree),
            Err(ContentError::Validation { .. })
        ));

        let duplicate_tier = r"
tree: Mage
skills:
  - tier: 1
    id: mage_a
    name: A
    description: A
    behavior:
      kind: line
      effect: skill_shot
      range: 10
      damage: 1
  - tier: 1
    id: mage_b
    name: B
    description: B
    behavior:
      kind: line
      effect: beam
      range: 20
      damage: 2
";
        assert!(matches!(
            parse_skill_yaml("skills/mage.yaml", duplicate_tier),
            Err(ContentError::Validation { .. })
        ));
    }

    #[test]
    fn parse_ascii_map_rejects_ragged_rows_and_missing_anchors() {
        let ragged = "A..\n..\n";
        assert!(matches!(
            parse_ascii_map("maps/ragged.txt", ragged),
            Err(ContentError::Validation { .. })
        ));

        let missing_anchor = "...\n.#.\n...\n";
        assert!(matches!(
            parse_ascii_map("maps/missing.txt", missing_anchor),
            Err(ContentError::Validation { .. })
        ));
    }
}
