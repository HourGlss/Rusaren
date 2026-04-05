# Rusaren TODO

Current target release line: `1.0.0`
Current repo version: `0.8.0`
Current roadmap position: `0.9.7 Audio, Input, And Combat Polish`

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

- [x] Raise coverage expectations in core crates.
- [x] Run scheduled live fuzzing in CI and retain discovered corpus and artifacts.
- [x] Add mutation testing on lobby, match, domain, and core gameplay rule logic.
- [x] Add load and soak testing.
- [x] Define backend performance budgets for tick latency, command latency, CPU, memory, SQLite log write latency, and connection capacity.
- [x] Add repeatable load scenarios and quality gates that enforce those performance budgets on a fixed reference environment.
- [x] Implement the advanced spell semantics required by the class design: true channels, dispels, multi-source stacking periodic effects, and trigger-on-expire or trigger-on-dispel payloads.
- [x] Add append-only SQLite-backed match and combat logs for all non-movement player actions, picks, casts, hits, misses, healing, status changes, deaths, and match lifecycle events.
- [x] Add replay-style regression checks that validate selected end-to-end match flows from server-authored match and combat logs.
- [x] Add a private authenticated admin dashboard for health, tick timing, sessions, lobbies, matches, recent errors, and recent match/combat log views.
- [x] Add post-round and post-match summary screens with damage done, healing to allies and enemies, crowd control used, crowd control hits, and running totals carried forward after each round.
- [x] Add player-only scrolling combat text with a World of Warcraft style flow for outgoing and incoming combat events.
- [x] Add dynamic player coloring driven by pick order and class identity: slot 1 colors the center, slots 2 through 5 add outward rings, unpicked slots render black, the outer border is team-relative (`friendly = dark blue`, `enemy = red`), and a thin halo outside the team ring shows positive statuses on the right and negative statuses on the left with longer-duration effects higher in the stack.
- [x] Add more dynamic authored map items beyond the current static obstacle set.
- [x] Add post-deploy smoke checks and synthetic probes for the hosted backend path.
- [x] Add ADRs for protocol freeze, event logging, admin surface, and persistence, plus explicit crate-boundary rules and a human PR review checklist.
- [x] Extend backend tests, replay checks, and liveprobe scenarios to cover channel start, tick, cancel, dispel resolution, multi-source periodic stacking, and bloom-style expiration effects.
- [ ] Close remaining "cannot test this yet" items in the report.
- [ ] Make GitHub Actions, GitHub Pages, docs, rustdoc, and report publishing stable and routine.
- [x] Verify the hosted stack against the real domain path with TLS, TURN/STUN, and the web client.
- [x] Support authored maps with up to three Team A anchors and up to three Team B anchors instead of exactly one spawn anchor per side.
- [x] Support non-rectangular authored map footprints so out-of-shape cells are not treated as walkable arena space inside the rectangular ASCII bounds.

Release gate:
- no major architecture gaps remain
- all network ingress paths are fuzzed
- performance budgets are defined and passing on the reference environment
- non-movement gameplay actions are durably logged with server-authored match and combat events
- a private admin dashboard and hosted smoke probes exist for the backend
- CI and Pages are stable on main
- hosted staging or production path has been exercised end to end

Execution order for `0.9`:
- `0.9.1` mutation harness and survivor cleanup
- `0.9.2` combat semantics and advanced periodic mechanics
- `0.9.3` event spine and combat persistence
- `0.9.4` combat feedback and readability built on the event spine
- `0.9.5` maps and arena variety
- `0.9.6` ops, admin, performance gates, publication stability, and protocol freeze

### 0.9.1 Mutation Hardening And Release-Line Cleanup

- [x] Fix the mutation runner scratch-space isolation in `server/scripts/quality.ps1` so shards never share the same temp directory or cargo target directory.
- [x] Re-run the focused mutation shards that previously produced false baseline failures and confirm the harness is trustworthy again.
- [x] Kill the real mutation survivors in `crates/game_api/src/app/snapshots/visibility.rs` with stronger targeted tests and clearer visibility assertions.
- [x] Reduce the current timeout list by splitting overly broad mutation target groups, adding faster focused test commands, or explicitly documenting low-value exclusions.
- [x] Keep the mutation focus on gameplay, ingress, visibility, and protocol logic rather than spending disproportionate time on formatter-only or display-only code.
- [x] Update this roadmap to mark already-verified hosted-domain items done once the latest live-domain validation is reflected here.

