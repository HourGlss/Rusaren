# godot

This directory contains the Godot 4 browser shell that talks to the Rust backend over websocket signaling and WebRTC.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `.godot/`: local Godot editor metadata generated on developer machines. It is intentionally ignored from git.
- `scenes/`: Godot scene files that define the shell's node layout.
- `scripts/`: top-level Godot scripts that coordinate UI flow, networking, and state transitions.
- `tests/`: headless Godot checks for protocol behavior, shell layout, exports, and transport assumptions.
- `webrtc/`: local sync target for the optional native WebRTC extension bundle. It is intentionally ignored from git.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `export_presets.cfg`: Godot export preset definitions used by the web export script.
- `project.godot`: Godot project manifest for the browser shell.
