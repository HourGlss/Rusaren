# Skill Authoring Reference

This document describes the skill authoring surface that is actually accepted by the Rust loader and actually executed by the simulation. It is intentionally based on the source code in:

- `server/crates/game_content/src/yaml.rs`
- `server/crates/game_content/src/skills/mod.rs`
- `server/crates/game_content/src/skills/behavior.rs`
- `server/crates/game_content/src/model.rs`
- `server/crates/game_sim/src/actions.rs`
- `server/crates/game_sim/src/effects.rs`
- `server/crates/game_sim/src/ticks.rs`
- `server/crates/game_sim/src/lib.rs`

If this README and the code disagree, the code wins.

## Authoring Contract

Every `content/skills/*.yaml` file is deserialized with `serde` and `deny_unknown_fields`.

That means:

- unknown keys are hard errors
- misspelled keys are hard errors
- extra keys are hard errors
- field names are case-sensitive

The loader reads every `.yaml` file in this directory, sorts the paths, and loads them all into one global catalog.

## Global Rules

These rules are enforced before any gameplay logic runs.

### File-Level Rules

- A skill file must contain:
  - `tree`
  - `melee`
  - `skills`
- `skills` must contain exactly `5` entries.
- Tiers must be exactly `1`, `2`, `3`, `4`, and `5`.
- A tier may appear only once inside its file.

### Tree Name Rules

`tree` is parsed by `SkillTree::new`.

Accepted examples:

- `Warrior`
- `Rogue`
- `Mage`
- `Cleric`
- `Paladin`
- `Necromancer`
- `Ranger`
- `My Custom Tree`

Rejected tree names:

- empty or whitespace-only names
- names longer than `32` characters
- names containing characters other than ASCII letters, digits, space, `_`, or `-`

Known built-in tree names are:

- `Warrior`
- `Rogue`
- `Mage`
- `Cleric`

Custom tree names are accepted by the content loader. The loader does not require every tree to be one of the built-in four names.

### Text Length Rules

These fields must be non-empty and at most `120` characters:

- `melee.id`
- `melee.name`
- `melee.description`
- `skill.id`
- `skill.name`
- `skill.description`

### Global ID Uniqueness

Authored IDs are globally unique across all loaded skill files.

The loader rejects:

- two classes using the same melee `id`
- a melee `id` that matches any skill `id`
- two skill entries anywhere in the catalog using the same `id`

### Audio Cue ID Rules

`audio_cue_id` is optional on:

- `melee`
- each tiered `skill`

When present, `audio_cue_id` must use only:

- lowercase ASCII letters
- digits
- `_`
- `-`

Examples:

- valid: `mage_arc_bolt`
- valid: `rogue-nightcloak`
- invalid: `MageArcBolt`
- invalid: `cleric minor heal`

## Complete File Shape

```yaml
tree: Rogue
melee:
  id: rogue_dual_cut
  name: Dual Cut
  description: Fast melee hit.
  audio_cue_id: rogue_dual_cut
  cooldown_ms: 450
  range: 86
  radius: 38
  effect: melee_swing
  payload:
    kind: damage
    amount: 22
skills:
  - tier: 1
    id: rogue_venom_shiv
    name: Venom Shiv
    description: Projectile damage that applies poison.
    audio_cue_id: rogue_venom_shiv
    behavior:
      kind: projectile
      effect: skill_shot
      cooldown_ms: 650
      cast_time_ms: 0
      mana_cost: 14
      speed: 360
      range: 1500
      radius: 14
      payload:
        kind: damage
        amount: 10
        status:
          kind: poison
          duration_ms: 3000
          tick_interval_ms: 1000
          magnitude: 5
          max_stacks: 5
```

## Top-Level Keys

### `tree`

String. Required.

Examples:

- `Warrior`
- `Cleric`
- `Druid`

### `melee`

Object. Required.

This defines the always-available primary attack for the class.

### `skills`

Array of tier definitions. Required.

Must contain exactly five entries, one for each tier `1..=5`.

## `melee` Object

Allowed keys:

