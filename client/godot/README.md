# godot

This directory contains the Godot 4 browser shell that talks to the Rust backend over websocket signaling and WebRTC.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `.godot/`: local Godot editor metadata generated on developer machines. It is intentionally ignored from git.
- `content/`: checked-in client-side runtime manifests and other data files loaded directly by the Godot shell.
- `scenes/`: Godot scene files that define the shell's node layout.
- `scripts/content/`: runtime content loaders and registries that translate checked-in manifests into frontend lookups.
- `scripts/`: top-level Godot scripts that coordinate UI flow, networking, and state transitions.
- `tests/`: headless Godot checks for protocol behavior, shell layout, exports, and transport assumptions.
- `webrtc/`: local sync target for the optional native WebRTC extension bundle. It is intentionally ignored from git.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `export_presets.cfg`: Godot export preset definitions used by the web export script.
- `project.godot`: Godot project manifest for the browser shell.

## Future Spell Audio Assets
The frontend spell-audio seam is now checked in even though playback is not enabled yet.

Use these paths when real assets arrive:
- manifest: `client/godot/content/audio/spell_cues.json`
- registry loader: `client/godot/scripts/content/spell_audio_registry.gd`
- default asset root for actual files: `client/godot/assets/audio/spells/`

The manifest format is JSON because Godot can load it directly with `JSON.parse_string`:

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

To link that cue to an authored spell, add the same `audio_cue_id` in the backend skill YAML under `server/content/skills/*.yaml`:

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

The client currently resolves cue metadata only. Real playback and movement audio still belong to the remaining sound items in `0.9.7`.

## Frontend Performance Monitoring
The frontend now uses Godot's built-in `Performance` monitors together with repo-specific custom monitors.
This is the first-line performance path for the browser shell because it lets us correlate engine timing with the game's own UI and arena draw timings.

Built-in monitors sampled by the frontend diagnostics:
- `TIME_FPS`
- `TIME_PROCESS`
- `TIME_PHYSICS_PROCESS`
- `OBJECT_COUNT`
- `OBJECT_NODE_COUNT`
- `OBJECT_ORPHAN_NODE_COUNT`
- `RENDER_TOTAL_OBJECTS_IN_FRAME`
- `RENDER_TOTAL_PRIMITIVES_IN_FRAME`
- `RENDER_TOTAL_DRAW_CALLS_IN_FRAME`
- `RENDER_VIDEO_MEM_USED`

Custom Godot monitors registered by the client:
- `Rarena/UIRefreshMs`
- `Rarena/ArenaDrawMs`
- `Rarena/ArenaVisibilityMs`
- `Rarena/ArenaBaseDrawMs`
- `Rarena/ArenaCacheSyncMs`
- `Rarena/ArenaCacheBackgroundMs`
- `Rarena/ArenaCacheVisibilityMs`
- `Rarena/Players`
- `Rarena/VisibleTiles`

These custom monitors are visible in the Godot debugger's monitor panel and are also sampled by the headless frontend quality path.
The cache-related monitors are the quickest way to tell whether time is being spent drawing the live arena or rebuilding cached background and fog layers.

## Frontend Quality Artifacts
Run the frontend smoke and runtime-monitor checks through the backend wrapper:

```powershell
cd server
./scripts/quality.ps1 frontend
./scripts/quality.ps1 frontend-report
```

That path now writes:
- `server/target/reports/frontend/runtime_monitors.json`
- `server/target/reports/frontend/summary.json`
- `server/target/reports/frontend/index.html`

`runtime_monitors.json` is the structured runtime artifact for LLM troubleshooting.
It includes reference-scenario summaries for:
- built-in Godot monitors
- custom `Rarena/*` monitors
- pre-cleanup and post-cleanup engine snapshots
- cache-rebuild timings for arena background and visibility layers

## Frontend Debug Handoff
When the browser client feels slow or desynced, collect both the live client text and the generated runtime artifact.

1. In the browser shell, open `Menu -> Diagnostics` and copy the full diagnostics text.
2. Run:

```powershell
cd server
./scripts/quality.ps1 frontend-report
```

3. Share:
- the copied browser diagnostics text
- `server/target/reports/frontend/runtime_monitors.json`
- `server/target/reports/frontend/summary.json`

If the live browser diagnostics ever show all-zero custom timing buckets again while the built-in
Godot monitors are non-zero, treat that as a diagnostics regression. The checked-in
`performance_monitor_checks.gd` reference scenario now asserts that the custom `Rarena/*` timing
monitors produce sampled, non-zero values during the frontend quality run.

When the issue is render-specific, focus first on:
- `arena_draw_ms`
- `arena_draw_base_ms`
- `arena_visibility_ms`
- `arena_cache_sync_ms`
- `arena_cache_background_ms`
- `arena_cache_visibility_ms`

The checked-in positive frontend tests also cover the round-to-round skill bar bug where a later
pick from a different class could overwrite the wrong slot locally. The browser shell is expected
to track server-authored `slot` values, not just `tier` values.

For live deploy issues, also collect the host bundle from `python3 -m rusaren_ops collect-logs` as documented in `server/README.md`.

## Controls
- Open `Menu -> Controls` in the running shell to remap gameplay inputs.
- The current remappable actions are movement, primary attack, skill slots `1`-`5`, and self-cast.
- Rebinds accept keyboard keys plus the common mouse buttons and are saved in `user://controls.cfg`.
- Press `Escape` while the shell is waiting for a new binding to cancel capture.

## Manual Godot Profiling
For local editor runs, use Godot's built-in tooling directly:
- `Debugger -> Monitors` for built-in and custom `Rarena/*` counters
- `Debugger -> Profiler` for script and function timing
- `Debugger -> Video RAM` and the visual profiler for render-side investigation

The headless quality run is useful for repeatable baselines, but real draw-call and GPU investigation is still best done from the editor because some render counters are zero or unhelpful in headless mode.

## Match Objective Presentation
Generated match arenas can now carry a center-control objective from the backend.

The current client presentation contract is:
- `objective_tiles` from arena snapshots render as red floor in the cached arena background
- the match header shows both teams' accumulated center time against the shared `3:00` target
- the timers are authoritative and reset when the backend starts a new round
- both teams may keep accumulating time simultaneously if both occupy the center together

When debugging round flow, include these fields from `Menu -> Diagnostics`:
- `objective_tiles`
- `team_a_ms`
- `team_b_ms`
- `target_ms`
