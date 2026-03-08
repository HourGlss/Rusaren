# Buildability Assessment

## Current state

The repo is now buildable as a backend-first multiplayer prototype with a thin Godot shell, and the current websocket dev path can complete a full manual match loop. It is still not buildable as the intended browser-playable full game because the final transport, web export workflow, and gameplay presentation stack are not done yet.

## What is buildable now

Buildable today:
- the `server/` Cargo workspace
- the websocket dev adapter and binary control protocol
- the backend app layer, lobby flow, match flow, persistent `W-L-NC`, fake-client tests, and live websocket integration tests
- the thin Godot shell under `client/godot`, including manual placeholder combat input over the live websocket adapter
- CI and local quality commands

Not buildable yet:
- the final WebRTC gameplay transport
- browser-hosted web export validation and deployment
- full combat rendering and snapshot-driven gameplay presentation in Godot
- authored content loading

## What is specified well enough to start coding

You can start implementation now for:
- browser export and hosting work for the current shell
- backend-to-frontend protocol hardening
- WebRTC adapter work
- snapshot replication and visual presentation

## Human decisions still required

There are no remaining architecture-blocking networking decisions in the current docs.

Still open, but no longer blocking:
- some individual class/content tuning details are still placeholders
- exact numeric quantization ranges may need adjustment once real maps and movement values exist

## Recommendation

The next implementation steps should be:
1. make the Godot shell a real browser-playable MVP through web export and hosted static delivery
2. add the real WebRTC transport adapter beside the websocket dev adapter
3. keep the Godot shell synced to that richer transport surface
4. start snapshot replication, gameplay presentation, and then content loading once the network surface stops moving

The next decisions should come from implementation feedback, not more speculative architecture drafting.
