#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SERVER_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd -- "${SERVER_ROOT}/.." && pwd)"

GODOT_BIN="${GODOT_BIN:-}"
PROJECT_PATH="${PROJECT_PATH:-${REPO_ROOT}/client/godot}"
OUTPUT_PATH="${OUTPUT_PATH:-${SERVER_ROOT}/static/webclient/index.html}"
TEMPLATE_ROOT="${TEMPLATE_ROOT:-}"
INSTALL_TEMPLATES=1

log() {
    printf '[export-web-client] %s\n' "$*"
}

fatal() {
    printf '[export-web-client] ERROR: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Usage: server/scripts/export-web-client.sh [--godot-bin PATH] [--project-path PATH] [--output-path PATH] [--template-root PATH] [--skip-template-install]

Exports the Godot Web build into server/static/webclient for the hosted backend.
Defaults:
  --project-path client/godot
  --output-path  server/static/webclient/index.html
  --godot-bin    auto-detect godot4, godot-4, then godot
  --template-root auto-detect standard Linux or snap Godot template paths
  --skip-template-install do not auto-download export templates if missing
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

    if command -v godot-4 >/dev/null 2>&1; then
        command -v godot-4
        return
    fi

    if command -v godot >/dev/null 2>&1; then
        command -v godot
        return
    fi

    fatal "No Godot binary found. Install godot4 or set GODOT_BIN/--godot-bin."
}

parse_godot_build_info() {
    local godot_bin="$1"
    local version_output
    version_output="$("${godot_bin}" --version | head -n1)"

    if [[ "${version_output}" =~ ^([0-9]+\.[0-9]+(\.[0-9]+)?)\.([A-Za-z0-9]+) ]]; then
        GODOT_VERSION_TEXT="${BASH_REMATCH[1]}"
        GODOT_CHANNEL="${BASH_REMATCH[3]}"
        GODOT_VERSION_TAG="${GODOT_VERSION_TEXT}-${GODOT_CHANNEL}"
        GODOT_TEMPLATE_DIR="${GODOT_VERSION_TEXT}.${GODOT_CHANNEL}"
        return
    fi

    fatal "Unable to parse Godot version from: ${version_output}"
}

resolve_template_root() {
    local godot_bin="$1"

    if [[ -n "${TEMPLATE_ROOT}" ]]; then
        printf '%s\n' "${TEMPLATE_ROOT}"
        return
    fi

    if [[ "${godot_bin}" == *"/snap/"* || "${godot_bin}" == "/snap/bin/"* ]]; then
        local snap_name
        snap_name="$(basename -- "${godot_bin}")"
        printf '%s\n' "${HOME}/snap/${snap_name}/current/.local/share/godot/export_templates"
        return
    fi

    if [[ -n "${XDG_DATA_HOME:-}" ]]; then
        printf '%s\n' "${XDG_DATA_HOME}/godot/export_templates"
        return
    fi

    printf '%s\n' "${HOME}/.local/share/godot/export_templates"
}

ensure_templates_installed() {
    local godot_bin="$1"
    local template_root
    template_root="$(resolve_template_root "${godot_bin}")"
    local template_dir="${template_root}/${GODOT_TEMPLATE_DIR}"
    local required_templates=(
        web_debug.zip
        web_release.zip
        web_nothreads_debug.zip
        web_nothreads_release.zip
    )
    local missing_templates=()
    local template_name

    for template_name in "${required_templates[@]}"; do
        if [[ ! -f "${template_dir}/${template_name}" ]]; then
            missing_templates+=("${template_name}")
        fi
    done

    if [[ "${#missing_templates[@]}" -eq 0 ]]; then
        return
    fi

    if [[ "${INSTALL_TEMPLATES}" != "1" ]]; then
        fatal "Godot export templates are incomplete at ${template_dir}. Missing: ${missing_templates[*]}. Re-run without --skip-template-install or install them manually."
    fi

    command -v curl >/dev/null 2>&1 || fatal "curl is required to install Godot export templates"
    command -v unzip >/dev/null 2>&1 || fatal "unzip is required to install Godot export templates"

    local temp_root archive_path extract_root payload_dir
    temp_root="$(mktemp -d)"
    archive_path="${temp_root}/godot-templates.tpz"
    extract_root="${temp_root}/extract"

    rm -rf "${template_dir}"
    mkdir -p "${template_dir}" "${extract_root}"

    log "downloading Godot export templates for ${GODOT_VERSION_TAG} into ${template_dir}"
    curl -L --fail --output "${archive_path}" \
        "https://github.com/godotengine/godot-builds/releases/download/${GODOT_VERSION_TAG}/Godot_v${GODOT_VERSION_TAG}_export_templates.tpz"

    unzip -q "${archive_path}" -d "${extract_root}"
    payload_dir="$(find "${extract_root}" -type f -name version.txt -printf '%h\n' | head -n1)"
    [[ -n "${payload_dir}" ]] || fatal "Could not locate extracted Godot export templates payload"

    cp -a "${payload_dir}/." "${template_dir}/"
    rm -rf "${temp_root}"
    log "installed Godot export templates into ${template_dir}"
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
            --template-root)
                [[ $# -ge 2 ]] || fatal "--template-root requires a value"
                TEMPLATE_ROOT="$2"
                shift 2
                ;;
            --skip-template-install)
                INSTALL_TEMPLATES=0
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

    [[ -f "${PROJECT_PATH}/project.godot" ]] || fatal "Godot project not found at ${PROJECT_PATH}"

    local godot_bin
    godot_bin="$(find_godot_bin)"
    parse_godot_build_info "${godot_bin}"
    ensure_templates_installed "${godot_bin}"
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
