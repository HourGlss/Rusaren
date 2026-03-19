# game_net

This directory contains the wire-format crate for packet headers, control messages, ingress validation, and codec benchmarks.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `benches/`: Criterion benchmarks for packet and snapshot codec hot paths.
- `src/`: source modules for packet headers, control packets, input frames, errors, and ingress rules.
- `tests/`: integration and replay tests for packet codecs and fuzz corpora.
- `Cargo.toml`: Cargo manifest that declares this package's metadata, dependencies, and targets.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
