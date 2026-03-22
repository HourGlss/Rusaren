# docs

This directory contains long-form project documentation covering architecture, gameplay, networking, testing, and operations.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `adr/`: short architecture decision records for protocol, admin, logging, and persistence choices.
- `classes/`: class-specific design notes that complement the live authored YAML data.
- `maps/`: map-specific documentation and templates.
- `00_index.md`: Top-level documentation index that links the rest of the design and ops notes together.
- `01_principles.md`: Guiding engineering principles for the project.
- `02_repo_layout.md`: Narrative walkthrough of the repository structure.
- `03_domain_model.md`: Domain-model documentation for IDs, players, teams, and progression concepts.
- `04_match_flow.md`: Detailed description of the lobby-to-match flow.
- `05_simulation_loop.md`: Explanation of the fixed-tick simulation loop and its responsibilities.
- `06_networking.md`: Networking architecture notes for packets, transports, and deployment shape.
- `07_skills_spells_modifiers.md`: Gameplay design note for skills, spells, statuses, and modifier families.
- `08_godot_client.md`: Client-side design note for the Godot shell.
- `09_testing_ops.md`: Testing and operations playbook for local and CI validation.
- `10_maps.md`: Map design and authored arena notes.
- `11_classes.md`: High-level class roster and class-design notes.
- `12_rust_tooling.md`: Rust tooling guidance for building, linting, and analyzing the backend.
- `13_verus_strategy.md`: Verification strategy note for the Verus models.
- `14_buildability_assessment.md`: Buildability and readiness assessment for the current milestone.
- `15_deployment_ops.md`: Deployment and operations guide for the hosted backend stack.
- `16_runbooks.md`: Operational runbooks for routine backend tasks.
- `17_linode_deploy.md`: Linode-specific deployment guide for bringing up the hosted stack.
- `18_performance_budgets.md`: The current backend budget targets, reference environments, and performance-gating intent for `0.9`.
- `19_architecture_governance.md`: Crate-boundary rules, module discipline, ADR expectations, and the human PR review checklist.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
