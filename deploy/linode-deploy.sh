#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
COMPOSE_FILE="${REPO_ROOT}/deploy/docker-compose.yml"
ENV_FILE="${REPO_ROOT}/deploy/.env"
SMOKE_SCRIPT="${REPO_ROOT}/deploy/host-smoke.sh"

log() {
    printf '[linode-deploy] %s\n' "$*"
}

fatal() {
    printf '[linode-deploy] ERROR: %s\n' "$*" >&2
    exit 1
}

require_file() {
    local path="$1"
    [[ -f "${path}" ]] || fatal "missing required file: ${path}"
}

ensure_static_root() {
    mkdir -p "${REPO_ROOT}/server/static/webclient"
    if [[ ! -f "${REPO_ROOT}/server/static/webclient/index.html" ]]; then
        log "no exported web bundle detected at server/static/webclient; the backend will still start and serve the placeholder root page"
    fi
}

wait_for_healthz() {
    local attempts=60
    local delay_seconds=2

    for ((attempt = 1; attempt <= attempts; attempt += 1)); do
        if curl --fail --silent --show-error http://127.0.0.1:3000/healthz >/dev/null 2>&1; then
            log "backend health check passed"
            return 0
        fi
        sleep "${delay_seconds}"
    done

    fatal "backend did not become healthy at http://127.0.0.1:3000/healthz"
}

main() {
    require_file "${COMPOSE_FILE}"
    require_file "${SMOKE_SCRIPT}"
    if [[ ! -f "${ENV_FILE}" ]]; then
        require_file "${REPO_ROOT}/deploy/.env.example"
        cp "${REPO_ROOT}/deploy/.env.example" "${ENV_FILE}"
        fatal "created ${ENV_FILE}; set real values first, then rerun this script"
    fi

    ensure_static_root

    log "validating compose configuration"
    docker compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" config -q

    log "building and starting the production stack"
    docker compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" build --pull
    docker compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" up -d --remove-orphans

    wait_for_healthz

    log "running local backend smoke probes"
    bash "${SMOKE_SCRIPT}" --env-file "${ENV_FILE}" --origin "http://127.0.0.1:3000"

    if [[ "${RUN_PUBLIC_SMOKE:-1}" == "1" ]]; then
        log "running hosted smoke probes"
        bash "${SMOKE_SCRIPT}" --env-file "${ENV_FILE}"
    else
        log "skipping hosted smoke probes because RUN_PUBLIC_SMOKE=${RUN_PUBLIC_SMOKE:-1}"
    fi

    log "current service status"
    docker compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" ps
}

main "$@"
