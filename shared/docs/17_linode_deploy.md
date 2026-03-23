# Linode Deploy Guide

Use this guide when you want the first real internet-reachable hosted test of the current stack.
It assumes the repo's current architecture honestly:
- one primary app host
- optional separate TURN host
- Docker Compose deploy
- exported Godot web bundle already built
- `pvpnowfast.com` should serve the game shell directly at `/`, not a chooser page

Official Linode references:
- Getting started: https://techdocs.akamai.com/cloud-computing/docs/getting-started
- Create a compute instance: https://techdocs.akamai.com/cloud-computing/docs/create-a-compute-instance
- Add an SSH key: https://techdocs.akamai.com/cloud-computing/docs/add-an-ssh-key-in-cloud-manager
- Create and manage domains: https://techdocs.akamai.com/cloud-computing/docs/create-a-domain-zone
- Cloud Firewall: https://techdocs.akamai.com/cloud-computing/docs/getting-started-with-cloud-firewall
- Secure a Linode: https://techdocs.akamai.com/cloud-computing/docs/set-up-and-secure

## Recommended targets

First real live test:
- App host: 1 x Ubuntu 24.04 LTS Linode, `Linode 4 GB` or better
- TURN host: reuse the app host first, or use a separate `Linode 2 GB` if you want cleaner isolation
- Region: pick the region closest to the first expected playtesters
- Enable backups

Current 1.0 production-quality target:
- App host: 1 x Ubuntu 24.04 LTS Linode, `Linode 8 GB` or better
- TURN host: separate small Linode
- Domain and DNS managed in the same operator account
- Cloud Firewall on every public Linode

Do not jump to multi-app-host gameplay yet:
- the current app keeps player records locally
- match ownership is still single-process
- active-active gameplay hosting would require persistence/session work that is not done yet

## What you need before the first live test

1. A domain you control.
2. An SSH key uploaded to Linode Cloud Manager.
3. A built Godot web export under `server/static/webclient/`.
4. Real values for:
   - `PUBLIC_HOST`
   - `ACME_EMAIL`
   - `TURN_PUBLIC_HOST`
   - `TURN_REALM`
   - `TURN_SHARED_SECRET`
   - `TURN_EXTERNAL_IP`
   - optional `RARENA_ADMIN_USERNAME`
   - optional `RARENA_ADMIN_PASSWORD`

## Fast path

From the repo root on the host:

```bash
sudo PUBLIC_HOST=pvpnowfast.com \
  ACME_EMAIL=ops@pvpnowfast.com \
  bash deploy/linode-setup.sh
```

That script now handles:
- Ubuntu package updates
- timezone and hostname
- limited admin user creation
- SSH hardening when key-based access is present
- unattended security upgrades
- `fail2ban`
- `ufw`
- Docker Engine install from Docker's apt repository
- Docker daemon settings for `buildkit`, `live-restore`, and rotating `local` logs
- `deploy/.env` creation
- a `rusaren-compose.service` systemd unit
- a `rusaren-smoke.timer` systemd timer
- first stack bring-up through `deploy/linode-deploy.sh`

For later code updates on the same host:

```bash
sudo bash deploy/linode-deploy.sh
```

## Step-by-step

### 1. Create the Linode hosts

In Cloud Manager:
1. Create the app Linode.
2. Choose `Ubuntu 24.04 LTS`.
3. Pick `Linode 4 GB` or better for the first live test.
4. Add your SSH key during creation.
5. Enable backups.
6. Repeat for a separate TURN host if you want TURN isolated from the app host.

### 2. Create DNS records

In Cloud Manager Domains:
1. Create or import the domain zone.
2. Add:
   - `A` record for `pvpnowfast.com` -> app host public IP
   - `A` record for `turn.pvpnowfast.com` -> TURN host public IP
3. If TURN is co-hosted on the app machine, both records can point to the same IP.

### 3. Apply Cloud Firewall rules

App host:
- allow `22/tcp` only from your admin IPs
- allow `80/tcp`
- allow `443/tcp`

