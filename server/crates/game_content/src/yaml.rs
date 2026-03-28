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
    pub(super) behavior: SkillBehaviorYaml,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EffectPayloadYaml {
    pub(super) kind: String,
    pub(super) amount: Option<u16>,
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
    pub(super) payload: Option<EffectPayloadYaml>,
}
