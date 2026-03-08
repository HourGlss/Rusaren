# Rarena

Rarena is a server-authoritative arena game prototype. The current repository is still in the architecture-first stage: the `server/` Rust workspace and quality tooling are scaffolded, but the gameplay, networking, content, and client implementations have not been built yet.

## Current status

Buildable now:
- the `server/` Cargo workspace scaffold
- local quality scripts under `server/scripts`
- GitHub Actions quality workflows

Not implemented yet:
- real networking
- simulation logic
- content loading and validation
- a playable client

## Quick start

Run the current Rust workspace scaffold:

```powershell
cd server
rustup run stable cargo test --workspace
```

Install the configured quality tools:

```powershell
./scripts/install-tools.ps1
```

Run the configured quality checks:

```powershell
./scripts/quality.ps1
```

## Docs

Start with:
- `shared/docs/00_index.md`
- `shared/docs/12_rust_tooling.md`
- `shared/docs/13_verus_strategy.md`
- `shared/docs/14_buildability_assessment.md`
