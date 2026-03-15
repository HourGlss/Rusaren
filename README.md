# Rarena

Current repo version: `0.8.0`

Rarena is a server-authoritative arena game prototype. The current repository now contains a backend-first vertical slice for lobby, match flow, simulation, and packet validation, plus local and CI quality tooling around it.

## Current status

Buildable now:
- the `server/` Cargo workspace scaffold
- a scripted backend-only gameplay slice that exercises lobby -> match -> combat -> no-contest flow
- a real WebRTC gameplay transport on top of websocket signaling, plus the older raw websocket dev adapter at `/ws-dev`
- a Godot 4 shell under `client/godot` that drives the browser gameplay path through websocket signaling at `/ws` and binary WebRTC data channels
- a focused in-match Godot shell layout that hides the setup chrome after lobby join, surfaces skill picks before the arena during skill-pick windows, and returns to the central layout on disconnect
- authoritative full and delta arena snapshots carrying match phase, hp, mana, cooldowns, active statuses, and projectile state
- a first playable arena slice with a mostly empty map, four central square pillars, traversable shrub collars, authoritative player circles, per-player fog-of-war, WASD movement, mouse aim, left-click melee, authored class melee/spells on `1`-`5`, projectile combat, debuffs, HoTs, health, mana, and cooldown state
- runtime-loaded authored content under `server/content/skills/*.yaml` and `server/content/maps/prototype_arena.txt`
- a same-origin Godot Web export path hosted directly by the Rust server at `/`
- a documented production-style deploy path with Caddy, Prometheus, and `coturn`
- stricter authored YAML and ASCII content validation with clean boot-time failures on invalid content
- backend gameplay tests that exercise every shipped melee and authored slot skill directly against the sim, including hit, miss, range, cooldown, mana, and status behavior
- Criterion benchmark targets for hot-path sim ticks and snapshot packet codec work
- persistent player records under `server/var/player_records.tsv`
- local quality scripts under `server/scripts`
- GitHub Actions quality workflows plus Godot web export and deploy smoke workflows

Not implemented yet:
- the full 1.0 authored class and spell set beyond the current shipped runtime kit
- more aggressive snapshot compression beyond the current full-vs-delta split
- the full 1.0 Godot gameplay presentation bar: HUD polish, stronger spell visuals, and always-readable health and mana display in crowded fights
- rustdoc/API guidance that is complete enough for an external client or bot author to play through the game protocol without Godot
- advanced vision features beyond the current per-player fog-of-war, explored-tile memory, and shrub sight blocking

## Build and run

Build the Rust workspace:

```powershell
cd server
rustup run stable cargo build --workspace
```

Run the backend:

```powershell
cd server
rustup run stable cargo run -p dedicated_server --quiet
```

Start the easiest local playable build:

```powershell
./server/scripts/play-local.ps1 -GodotExecutable <GODOT_EXECUTABLE>
```

That script exports the Godot web client, starts the Rust server directly on the host by default, and opens the browser shell at `http://127.0.0.1:3000/`.
Direct host mode is the default because browser WebRTC is more reliable there than behind local Docker NAT.

The backend listens on:
- `http://127.0.0.1:3000/healthz`
- `http://127.0.0.1:3000/metrics`
- `http://127.0.0.1:3000/session/bootstrap` for short-lived websocket bootstrap tokens
- `ws://127.0.0.1:3000/ws` for websocket signaling plus TURN/STUN configuration handoff
- `ws://127.0.0.1:3000/ws-dev` for the raw websocket dev adapter and legacy transport tests
- `http://127.0.0.1:3000/` for the exported Godot web shell when `server/static/webclient` exists

When deployed behind Caddy on the real domain, the same browser path becomes `https://<domain>/session/bootstrap` and `wss://<domain>/ws`.

The dev adapter persists player `W-L-NC` records at:
- `server/var/player_records.tsv`

The client no longer chooses its own runtime player ID.
The connect packet now sends only the player name, the Rust backend assigns a random player ID,
and the current persistent `W-L-NC` store is keyed by player name.
The skill-pick flow is server-gated by tree progression, and the Godot shell only enables tier 1
for unstarted trees or the next tier for trees the player has already advanced.
The runtime game content now lives under:
- `server/content/skills/*.yaml` for authored class/skill definitions
- `server/content/maps/prototype_arena.txt` for the current ASCII arena map

Those files are the live source of truth for the backend. The Markdown docs under `shared/docs/`
document the design, but they are no longer treated as runtime content.

Open the Godot shell:

```text
client/godot/project.godot
```

