use std::collections::{BTreeMap, BTreeSet};

use game_domain::SkillTree;

use super::{
    default_mechanics, ClassDefinition, ContentError, MechanicCatalog, MeleeDefinition, MeleeYaml,
    SkillCatalog, SkillDefinition, SkillFileYaml, MAX_SKILL_TEXT_LEN, REQUIRED_TIERS,
};

mod behavior;

pub(super) use behavior::parse_effect_kind;
use behavior::{parse_payload, parse_skill_behavior, require_positive_u16};

pub fn parse_skill_yaml(source: &str, yaml: &str) -> Result<ClassDefinition, ContentError> {
    parse_skill_yaml_with_mechanics(source, yaml, default_mechanics())
}

pub(crate) fn parse_skill_yaml_with_mechanics(
    source: &str,
    yaml: &str,
    mechanics: &MechanicCatalog,
) -> Result<ClassDefinition, ContentError> {
    let document =
        serde_yaml::from_str::<SkillFileYaml>(yaml).map_err(|error| ContentError::Parse {
            source: String::from(source),
            message: error.to_string(),
        })?;
    validate_skill_file(source, document, mechanics)
}

#[cfg(test)]
pub(crate) fn load_skill_catalog_from_pairs(
    pairs: &[(&str, &str)],
) -> Result<SkillCatalog, ContentError> {
    load_skill_catalog_from_pairs_with_mechanics(pairs, default_mechanics())
}

pub(crate) fn load_skill_catalog_from_pairs_with_mechanics(
    pairs: &[(&str, &str)],
    mechanics: &MechanicCatalog,
) -> Result<SkillCatalog, ContentError> {
    let mut by_choice = BTreeMap::new();
    let mut melee_by_tree = BTreeMap::new();
    let mut ids_by_owner = BTreeMap::<String, String>::new();
    if pairs.is_empty() {
        return Err(ContentError::Validation {
            source: String::from("skills"),
            message: String::from("at least one class skill file is required"),
        });
    }
    for (source, yaml) in pairs {
        let definition = parse_skill_yaml_with_mechanics(source, yaml, mechanics)?;
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
            .insert(definition.tree.clone(), definition.melee.clone())
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
            let key = (skill.tree.clone(), skill.tier);
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

    Ok(SkillCatalog::new(by_choice, melee_by_tree))
}

fn validate_skill_file(
    source: &str,
    document: SkillFileYaml,
    mechanics: &MechanicCatalog,
) -> Result<ClassDefinition, ContentError> {
    let tree = parse_skill_tree(source, &document.tree)?;
    let melee = parse_melee_definition(source, tree.clone(), document.melee)?;
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
        let behavior = parse_skill_behavior(source, &yaml_skill.behavior, mechanics)?;
        skills.push(SkillDefinition {
            tree: tree.clone(),
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
        payload: parse_payload(
            source,
            Some(yaml.payload),
            "melee.payload",
            default_mechanics(),
        )?,
    })
}

pub(super) fn validate_skill_text(
    source: &str,
    field: &str,
    value: &str,
) -> Result<(), ContentError> {
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
