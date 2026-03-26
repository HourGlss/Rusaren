# server

This directory contains the Rust workspace, authored content, quality tooling, verification models, and deployment runtime pieces for the backend.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `.cargo/`: Cargo configuration that shapes workspace builds and mutation-testing behavior.
- `.config/`: tool-specific runtime configuration checked into the backend workspace.
- `bin/`: small executable packages that support analysis, serving, and test-data generation around the main backend.
- `content/`: runtime-authored gameplay content loaded by the backend at startup.
- `crates/`: the backend's library crates, separated by domain, network, simulation, content, lobby, match, and API concerns.
- `fuzz/`: the cargo-fuzz package, seed corpora, and harness definitions for backend fuzzing.
- `mutants.out/`: ignored local mutation-testing output from cargo-mutants runs.
- `scripts/`: backend automation scripts for exports, quality gates, reports, Docker smoke checks, and local play.
- `static/`: checked-in static asset stubs and the landing place for the exported web client bundle.
- `target/`: ignored local build artifacts and generated reports for the backend workspace.
- `target/reports/mutants-campaigns/`: ignored long-running mutation-test campaign output planned and summarized by the helper scripts.
- `tools/`: ignored repo-local tool caches installed by the workspace scripts.
- `var/`: ignored local runtime state such as player records and other writable backend data.
- `verus/`: Verus models that specify and check the backend's protocol and ingress invariants.
- `Cargo.lock`: Workspace or package lockfile that pins Cargo dependency resolution for reproducible builds.
- `Cargo.toml`: Cargo manifest that declares this package's metadata, dependencies, and targets.
- `Dockerfile`: Container build recipe for the production dedicated server image.
- `Makefile`: Linux-friendly shortcuts for the repo's common quality and report commands.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `clippy.toml`: Clippy configuration that tunes lint thresholds and allowed patterns for the backend workspace.
- `deny.toml`: cargo-deny policy file for dependency and license checks.
- `rust-toolchain.toml`: Pinned Rust toolchain configuration for the backend workspace.

## Mutation Campaigns
Use the helper scripts when a full cargo-mutants run would take hours and needs to be split into manual shards.

1. Create a campaign plan:
   `pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/plan-mutants.ps1 -RunId cast-passives -ShardCount 8`
2. Run one shard at a time:
   `pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/run-mutants-shard.ps1 -RunId cast-passives -Shard 1/8`
3. Rebuild the aggregate summary at any time:
   `pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/summarize-mutants.ps1 -RunId cast-passives`

Each campaign writes isolated shard output plus a merged `summary.md`, `summary.json`, `missed.txt`, and `timeout.txt` under `target/reports/mutants-campaigns/<run-id>/`.
