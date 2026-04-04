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
- authoritative full and delta arena snapshots carrying match phase, hp, mana, cooldowns, active statuses, projectile state, and only the terrain/obstacles the viewing player is allowed to know about
- a first playable arena slice with a mostly empty map, four central square pillars, traversable shrub collars, authoritative player circles, per-player fog-of-war, WASD movement, mouse aim, left-click melee, authored class melee/spells on `1`-`5`, projectile combat, debuffs, HoTs, health, mana, and cooldown state
- held `X` self-cast for authored skills that normally target another player or an aimed point, plus crowd-control diminishing returns across hard-CC, movement-CC, and cast-CC buckets
- toggleable self-anchored auras such as Rogue Nightcloak, plus authored aura payload hooks that can fire at cast start and at aura end/cancel
- a shipped authored class roster of Warrior, Rogue, Mage, Cleric, Paladin, Ranger, Bard, Druid, and Necromancer
- runtime-loaded authored content under `server/content/skills/*.yaml`, `server/content/maps/prototype_arena.txt`, and `server/content/mechanics/registry.yaml`
- optional backend-authored `audio_cue_id` plumbing in the skill catalog, with the Godot client resolving cue IDs through `client/godot/content/audio/spell_cues.json` when frontend spell audio assets exist
- a same-origin Godot Web export path hosted directly by the Rust server at `/`
- a documented production-style deploy path with Caddy, Prometheus, and `coturn`
- stricter authored YAML and ASCII content validation with clean boot-time failures on invalid content
- backend gameplay tests that exercise every shipped melee and authored slot skill directly against the sim, including hit, miss, range, cooldown, mana, and status behavior
- repeated soak/load regression tests for lobby churn and repeated match completion without state leaks
- fixed-reference performance budget gates for `100` idle clients, `10` simultaneous matches, command and tick latency, Linux RSS, and SQLite combat-log append/query latency
- Criterion benchmark targets for hot-path sim ticks and snapshot packet codec work
- persistent player records under `server/var/player_records.tsv`
- append-only SQLite-backed match and combat logs plus replay-style regression checks built from those server-authored events
- a private authenticated `/adminz` operator surface with HTML and JSON views for runtime health, recent errors, and recent match/combat-log summaries
- local quality scripts under `server/scripts`
- GitHub Actions quality workflows plus Godot web export and deploy smoke workflows
- recurring hosted smoke and live transport probe timers on the Linux deploy path
- scheduled mutation-testing shards over ingress, protocol, visibility, match-flow, and simulation core logic
- a focused packet-ingress mutation smoke path that exercises `game_net::ingress` directly and catches exact-limit packet-boundary regressions

Not implemented yet:
- the full 1.0 authored class and spell set beyond the current shipped runtime kit
- shipped spell and movement audio playback beyond the new cue-id plumbing
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
- `http://127.0.0.1:3000/adminz` for the authenticated operator dashboard
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
for unstarted trees or the next tier for trees the player has already advanced inside a scrollable
catalog that can handle larger class sets.
The backend now also sends the authored skill catalog in the `Connected` event, and the Godot
skill picker renders those backend-authored names on the buttons instead of local placeholder labels.
The runtime game content now lives under:
- `server/content/skills/*.yaml` for authored class/skill definitions
- `server/content/maps/prototype_arena.txt` for the current ASCII arena map
- `server/content/mechanics/registry.yaml` for implemented and planned mechanic families, plus the data-driven validation schema for each implemented mechanic

Those files are the live source of truth for the backend. The Markdown docs under `shared/docs/`
document the design, but they are no longer treated as runtime content.
Adding more classes is now mostly centralized around:
- one new authored YAML file under `server/content/skills/`
- optional mechanic-family additions in `server/content/mechanics/registry.yaml` when you want to declare a new planned mechanic or extend validation metadata
- the small backend mechanic-specific execution locations in `server/crates/game_sim` only when a class needs a genuinely new runtime behavior

The UI and network catalog path now follow backend-authored class names and skill IDs instead of a fixed four-class wire enum.
That means classes using the existing runtime mechanic set can now be added without touching protocol or frontend registries.

