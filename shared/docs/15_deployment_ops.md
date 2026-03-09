# Deployment and Ops

## Goal
`0.6.0` adds one documented production-style deploy path for the current same-origin hosted shell and Rust backend.

This is the current hosted topology:
- `https://domain.com` -> Caddy reverse proxy with automatic TLS
- Caddy -> `rarena-server` container on the internal Docker network
- Prometheus -> scrapes `rarena-server:3000/metrics` on the internal Docker network
- `turn.domain.com` -> `coturn` on the same operator-managed host

Current transport note:
- the public shell now uses `/session/bootstrap` plus websocket signaling at `/ws` and WebRTC data channels for live gameplay traffic
- `coturn` is provisioned because TURN relay fallback is required for reliable browser connectivity on real networks

## Checked-in stack
Files:
- `server/Dockerfile`
- `deploy/docker-compose.yml`
- `deploy/Caddyfile`
- `deploy/prometheus.yml`
- `deploy/coturn/turnserver.conf`
- `deploy/.env.example`
- `server/scripts/docker-smoke.ps1`

What each part does:
- `rarena-server`: serves `/`, `/session/bootstrap`, `/ws`, `/ws-dev`, `/healthz`, and `/metrics`
- `caddy`: terminates TLS and reverse-proxies the public site to the Rust server
- `prometheus`: scrapes backend metrics
- `coturn`: provides STUN/TURN service on the operator-managed domain

## Prerequisites
- Docker and Docker Compose plugin on the host
- DNS records:
  - `domain.com` -> public server IP
  - `turn.domain.com` -> public server IP
- ports open:
  - `80/tcp`
  - `443/tcp`
  - `3478/tcp`
  - `3478/udp`
  - `49160-49200/udp`
- a copy of `deploy/.env.example` saved as `deploy/.env` with real values

## First deploy
1. Build the current Godot web export into `server/static/webclient/`.
2. Copy `deploy/.env.example` to `deploy/.env`.
3. Set:
   - `PUBLIC_HOST`
   - `ACME_EMAIL`
   - `RARENA_RUST_LOG`
   - `TURN_PUBLIC_HOST`
   - `TURN_REALM`
   - `TURN_SHARED_SECRET`
   - `TURN_EXTERNAL_IP`
4. From the repo root, run:
   - `docker compose --env-file deploy/.env -f deploy/docker-compose.yml build`
   - `docker compose --env-file deploy/.env -f deploy/docker-compose.yml up -d`
5. Verify:
   - `https://domain.com/`
   - `https://domain.com/healthz`
   - Prometheus locally on the bind from `PROMETHEUS_BIND`

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

## Security posture for this milestone
- TLS is terminated by Caddy
- `/metrics` is scraped internally by Prometheus and is not proxied publicly by the checked-in `Caddyfile`
- `coturn` uses long-term credentials via a shared secret in this deployment milestone
- `rarena-server` now runs as a non-root user with a read-only root filesystem, `no-new-privileges`, and all Linux capabilities dropped in the checked-in compose path
- TURN secrets and public host values still need to be replaced with operator-generated production values and kept out of git

## Current limitation
This deploy path is production-style and testable, but not yet the final game transport:
- WebRTC is now the intended gameplay transport, but the current replication path still relies on full snapshots plus event batches rather than the final delta stream
- a real hosted-domain test still depends on operator DNS, certificates, and secret material
