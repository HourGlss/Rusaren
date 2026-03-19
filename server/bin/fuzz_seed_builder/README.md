# fuzz_seed_builder

This directory contains the utility crate that writes checked-in fuzz corpus seeds for the backend.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `src/`: seed generation logic for the repo's fuzz targets.
- `Cargo.toml`: Cargo manifest that declares this package's metadata, dependencies, and targets.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
