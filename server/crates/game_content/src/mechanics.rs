use super::skills::{parse_effect_kind, validate_skill_text};
use crate::yaml::MechanicsFileYaml;
use std::collections::{BTreeMap, BTreeSet};

use super::{
    BehaviorSchema, ContentError, MechanicCatalog, MechanicCategory, MechanicDefinition,
    MechanicSchemaYaml, MechanicYaml, NumericFieldRule, NumericRuleYaml, PayloadFieldRule,
    PayloadRuleYaml, StackRule, StackRuleYaml, StatusSchema, BEHAVIOR_NUMERIC_FIELDS,
    STATUS_NUMERIC_FIELDS,
};

pub fn parse_mechanics_yaml(source: &str, yaml: &str) -> Result<MechanicCatalog, ContentError> {
    let document =
        serde_yaml::from_str::<MechanicsFileYaml>(yaml).map_err(|error| ContentError::Parse {
            source: String::from(source),
            message: error.to_string(),
        })?;

    Ok(MechanicCatalog {
        behaviors: validate_mechanics(source, document.behaviors, MechanicCategory::Behavior)?,
        statuses: validate_mechanics(source, document.statuses, MechanicCategory::Status)?,
    })
}

fn validate_mechanics(
    source: &str,
    yaml_mechanics: Vec<MechanicYaml>,
    category: MechanicCategory,
) -> Result<Vec<MechanicDefinition>, ContentError> {
    if yaml_mechanics.is_empty() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{category:?} registry must contain at least one entry"),
        });
    }

    let mut seen_ids = BTreeSet::new();
    let mut mechanics = Vec::with_capacity(yaml_mechanics.len());
    for yaml in yaml_mechanics {
        validate_skill_text(source, "mechanic.id", &yaml.id)?;
        validate_skill_text(source, "mechanic.label", &yaml.label)?;
        validate_skill_text(source, "mechanic.inspiration", &yaml.inspiration)?;
        validate_skill_text(source, "mechanic.notes", &yaml.notes)?;
        if !seen_ids.insert(yaml.id.clone()) {
            return Err(ContentError::Validation {
                source: String::from(source),
                message: format!("duplicate mechanic id '{}'", yaml.id),
            });
        }
        mechanics.push(MechanicDefinition {
            id: yaml.id,
            label: yaml.label,
            category,
            implemented: yaml.implemented,
            inspiration: yaml.inspiration,
            notes: yaml.notes,
            behavior_schema: match category {
                MechanicCategory::Behavior => Some(parse_behavior_schema(
                    source,
                    yaml.implemented,
                    yaml.schema.as_ref(),
                )?),
                MechanicCategory::Status => None,
            },
            status_schema: match category {
                MechanicCategory::Behavior => None,
                MechanicCategory::Status => Some(parse_status_schema(
                    source,
                    yaml.implemented,
                    yaml.schema.as_ref(),
                )?),
            },
        });
    }
    Ok(mechanics)
}

fn parse_behavior_schema(
    source: &str,
    implemented: bool,
    yaml: Option<&MechanicSchemaYaml>,
) -> Result<BehaviorSchema, ContentError> {
    let Some(yaml) = yaml else {
        if implemented {
            return Err(ContentError::Validation {
                source: String::from(source),
                message: String::from("implemented behavior mechanics must define a schema"),
            });
        }
        return Ok(BehaviorSchema {
            numeric_fields: BTreeMap::new(),
            payload: PayloadFieldRule::Forbidden,
            allowed_effects: Vec::new(),
        });
    };

    validate_schema_field_names(source, yaml.numeric_fields.keys(), &BEHAVIOR_NUMERIC_FIELDS)?;

    let numeric_fields = yaml
        .numeric_fields
        .iter()
        .map(|(field, rule)| (field.clone(), parse_numeric_rule(*rule)))
        .collect::<BTreeMap<_, _>>();
    let allowed_effects = yaml
        .allowed_effects
        .iter()
        .map(|effect| parse_effect_kind(source, effect))
        .collect::<Result<Vec<_>, _>>()?;

    if implemented && allowed_effects.is_empty() {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: String::from("implemented behavior mechanics must declare allowed_effects"),
        });
    }

    Ok(BehaviorSchema {
        numeric_fields,
        payload: parse_payload_rule(yaml.payload),
        allowed_effects,
    })
}

fn parse_status_schema(
    source: &str,
    implemented: bool,
    yaml: Option<&MechanicSchemaYaml>,
) -> Result<StatusSchema, ContentError> {
    let Some(yaml) = yaml else {
        if implemented {
            return Err(ContentError::Validation {
                source: String::from(source),
                message: String::from("implemented status mechanics must define a schema"),
            });
        }
        return Ok(StatusSchema {
            numeric_fields: BTreeMap::new(),
            max_stacks: StackRule::One,
        });
    };

    validate_schema_field_names(source, yaml.numeric_fields.keys(), &STATUS_NUMERIC_FIELDS)?;

    Ok(StatusSchema {
        numeric_fields: yaml
            .numeric_fields
            .iter()
            .map(|(field, rule)| (field.clone(), parse_numeric_rule(*rule)))
            .collect(),
        max_stacks: parse_stack_rule(yaml.max_stacks),
    })
}

fn validate_schema_field_names<'a>(
    source: &str,
    fields: impl Iterator<Item = &'a String>,
    allowed: &[&str],
) -> Result<(), ContentError> {
    for field in fields {
        if !allowed.contains(&field.as_str()) {
            return Err(ContentError::Validation {
                source: String::from(source),
                message: format!("schema field '{field}' is not supported"),
            });
        }
    }
    Ok(())
}

const fn parse_numeric_rule(rule: NumericRuleYaml) -> NumericFieldRule {
    match rule {
        NumericRuleYaml::Required => NumericFieldRule::Required,
        NumericRuleYaml::Optional => NumericFieldRule::Optional,
        NumericRuleYaml::Zero => NumericFieldRule::Zero,
        NumericRuleYaml::Forbidden => NumericFieldRule::Forbidden,
    }
}

const fn parse_payload_rule(rule: PayloadRuleYaml) -> PayloadFieldRule {
    match rule {
        PayloadRuleYaml::Required => PayloadFieldRule::Required,
        PayloadRuleYaml::Optional => PayloadFieldRule::Optional,
        PayloadRuleYaml::Forbidden => PayloadFieldRule::Forbidden,
    }
}

const fn parse_stack_rule(rule: StackRuleYaml) -> StackRule {
    match rule {
        StackRuleYaml::Positive => StackRule::Positive,
        StackRuleYaml::One => StackRule::One,
    }
}
