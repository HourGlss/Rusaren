# src

This directory contains source modules for packet headers, control packets, input frames, errors, and ingress rules.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `control/`: packet-control codec modules for client commands, server events, and snapshot payloads.
- `ingress/`: focused ingress validation tests for the packet guard surface.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `control.rs`: Public control-packet module that wires client, server, and snapshot codec pieces together.
- `error.rs`: Error types and formatting helpers for this crate.
- `header.rs`: Packet header types and encode/decode helpers.
- `ingress.rs`: Ingress validation and first-packet/session guard logic.
- `input.rs`: Input-frame packet types and validation helpers.
- `lib.rs`: Crate facade that ties the folder's modules into the public API surface.
- `packet_types.rs`: Shared packet enums, constants, or type-level helpers.
