# Maps + Vision System (Server Authoritative)

Goal:
- Vision works like StarCraft 2 fog-of-war: you see what your unit can currently see.
- Fog is per-player. Allies do NOT share vision.
- Client never decides what is visible; it only renders what the server says.

## Concepts

Per player we track:
- Explored: tiles the player has seen at least once this match
- Visible: tiles currently visible this tick
- Revealed entities: entities currently visible (or revealed via special effects)

Important: "team" is for scoring only. Vision is individual.

## Vision primitives (map authored)

Maps provide server-side geometry primitives in "map content":

1) Movement blockers
- Solid walls / rocks / pillars
- Block movement and LoS

2) Vision blockers (LoS blockers that do not necessarily block movement)
- Used for "SC2 bush/tall-grass style" zones and smoke walls
- Block vision through them
- Can be configured to also block vision into them

3) Stealth zones (the bush behavior)
A StealthZone is a region with these default rules:
- If an entity is inside the zone, it is hidden from observers outside the zone
- Observers inside the same zone can see entities inside the zone
- Observers outside cannot see into the zone unless they have a Reveal effect
- Vision does not pass through the zone (can't see "through the bush")

Optional config knobs per zone:
- hide_projectiles: true/false (default false)
- block_vision_into: true/false (default true)
- block_vision_through: true/false (default true)

4) Reveal sources
- Map-owned watchtowers and shrines are omitted in v1.
- Reveal sources may instead come from class abilities or spawned vision devices.
- These reveal ONLY to the player who owns/created them (still no ally sharing)

## Line-of-sight + field-of-view

Baseline:
- Each player has a vision radius (meters) and optionally a facing-based cone (later)
- LoS is computed against LoS blockers and StealthZones

Recommended implementation v1:
- Grid-based visibility with 1.0m tiles
- For each tick, compute Visible via:
  - raycasting to boundary points

Server outputs:
- visible_tiles bitset (compressed)
- explored_tiles bitset (rarely changes; send on join + deltas)
- visible_entities list/delta

## Entity visibility rules

Entity is visible to player if any of these are true:
1) Entity position is in player's Visible tiles
2) Entity is affected by a Reveal status owned by that player
3) Entity has recently caused damaging impact and is therefore briefly revealed

StealthZone override:
- If entity is inside StealthZone and observer is outside:
  - entity is NOT visible unless rule (2) applies

Dead players:
- Suggested: dead players may spectate freely (client-side), but spectating must not leak info
- Hard rule: a living player’s visibility does not change based on dead allies

## Map fairness + validation

Validation checks on server boot:
- Spawn groups are symmetric/fair (distance-to-mid within tolerance)
- No spawn point is inside LoS-blocking geometry
- StealthZones aren't overlapping spawns
- No unreachable regions if movement has no jump/teleport (optional)

## Data model hooks

Map content should define:
- bounds
- spawn groups (per team slot)
- geometry (movement blockers)
- LoS blockers
- StealthZones
- named regions (for debugging/telemetry)
- no mandatory reveal interactables in v1

See docs/maps/_template.md

## Current playable prototype map

The current first playable slice uses a deliberately simple authored layout:
- mostly empty arena floor
- four square central pillars
- each pillar is wrapped by a thin square shrub collar
- open lanes around the perimeter for early movement/combat testing

Current purpose:
- validate server-authoritative movement and collision
- validate mouse aim + simple skill/melee effects in the browser shell
- provide a predictable layout for early fuzzing, tests, and packet debugging
