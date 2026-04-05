# Deployment and Ops

## Goal
`0.8.0` keeps the same deployment shape, but the current hosted slice now includes authored YAML/ASCII content loading, a real WebRTC gameplay path, a minimally usable HUD, and backend-tested spell/status behavior.
For the first real hosted rollout, `https://pvpnowfast.com/` should open the game shell directly with no chooser page.

This is the current hosted topology:
- `https://pvpnowfast.com` -> Caddy reverse proxy with automatic TLS
- Caddy -> `rarena-server` container on the internal Docker network
- Prometheus -> scrapes `rarena-server:3000/metrics` on the internal Docker network
- `turn.pvpnowfast.com` -> `coturn` on the same operator-managed host

Current transport note:
- the public shell now uses `/session/bootstrap` plus websocket signaling at `/ws` and WebRTC data channels for live gameplay traffic
- `coturn` is provisioned because TURN relay fallback is required for reliable browser connectivity on real networks

Current hosting honesty:
- the current architecture is still single-app-host oriented because match sessions and player records are local to the running server
- for the first real hosted playtests, use one app host and treat it as a live-test/staging environment
- do not design around active-active multi-node gameplay yet until persistence and session ownership are reworked

## Checked-in stack
Files:
- `server/Dockerfile`
- `deploy/docker-compose.yml`
- `deploy/Caddyfile`
- `deploy/prometheus.yml`
- `deploy/coturn/turnserver.conf`
- `deploy/config.env.example`
- `deploy/docker-compose.override.example.yml`
- `deploy/README.md`
- `python3 -m rusaren_ops setup`
- `python3 -m rusaren_ops deploy`
- `python3 -m rusaren_ops smoke`
- `python3 -m rusaren_ops live-probe`
- `python3 -m rusaren_ops collect-logs`
- `server/scripts/docker-smoke.ps1`
- `python3 -m rusaren_ops export-web-client`

What each part does:
- `rarena-server`: serves `/`, `/session/bootstrap`, `/ws`, `/ws-dev`, `/healthz`, and `/metrics`
- `caddy`: terminates TLS and reverse-proxies the public site to the Rust server
- `prometheus`: scrapes backend metrics
- `coturn`: provides STUN/TURN service on the operator-managed domain
- `python3 -m rusaren_ops export-web-client`: Linux-native Godot Web export helper used by deploy when the hosted shell needs to be rebuilt on the server

## Prerequisites
- Docker and Docker Compose plugin on the host
- DNS records:
  - `pvpnowfast.com` -> public server IP
  - `turn.pvpnowfast.com` -> public server IP
- ports open:
  - `80/tcp`
  - `443/tcp`
  - `3478/tcp`
  - `3478/udp`
  - `49160-49200/udp`
- a host-side config directory, by default `~/rusaren-config/`, with:
  - `config.env`
  - optional `docker-compose.override.yml`

## First deploy
1. On the Linode host, run:
   - `sudo PUBLIC_HOST=<your domain> ACME_EMAIL=<your email> python3 -m rusaren_ops setup`
2. For later updates from the repo root on the host, run:
   - `sudo python3 -m rusaren_ops deploy`
3. Verify:
   - `https://pvpnowfast.com/`
   - `https://pvpnowfast.com/healthz`
   - `https://pvpnowfast.com/adminz`
   - Prometheus locally on the bind from `PROMETHEUS_BIND`
   - the root route serves the Godot shell directly

## Recommended Linode targets

Live-test / staging target:
- one shared or dedicated Ubuntu 24.04 LTS Linode for:
  - Caddy
  - `rarena-server`
  - Prometheus
  - the exported Godot web bundle, built on-host by default during deploy
- one TURN host:
  - either the same Linode for a cheap first live test
  - or a second small Linode for cleaner isolation

Practical minimum for the first live internet test:
- app host: `Linode 4 GB` or better
- TURN host: `Linode 2 GB` or better if separated
- backups enabled
- Cloud Firewall enabled

