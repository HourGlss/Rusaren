# scripts

This directory contains automation scripts for backend quality, Godot frontend checks, report generation, Docker smoke checks, and local play.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `build-docs.ps1`: Builds the mdBook, rustdoc, and documentation artifacts published by the quality pipeline.
- `check-core-coverage.ps1`: Checks the core runtime coverage gate after report generation.
- `docker-smoke.ps1`: Builds and smoke-tests the Docker deployment path locally.
- `export-web-client.ps1`: Windows-friendly Godot web export helper for the backend static root.
- `export-web-client.py`: Linux-friendly Godot web export helper for the backend static root.
- `frontend-quality.ps1`: Generates the Godot runtime GDScript quality report and A-F grade under `target/reports/frontend/`.
- `generate-reports.ps1`: Generates the backend reports plus the Godot frontend quality report.
- `install-tools.ps1`: Installs the Rust, fuzzing, docs, and analysis tools used by the repo scripts.
- `play-local.ps1`: Starts the easiest local end-to-end browser path for the game.
- `quality.ps1`: Main orchestration script for linting, tests, fuzzing, frontend checks, reports, and other quality tasks.
- `verus.ps1`: Runs the repo's Verus models and related proof-oriented checks.

## Notable quality tasks
- `./scripts/quality.ps1 soak`: runs the long-running soak suite plus the fixed-reference performance-budget gates.
- `./scripts/quality.ps1 reports`: generates the backend reports plus the frontend Godot quality report under `target/reports/`.
- `./scripts/quality.ps1 frontend-report`: generates the docs-backed A-F GDScript quality report for the Godot runtime code.
