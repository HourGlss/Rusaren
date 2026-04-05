# deploy

This directory contains production deployment assets for the hosted stack, including Compose, reverse proxying, monitoring, and TURN configuration.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `coturn/`: TURN server configuration shared by the hosted deployment path.
- `.env.example`: Legacy sample env file kept for compatibility with older local helpers.
- `Caddyfile`: Caddy reverse-proxy and automatic TLS configuration for the public site.
- `config.env.example`: Sample external runtime config file that `python3 -m rusaren_ops setup` copies to `~/rusaren-config/config.env` on Linux hosts.
- `docker-compose.override.example.yml`: Sample external Compose override file that `python3 -m rusaren_ops setup` copies to `~/rusaren-config/docker-compose.override.yml`.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `docker-compose.yml`: Docker Compose stack definition for the hosted backend, proxy, metrics, and TURN services.
- `prometheus.yml`: Prometheus scrape configuration for the backend metrics endpoint.

## CLI
The Linux operational entrypoints now live under the Python package CLI instead of one-file wrappers in this directory.

- `python3 -m rusaren_ops --help`: show the top-level operational menu.
- `python3 -m rusaren_ops setup`: bootstrap a fresh Linux host and create `~/rusaren-config/`.
- `python3 -m rusaren_ops deploy`: build, validate, and start the hosted stack.
- `python3 -m rusaren_ops smoke`: run the hosted root, health, bootstrap, and admin probes.
- `python3 -m rusaren_ops live-probe`: run the real hosted transport probe with diagnostics fallback.
- `python3 -m rusaren_ops collect-logs`: collect the compact host diagnostics bundle.
- `python3 -m rusaren_ops export-web-client`: build the Linux Godot web export into `server/static/webclient/`.

## Routine verification
- `python3 -m rusaren_ops smoke` validates the hosted root, `/healthz`, `/session/bootstrap`, and the authenticated `/adminz` HTML and JSON views.
- `python3 -m rusaren_ops live-probe` keeps the real hosted transport path exercised against the live mechanic surface and collects a diagnostics bundle on failure.
- `python3 -m rusaren_ops setup` installs both `rusaren-smoke.timer` and `rusaren-liveprobe.timer` so hosted verification keeps running after the initial deploy.
