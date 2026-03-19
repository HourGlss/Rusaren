# src

This directory contains source modules for the simulation world, ticks, actions, effects, helpers, and tests.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `geometry/`: geometry-specific simulation tests.
- `tests/`: split simulation tests grouped by gameplay concern.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `actions.rs`: Simulation action resolution helpers.
- `effects.rs`: Simulation effect application and status-resolution helpers.
- `geometry.rs`: Geometry helpers used by simulation movement, projectiles, or line-of-sight logic.
- `helpers.rs`: Shared support helpers for the surrounding crate.
- `lib.rs`: Crate facade that ties the folder's modules into the public API surface.
- `tests.rs`: Tests for the modules in this folder.
- `ticks.rs`: Fixed-tick simulation update helpers.
