# Linode Deploy Guide

Use this guide when you want the first real internet-reachable hosted test of the current stack.
It assumes the repo's current architecture honestly:
- one primary app host
- optional separate TURN host
- Docker Compose deploy
- exported Godot web bundle already built

Official Linode references:
- Getting started: https://techdocs.akamai.com/cloud-computing/docs/getting-started
- Create a compute instance: https://techdocs.akamai.com/cloud-computing/docs/create-a-compute-instance
- Add an SSH key: https://techdocs.akamai.com/cloud-computing/docs/add-an-ssh-key-in-cloud-manager
- Create and manage domains: https://techdocs.akamai.com/cloud-computing/docs/create-a-domain-zone
- Cloud Firewall: https://techdocs.akamai.com/cloud-computing/docs/getting-started-with-cloud-firewall
- Secure a Linode: https://techdocs.akamai.com/cloud-computing/docs/set-up-and-secure

## Recommended targets

First real live test:
- App host: 1 x Ubuntu LTS Linode, `Linode 4 GB` or better
- TURN host: reuse the app host first, or use a separate `Linode 2 GB` if you want cleaner isolation
- Region: pick the region closest to the first expected playtesters
- Enable backups

Current 1.0 production-quality target:
- App host: 1 x Ubuntu LTS Linode, `Linode 8 GB` or better
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

## Step-by-step

### 1. Create the Linode hosts

In Cloud Manager:
1. Create the app Linode.
2. Choose Ubuntu LTS.
3. Pick `Linode 4 GB` or better for the first live test.
4. Add your SSH key during creation.
5. Enable backups.
6. Repeat for a separate TURN host if you want TURN isolated from the app host.

### 2. Create DNS records

In Cloud Manager Domains:
1. Create or import the domain zone.
2. Add:
   - `A` record for `domain.com` -> app host public IP
   - `A` record for `turn.domain.com` -> TURN host public IP
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

After SSH login:
1. update packages
2. create or confirm a non-root admin user if needed
3. confirm SSH key login works
4. follow Linode's secure-hosting guidance before opening the service broadly

Typical commands:

```bash
sudo apt-get update
sudo apt-get upgrade -y
sudo apt-get install -y ca-certificates curl git
```

### 5. Install Docker and Compose plugin

On the app host:

```bash
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker "$USER"
newgrp docker
docker version
docker compose version
```

Repeat on the TURN host only if you are deploying TURN there with Docker Compose too.

### 6. Copy the repo and the exported web bundle

The live deploy needs the generated web client files.
They are not tracked in git, so you must either:
- export locally and copy them to the host, or
- build them in CI and deploy the artifact

Local-first path:
1. On the dev machine, run:

```powershell
./server/scripts/export-web-client.ps1 -GodotExecutable <GODOT_EXECUTABLE> -InstallTemplates
```

2. Copy the repo to the host.
3. Make sure `server/static/webclient/` exists on the host after the copy.

Example copy command from a Unix-like shell:

```bash
rsync -avz --delete ./ user@app-host:/opt/rusaren/
```

### 7. Configure environment values

On the app host:

```bash
cd /opt/rusaren
cp deploy/.env.example deploy/.env
```

Edit `deploy/.env` and set real values:
- `PUBLIC_HOST=domain.com`
- `ACME_EMAIL=<your real email>`
- `TURN_PUBLIC_HOST=turn.domain.com`
- `TURN_REALM=domain.com`
- `TURN_SHARED_SECRET=<long random secret>`
- `TURN_EXTERNAL_IP=<public IP of TURN host or app host>`
- `RARENA_RUST_LOG=info,axum=info,tower_http=info`

If TURN is on a separate Linode, keep the same shared secret and public host aligned with that machine.

### 8. Build and start the stack

From the repo root on the app host:

```bash
docker compose --env-file deploy/.env -f deploy/docker-compose.yml build
docker compose --env-file deploy/.env -f deploy/docker-compose.yml up -d
```

### 9. Verify the live test

Check:

```bash
docker compose --env-file deploy/.env -f deploy/docker-compose.yml ps
docker compose --env-file deploy/.env -f deploy/docker-compose.yml logs rarena-server --tail=200
docker compose --env-file deploy/.env -f deploy/docker-compose.yml logs caddy --tail=200
docker compose --env-file deploy/.env -f deploy/docker-compose.yml logs coturn --tail=200
```

Then test:
- `https://domain.com/`
- `https://domain.com/healthz`
- browser connect flow through `/session/bootstrap` and `/ws`
- real match traffic over WebRTC

### 10. If WebRTC fails on the live host

Check in order:
1. `turn.domain.com` resolves publicly
2. firewall rules include `3478/tcp`, `3478/udp`, and the relay UDP range
3. `TURN_EXTERNAL_IP` is the true public IP of the TURN host
4. `TURN_SHARED_SECRET` matches between the Rust app and `coturn`
5. TLS and DNS for `domain.com` are working
6. browser dev tools show ICE candidates and no obvious signaling errors

## What you do not need yet

You do not need a Linode NodeBalancer yet for the current codebase.
Until the app stops being single-host oriented, the simpler and more honest target is:
- one app host
- optional separate TURN host
- Cloud Firewall
- backups
- DNS + TLS
