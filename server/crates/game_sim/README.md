# game_sim

This directory contains the simulation crate that owns authoritative combat, movement, cooldowns, statuses, and effect resolution.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `benches/`: Criterion benchmarks for simulation hot paths.
- `src/`: source modules for the simulation world, ticks, actions, effects, helpers, and tests.
- `Cargo.toml`: Cargo manifest that declares this package's metadata, dependencies, and targets.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
