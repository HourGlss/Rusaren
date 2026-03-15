# Godot Shell

This is the `0.8.0` browser-safe Godot shell for the current Rust backend.

What it does:
- fetches a short-lived bootstrap token from `http://127.0.0.1:3000/session/bootstrap`
- connects to the websocket signaling endpoint at `ws://127.0.0.1:3000/ws`
- defaults browser exports to the same-origin `/ws` endpoint automatically
- lets the server assign the runtime player ID after connect instead of exposing a player-id field in the UI
- only enables legal skill buttons for the current round: tier 1 on unstarted trees or the next tier on started trees
- sends real binary control packets
- sends real binary combat input frames during the current prototype combat slice over the WebRTC input channel
- decodes real binary server control events
- renders central-lobby, game-lobby, countdown, match, and results screens
- renders a simple top-down arena with a mostly empty floor, four central pillars, and shrub collars
- renders authoritative player circles, names, hp bars, mana bars, active status labels, cooldown text, projectile state, short-lived skill/melee effects, and per-player fog-of-war
- consumes authoritative lobby-directory and game-lobby snapshots
- consumes authoritative full arena snapshots, delta arena snapshots, and arena effect batches
- lets players click an open lobby directly from the central directory
- collapses the setup chrome once a player joins a lobby so the shell can focus on lobby or match actions
- puts skill picking ahead of the arena view during the round-opening skill-pick window so the legal choices stay visible without scrolling
- can be hosted behind the documented Caddy reverse-proxy path from `deploy/`
- consumes a runtime arena and skill set authored under `server/content/`

What it does not do yet:
- polished movement/combat rendering
- interpolation
- native desktop WebRTC transport without the `webrtc-native` extension
- the final 1.0 API-doc-quality surface for non-Godot clients

Current shell limitation:
- the combat loop is still prototype-level, even though the current map and slot skills now load from authored YAML and ASCII content files and already support real melee/projectile/status interactions
- shrubs are traversable cover: they no longer block movement, but they do block sight through the authoritative fog-of-war
- the shell now has a usable HUD and clearer melee/beam/projectile visuals, but not final readability polish or final effects for every future spell
- disconnecting or transport failure now returns the shell to the central-state layout instead of leaving stale match UI on screen
- the current snapshot delta is a simple dynamic-state packet, not a final compressed rollback/interpolation format
- native/headless transport testing depends on the `webrtc-native` extension being available to the editor/runtime; if your local Godot install ships it under a folder like `Godot/webrtc/`, `server/scripts/export-web-client.ps1` now syncs that bundle into the ignored local project path `client/godot/webrtc/`
- browser play remains the primary supported networked path on this machine; the synced native extension is only for local editor/headless validation

Run flow:
1. start the Rust backend with `cd server && rustup run stable cargo run -p dedicated_server --quiet`
2. optionally validate the packet encoder with `godot4 --headless --path client/godot -s res://tests/protocol_checks.gd`
3. optionally validate the web-export defaults with `godot4 --headless --path client/godot -s res://tests/web_export_checks.gd`
4. optionally validate the shell layout flow with `godot4 --headless --path client/godot -s res://tests/shell_layout_checks.gd`
5. export the web shell with `powershell -NoProfile -ExecutionPolicy Bypass -File server/scripts/export-web-client.ps1 -InstallTemplates`
   If a local `Godot/webrtc/` bundle exists, the export script syncs it into the ignored local project path `client/godot/webrtc/` first.
6. open `http://127.0.0.1:3000/` in a browser, or run `res://scenes/main.tscn` in Godot 4
   Browser play is the supported networked path on this machine.
7. connect, create or join a lobby, pick teams, ready up, choose skills, then use `WASD`, mouse aim, left click, and `1`-`5` during combat to drive the current backend slice end to end
   The shell asks for a player name only; the backend assigns the runtime player ID.
   Cooldowns, mana, hp, and active statuses shown in the HUD are driven by authoritative server snapshots.

Fast content iteration:
1. edit `server/content/skills/*.yaml` or `server/content/maps/prototype_arena.txt`
2. rerun `server/scripts/play-local.ps1` or restart `dedicated_server`
3. reload the browser shell

Fastest local browser path:
1. run `powershell -NoProfile -ExecutionPolicy Bypass -File server/scripts/play-local.ps1 -GodotExecutable <GODOT_EXECUTABLE>`
2. open `http://127.0.0.1:3000/` in two browser tabs
3. connect two players, receive server-assigned IDs, and play through the placeholder match loop
