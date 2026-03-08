# Simulation Loop (Server)

## Tick model
- Fixed tick rate 60 Hz
- Each tick:
  1) ingest inputs for each player (for this tick number)
  2) update movement intent -> velocity -> position
  3) resolve collision (world + entities)
  4) update casting/channeling state machines
  5) apply effects (damage/heal/status) scheduled for this tick
  6) update cooldowns/resources
  7) emit events for net replication

## “Still” definition
Server decides what “still” means.
- Any non-zero movement input immediately breaks “still”.
- V1 uses an immediate-stop movement model: if a player stops issuing movement input, the server stops that player's controlled movement on that tick.
- A player counts as still when movement input is zero and no forced-movement effect is currently moving them.

## Casting state machines
Instant:
- Executes immediately, does NOT require stillness.

CastTime:
- Enters Casting(duration)
- Fails if:
  - player moves (break still)
  - enemy interrupts (effect triggers Interrupt flag)
- Succeeds when timer reaches 0 and still uninterrupted.

Channel:
- Enters Channeling(max_duration)
- Every tick (or every channel_period ticks) applies an action
- Ends when:
  - player moves
  - interrupted
  - max_duration reached
  - player cancels (if allowed)

## Event-first, not RPC-first
The sim emits domain events:
- DamageApplied, HealApplied
- StatusApplied/Removed
- CastStarted/Interrupted/Completed
- ProjectileSpawned/Hit
- RoundStateChanged, MatchEnded

Networking replicates state + events. Domain never “sends packets”.
