#!/usr/bin/env bash
set -euo pipefail

MEMORY_MAX="${BROWSER_MCP_MEMORY_MAX:-8G}"
LIMIT_AS="${BROWSER_MCP_LIMIT_AS:-infinity}"
UNIT_BASE="${1:?missing unit base}"
shift

if [[ $# -eq 0 ]]; then
  printf 'missing command for cgroup wrapper\n' >&2
  exit 1
fi

unit_suffix="$(date +%s)-$$"
unit_name="${UNIT_BASE}-${unit_suffix}.service"

if command -v systemd-run >/dev/null 2>&1; then
  if exec systemd-run --user --wait --collect --pipe --quiet \
    --unit="$unit_name" \
    --property="LimitAS=${LIMIT_AS}" \
    --property="MemoryMax=${MEMORY_MAX}" \
    -- "$@"; then
    exit 0
  fi
fi

exec "$@"
