#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
COMPOSE_FILE="${REPO_ROOT}/deploy/docker-compose.yml"
ENV_FILE="${REPO_ROOT}/deploy/.env"
SMOKE_SCRIPT="${REPO_ROOT}/deploy/host-smoke.sh"
EXPORT_SCRIPT="${REPO_ROOT}/server/scripts/export-web-client.sh"

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
}

build_web_client_if_requested() {
    local mode="${BUILD_WEB_CLIENT:-1}"
    local index_path="${REPO_ROOT}/server/static/webclient/index.html"
    local should_build=0

    case "${mode}" in
        1|true|always)
            should_build=1
            ;;
        auto)
            if [[ ! -f "${index_path}" ]]; then
                should_build=1
            fi
            ;;
        0|false|never)
            if [[ ! -f "${index_path}" ]]; then
                log "no exported web bundle detected at server/static/webclient; deploy will continue and the backend will serve the placeholder root page"
            fi
            return
            ;;
        *)
            fatal "invalid BUILD_WEB_CLIENT value: ${mode}"
            ;;
    esac

    [[ "${should_build}" -eq 1 ]] || return
    require_file "${EXPORT_SCRIPT}"

    log "building the Godot web client on the host"
    if [[ -n "${GODOT_BIN:-}" ]]; then
        bash "${EXPORT_SCRIPT}" --godot-bin "${GODOT_BIN}"
    else
        bash "${EXPORT_SCRIPT}"
    fi
}

wait_for_healthz() {
    local attempts=60
    local delay_seconds=2
    local container_id=""

    for ((attempt = 1; attempt <= attempts; attempt += 1)); do
        container_id="$(docker compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" ps -q rarena-server 2>/dev/null || true)"
        if [[ -n "${container_id}" ]]; then
            local health_status
            health_status="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "${container_id}" 2>/dev/null || true)"
            if [[ "${health_status}" == "healthy" || "${health_status}" == "running" ]]; then
                log "backend container health check passed"
                return 0
            fi
        fi
        sleep "${delay_seconds}"
    done

    fatal "backend container did not become healthy"
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
    build_web_client_if_requested

    log "validating compose configuration"
    docker compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" config -q

    log "building and starting the production stack"
    docker compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" build --pull
    docker compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" up -d --remove-orphans

    wait_for_healthz

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
