# state

This directory contains Godot state-management helpers that keep the shell's local view in sync with authoritative backend events.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `client_state.gd`: Client-side state container that tracks lobby, match, and arena data mirrored from the backend.
- `client_state.gd.uid`: Godot UID sidecar for `client_state`. It preserves a stable resource identifier for the neighboring script.
