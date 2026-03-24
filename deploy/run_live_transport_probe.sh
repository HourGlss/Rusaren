#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CONFIG_DIR="${RARENA_CONFIG_DIR:-${HOME}/rusaren-config}"
ENV_FILE="${RARENA_ENV_FILE:-${CONFIG_DIR}/config.env}"
OUTPUT_DIR="${RARENA_PROBE_OUTPUT_DIR:-${CONFIG_DIR}/probes}"
RUST_IMAGE="${RARENA_PROBE_RUST_IMAGE:-rust:1.94-bookworm}"
CARGO_HOME_DIR="${RARENA_PROBE_CARGO_HOME:-${CONFIG_DIR}/cargo-home}"
CARGO_TARGET_DIR="${RARENA_PROBE_TARGET_DIR:-${CONFIG_DIR}/cargo-target/live-transport-probe}"

if [[ -f "${ENV_FILE}" ]]; then
  # shellcheck disable=SC1090
  source "${ENV_FILE}"
fi

ORIGIN="${RARENA_PROBE_ORIGIN:-}"
if [[ -z "${ORIGIN}" ]]; then
  if [[ -n "${PUBLIC_HOST:-}" ]]; then
    ORIGIN="https://${PUBLIC_HOST}"
  else
    echo "run_live_transport_probe: set RARENA_PROBE_ORIGIN or PUBLIC_HOST" >&2
    exit 1
  fi
fi

mkdir -p "${OUTPUT_DIR}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
PROBE_LOG="${OUTPUT_DIR}/live-transport-probe-${STAMP}.jsonl"
DIAG_LOG="${OUTPUT_DIR}/live-transport-diagnostics-${STAMP}.txt"

echo "[run_live_transport_probe] origin=${ORIGIN}"
echo "[run_live_transport_probe] probe_log=${PROBE_LOG}"

run_probe_with_local_cargo() {
  local cargo_bin=()
  if [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
    cargo_bin=("${HOME}/.cargo/bin/cargo")
  elif command -v cargo >/dev/null 2>&1; then
    cargo_bin=("$(command -v cargo)")
  elif command -v rustup >/dev/null 2>&1; then
    cargo_bin=("rustup" "run" "stable" "cargo")
  else
    return 127
  fi

  pushd "${REPO_ROOT}/server" >/dev/null
  set +e
  "${cargo_bin[@]}" run -p live_transport_probe --release -- --origin "${ORIGIN}" --output "${PROBE_LOG}" "$@"
  local status=$?
  set -e
  popd >/dev/null
  return "${status}"
}

run_probe_with_docker() {
  mkdir -p "${CARGO_HOME_DIR}" "${CARGO_TARGET_DIR}"
  echo "[run_live_transport_probe] cargo not found, using ${RUST_IMAGE} via docker"
  set +e
  docker run --rm \
    --user "$(id -u):$(id -g)" \
    -e CARGO_HOME="${CARGO_HOME_DIR}" \
    -e CARGO_TARGET_DIR="${CARGO_TARGET_DIR}" \
    -v "${REPO_ROOT}/server:/workspace" \
    -v "${CONFIG_DIR}:${CONFIG_DIR}" \
    -w /workspace \
    "${RUST_IMAGE}" \
    cargo run -p live_transport_probe --release -- --origin "${ORIGIN}" --output "${PROBE_LOG}" "$@"
  local status=$?
  set -e
  return "${status}"
}

if run_probe_with_local_cargo "$@"; then
  STATUS=0
else
  STATUS=$?
  if [[ ${STATUS} -eq 127 ]]; then
    run_probe_with_docker "$@"
    STATUS=$?
  fi
fi

if [[ ${STATUS} -ne 0 ]]; then
  echo "[run_live_transport_probe] probe failed, collecting backend diagnostics into ${DIAG_LOG}"
  bash "${REPO_ROOT}/deploy/useful_log_collect.sh" --origin "${ORIGIN}" --output "${DIAG_LOG}" || true
  echo "[run_live_transport_probe] paste these files when reporting the failure:"
  echo "  ${PROBE_LOG}"
  echo "  ${DIAG_LOG}"
fi

exit "${STATUS}"
