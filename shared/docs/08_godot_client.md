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
- `client/godot/` now contains a thin Godot 4 shell for the websocket dev adapter.
- The current shell uses a binary `NetAdapter` for `ClientControlCommand` and `ServerControlEvent`.
- The current shell renders central lobby, game lobby, launch countdown, match skill-pick state, results, and central-lobby directory snapshots.
- The current shell now consumes authoritative full game-lobby snapshots, including late-joiner roster state and `W-L-NC`.
- The current shell now sends real binary `InputFrame` packets for a placeholder combat action over the websocket dev adapter.
- Combat rendering is still placeholder-only.
- The current shell is intentionally websocket-first while the WebRTC transport stays in planning.

Current backend limitations the shell must expose honestly:
- Joining a lobby is still manual by `lobby_id`, even though the server now publishes a central-lobby directory snapshot.
- Combat rendering is still placeholder-only; the shell shows authoritative state changes, not final gameplay presentation.
- The current combat slice is intentionally narrow: once combat starts, the shell uses a placeholder primary-attack control to drive the backend through real rounds and match resolution.

Disconnect UX:
- If a match is aborted because a player disconnects, show: `<PLAYER_NAME> has disconnected. Game is over.`
- Show the match result as `No Contest` for every player.
- Show `W-L-NC` in both the central lobby and the game lobby.

Web export requirement:
- The primary client path must remain compatible with Godot web export.
- Do not assume native-only networking APIs or a native-only client stack.
- Do not make the main gameplay client depend on a desktop-only scripting/runtime path.

Lag handling:
- Interpolate positions between snapshots
- No movement or cast prediction in v1.
- Cooldown UI may use cosmetic prediction, but server state remains authoritative.

Current local validation:
- Run the shell headlessly with `godot4 --headless --path client/godot --quit` to verify that the project boots.
- Run `godot4 --headless --path client/godot -s res://tests/protocol_checks.gd` to verify the Godot packet encoder's positive and negative `InputFrame` cases.
