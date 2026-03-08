# Rusaren TODO

Current target release line: `1.0.0`
Current repo version: `0.5.0`
Current roadmap position: `0.6.0 Hosting and Ops MVP`

Completed:
- `0.3.0 Quality Foundation`
- `0.4.0 Backend MVP`
- `0.5.0 Godot Web MVP`

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
- [ ] Deployment and operational steps are documented well enough for another engineer to run the project.

Release gate:
- no known crash-on-malformed-input bugs
- no unverified network ingress path
- no undocumented deployment-critical step