Milestone gate:
- mutation shards run reliably without shared-artifact contamination
- the known visibility survivors are eliminated
- the timeout list is smaller and understood

### 0.9.2 Combat Semantics And Advanced Periodic Mechanics

- [x] Add a true `Channel` casting mode that matches [11_classes.md](/C:/Users/azbai/Documents/Rarena/shared/docs/11_classes.md): channeling requires stillness, ticks while maintained, and stops cleanly on movement, interrupt, manual cancel, stun, or silence according to the authored rules.
- [x] Change periodic status handling so `Poison`, `Chill`, and `HoT` can coexist from multiple source players on the same target instead of forcing a single shared instance for the target.
- [x] Preserve the intended same-source rules while doing that source split: same-source applications should still stack or refresh according to the authored status family instead of duplicating into runaway parallel copies.
- [x] Add a real `Dispel` mechanic with authored targeting and removal rules so spells can strip eligible positive or negative effects from a target.
- [x] Add generic trigger-on-expire and trigger-on-dispel payload support so Lifebloom-style "stacked HoT that blooms when it expires or is dispelled" is authorable in content instead of hard-coded as a one-off.
- [x] Expand the authored content schema and registry docs so channels, dispels, multi-source periodic stacks, and bloom-style triggers are first-class mechanics rather than implied future work.
- [x] Add focused backend tests for channel start, tick cadence, interrupt, movement cancel, manual cancel, multi-source `Poison`, multi-source `Chill`, multi-source `HoT`, dispel removal, and expire-or-dispel trigger payloads.
- [x] Add liveprobe scenarios for the new mechanic families so the hosted-path probe can exercise channel maintenance, dispel resolution, and periodic stack behavior instead of only instant or cast-time skills.

Milestone gate:
- channels behave according to the class design rules rather than as an undocumented aura approximation
- `Poison`, `Chill`, and `HoT` all support multi-source coexistence with correct same-source stack or refresh behavior
- dispels and expire-or-dispel trigger payloads are content-authorable and test-covered

### 0.9.3 Event Spine And Combat Persistence

- [x] Add the append-only SQLite-backed match and combat log as the server-authored source of truth for non-movement gameplay actions.
- [x] Define and persist combat events for picks, cast start, cast complete, cast cancel, hits, misses, damage, healing, status apply/remove, defeats, round transitions, and match transitions.
- [x] Include channel-specific and status-stack-specific event detail in that combat log, including channel tick, channel cancel reason, dispel cast, dispel result, source-aware status stack changes, and expire-or-dispel trigger outcomes.
- [x] Make the event model stable enough that both UI consumers and regression tooling can read it without scraping ad-hoc text logs.
- [x] Add replay-style regression checks that rebuild selected end-to-end match expectations from those server-authored logs.
- [x] Use the same event spine to support later admin views, round summaries, match summaries, and scrolling combat text instead of inventing separate one-off pipelines.

Milestone gate:
- a complete match produces a durable server-authored combat log
- at least one replay-style regression check validates a real logged match flow
- UI-facing combat consumers can read from the same event model

### 0.9.4 Combat Feedback And Readability

