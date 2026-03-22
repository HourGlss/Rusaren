# adr

This directory contains short architecture decision records for the backend surfaces that are expensive to change once the project reaches `1.0`.
Use these notes to understand why the repo froze a boundary, chose a persistence shape, or exposed an operator surface in a specific way.

## Structure
- `0001_protocol_surface.md`: Decision record for freezing the protocol and signaling surface before `1.0`.
- `0002_admin_surface.md`: Decision record for the private read-only operator dashboard and auth model.
- `0003_event_logging.md`: Decision record for future server-authored match and combat event logging.
- `0004_persistence_boundary.md`: Decision record for the current single-host persistence boundary and its limitations.
- `README.md`: This guide documents the folder structure and explains what the checked-in files are for.
