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
