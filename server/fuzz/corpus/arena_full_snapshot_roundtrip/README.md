# arena_full_snapshot_roundtrip

This directory contains checked-in seed inputs for the `arena_full_snapshot_roundtrip` fuzz target.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `Hash-named files`: Each SHA-like file is a minimized fuzz seed kept exactly as emitted by the fuzzing toolchain so replay coverage stays stable for `arena_full_snapshot_roundtrip`.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `arena_state_effect_batch.bin`: Handwritten `arena state effect batch` fixture for `arena_full_snapshot_roundtrip`.
- `arena_state_packet.bin`: Handwritten `arena state packet` fixture for `arena_full_snapshot_roundtrip`.
- `arena_state_variant_packet.bin`: Handwritten `arena state variant packet` fixture for `arena_full_snapshot_roundtrip`.
