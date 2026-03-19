# snapshots

This directory contains snapshot construction, arena projection, and visibility helpers used by the app layer.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `arena.rs`: Arena snapshot-building helpers that project simulation state into transport structs.
- `mod.rs`: Module entrypoint that ties together the sibling files in this folder.
- `visibility.rs`: Visibility and fog-of-war helpers used when building player-specific snapshots.
