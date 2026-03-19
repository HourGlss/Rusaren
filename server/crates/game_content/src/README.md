# src

This directory contains source modules for authored-content models, parsing, validation, and tests.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `skills/`: skill-behavior parsing and normalization helpers for authored content.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `error.rs`: Error types and formatting helpers for this crate.
- `lib.rs`: Crate facade that ties the folder's modules into the public API surface.
- `maps.rs`: Map parsing and authored map helpers for the content crate.
- `mechanics.rs`: Mechanic-registry parsing and validation helpers for authored gameplay content.
- `model.rs`: Core authored-content data structures shared by the content loader.
- `tests.rs`: Tests for the modules in this folder.
- `yaml.rs`: YAML parsing and validation helpers for the authored content pipeline.
