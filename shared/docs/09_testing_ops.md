# Testing, Validation, Ops

## Content validation tests
On server boot:
- ensure every class file defines melee plus tiers 1..=5
- ensure authored skill ids stay globally unique across files
- ensure every ability uses a valid behavior shape for its kind
- ensure every status kind uses only the fields legal for that status
- ensure malformed ASCII maps fail cleanly before boot
- fuzz malformed YAML and ASCII content payloads in `game_content`

## Simulation unit tests
- every currently shipped melee and authored slot skill hits valid targets and misses invalid ones
- cooldowns and mana costs match authored content
- poison, hot, chill, root, haste, silence, and stun all apply and remove when they should
- locked-slot casts are rejected even when the packet shape is otherwise valid
- repeated movement packets in one frame do not move a player farther than one authoritative frame of travel
- illegal movement components are rejected before they can reach simulation state
- Skill progression rule enforced (tier gating)
- round transition rebuilds a clean combat world
- Round win condition correct
- disconnect after countdown causes immediate match abort and `No Contest` result for every player
- property-test cast/channel/interruption transitions in `game_domain` and `game_sim`
- soak repeated match loops and lobby churn in `game_api`

## Determinism-ish regression
Record:
- initial seed
- tick-by-tick input stream
Replay:
- ensure final match result + key state hashes match
- fuzz tick input streams around simultaneous effects, interrupts, and no-timeout combat edge cases

## Observability
- structured logs (match id, tick, player id)
- metrics: tick duration, packet rates, disconnects
- tracing for slow ticks / overload
- expose Prometheus metrics from the Rust server and scrape them from the hosted stack
- validate the deploy stack in CI with a same-image smoke test that checks `/`, `/healthz`, and `/metrics`
- run the same Docker smoke path locally with `server/scripts/docker-smoke.ps1` before host deploy changes land
- keep the hosted path on recurring timers with `deploy/host-smoke.sh` and `deploy/run_live_transport_probe.sh`
- use `/adminz?format=json` plus the combat-log diagnostics surface as the operator-facing source of truth during hosted triage
- collect coverage and complexity reports for `game_sim`, `game_net`, and `game_content`
- publish generated docs and API docs per commit so test/coverage output has architecture context beside it

## Abuse resistance
- input rate limits
- server-side sanity checks for "still" and casting
- strictly increasing packet sequence enforcement on inbound control and input channels
- strictly increasing `client_input_tick` enforcement for live combat input streams
- disconnect after launch countdown immediately ends the match
- no reconnect-to-match flow in v1
- fuzzing priority is network ingress: packet headers, control-command decode, control-command round-trip, server-control-event decode, input-frame decode, input-frame round-trip, ingress/session sequencing, full snapshot decode, delta snapshot decode, and WebRTC signaling parsing/round-trip
- mutation testing should focus on packet validation, lobby/match/domain rules, and core gameplay logic where a bad mutation could allow malformed lengths, stale ordering, locked-skill casts, or impossible movement
- a focused local ingress mutation smoke can be run with `rustup run stable cargo mutants --no-config --package game_net --test-package game_net --file crates/game_net/src/ingress.rs --copy-target false --test-tool nextest --baseline skip --jobs 1 --timeout 60 --build-timeout 60 --output target/reports/mutants-ingress`
- keep non-network fuzzing supplemental rather than release-gating

## Coverage gates
- `./scripts/quality.ps1 coverage-gate` enforces minimum line/function coverage on the core runtime crates
- the current gate checks `game_api`, `game_domain`, `game_lobby`, `game_match`, `game_net`, and `game_sim`

## Fixed-reference performance gates
- `./scripts/quality.ps1 soak` now includes the fixed-reference `performance_budget_gates` suite in `game_api`
- that gate covers:
  - `100` idle sessions
  - `10` active matches
  - command latency budgets
  - tick latency budgets
  - Linux RSS budget
  - SQLite combat-log append and query budgets
- the gate is intended for repeatable regressions on Linux CI or Linux reference machines, while hosted-path validation stays in the deploy smoke and liveprobe flow