Spell audio cue plumbing is now wired the same backend-authored way:
- add an optional `audio_cue_id` to `melee:` or a skill entry inside `server/content/skills/*.yaml`
- register that same cue ID in `client/godot/content/audio/spell_cues.json`
- place the eventual audio file under `client/godot/assets/audio/spells/`

Example authored skill snippet:

```yaml
- tier: 1
  id: mage_arc_bolt
  name: Arc Bolt
  description: Fast projectile damage.
  audio_cue_id: mage_arc_bolt
  behavior:
    kind: projectile
    effect: skill_shot
    cooldown_ms: 700
    mana_cost: 16
    speed: 320
    range: 1600
    radius: 18
    payload:
      kind: damage
      amount: 18
```

Matching frontend manifest entry:

```json
{
  "format_version": 1,
  "asset_root": "res://assets/audio/spells",
  "cues": {
    "mage_arc_bolt": {
      "file": "mage/arc_bolt.ogg"
    }
  }
}
```

The current client only resolves cue metadata; it does not play those files yet. This keeps the content and protocol seam in place so real spell audio can be added without another schema change.

Aura authoring now supports persistent self-toggles and payload hooks at the boundaries of the aura lifecycle:
- set `toggleable: true` on an `aura` behavior to make a self-anchored aura recastable as an off-toggle
- use `cast_start_payload` to apply an effect immediately when the aura is activated
- use `cast_end_payload` to apply an effect when the aura is canceled or expires naturally

Example authored stealth toggle:

```yaml
- tier: 4
  id: rogue_nightcloak
  name: Nightcloak
  description: Toggle into stealth until canceled or broken by action or damage.
  behavior:
    kind: aura
    effect: nova
    cooldown_ms: 2400
    mana_cost: 10
    toggleable: true
    radius: 12
    duration_ms: 30000
    tick_interval_ms: 1000
    cast_start_payload:
      kind: heal
      amount: 0
      status:
        kind: stealth
        duration_ms: 1200
        magnitude: 0
    payload:
      kind: heal
      amount: 0
      status:
        kind: stealth
        duration_ms: 1200
        magnitude: 0
```

Toggleable auras are intentionally restricted to self-anchored aura shapes. They cannot declare travel distance or deployable hit points.

The planned `0.9` player-token rendering language is:
- skill slot `1` colors the player center
- skill slots `2` through `5` add outward rings in pick order
- unpicked future slots render as black rings
- the outer team border is client-relative: your own team is dark blue and the opposing team is red
- thin status halos sit just outside the team ring, with negative effects on the left and positive effects on the right
- when multiple effects share one side, they split into distinct stacked sections ordered by remaining duration, longest at the top and shortest at the bottom

Class-color sourcing for that visual language is fixed as:
- WoW-style class colors for the closest analogue classes: Warrior, Mage, Rogue, Paladin, Druid, Ranger using Hunter's color, and Cleric using Priest's color
- the Glasbey et al. 2007 categorical-color approach for non-WoW classes and future class growth, with the reserved palette documented under `shared/docs/classes/README.md`

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

Run the Godot frontend smoke checks through the repo quality wrapper:

```powershell
cd server
./scripts/quality.ps1 frontend
```

Generate the docs-backed Godot runtime GDScript quality report and A-F grade:

```powershell
cd server
./scripts/quality.ps1 frontend-report
```

That writes the frontend summary and HTML report under:
- `server/target/reports/frontend/summary.json`
- `server/target/reports/frontend/index.html`

The frontend report path also now writes a structured runtime monitor artifact at:
- `server/target/reports/frontend/runtime_monitors.json`

That artifact is produced from:
- official Godot `Performance` monitors
- custom `Rarena/*` monitors for UI refresh, arena draw, visibility draw, player count, and visible tile count

Use it together with the browser's `Menu -> Diagnostics` output when the frontend feels slow or visually desynced.

For local Godot editor profiling, prefer:
- `Debugger -> Monitors`
- `Debugger -> Profiler`
- the visual profiler for render-heavy issues

