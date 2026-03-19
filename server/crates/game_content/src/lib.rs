//! Data loading and validation for authored YAML skills and ASCII maps.
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod error;
mod model;
mod yaml;

pub use error::ContentError;
pub use model::*;
use yaml::{
    MechanicSchemaYaml, MechanicYaml, MeleeYaml, NumericRuleYaml, PayloadRuleYaml, SkillFileYaml,
    StackRuleYaml,
};

const DEFAULT_TILE_UNITS: u16 = 50;
const MAX_MAP_DIMENSION_TILES: usize = 128;
const MAX_SKILL_TEXT_LEN: usize = 120;
const REQUIRED_TIERS: [u8; 5] = [1, 2, 3, 4, 5];
const DEFAULT_MECHANICS_REGISTRY: &str = include_str!("../../../content/mechanics/registry.yaml");
const BEHAVIOR_NUMERIC_FIELDS: [&str; 7] = [
    "cooldown_ms",
    "mana_cost",
    "range",
    "radius",
    "distance",
    "speed",
    "impact_radius",
];
const STATUS_NUMERIC_FIELDS: [&str; 4] = [
    "duration_ms",
    "tick_interval_ms",
    "magnitude",
    "trigger_duration_ms",
];

type AnchorPoint = (i16, i16);

fn workspace_content_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content")
}

fn read_skill_file_pairs(root: &Path) -> Result<Vec<(String, String)>, ContentError> {
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
    Ok(pairs)
}

fn default_mechanics() -> &'static MechanicCatalog {
    static DEFAULT_MECHANICS: OnceLock<MechanicCatalog> = OnceLock::new();
    DEFAULT_MECHANICS.get_or_init(|| {
        match parse_mechanics_yaml(
            "content/mechanics/registry.yaml",
            DEFAULT_MECHANICS_REGISTRY,
        ) {
            Ok(mechanics) => mechanics,
            Err(error) => panic!("default mechanics registry should parse: {error}"),
        }
    })
}

mod maps;
mod mechanics;
mod skills;

pub use maps::parse_ascii_map;
pub use mechanics::parse_mechanics_yaml;
pub use skills::parse_skill_yaml;

#[cfg(test)]
pub(crate) use skills::load_skill_catalog_from_pairs;
pub(crate) use skills::load_skill_catalog_from_pairs_with_mechanics;

#[cfg(test)]
mod tests;
