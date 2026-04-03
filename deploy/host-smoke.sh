#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
CONFIG_DIR="${CONFIG_DIR:-}"
ENV_FILE="${ENV_FILE:-}"
BASE_URL="${BASE_URL:-}"
SKIP_ADMIN_CHECK=0

log() {
    printf '[host-smoke] %s\n' "$*"
}

fatal() {
    printf '[host-smoke] ERROR: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Usage: deploy/host-smoke.sh [--origin URL] [--env-file PATH] [--skip-admin]

Runs a small post-deploy smoke suite against the hosted backend path.
The script checks `/`, `/healthz`, `/session/bootstrap`, and both the HTML and
JSON forms of `/adminz` when admin credentials are present in the deploy
environment file.
By default it loads `~/rusaren-config/config.env` for the current deploy user.
EOF
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

fetch_status_code() {
    local url="$1"
    local output_file="$2"
    shift 2
    curl --silent --show-error --location --max-time 15 \
        --output "${output_file}" \
        --write-out '%{http_code}' \
        "$@" \
        "${url}"
}

assert_html_root() {
    local base_url="$1"
    local response_file
    response_file="$(mktemp)"

    local status_code
    status_code="$(fetch_status_code "${base_url}/" "${response_file}")"
    [[ "${status_code}" == "200" ]] || fatal "expected ${base_url}/ to return 200, got ${status_code}"

    grep -Eqi '<!doctype html|<html' "${response_file}" ||
        fatal "expected ${base_url}/ to return HTML"

    rm -f "${response_file}"
    log "root page responded with HTML"
}

assert_healthz() {
    local base_url="$1"
    local response
    response="$(curl --fail --silent --show-error --location --max-time 10 "${base_url}/healthz")"
    [[ "${response}" == "ok" ]] || fatal "expected ${base_url}/healthz to return ok"
    log "healthz responded with ok"
}

assert_session_bootstrap() {
    local base_url="$1"
    local response
    response="$(curl --fail --silent --show-error --location --max-time 10 "${base_url}/session/bootstrap")"

    printf '%s' "${response}" | jq -e '.token | strings | length > 0' >/dev/null ||
        fatal "session bootstrap response did not contain a token"
    printf '%s' "${response}" | jq -e '.expires_in_ms | numbers | . > 0' >/dev/null ||
        fatal "session bootstrap response did not contain a positive expires_in_ms"

    log "session bootstrap minted a token"
}

assert_admin_dashboard() {
    local base_url="$1"

    if [[ "${SKIP_ADMIN_CHECK}" == "1" ]]; then
        log "skipping admin dashboard probe by request"
        return
    fi

    if [[ -z "${RARENA_ADMIN_USERNAME:-}" || -z "${RARENA_ADMIN_PASSWORD:-}" ]]; then
        log "skipping admin dashboard probe because admin credentials are not configured"
        return
    fi

    local response_file
    response_file="$(mktemp)"

    local unauthenticated_status
    unauthenticated_status="$(fetch_status_code "${base_url}/adminz" "${response_file}")"
    [[ "${unauthenticated_status}" == "401" ]] ||
        fatal "expected unauthenticated ${base_url}/adminz to return 401, got ${unauthenticated_status}"

    local basic_auth
    basic_auth="$(printf '%s:%s' "${RARENA_ADMIN_USERNAME}" "${RARENA_ADMIN_PASSWORD}" | base64 | tr -d '\n')"

    local authenticated_status
    authenticated_status="$(fetch_status_code "${base_url}/adminz" "${response_file}" --header "Authorization: Basic ${basic_auth}")"
    [[ "${authenticated_status}" == "200" ]] ||
        fatal "expected authenticated ${base_url}/adminz to return 200, got ${authenticated_status}"

    grep -q "Rusaren Admin Dashboard" "${response_file}" ||
        fatal "expected authenticated ${base_url}/adminz to render the admin dashboard"

    local json_status
    json_status="$(fetch_status_code "${base_url}/adminz?format=json" "${response_file}" --header "Authorization: Basic ${basic_auth}")"
    [[ "${json_status}" == "200" ]] ||
        fatal "expected authenticated ${base_url}/adminz?format=json to return 200, got ${json_status}"

    jq -e '.runtime.connected_players >= 0' "${response_file}" >/dev/null ||
        fatal "expected ${base_url}/adminz?format=json to include runtime.connected_players"
    jq -e '.app_diagnostics.combat_log.append.p99_ms >= 0' "${response_file}" >/dev/null ||
        fatal "expected ${base_url}/adminz?format=json to include combat log timing diagnostics"
    jq -e '.recent_matches | arrays' "${response_file}" >/dev/null ||
        fatal "expected ${base_url}/adminz?format=json to include recent_matches"

    rm -f "${response_file}"
    log "admin dashboard requires auth and renders successfully in HTML and JSON"
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
            --skip-admin)
                SKIP_ADMIN_CHECK=1
                shift
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
    local base_url
    base_url="$(resolve_base_url)"
    log "probing ${base_url}"

    assert_html_root "${base_url}"
    assert_healthz "${base_url}"
    assert_session_bootstrap "${base_url}"
    assert_admin_dashboard "${base_url}"

    log "all smoke probes passed"
}

main "$@"
