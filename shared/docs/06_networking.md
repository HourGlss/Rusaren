# Networking

## Transport
Browser support is a hard requirement, so v1 transport is:
- HTTPS + JSON for auth, service discovery, version checks, and other control-plane APIs
- WebSocket for signaling and session setup
- WebRTC DataChannel for live match traffic

Current hosted-dev and hosted-MVP note:
- The live playable shell now uses websocket signaling at `/ws` plus WebRTC data channels for gameplay traffic.
- The browser shell first fetches a short-lived one-time token from `/session/bootstrap`, then uses that token on the websocket upgrade to `/ws` or `/ws-dev`.
- The older raw websocket dev adapter remains available at `/ws-dev` for fallback tests and transport regression coverage.
- The deploy stack already provisions the same-origin public host and a self-hosted `coturn` service for the WebRTC gameplay path.

Do not use Godot's high-level multiplayer protocol as the wire format for the Rust server.

## Transport structure
- Keep one gameplay packet format shared across native and web clients.
- Use WebSocket only for signaling and non-realtime control messages.
- Use WebRTC DataChannel for lobby updates, match traffic, snapshots, and gameplay events.
- Split live traffic into logical channels:
  - reliable ordered: lobby actions, ready changes, countdown, round transitions, match result
  - unordered unreliable with app-level sequence numbers: player inputs
  - unordered unreliable with app-level sequence numbers: snapshots and transient combat events

Concrete v1 channel options in Godot/WebRTC:
- `control` channel: ordered + reliable
- `input` channel: `ordered = false`, `maxRetransmits = 0`
- `snapshot` channel: `ordered = false`, `maxRetransmits = 0`

Note:
- WebRTC does not provide a native "sequenced but not reliable" mode.
- V1 implements "sequenced" behavior at the application layer by attaching a monotonically increasing sequence number and dropping any packet older than the newest one already processed on that channel.

## Encoding
- Use JSON for HTTP and signaling payloads.
- Use a custom binary packet format for live gameplay messages.
- Do not use Godot Variant serialization for untrusted network traffic.

## Messages (high level)
Client -> Server (commands / intents):
- CreateGameLobby, JoinGameLobby, LeaveGameLobby
- SelectTeam(side), ReadyToggle
- SelectSkill(tree, tier)
- InputTick(tick, move_vector, aim, buttons)
- CastIntent(ability_id, aim_context)
- QuitToCentralLobby

Server -> Client (authoritative):
- CentralLobbySnapshot / CentralLobbyDelta
- GameLobbySnapshot / GameLobbyDelta
- LaunchCountdownStarted
- MatchStarted
- ArenaStateSnapshot / ArenaDeltaSnapshot / ArenaEffectBatch
- RoundStateChanged
- player runtime state including hp, mana, cooldowns, active statuses, team, defeat state, and projectile ownership/state
- MatchAborted(player_name, reason)
- MatchStatistics
- Events (cast started, interrupted, damage numbers, etc.)

Lobby-oriented snapshots should include each visible player's `W-L-NC` record.

## Snapshot strategy
Current working implementation:
- Send a full `ArenaStateSnapshot` when combat starts, when phase transitions happen, and when the client needs a clean authoritative reset of the arena state.
- Send `ArenaDeltaSnapshot` packets during live combat frames and on aim changes.
- Include only viewer-allowed terrain knowledge in both full and delta snapshots.
- The backend computes visible and explored tiles, filters terrain/obstacle entries against that mask, and never relies on the Godot client to decide what terrain is visible.
- Send short-lived combat visuals separately through `ArenaEffectBatch`.

Current note:
- The live delta packet is authoritative and working, but it is still a simple dynamic-state packet, not a final baseline-referenced compression format.
- The client remains presentation-only and does not own authority.

## ICE, STUN, and TURN
Use all three concepts together:
- ICE is the overall WebRTC connectivity process that tries candidate pairs and picks a working path.
- STUN helps a peer discover a server-reflexive public address so a direct path can work when the network allows it.
- TURN relays traffic through a relay server when direct connectivity fails.

Concrete v1 deployment choice:
- Self-host `coturn`.
- Run it under the same operator as the game service, preferably on `turn.<domain>`.
- Configure both STUN and TURN entries in the client's `iceServers`.
- Prefer direct ICE candidate pairs when they succeed.
- Fall back to TURN relay when the browser is behind a restrictive NAT, firewall, school network, hotel network, or similar environment.

Recommended `iceServers` shape:
- `stun:turn.<domain>:3478`
- `turn:turn.<domain>:3478?transport=udp`
- `turns:turn.<domain>:5349?transport=tcp`

