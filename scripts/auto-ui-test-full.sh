#!/usr/bin/env bash
# Full auto-ui suite for rust-chatbot.
#
# Adds the prompt_debug scenario (45s alone) on top of the fast suite.
# Expect this to run 2-4 minutes. Use auto-ui-test.sh for the budgeted path.
#
# Env knobs match auto-ui-test.sh, plus:
#   USE_REAL_DISPLAY=1         disable headless mode and run on $DISPLAY directly (visual debug)
#   FIXTURE_PROMPT_DEBUG_PROVIDER provider for the headless fixture-backed prompt_debug run
#   FIXTURE_PROMPT_DEBUG_SESSION_ID session id for the headless fixture-backed prompt_debug run
#   FIXTURE_PROMPT_DEBUG_PROMPT prompt text for the headless fixture-backed prompt_debug run
#   FIXTURE_PROMPT_DEBUG_REQUIRE_SUBSTRING optional substring assertion for the headless fixture-backed prompt_debug run
#   FIXTURE_PROMPT_DEBUG_TRACE_TIMEOUT response trace timeout for the headless fixture-backed prompt_debug run
#   PROMPT_DEBUG_SESSION_ID     when set, run the real prompt-debug verification wrapper too
#   PROMPT_DEBUG_SESSION_JSON   saved session JSON for the real prompt-debug verification
#   PROMPT_DEBUG_PROVIDER       provider for the real prompt-debug verification
#   PROMPT_DEBUG_PROMPT         prompt text for the real prompt-debug verification wrapper
#   PROMPT_DEBUG_TRACE_TIMEOUT  response trace timeout for the real prompt-debug verification
#   PROMPT_DEBUG_REQUIRE_SUBSTRING  repeatable substring check is not supported here;
#                                   set a single required substring via env if needed
#   PROMPT_DEBUG_REQUIRE_TOOL_TRANSCRIPT_COUNT
#                               minimum number of transcript-style tool lines required
#   PROMPT_DEBUG_FORBID_TOOL_TRANSCRIPT_SUBSTRING
#                               optional transcript substring that must not appear
#   SKIP_FAST=1                 skip the fast suite when focusing only on prompt_debug
#   SKIP_FIXTURE_PROMPT_DEBUG=1 skip the fixture-backed prompt_debug stage and only run the
#                               real prompt_debug verification wrapper when configured

set -euo pipefail

APP_ROOT="${APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
REAL_HOME="${HOME}"
FIXTURE_PROMPT_DEBUG_PROVIDER="${FIXTURE_PROMPT_DEBUG_PROVIDER:-codex}"
FIXTURE_PROMPT_DEBUG_SESSION_ID="${FIXTURE_PROMPT_DEBUG_SESSION_ID:-fixture-pl-update}"
FIXTURE_PROMPT_DEBUG_PROMPT="${FIXTURE_PROMPT_DEBUG_PROMPT:-render a short markdown table and a fenced code block}"
FIXTURE_PROMPT_DEBUG_REQUIRE_SUBSTRING="${FIXTURE_PROMPT_DEBUG_REQUIRE_SUBSTRING:-}"
FIXTURE_PROMPT_DEBUG_TRACE_TIMEOUT="${FIXTURE_PROMPT_DEBUG_TRACE_TIMEOUT:-45}"
PROMPT_DEBUG_PROVIDER="${PROMPT_DEBUG_PROVIDER:-minimaxforge}"
PROMPT_DEBUG_SESSION_ID="${PROMPT_DEBUG_SESSION_ID:-}"
PROMPT_DEBUG_SESSION_JSON="${PROMPT_DEBUG_SESSION_JSON:-}"
PROMPT_DEBUG_PROMPT="${PROMPT_DEBUG_PROMPT:-}"
PROMPT_DEBUG_TRACE_TIMEOUT="${PROMPT_DEBUG_TRACE_TIMEOUT:-}"
PROMPT_DEBUG_REQUIRE_SUBSTRING="${PROMPT_DEBUG_REQUIRE_SUBSTRING:-fn main()}"
PROMPT_DEBUG_REQUIRE_TOOL_TRANSCRIPT_COUNT="${PROMPT_DEBUG_REQUIRE_TOOL_TRANSCRIPT_COUNT:-0}"
PROMPT_DEBUG_FORBID_TOOL_TRANSCRIPT_SUBSTRING="${PROMPT_DEBUG_FORBID_TOOL_TRANSCRIPT_SUBSTRING:-}"
SKIP_FIXTURE_PROMPT_DEBUG="${SKIP_FIXTURE_PROMPT_DEBUG:-0}"

# Run the fast suite first (covers debug + header_debug).
if [[ "${SKIP_FAST:-0}" != "1" ]]; then
  "$APP_ROOT/scripts/auto-ui-test.sh"
fi

AUTO_UI_BIN="${AUTO_UI_BIN:-$HOME/git/auto-ui/target/release/auto-ui}"
FIXTURE_SRC="$APP_ROOT/tests/auto-ui/fixtures/home"

