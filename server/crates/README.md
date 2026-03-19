# crates

This directory contains the backend's library crates, separated by domain, network, simulation, content, lobby, match, and API concerns.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `game_api/`: the application-service crate that binds together records, realtime transport, lobby flow, and snapshot delivery.
- `game_content/`: the content-loading crate that parses authored maps, mechanics, and skill YAML.
- `game_domain/`: the pure domain crate for IDs, player names, teams, rounds, and skill progression rules.
- `game_lobby/`: the lobby-state crate that models team selection, readiness, and countdown behavior.
- `game_match/`: the match-flow crate that manages phases, skill picks, defeats, and round scoring.
- `game_net/`: the wire-format crate for packet headers, control messages, ingress validation, and codec benchmarks.
- `game_sim/`: the simulation crate that owns authoritative combat, movement, cooldowns, statuses, and effect resolution.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
