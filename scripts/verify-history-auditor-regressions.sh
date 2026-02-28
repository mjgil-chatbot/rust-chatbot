#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="${APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
EXTRACTED_ROOT="${EXTRACTED_ROOT:-$(cd "${APP_ROOT}/../history-auditor" && pwd)}"

if [[ ! -x "${EXTRACTED_ROOT}/scripts/verify-history-auditor-regressions.sh" ]]; then
  echo "missing extracted regression runner: ${EXTRACTED_ROOT}/scripts/verify-history-auditor-regressions.sh" >&2
  exit 1
fi

exec "${EXTRACTED_ROOT}/scripts/verify-history-auditor-regressions.sh"