- `id`
- `name`
- `description`
- `audio_cue_id`
- `cooldown_ms`
- `range`
- `radius`
- `effect`
- `payload`

### `melee.cooldown_ms`

Required positive `u16`.

Example:

- `450` means a `0.45` second melee cooldown
- `650` means a `0.65` second melee cooldown

### `melee.range`

Required positive `u16`.

This is the forward reach in world units.

### `melee.radius`

Required positive `u16`.

This is the impact radius around the projected target point.

### `melee.effect`

Required string parsed by `parse_effect_kind`.

Accepted strings:

- `melee_swing`
- `skill_shot`
- `dash_trail`
- `burst`
- `nova`
- `beam`
- `hit_spark`

### `melee.payload`

Required `EffectPayload`.

Unlike behavior payloads, melee payloads are not constrained by a behavior schema. If the payload itself parses, the melee definition accepts it.

## Tiered Skill Entries

Each entry under `skills:` accepts:

- `tier`
- `id`
- `name`
- `description`
- `audio_cue_id`
- `behavior`

### `tier`

Required `u8`.

Must be one of:

- `1`
- `2`
- `3`
- `4`
- `5`

## `behavior` Object

The `behavior` object accepts this full key set:

- `kind`
- `effect`
- `cooldown_ms`
- `cast_time_ms`
- `mana_cost`
- `range`
- `radius`
- `distance`
- `speed`
- `impact_radius`
- `duration_ms`
- `hit_points`
- `tick_interval_ms`
- `player_speed_bps`
- `projectile_speed_bps`
- `cooldown_bps`
- `cast_time_bps`
- `toggleable`
- `cast_start_payload`
- `cast_end_payload`
- `payload`

Not every behavior kind accepts every field. The validator rejects forbidden fields per mechanic schema.

### Shared Behavior Keys

#### `behavior.kind`

Required string. Dispatchable values are:

- `projectile`
- `beam`
- `dash`
- `burst`
- `nova`
- `teleport`
- `channel`
- `passive`
- `summon`
- `ward`
- `trap`
- `barrier`
- `aura`

Important limitation:

- `interrupt` and `dispel` appear in the mechanics registry, but they are not dispatchable `behavior.kind` values.
- The runtime treats them as payload modifiers, not standalone behaviors.

#### `behavior.effect`

Required string. Accepted values:

- `melee_swing`
- `skill_shot`
- `dash_trail`
- `burst`
- `nova`
- `beam`
- `hit_spark`

Each behavior kind has its own allow-list. Examples:

- `projectile` accepts `skill_shot`
- `beam` accepts `beam` and `skill_shot`
- `dash` accepts `dash_trail`
- `aura` accepts `nova`

#### `behavior.cooldown_ms`

Usually required. Positive milliseconds.

Example:

- `900` means `0.9` seconds
- `3200` means `3.2` seconds

#### `behavior.cast_time_ms`

Optional on many active skills. Positive when present.

If omitted, the cast time is `0`.

#### `behavior.mana_cost`

Optional on many active skills. Positive when present.

If omitted, the mana cost is `0`.

#### `behavior.range`

World units.

Used by:

- `projectile`
- `beam`
- `burst`
- `channel`
- `summon`

Channel nuance:

- `channel.range` is optional.
- If omitted or effectively `0`, the channel is centered on the caster.
- If positive, the channel center is aimed outward up to that distance and clipped by obstacles.

#### `behavior.radius`

World units.

Used for:

- projectile collision radius
- beam thickness
- burst area
- nova area
- deployable radius
- ward vision radius
- trap trigger radius
- aura pulse radius
- barrier footprint half-width and half-height

#### `behavior.distance`

World units.

Used for:

- `dash`
- `teleport`
- `summon`
- `ward`
- `trap`
- `barrier`
- some non-toggle auras

It is the placement or movement distance projected from the current aim.

#### `behavior.speed`

World units per second.

Used only by `projectile`.

Example:

- `300` means the projectile travels `300` units per second before passive/global scaling

#### `behavior.impact_radius`

Optional positive `u16`.

Used only by `dash`.

