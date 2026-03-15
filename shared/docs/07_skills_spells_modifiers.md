# Skills, Spells, Modifiers (Data-Driven)

## Content goals
- Designers (you) can add/adjust abilities by editing data files.
- Authored content uses YAML in v1.
- Server loads content on boot, validates, then treats it as read-only.
- Client loads the same content for UI text only (never authoritative).
- The current runtime source of truth lives under `server/content/skills/*.yaml`.
- Implemented and planned mechanic families now live under `server/content/mechanics/registry.yaml`.

## Skill tree structure
Tree: Rogue | Mage | Cleric
Tier: 1..5

Typical tier 1 responsibilities:
- weapon/stance assignment
- base movement profile changes (dash, sprint speed, etc.)

Tier >1:
- passives (poison on hit, chill on spell hit, etc.)
- new abilities or upgrades to existing ones

## Modifier system (extensible)
Represent everything that “adjusts numbers” as modifiers:
- Movement modifiers (speed, acceleration, turn rate, dash distance)
- Damage modifiers (additive, multiplicative, crit rules)
- Cast-time modifiers (cast duration multiplier, channel tick rate, interrupt resistance)

Implementation guideline:
- Use a unified "Stat" layer with a stacking policy:
  - base value
  - additive bonuses
  - multiplicative bonuses
  - clamps (min/max)
- Each modifier includes:
  - scope: self | allies | enemies | aura radius
  - duration: instant, timed, while-equipped, while-channeling
  - stacking: refresh, stack_count, unique, strongest-wins

## Friendly fire and hit resolution
- There is full friendly fire in v1.
- Effects apply based on what the spell or attack actually hits, not on ally/enemy target filtering.
- A heal that hits an enemy still heals that enemy.
- A damage effect that hits an ally still damages that ally.
- Team affiliation matters for scoring and lobby structure, not for effect immunity.

## Status ownership and stacking
- Buffs and debuffs are tracked per source player.
- A target may carry multiple instances of the same family if they came from different players.
- Poison stacks up to 5 per source player.
- Chill stacks up to 3 per source player; at 3 stacks it applies Root.
- HoTs are tracked per caster and per spell source. Reapplying the same HoT from the same caster refreshes that source's timer.

## Effect model
Keep effects composable:
- DealDamage(amount, type)
- Heal(amount)
- ApplyStatus(status_id, duration)
- RemoveStatus(status_id)
- SpawnProjectile(projectile_id)
- SpawnVisionDevice(device_id)
- InterruptCast(target_rules)
- ModifyStat(stat, op, value, duration)

An ability is just:
- cast model
- delivery model
- list of effects triggered on cast / on tick / on hit

## Current extension path
- Adding a class that only uses already-implemented runtime mechanics should now be mostly a content change: add one YAML file under `server/content/skills/`.
- The protocol and Godot picker now use backend-authored class names instead of a fixed four-class wire enum.
- If a new class needs a brand-new mechanic family, declare that family in `server/content/mechanics/registry.yaml` and then add the runtime execution path in the focused mechanic-specific locations in `game_sim`.

## Poison / Chill examples
Poison-on-hit:
- On weapon hit event -> ApplyStatus(Poison, 3s)
- Poison status ticks DealDamage every second

Chill-on-spell-hit:
- On spell hit event -> ApplyStatus(Chill, 2s)
- Chill status applies MovementSpeed multiplier (e.g., 0.7x)
- At 3 stacks from the same source, Chill additionally applies Root.
