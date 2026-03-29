#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
CONFIG_DIR="${CONFIG_DIR:-}"
ENV_FILE="${ENV_FILE:-}"
COMPOSE_OVERRIDE_FILE="${COMPOSE_OVERRIDE_FILE:-}"
BASE_URL="${BASE_URL:-}"
SINCE="${SINCE:-20m}"
TAIL="${TAIL:-200}"
OUTPUT_FILE="${OUTPUT_FILE:-}"
BUNDLE_DIR="${BUNDLE_DIR:-}"
COMPOSE_ARGS=()

usage() {
    cat <<'EOF'
Usage: deploy/useful_log_collect.sh [--origin URL] [--env-file PATH] [--since WINDOW] [--tail COUNT] [--output PATH] [--bundle-dir PATH]

Collects a compact hosted-backend diagnostics report intended for copy/paste.
By default it loads `~/rusaren-config/config.env` and the optional external
Compose override for the current deploy user. When `--bundle-dir` is supplied,
it also writes structured host, admin, metrics, and container artifacts there.
EOF
}

fatal() {
    printf '[useful-log-collect] ERROR: %s\n' "$*" >&2
    exit 1
}

default_config_dir() {
    if [[ -n "${CONFIG_DIR}" ]]; then
        printf '%s\n' "${CONFIG_DIR}"
        return
    fi

    if [[ "${EUID:-$(id -u)}" -eq 0 && -n "${SUDO_USER:-}" && "${SUDO_USER}" != "root" ]]; then
        local sudo_home
        sudo_home="$(getent passwd "${SUDO_USER}" | cut -d: -f6)"
        if [[ -n "${sudo_home}" ]]; then
            printf '%s/rusaren-config\n' "${sudo_home}"
            return
        fi
    fi

    printf '%s/rusaren-config\n' "${HOME}"
}

load_env() {
    if [[ -z "${ENV_FILE}" ]]; then
        ENV_FILE="$(default_config_dir)/config.env"
    fi

    if [[ -f "${ENV_FILE}" ]]; then
        set -a
        # shellcheck disable=SC1090
        source "${ENV_FILE}"
        set +a
    fi

    if [[ -z "${COMPOSE_OVERRIDE_FILE}" ]]; then
        COMPOSE_OVERRIDE_FILE="$(default_config_dir)/docker-compose.override.yml"
    fi
}

prepare_compose_args() {
    COMPOSE_ARGS=(--env-file "${ENV_FILE}" -f "${REPO_ROOT}/deploy/docker-compose.yml")
    if [[ -n "${COMPOSE_OVERRIDE_FILE}" && -f "${COMPOSE_OVERRIDE_FILE}" ]]; then
        COMPOSE_ARGS+=(-f "${COMPOSE_OVERRIDE_FILE}")
    fi
}

compose() {
    docker compose "${COMPOSE_ARGS[@]}" "$@"
}

resolve_base_url() {
    if [[ -n "${BASE_URL}" ]]; then
        printf '%s\n' "${BASE_URL}"
        return
    fi

    if [[ -n "${PUBLIC_HOST:-}" ]]; then
        printf 'https://%s\n' "${PUBLIC_HOST}"
        return
    fi

    printf '%s\n' 'http://127.0.0.1:3000'
}

print_section() {
    printf '\n=== %s ===\n' "$1"
}

fetch_status_code() {
    local url="$1"
    local output_file="$2"
    shift 2
    curl --silent --show-error --location --max-time 15 --insecure \
        --output "${output_file}" \
        --write-out '%{http_code}' \
        "$@" \
        "${url}"
}

print_public_probe_summary() {
    local base_url="$1"
    local response_file status_code
    response_file="$(mktemp)"

    status_code="$(fetch_status_code "${base_url}/" "${response_file}" || true)"
    printf 'root_status: %s\n' "${status_code:-unavailable}"
    rm -f "${response_file}"

    response_file="$(mktemp)"
    status_code="$(fetch_status_code "${base_url}/healthz" "${response_file}" || true)"
    printf 'healthz_status: %s\n' "${status_code:-unavailable}"
    if [[ -s "${response_file}" ]]; then
        printf 'healthz_body: %s\n' "$(tr -d '\r\n' < "${response_file}")"
    fi
    rm -f "${response_file}"

    response_file="$(mktemp)"
    status_code="$(fetch_status_code "${base_url}/session/bootstrap" "${response_file}" || true)"
    printf 'session_bootstrap_status: %s\n' "${status_code:-unavailable}"
    if [[ -s "${response_file}" ]]; then
        python3 - "${response_file}" <<'PY'
import json
import sys

try:
    payload = json.load(open(sys.argv[1], 'r', encoding='utf-8'))
except Exception as exc:
    print(f"session_bootstrap_parse_error: {exc}")
    raise SystemExit(0)

token = payload.get("token", "")
expires = payload.get("expires_in_ms")
print(f"session_bootstrap_token_present: {'yes' if isinstance(token, str) and len(token) > 0 else 'no'}")
print(f"session_bootstrap_expires_in_ms: {expires}")
PY
    fi
    rm -f "${response_file}"
}