If present, the dash can apply its optional payload in a radius at the destination.

#### `behavior.duration_ms`

Milliseconds.

Used by:

- `channel`
- `summon`
- `trap`
- `barrier`
- `ward`
- `aura`

Special cases:

- `ward.duration_ms: 0` is valid and means the ward lasts until killed
- `summon`, `trap`, `barrier`, and non-toggle auras require positive duration
- `toggleable: true` auras still require `duration_ms`, but runtime does not count them down while active

#### `behavior.hit_points`

Positive `u16`.

Used by:

- `summon`
- `trap`
- `barrier`
- `ward`
- optionally `aura`

Aura nuance:

- if `hit_points` is present, the aura becomes a placed deployable entity
- if `hit_points` is absent, the aura is anchored to the player

#### `behavior.tick_interval_ms`

Positive `u16`.

Used by:

- `channel`
- `summon`
- `aura`

Meaning:

- `channel`: time between repeated pulse applications
- `summon`: time between automatic attacks
- `aura`: time between aura pulses

#### Passive Basis-Point Fields

These exist only for `passive`.

- `player_speed_bps`
- `projectile_speed_bps`
- `cooldown_bps`
- `cast_time_bps`

All are optional positive values when provided. At least one of them must be non-zero.

Interpretation:

- `100` = `1%`
- `500` = `5%`
- `900` = `9%`
- `1500` = `15%`

Runtime formulas:

- move speed scales by `(10000 + player_speed_bps) / 10000`
- projectile speed scales by `(10000 + projectile_speed_bps) / 10000`
- cooldown scales by `(10000 - cooldown_bps) / 10000`
- cast time scales by `(10000 - cast_time_bps) / 10000`

Examples:

- `player_speed_bps: 1500` means `+15%` move speed
- `projectile_speed_bps: 2000` means `+20%` projectile speed
- `cooldown_bps: 900` means cooldowns become `91%` of base
- `cast_time_bps: 2500` means cast times become `75%` of base

Passive caps in runtime:

- `player_speed_bps` total passive bonus caps at `9000`
- `projectile_speed_bps` total passive bonus caps at `9000`
- `cooldown_bps` total passive reduction caps at `9000`
- `cast_time_bps` total passive reduction caps at `9500`

#### `behavior.proc_reset`

Optional object. Valid only on `passive`.

Accepted keys:

- `trigger`
- `source_skill_ids`
- `reset_skill_ids`
- `instacast_skill_ids`
- `instacast_costs_mana`
- `instacast_starts_cooldown`
- `internal_cooldown_ms`

Runtime meaning:

- listens for the authored trigger on the passive owner
- optionally filters to the listed `source_skill_ids` including melee IDs
- resets cooldowns for each matching `reset_skill_ids`
- grants one pending instant cast for `instacast_skill_ids`
- `instacast_costs_mana: false` makes the consumed proc free
- `instacast_starts_cooldown: false` keeps the consumed proc cast from starting cooldown
- `internal_cooldown_ms` throttles how often the passive can trigger

Accepted trigger strings:

- `on_hit`
- `on_crit`
- `on_heal`
- `on_tick`

Example:

```yaml
behavior:
  kind: passive
  effect: nova
  proc_reset:
    trigger: on_hit
    source_skill_ids: [rogue_venom_shiv]
    reset_skill_ids: [rogue_veil_step, rogue_shadow_dance]
    instacast_skill_ids: [rogue_lullwire_trap]
    instacast_costs_mana: false
    instacast_starts_cooldown: false
    internal_cooldown_ms: 12000
```

#### `behavior.toggleable`

Boolean. Only valid on `aura`.

Runtime meaning:

- recasting the same slot toggles the aura off
- a toggleable aura persists until canceled or broken
- toggleable stealth auras are canceled by taking an action or damage

Additional parser restrictions:

- toggleable auras must be self-anchored
- toggleable auras may not set `distance`
- toggleable auras may not set `hit_points`

#### `behavior.cast_start_payload`

Optional payload. Only valid on `aura`.

Runtime meaning:

- applied immediately when the aura starts
- uses the aura center and aura radius
- targets the same way a normal aura pulse does

