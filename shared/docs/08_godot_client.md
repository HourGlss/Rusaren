# Godot Client (Thin Client)

Client responsibilities:
- Input capture (move, aim, cast)
- UI (central lobby, game lobby, team selection, skill selection, HUD, post-match stats screen, W-L-NC record display)
- Presentation (animations, particles, sound, camera)
- WebSocket signaling + WebRTC session setup
- Interpolation of server snapshots

Client non-responsibilities:
- No authoritative collision.
- No authoritative health/damage.
- No “I hit him” decisions.
- No local cooldown truth (can display predicted cooldown UI, but server decides).

Recommended client architecture:
- SignalingClient: HTTP/WebSocket setup and session negotiation
- MatchTransport: WebRTC data-channel transport for live match traffic
- NetAdapter: serialize/deserialize protocol messages
- ViewModel: transforms server snapshots into renderable state
- Scene graph: purely visual nodes bound to ViewModel data

Current implementation status:
- `client/godot/` now contains a thin Godot 4 shell for websocket signaling plus WebRTC gameplay transport.
- The current shell uses a binary `NetAdapter` for `ClientControlCommand` and `ServerControlEvent`.
- The current shell sends only the player name on connect; the backend assigns the runtime `player_id`.
- The current shell renders central lobby, game lobby, launch countdown, match skill-pick state, results, and central-lobby directory snapshots.
- The current shell now consumes authoritative full game-lobby snapshots, including late-joiner roster state and `W-L-NC`.
- The current shell now sends real binary `InputFrame` packets over the unordered WebRTC input data channel.
- The current shell now renders a simple top-down arena with a mostly empty floor, four central square pillars, and shrub collars.
- The current shell now consumes authoritative `ArenaStateSnapshot`, `ArenaDeltaSnapshot`, and `ArenaEffectBatch` events to draw players, aim lines, hp bars, mana bars, active statuses, cooldown state, projectile state, and short-lived combat effects.
- The current shell now has a minimally usable combat HUD: readable hp/mana above players, basic cooldown display, and simple spell/melee visuals driven by authoritative events.
- The current shell only enables legal skill picks for the local player: tier 1 for unstarted trees or the next tier in a tree already started this match.
- The current shell now exports to Web and defaults browser builds to the same-origin `/ws` endpoint.
- The current shell first fetches `/session/bootstrap`, then upgrades `/ws` with a short-lived one-time token.
- The Rust dev server can now host the exported shell directly at `/`.
- The documented production path now places Caddy in front of the Rust server for same-origin TLS while preserving the `/ws` websocket endpoint.
- The current runtime skills and prototype map load from `server/content/skills/*.yaml` and `server/content/maps/prototype_arena.txt`.
- Combat rendering is still placeholder-only.
- The older websocket gameplay adapter remains at `/ws-dev` for regression tests and debugging, but browser play is expected to use WebRTC.

Current backend limitations the shell must expose honestly:
- Combat content is still prototype-level even though the shell now shows a real arena and consumes authored YAML/ASCII content.
- The current combat slice is intentionally narrow: once combat starts, the shell supports `WASD` movement, mouse aim, left-click melee, authored slot skills on `1`-`5`, projectile combat, AoE skills, haste/silence/stun/chill/poison/hot statuses, and authoritative cooldown display, but not the final class set yet.
- The current delta packet is authoritative and works for live play, but it is not yet the final compressed/interpolated replication format.
- Vision is still intentionally minimal/stubbed compared with the rest of the combat slice.

Disconnect UX:
- If a match is aborted because a player disconnects, show: `<PLAYER_NAME> has disconnected. Game is over.`
- Show the match result as `No Contest` for every player.
- Show `W-L-NC` in both the central lobby and the game lobby.

Web export requirement:
- The primary client path must remain compatible with Godot web export.
- Do not assume native-only networking APIs or a native-only client stack.
- Do not make the main gameplay client depend on a desktop-only scripting/runtime path.
- Native Godot transport testing depends on the `webrtc-native` extension being available to the editor/runtime. If the local Godot install ships it under a folder like `Godot/webrtc/`, `server/scripts/export-web-client.ps1` can sync that bundle into the ignored local project path `client/godot/webrtc/` for native/headless validation; otherwise full networked transport testing remains browser-first.

Lag handling:
- Interpolate positions between snapshots
- No movement or cast prediction in v1.
- Cooldown UI may use cosmetic prediction, but server state remains authoritative.

Current local validation:
- Run the shell headlessly with `godot4 --headless --path client/godot --quit` to verify that the project boots.
- Run `godot4 --headless --path client/godot -s res://tests/protocol_checks.gd` to verify the Godot packet encoder plus arena event decoding.
- Run `godot4 --headless --path client/godot -s res://tests/web_export_checks.gd` to verify the same-origin browser websocket defaults, clickable lobby-directory formatting, and local arena combat-slot state handling.
- Run `server/scripts/export-web-client.ps1` to build the browser shell into `server/static/webclient/`.
- For hosted deployment, build the web export first, then package it with the Rust server image described in `15_deployment_ops.md`.