print_admin_summary() {
    local base_url="$1"
    if [[ -z "${RARENA_ADMIN_USERNAME:-}" || -z "${RARENA_ADMIN_PASSWORD:-}" ]]; then
        printf 'adminz: skipped (credentials not configured)\n'
        return
    fi

    local response_file status_code basic_auth
    response_file="$(mktemp)"
    basic_auth="$(printf '%s:%s' "${RARENA_ADMIN_USERNAME}" "${RARENA_ADMIN_PASSWORD}" | base64 | tr -d '\n')"
    status_code="$(fetch_status_code "${base_url}/adminz" "${response_file}" --header "Authorization: Basic ${basic_auth}" || true)"
    printf 'adminz_status: %s\n' "${status_code:-unavailable}"
    if [[ "${status_code}" != "200" ]]; then
        rm -f "${response_file}"
        return
    fi

    python3 - "${response_file}" <<'PY'
from html import unescape
import re
import sys

text = open(sys.argv[1], 'r', encoding='utf-8').read()

def clean(value: str) -> str:
    value = re.sub(r'<[^>]+>', '', value)
    value = unescape(value)
    value = re.sub(r'\s+', ' ', value)
    return value.strip()

rows = re.findall(r'<tr><th>(.*?)</th><td>(.*?)</td></tr>', text, flags=re.S)
for key, value in rows:
    print(f"admin_{clean(key).lower().replace(' ', '_')}: {clean(value)}")

diag_match = re.search(r'<h2>Recent Diagnostics</h2><table>(.*?)</table>', text, flags=re.S)
if not diag_match:
    print("admin_recent_diagnostics: none")
    raise SystemExit(0)

diag_rows = re.findall(
    r'<tr><td>(.*?)</td><td>(.*?)</td><td>(.*?)</td><td>(.*?)</td><td>(.*?)</td></tr>',
    diag_match.group(1),
    flags=re.S,
)
if not diag_rows:
    print("admin_recent_diagnostics: none")
    raise SystemExit(0)

print("admin_recent_diagnostics:")
for elapsed_s, category, connection, player, detail in diag_rows[:25]:
    print(
        "  - elapsed_s={elapsed} category={category} connection={connection} player={player} detail={detail}".format(
            elapsed=clean(elapsed_s),
            category=clean(category),
            connection=clean(connection),
            player=clean(player),
            detail=clean(detail),
        )
    )
PY

    rm -f "${response_file}"
}

print_filtered_logs() {
    local log_output
    log_output="$(
        compose logs --no-color --since "${SINCE}" --tail "${TAIL}" rarena-server caddy coturn 2>&1 \
            | grep -Ei 'warn|error|disconnect|rejection|reject|webrtc|websocket|bootstrap|ingress|peer connection|ice|turn|failed|close'
    )" || true

    if [[ -z "${log_output}" ]]; then
        printf '(no matching log lines found in the selected window)\n'
        return
    fi

    printf '%s\n' "${log_output}"
}

print_host_summary() {
    printf 'time_utc: %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf 'host: %s\n' "$(hostname)"
    if [[ -f /proc/loadavg ]]; then
        printf 'loadavg: %s\n' "$(cat /proc/loadavg)"
    fi
    if command -v uptime >/dev/null 2>&1; then
        printf 'uptime: %s\n' "$(uptime)"
    fi
    if command -v free >/dev/null 2>&1; then
        printf '\n[mem]\n'
        free -h
    fi
    if command -v df >/dev/null 2>&1; then
        printf '\n[df_root]\n'
        df -h /
    fi
}

print_docker_stats() {
    docker stats --no-stream --format 'table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.NetIO}}\t{{.BlockIO}}' || true
}

