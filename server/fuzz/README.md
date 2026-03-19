# fuzz

This directory contains the cargo-fuzz package, seed corpora, and harness definitions for backend fuzzing.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `artifacts/`: ignored local crash artifacts emitted by cargo-fuzz when a target finds an interesting failure.
- `corpus/`: checked-in fuzz corpora grouped by target so replay coverage and smoke runs stay deterministic.
- `coverage/`: ignored local line-coverage output generated while exploring fuzz targets.
- `fuzz_targets/`: cargo-fuzz entrypoints for each parser, codec, and ingress boundary under test.
- `support/`: shared Rust support modules used by multiple fuzz targets.
- `target/`: ignored build artifacts for the cargo-fuzz package.
- `Cargo.lock`: Workspace or package lockfile that pins Cargo dependency resolution for reproducible builds.
- `Cargo.toml`: Cargo manifest that declares this package's metadata, dependencies, and targets.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
