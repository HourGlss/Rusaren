# Principles

## Server authoritative, client untrusted
- The server is the only source of truth.
- The client sends inputs (move intent, cast intent, aim/delivery intent).
- The server validates, simulates, and broadcasts results.

## Deterministic-ish simulation
Goal: the same inputs produce the same results for a given server build.
- Fixed tick rate (e.g., 30 or 60 Hz).
- Pure functions where possible; isolate RNG behind a single seeded component.
- Avoid frame-time dependence.

## Modular boundaries (hexagonal-ish)
- Domain: rules, entities, abilities, modifiers, state machines.
- Application: orchestrates lobbies, matches, persistence, matchmaking, telemetry.
- Infrastructure: networking, serialization, DB, logging, metrics.

No networking types inside the domain. No domain logic in the networking layer.

## Extensibility targets
- Add a new spell without touching the simulation loop.
- Add a new modifier type without changing every spell.
- Add a new game mode (e.g., 1v1, king-of-the-hill) without rewriting lobby + net.