The current Godot shell is wired to `/session/bootstrap` for a one-time websocket token, then `/ws` for websocket signaling and WebRTC data channels for live gameplay traffic.
The project metadata version is currently `0.8.0`.
Known shell limitations:
- the arena slice is intentionally simple, even though the current skills and map now load from authored content files
- the current fog-of-war is authoritative and per-player, but it is still a simple radius-and-blocker implementation rather than the final polished vision system
- the shell now consumes authoritative full and delta arena snapshots plus effect batches, but the current delta format is still a simple dynamic-state packet rather than a heavily compressed baseline-referenced diff
- native/headless Godot transport testing depends on the `webrtc-native` extension being available to the editor/runtime; if your local Godot install ships that extension under a folder like `Godot/webrtc/`, `export-web-client.ps1` now syncs that bundle into the ignored local project path `client/godot/webrtc/` before export or headless checks
- browser play remains the primary supported networked path on this machine; the synced native extension is for local editor/headless validation and is not tracked in git

Run the Godot protocol checks headlessly:

```powershell
godot4 --headless --path client/godot -s res://tests/protocol_checks.gd
```

On this machine, the equivalent command is:

```powershell
<GODOT_EXECUTABLE> --headless --path client\godot -s res://tests/protocol_checks.gd
```

Run the browser-export checks headlessly:

```powershell
godot4 --headless --path client/godot -s res://tests/web_export_checks.gd
```

On this machine, the equivalent command is:

```powershell
<GODOT_EXECUTABLE> --headless --path client\godot -s res://tests/web_export_checks.gd
```

Run the shell layout checks headlessly:

```powershell
godot4 --headless --path client/godot -s res://tests/shell_layout_checks.gd
```

On this machine, the equivalent command is:

```powershell
<GODOT_EXECUTABLE> --headless --path client\godot -s res://tests/shell_layout_checks.gd
```

Export the Godot web client into the Rust server static root:

```powershell
./server/scripts/export-web-client.ps1 -GodotExecutable <GODOT_EXECUTABLE> -InstallTemplates
```

If a local `Godot/webrtc/` bundle exists, that export script also syncs it into the ignored local project path `client/godot/webrtc/` so native/headless Godot checks can use the same extension bundle.

For CI or a machine without a local Godot install, the script can download a portable editor and export templates:

```powershell
./server/scripts/export-web-client.ps1 -DownloadPortable -InstallTemplates
```

Run the backend-only demo slice instead:

```powershell
cd server
rustup run stable cargo run -p dedicated_server --quiet -- --demo
```

Edit gameplay content quickly:

```text
server/content/skills/*.yaml
server/content/maps/prototype_arena.txt
```

Restart the server or rerun `./server/scripts/play-local.ps1` after editing those files.

Run the test suite:

```powershell
cd server
rustup run stable cargo test --workspace
```

Run the ingress fuzz smoke checks:

```powershell
cd server
./scripts/install-tools.ps1 -IncludeNightly -IncludeFuzzTools
./scripts/quality.ps1 fuzz
```

Run a bounded live ingress fuzz campaign on Linux, Docker, or WSL:

```powershell
cd server
./scripts/quality.ps1 fuzz-live
```

Install the configured quality tools:

```powershell
cd server
./scripts/install-tools.ps1
```

That script now installs Verus into the repo-local cache at `server/tools/verus/current`.
It also installs `mdbook`, `cargo-fuzz`, and the nightly toolchain required by the pre-commit ingress fuzz hook.
The backend call-graph report now uses the repo-local `backend_callgraph` binary in this workspace plus `rust-analyzer`, so there is no separate call-graph tool checkout to manage.

Compile the hot-path benchmarks:

```powershell
cd server
rustup run stable cargo bench --workspace --no-run
```

Run the Rust WebRTC integration suite:

```powershell
cd server
rustup run stable cargo test -p game_api --test realtime_webrtc -- --nocapture
```

Run the configured quality checks:

```powershell
cd server
./scripts/quality.ps1
```

On Linux or inside a container, the checked-in `server/Makefile` provides the same entrypoints through `make lint`, `make test`, `make fuzz`, `make verus`, and `make reports`.

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
- `server/target/reports/tests/nextest.jsonl`
- `server/target/reports/complexity/summary.json`

On pushes to `main`, the same `server/target/reports/` tree is also published to GitHub Pages.
The intended Pages landing URL for this repo is:
- `https://hourglss.github.io/Rusaren/`

Useful Pages paths:
- `https://hourglss.github.io/Rusaren/` for the report index
- `https://hourglss.github.io/Rusaren/docs/site/` for the mdBook docs site
- `https://hourglss.github.io/Rusaren/rustdoc/` for the Rust API docs

For the `1.0.0` release line, `/rustdoc/` is not just a published artifact.
It is expected to document how an external client or bot can play the game through the API:
- session bootstrap
- websocket signaling and WebRTC setup
- control and input messages
- snapshots and player status/state interpretation
- enough example flow to connect, join a lobby, enter a match, and play

The GitHub Actions job summary for `server-quality` now includes:
- total Rust tests, passed tests, skipped tests, and total test duration from the structured `nextest` log
- the current complexity score and the top worst-function hotspot from `server/target/reports/complexity/summary.json`

If that URL does not come up after a successful `server-quality` run, the remaining GitHub-side step is:
- repo `Settings -> Pages -> Source: GitHub Actions`

