# arena_delta_snapshot_roundtrip

This directory contains checked-in seed inputs for the `arena_delta_snapshot_roundtrip` fuzz target.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `Hash-named files`: Each SHA-like file is a minimized fuzz seed kept exactly as emitted by the fuzzing toolchain so replay coverage stays stable for `arena_delta_snapshot_roundtrip`.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `arena_delta_packet.bin`: Handwritten `arena delta packet` fixture for `arena_delta_snapshot_roundtrip`.
- `arena_delta_variant_packet.bin`: Handwritten `arena delta variant packet` fixture for `arena_delta_snapshot_roundtrip`.
- `lobby_snapshot_packet.bin`: Handwritten `lobby snapshot packet` fixture for `arena_delta_snapshot_roundtrip`.
