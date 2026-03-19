# scripts

This directory contains backend automation scripts for exports, quality gates, reports, Docker smoke checks, and local play.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `build-docs.ps1`: Builds the mdBook, rustdoc, and documentation artifacts published by the quality pipeline.
- `check-core-coverage.ps1`: Checks the core runtime coverage gate after report generation.
- `docker-smoke.ps1`: Builds and smoke-tests the Docker deployment path locally.
- `export-web-client.ps1`: Exports the Godot web client into the backend static root.
- `generate-reports.ps1`: Generates the backend HTML, JSON, complexity, docs, and fuzz reports.
- `install-tools.ps1`: Installs the Rust, fuzzing, docs, and analysis tools used by the repo scripts.
- `play-local.ps1`: Starts the easiest local end-to-end browser path for the game.
- `quality.ps1`: Main orchestration script for linting, tests, fuzzing, reports, and other quality tasks.
- `verus.ps1`: Runs the repo's Verus models and related proof-oriented checks.
