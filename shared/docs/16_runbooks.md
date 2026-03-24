# Runbooks

## Deploy or update the hosted stack
1. On Linux hosts, let `deploy/deploy.sh` rebuild the Godot web client by default; if you want to do it manually first, run `bash server/scripts/export-web-client.sh`.
2. Update `~/rusaren-config/config.env`.
3. Run:
   - `sudo bash deploy/deploy.sh`
4. Check:
   - `https://domain.com/`
   - `https://domain.com/healthz`
   - `https://domain.com/adminz`
   - Prometheus scrape status
   - `bash deploy/host-smoke.sh --env-file ~/rusaren-config/config.env`

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
   - `docker compose --env-file ~/rusaren-config/config.env -f deploy/docker-compose.yml up -d`
4. Re-check `https://domain.com/healthz` and Prometheus target health.

## Root page shows the “web client is not built yet” placeholder
This means the backend started, but the web export bundle is missing from `server/static/webclient/`.

Fix:
1. Run `bash server/scripts/export-web-client.sh`.
2. Rerun `sudo bash deploy/deploy.sh`.

If the host should not build the bundle automatically, check that `BUILD_WEB_CLIENT` was not set to `0`.

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
1. `RARENA_ADMIN_USERNAME` and `RARENA_ADMIN_PASSWORD` exist in `~/rusaren-config/config.env`
2. `docker compose logs rarena-server --tail=200`
3. `bash deploy/host-smoke.sh --env-file ~/rusaren-config/config.env`
4. that you are using the expected credentials and did not leave the example password in place

## Live disconnects need a compact diagnostic bundle
Run:
1. `bash deploy/useful_log_collect.sh --output /tmp/rusaren-diagnostics.txt`
2. paste `/tmp/rusaren-diagnostics.txt`

The collector summarizes:
- compose state
- public root, health, bootstrap, and admin checks
- recent `/adminz` diagnostics
- filtered backend, proxy, and TURN logs for websocket and WebRTC failures

## Logs are too noisy or too quiet
Set `RARENA_RUST_LOG` in `~/rusaren-config/config.env`.

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
