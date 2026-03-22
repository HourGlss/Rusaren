# Rusaren TODO

Current target release line: `1.0.0`
Current repo version: `0.8.0`
Current roadmap position: `0.9.0 Beta Hardening`

Completed:
- `0.3.0 Quality Foundation`
- `0.4.0 Backend MVP`
- `0.5.0 Godot Web MVP`
- `0.6.0 Hosting and Ops MVP`
- `0.7.0 Final Transport and Replication`
- `0.8.0 Content and Gameplay Hardening`

## 0.7.0 Final Transport and Replication

- [x] Add the real WebRTC gameplay transport beside the websocket dev adapter.
- [x] Make browser gameplay use the intended WebRTC path reliably in real match flow.
- [x] Implement authoritative gameplay snapshot and delta replication.
- [x] Add hostile-input fuzzing for snapshot and delta decoders.
- [x] Add malformed-packet and transport regression suites for signaling, input, snapshot, and control traffic.
- [x] Expose enough authoritative runtime state for clients and API consumers to understand player status, cooldowns, hp, mana, active statuses, and match phase.

Release gate:
- browser gameplay uses WebRTC reliably for real match traffic
- snapshot and delta decode paths are covered by ingress fuzzing and regression tests
- player runtime state needed by the client is exposed through the protocol and API

## 0.8.0 Content and Gameplay Hardening

- [x] Load authored skills and maps from runtime content files.
- [x] Harden YAML skill validation and ASCII map validation so invalid content fails cleanly at boot.
- [x] Expand the v1 authored class and spell set so every intended spell exists and functions.
- [x] Ensure all authored spells work in backend simulation, including melee, projectile, AoE, buffs, debuffs, HoTs, DoTs, silence, slow, stun, and cooldown behavior.
- [x] Add backend gameplay tests for all authored spells, including hit, miss, range, duration, stack, refresh, removal, and cooldown edge cases.
- [x] Add content fuzzing and schema-style validation around authored YAML and ASCII inputs.
- [x] Add replay and regression tests for gameplay transitions.
- [x] Keep vision and fog-of-war intentionally minimal unless they block playability.
- [x] Add Criterion benchmarks for hot-path sim and net code.

Release gate:
- authored content loads from files and fails cleanly on invalid input
- every shipped spell has direct backend tests
- status application and removal are validated in backend tests
- gameplay is functionally complete even if still visually unpolished

## 0.9.0 Beta Hardening

- [ ] Freeze the 1.0 protocol surface.
- [x] Raise coverage expectations in core crates.
- [x] Run scheduled live fuzzing in CI and retain discovered corpus and artifacts.
- [x] Add mutation testing on lobby, match, domain, and core gameplay rule logic.
- [x] Add load and soak testing.
- [x] Define backend performance budgets for tick latency, command latency, CPU, memory, SQLite log write latency, and connection capacity.
- [ ] Add repeatable load scenarios and quality gates that enforce those performance budgets on a fixed reference environment.
- [ ] Add append-only SQLite-backed match and combat logs for all non-movement player actions, picks, casts, hits, misses, healing, status changes, deaths, and match lifecycle events.
- [ ] Add replay-style regression checks that validate selected end-to-end match flows from server-authored match and combat logs.
- [ ] Add a private authenticated admin dashboard for health, tick timing, sessions, lobbies, matches, recent errors, and recent match/combat log views.
- [x] Add post-deploy smoke checks and synthetic probes for the hosted backend path.
- [x] Add ADRs for protocol freeze, event logging, admin surface, and persistence, plus explicit crate-boundary rules and a human PR review checklist.
- [ ] Close remaining "cannot test this yet" items in the report.
- [ ] Make GitHub Actions, GitHub Pages, docs, rustdoc, and report publishing stable and routine.
- [ ] Verify the hosted stack against the real domain path with TLS, TURN/STUN, and the web client.

Release gate:
- no major architecture gaps remain
- all network ingress paths are fuzzed
- performance budgets are defined and passing on the reference environment
- non-movement gameplay actions are durably logged with server-authored match and combat events
- a private admin dashboard and hosted smoke probes exist for the backend
- CI and Pages are stable on main
- hosted staging or production path has been exercised end to end

## 1.0.0 Release

- [ ] Hosted backend and hosted Godot web client are stable on the production domain.
- [ ] The Godot web client is playable with basic graphics and UI:
- [ ] players rendered as simple shapes with collision
- [ ] spells produce visible graphics and effects
- [ ] a basic HUD exists
- [ ] health and mana are shown over each player
- [ ] rounds can be fully played in browser without sprites or final polish
- [ ] All shipped spells work in the real game loop.
- [ ] Player status is available through the protocol and API, including hp, mana, cooldowns, active statuses, and match state.
- [ ] A player can play the game through the API, not only through Godot.
- [ ] Public docs site is current.
- [ ] Rust API docs are published and document how to connect, signal, send input, receive state, interpret status, and play through the API.
- [ ] Fuzzing is active in CI, not just configured.
- [ ] Deployment and operational steps are documented well enough for another engineer to run the project.

Release gate:
- a player can complete a full match in the hosted Godot web client
- all shipped spells are backend-tested and work in live play
- no known crash-on-malformed-input bugs
- no unverified network ingress path
- no undocumented deployment-critical step
- rustdoc is sufficient for an external client or bot author to play through the API
