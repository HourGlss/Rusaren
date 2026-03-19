# session_ingress_sequence

This directory contains checked-in seed inputs for the `session_ingress_sequence` fuzz target.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `Hash-named files`: Each SHA-like file is a minimized fuzz seed kept exactly as emitted by the fuzzing toolchain so replay coverage stays stable for `session_ingress_sequence`.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `connect_valid.bin`: Handwritten `connect valid` fixture for `session_ingress_sequence`.
- `create_valid.bin`: Handwritten `create valid` fixture for `session_ingress_sequence`.
- `oversized_create.bin`: Handwritten `oversized create` fixture for `session_ingress_sequence`.
- `prefixed_bind_then_ready.bin`: Handwritten `prefixed bind then ready` fixture for `session_ingress_sequence`.
- `reconnect_valid.bin`: Handwritten `reconnect valid` fixture for `session_ingress_sequence`.
- `select_team_valid.bin`: Handwritten `select team valid` fixture for `session_ingress_sequence`.
- `truncated_connect.bin`: Handwritten `truncated connect` fixture for `session_ingress_sequence`.
- `wrong_packet_kind.bin`: Handwritten `wrong packet kind` fixture for `session_ingress_sequence`.
