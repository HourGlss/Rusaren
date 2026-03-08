# Godot Shell

This is the first browser-safe Godot shell for the current Rust backend.

What it does:
- connects to the websocket dev adapter at `ws://127.0.0.1:3000/ws`
- sends real binary control packets
- decodes real binary server control events
- renders central-lobby, game-lobby, countdown, match, and results screens

What it does not do yet:
- WebRTC gameplay transport
- movement/combat rendering
- interpolation
- lobby discovery from the backend
- full lobby snapshots for players who join an already-populated lobby

Current shell limitation:
- joining a lobby requires a manual lobby ID
- roster state is reconstructed from live events, so late joiners do not get a complete lobby picture yet

Run flow:
1. start the Rust backend with `cd server && rustup run stable cargo run -p dedicated_server --quiet`
2. open this project in Godot 4
3. run `res://scenes/main.tscn`
4. connect, create or join a lobby, pick teams, ready up, and drive the backend flow
