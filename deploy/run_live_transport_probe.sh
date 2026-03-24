#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CONFIG_DIR="${RARENA_CONFIG_DIR:-${HOME}/rusaren-config}"
ENV_FILE="${RARENA_ENV_FILE:-${CONFIG_DIR}/config.env}"
OUTPUT_DIR="${RARENA_PROBE_OUTPUT_DIR:-${CONFIG_DIR}/probes}"

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

pushd "${REPO_ROOT}/server" >/dev/null
set +e
cargo run -p live_transport_probe --release -- --origin "${ORIGIN}" --output "${PROBE_LOG}" "$@"
STATUS=$?
set -e
popd >/dev/null

if [[ ${STATUS} -ne 0 ]]; then
  echo "[run_live_transport_probe] probe failed, collecting backend diagnostics into ${DIAG_LOG}"
  bash "${REPO_ROOT}/deploy/useful_log_collect.sh" --origin "${ORIGIN}" --output "${DIAG_LOG}" || true
  echo "[run_live_transport_probe] paste these files when reporting the failure:"
  echo "  ${PROBE_LOG}"
  echo "  ${DIAG_LOG}"
fi

exit "${STATUS}"
