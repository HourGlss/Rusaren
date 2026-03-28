#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
COMPOSE_FILE="${REPO_ROOT}/deploy/docker-compose.yml"
SMOKE_SCRIPT="${REPO_ROOT}/deploy/host-smoke.sh"
EXPORT_SCRIPT="${REPO_ROOT}/server/scripts/export-web-client.sh"
CONFIG_DIR="${CONFIG_DIR:-}"
ENV_FILE="${ENV_FILE:-}"
COMPOSE_OVERRIDE_FILE="${COMPOSE_OVERRIDE_FILE:-}"
DOWN_ONLY=0
COMPOSE_ARGS=()

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

resolve_runtime_user() {
    if [[ -n "${DEPLOY_RUNTIME_USER:-}" ]]; then
        printf '%s\n' "${DEPLOY_RUNTIME_USER}"
        return
    fi

    if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
        if [[ -n "${SUDO_USER:-}" && "${SUDO_USER}" != "root" ]]; then
            printf '%s\n' "${SUDO_USER}"
            return
        fi

        local repo_owner
        repo_owner="$(stat -c '%U' "${REPO_ROOT}" 2>/dev/null || true)"
        if [[ -n "${repo_owner}" && "${repo_owner}" != "root" ]]; then
            printf '%s\n' "${repo_owner}"
            return
        fi
    fi

    id -un
}

resolve_runtime_home() {
    local runtime_user
    runtime_user="$(resolve_runtime_user)"
    local runtime_home
    runtime_home="$(getent passwd "${runtime_user}" | cut -d: -f6)"
    [[ -n "${runtime_home}" ]] || fatal "unable to resolve a home directory for runtime user ${runtime_user}"
    printf '%s\n' "${runtime_home}"
}

resolve_config_dir() {
    if [[ -n "${CONFIG_DIR}" ]]; then
        printf '%s\n' "${CONFIG_DIR}"
        return
    fi

    printf '%s/rusaren-config\n' "$(resolve_runtime_home)"
}

build_compose_args() {
    COMPOSE_ARGS=(--env-file "${ENV_FILE}" -f "${COMPOSE_FILE}")
    if [[ -n "${COMPOSE_OVERRIDE_FILE}" && -f "${COMPOSE_OVERRIDE_FILE}" ]]; then
        COMPOSE_ARGS+=(-f "${COMPOSE_OVERRIDE_FILE}")
    fi
}

run_compose() {
    docker compose "${COMPOSE_ARGS[@]}" "$@"
}

resolve_export_user() {
    if [[ -n "${DEPLOY_EXPORT_USER:-}" ]]; then
        printf '%s\n' "${DEPLOY_EXPORT_USER}"
        return
    fi

    resolve_runtime_user
}

run_web_client_export() {
    local export_user
    export_user="$(resolve_export_user)"
    local export_home
    export_home="$(getent passwd "${export_user}" | cut -d: -f6)"
    [[ -n "${export_home}" ]] || fatal "unable to resolve a home directory for export user ${export_user}"
    local -a export_args
    export_args=("${EXPORT_SCRIPT}")
    if [[ -n "${GODOT_BIN:-}" ]]; then
        export_args+=("--godot-bin" "${GODOT_BIN}")
    fi

    log "building the Godot web client on the host as ${export_user}"

    if [[ "${export_user}" == "$(id -un)" ]]; then
        HOME="${export_home}" bash "${export_args[@]}"
        return
    fi

    local -a export_env
    export_env=("HOME=${export_home}")
    if [[ -n "${GODOT_BIN:-}" ]]; then
        export_env+=("GODOT_BIN=${GODOT_BIN}")
    fi

    runuser -u "${export_user}" -- env "${export_env[@]}" bash "${export_args[@]}"
}

ensure_static_root() {
    local static_root="${REPO_ROOT}/server/static/webclient"
    mkdir -p "${static_root}"

    if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
        local export_user
        export_user="$(resolve_export_user)"
        local export_group
        export_group="$(id -gn "${export_user}")"
        chown -R "${export_user}:${export_group}" "${static_root}"
    fi
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

    run_web_client_export
}

wait_for_healthz() {
    local attempts=60
    local delay_seconds=2
    local container_id=""

    for ((attempt = 1; attempt <= attempts; attempt += 1)); do
        container_id="$(run_compose ps -q rarena-server 2>/dev/null || true)"
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

usage() {
    cat <<'EOF'
Usage: deploy/deploy.sh [--down] [--config-dir PATH] [--env-file PATH] [--compose-override PATH]

Deploys or stops the Rusaren hosted stack using `~/rusaren-config/` by default.
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --down)
                DOWN_ONLY=1
                shift
                ;;
            --config-dir)
                [[ $# -ge 2 ]] || fatal "--config-dir requires a path"
                CONFIG_DIR="$2"
                shift 2
                ;;
            --env-file)
                [[ $# -ge 2 ]] || fatal "--env-file requires a path"
                ENV_FILE="$2"
                shift 2
                ;;
            --compose-override)
                [[ $# -ge 2 ]] || fatal "--compose-override requires a path"
                COMPOSE_OVERRIDE_FILE="$2"
                shift 2
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                fatal "unknown argument: $1"
                ;;
        esac
    done
}

ensure_env_file() {
    local default_example="${REPO_ROOT}/deploy/config.env.example"
    local legacy_example="${REPO_ROOT}/deploy/.env.example"

    if [[ -f "${ENV_FILE}" ]]; then
        return
    fi

    mkdir -p "$(dirname -- "${ENV_FILE}")"
    if [[ -f "${default_example}" ]]; then
        cp "${default_example}" "${ENV_FILE}"
    else
        require_file "${legacy_example}"
        cp "${legacy_example}" "${ENV_FILE}"
    fi

    fatal "created ${ENV_FILE}; set real values first, then rerun this script"
}

main() {
    parse_args "$@"

    require_file "${COMPOSE_FILE}"
    require_file "${SMOKE_SCRIPT}"

    if [[ -z "${ENV_FILE}" ]]; then
        ENV_FILE="$(resolve_config_dir)/config.env"
    fi
    if [[ -z "${COMPOSE_OVERRIDE_FILE}" ]]; then
        COMPOSE_OVERRIDE_FILE="$(resolve_config_dir)/docker-compose.override.yml"
    fi
    build_compose_args
    ensure_env_file

    if [[ "${DOWN_ONLY}" -eq 1 ]]; then
        log "stopping the production stack"
        run_compose down
        return
    fi

    ensure_static_root
    build_web_client_if_requested

    log "validating compose configuration"
    run_compose config -q

    log "building and starting the production stack"
    run_compose build --pull
    run_compose up -d --remove-orphans

    wait_for_healthz

    if [[ "${RUN_PUBLIC_SMOKE:-1}" == "1" ]]; then
        log "running hosted smoke probes"
        bash "${SMOKE_SCRIPT}" --env-file "${ENV_FILE}"
    else
        log "skipping hosted smoke probes because RUN_PUBLIC_SMOKE=${RUN_PUBLIC_SMOKE:-1}"
    fi

    log "current service status"
    run_compose ps
}

main "$@"