Operational guidance:
- Use short-lived TURN credentials issued by the HTTPS/WebSocket auth layer, not a static shared credential embedded in the client.
- If locked-down enterprise networks matter a lot, also consider offering TURN-over-TLS on `443` in addition to `5349`.
- TURN relay is the compatibility fallback, not the preferred steady-state path, because it adds relay bandwidth cost and some latency.
- The current signaling path already returns ephemeral TURN credentials and ICE server configuration to the client hello message on `/ws`.

## Versioning
- Match traffic has an exact `protocol_version`.
- Server rejects mismatched clients with a clear error and requires a reload/update.
- Content definitions carry a separate hash/version so UI data matches server behavior.
- Keep protocol compatibility rules strict in v1. Exact match is simpler than partial compatibility for realtime simulation.

## Browser-specific concerns
- WebRTC requires signaling and ICE negotiation before match traffic can flow.
- The authoritative server and web client are expected to be hosted by the same operator/domain.
- v1 uses self-hosted `coturn` for both STUN and TURN.
- Expect browser refreshes, tab closes, and transient disconnects to happen often enough that disconnect handling must be part of the protocol design.
- There is no reconnect-to-match flow in v1. A browser-side disconnect after launch countdown starts ends the current match.

## Session setup
- Player authenticates over HTTPS.
- Server issues a short-lived session bootstrap token for the websocket upgrade plus short-lived TURN credentials for relay use.
- Client fetches `/session/bootstrap`, upgrades `/ws?token=<token>`, then uses websocket signaling to establish the WebRTC session for live authoritative state, including lobby and match traffic.

## Security sanity
- Rate-limit inputs.
- Reject impossible command sequences (casting while stunned, selecting tier 4 without tier 3, etc.).
- Validate lobby membership, team membership, and countdown state on every control command.
- Server owns truth; client “requests”.

## Binary packet format
Use one binary framing format across all live gameplay channels.

Common packet header, little-endian, fixed 16 bytes:
```text
u16 magic        = 0x5241   // "RA"
u8  version      = 2
u8  channel_id   // 0=control, 1=input, 2=snapshot
u8  packet_kind
u8  flags
u16 payload_len
u32 seq          // monotonically increasing per sender per channel
u32 sim_tick     // authoritative tick when relevant, else 0
```

Header rules:
- `magic` + `version` reject garbage and stale clients early.
- `seq` is required on every packet even on reliable channels. On reliable channels it is mainly for tracing/debugging; on unordered unreliable channels it is used to drop stale packets.
- `payload_len` is the payload size after the 16-byte header.
- Use little-endian integers everywhere in v1 for implementation simplicity in Rust and Godot.

Packet kinds by channel:
- `control`: lobby snapshot, lobby delta, countdown state, round state, match result, statistics, abort notice
- `input`: input frame
- `snapshot`: full snapshot, delta snapshot, event batch

## Input packet payload
`input` packets carry one current input frame:
```text
u32 client_input_tick
i16 move_x_q
i16 move_y_q
i16 aim_x_q
i16 aim_y_q
u16 buttons
u16 ability_or_context
```

Rules:
- Server keeps only the newest valid input packet per player.
- Lost input packets are not retransmitted.

## Snapshot packet bodies
Current full snapshot body:
```text
u8  event_kind = 19
u8  phase
opt<u8> phase_seconds_remaining
u16 arena_width
u16 arena_height
u16 tile_units
blob visible_tiles
blob explored_tiles
u16 obstacle_count
... obstacle entries ...
u16 player_count
... player snapshot entries ...
u16 projectile_count
... projectile entries ...
```

Current delta snapshot body:
```text
u8  event_kind = 20
u8  phase
opt<u8> phase_seconds_remaining
u16 tile_units
blob visible_tiles
blob explored_tiles
u16 obstacle_count
... obstacle entries ...
u16 player_count
... player snapshot entries ...
u16 projectile_count
... projectile entries ...
```

Current effect batch body:
```text
u8  event_kind = 21
u8  effect_count
... effect entries ...
```

Current player snapshot payload includes:
- `player_id`
- `player_name`
- `team`
- `x`, `y`, `aim_x`, `aim_y`
- `hp`, `max_hp`
- `mana`, `max_mana`
- melee and slot cooldown remaining/total arrays
- `alive`
- `unlocked_skill_slots`
- `active_statuses`

Current `Connected` event payload also includes:
- authoritative player record
- the authored skill catalog used by the client to label skill-pick buttons and drive UI ordering

Current active status payload includes:
- kind (`Poison`, `Hot`, `Chill`, `Root`, `Silence`, `Stun`)
- `source_player_id`
- `stacks`
- `remaining_ms`

## Design notes
- The current implementation keeps the packet format simple and inspectable first: explicit full snapshots, explicit dynamic-state deltas, and separate effect batches.
- It deliberately does NOT copy Gaffer's UDP ack-bit packet header, because WebRTC DataChannel already provides message framing and configurable retransmission behavior underneath.
- More aggressive snapshot compression can happen later without discarding the current transport split or the exposed authoritative runtime state.
