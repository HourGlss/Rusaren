# Rust Tooling Baseline

This repo is currently architecture-first. The docs define the target crate layout and the quality goals before the Rust implementation exists, so the first setup step is to make those expectations explicit in the workspace and local tooling.

## Editor baseline

Recommended VS Code extensions:
- `rust-lang.rust-analyzer`
- `vadimcn.vscode-lldb`
- `tamasfe.even-better-toml`

The repo-local `.vscode` settings point `rust-analyzer` at `server/Cargo.toml` and use `clippy` as the default check command.

## Tooling baseline

Primary quality tools:
- `cargo-nextest` for the default test runner
- `cargo-llvm-cov` for coverage
- `clippy` for static analysis
- `cargo-deny` for license, source, and advisory policy
- `cargo-audit` for RustSec checks
- `cargo-hack` for feature-matrix checking
- `cargo-udeps` for unused dependency detection
- `cargo-mutants` for mutation testing on branch-heavy rules code
- `rust-code-analysis-cli` for complexity and maintainability metrics
- `cargo-geiger` for unsafe usage reporting
- `miri` for undefined-behavior checks
- `cargo-fuzz` for coverage-guided fuzzing against the network boundary and future content loaders
- `mdbook` for generated architecture, protocol, and ops docs from `shared/docs`
- `cargo doc --workspace --all-features --no-deps` for Rust API docs
- `typos-cli` for docs and source spelling checks
- `taplo-cli` for TOML formatting checks
- `zizmor` for GitHub Actions security analysis
- `criterion` for repeatable microbenchmarks in hot-path crates

## Why these tools fit this repo

The current docs create a few clear priorities:
- [01_principles.md](01_principles.md): server-authoritative, client-untrusted design
- [05_simulation_loop.md](05_simulation_loop.md): fixed-tick deterministic-ish simulation
- [06_networking.md](06_networking.md): untrusted protocol input and strict versioning
- [07_skills_spells_modifiers.md](07_skills_spells_modifiers.md): data-driven content and composable effects
- [09_testing_ops.md](09_testing_ops.md): validation, replay, and abuse-resistance requirements
- [10_maps.md](10_maps.md): line-of-sight, stealth, and fog-of-war rules

That means this repo should optimize for correctness, state-machine regression resistance, and malicious-input handling before it optimizes for developer convenience.

## Planned crate priorities

`game_domain`
- unit and property tests for progression, cooldowns, modifiers, and statuses
- `cargo-mutants` and `miri` have high value because this crate should stay pure and small

`game_content`
- validation tests for skill graphs, effect references, stat keys, maps, and classes
- first fuzz target for malformed content payloads and graph edge cases

`game_sim`
- replay-based regression tests
- property tests for cast, channel, interrupt, and scheduled effects
- high priority for complexity tracking and future fuzzing
- first Criterion benchmark target once the tick loop exists

`game_net`
- round-trip tests for snapshots and deltas
- fuzzing for decoder paths and invalid command sequences
- strict linting if any unsafe parsing or byte-level work appears

`game_lobby` and `game_match`
- state-machine tests for ready checks, team-change resets, countdown locks, disconnect-aborts, and no-timeout combat flow
- mutation testing is useful because the logic will be branch-heavy

`game_api`
- integration tests and dependency-policy checks
- lower fuzzing priority than `game_content`, `game_net`, and `game_sim`

## Complexity-guided fuzzing

There is no single mainstream Rust tool that automatically picks fuzz targets from static analysis alone. The practical workflow is:
1. generate complexity metrics with `rust-code-analysis-cli`
2. combine that with low-coverage areas from `cargo-llvm-cov`
3. use `cargo-geiger` to prioritize any unsafe-adjacent logic
4. build or expand `cargo-fuzz` targets for the top-ranked modules

For this repo, the first target order should be:
1. protocol decoding and ingress sequencing in `game_net`
2. content loaders and validators in `game_content`
3. tick input streams and state transitions in `game_sim`
4. line-of-sight and stealth visibility logic described in [10_maps.md](10_maps.md)

The initial real fuzz targets now live under `server/fuzz/` and cover:
- packet header decode
- client control command decode
- server control event decode
- validated input frame decode
- ingress/session sequencing
- HTTP route classification for the observability layer
- Prometheus observability metric rendering and counter/gauge update flows
- persisted player-record TSV parsing and canonicalization at the storage boundary

## Commands

Install the baseline tools:

```powershell
./server/scripts/install-tools.ps1
```

Run the common stable checks:

```powershell
./server/scripts/quality.ps1
```

Run advanced checks after installing nightly:

```powershell
./server/scripts/install-tools.ps1 -IncludeNightly -IncludeFuzzTools
./server/scripts/quality.ps1 udeps
./server/scripts/quality.ps1 miri
./server/scripts/quality.ps1 complexity
./server/scripts/quality.ps1 bench
./server/scripts/quality.ps1 fuzz
```

Additional repo-wide checks:

```powershell
./server/scripts/quality.ps1 hack
./server/scripts/quality.ps1 docs-artifacts
./server/scripts/quality.ps1 typos
./server/scripts/quality.ps1 taplo
./server/scripts/quality.ps1 zizmor
```

## Generated docs

The source of truth stays in `shared/docs`, but `mdBook` now generates a browsable docs site under `server/target/reports/docs/site`, and `cargo doc --workspace --all-features --no-deps` publishes workspace API docs under `server/target/reports/rustdoc`.

Those documentation artifacts are regenerated by the post-commit hook and uploaded by CI in the same artifact bundle as coverage, complexity, and callgraph reports.
