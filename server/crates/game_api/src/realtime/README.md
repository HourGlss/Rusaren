# realtime

This directory contains HTTP, websocket, session, and signaling server code for the realtime boundary.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `signaling/`: the signaling transport modules used by the realtime server surface.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `server.rs`: Server-side runtime helpers for the surrounding module; in realtime code this is the hosted HTTP or signaling server surface.
- `sessions.rs`: Session-tracking helpers that bind player identity, transport state, or browser bootstrap flow together.
- `tests.rs`: Tests for the modules in this folder.
