# Classes / Skill Trees

A "class" is a 5-tier skill tree.
Players pick exactly ONE skill node per round.
Match is best-of-5 rounds, so a player makes 5 picks total.

Current authored runtime trees:
- Warrior
- Rogue
- Mage
- Cleric
- Paladin
- Ranger
- Bard
- Druid
- Necromancer

## Progression rule (per player)
Each tree tracks an independent tier level in {0..5}.

At the start of each round, a player may pick:
- Tier 1 of any tree with tier=0 (start that tree), OR
- Tier (n+1) of a tree already started at tier n (advance it)

Client UX rule:
- The client should only enable these currently legal picks; invalid future tiers may still be attempted by a hostile client, so the server must continue to reject them authoritatively.

Example valid sequence:
- R1: Rogue 1
- R2: Cleric 1
- R3: Cleric 2
- R4: Rogue 2
- R5: Mage 1

## Tier design guidelines
- Tier 1: defines stance/weapon + baseline movement profile + baseline melee
- Tier 2: core identity (the “this is what this class IS” mechanic)
- Tier 3: very powerful passive effect (build-defining)
- Tier 4: strong upgrade / utility / counterplay tool
- Tier 5: ultimate (very powerful, round-swinging, but fair via cast/channel/telegraph)

## Universal constraints
- Every class must have at least one melee ability available at Tier 1.
- Some classes scale melee harder (Rogue, Warrior), some scale spells harder (Mage, Cleric).
- Full friendly fire is enabled. Abilities affect whatever player they actually hit.
- All abilities obey the casting rules:
  - Instant: can be used while moving
  - CastTime: requires stillness; cancels on movement; can be interrupted
  - Channel: requires stillness; ticks; stops on movement/interrupt/cancel

## Runtime status families (current 0.8 slice)
- Poison: DoT (damage over time), stacks up to 5 per source player
- Chill: slow (movement modifier), stacks to 3 per source player and then roots
- HoT: heal over time, refreshed per caster and spell source
- Haste: movement-speed buff with authored duration and magnitude
- Silence: blocks skill casts, but still allows melee and movement
- Root: prevents movement, but still allows melee and casting
- Stun: prevents movement and both melee/skill actions for its authored duration

Not a current runtime focus:
- Reveal and Stealth remain future mechanics
- Interrupt remains future explicit content; the current control slice uses Silence and Stun instead
