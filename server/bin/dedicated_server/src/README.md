# src

This directory contains runtime configuration, logging, rendering helpers, and entrypoint code for the dedicated server.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `config.rs`: Configuration parsing and normalization helpers for this package.
- `demo.rs`: Demo-mode helpers and scenario drivers for the dedicated server.
- `logging.rs`: Tracing and log-format setup for the dedicated server.
- `main.rs`: Executable entrypoint that wires this package's runtime behavior together.
- `render.rs`: Human-readable event and state rendering helpers used by server logs or demos.
- `tests.rs`: Tests for the modules in this folder.