if [[ ! -x "$AUTO_UI_BIN" ]]; then
  echo "auto-ui binary not found at $AUTO_UI_BIN" >&2
  exit 1
fi

# Match the fast suite's display behavior so prompt_debug can run in managed
# headless mode under Xvfb + openbox when no real desktop session is available.
HEADLESS_FLAG=""
if [[ "${USE_REAL_DISPLAY:-0}" == "1" ]]; then
  if [[ -z "${DISPLAY:-}" ]]; then
    echo 'USE_REAL_DISPLAY=1 set but $DISPLAY is empty.' >&2
    exit 1
  fi
  echo "[auto-ui-test-full] running on real display \$DISPLAY=$DISPLAY (focus may be disrupted)"
else
  HEADLESS_FLAG="--headless"
  echo "[auto-ui-test-full] running headless (Xvfb + openbox managed by auto-ui)"
fi

# prompt_debug needs an existing session_id; use chat-alpha from the fixture.
TMP_HOME="$(mktemp -d -t rust-chatbot-auto-ui-full-home-XXXXXX)"
OUT_DIR="$(mktemp -d -t rust-chatbot-auto-ui-full-out-XXXXXX)"
trap 'rm -rf "$TMP_HOME" "$OUT_DIR"' EXIT
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
if [[ -f "$REAL_HOME/.rust-chatbot/.gemini/oauth_creds.json" ]]; then
  mkdir -p "$TMP_HOME/.rust-chatbot/.gemini"
  mkdir -p "$TMP_HOME/.gemini"
  cp \
    "$REAL_HOME/.rust-chatbot/.gemini/oauth_creds.json" \
    "$TMP_HOME/.rust-chatbot/.gemini/oauth_creds.json"
  cp \
    "$REAL_HOME/.rust-chatbot/.gemini/oauth_creds.json" \
    "$TMP_HOME/.gemini/oauth_creds.json"
fi
if [[ -f "$REAL_HOME/.rust-chatbot/.gemini/api.key" ]]; then
  mkdir -p "$TMP_HOME/.rust-chatbot/.gemini"
  mkdir -p "$TMP_HOME/.gemini"
  cp \
    "$REAL_HOME/.rust-chatbot/.gemini/api.key" \
    "$TMP_HOME/.rust-chatbot/.gemini/api.key"
  cp \
    "$REAL_HOME/.rust-chatbot/.gemini/api.key" \
    "$TMP_HOME/.gemini/api.key"
fi
for gemini_state_file in \
  settings.json \
  google_accounts.json \
  projects.json \
  installation_id \
  trustedFolders.json; do
  if [[ -f "$REAL_HOME/.gemini/$gemini_state_file" ]]; then
    mkdir -p "$TMP_HOME/.gemini"
    cp \
      "$REAL_HOME/.gemini/$gemini_state_file" \
      "$TMP_HOME/.gemini/$gemini_state_file"
  fi
done
if [[ -d "$REAL_HOME/.rust-chatbot/.forge" ]]; then
  cp -a \
    "$REAL_HOME/.rust-chatbot/.forge" \
    "$TMP_HOME/.rust-chatbot/.forge"
fi
if [[ -f "$REAL_HOME/.config/gcloud/application_default_credentials.json" ]]; then
  mkdir -p "$TMP_HOME/.config/gcloud"
  cp \
    "$REAL_HOME/.config/gcloud/application_default_credentials.json" \
    "$TMP_HOME/.config/gcloud/application_default_credentials.json"
fi
for gcloud_state_file in \
  active_config \
  access_tokens.db \
  config_sentinel \
  credentials.db \
  default_configs.db; do
  if [[ -f "$REAL_HOME/.config/gcloud/$gcloud_state_file" ]]; then
    mkdir -p "$TMP_HOME/.config/gcloud"
    cp \
      "$REAL_HOME/.config/gcloud/$gcloud_state_file" \
      "$TMP_HOME/.config/gcloud/$gcloud_state_file"
  fi
done
if [[ -f "$REAL_HOME/.config/gcloud/credentials.db" ]]; then
  :
fi
if [[ -f "$REAL_HOME/.config/gcloud/configurations/config_default" ]]; then
  mkdir -p "$TMP_HOME/.config/gcloud/configurations"
  cp \
    "$REAL_HOME/.config/gcloud/configurations/config_default" \
    "$TMP_HOME/.config/gcloud/configurations/config_default"
fi
if [[ -d "$REAL_HOME/.config/gcloud/legacy_credentials" ]]; then
  mkdir -p "$TMP_HOME/.config/gcloud"
  cp -a \
    "$REAL_HOME/.config/gcloud/legacy_credentials" \
    "$TMP_HOME/.config/gcloud/legacy_credentials"
fi
log_dir="$TMP_HOME/.local/state/rust-chatbot"
mkdir -p "$log_dir"
: > "$log_dir/rust-chatbot.log.$(date -u +%Y-%m-%d)"
: > "$log_dir/rust-chatbot.log.$(date -u -d 'yesterday' +%Y-%m-%d)"

