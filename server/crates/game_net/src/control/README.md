# control

This directory contains packet-control codec modules for client commands, server events, and snapshot payloads.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `client.rs`: Client-side packet helpers for this module, usually focused on client-originated control messages.
- `codec.rs`: Low-level encoding and decoding helpers shared by the neighboring packet modules.
- `server.rs`: Server-side runtime helpers for the surrounding module; in realtime code this is the hosted HTTP or signaling server surface.
- `server_decode.rs`: Decode-side server packet logic for this module.
- `server_encode.rs`: Encode-side server packet logic for this module.
- `server_types.rs`: Shared type definitions used by the server packet modules in this folder.
- `snapshots.rs`: Rust source file for snapshots in this folder.
- `snapshots_decode.rs`: Decode-side snapshot packet logic for full and delta arena state.
- `snapshots_encode.rs`: Encode-side snapshot packet logic for full and delta arena state.
