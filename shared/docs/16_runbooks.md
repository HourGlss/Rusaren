# Runbooks

## Deploy or update the hosted stack
1. Export the Godot web client into `server/static/webclient/` with `bash server/scripts/export-web-client.sh`.
2. Update `deploy/.env`.
3. Run:
   - `docker compose --env-file deploy/.env -f deploy/docker-compose.yml build`
   - `docker compose --env-file deploy/.env -f deploy/docker-compose.yml up -d`
4. Check:
   - `https://domain.com/`
   - `https://domain.com/healthz`
   - `https://domain.com/adminz`
   - Prometheus scrape status
   - `bash deploy/host-smoke.sh --env-file deploy/.env`

Success criteria:
- root page serves the shell
- `/healthz` returns `ok`
- `/adminz` requires auth and renders for operators
- Prometheus sees the backend target as `UP`

If the deploy target is Linode, follow the host/DNS/firewall setup in `17_linode_deploy.md` first.

## Roll back the hosted stack
1. Restore the previous image or git revision.
2. Rebuild or pull the prior backend image.
3. Run:
   - `docker compose --env-file deploy/.env -f deploy/docker-compose.yml up -d`
4. Re-check `https://domain.com/healthz` and Prometheus target health.

## Root page shows the “web client is not built yet” placeholder
This means the backend started, but the web export bundle is missing from `server/static/webclient/`.

Fix:
1. Run `bash server/scripts/export-web-client.sh`.
2. Rebuild the backend image.
3. Restart `rarena-server`.

## `/healthz` fails
Check in order:
1. `docker compose ps`
2. `docker compose logs rarena-server --tail=200`
3. player-record volume path and filesystem permissions
4. whether the process exited because of invalid env values

## `/metrics` is empty or Prometheus target is down
Check:
1. `docker compose logs rarena-server --tail=200`
2. `docker compose logs prometheus --tail=200`
3. that `rarena-server` is healthy
4. that `prometheus.yml` still points to `rarena-server:3000`

## `/adminz` is unavailable
Check:
1. `RARENA_ADMIN_USERNAME` and `RARENA_ADMIN_PASSWORD` exist in `deploy/.env`
2. `docker compose logs rarena-server --tail=200`
3. `bash deploy/host-smoke.sh --env-file deploy/.env`
4. that you are using the expected credentials and did not leave the example password in place

## Logs are too noisy or too quiet
Set `RARENA_RUST_LOG` in `deploy/.env`.

Examples:
- `info,axum=info,tower_http=info`
- `warn,rarena_server=info,game_api=debug`

Restart the stack after changing it.

## coturn checklist
1. make sure `turn.domain.com` resolves publicly
2. confirm ports `3478/tcp`, `3478/udp`, and the relay UDP range are open
3. set a real `TURN_SHARED_SECRET`
4. set the real public `TURN_EXTERNAL_IP`

Current note:
- the browser gameplay path now depends on `/session/bootstrap`, `/ws` signaling, and the checked-in STUN/TURN configuration
- if browser sessions fail to connect, check the `coturn` logs and the ICE/TURN environment values first
