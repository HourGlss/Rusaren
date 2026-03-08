# Testing, Validation, Ops

## Content validation tests
On server boot:
- ensure every skill node references valid prerequisites
- ensure no circular prereqs
- ensure every ability references valid effects/status/projectiles
- ensure every stat/modifier uses known enum keys
- fuzz malformed content payloads and graph edge cases in `game_content`

## Simulation unit tests
- CastTime cancels on movement
- Channel ticks apply on schedule and stop on movement
- Interrupt events cancel casting
- Round win condition correct
- Skill progression rule enforced (tier gating)
- disconnect after countdown causes immediate match abort and `No Contest` result for every player
- property-test cast/channel/interruption transitions in `game_domain` and `game_sim`

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
- collect coverage and complexity reports for `game_sim`, `game_net`, and `game_content`
- publish generated docs and API docs per commit so test/coverage output has architecture context beside it

## Abuse resistance
- input rate limits
- server-side sanity checks for "still" and casting
- disconnect after launch countdown immediately ends the match
- no reconnect-to-match flow in v1
- fuzz protocol decode and invalid client command sequences in `game_net`
- fuzz the low-cardinality HTTP route classifier that feeds the observability layer
- fuzz the Prometheus observability renderer and counter/gauge update paths that back `/metrics`
- initial fuzz targets should stay live for packet headers, control-command decode, server-control-event decode, input-frame decode, ingress/session sequencing, and the observability surface