TURN host:
- allow `22/tcp` only from your admin IPs
- allow `3478/tcp`
- allow `3478/udp`
- allow `49160-49200/udp`

If TURN is co-hosted on the app machine, apply the union of both rule sets to the app host.

Do not expose Prometheus publicly unless you intentionally want that.
The checked-in stack binds Prometheus locally by default.

### 4. Secure the host

`deploy/linode-setup.sh` now handles the host-side baseline:
- updates packages
- configures a limited admin user
- hardens SSH when key access is present
- enables unattended upgrades and `fail2ban`
- configures `ufw`

Keep Linode Cloud Firewall enabled as the edge control plane.
Docker's published container ports intentionally remain reachable, and Docker documents that published ports can bypass `ufw` filtering.

### 5. Install Docker and Compose plugin

`deploy/linode-setup.sh` installs Docker Engine from Docker's official apt repository, not the convenience script.

### 6. Copy the repo and the exported web bundle

The live deploy needs the generated web client files.
They are not tracked in git, so you must either:
- export locally and copy them to the host, or
- build them in CI and deploy the artifact

Local-first path:
1. On the dev machine, run:

```bash
bash server/scripts/export-web-client.sh --godot-bin godot4
```

2. Copy the repo to the host.
3. Make sure `server/static/webclient/` exists on the host after the copy.

Example copy command from a Unix-like shell:

```bash
rsync -avz --delete ./ user@app-host:/opt/rusaren/
```

### 7. Configure environment values

`deploy/linode-setup.sh` writes `deploy/.env`.
Set these variables before running it if you want to override the defaults:
- `PUBLIC_HOST=pvpnowfast.com`
- `ACME_EMAIL=<your real email>`
- `TURN_PUBLIC_HOST=turn.pvpnowfast.com`
- `TURN_REALM=pvpnowfast.com`
- `TURN_SHARED_SECRET=<long random secret>`
- `TURN_EXTERNAL_IP=<public IP of TURN host or app host>`
- `RARENA_RUST_LOG=info,axum=info,tower_http=info`
- `RARENA_ADMIN_USERNAME=<admin username>`
- `RARENA_ADMIN_PASSWORD=<admin password>`

If TURN is on a separate Linode, keep the same shared secret and public host aligned with that machine.
If the admin password is omitted, the setup script generates one and writes it to `deploy/.env`.

### 8. Build and start the stack

`deploy/linode-deploy.sh` validates the compose file, builds the image, starts the stack, waits for the backend container healthcheck, and then runs hosted smoke probes.

### 9. Verify the live test

Check:

```bash
docker compose --env-file deploy/.env -f deploy/docker-compose.yml ps
docker compose --env-file deploy/.env -f deploy/docker-compose.yml logs rarena-server --tail=200
docker compose --env-file deploy/.env -f deploy/docker-compose.yml logs caddy --tail=200
docker compose --env-file deploy/.env -f deploy/docker-compose.yml logs coturn --tail=200
```

Then test:
- `https://pvpnowfast.com/`
- `https://pvpnowfast.com/healthz`
- `https://pvpnowfast.com/adminz`
- browser connect flow through `/session/bootstrap` and `/ws`
- real match traffic over WebRTC
- confirm the root page is the game shell directly and there is no landing-page chooser

Also run:

```bash
bash deploy/host-smoke.sh --env-file deploy/.env
systemctl status rusaren-smoke.timer
```

### 10. If WebRTC fails on the live host

Check in order:
1. `turn.pvpnowfast.com` resolves publicly
2. firewall rules include `3478/tcp`, `3478/udp`, and the relay UDP range
3. `TURN_EXTERNAL_IP` is the true public IP of the TURN host
4. `TURN_SHARED_SECRET` matches between the Rust app and `coturn`
5. TLS and DNS for `pvpnowfast.com` are working
6. browser dev tools show ICE candidates and no obvious signaling errors

## What you do not need yet

You do not need a Linode NodeBalancer yet for the current codebase.
Until the app stops being single-host oriented, the simpler and more honest target is:
- one app host
- optional separate TURN host
- Cloud Firewall
- backups
- DNS + TLS
