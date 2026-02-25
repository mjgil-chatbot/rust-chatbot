#!/usr/bin/env bash
# Fast auto-ui smoke suite for rust-chatbot.
#
# Runs the auto-ui scenarios headlessly by default (--headless flag on auto-ui),
# so windows never appear on the user's real screen and never steal focus.
#
# Targets a 2-minute budget by:
#   - using a tiny dummy HOME with 7 fixture sessions
#   - tight runtime timeouts (see tests/auto-ui/fast*.toml)
#   - skipping the prompt-debug scenario (see auto-ui-test-full.sh for that)
#
# Env knobs:
#   AUTO_UI_BIN        path to the auto-ui binary (default: ~/git/auto-ui/target/release/auto-ui)
#   APP_ROOT           rust-chatbot repo root (default: script's parent dir)
#   USE_REAL_DISPLAY=1 disable headless mode and run on $DISPLAY directly (visual debug)
#   KEEP_HOME=1        preserve the temp HOME for inspection
#   KEEP_OUT=1         preserve the output dir for inspection
#   SKIP_ASSERT=1      skip the post-run behavioral assertions
#   MULTI_WIDTH=1      add the multi-width.toml run (height cache across widths)

set -euo pipefail

APP_ROOT="${APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
AUTO_UI_BIN="${AUTO_UI_BIN:-$HOME/git/auto-ui/target/release/auto-ui}"
FIXTURE_SRC="$APP_ROOT/tests/auto-ui/fixtures/home"

if [[ ! -x "$AUTO_UI_BIN" ]]; then
  echo "auto-ui binary not found at $AUTO_UI_BIN" >&2
  echo "Build it first: (cd ~/git/auto-ui && cargo build --release --bin auto-ui)" >&2
  echo "Or set AUTO_UI_BIN to override the path." >&2
  exit 1
fi

if [[ ! -d "$FIXTURE_SRC" ]]; then
  echo "Fixture directory missing: $FIXTURE_SRC" >&2
  exit 1
fi

# Ensure rust-chatbot release binaries are present (auto-ui requires them).
need_build=0
for bin in rust-chatbot chatbot-ctl; do
  if [[ ! -x "$APP_ROOT/target/release/$bin" ]]; then
    need_build=1
    break
  fi
done
if [[ $need_build -eq 1 ]]; then
  echo "Building rust-chatbot release binaries..."
  (cd "$APP_ROOT" && cargo build --release --bin rust-chatbot --bin chatbot-ctl)
fi

# ── Display mode ──────────────────────────────────────────────────────────
HEADLESS_FLAG=""
if [[ "${USE_REAL_DISPLAY:-0}" == "1" ]]; then
  if [[ -z "${DISPLAY:-}" ]]; then
    echo "USE_REAL_DISPLAY=1 set but \$DISPLAY is empty." >&2
    exit 1
  fi
  echo "[auto-ui-test] running on real display \$DISPLAY=$DISPLAY (focus may be disrupted)"
else
  HEADLESS_FLAG="--headless"
  echo "[auto-ui-test] running headless (Xvfb + openbox managed by auto-ui)"
fi

TMP_HOME="$(mktemp -d -t rust-chatbot-auto-ui-home-XXXXXX)"
OUT_DIR="$(mktemp -d -t rust-chatbot-auto-ui-out-XXXXXX)"

cleanup() {
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

# Mirror legacy provider fixture directories into the shared runtime root used
# by current desktop providers.
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

# Pin the config to the fixture explicitly so a real chatbot-config.toml in
# the repo root cannot preempt it. The env var is the exclusive search entry
# for AppConfig::config_search_path (src/config/mod.rs).
export RUST_CHATBOT_CONFIG_PATH="$TMP_HOME/.rust-chatbot/config.toml"

# Seed the trace log file auto-ui scans (newest_trace_log in
# auto-ui-adapter-rust-chatbot). rust-chatbot's tracing_appender::rolling::daily
# names files in UTC (rust-chatbot.log.YYYY-MM-DD), and opens the same path on
# launch to append. We seed today's UTC file so auto-ui's newest_trace_log
# captures the same path the binary writes to. We also seed yesterday's UTC
# file so a date rollover mid-test doesn't strand auto-ui on a stale path.
log_dir="$TMP_HOME/.local/state/rust-chatbot"
mkdir -p "$log_dir"
: > "$log_dir/rust-chatbot.log.$(date -u +%Y-%m-%d)"
: > "$log_dir/rust-chatbot.log.$(date -u -d 'yesterday' +%Y-%m-%d)"

start=$(date +%s)
echo "[auto-ui-test] HOME=$HOME OUT=$OUT_DIR"

echo "[auto-ui-test] running fast.toml (debug scenario)"
"$AUTO_UI_BIN" run \
  $HEADLESS_FLAG \
  --config "$APP_ROOT/tests/auto-ui/fast.toml" \
  --output-dir "$OUT_DIR/debug"

echo "[auto-ui-test] running fast-header.toml (header_debug scenario)"
"$AUTO_UI_BIN" run \
  $HEADLESS_FLAG \
  --config "$APP_ROOT/tests/auto-ui/fast-header.toml" \
  --output-dir "$OUT_DIR/header"

echo "[auto-ui-test] running workspace picker Enter scenario"
KEEP_HOME=0 KEEP_OUT=1 OUT_DIR="$OUT_DIR/workspace" USE_REAL_DISPLAY="${USE_REAL_DISPLAY:-0}" \
  "$APP_ROOT/scripts/auto-ui-test-workspace.sh"

if [[ "${MULTI_WIDTH:-0}" == "1" ]]; then
  echo "[auto-ui-test] running multi-width.toml (3 widths × 7 sessions)"
  "$AUTO_UI_BIN" run \
    $HEADLESS_FLAG \
    --config "$APP_ROOT/tests/auto-ui/multi-width.toml" \
    --output-dir "$OUT_DIR/multi-width"
fi

if [[ "${SKIP_ASSERT:-0}" != "1" ]]; then
  echo "[auto-ui-test] asserting fast.toml report"
  "$APP_ROOT/scripts/auto-ui-assertions.py" \
    "$OUT_DIR/debug/report.json" --home "$TMP_HOME"
  if [[ "${MULTI_WIDTH:-0}" == "1" ]]; then
    echo "[auto-ui-test] asserting multi-width.toml report"
    "$APP_ROOT/scripts/auto-ui-assertions.py" \
      "$OUT_DIR/multi-width/report.json" --home "$TMP_HOME" --multi-width
  fi
fi

end=$(date +%s)
echo "[auto-ui-test] total: $((end - start))s"
