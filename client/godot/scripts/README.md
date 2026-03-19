# scripts

This directory contains top-level Godot scripts that coordinate UI flow, networking, and state transitions.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `arena/`: Godot scripts for rendering the arena, players, projectiles, and combat-state visuals.
- `net/`: Godot networking helpers for websocket signaling, binary protocol handling, and runtime connection config.
- `state/`: Godot state-management helpers that keep the shell's local view in sync with authoritative backend events.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `main.gd`: Top-level Godot controller script that wires UI flow, networking, and authoritative state updates together.
- `main.gd.uid`: Godot UID sidecar for `main`. It preserves a stable resource identifier for the neighboring script.
