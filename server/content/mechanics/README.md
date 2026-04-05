# Mechanics Registry Reference

This document explains `content/mechanics/registry.yaml` using the Rust code as the source of truth. It is based on:

- `server/crates/game_content/src/yaml.rs`
- `server/crates/game_content/src/mechanics.rs`
- `server/crates/game_content/src/skills/behavior.rs`
- `server/crates/game_content/src/model.rs`
- `server/crates/game_sim/src/actions.rs`
- `server/crates/game_sim/src/effects.rs`
- `server/crates/game_sim/src/ticks.rs`
- `server/crates/game_sim/src/lib.rs`

The registry does two jobs:

1. it documents the intended mechanic surface
2. it constrains which fields are legal for already-implemented behavior and status kinds

It is not a fully data-driven plug-in system. New mechanic names still require Rust support if they are not already hard-coded.

## Top-Level File Shape

`registry.yaml` must contain:

- `behaviors`
- `statuses`

Both arrays must be non-empty.

Each entry in `behaviors` or `statuses` accepts:

- `id`
- `label`
- `implemented`
- `inspiration`
- `notes`
- `schema`

All entries use `deny_unknown_fields`.

## What The Registry Can And Cannot Do

The registry can:

- constrain which numeric fields are allowed for a behavior kind
- constrain whether payload fields are required, optional, or forbidden
- constrain which effect visuals are allowed for a behavior kind
- constrain stack rules and trigger-payload rules for statuses
- carry design notes and labels for human readers

The registry cannot by itself:

- add a new dispatchable `behavior.kind`
- add a new `status.kind`
- define new runtime logic for an existing kind

## Mechanic Entry Fields

### `id`

Required non-empty string, max `120` chars.

Duplicate IDs are rejected inside the same category:

- duplicate behavior IDs are rejected
- duplicate status IDs are rejected

### `label`

Required non-empty string, max `120` chars.

Purely descriptive.

### `implemented`

Required boolean.

Meaning:

- for behaviors:
  - `implemented: true` requires a schema
  - behavior parsing refuses a kind whose registry entry says `implemented: false`
- for statuses:
  - the registry stores the flag, but status authoring is still primarily gated by the hard-coded `StatusKind` enum and schema availability

### `inspiration`

Required non-empty string, max `120` chars.

Purely descriptive.

### `notes`

Required non-empty string, max `120` chars.

Purely descriptive.

### `schema`

Optional object at the YAML layer.

But:

- implemented behavior entries must provide one
- implemented status entries must provide one

## Schema Keys

The schema object accepts:

- `numeric_fields`
- `payload`
- `cast_start_payload`
- `cast_end_payload`
- `expire_payload`
- `dispel_payload`
- `allowed_effects`
- `max_stacks`

### Behavior Schema Meaning

Behavior entries use:

- `numeric_fields`
- `payload`
- `cast_start_payload`
- `cast_end_payload`
- `allowed_effects`

Behavior entries ignore:

- `expire_payload`
- `dispel_payload`
- `max_stacks`

### Status Schema Meaning

Status entries use:

- `numeric_fields`
- `max_stacks`
- `expire_payload`
- `dispel_payload`

Status entries ignore:

- `payload`
- `cast_start_payload`
- `cast_end_payload`
- `allowed_effects`

## Supported Numeric Field Names

Behavior numeric field names are fixed to this exact list:

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

Status numeric field names are fixed to this exact list:

- `duration_ms`
- `tick_interval_ms`
- `magnitude`
- `trigger_duration_ms`

## Rule Enums

### Numeric Rules

Accepted strings:

- `required`
- `optional`
- `non_negative`
- `zero`
- `forbidden`

#### `required`

- field must be present
- value must be greater than zero

#### `optional`

- field may be omitted
- if provided, value must be greater than zero

#### `non_negative`

- field must be present
- value may be zero

Important live case:

- `ward.duration_ms: 0` is accepted and means persistent until killed

#### `zero`

- field may be omitted or explicitly set to `0`
- any non-zero value is rejected

#### `forbidden`

- field must not be present

### Payload Field Rules

