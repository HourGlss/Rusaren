# Domain Model

## Core types
- PlayerId, MatchId, LobbyId, TeamId
- EntityId (server-only)
- Vec2/Vec3, TimeMs, Tick

## Player (domain concept)
- InputState: movement intent, facing/aim, cast button, optional delivery/aim context
- LoadoutState: per-tree progress + active skills chosen this match
- StatusState: health, mana/energy, buffs/debuffs, cooldowns
- PlayerRecord: wins, losses, no_contests

## Match (domain concept)
- Teams: roster, ready state, side selection
- Score: rounds_won[team]
- RoundState machine (see match_flow)
- MatchOutcome: team_a_win | team_b_win | no_contest

## Skills and Trees
A “tree” is a themed progression track with 5 tiers.

Per-match rule recommendation (fits 5 rounds cleanly):
- Each player tracks progress per tree: tier_unlocked[tree] in {0..5}
- At the start of each round, the player chooses ONE of:
  - Advance an existing tree by 1 tier (if < 5)
  - Start a new tree at tier 1
- Players may switch trees between rounds (no requirement to stay in one tree)

Skill nodes are content-driven with:
- prerequisites (default: previous tier in same tree)
- granted weapon/stance changes (often tier 1)
- granted abilities (spells)
- passive modifiers (movement/damage/cast-time adjustments)

## Abilities (spells)
An ability is defined by:
- cast model: Instant | CastTime | Channel
- targeting/delivery: self, ground point, projectile, cone, aura, zone, etc.
- team affiliation does not protect targets from effects in v1; all effects follow hit resolution and full friendly fire is enabled
- effects: damage/heal, apply status, spawn projectile, dispel, etc.
- constraints: requires_still, range, LOS rules, cooldown, resource cost
