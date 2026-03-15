# Buildability Assessment

## Current state

The repo is now buildable as a backend-first multiplayer prototype with a thin Godot shell, same-origin web hosting, observability endpoints, and a documented production-style deploy path. The current browser shell completes its live gameplay path through websocket signaling plus WebRTC data channels, the shell has a real Godot Web export path, and the repo includes a reverse-proxy/TLS/container stack for hosted operation. It is still not buildable as the intended final browser-playable game because broader gameplay presentation, richer spell/content breadth, and the external-client API documentation are not done yet.

## What is buildable now

Buildable today:
- the `server/` Cargo workspace
- the WebRTC gameplay transport, websocket signaling path, and the raw websocket dev adapter fallback
- the backend app layer, lobby flow, match flow, persistent `W-L-NC`, fake-client tests, live websocket integration tests, and Rust-side WebRTC integration tests
- the thin Godot shell under `client/godot`, including manual placeholder combat input over the live browser WebRTC transport
- authoritative full and delta gameplay snapshots that carry match phase, hp, mana, cooldowns, active statuses, and projectile state
- runtime content loading from `server/content/skills/*.yaml` and `server/content/maps/prototype_arena.txt`
- a Godot Web export pipeline plus CI smoke checks
- same-origin static hosting of the exported web shell from `dedicated_server`
- structured logs, `/healthz`, `/metrics`, and Prometheus-friendly observability on the Rust server
- a documented deploy stack with Caddy, Prometheus, and `coturn`
- CI and local quality commands

Not buildable yet:
- full combat rendering and snapshot-driven gameplay presentation in Godot
- a broad final class/spell content set
- rustdoc/API guidance that is sufficient for an external client or bot to play through the protocol without Godot

## What is specified well enough to start coding

You can start implementation now for:
- backend-to-frontend protocol hardening
- gameplay-content expansion and correctness testing
- broader gameplay presentation on top of the working snapshot and delta path
- real hosted-domain bring-up using the checked-in deploy assets

## Human decisions still required

There are no remaining architecture-blocking networking decisions in the current docs.

Still open, but no longer blocking:
- some individual class/content tuning details are still placeholders
- exact numeric quantization ranges may need adjustment once real maps and movement values exist
- the checked-in deploy path is ready, but an actual live domain cutover still needs operator-owned DNS, certificates, and secret material

## Recommendation

The next implementation steps should be:
1. keep the Godot shell and backend packet surface synced while the current WebRTC path gets more real-world playtime
2. expand authored content and backend gameplay correctness coverage now that the runtime transport surface is in place
3. continue gameplay presentation and HUD work on top of the working snapshot/delta path
4. use the checked-in hosting stack to perform the real production-domain rollout once operator secrets and DNS are available

The next decisions should come from implementation feedback, not more speculative architecture drafting.
