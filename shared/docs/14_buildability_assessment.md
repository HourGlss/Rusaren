# Buildability Assessment

## Current state

The repo is now buildable as a backend-first multiplayer prototype with a thin Godot shell, same-origin web hosting, observability endpoints, and a documented production-style deploy path. The current websocket dev path can complete a full manual match loop, the shell has a real Godot Web export path, and the repo now includes a reverse-proxy/TLS/container stack for hosted operation. It is still not buildable as the intended final browser-playable game because the final WebRTC transport and full gameplay presentation stack are not done yet.

## What is buildable now

Buildable today:
- the `server/` Cargo workspace
- the websocket dev adapter and binary control protocol
- the backend app layer, lobby flow, match flow, persistent `W-L-NC`, fake-client tests, and live websocket integration tests
- the thin Godot shell under `client/godot`, including manual placeholder combat input over the live websocket adapter
- runtime content loading from `server/content/skills/*.yaml` and `server/content/maps/prototype_arena.txt`
- a Godot Web export pipeline plus CI smoke checks
- same-origin static hosting of the exported web shell from `dedicated_server`
- structured logs, `/healthz`, `/metrics`, and Prometheus-friendly observability on the Rust server
- a documented deploy stack with Caddy, Prometheus, and `coturn`
- CI and local quality commands

Not buildable yet:
- the final WebRTC gameplay transport
- full combat rendering and snapshot-driven gameplay presentation in Godot
- a broad final class/spell content set

## What is specified well enough to start coding

You can start implementation now for:
- backend-to-frontend protocol hardening
- WebRTC adapter work
- snapshot replication and visual presentation
- real hosted-domain bring-up using the checked-in deploy assets

## Human decisions still required

There are no remaining architecture-blocking networking decisions in the current docs.

Still open, but no longer blocking:
- some individual class/content tuning details are still placeholders
- exact numeric quantization ranges may need adjustment once real maps and movement values exist
- the checked-in deploy path is ready, but an actual live domain cutover still needs operator-owned DNS, certificates, and secret material

## Recommendation

The next implementation steps should be:
1. add the real WebRTC transport adapter beside the websocket dev adapter
2. keep the Godot shell synced to that richer transport surface
3. start snapshot replication, gameplay presentation, and then content loading once the network surface stops moving
4. use the checked-in hosting stack to perform the real production-domain rollout once operator secrets and DNS are available

The next decisions should come from implementation feedback, not more speculative architecture drafting.