- [x] Add post-round and post-match summary screens with damage done, healing to allies and enemies, crowd control used, crowd control hits, and running totals that carry forward after each round.
- [x] Add player-only scrolling combat text with a World of Warcraft style flow for that player's own outgoing and incoming combat events.
- [x] Drive the combat text and summary screens from the shared combat-event spine instead of local-only client guesses.
- [x] Surface channeling, dispels, and bloom-style trigger outcomes clearly enough in the combat UI that players can tell why a periodic heal or damage effect started, stacked, bloomed, or was removed.
- [x] Replace generic skill-picker hover text with authored skill descriptions plus mechanic summaries pulled from the shared skill catalog, and use category-colored skill labels so damage, healing, control, and damage-over-time skills read differently at a glance.
- [x] Keep aura mechanics gameplay-only in the arena view: aura deployables should not render as visible props, labels, or health bars unless a later milestone introduces a deliberate aura-specific VFX treatment.
- [x] Keep the current player physics and collision size unchanged while improving visual readability only.
- [x] Add dynamic player coloring driven by pick order and class identity: slot 1 colors the center, slots 2 through 5 add outward rings, unpicked slots render black, the outer border is team-relative (`friendly = dark blue`, `enemy = red`), and a thin halo outside the team ring shows positive statuses on the right and negative statuses on the left with longer-duration effects higher in the stack.
- [x] Use WoW-style class colors for the current WoW-analogue classes and reserve the Glasbey et al. 2007 categorical palette for non-WoW classes and future class growth.
- [x] Save the reserved non-WoW palette in the docs and explicitly mark which colors are already consumed by shipped classes so future class additions do not re-choose colors ad hoc.

Milestone gate:
- combat text, round summaries, and match summaries all work from server-authored event data
- class identity, team identity, status state, and skill-pick readability are clear in crowded fights without changing player hitboxes

### 0.9.5 Maps And Arena Variety

- [x] Support authored maps with up to three Team A anchors and up to three Team B anchors instead of exactly one spawn anchor per side.
- [x] Support non-rectangular authored map footprints so out-of-shape cells are treated as out-of-bounds rather than walkable arena space inside a rectangular ASCII box.
- [x] Add more dynamic authored map items beyond the current static obstacle set.
- [x] Extend map validation, parsing tests, and simulation/runtime tests so spawn assignment, occupancy, walkability, line-of-sight, and combat interactions stay correct as the map grammar expands.
- [x] Keep authored map growth data-driven so new arena items and future map shapes do not require a parser rewrite each time.
- [x] Add a single-player training environment with resettable combat dummies that report damage, healing, DPS, and HPS over the current session.
- [x] Add two authored dummy variants with `100x` player health: one dummy resets to full health after dropping below `5%` HP, and one dummy returns to a low-health state for execute-range testing instead of dying.
- [x] Add a player-facing reset action in the training environment that clears damage, healing, and timer metrics without restarting the whole map.
- [x] Support a separate authored training map where `d` and `D` place the two dummy variants directly from the ASCII map file.

Milestone gate:
- authored maps can express the expanded spawn and footprint rules
- new map items and training-map entities are authored from data and test-covered in simulation/runtime

### 0.9.6 Operations, Admin, Performance Gates, And Publication Stability

- [x] Add repeatable load scenarios and quality gates that enforce the defined performance budgets on a fixed reference environment.
- [x] Add a private authenticated admin dashboard for health, tick timing, sessions, lobbies, matches, recent errors, and recent match/combat log views.
- [x] Keep the liveprobe and hosted smoke checks current with the new mechanic surface so channel, dispel, and periodic-stack regressions are caught on the real hosted path.
- [x] Close the remaining "cannot test this yet" report items.
- [x] Make GitHub Actions, GitHub Pages, docs, rustdoc, and report publishing stable and routine.
- [x] Keep hosted smoke probes and real-domain validation current as the release line moves, rather than treating live verification as a one-time checkbox.
- [x] Freeze the 1.0 protocol surface only after the event schema, replay checks, UI consumers, and admin/log consumers have settled.

Milestone gate:
- ops dashboards, load scenarios, and publication/reporting flows are routine rather than one-off
- no major 0.9 architecture or observability gap remains

### 0.9.7 Audio, Input, And Combat Polish
- [x] Add semi-unique sound cues for shipped spells so players can distinguish major actions without relying only on visuals.
- [x] Add `3D` movement audio, but suppress it for stealthed players and for players currently inside brush.
- [x] Add a self-cast shortcut on `x` so held self-cast targets the casting player while still respecting normal stun, silence, range, and cast-validity rules.
- [x] Implement and test crowd-control diminishing returns.
- [x] Add non-playback spell cue plumbing so backend-authored skills can declare optional `audio_cue_id` values and the Godot client can resolve them through a checked-in frontend manifest when real assets arrive.
- [x] Add a YAML-configurable chance for brush movement to make an audible sound instead of being perfectly silent every time.
- [x] Allow nearby stealthed players to be heard within a small radius without revealing them.

