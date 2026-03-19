# src

This directory contains source modules for domain value objects, validation, and tests.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `error.rs`: Error types and formatting helpers for this crate.
- `ids.rs`: Strongly typed identifiers and related helpers for the domain model.
- `lib.rs`: Crate facade that ties the folder's modules into the public API surface.
- `player.rs`: Player-facing domain value objects and validation helpers.
- `round.rs`: Round and progression helpers for match sequencing.
- `skill.rs`: Skill-tree and loadout progression types and helpers.
- `tests.rs`: Tests for the modules in this folder.
