# Networking

## Transport
Browser support is a hard requirement, so v1 transport is:
- HTTPS + JSON for auth, service discovery, version checks, and other control-plane APIs
- WebSocket for signaling and session setup
- WebRTC DataChannel for live match traffic

Current hosted-dev and hosted-MVP note:
- `0.6.0` still serves the live playable shell over the websocket dev adapter.
- The deploy stack already provisions the same-origin public host and a self-hosted `coturn` service so `0.7.0` can add the real WebRTC gameplay path without redesigning hosting.

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
- MatchSnapshot / MatchDelta
- RoundStateChanged
- EntitySnapshot (position, velocity, health, statuses)
- MatchAborted(player_name, reason)
- MatchStatistics
- Events (cast started, interrupted, damage numbers, etc.)

Lobby-oriented snapshots should include each visible player's `W-L-NC` record.

## Snapshot strategy
Start simple:
- Send a full snapshot baseline every 250 ms.
- Send smaller delta snapshots every simulation tick at 60 Hz.
- Delta snapshots reference a prior baseline sequence.
- If a baseline is missing or too old, resend a fresh full snapshot.

Client should interpolate between snapshots. No authority.

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
- The `0.6.0` deploy stack already includes `coturn`, but the current playable shell still uses the websocket dev adapter until the `0.7.0` WebRTC transport lands.

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
- Server issues a short-lived signed session token.
- Server also issues short-lived TURN credentials for that session.
- Client uses WebSocket signaling to establish the WebRTC session for live authoritative state, including lobby and match traffic.

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
u8  version      = 1
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

## Snapshot packet prefix
All snapshot packets begin with:
```text
u32 snapshot_seq
u32 baseline_seq   // 0xFFFFFFFF means full snapshot
u16 entity_op_count
u16 event_count
```

Interpretation:
- `full snapshot`: `baseline_seq = 0xFFFFFFFF`
- `delta snapshot`: `baseline_seq = last baseline snapshot sequence used for encoding`
- If client does not have that baseline, it discards the delta and waits for the next full snapshot.

## Entity delta layout
Entity ops are sorted by ascending `net_entity_id`.
Each entity op begins with:
```text
varuint entity_id_delta   // relative to previous entity id, first entry relative to 0
u8     entity_op          // 0=spawn_full, 1=patch, 2=despawn
```

For `spawn_full`:
```text
u8  entity_type
u16 owner_entity_id_or_ffff
u8  team_id
u16 field_mask
... absolute field values in field order ...
```

For `patch`:
```text
u16 field_mask
... changed field values in field order ...
```

For `despawn`:
- no additional payload

Patch `field_mask` bits:
- bit 0: `pos_x`
- bit 1: `pos_y`
- bit 2: `vel_x`
- bit 3: `vel_y`
- bit 4: `facing`
- bit 5: `hp`
- bit 6: `resource`
- bit 7: `life_flags`
- bit 8: `cast_block`
- bit 9: `status_delta_block`
- bit 10: `reveal_flags`
- bit 11: `aux_state`
- bits 12-15: reserved

Field encoding rules:
- `pos_*`, `vel_*`, and `facing` are quantized fixed-point values, not floats.
- In a `spawn_full`, those fields are encoded as absolute values.
- In a `patch`, those fields are encoded relative to the baseline snapshot where possible; if a value cannot be represented cleanly as a bounded delta, send `spawn_full` for that entity instead of a `patch`.

Status delta block:
```text
u8 status_op_count
repeat status_op_count times:
  u8  op_kind          // 0=apply, 1=refresh, 2=stack_set, 3=remove
  u16 status_id
  u16 source_entity_id
  u8  stacks
  u16 remaining_ms
```

This layout matches the current gameplay rules:
- status ownership is per source player
- multiple sources can coexist on the same target
- stack count and refresh are explicit protocol concepts

## Design notes
- This follows the same broad direction as Gaffer-style snapshot replication: full baselines, delta snapshots, sorted changed entities, and relative entity id encoding.
- It deliberately does NOT copy Gaffer's UDP ack-bit packet header, because WebRTC DataChannel already provides message framing and configurable retransmission behavior underneath.
- Keep the v1 packet format simple and inspectable first; if bandwidth becomes a real problem later, add bitpacking inside the field blocks instead of redesigning the protocol from scratch.
