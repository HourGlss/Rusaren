# live_transport_probe

This directory contains the standalone binary crate that stress-checks the live transport path with four headless WebRTC clients.
It runs against a real backend origin, plays full 2v2 matches, rotates through the skill catalog, and writes durable probe logs for later diagnosis.

## Structure
- `Cargo.toml`: crate manifest for the live transport probe executable and its runtime dependencies.
- `README.md`: this guide explains the folder structure and what each checked-in file is for.
- `src/`: Rust source for the probe CLI, real WebRTC client, match planner, orchestration, and tests.
