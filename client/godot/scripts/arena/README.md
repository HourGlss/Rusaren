# arena

This directory contains Godot scripts for rendering the arena, players, projectiles, and combat-state visuals.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `arena_view.gd`: Arena rendering script that projects authoritative simulation state into the Godot scene.
- `arena_view.gd.uid`: Godot UID sidecar for `arena_view`. It preserves a stable resource identifier for the neighboring script.

## Planned 0.9 Rendering Contract
- Player physics and collision size stay unchanged; readability changes are visual only.
- `arena_view.gd` is expected to render players as concentric rings driven by skill-pick order:
  - slot `1` is the center fill
  - slots `2` through `5` become outward rings
  - unpicked slots render as black rings
- The outer border is team-relative on each client:
  - friendly players render with a dark-blue border
  - enemy players render with a red border
- Status presentation sits just outside the team ring as a thin halo:
  - negative effects on the left side
  - positive effects on the right side
  - multiple effects on one side split into distinct sections ordered by remaining duration, longest at the top and shortest at the bottom
- Scrolling combat text is player-only and should follow a World of Warcraft style flow for that player's own outgoing and incoming events.
