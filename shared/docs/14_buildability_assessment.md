# Buildability Assessment

## Current state

The repo is now buildable as a backend-first multiplayer prototype with a thin Godot shell, same-origin web hosting, observability endpoints, and a documented production-style deploy path. The current browser shell completes its live gameplay path through websocket signaling plus WebRTC data channels, the shell has a real Godot Web export path, and the repo includes a reverse-proxy/TLS/container stack for hosted operation. It is still not buildable as the intended final browser-playable game because broader content breadth, richer presentation polish, live hosted validation, and the external-client API documentation are not done yet.

## What is buildable now

Buildable today:
- the `server/` Cargo workspace
- the WebRTC gameplay transport, websocket signaling path, and the raw websocket dev adapter fallback
- the backend app layer, lobby flow, match flow, persistent `W-L-NC`, fake-client tests, live websocket integration tests, and Rust-side WebRTC integration tests
- the thin Godot shell under `client/godot`, including manual placeholder combat input over the live browser WebRTC transport
- authoritative full and delta gameplay snapshots that carry match phase, hp, mana, cooldowns, active statuses, projectile state, per-player visible/explored fog masks, and only the terrain knowledge the viewing player has earned
- runtime content loading from `server/content/skills/*.yaml`, `server/content/maps/prototype_arena.txt`, and `server/content/mechanics/registry.yaml`
- runtime validation that rejects malformed YAML skill shapes, duplicate authored ids, and malformed ASCII maps before boot
- backend gameplay tests that directly exercise every currently shipped melee and authored slot skill for hit/miss/range/cooldown/status behavior
- explicit abuse regressions for stale input ticks, locked-slot cast cheating, illegal movement components, and movement-spam distance caps
- soak/load regression coverage for repeated match sessions and parallel lobby churn
- hot-path Criterion benchmark targets for simulation ticks and snapshot packet codec work
- a Godot Web export pipeline plus CI smoke checks
- same-origin static hosting of the exported web shell from `dedicated_server`
- structured logs, `/healthz`, `/metrics`, and Prometheus-friendly observability on the Rust server
- a documented deploy stack with Caddy, Prometheus, and `coturn`
- CI and local quality commands

Not buildable yet:
- the final presentation bar for 1.0 combat readability and polish
- a broader final class/spell content set beyond the current shipped 20-skill runtime slice
- rustdoc/API guidance that is sufficient for an external client or bot to play through the protocol without Godot
- a proven hosted-domain live deployment on operator infrastructure such as Linode
- a completed 0.9 mutation-testing review cycle over the core runtime files

Current class-growth note:
- authored skills are already sourced from runtime YAML files
- the client skill picker already builds from backend-authored catalog data instead of local hardcoded button names
- class names now flow through the protocol and Godot shell from backend-authored catalog data instead of a fixed four-class wire enum
- implemented and planned mechanic families are now tracked in `server/content/mechanics/registry.yaml`
- implemented mechanic validation rules now also live in that registry, so most field-shape changes no longer require editing `game_content` parser match arms
- the main remaining growth bottleneck is only backend runtime execution when a class introduces a genuinely new mechanic family

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
3. continue gameplay presentation and HUD work on top of the working snapshot/delta path, especially readability under fog-of-war
4. use the checked-in hosting stack plus the Linode deployment guide to perform the first real hosted-domain rollout for `pvpnowfast.com` once operator secrets and DNS are available

The next decisions should come from implementation feedback, not more speculative architecture drafting.