#### `behavior.cast_end_payload`

Optional payload. Only valid on `aura`.

Runtime meaning:

- applied when the aura ends naturally
- also applied when a toggleable aura is toggled off

#### `behavior.payload`

Required or optional depending on behavior kind.

Examples:

- required for `projectile`, `beam`, `burst`, `nova`, `channel`, `summon`, `trap`, `aura`
- optional for `dash`
- forbidden for `teleport`, `ward`, `barrier`, `passive`

## Behavior Kinds In Detail

### `projectile`

Required fields:

- `kind`
- `effect: skill_shot`
- `cooldown_ms`
- `speed`
- `range`
- `radius`
- `payload`

Optional:

- `cast_time_ms`
- `mana_cost`

Runtime behavior:

- spawns a moving projectile
- stops on the first valid target hit
- can be blocked by movement/projectile blockers
- expires when range runs out

### `beam`

Required fields:

- `kind`
- `effect: beam` or `skill_shot`
- `cooldown_ms`
- `range`
- `radius`
- `payload`

Optional:

- `cast_time_ms`
- `mana_cost`

Runtime behavior:

- resolves immediately along a line
- hits the first valid target on that line

### `dash`

Required fields:

- `kind`
- `effect: dash_trail`
- `cooldown_ms`
- `distance`

Optional:

- `cast_time_ms`
- `mana_cost`
- `impact_radius`
- `payload`

Runtime behavior:

- moves the caster
- optional payload can apply in the authored `impact_radius` at the destination

### `burst`

Required fields:

- `kind`
- `effect: burst`
- `cooldown_ms`
- `range`
- `radius`
- `payload`

Optional:

- `cast_time_ms`
- `mana_cost`

Runtime behavior:

- creates a targeted area effect centered at the aimed point

### `nova`

Required fields:

- `kind`
- `effect: nova`
- `cooldown_ms`
- `radius`
- `payload`

Optional:

- `cast_time_ms`
- `mana_cost`

Runtime behavior:

- pulses around the caster

### `teleport`

Required fields:

- `kind`
- `effect: dash_trail`
- `cooldown_ms`
- `distance`

Optional:

- `cast_time_ms`
- `mana_cost`

Forbidden:

- payloads
- toggleable

Runtime behavior:

- repositions instantly
- ignores walls when computing the desired location
- then clamps to a valid landing point

### `channel`

Required fields:

- `kind`
- `effect: nova` or `burst`
- `cooldown_ms`
- `radius`
- `duration_ms`
- `tick_interval_ms`
- `payload`

Optional:

- `cast_time_ms`
- `mana_cost`
- `range`

Runtime behavior:

- after any cast windup, enters channel mode
- movement cancels the channel
- manual cancel stops future ticks
- each tick reapplies the payload
- `range: 0` or omitted means caster-centered

### `passive`

Required fields:

- `kind`
- one or more passive basis-point fields

Accepted effect:

- the mechanics registry currently allows `nova`

Runtime behavior:

- cannot be cast
- modifies stats as long as it is equipped

### `summon`

Required fields:

- `kind`
- `effect: skill_shot`, `burst`, or `nova`
- `cooldown_ms`
- `distance`
- `radius`
- `duration_ms`
- `hit_points`
- `range`
- `tick_interval_ms`
- `payload`

Optional:

- `cast_time_ms`
- `mana_cost`

Runtime behavior:

- spawns an allied entity
- entity automatically attacks nearest valid target in `range`
- attacks every `tick_interval_ms`
- entity can be damaged and killed

### `ward`

Required fields:

- `kind`
- `effect: nova`
- `cooldown_ms`
- `distance`
- `radius`
- `duration_ms`
- `hit_points`

Optional:

- `cast_time_ms`
- `mana_cost`

Forbidden:

- `payload`
- `toggleable`
- aura lifecycle payloads

Runtime behavior:

- spawns a vision source
- `radius` is the ward vision radius
- `duration_ms: 0` means persistent until killed
- ward has hit points and can be destroyed

### `trap`

Required fields:

