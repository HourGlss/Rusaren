use std::collections::BTreeMap;

use crate::yaml::{EffectPayloadYaml, SkillBehaviorYaml, StatusYaml};
use crate::{
    BehaviorSchema, CombatValueKind, ContentError, EffectPayload, MechanicCatalog,
    NumericFieldRule, PayloadFieldRule, SkillBehavior, SkillEffectKind, StackRule,
    StatusDefinition, StatusKind,
};

struct BehaviorNumericFields {
    cooldown_ms: Option<u16>,
    mana_cost: Option<u16>,
    range: Option<u16>,
    radius: Option<u16>,
    distance: Option<u16>,
    speed: Option<u16>,
    impact_radius: Option<u16>,
}

fn parse_behavior_numeric_fields(
    source: &str,
    yaml: &SkillBehaviorYaml,
    schema: &BehaviorSchema,
) -> Result<BehaviorNumericFields, ContentError> {
    Ok(BehaviorNumericFields {
        cooldown_ms: read_numeric_field(
            source,
            "cooldown_ms",
            yaml.cooldown_ms,
            schema_numeric_rule(&schema.numeric_fields, "cooldown_ms"),
        )?,
        mana_cost: read_numeric_field(
            source,
            "mana_cost",
            yaml.mana_cost,
            schema_numeric_rule(&schema.numeric_fields, "mana_cost"),
        )?,
        range: read_numeric_field(
            source,
            "range",
            yaml.range,
            schema_numeric_rule(&schema.numeric_fields, "range"),
        )?,
        radius: read_numeric_field(
            source,
            "radius",
            yaml.radius,
            schema_numeric_rule(&schema.numeric_fields, "radius"),
        )?,
        distance: read_numeric_field(
            source,
            "distance",
            yaml.distance,
            schema_numeric_rule(&schema.numeric_fields, "distance"),
        )?,
        speed: read_numeric_field(
            source,
            "speed",
            yaml.speed,
            schema_numeric_rule(&schema.numeric_fields, "speed"),
        )?,
        impact_radius: read_numeric_field(
            source,
            "impact_radius",
            yaml.impact_radius,
            schema_numeric_rule(&schema.numeric_fields, "impact_radius"),
        )?,
    })
}

pub(super) fn parse_skill_behavior(
    source: &str,
    yaml: &SkillBehaviorYaml,
    mechanics: &MechanicCatalog,
) -> Result<SkillBehavior, ContentError> {
    let schema = behavior_schema(mechanics, source, &yaml.kind)?;
    let effect = parse_effect_kind(source, &yaml.effect)?;
    validate_allowed_effect(source, &yaml.kind, effect, &schema.allowed_effects)?;
    let fields = parse_behavior_numeric_fields(source, yaml, schema)?;
    let cooldown_ms = require_present_u16(source, "cooldown_ms", fields.cooldown_ms)?;
    let mana_cost = fields.mana_cost.unwrap_or(0);
    match yaml.kind.as_str() {
        "projectile" => Ok(SkillBehavior::Projectile {
            cooldown_ms,
            mana_cost,
            speed: require_present_u16(source, "speed", fields.speed)?,
            range: require_present_u16(source, "range", fields.range)?,
            radius: require_present_u16(source, "radius", fields.radius)?,
            effect,
            payload: parse_behavior_payload(
                source,
                yaml.payload.clone(),
                schema.payload,
                mechanics,
            )?,
        }),
        "beam" => Ok(SkillBehavior::Beam {
            cooldown_ms,
            mana_cost,
            range: require_present_u16(source, "range", fields.range)?,
            radius: require_present_u16(source, "radius", fields.radius)?,
            effect,
            payload: parse_behavior_payload(
                source,
                yaml.payload.clone(),
                schema.payload,
                mechanics,
            )?,
        }),
        "dash" => Ok(SkillBehavior::Dash {
            cooldown_ms,
            mana_cost,
            distance: require_present_u16(source, "distance", fields.distance)?,
            effect,
            impact_radius: fields.impact_radius,
            payload: parse_optional_behavior_payload(
                source,
                yaml.payload.clone(),
                schema.payload,
                mechanics,
            )?,
        }),
        "burst" => Ok(SkillBehavior::Burst {
            cooldown_ms,
            mana_cost,
            range: require_present_u16(source, "range", fields.range)?,
            radius: require_present_u16(source, "radius", fields.radius)?,
            effect,
            payload: parse_behavior_payload(
                source,
                yaml.payload.clone(),
                schema.payload,
                mechanics,
            )?,
        }),
        "nova" => Ok(SkillBehavior::Nova {
            cooldown_ms,
            mana_cost,
            radius: require_present_u16(source, "radius", fields.radius)?,
            effect,
            payload: parse_behavior_payload(
                source,
                yaml.payload.clone(),
                schema.payload,
                mechanics,
            )?,
        }),
        other => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("unknown behavior kind '{other}'"),
        }),
    }
}

