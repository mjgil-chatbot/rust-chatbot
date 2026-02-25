#!/usr/bin/env bash
# Long auto-ui test suite for rust-chatbot.
#
# No time budget. Uses a mock provider server so responses are fast and
# deterministic without requiring real API credentials.
#
# Tests:
#   - prompt_debug scenario: sends a message, gets a mock response, verifies
#     the session is promoted to sidebar top and stays active.
#
# Env knobs:
#   AUTO_UI_BIN        path to the auto-ui binary (default: ~/git/auto-ui/target/release/auto-ui)
#   APP_ROOT           rust-chatbot repo root (default: script's parent dir)
#   USE_REAL_DISPLAY=1 disable headless mode and run on $DISPLAY directly
#   KEEP_HOME=1        preserve the temp HOME for inspection
#   KEEP_OUT=1         preserve the output dir for inspection

set -euo pipefail

APP_ROOT="${APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
AUTO_UI_BIN="${AUTO_UI_BIN:-$HOME/git/auto-ui/target/release/auto-ui}"
FIXTURE_SRC="$APP_ROOT/tests/auto-ui/fixtures/home"

if [[ ! -x "$AUTO_UI_BIN" ]]; then
  echo "auto-ui binary not found at $AUTO_UI_BIN" >&2
  echo "Build it first: (cd ~/git/auto-ui && cargo build --release --bin auto-ui)" >&2
  exit 1
fi

if [[ ! -d "$FIXTURE_SRC" ]]; then
  echo "Fixture directory missing: $FIXTURE_SRC" >&2
  exit 1
fi

# Ensure release binaries are present.
need_build=0
for bin in rust-chatbot chatbot-ctl chatbot-worker; do
  if [[ ! -x "$APP_ROOT/target/release/$bin" ]]; then
    need_build=1
    break
  fi
done
if [[ $need_build -eq 1 ]]; then
  echo "Building rust-chatbot release binaries..."
  (cd "$APP_ROOT" && cargo build --release)
fi

# ── Display mode ──────────────────────────────────────────────────────────
HEADLESS_FLAG=""
if [[ "${USE_REAL_DISPLAY:-0}" == "1" ]]; then
  if [[ -z "${DISPLAY:-}" ]]; then
    echo "USE_REAL_DISPLAY=1 set but \$DISPLAY is empty." >&2
    exit 1
  fi
  echo "[auto-ui-test-long] running on real display \$DISPLAY=$DISPLAY"
else
  HEADLESS_FLAG="--headless"
  echo "[auto-ui-test-long] running headless (Xvfb + openbox managed by auto-ui)"
fi

# ── Mock provider setup ───────────────────────────────────────────────────
MOCK_PORT_FILE="$(mktemp -t mock-port-XXXXXX)"
MOCK_SERVER_PID=""

start_mock_provider() {
  python3 "$APP_ROOT/scripts/mock-provider-server.py" \
    --port-file "$MOCK_PORT_FILE" \
    --response-text "This is a mock assistant response for the auto-ui test." &
  MOCK_SERVER_PID=$!
  for _ in $(seq 1 50); do
    if [[ -s "$MOCK_PORT_FILE" ]]; then break; fi
    sleep 0.1
  done
  if ! kill -0 "$MOCK_SERVER_PID" 2>/dev/null; then
    echo "Mock provider server failed to start" >&2
    exit 1
  fi
  MOCK_PORT=$(cat "$MOCK_PORT_FILE")
  echo "[auto-ui-test-long] mock provider on 127.0.0.1:$MOCK_PORT (pid=$MOCK_SERVER_PID)"
}

# ── Temp dirs and cleanup ─────────────────────────────────────────────────
TMP_HOME="$(mktemp -d -t rust-chatbot-auto-ui-long-home-XXXXXX)"
OUT_DIR="$(mktemp -d -t rust-chatbot-auto-ui-long-out-XXXXXX)"