fetch_admin_json() {
    local base_url="$1"
    if [[ -z "${RARENA_ADMIN_USERNAME:-}" || -z "${RARENA_ADMIN_PASSWORD:-}" ]]; then
        return 0
    fi
    local basic_auth
    basic_auth="$(printf '%s:%s' "${RARENA_ADMIN_USERNAME}" "${RARENA_ADMIN_PASSWORD}" | base64 | tr -d '\n')"
    curl --silent --show-error --location --max-time 20 --insecure \
        --header "Authorization: Basic ${basic_auth}" \
        "${base_url}/adminz?format=json"
}

write_bundle_artifacts() {
    local base_url="$1"
    mkdir -p "${BUNDLE_DIR}"
    generate_report > "${BUNDLE_DIR}/summary.txt"
    compose ps > "${BUNDLE_DIR}/docker-compose-ps.txt" 2>&1 || true
    print_public_probe_summary "${base_url}" > "${BUNDLE_DIR}/public-probes.txt"
    print_admin_summary "${base_url}" > "${BUNDLE_DIR}/admin-summary.txt"
    fetch_admin_json "${base_url}" > "${BUNDLE_DIR}/adminz.json" 2>&1 || true
    curl --silent --show-error --location --max-time 20 --insecure \
        "${base_url}/metrics" > "${BUNDLE_DIR}/metrics.prom" 2>&1 || true
    print_filtered_logs > "${BUNDLE_DIR}/filtered-logs.txt"
    print_host_summary > "${BUNDLE_DIR}/host.txt"
    print_docker_stats > "${BUNDLE_DIR}/docker-stats.txt"
}

generate_report() {
    local base_url
    base_url="$(resolve_base_url)"

    print_section "Context"
    printf 'time_utc: %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf 'host: %s\n' "$(hostname)"
    printf 'repo_head: %s\n' "$(git -C "${REPO_ROOT}" rev-parse --short HEAD 2>/dev/null || printf 'unavailable')"
    printf 'base_url: %s\n' "${base_url}"
    printf 'env_file: %s\n' "${ENV_FILE}"
    if [[ -f "${COMPOSE_OVERRIDE_FILE}" ]]; then
        printf 'compose_override: %s\n' "${COMPOSE_OVERRIDE_FILE}"
    else
        printf 'compose_override: none\n'
    fi
    printf 'public_host: %s\n' "${PUBLIC_HOST:-unset}"
    printf 'turn_public_host: %s\n' "${TURN_PUBLIC_HOST:-unset}"
    printf 'turn_external_ip: %s\n' "${TURN_EXTERNAL_IP:-unset}"
    printf 'rust_log: %s\n' "${RARENA_RUST_LOG:-unset}"
    printf 'log_format: %s\n' "${RARENA_LOG_FORMAT:-unset}"

    print_section "Docker Compose PS"
    compose ps || true

    print_section "Public Probes"
    print_public_probe_summary "${base_url}"

    print_section "Admin Summary"
    print_admin_summary "${base_url}"

    print_section "Filtered Logs"
    print_filtered_logs
}

main() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --origin)
                [[ $# -ge 2 ]] || fatal "--origin requires a value"
                BASE_URL="$2"
                shift 2
                ;;
            --env-file)
                [[ $# -ge 2 ]] || fatal "--env-file requires a value"
                ENV_FILE="$2"
                shift 2
                ;;
            --since)
                [[ $# -ge 2 ]] || fatal "--since requires a value"
                SINCE="$2"
                shift 2
                ;;
            --tail)
                [[ $# -ge 2 ]] || fatal "--tail requires a value"
                TAIL="$2"
                shift 2
                ;;
            --output)
                [[ $# -ge 2 ]] || fatal "--output requires a value"
                OUTPUT_FILE="$2"
                shift 2
                ;;
            --bundle-dir)
                [[ $# -ge 2 ]] || fatal "--bundle-dir requires a value"
                BUNDLE_DIR="$2"
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

    load_env
    prepare_compose_args

    if [[ -n "${OUTPUT_FILE}" ]]; then
        generate_report | tee "${OUTPUT_FILE}"
    else
        generate_report
    fi
    if [[ -n "${BUNDLE_DIR}" ]]; then
        write_bundle_artifacts "$(resolve_base_url)"
        printf '\n[useful-log-collect] wrote diagnostics bundle to %s\n' "${BUNDLE_DIR}"
    fi
}

main "$@"
