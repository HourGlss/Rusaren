# Godot Shell

This is the `0.5.0` browser-safe Godot shell for the current Rust backend.

What it does:
- connects to the websocket dev adapter at `ws://127.0.0.1:3000/ws`
- defaults browser exports to the same-origin `/ws` endpoint automatically
- sends real binary control packets
- sends real binary combat input frames during the current placeholder combat slice
- decodes real binary server control events
- renders central-lobby, game-lobby, countdown, match, and results screens
- consumes authoritative lobby-directory and game-lobby snapshots
- lets players click an open lobby directly from the central directory

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
