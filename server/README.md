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

## Authored Skill Cue IDs
Authored melee and spell entries in `content/skills/*.yaml` can now declare an optional `audio_cue_id`.
That field is copied into the connected skill catalog and exposed to the Godot client, but the backend does not load or play audio files itself.

Example:

```yaml
melee:
  id: warrior_broadswing
  name: Broadswing
  description: Heavy melee hit.
  audio_cue_id: warrior_broadswing
  cooldown_ms: 650
  range: 92
  radius: 42
  effect: melee_swing
  payload:
    kind: damage
    amount: 18
```

For spell tiers:

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

The matching frontend registry lives at `client/godot/content/audio/spell_cues.json`, and the default asset root there is `res://assets/audio/spells`.
The current `0.9.7` work only wires the shared cue ID seam; real playback assets and movement audio still belong to the remaining sound items in the roadmap.

## Mutation Campaigns
Use the helper scripts when a full cargo-mutants run would take hours and needs to be split into manual shards.

1. Create a campaign plan:
   `pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/plan-mutants.ps1 -RunId cast-passives -ShardCount 8`
2. Run one zero-based shard at a time:
   `pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/run-mutants-shard.ps1 -RunId cast-passives -Shard 0/8`
3. Rebuild the aggregate summary at any time:
   `pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/summarize-mutants.ps1 -RunId cast-passives`

Each campaign writes isolated shard output plus a merged `summary.md`, `summary.json`, `missed.txt`, and `timeout.txt` under `target/reports/mutants-campaigns/<run-id>/`.
The helper scripts validate zero-based shard descriptors, isolate each shard into its own scratch root and Cargo target directory, and ignore invalid shard folders during summary generation.

For `0.9.1` mutation hardening, prefer focused package and file filters instead of whole-workspace reruns that spend hours in top-level app timeouts.

- Visibility logic:
  `pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/plan-mutants.ps1 -RunId visibility-pass -ShardCount 2 -Jobs 1 -Package game_api -TestPackage game_api -File crates/game_api/src/app/snapshots/visibility.rs`
- Ingress and packet boundaries:
  `pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/plan-mutants.ps1 -RunId ingress-pass -ShardCount 2 -Jobs 1 -Package game_net -TestPackage game_net -File crates/game_net/src/ingress.rs`
- Sim walkability and arena rules:
  `pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/plan-mutants.ps1 -RunId sim-core-pass -ShardCount 2 -Jobs 1 -Package game_sim -TestPackage game_sim -File crates/game_sim/src/lib.rs`

The default `server/.cargo/mutants.toml` slice is intentionally biased toward gameplay, ingress, visibility, and protocol logic. Display-only error formatter files are left out of that default pass so release-line hardening time stays on higher-value rules.

## Lag And Desync Diagnostics
Use the client diagnostics menu together with the host-side collector so every report includes frontend, transport, backend, database, and host evidence in a repeatable format.

1. Reproduce the issue in the browser client.
2. In the client shell, open `Menu -> Diagnostics` and copy the structured report.
3. On the host, collect the server-side bundle:
   `bash deploy/useful_log_collect.sh --output /tmp/rusaren-diagnostics.txt --bundle-dir /tmp/rusaren-diagnostics-bundle`
4. Give the LLM both:
   - the copied client diagnostics text
   - `/tmp/rusaren-diagnostics.txt`
   - the bundle directory contents, especially:
     - `adminz.json`
     - `metrics.prom`
     - `docker-stats.txt`
     - `filtered-logs.txt`
     - `host.txt`

The browser diagnostics report captures:
- UI refresh timing
- visual smoothing timing
- packet decode timing
- snapshot apply timing
- arena draw timing
- Godot built-in monitor snapshots such as FPS, process time, node count, orphan node count, and render counters
- local object counts and footprint/visibility tile counts
- control/snapshot packet counts and byte totals
- current WebSocket and data-channel states

The host-side bundle captures:
- the same compact text summary as before
- a structured `/adminz?format=json` snapshot
- raw Prometheus metrics
- Docker `ps` and `stats --no-stream`
- filtered backend/Caddy/coturn logs
- host load, uptime, memory, and root filesystem usage

For frontend-only investigation, also run the repeatable client-side reference monitor pass:

```powershell
cd server
./scripts/quality.ps1 frontend-report
```

That writes:
- `target/reports/frontend/runtime_monitors.json`
- `target/reports/frontend/summary.json`
- `target/reports/frontend/index.html`

Use `runtime_monitors.json` together with the copied browser diagnostics text when asking the LLM to compare a good run against a bad run.
The runtime artifact includes both:
- official Godot `Performance` monitors
- custom `Rarena/*` monitors backed by the game's own UI and arena timing buckets

When the browser shell looks visually laggy, prioritize these frontend timings first:
- `arena_draw_ms`
- `arena_draw_base_ms`
- `arena_visibility_ms`
- `arena_cache_sync_ms`
- `arena_cache_background_ms`
- `arena_cache_visibility_ms`

For local editor profiling, open Godot's debugger and use:
- `Monitors` for built-in engine counters and custom `Rarena/*` counters
- `Profiler` for script timing
- the visual profiler when the problem looks render-side

Headless quality runs are the repeatable baseline; editor profiling is still the better source for real render-call inspection.

## Fixed-Reference Performance Gates
The current `0.9.6` reference gate lives in `game_api` and runs through:

- `pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/quality.ps1 soak`

That task now includes:
- repeated soak-match flow coverage
- a fixed-reference load scenario with `100` idle clients and `10` simultaneous matches
- command latency budgets
- tick latency budgets
- Linux RSS budget checks
- SQLite combat-log append and query budget checks
