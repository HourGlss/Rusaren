# Rusaren TODO

Current target release line: `1.0.0`
Current repo version: `0.2.0`

## 0.3.0 Quality Foundation

- [x] Add a real `server/fuzz/` workspace with `cargo-fuzz`.
- [x] Add first fuzz targets for packet-header decode, control-command decode, input-frame decode, and ingress/session sequencing.
- [x] Add `proptest` for parser and state-machine boundary testing.
- [x] Stand up `mdBook` for project docs and architecture notes.
- [x] Publish `cargo doc --workspace --no-deps` as API docs in CI.
- [x] Make reports, docs, and callgraph available as per-commit artifacts.
- [x] Make fuzz-target builds part of the pre-commit workflow for network-boundary changes.

Release gate:
- [x] `quality.ps1 fuzz` builds real targets.
- [x] docs site builds in CI.
- [x] reports and docs are generated automatically.

## 0.4.0 Backend MVP

- [ ] Finish the backend MVP around one fully playable rules slice.
- [ ] Persist player identity and `W-L-NC`.
- [ ] Add lobby discovery and full lobby snapshot events.
- [ ] Keep strict validation on every network boundary.

Release gate:
- hosted backend can drive a full match loop without fake clients.

## 0.5.0 Godot Web MVP

- [ ] Make the Godot shell a real browser-playable MVP.
- [ ] Add central lobby, game lobby, roster, skill-pick, match-state, and results flows against live backend state.
- [ ] Add web export smoke checks in CI.
- [ ] Host the static web client on the production domain.

Release gate:
- browser client can load from the hosted domain and finish a full match loop.

## 0.6.0 Hosting and Ops MVP

- [ ] Deploy the stack on the production domain.
- [ ] Add TLS, health checks, structured logs, Prometheus metrics, and restart policy.
- [ ] Host `coturn` for STUN/TURN.
- [ ] Add deployment docs and runbooks.

Release gate:
- one documented deploy path exists and the hosted stack is observable.

## 0.7.0 Final Transport and Replication

- [ ] Add the real WebRTC gameplay transport beside the websocket dev adapter.
- [ ] Implement snapshot and delta replication for gameplay.
- [ ] Add hostile-input fuzzing for snapshot and delta decoders.
- [ ] Add transport compatibility and malformed-packet regression suites.

Release gate:
- browser gameplay uses the intended transport reliably.

## 0.8.0 Content and Gameplay Hardening

- [ ] Add real content loading and validation.
- [ ] Expand the v1 skill and class set from placeholders into real authored content.
- [ ] Add content fuzzing and schema validation.
- [ ] Add replay and regression tests for gameplay transitions.
- [ ] Add Criterion benchmarks for hot-path sim and net code.

Release gate:
- authored content loads from files and fails cleanly at boot on invalid input.

## 0.9.0 Beta Hardening

- [ ] Freeze the 1.0 protocol surface.
- [ ] Raise coverage expectations in core crates.
- [ ] Run scheduled fuzzing in CI.
- [ ] Add mutation testing on lobby, match, and domain logic.
- [ ] Add load and soak testing.
- [ ] Close remaining "cannot test this yet" items in the report.

Release gate:
- no major architecture gaps remain and all network ingress paths are fuzzed.

## 1.0.0 Release

- [ ] Hosted backend and hosted Godot web client are stable on the production domain.
- [ ] Public docs site is current.
- [ ] Rust API docs are published.
- [ ] Fuzzing is active in CI, not just configured.
- [ ] Coverage and complexity reports are generated per commit.
- [ ] Deployment and operational steps are documented well enough for another engineer to run the project.

Release gate:
- no known crash-on-malformed-input bugs
- no unverified network ingress path
- no undocumented deployment-critical step