Accepted strings:

- `required`
- `optional`
- `forbidden`

The same rule family is used for:

- `payload`
- `cast_start_payload`
- `cast_end_payload`
- `expire_payload`
- `dispel_payload`

### Stack Rules

Accepted strings:

- `positive`
- `one`

Meaning:

- `positive`: `max_stacks` must be greater than zero
- `one`: `max_stacks` must be exactly `1`

## Allowed Effect Strings

The registry uses `allowed_effects`, and those values are parsed through `parse_effect_kind`.

Accepted effect strings are:

- `melee_swing`
- `skill_shot`
- `dash_trail`
- `burst`
- `nova`
- `beam`
- `hit_spark`

## Current Dispatchable Behavior Kinds

These `id` values are both present in the registry and actually dispatched by the parser/runtime:

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

## Current Helper-Only Behavior IDs

These behavior entries exist in the registry, but are not legal `behavior.kind` values today:

- `interrupt`
- `dispel`

The correct authoring pattern is:

- interrupt: `payload.interrupt_silence_duration_ms`
- dispel: `payload.dispel`

## Current Authorable Status Kinds

These are hard-coded in `parse_status_kind`:

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

Adding a new status entry to the registry alone is not enough. The Rust enum and parser must also know that status.

## Current Behavior Registry, Interpreted

### `projectile`

- payload: required
- allowed effects: `skill_shot`
- required numeric fields:
  - `cooldown_ms`
  - `range`
  - `radius`
  - `speed`
- optional:
  - `cast_time_ms`
  - `mana_cost`

### `beam`

- payload: required
- allowed effects:
  - `beam`
  - `skill_shot`
- required:
  - `cooldown_ms`
  - `range`
  - `radius`
- optional:
  - `cast_time_ms`
  - `mana_cost`

### `dash`

- payload: optional
- allowed effects:
  - `dash_trail`
- required:
  - `cooldown_ms`
  - `distance`
- optional:
  - `cast_time_ms`
  - `mana_cost`
  - `impact_radius`

### `burst`

- payload: required
- allowed effects:
  - `burst`
- required:
  - `cooldown_ms`
  - `range`
  - `radius`
- optional:
  - `cast_time_ms`
  - `mana_cost`

### `nova`

- payload: required
- allowed effects:
  - `nova`
- required:
  - `cooldown_ms`
  - `radius`
- optional:
  - `cast_time_ms`
  - `mana_cost`

### `summon`

- payload: required
- allowed effects:
  - `skill_shot`
  - `burst`
  - `nova`
- required:
  - `cooldown_ms`
  - `distance`
  - `radius`
  - `duration_ms`
  - `hit_points`
  - `range`
  - `tick_interval_ms`
- optional:
  - `cast_time_ms`
  - `mana_cost`

### `barrier`

- payload: forbidden
- allowed effects:
  - `burst`
  - `nova`
- required:
  - `cooldown_ms`
  - `distance`
  - `radius`
  - `duration_ms`
  - `hit_points`
- optional:
  - `cast_time_ms`
  - `mana_cost`

### `trap`

- payload: required
- allowed effects:
  - `burst`
  - `nova`
- required:
  - `cooldown_ms`
  - `distance`
  - `radius`
  - `duration_ms`
  - `hit_points`
- optional:
  - `cast_time_ms`
  - `mana_cost`

### `aura`

- payload: required
- cast_start_payload: optional
- cast_end_payload: optional
- allowed effects:
  - `nova`
- required:
  - `cooldown_ms`
  - `radius`
  - `duration_ms`
  - `tick_interval_ms`
- optional:
  - `cast_time_ms`
  - `mana_cost`
  - `distance`
  - `hit_points`

### `teleport`

- payload: forbidden
- allowed effects:
  - `dash_trail`
- required:
  - `cooldown_ms`
  - `distance`
- optional:
  - `cast_time_ms`
  - `mana_cost`

### `channel`

- payload: required
- allowed effects:
  - `nova`
  - `burst`
- required:
  - `cooldown_ms`
  - `radius`
  - `duration_ms`
  - `tick_interval_ms`
