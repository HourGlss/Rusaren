#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SERVER_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd -- "${SERVER_ROOT}/.." && pwd)"

GODOT_BIN="${GODOT_BIN:-}"
PROJECT_PATH="${PROJECT_PATH:-${REPO_ROOT}/client/godot}"
OUTPUT_PATH="${OUTPUT_PATH:-${SERVER_ROOT}/static/webclient/index.html}"

log() {
    printf '[export-web-client] %s\n' "$*"
}

fatal() {
    printf '[export-web-client] ERROR: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Usage: server/scripts/export-web-client.sh [--godot-bin PATH] [--project-path PATH] [--output-path PATH]

Exports the Godot Web build into server/static/webclient for the hosted backend.
Defaults:
  --project-path client/godot
  --output-path  server/static/webclient/index.html
  --godot-bin    auto-detect godot4, then godot
EOF
}

find_godot_bin() {
    if [[ -n "${GODOT_BIN}" ]]; then
        [[ -x "${GODOT_BIN}" ]] || fatal "Godot binary is not executable: ${GODOT_BIN}"
        printf '%s\n' "${GODOT_BIN}"
        return
    fi

    if command -v godot4 >/dev/null 2>&1; then
        command -v godot4
        return
    fi

    if command -v godot >/dev/null 2>&1; then
        command -v godot
        return
    fi

    fatal "No Godot binary found. Install godot4 or set GODOT_BIN/--godot-bin."
}

clear_output_root() {
    local output_root
    output_root="$(dirname -- "${OUTPUT_PATH}")"
    mkdir -p "${output_root}"
    find "${output_root}" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
}

assert_export_artifacts() {
    local output_root
    output_root="$(dirname -- "${OUTPUT_PATH}")"

    [[ -f "${OUTPUT_PATH}" ]] || fatal "Export did not produce ${OUTPUT_PATH}"
    compgen -G "${output_root}/*.js" >/dev/null || fatal "Export did not produce a JavaScript bundle in ${output_root}"
    compgen -G "${output_root}/*.wasm" >/dev/null || fatal "Export did not produce a WebAssembly bundle in ${output_root}"
}

main() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --godot-bin)
                [[ $# -ge 2 ]] || fatal "--godot-bin requires a value"
                GODOT_BIN="$2"
                shift 2
                ;;
            --project-path)
                [[ $# -ge 2 ]] || fatal "--project-path requires a value"
                PROJECT_PATH="$2"
                shift 2
                ;;
            --output-path)
                [[ $# -ge 2 ]] || fatal "--output-path requires a value"
                OUTPUT_PATH="$2"
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

    [[ -f "${PROJECT_PATH}/project.godot" ]] || fatal "Godot project not found at ${PROJECT_PATH}"

    local godot_bin
    godot_bin="$(find_godot_bin)"
    clear_output_root

    log "exporting Web build with ${godot_bin}"
    if ! "${godot_bin}" --headless --path "${PROJECT_PATH}" --export-release Web "${OUTPUT_PATH}"; then
        fatal "Godot export failed. Ensure the Web export preset and export templates are installed for this editor."
    fi

    assert_export_artifacts

    log "Godot Web export complete"
    log "project: ${PROJECT_PATH}"
    log "output: ${OUTPUT_PATH}"
}

main "$@"
