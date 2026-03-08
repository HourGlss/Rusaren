# Godot Shell

This is the `0.6.0` browser-safe Godot shell for the current Rust backend.

What it does:
- connects to the websocket dev adapter at `ws://127.0.0.1:3000/ws`
- defaults browser exports to the same-origin `/ws` endpoint automatically
- lets the server assign the runtime player ID after connect instead of exposing a player-id field in the UI
- sends real binary control packets
- sends real binary combat input frames during the current placeholder combat slice
- decodes real binary server control events
- renders central-lobby, game-lobby, countdown, match, and results screens
- consumes authoritative lobby-directory and game-lobby snapshots
- lets players click an open lobby directly from the central directory
- can be hosted behind the documented Caddy reverse-proxy path from `deploy/`

What it does not do yet:
- WebRTC gameplay transport
- final movement/combat rendering
- interpolation

Current shell limitation:
- the combat loop is still placeholder-only, with a single primary attack button standing in for the final gameplay input/presentation layer

Run flow:
1. start the Rust backend with `cd server && rustup run stable cargo run -p dedicated_server --quiet`
2. optionally validate the packet encoder with `godot4 --headless --path client/godot -s res://tests/protocol_checks.gd`
3. optionally validate the web-export defaults with `godot4 --headless --path client/godot -s res://tests/web_export_checks.gd`
4. export the web shell with `powershell -NoProfile -ExecutionPolicy Bypass -File server/scripts/export-web-client.ps1 -InstallTemplates`
5. open `http://127.0.0.1:3000/` in a browser, or run `res://scenes/main.tscn` in Godot 4
6. connect, create or join a lobby, pick teams, ready up, choose skills, and press `Primary Attack` during combat to drive the current backend slice end to end
   The shell asks for a player name only; the backend assigns the runtime player ID.

Fastest local browser path:
1. run `powershell -NoProfile -ExecutionPolicy Bypass -File server/scripts/play-local.ps1 -GodotExecutable C:\Users\azbai\Documents\Rarena\Godot\Godot_v4.6.1-stable_win64_console.exe`
2. open `http://127.0.0.1:3000/` in two browser tabs
3. connect two players, receive server-assigned IDs, and play through the placeholder match loop
