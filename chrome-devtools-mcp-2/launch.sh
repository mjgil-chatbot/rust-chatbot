#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSCURA_PORT="${OBSCURA_PORT:-9222}"
OBSCURA_START_TIMEOUT_SECS="${OBSCURA_START_TIMEOUT_SECS:-20}"
OBSCURA_LOG_FILE="${OBSCURA_LOG_FILE:-$SCRIPT_DIR/obscura.log}"
OBSCURA_PID_FILE="$SCRIPT_DIR/obscura.pid"
LOCAL_CHROME_DEVTOOLS_MCP_BIN="${LOCAL_CHROME_DEVTOOLS_MCP_BIN:-${HOME}/git/mcp-servers/chrome-devtools-mcp/target/release/chrome-devtools-mcp}"
LOCAL_CHROME_DEVTOOLS_MCP_MANIFEST="${LOCAL_CHROME_DEVTOOLS_MCP_MANIFEST:-${HOME}/git/mcp-servers/chrome-devtools-mcp/apps/stdio-server/Cargo.toml}"
CDP_BASE_URL="http://127.0.0.1:${OBSCURA_PORT}"

resolve_obscura_bin() {
  if [[ -n "${OBSCURA_BIN:-}" ]]; then
    printf '%s\n' "$OBSCURA_BIN"
    return 0
  fi

  if command -v obscura >/dev/null 2>&1; then
    command -v obscura
    return 0
  fi

  if [[ -x "${HOME}/git/others/obscura/target/release/obscura" ]]; then
    printf '%s\n' "${HOME}/git/others/obscura/target/release/obscura"
    return 0
  fi

  if [[ -x "${HOME}/.rust-chatbot/build/obscura/release/obscura" ]]; then
    printf '%s\n' "${HOME}/.rust-chatbot/build/obscura/release/obscura"
    return 0
  fi

  printf 'Unable to find an Obscura binary. Set OBSCURA_BIN.\n' >&2
  return 1
}

cdp_ready() {
  curl --silent --fail "${CDP_BASE_URL}/json/version" >/dev/null 2>&1
}

cleanup_stale_pid() {
  if [[ ! -f "$OBSCURA_PID_FILE" ]]; then
    return 0
  fi

  local pid
  pid="$(cat "$OBSCURA_PID_FILE" 2>/dev/null || true)"
  if [[ -z "$pid" ]]; then
    rm -f "$OBSCURA_PID_FILE"
    return 0
  fi

  if kill -0 "$pid" >/dev/null 2>&1; then
    return 0
  fi

  rm -f "$OBSCURA_PID_FILE"
}

start_obscura_if_needed() {
  if cdp_ready; then
    return 0
  fi

  cleanup_stale_pid

  local obscura_bin
  obscura_bin="$(resolve_obscura_bin)"

  local -a extra_args=()
  if [[ -n "${OBSCURA_EXTRA_ARGS:-}" ]]; then
    # Intentional word splitting for shell-style env arg injection.
    # shellcheck disable=SC2206
    extra_args=(${OBSCURA_EXTRA_ARGS})
  fi

  nohup "$obscura_bin" serve \
    --port "$OBSCURA_PORT" \
    "${extra_args[@]}" \
    >>"$OBSCURA_LOG_FILE" 2>&1 &
  local pid=$!
  printf '%s\n' "$pid" >"$OBSCURA_PID_FILE"

  local deadline=$((SECONDS + OBSCURA_START_TIMEOUT_SECS))
  while (( SECONDS < deadline )); do
    if cdp_ready; then
      return 0
    fi
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      printf 'Obscura exited before CDP became ready. See %s\n' "$OBSCURA_LOG_FILE" >&2
      return 1
    fi
    sleep 1
  done

  printf 'Timed out waiting for Obscura CDP at %s\n' "$CDP_BASE_URL" >&2
  return 1
}

start_obscura_if_needed

if [[ -x "$LOCAL_CHROME_DEVTOOLS_MCP_BIN" ]]; then
  exec "$LOCAL_CHROME_DEVTOOLS_MCP_BIN" --browser-url "$CDP_BASE_URL" "$@"
fi

exec cargo run --release --quiet \
  --manifest-path "$LOCAL_CHROME_DEVTOOLS_MCP_MANIFEST" \
  -- \
  --browser-url "$CDP_BASE_URL" \
  "$@"
