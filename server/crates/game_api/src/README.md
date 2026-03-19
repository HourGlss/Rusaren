# src

This directory contains source modules for the game_api crate.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `app/`: ServerApp support modules for lifecycle flow, ingress handling, snapshots, and tests.
- `observability/`: unit tests and helpers around the observability surface.
- `realtime/`: HTTP, websocket, session, and signaling server code for the realtime boundary.
- `records/`: record-store tests and helpers for persistent player records.
- `webrtc/`: WebRTC runtime configuration, signaling parsing, and tests for browser transport setup.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `app.rs`: Rust source file for app in this folder.
- `lib.rs`: Crate facade that ties the folder's modules into the public API surface.
- `observability.rs`: Observability types and metric helpers exposed by the API crate.
- `realtime.rs`: Realtime server surface that wires HTTP routes, websocket signaling, and transport dependencies.
- `records.rs`: Player-record storage surface exposed by the API crate.
- `transport.rs`: Transport abstraction glue between the app layer and external networking surfaces.
- `webrtc.rs`: WebRTC-facing API surface for runtime ICE and signaling configuration.
