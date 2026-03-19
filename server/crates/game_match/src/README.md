# src

This directory contains split source modules for match types, flow, accessors, and tests.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `accessors.rs`: Read-oriented helpers and accessors for the surrounding module.
- `flow.rs`: State-transition and phase-flow logic for the surrounding crate.
- `lib.rs`: Crate facade that ties the folder's modules into the public API surface.
- `tests.rs`: Tests for the modules in this folder.
- `types.rs`: Shared type declarations for the surrounding module.