PROMPT_TOML="$(mktemp -t rust-chatbot-prompt-XXXXXX.toml)"
trap 'rm -f "$PROMPT_TOML"' EXIT
cat >"$PROMPT_TOML" <<TOML
target = "rust_chatbot"
scenario = "prompt_debug"

[app]
root = "$APP_ROOT"
provider = "$FIXTURE_PROMPT_DEBUG_PROVIDER"
session_id = "$FIXTURE_PROMPT_DEBUG_SESSION_ID"

[window]
width = 900
height = 700
keep_front = false

[prompt]
text = "$FIXTURE_PROMPT_DEBUG_PROMPT"

[runtime]
window_timeout = 10
trace_timeout = $FIXTURE_PROMPT_DEBUG_TRACE_TIMEOUT
settle = 0.5
TOML

if [[ "$SKIP_FIXTURE_PROMPT_DEBUG" != "1" ]]; then
  echo "[auto-ui-test-full] running prompt_debug"
  "$AUTO_UI_BIN" run $HEADLESS_FLAG --config "$PROMPT_TOML" --output-dir "$OUT_DIR/prompt"

  case "$FIXTURE_PROMPT_DEBUG_PROVIDER" in
    claude) fixture_provider_dir=".claude-desktop" ;;
    codex) fixture_provider_dir=".codex-desktop" ;;
    gemini) fixture_provider_dir=".gemini-desktop" ;;
    geminiforge) fixture_provider_dir=".gemini-forge-desktop" ;;
    geminicliforge) fixture_provider_dir=".gemini-cli-forge-desktop" ;;
    minimax) fixture_provider_dir=".minimax-desktop" ;;
    minimaxforge) fixture_provider_dir=".minimax-forge-desktop" ;;
    omniforge) fixture_provider_dir=".omniforge-desktop" ;;
    *)
      echo "Unsupported FIXTURE_PROMPT_DEBUG_PROVIDER: $FIXTURE_PROMPT_DEBUG_PROVIDER" >&2
      exit 1
      ;;
  esac

  fixture_prompt_session_json="$TMP_HOME/.rust-chatbot/$fixture_provider_dir/sessions/$FIXTURE_PROMPT_DEBUG_SESSION_ID.json"
  prompt_assert_args=(
    "$APP_ROOT/scripts/auto-ui-assertions.py"
    "$OUT_DIR/prompt/report.json"
    "--prompt-debug-session-json"
    "$fixture_prompt_session_json"
  )
  if [[ -n "$FIXTURE_PROMPT_DEBUG_REQUIRE_SUBSTRING" ]]; then
    prompt_assert_args+=("--require-substring" "$FIXTURE_PROMPT_DEBUG_REQUIRE_SUBSTRING")
  fi
  python3 "${prompt_assert_args[@]}"
else
  echo "[auto-ui-test-full] skipping fixture prompt_debug"
fi

if [[ -n "$PROMPT_DEBUG_SESSION_ID" || -n "$PROMPT_DEBUG_SESSION_JSON" ]]; then
  if [[ -z "$PROMPT_DEBUG_SESSION_ID" || -z "$PROMPT_DEBUG_SESSION_JSON" ]]; then
    echo "PROMPT_DEBUG_SESSION_ID and PROMPT_DEBUG_SESSION_JSON must both be set for real prompt_debug verification" >&2
    exit 1
  fi
  echo "[auto-ui-test-full] running real prompt_debug verification wrapper"
  (
    export HOME="$REAL_HOME"
    real_prompt_args=(
      "$APP_ROOT/scripts/auto-ui-test-prompt-debug.sh"
      --session-id "$PROMPT_DEBUG_SESSION_ID"
      --session-json "$PROMPT_DEBUG_SESSION_JSON"
      --provider "$PROMPT_DEBUG_PROVIDER"
      --require-substring "$PROMPT_DEBUG_REQUIRE_SUBSTRING"
    )
    if [[ -n "$PROMPT_DEBUG_PROMPT" ]]; then
      real_prompt_args+=(--prompt "$PROMPT_DEBUG_PROMPT")
    fi
    if [[ -n "$PROMPT_DEBUG_TRACE_TIMEOUT" ]]; then
      real_prompt_args+=(--trace-timeout "$PROMPT_DEBUG_TRACE_TIMEOUT")
    fi
    if [[ "$PROMPT_DEBUG_REQUIRE_TOOL_TRANSCRIPT_COUNT" != "0" ]]; then
      real_prompt_args+=(
        --require-tool-transcript-count
        "$PROMPT_DEBUG_REQUIRE_TOOL_TRANSCRIPT_COUNT"
      )
    fi
    if [[ -n "$PROMPT_DEBUG_FORBID_TOOL_TRANSCRIPT_SUBSTRING" ]]; then
      real_prompt_args+=(
        --forbid-tool-transcript-substring
        "$PROMPT_DEBUG_FORBID_TOOL_TRANSCRIPT_SUBSTRING"
      )
    fi
    "${real_prompt_args[@]}"
  )
fi