Milestone gate:
- shipped spells and player movement have readable audio cues
- self-cast and crowd-control diminishing returns behave according to the authored rules
- stealth and brush audio behavior is data-driven and test-covered


## 1.0.0 Release

- [x] Hosted backend and hosted Godot web client are stable on the production domain.
- [ ] Godot web client baseline playability is shippable, including:
- [x] players rendered as simple shapes with collision
- [x] spells produce visible graphics and effects
- [x] a basic HUD exists
- [ ] health and mana are shown over each player
- [x] rounds can be fully played in browser without sprites or final polish
- [ ] All shipped spells work in the real game loop, not only in backend simulation.
- [ ] Player runtime status is exposed through the protocol and API, including hp, mana, cooldowns, active statuses, and match state.
- [ ] A player can play the game through the API, not only through the Godot client.
- [ ] Public docs site reflects the shipped game and the hosted path accurately.
- [ ] Rust API docs are published and are sufficient to connect, signal, send input, receive state, interpret status, and play through the API.
- [ ] Fuzzing is active in CI as part of the real release path, not just configured locally.
- [ ] Deployment and operational steps are documented well enough for another engineer to run the project.

Release gate:
- a player can complete a full match in the hosted Godot web client
- all shipped spells are backend-tested and work in live play
- no known crash-on-malformed-input bugs
- no unverified network ingress path
- no undocumented deployment-critical step
- rustdoc is sufficient for an external client or bot author to play through the API

Execution order for `1.0`:
- `1.0.1` scope freeze and live verification
- `1.0.2` final UI and documentation pass
- `1.0.3` final quality sweep, with the full mutation campaign last

### 1.0.1 Scope Freeze And Live Verification

- [ ] Freeze scope so release work is about hardening, readability, verification, and documentation instead of introducing new mechanic families.
- [ ] Verify full browser matches on the hosted production domain using the real web client and real players over the internet.
- [ ] Verify that every shipped spell works in the real game loop, not only in backend simulation.
- [ ] Eliminate known live blockers in hit registration, cast registration, disconnect handling, and transport stability.

Milestone gate:
- the release candidate is feature-frozen
- the hosted path has been exercised by real players through complete matches

### 1.0.2 Final UI And Documentation Pass

- [ ] Finalize the HUD and readability pass so health, mana, cast state, combat feedback, and player identity remain readable in crowded fights.
- [ ] Polish end-round and end-game summaries and scrolling combat text until they are genuinely useful during live play, not just technically present.
- [ ] Make deployment and operational steps clear enough for another engineer to bring up and maintain the project.
- [ ] Make the Rust API docs and gameplay/API guidance sufficient for an external client or bot author to connect, signal, send input, receive state, and play through the API.
- [ ] Keep runbooks, deployment docs, and public docs synchronized with the actual hosted and release paths.

Milestone gate:
- player-facing UI is readable enough for release
- another engineer could operate or integrate with the project from the docs

### 1.0.3 Final Quality Sweep

- [ ] Run the full normal test suite, replay regressions, fuzzing, and load/performance gates against the feature-frozen release candidate.
- [ ] Confirm there are no known crash-on-malformed-input bugs and no unverified ingress path left in the release surface.
- [ ] Run the full mutation campaign only after the release candidate is otherwise stable, and treat that long mutation run as the final quality gate rather than a repeated mid-feature loop.
- [ ] Fix or explicitly justify any surviving high-value mutants revealed by that final campaign before tagging `1.0.0`.

Milestone gate:
- all normal release checks pass on the frozen candidate
- the full mutation campaign is the last hardening pass before release tagging

### Roadmap Hygiene

- [ ] Audit the original `0.9.0 Beta Hardening` checklist line by line against the implemented code, hosted-path validation, and current automated tests; mark each line item done only after it is explicitly confirmed.