cleanup() {
  if [[ -n "$MOCK_SERVER_PID" ]] && kill -0 "$MOCK_SERVER_PID" 2>/dev/null; then
    kill "$MOCK_SERVER_PID" 2>/dev/null || true
    wait "$MOCK_SERVER_PID" 2>/dev/null || true
  fi
  rm -f "$MOCK_PORT_FILE"
  if [[ "${KEEP_HOME:-0}" != "1" ]]; then
    rm -rf "$TMP_HOME"
  else
    echo "kept HOME: $TMP_HOME"
  fi
  if [[ "${KEEP_OUT:-0}" != "1" ]]; then
    rm -rf "$OUT_DIR"
  else
    echo "kept output: $OUT_DIR"
  fi
}
trap cleanup EXIT

cp -a "$FIXTURE_SRC/." "$TMP_HOME/"
export HOME="$TMP_HOME"
mkdir -p "$TMP_HOME/.rust-chatbot"
shopt -s nullglob
for legacy_dir in "$TMP_HOME"/.*-desktop; do
  base_name="$(basename "$legacy_dir")"
  canonical_dir="$TMP_HOME/.rust-chatbot/$base_name"
  if [[ ! -e "$canonical_dir" ]]; then
    ln -s "$legacy_dir" "$canonical_dir"
  fi
done
shopt -u nullglob
export RUST_CHATBOT_CONFIG_PATH="$TMP_HOME/.rust-chatbot/config.toml"

log_dir="$TMP_HOME/.local/state/rust-chatbot"
mkdir -p "$log_dir"
: > "$log_dir/rust-chatbot.log.$(date -u +%Y-%m-%d)"
: > "$log_dir/rust-chatbot.log.$(date -u -d 'yesterday' +%Y-%m-%d)"

# ── Start mock provider ───────────────────────────────────────────────────
start_mock_provider

export RUST_CHATBOT_TEST_PROVIDER_CLI="$APP_ROOT/scripts/fake-claude-cli.py"
export RUST_CHATBOT_TEST_ANTHROPIC_BASE_URL="http://127.0.0.1:$MOCK_PORT/anthropic"
export RUST_CHATBOT_TEST_ANTHROPIC_AUTH_TOKEN="mock-token"
export ANTHROPIC_BASE_URL="http://127.0.0.1:$MOCK_PORT/anthropic"
export ANTHROPIC_API_KEY="mock-token"

start=$(date +%s)
echo "[auto-ui-test-long] HOME=$HOME OUT=$OUT_DIR"

# ── prompt_debug scenario ─────────────────────────────────────────────────
PROMPT_TOML="$(mktemp -t rust-chatbot-prompt-XXXXXX.toml)"
cat >"$PROMPT_TOML" <<TOML
target = "rust_chatbot"
scenario = "prompt_debug"

[app]
root = "$APP_ROOT"
provider = "codex"
session_id = "fixture-pl-update"

[window]
width = 900
height = 700
keep_front = false

[prompt]
text = "say hello"

[runtime]
window_timeout = 15
trace_timeout = 30
settle = 1.0
TOML

echo "[auto-ui-test-long] running prompt_debug (mocked provider)"
"$AUTO_UI_BIN" run \
  $HEADLESS_FLAG \
  --config "$PROMPT_TOML" \
  --output-dir "$OUT_DIR/prompt"
rm -f "$PROMPT_TOML"

# ── Assertions ─────────────────────────────────────────────────────────────
REPORT="$OUT_DIR/prompt/report.json"
if [[ -f "$REPORT" ]]; then
  status=$(python3 -c "import json; r=json.load(open('$REPORT')); print(r.get('status','unknown'))")
  echo "[auto-ui-test-long] prompt_debug report status: $status"
  if [[ "$status" != "ok" ]]; then
    echo "FAIL: prompt_debug report status is '$status', expected 'ok'" >&2
    cat "$REPORT" >&2
    exit 1
  fi
else
  echo "FAIL: report not found at $REPORT" >&2
  exit 1
fi

end=$(date +%s)
echo "[auto-ui-test-long] total: $((end - start))s"
echo "PASS: long auto-ui tests completed"
