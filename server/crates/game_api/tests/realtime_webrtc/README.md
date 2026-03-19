# realtime_webrtc

This directory contains support code for WebRTC integration scenarios.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `client.rs`: Client-side packet helpers for this module, usually focused on client-originated control messages.
- `session.rs`: Session-scoped WebRTC test helpers for the neighboring integration suite.
- `support.rs`: Shared helper functions and internal glue for the neighboring application modules.