- `kind`
- `effect: burst` or `nova`
- `cooldown_ms`
- `distance`
- `radius`
- `duration_ms`
- `hit_points`
- `payload`

Optional:

- `cast_time_ms`
- `mana_cost`

Runtime behavior:

- spawns a trap
- waits for an enemy player to enter the trigger radius
- applies payload once
- expires immediately after firing

### `barrier`

Required fields:

- `kind`
- `effect: burst` or `nova`
- `cooldown_ms`
- `distance`
- `radius`
- `duration_ms`
- `hit_points`

Optional:

- `cast_time_ms`
- `mana_cost`

Runtime behavior:

- creates temporary cover
- blocks movement
- blocks projectiles
- does not block vision

### `aura`

Required fields:

- `kind`
- `effect: nova`
- `cooldown_ms`
- `radius`
- `duration_ms`
- `tick_interval_ms`
- `payload`

Optional:

- `cast_time_ms`
- `mana_cost`
- `distance`
- `hit_points`
- `toggleable`
- `cast_start_payload`
- `cast_end_payload`

Runtime behavior:

- pulses its payload every `tick_interval_ms`
- without `hit_points`, it anchors to the caster
- with `hit_points`, it becomes a placed deployable
- `cast_start_payload` fires immediately on creation
- `cast_end_payload` fires on expiration or toggle-off

## `payload` Object

Accepted keys:

- `kind`
- `amount`
- `status`
- `interrupt_silence_duration_ms`
- `dispel`

At least one of these must contribute something meaningful:

- positive `amount`
- `status`
- `interrupt_silence_duration_ms`
- `dispel`

A payload with all of those absent or zero is rejected.

### `payload.kind`

Required string.

Accepted values:

- `damage`
- `heal`

### `payload.amount`

Optional `u16`. Defaults to `0`.

Examples:

- `amount: 18` means direct damage or healing of `18`
- `amount: 0` is valid when the payload is pure status, pure dispel, or pure interrupt

### `payload.amount_min` / `payload.amount_max`

Optional `u16` pair for direct variable results.

Rules:

- author both together or neither
- do not combine them with `payload.amount`
- runtime rolls an inclusive value between them for each target hit

### `payload.crit_chance_bps`

Optional positive `u16`.

Interpretation:

- `100` = `1%`
- `1500` = `15%`
- `10000` = `100%`

Requires a direct amount or amount range.

### `payload.crit_multiplier_bps`

Optional `u16`, used only when `payload.crit_chance_bps` is present.

Interpretation:

- `15000` = `150%` result on crit
- `20000` = `200%` result on crit

Defaults to `15000` when omitted and `payload.crit_chance_bps` is present.

### `payload.interrupt_silence_duration_ms`

Optional positive `u16`.

Runtime behavior:

- if the target is actively casting, cancel that cast
- then apply a silence for the authored duration

### `payload.dispel`

Optional object.

Keys:

- `scope`
- `max_statuses`

#### `payload.dispel.scope`

Accepted strings:

- `positive`
- `negative`
- `all`

Runtime category map:

- positive:
  - `hot`
  - `haste`
  - `shield`
  - `stealth`
- negative:
  - `poison`
  - `chill`
  - `root`
  - `silence`
  - `stun`
  - `sleep`
  - `reveal`
  - `fear`
  - `healing_reduction`
- all:
  - everything

#### `payload.dispel.max_statuses`

Optional positive `u8`. Defaults to `1`.

The runtime removes up to that many eligible statuses, preferring longer remaining durations first.

### Payload Application Order

When a payload resolves, runtime order is:

1. damage or heal
2. interrupt and silence
3. dispel
4. status application

## `status` Object

Accepted keys:

- `kind`
- `duration_ms`
- `tick_interval_ms`
- `magnitude`
- `max_stacks`
- `trigger_duration_ms`
- `expire_payload`
- `dispel_payload`

### Supported Status Kinds

Accepted strings:

- `poison`
- `hot`
- `chill`
- `root`
- `haste`
- `silence`
- `stun`
- `sleep`
- `shield`
- `stealth`
- `reveal`
- `fear`
- `healing_reduction`