The docs report includes a per-file publication table for every Markdown file under `shared/docs`.
The fuzz report now shows replay coverage over the checked-in seed corpus plus any discovered corpus already present under `server/target/fuzz-generated-corpus/`. The headline score is intentionally scoped to network-ingress targets such as packet decode, ingress sequencing, server-control-event decode, and WebRTC signaling JSON parsing. On native Windows, `cargo fuzz run` is still not dependable for this repo, so real live fuzz campaigns are expected to run in Linux CI, Docker, or WSL.

Run the local Docker deploy smoke path:

```powershell
./server/scripts/docker-smoke.ps1
```

That script validates the checked-in compose file, builds the current server image, runs the container under a read-only root filesystem with dropped Linux capabilities, and probes `/`, `/healthz`, and `/metrics`.

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
- `pre-commit` also runs the current ingress fuzz smoke task when network-boundary or fuzz-target files change.
- `post-commit` generates the HTML reports and writes them to `server/target/reports/output.html`.
- `post-commit` also refreshes the docs site, Rust API docs, and backend call graph under `server/target/reports/`.
- `pre-push` runs Rust linting and tests before the branch leaves your machine.
- each push to `main` uploads a GitHub Actions artifact named `server-reports-<commit-sha>` that contains `server/target/reports/output.html`
- each successful push to `main` also deploys the same report tree to GitHub Pages so reports and docs can be reviewed without downloading artifacts

Current local fallback behavior:
- if `cargo-nextest` is installed, the quality script uses it for the normal test task
- if `cargo-nextest` is not installed, the quality script falls back to `cargo test`
- fuzzing uses `cargo-fuzz` under `server/fuzz/` and is prioritized around ingress boundaries where external data enters the application, especially networking paths such as packet-header, control-command, server-control-event, input-frame, ingress-session decoding/validation, and WebRTC signaling JSON parsing
- local Windows fuzzing is a smoke/build path; bounded live `cargo fuzz run` campaigns are enforced in Linux CI where the sanitizer runtime is available
- gameplay correctness is primarily enforced with Rust unit and integration tests, not fuzzing
- project docs are generated from `shared/docs` through `mdBook`, while Rust API docs are generated with `cargo doc --workspace --all-features --no-deps`
- browser-export smoke checks run in `.github/workflows/godot-web-smoke.yml` and verify that the exported shell can be hosted by `dedicated_server`
- deploy smoke checks run in `.github/workflows/deploy-stack-smoke.yml` and through `./server/scripts/docker-smoke.ps1`, validating the Docker image plus the checked-in compose path

Current manual full-loop slice:
- start the Rust backend
- export the Godot web shell and open `http://127.0.0.1:3000/` in two browser tabs
- connect both players, let the server assign their runtime player IDs, create/join a lobby, choose teams, ready up
- click a lobby from the central directory or join by manual lobby ID
- choose a skill each round
- use `WASD` to move during combat
- aim with the mouse inside the arena
- left click for melee
- use `1`-`5` for the currently unlocked authored skill slots loaded from YAML
- watch the cooldown HUD, mana bars, and active status labels update from authoritative server snapshots as you fight
- review the result screen and quit back to the central lobby

Current easiest full-loop slice:
- run `./server/scripts/play-local.ps1 -GodotExecutable <GODOT_EXECUTABLE>`
- open two browser tabs to `http://127.0.0.1:3000/`
- connect two players, receive server-assigned IDs, and play through the placeholder round flow

## Deploy

The checked-in `0.8.0` hosted path is:
- `server/Dockerfile`
- `deploy/docker-compose.yml`
- `deploy/Caddyfile`
- `deploy/prometheus.yml`
- `deploy/coturn/turnserver.conf`
- `deploy/.env.example`

High-level hosted flow:
1. export the Godot web client into `server/static/webclient/`
2. copy `deploy/.env.example` to `deploy/.env` and fill the real host and secrets
3. run `docker compose --env-file deploy/.env -f deploy/docker-compose.yml build`
4. run `docker compose --env-file deploy/.env -f deploy/docker-compose.yml up -d`

Local container smoke before a real host deploy:
1. run `./server/scripts/docker-smoke.ps1`
2. verify the local image serves `/`, `/healthz`, and `/metrics`

Use:
- `shared/docs/15_deployment_ops.md`
- `shared/docs/17_linode_deploy.md`

For the first real internet-reachable test, the current recommended shape is:
- one app host
- optional separate TURN host
- Docker Compose deploy
- exported Godot web bundle copied to the host before `docker compose build`

That is the honest target for the current codebase because match ownership and player records are still local to the running server.
- `shared/docs/16_runbooks.md`

## Docs

Start with:
- `shared/docs/00_index.md`
- `shared/docs/08_godot_client.md`
- `shared/docs/12_rust_tooling.md`
- `shared/docs/13_verus_strategy.md`
- `shared/docs/14_buildability_assessment.md`