pub(super) fn parse_payload(
    source: &str,
    yaml: Option<EffectPayloadYaml>,
    field: &str,
    mechanics: &MechanicCatalog,
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
            .map(|status| parse_status(source, status, mechanics))
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

fn parse_status(
    source: &str,
    yaml: &StatusYaml,
    mechanics: &MechanicCatalog,
) -> Result<StatusDefinition, ContentError> {
    let definition = mechanics
        .status(&yaml.kind)
        .ok_or_else(|| ContentError::Validation {
            source: String::from(source),
            message: format!("unknown status kind '{}'", yaml.kind),
        })?;
    let schema = definition
        .status_schema
        .as_ref()
        .ok_or_else(|| ContentError::Validation {
            source: String::from(source),
            message: format!("status '{}' is missing a schema definition", yaml.kind),
        })?;
    let kind = parse_status_kind(source, &yaml.kind)?;
    let duration_ms = require_present_u16(
        source,
        "status.duration_ms",
        read_numeric_field(
            source,
            "status.duration_ms",
            Some(yaml.duration_ms),
            schema_numeric_rule(&schema.numeric_fields, "duration_ms"),
        )?,
    )?;
    let tick_interval_ms = read_numeric_field(
        source,
        "status.tick_interval_ms",
        yaml.tick_interval_ms,
        schema_numeric_rule(&schema.numeric_fields, "tick_interval_ms"),
    )?;
    let magnitude = read_numeric_field(
        source,
        "status.magnitude",
        Some(yaml.magnitude),
        schema_numeric_rule(&schema.numeric_fields, "magnitude"),
    )?
    .unwrap_or(0);
    let trigger_duration_ms = read_numeric_field(
        source,
        "status.trigger_duration_ms",
        yaml.trigger_duration_ms,
        schema_numeric_rule(&schema.numeric_fields, "trigger_duration_ms"),
    )?;
    let max_stacks = validate_max_stacks(
        source,
        kind,
        yaml.max_stacks.unwrap_or(1),
        schema.max_stacks,
    )?;

    Ok(StatusDefinition {
        kind,
        duration_ms,
        tick_interval_ms,
        magnitude,
        max_stacks,
        trigger_duration_ms,
    })
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

fn behavior_schema<'a>(
    mechanics: &'a MechanicCatalog,
    source: &str,
    kind: &str,
) -> Result<&'a BehaviorSchema, ContentError> {
    let definition = mechanics
        .behavior(kind)
        .ok_or_else(|| ContentError::Validation {
            source: String::from(source),
            message: format!("unknown behavior kind '{kind}'"),
        })?;
    if !definition.implemented {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!("behavior kind '{kind}' is not implemented yet"),
        });
    }
    definition
        .behavior_schema
        .as_ref()
        .ok_or_else(|| ContentError::Validation {
            source: String::from(source),
            message: format!("behavior kind '{kind}' is missing a schema definition"),
        })
}