### Status Field Meanings

#### `status.duration_ms`

Required for every current status kind.

Milliseconds before the status expires, subject to crowd-control diminishing returns for some statuses.

#### `status.tick_interval_ms`

Used only by periodic statuses such as:

- `poison`
- `hot`

#### `status.magnitude`

Meaning depends on the status:

- `poison`: damage per tick, before stack multiplication
- `hot`: healing per tick, before stack multiplication
- `chill`: slow in basis points per stack
- `haste`: move-speed bonus in basis points
- `shield`: absorb amount per stack
- `healing_reduction`: reduced healing received in basis points
- `root`, `silence`, `stun`, `sleep`, `stealth`, `reveal`, `fear`: must be `0`

#### `status.max_stacks`

Optional `u8`. Defaults to `1`.

Examples:

- `poison max_stacks: 5`
- `shield max_stacks: 3`
- `sleep max_stacks: 1`

#### `status.trigger_duration_ms`

Currently meaningful only for `chill`.

When chill reaches its authored `max_stacks` and `trigger_duration_ms` is present, runtime applies `root` for that duration.

#### `status.expire_payload`

Optional nested payload, applied when the status expires naturally.

#### `status.dispel_payload`

Optional nested payload, applied when the status is removed by dispel.

## Runtime Status Semantics

- `poison`: periodic damage, `magnitude * stacks` each tick
- `hot`: periodic healing, `magnitude * stacks` each tick
- `chill`: slow, and may escalate into `root`
- `root`: blocks movement
- `haste`: boosts movement speed
- `silence`: blocks casts
- `stun`: blocks movement and actions
- `sleep`: behaves like hard CC and breaks on damage
- `shield`: absorbs damage before hit points
- `stealth`: prevents enemy targeting until damage or action breaks it
- `reveal`: allows enemies to target stealthed units
- `fear`: blocks actions and forces retreat from the source
- `healing_reduction`: reduces incoming healing and does not stack additively at runtime; strongest wins

## Crowd-Control Diminishing Returns

Runtime DR window is `15000` ms.

Buckets:

- hard CC:
  - `stun`
  - `sleep`
  - `fear`
- movement CC:
  - `root`
- cast CC:
  - `silence`

Duration scaling inside the DR window:

- first: `100%`
- second: `50%`
- third: `25%`
- fourth and later: immune until the bucket window expires

## Worked Combinations

### WoW-Style Toggle Stealth Aura

Current live example: `rogue_nightcloak`.

Pattern:

- `kind: aura`
- `toggleable: true`
- no `distance`
- no `hit_points`
- `cast_start_payload` applies `stealth`
- recurring `payload` also applies `stealth`
- use `amount: 0`

### Persistent Vision Ward

Current live example: `cleric_lantern_ward`.

Pattern:

- `kind: ward`
- `duration_ms: 0`
- `hit_points` positive
- `radius` is the ward vision radius

### Summon That Roots

Current live example: `druid_thornbark_treant`.

Pattern:

- `kind: summon`
- author a repeated attack cadence with `tick_interval_ms`
- put the crowd control in the summon payload

### Heal That Blooms On Expire Or Dispel

Current live example: `druid_lifebloom`.

Pattern:

- direct heal in `payload.amount`
- periodic heal in `payload.status`
- extra bloom in both `status.expire_payload` and `status.dispel_payload`

## Practical Authoring Advice

- Start from a live skill that already uses the same behavior kind.
- Keep units consistent:
  - `_ms` is milliseconds
  - `range`, `radius`, `distance`, and `impact_radius` are world units
  - `speed` is world units per second
  - `_bps` is basis points
- If a field is not listed by the mechanic schema for that behavior, do not author it.
- If you want a pure status spell, `amount: 0` is valid as long as the payload carries a status, dispel, or interrupt.
- If you want a status to stack, do not forget `max_stacks`.
- If you want a ward to last until destroyed, set `duration_ms: 0`.
- If you want a toggle aura, it must be an `aura`; no other behavior accepts `toggleable`.
