# game_net

This directory contains seed builders and helper routines shared by the game_net-focused fuzz targets.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `common.rs`: Shared fuzz-support helpers used across multiple corpus builders or round-trip targets.
- `events.rs`: Fuzz-support helpers for building or mutating server-event fixtures.
- `ingress.rs`: Ingress validation and first-packet/session guard logic.
- `mod.rs`: Module entrypoint that ties together the sibling files in this folder.
