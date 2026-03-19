# signaling

This directory contains the signaling transport modules used by the realtime server surface.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `mod.rs`: Module entrypoint that ties together the sibling files in this folder.
- `transport.rs`: Transport abstraction glue between the app layer and external networking surfaces.
