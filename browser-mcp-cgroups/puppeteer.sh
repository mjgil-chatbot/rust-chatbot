#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WRAP="$SCRIPT_DIR/cgroup-wrap.sh"
LOCAL_BIN="${HOME}/git/mcp-servers/mcp-server-puppeteer/target/release/mcp-server-puppeteer"
MANIFEST="${HOME}/git/mcp-servers/mcp-server-puppeteer/apps/stdio-server/Cargo.toml"
REAL_CHROME="/opt/google/chrome/chrome"

if [[ -z "${CHROME:-}" && -x "$REAL_CHROME" ]]; then
  export CHROME="$REAL_CHROME"
fi

if [[ -x "$LOCAL_BIN" ]]; then
  exec "$WRAP" mcp-server-puppeteer "$LOCAL_BIN" "$@"
fi

exec "$WRAP" mcp-server-puppeteer \
  cargo run --release --quiet --manifest-path "$MANIFEST" -- "$@"
