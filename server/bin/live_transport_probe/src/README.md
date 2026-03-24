# src

This directory contains the implementation of the live transport probe binary.
The modules are split so the CLI, transport client, planning, orchestration, and tests stay focused and reviewable.

## Structure
- `README.md`: this guide documents the folder structure and the purpose of each source file.
- `cli.rs`: parses probe command-line flags into runtime configuration.
- `client.rs`: real bootstrap, websocket signaling, and WebRTC data-channel client code used by the probe.
- `event_log.rs`: structured JSONL logging for probe runs and failures.
- `lib.rs`: crate root that wires modules together and exposes the probe API for tests.
- `main.rs`: small executable entrypoint that parses flags and runs the probe.
- `planner.rs`: skill-catalog planning logic that groups authored trees into 2v2 match batches.
- `probe.rs`: orchestration logic that connects four clients, runs matches, and drives combat.
- `tests.rs`: unit and local end-to-end tests for the probe.
