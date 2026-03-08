# Buildability Assessment

## Current state

The repo is now buildable as a backend-first multiplayer prototype with a thin Godot shell, and the current websocket dev path can complete a full manual match loop. The shell now has a real Godot Web export path and same-origin static hosting through the Rust dev server. It is still not buildable as the intended final browser-playable game because the final WebRTC transport and full gameplay presentation stack are not done yet.

## What is buildable now

Buildable today:
- the `server/` Cargo workspace
- the websocket dev adapter and binary control protocol
- the backend app layer, lobby flow, match flow, persistent `W-L-NC`, fake-client tests, and live websocket integration tests
- the thin Godot shell under `client/godot`, including manual placeholder combat input over the live websocket adapter
- a Godot Web export pipeline plus CI smoke checks
- same-origin static hosting of the exported web shell from `dedicated_server`
- CI and local quality commands

Not buildable yet:
- the final WebRTC gameplay transport
- full combat rendering and snapshot-driven gameplay presentation in Godot
- authored content loading

## What is specified well enough to start coding

You can start implementation now for:
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
1. add the real WebRTC transport adapter beside the websocket dev adapter
2. keep the Godot shell synced to that richer transport surface
3. start snapshot replication, gameplay presentation, and then content loading once the network surface stops moving
4. deploy the same-origin hosted shell and backend on the production domain with TLS and TURN

The next decisions should come from implementation feedback, not more speculative architecture drafting.
