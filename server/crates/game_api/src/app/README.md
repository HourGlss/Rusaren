# app

This directory contains ServerApp support modules for lifecycle flow, ingress handling, snapshots, and tests.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `ingress/`: command and input handling for lobby and match-flow state changes.
- `snapshots/`: snapshot construction, arena projection, and visibility helpers used by the app layer.
- `tests/`: focused app-layer test slices split by scenario family.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `lifecycle.rs`: ServerApp lifecycle helpers for ticking, cleanup, and match progression.
- `support.rs`: Shared helper functions and internal glue for the neighboring application modules.
- `tests.rs`: Tests for the modules in this folder.
