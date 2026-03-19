# ingress

This directory contains command and input handling for lobby and match-flow state changes.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `lobby.rs`: Lobby-ingress handlers and supporting logic for player-facing lobby commands.
- `match_flow.rs`: Match-ingress handlers and supporting logic for active match flow and command gating.
- `mod.rs`: Module entrypoint that ties together the sibling files in this folder.
