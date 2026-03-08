# Rarena

Current repo version: `0.5.0`

Rarena is a server-authoritative arena game prototype. The current repository now contains a backend-first vertical slice for lobby, match flow, simulation, and packet validation, plus local and CI quality tooling around it.

## Current status

Buildable now:
- the `server/` Cargo workspace scaffold
- a scripted backend-only gameplay slice that exercises lobby -> match -> combat -> no-contest flow
- a real websocket dev adapter on top of the backend app layer
- a Godot 4 shell under `client/godot` that drives the websocket dev adapter with real binary control packets and live combat input frames
- a same-origin Godot Web export path hosted directly by the Rust server at `/`
- persistent player records under `server/var/player_records.tsv`
- local quality scripts under `server/scripts`
- GitHub Actions quality workflows plus a Godot web export smoke workflow

Not implemented yet:
- real WebRTC transport integration
- full Godot gameplay rendering
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
- `http://127.0.0.1:3000/` for the exported Godot web shell when `server/static/webclient` exists

The dev adapter persists player `W-L-NC` records at:
- `server/var/player_records.tsv`

Open the Godot shell:

```text
client/godot/project.godot
```

The current Godot shell is wired to the websocket dev adapter first, not WebRTC yet.
The project metadata version is currently `0.5.0`.
Known shell limitations:
- the final production transport is still planned as WebRTC, so browser play currently uses the websocket dev adapter
- gameplay rendering is still placeholder-only even though the shell now consumes central lobby directory snapshots and full lobby snapshots
- the current local combat slice resolves rounds with a placeholder primary attack button rather than final movement/casting gameplay

Run the Godot protocol checks headlessly:

```powershell
godot4 --headless --path client/godot -s res://tests/protocol_checks.gd
```

On this machine, the equivalent command is:

```powershell
C:\Users\azbai\Documents\Rarena\Godot\Godot_v4.6.1-stable_win64_console.exe --headless --path client\godot -s res://tests/protocol_checks.gd
```

Run the browser-export checks headlessly:

```powershell
godot4 --headless --path client/godot -s res://tests/web_export_checks.gd
```

On this machine, the equivalent command is:

```powershell
C:\Users\azbai\Documents\Rarena\Godot\Godot_v4.6.1-stable_win64_console.exe --headless --path client\godot -s res://tests/web_export_checks.gd
```

Export the Godot web client into the Rust server static root:

```powershell
./server/scripts/export-web-client.ps1 -GodotExecutable C:\Users\azbai\Documents\Rarena\Godot\Godot_v4.6.1-stable_win64_console.exe -InstallTemplates
```

For CI or a machine without a local Godot install, the script can download a portable editor and export templates:

```powershell
./server/scripts/export-web-client.ps1 -DownloadPortable -InstallTemplates
```

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

Build the initial fuzz targets:

```powershell
cd server
./scripts/install-tools.ps1 -IncludeNightly -IncludeFuzzTools
./scripts/quality.ps1 fuzz
```

Install the configured quality tools:

```powershell
cd server
./scripts/install-tools.ps1
```

That script now installs Verus into the repo-local cache at `server/tools/verus/current`.
It also installs `mdbook`, `cargo-fuzz`, and the nightly toolchain required by the pre-commit fuzz hook.
The backend call-graph report now uses the repo-local `backend_callgraph` binary in this workspace plus `rust-analyzer`, so there is no separate call-graph tool checkout to manage.

Run the configured quality checks:

```powershell
cd server
./scripts/quality.ps1
```

Run only the Verus network-boundary models:

```powershell
cd server
./scripts/quality.ps1 verus
```

Generate the HTML reports locally:

```powershell
cd server
./scripts/quality.ps1 reports
```

Generate the documentation artifacts only:

```powershell
cd server
./scripts/quality.ps1 docs-artifacts
```

Generate only the backend call-graph report:

```powershell
cd server
./scripts/quality.ps1 callgraph
```

Open the combined report:

```text
server/target/reports/output.html
```

Open the fuzz corpus coverage report directly:

```text
server/target/reports/fuzz/output.html
```

Open the main backend call graph directly:

```text
server/target/reports/callgraph/output.html
```

The quickest curated backend artifact is:

```text
server/target/reports/callgraph/backend_core.overview.simple.svg
```

The detailed function-level graph is still available at:

```text
server/target/reports/callgraph/backend_core.simple.svg
```

The call-graph report always writes DOT plus safe SVG output:
- `server/target/reports/callgraph/backend_core.overview.dot`
- `server/target/reports/callgraph/backend_core.overview.simple.svg`
- `server/target/reports/callgraph/backend_core.dot`
- `server/target/reports/callgraph/backend_core.simple.svg`

The documentation artifacts live at:
- `server/target/reports/docs/index.html`
- `server/target/reports/docs/summary.json`
- `server/target/reports/docs/site/index.html`
- `server/target/reports/rustdoc/index.html`

The docs report includes a per-file publication table for every Markdown file under `shared/docs`.
The fuzz report shows corpus replay coverage, which means line coverage measured by replaying the checked-in seed corpus through the same decode and ingress APIs used by the fuzz targets.

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
./scripts/quality.ps1 fuzz
./scripts/quality.ps1 verus
./scripts/quality.ps1 test
./scripts/quality.ps1 reports
```

Then validate the current Godot shell protocol path:

```powershell
godot4 --headless --path client/godot -s res://tests/protocol_checks.gd
```

Recommended advanced local flow before touching network-boundary code:

```powershell
cd server
./scripts/install-tools.ps1 -IncludeNightly -IncludeFuzzTools
./scripts/quality.ps1 fuzz
```

Then commit from the repo root:

```powershell
git add .
git commit -m "Describe the change"
```

Hook behavior:
- `pre-commit` runs fast repo checks such as whitespace, TOML/YAML validation, `typos`, `taplo`, and Rust formatting.
- `pre-commit` also builds the current fuzz targets when network-boundary or fuzz-target files change.
- `post-commit` generates the HTML reports and writes them to `server/target/reports/output.html`.
- `post-commit` also refreshes the docs site, Rust API docs, and backend call graph under `server/target/reports/`.
- `pre-push` runs Rust linting and tests before the branch leaves your machine.
- each push to `main` uploads a GitHub Actions artifact named `server-reports-<commit-sha>` that contains `server/target/reports/output.html`

Current local fallback behavior:
- if `cargo-nextest` is installed, the quality script uses it for the normal test task
- if `cargo-nextest` is not installed, the quality script falls back to `cargo test`
- fuzzing uses `cargo-fuzz` under `server/fuzz/` and currently starts with packet-header, control-command, server-control-event, input-frame, and ingress-session targets
- project docs are generated from `shared/docs` through `mdBook`, while Rust API docs are generated with `cargo doc --workspace --all-features --no-deps`
- browser-export smoke checks run in `.github/workflows/godot-web-smoke.yml` and verify that the exported shell can be hosted by `dedicated_server`

Current manual full-loop slice:
- start the Rust backend
- export the Godot web shell and open `http://127.0.0.1:3000/` in two browser tabs, or open two native Godot clients
- connect both players, create/join a lobby, choose teams, ready up
- click a lobby from the central directory or join by manual lobby ID
- choose a skill each round
- press `Primary Attack` during combat to resolve the current placeholder one-hit round flow
- review the result screen and quit back to the central lobby

## Docs

Start with:
- `shared/docs/00_index.md`
- `shared/docs/08_godot_client.md`
- `shared/docs/12_rust_tooling.md`
- `shared/docs/13_verus_strategy.md`
- `shared/docs/14_buildability_assessment.md`