fn validate_allowed_effect(
    source: &str,
    kind: &str,
    effect: SkillEffectKind,
    allowed: &[SkillEffectKind],
) -> Result<(), ContentError> {
    if !allowed.contains(&effect) {
        return Err(ContentError::Validation {
            source: String::from(source),
            message: format!("effect '{effect:?}' is not valid for behavior kind '{kind}'"),
        });
    }
    Ok(())
}

fn schema_numeric_rule(
    numeric_fields: &BTreeMap<String, NumericFieldRule>,
    field: &str,
) -> NumericFieldRule {
    numeric_fields
        .get(field)
        .copied()
        .unwrap_or(NumericFieldRule::Forbidden)
}

fn read_numeric_field(
    source: &str,
    field: &str,
    value: Option<u16>,
    rule: NumericFieldRule,
) -> Result<Option<u16>, ContentError> {
    match (rule, value) {
        (NumericFieldRule::Required, Some(0)) => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{field} must be greater than zero"),
        }),
        (NumericFieldRule::Required, Some(value)) => Ok(Some(value)),
        (NumericFieldRule::Required, None) => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{field} is required"),
        }),
        (NumericFieldRule::Optional, Some(0)) => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{field} must be greater than zero when provided"),
        }),
        (NumericFieldRule::Optional, value) => Ok(value),
        (NumericFieldRule::Zero, Some(0) | None) => Ok(Some(0)),
        (NumericFieldRule::Zero, Some(_)) => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{field} must be zero for this mechanic"),
        }),
        (NumericFieldRule::Forbidden, Some(_)) => Err(ContentError::Validation {
            source: String::from(source),
            message: format!("{field} is not valid for this mechanic"),
        }),
        (NumericFieldRule::Forbidden, None) => Ok(None),
    }
}

fn require_present_u16(source: &str, field: &str, value: Option<u16>) -> Result<u16, ContentError> {
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

fn parse_behavior_payload(
    source: &str,
    payload: Option<EffectPayloadYaml>,
    rule: PayloadFieldRule,
    mechanics: &MechanicCatalog,
) -> Result<EffectPayload, ContentError> {
    match rule {
        PayloadFieldRule::Required | PayloadFieldRule::Optional => {
            parse_payload(source, payload, "behavior.payload", mechanics)
        }
        PayloadFieldRule::Forbidden => Err(ContentError::Validation {
            source: String::from(source),
            message: String::from("behavior.payload is not valid for this mechanic"),
        }),
    }
}

fn parse_optional_behavior_payload(
    source: &str,
    payload: Option<EffectPayloadYaml>,
    rule: PayloadFieldRule,
    mechanics: &MechanicCatalog,
) -> Result<Option<EffectPayload>, ContentError> {
    match rule {
        PayloadFieldRule::Required => {
            parse_payload(source, payload, "behavior.payload", mechanics).map(Some)
        }
        PayloadFieldRule::Optional => payload
            .map(|payload| parse_payload(source, Some(payload), "behavior.payload", mechanics))
            .transpose(),
        PayloadFieldRule::Forbidden => {
            if payload.is_some() {
                return Err(ContentError::Validation {
                    source: String::from(source),
                    message: String::from("behavior.payload is not valid for this mechanic"),
                });
            }
            Ok(None)
        }
    }
}

fn validate_max_stacks(
    source: &str,
    kind: StatusKind,
    max_stacks: u8,
    rule: StackRule,
) -> Result<u8, ContentError> {
    match rule {
        StackRule::Positive => {
            if max_stacks == 0 {
                return Err(ContentError::Validation {
                    source: String::from(source),
                    message: format!("status '{kind:?}' max_stacks must be greater than zero"),
                });
            }
            Ok(max_stacks)
        }
        StackRule::One => {
            if max_stacks != 1 {
                return Err(ContentError::Validation {
                    source: String::from(source),
                    message: format!("status '{kind:?}' max_stacks must be exactly one"),
                });
            }
            Ok(1)
        }
    }
}

pub(crate) fn parse_effect_kind(source: &str, raw: &str) -> Result<SkillEffectKind, ContentError> {
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

pub(super) fn require_positive_u16(
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
