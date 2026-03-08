# Buildability Assessment

## Current state

The repo is now buildable as a backend-first multiplayer prototype with a thin Godot shell, but it is not yet buildable as the intended browser-playable full game. The current code is real enough to exercise the lobby and match flow, but not yet the final transport or presentation stack.

## What is buildable now

Buildable today:
- the `server/` Cargo workspace
- the websocket dev adapter and binary control protocol
- the backend app layer, lobby flow, match flow, and fake-client tests
- the thin Godot shell under `client/godot`
- CI and local quality commands

Not buildable yet:
- the final WebRTC gameplay transport
- a server-driven central-lobby browser
- full lobby snapshots for late joiners
- full combat rendering and snapshot-driven gameplay presentation in Godot
- authored content loading

## What is specified well enough to start coding

You can start implementation now for:
- frontend shell validation against the live backend
- backend-to-frontend protocol hardening
- server-side lobby snapshot events
- WebRTC adapter work
- snapshot replication and visual presentation

## Human decisions still required

There are no remaining architecture-blocking networking decisions in the current docs.

Still open, but no longer blocking:
- some individual class/content tuning details are still placeholders
- exact numeric quantization ranges may need adjustment once real maps and movement values exist

## Recommendation

The next implementation steps should be:
1. add backend events for lobby discovery and full lobby snapshots
2. keep the Godot shell synced to that richer control surface
3. add the real WebRTC transport adapter beside the websocket dev adapter
4. start snapshot replication and gameplay presentation
5. add content loading and validation once the network surface stops moving

The next decisions should come from implementation feedback, not more speculative architecture drafting.