Those editor tools remain the best source for real draw-call investigation; the headless quality artifact is meant to be the repeatable baseline.

Export the Godot web client into the Rust server static root:

```powershell
./server/scripts/export-web-client.ps1 -GodotExecutable <GODOT_EXECUTABLE> -InstallTemplates

# Linux
bash server/scripts/export-web-client.sh --godot-bin godot4
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
server/content/mechanics/registry.yaml
```

Restart the server or rerun `./server/scripts/play-local.ps1` after editing those files.

Run the test suite:

```powershell
cd server
rustup run stable cargo test --workspace
```

Run the soak and load regression suite:

```powershell
cd server
./scripts/quality.ps1 soak
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

Run a focused packet-boundary mutation smoke against ingress validation:

```powershell
cd server
$env:RARENA_MUTANTS_PACKAGE='game_net'
$env:RARENA_MUTANTS_TEST_PACKAGE='game_net'
$env:RARENA_MUTANTS_FILE='crates/game_net/src/ingress.rs'
$env:RARENA_MUTANTS_JOBS='1'
$env:RARENA_MUTANTS_TIMEOUT='60'
$env:RARENA_MUTANTS_BUILD_TIMEOUT='60'
./scripts/quality.ps1 mutants
```

That command is intentionally narrow and meant for local hardening of packet-boundary logic.
The broader mutation-testing slice runs in the scheduled GitHub Actions workflow.
If `F:\game_tests` exists on this machine, `./scripts/quality.ps1 mutants` now automatically uses it for mutation scratch space and Cargo build output to avoid exhausting the system drive.
For longer mutation work, prefer the campaign helper scripts in `server/README.md` and narrow the run with `-Package`, `-TestPackage`, and `-File` so `0.9.1` hardening stays focused on ingress, protocol, visibility, and simulation rules instead of broad app-loop timeouts.

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

`./scripts/quality.ps1 all` now includes the core runtime coverage gate after report generation.

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

The current hosted validation loop is expected to be routine, not one-off:
- deploy-time hosted smoke via `deploy/host-smoke.sh`
- recurring public smoke via `rusaren-smoke.timer`
- recurring real transport validation via `deploy/run_live_transport_probe.sh` and `rusaren-liveprobe.timer`

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
The primary ingress fuzz set now also includes structured round-trip fuzzing for control/input/signaling packets plus decode fuzzing for full and delta arena snapshots.

Run the core runtime coverage gate after reports:

```powershell
cd server
./scripts/quality.ps1 coverage-gate
```

Run the scheduled mutation-testing slice locally:

```powershell
cd server
./scripts/quality.ps1 mutants
```

That task uses `server/.cargo/mutants.toml` and writes its output under `server/target/reports/mutants/`.
When you need a longer campaign, use the shard helpers from `server/README.md`. Their shard syntax is zero-based (`0/8` through `7/8`), and the current release-line guidance is to run focused file/package slices rather than a whole-workspace campaign until the `1.0.0` freeze.

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
- `deploy/config.env.example`
- `deploy/docker-compose.override.example.yml`

High-level hosted flow:
1. let `deploy/deploy.sh` rebuild the Godot web client on Linux, or manually export it into `server/static/webclient/`
2. let `deploy/setup.sh` create `~/rusaren-config/config.env`, then edit that external file with the real host and secrets
3. run `sudo bash deploy/deploy.sh`

For the current live-domain target:
- `https://pvpnowfast.com/` should serve the game shell directly
- `https://turn.pvpnowfast.com/` should back STUN/TURN
- Ubuntu `24.04 LTS` is the documented Linode host target

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
- on-host Godot web export through `server/scripts/export-web-client.sh` during deploy

That is the honest target for the current codebase because match ownership and player records are still local to the running server.
- `shared/docs/16_runbooks.md`

## Docs

Start with:
- `shared/docs/00_index.md`
- `shared/docs/08_godot_client.md`
- `shared/docs/12_rust_tooling.md`
- `shared/docs/13_verus_strategy.md`
- `shared/docs/14_buildability_assessment.md`