- optional:
  - `cast_time_ms`
  - `mana_cost`
  - `range`

### `ward`

- payload: forbidden
- allowed effects:
  - `nova`
- required:
  - `cooldown_ms`
  - `distance`
  - `radius`
  - `duration_ms`
  - `hit_points`
- optional:
  - `cast_time_ms`
  - `mana_cost`

### `passive`

- payload: forbidden
- allowed effects:
  - `nova`
- optional numeric fields:
  - `player_speed_bps`
  - `projectile_speed_bps`
  - `cooldown_bps`
  - `cast_time_bps`

## Current Status Registry, Interpreted

### `poison`

- max stacks: positive
- expire payload: optional
- dispel payload: optional
- required numeric fields:
  - `duration_ms`
  - `tick_interval_ms`
  - `magnitude`

### `hot`

- max stacks: positive
- expire payload: optional
- dispel payload: optional
- required:
  - `duration_ms`
  - `tick_interval_ms`
  - `magnitude`

### `chill`

- max stacks: positive
- expire payload: optional
- dispel payload: optional
- required:
  - `duration_ms`
  - `magnitude`
- optional:
  - `trigger_duration_ms`
- forbidden:
  - `tick_interval_ms`

### `root`

- max stacks: one
- expire payload: optional
- dispel payload: optional
- required:
  - `duration_ms`
- forced zero:
  - `magnitude`

### `haste`

- max stacks: one
- expire payload: optional
- dispel payload: optional
- required:
  - `duration_ms`
  - `magnitude`

### `silence`

- max stacks: one
- expire payload: optional
- dispel payload: optional
- required:
  - `duration_ms`
- forced zero:
  - `magnitude`

### `stun`

- max stacks: one
- expire payload: optional
- dispel payload: optional
- required:
  - `duration_ms`
- forced zero:
  - `magnitude`

### `sleep`

- max stacks: one
- expire payload: optional
- dispel payload: optional
- required:
  - `duration_ms`
- forced zero:
  - `magnitude`

### `shield`

- max stacks: positive
- expire payload: optional
- dispel payload: optional
- required:
  - `duration_ms`
  - `magnitude`

### `stealth`

- max stacks: one
- expire payload: optional
- dispel payload: optional
- required:
  - `duration_ms`
- forced zero:
  - `magnitude`

### `reveal`

- max stacks: one
- expire payload: optional
- dispel payload: optional
- required:
  - `duration_ms`
- forced zero:
  - `magnitude`

### `fear`

- max stacks: one
- expire payload: optional
- dispel payload: optional
- required:
  - `duration_ms`
- forced zero:
  - `magnitude`

## Important Runtime Cross-Checks

The registry is not the whole story. Runtime code adds several important semantics.

### Toggleable Aura Restrictions

Even though the aura schema allows:

- `distance`
- `hit_points`

the parser rejects both when `toggleable: true`.

### Ward Persistence

The registry marks `ward.duration_ms` as `non_negative`. Runtime interprets:

- `duration_ms: 0` as a persistent ward that lasts until killed

### Crowd-Control DR

The registry does not describe diminishing returns, but runtime applies them to:

- hard CC:
  - `stun`
  - `sleep`
  - `fear`
- movement CC:
  - `root`
- cast CC:
  - `silence`

Scaling inside the DR window:

- first application: `100%`
- second: `50%`
- third: `25%`
- fourth and later: immune

### Stealth Behavior

The registry says stealth exists, but runtime defines the exact semantics:

- stealthed units are not enemy-targetable
- `reveal` overrides targeting restriction without removing stealth
- stealth breaks on action
- stealth breaks on damage
- pure stealth aura payloads hide the visible aura pulse

### Dispel Categories

The registry exposes `scope`, but runtime defines the category map:

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

## Practical Use

Use this file when you want to answer:

- which fields are legal for a given behavior or status kind
- whether a payload hook is even allowed
- whether a numeric field is required, optional, zero-only, or forbidden
- which effect visuals are legal for a behavior

Use `content/skills/README.md` when you want concrete examples of how those rules are authored in class files.
