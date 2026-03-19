# tests

This directory contains integration and replay tests for packet codecs and fuzz corpora.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `control_packets.rs`: Integration tests for control-packet encoding and decoding.
- `fuzz_corpus_replay.rs`: Replay tests that exercise checked-in fuzz corpus inputs against the crate surface.
- `packet_core.rs`: Integration tests for packet headers, input frames, and related core codec behavior.
