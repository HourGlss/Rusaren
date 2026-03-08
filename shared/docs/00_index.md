# Arena Game Architecture (Server Authoritative)

This project is a round-based team arena game. The server is built in Rust.

Non-negotiables:
- Server runs all game logic (movement, collision, spells, rounds, win conditions).
- Client is presentation + input only (no authoritative collision, no authoritative damage).
- Data-driven skills/spells/modifiers to support frequent iteration.

Docs:
- 01_principles.md
- 02_repo_layout.md
- 03_domain_model.md
- 04_match_flow.md
- 05_simulation_loop.md
- 06_networking.md
- 07_skills_spells_modifiers.md
- 08_godot_client.md
- 09_testing_ops.md
- 10_maps.md
- 11_classes.md
- 12_rust_tooling.md
- 13_verus_strategy.md
- 14_buildability_assessment.md
