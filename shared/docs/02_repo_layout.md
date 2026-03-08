# Repository Layout

Recommended mono-repo:

/server
  /crates
    game_domain/        # pure rules + data model (no IO)
    game_sim/           # tick loop, ECS/world state, collision resolution
    game_content/       # content loader + validation (skills/spells/modifiers)
    game_net/           # transport + protocol + snapshot/delta encoding
    game_lobby/         # lobby state machine, party/team selection, ready checks
    game_match/         # match orchestration, round flow, scoring
    game_api/           # HTTP for auth, versioning, service discovery, admin
  /bin
    dedicated_server/   # main entrypoint; wires crates together

/client
  godot/                # Godot project
  scripts/              # build + export tooling

/shared
  protocol/             # message schemas + versioning notes
  docs/                 # this documentation

Notes:
- Keep content data files in /shared/content (or /server/content) and mirror to client for UI only.
- Client can load the same content definitions for tooltips, but server validates and executes.