Production-quality target for the current 1.0 architecture:
- app host: `Linode 8 GB` or better
- separate TURN host
- operator-owned domain, TLS, backups, monitoring, and routine deploy verification

Reason for the single-app-host recommendation:
- the repo still uses local player-record persistence and single-process match ownership
- moving to multiple active gameplay hosts before reworking persistence/session ownership would add operational complexity that the codebase does not support yet

For exact Linode bring-up steps, see `17_linode_deploy.md`.

## Local smoke before host deploy
From the repo root, run:
- `./server/scripts/docker-smoke.ps1`

That smoke path:
- validates `deploy/docker-compose.yml`
- builds the current `server/Dockerfile`
- runs the game server image locally
- mounts a temporary placeholder web bundle
- probes `/`, `/healthz`, and `/metrics`

## Restart policy and persistence
- all services use `restart: unless-stopped`
- player records persist in the `rarena_data` Docker volume
- Prometheus data persists in the `prometheus_data` Docker volume
- Caddy certificate state persists in `caddy_data`

## Observability
The Rust server now exposes:
- `/healthz`
- `/metrics`

Current Prometheus metrics include:
- low-cardinality HTTP request counts by route label
- websocket upgrade attempts
- websocket session counts
- ingress packet accepted/rejected counts
- tick duration last/max
- server uptime
- build info

Current logs:
- `RARENA_LOG_FORMAT=json` is the recommended hosted default
- `RUST_LOG` or `RARENA_RUST_LOG` controls verbosity

Current operator surface:
- `/adminz` is a private read-only dashboard protected by deploy-time basic auth
- `~/rusaren-config/config.env` now carries `RARENA_ADMIN_USERNAME` and `RARENA_ADMIN_PASSWORD`
- `python3 -m rusaren_ops smoke` checks that `/adminz` rejects anonymous access and renders with valid credentials when the admin surface is enabled

## Security posture for this milestone
- TLS is terminated by Caddy
- `/metrics` is scraped internally by Prometheus and is not proxied publicly by the checked-in `Caddyfile`
- `coturn` uses long-term credentials via a shared secret in this deployment milestone
- `rarena-server` now runs as a non-root user with a read-only root filesystem, `no-new-privileges`, and all Linux capabilities dropped in the checked-in compose path
- TURN secrets and public host values still need to be replaced with operator-generated production values and kept out of git
- admin credentials still need to be replaced with operator-generated production values and kept out of git
- Docker-published service ports should still be protected by Linode Cloud Firewall rules because Docker documents that published container ports bypass `ufw` filtering

## Hosted smoke probes
- `python3 -m rusaren_ops deploy` now waits for the backend container healthcheck before it runs hosted smoke probes
- the same deploy script now rebuilds the Godot web bundle on Linux by default unless `BUILD_WEB_CLIENT=0`
- the same deploy script then runs hosted smoke probes against `https://$PUBLIC_HOST` unless `RUN_PUBLIC_SMOKE=0`
- `python3 -m rusaren_ops smoke` validates `/`, `/healthz`, `/session/bootstrap`, and the authenticated `/adminz` HTML and JSON views
- `python3 -m rusaren_ops setup` installs `snapd`, `unzip`, and a compatible Godot snap by default so the host can build the web bundle without a Windows step
- `python3 -m rusaren_ops setup` installs a `rusaren-smoke.timer` systemd timer so the host keeps re-running the public probes after deploy
- `python3 -m rusaren_ops setup` also installs `rusaren-liveprobe.timer` so the real hosted transport probe keeps exercising the live mechanic surface on a schedule

## Current limitation
This deploy path is production-style and testable, but not yet the final game transport:
- WebRTC is now the intended gameplay transport, but the current replication path still relies on full snapshots plus event batches rather than the final delta stream
- a real hosted-domain test still depends on operator DNS, certificates, and secret material
