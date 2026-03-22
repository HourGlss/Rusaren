# deploy

This directory contains production deployment assets for the hosted stack, including Compose, reverse proxying, monitoring, TURN, and Linode bootstrap scripts.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `coturn/`: TURN server configuration shared by the hosted deployment path.
- `.env.example`: Sample deploy environment file with the public host, TLS contact, logging, and TURN settings expected by the stack.
- `Caddyfile`: Caddy reverse-proxy and automatic TLS configuration for the public site.
- `host-smoke.sh`: Post-deploy smoke script that probes the hosted root, health, bootstrap, and admin routes.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `docker-compose.yml`: Docker Compose stack definition for the hosted backend, proxy, metrics, and TURN services.
- `linode-deploy.sh`: Idempotent deployment script for code updates on an existing host. It validates the compose file, rebuilds the image, starts the stack, and waits for backend health.
- `linode-setup.sh`: Bootstrap script for a fresh Linode host. It hardens the OS, installs Docker, writes deploy configuration, and registers the systemd service.
- `prometheus.yml`: Prometheus scrape configuration for the backend metrics endpoint.
