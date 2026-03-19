# arena

This directory contains Godot scripts for rendering the arena, players, projectiles, and combat-state visuals.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `arena_view.gd`: Arena rendering script that projects authoritative simulation state into the Godot scene.
- `arena_view.gd.uid`: Godot UID sidecar for `arena_view`. It preserves a stable resource identifier for the neighboring script.
