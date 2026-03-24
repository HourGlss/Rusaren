# deploy

This directory contains production deployment assets for the hosted stack, including Compose, reverse proxying, monitoring, TURN, and Linode bootstrap scripts.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `coturn/`: TURN server configuration shared by the hosted deployment path.
- `.env.example`: Legacy sample env file kept for compatibility with older local helpers.
- `Caddyfile`: Caddy reverse-proxy and automatic TLS configuration for the public site.
- `config.env.example`: Sample external runtime config file that `setup.sh` copies to `~/rusaren-config/config.env` on Linux hosts.
- `docker-compose.override.example.yml`: Sample external Compose override file that `setup.sh` copies to `~/rusaren-config/docker-compose.override.yml`.
- `host-smoke.sh`: Post-deploy smoke script that probes the hosted root, health, bootstrap, and admin routes.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `docker-compose.yml`: Docker Compose stack definition for the hosted backend, proxy, metrics, and TURN services.
- `deploy.sh`: Short wrapper around `linode-deploy.sh` for the normal Linux update/start path.
- `linode-deploy.sh`: Idempotent deployment script for code updates on an existing host. It reads `~/rusaren-config/config.env` by default, honors an external Compose override, rebuilds the Godot web client on Linux, and starts the stack.
- `linode-setup.sh`: Bootstrap script for a fresh Linode host. It hardens the OS, installs Docker, installs the Linux Godot export toolchain by default, writes external deploy configuration, and registers the systemd service.
- `prometheus.yml`: Prometheus scrape configuration for the backend metrics endpoint.
- `setup.sh`: Short wrapper around `linode-setup.sh` for the first Linux host bootstrap.
- `useful_log_collect.sh`: Compact host-side diagnostics collector that summarizes compose state, public probes, admin diagnostics, and filtered transport logs into one pasteable report.
