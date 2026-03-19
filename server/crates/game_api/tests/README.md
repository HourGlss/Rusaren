# tests

This directory contains integration and corpus-replay tests for the API crate.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `realtime_webrtc/`: support code for WebRTC integration scenarios.
- `realtime_websocket/`: split integration scenarios for websocket-only hosted behavior.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `observability_fuzz_corpus.rs`: Corpus replay tests for observability-related fuzz inputs.
- `realtime_webrtc.rs`: Rust source file for realtime webrtc in this folder.
- `realtime_websocket.rs`: Rust source file for realtime websocket in this folder.
- `records_fuzz_corpus.rs`: Corpus replay tests for persistent record parsing inputs.
- `soak_match_flow.rs`: Longer-running integration tests that repeat lobby and match flows for leak detection.
- `webrtc_signaling_fuzz_corpus.rs`: Corpus replay tests for WebRTC signaling message parsing.
