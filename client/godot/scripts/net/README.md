# net

This directory contains Godot networking helpers for websocket signaling, binary protocol handling, and runtime connection config.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `dev_socket_client.gd`: Legacy websocket-only client helper kept for dev transport paths and tests.
- `dev_socket_client.gd.uid`: Godot UID sidecar for `dev_socket_client`. It preserves a stable resource identifier for the neighboring script.
- `protocol.gd`: Binary protocol encoder and decoder helpers used by the Godot client.
- `protocol.gd.uid`: Godot UID sidecar for `protocol`. It preserves a stable resource identifier for the neighboring script.
- `websocket_config.gd`: Runtime websocket and bootstrap configuration helpers for the Godot shell.
- `websocket_config.gd.uid`: Godot UID sidecar for `websocket_config`. It preserves a stable resource identifier for the neighboring script.
