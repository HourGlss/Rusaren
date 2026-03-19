# server_control_event_roundtrip

This directory contains checked-in seed inputs for the `server_control_event_roundtrip` fuzz target.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `Hash-named files`: Each SHA-like file is a minimized fuzz seed kept exactly as emitted by the fuzzing toolchain so replay coverage stays stable for `server_control_event_roundtrip`.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `arena_delta_packet.bin`: Handwritten `arena delta packet` fixture for `server_control_event_roundtrip`.
- `arena_effect_batch_packet.bin`: Handwritten `arena effect batch packet` fixture for `server_control_event_roundtrip`.
- `arena_state_packet.bin`: Handwritten `arena state packet` fixture for `server_control_event_roundtrip`.
- `connected_packet.bin`: Handwritten `connected packet` fixture for `server_control_event_roundtrip`.
- `lobby_snapshot_packet.bin`: Handwritten `lobby snapshot packet` fixture for `server_control_event_roundtrip`.
