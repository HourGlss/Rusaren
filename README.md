# Rarena

Rarena is a server-authoritative arena game prototype. The current repository now contains a backend-first vertical slice for lobby, match flow, simulation, and packet validation, plus local and CI quality tooling around it.

## Current status

Buildable now:
- the `server/` Cargo workspace scaffold
- a scripted backend-only gameplay slice that exercises lobby -> match -> combat -> no-contest flow
- a real websocket dev adapter on top of the backend app layer
- local quality scripts under `server/scripts`
- GitHub Actions quality workflows

Not implemented yet:
- Godot browser client
- real WebRTC transport integration
- content loading and validation
- full combat/class implementation

## Build and run

Build the Rust workspace:

```powershell
cd server
rustup run stable cargo build --workspace
```

Run the websocket dev adapter:

```powershell
cd server
rustup run stable cargo run -p dedicated_server --quiet
```

The dev adapter listens on:
- `http://127.0.0.1:3000/healthz`
- `ws://127.0.0.1:3000/ws`

Run the backend-only demo slice instead:

```powershell
cd server
rustup run stable cargo run -p dedicated_server --quiet -- --demo
```

Run the test suite:

```powershell
cd server
rustup run stable cargo test --workspace
```

Install the configured quality tools:

```powershell
cd server
./scripts/install-tools.ps1
```

Run the configured quality checks:

```powershell
cd server
./scripts/quality.ps1
```

Run the Verus network-boundary models explicitly if Verus is installed:

```powershell
cd server
./scripts/quality.ps1 verus
```

Generate the HTML reports locally:

```powershell
cd server
./scripts/quality.ps1 reports
```

Open the combined report:

```text
server/target/reports/output.html
```

Do not run `./scripts/quality.ps1 test` and `./scripts/quality.ps1 reports` in parallel. The coverage step uses its own target directory and those commands can interfere with each other if started at the same time.

## Commit workflow

Install the Git hooks once from the repo root:

```powershell
python -m pre_commit install --install-hooks --hook-type pre-commit --hook-type pre-push --hook-type post-commit
```

Recommended local flow for each change:

```powershell
cd server
rustup run stable cargo build --workspace
./scripts/quality.ps1 lint
./scripts/quality.ps1 test
./scripts/quality.ps1 reports
```

Then commit from the repo root:

```powershell
git add .
git commit -m "Describe the change"
```

Hook behavior:
- `pre-commit` runs fast repo checks such as whitespace, TOML/YAML validation, `typos`, `taplo`, and Rust formatting.
- `post-commit` generates the HTML reports and writes them to `server/target/reports/output.html`.
- `pre-push` runs Rust linting and tests before the branch leaves your machine.
- each push to `main` uploads a GitHub Actions artifact named `server-reports-<commit-sha>` that contains `server/target/reports/output.html`

Current local fallback behavior:
- if `cargo-nextest` is installed, the quality script uses it for the normal test task
- if `cargo-nextest` is not installed, the quality script falls back to `cargo test`

## Docs

Start with:
- `shared/docs/00_index.md`
- `shared/docs/12_rust_tooling.md`
- `shared/docs/13_verus_strategy.md`
- `shared/docs/14_buildability_assessment.md`
