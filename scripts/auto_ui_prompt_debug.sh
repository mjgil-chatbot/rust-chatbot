#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APP_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
AUTO_UI_ROOT="${AUTO_UI_ROOT:-${APP_ROOT}/../auto-ui}"
AUTO_UI_BIN="${AUTO_UI_BIN:-${AUTO_UI_ROOT}/target/release/auto-ui}"
PROVIDER="${PROVIDER:-codex}"
SESSION_ID="${SESSION_ID:-}"
PROMPT="${PROMPT:-Generate about 6 KB of markdown with headings, bullet lists, a small table, and one fenced code block. Output only the markdown.}"
WIDTH="${WIDTH:-700}"
HEIGHT="${HEIGHT:-900}"
TRACE_TIMEOUT="${TRACE_TIMEOUT:-90.0}"
SETTLE="${SETTLE:-0.8}"
KEEP_FRONT="${KEEP_FRONT:-false}"
OUTPUT_DIR="${OUTPUT_DIR:-}"

usage() {
  cat <<'EOF'
Usage:
  auto_ui_prompt_debug.sh --session-id <session-id> [options]

Options:
  --session-id <id>      Target rust-chatbot session id. Required unless SESSION_ID is set.
  --prompt <text>        Prompt to send. Defaults to the markdown test prompt.
  --provider <name>      Provider name. Default: codex
  --width <px>           Window width. Default: 700
  --height <px>          Window height. Default: 900
  --trace-timeout <sec>  Prompt debug trace timeout. Default: 90.0
  --settle <sec>         Launch settle delay. Default: 0.8
  --output-dir <path>    Output directory for auto-ui artifacts.
  --keep-front           Keep the automation window in the foreground.
  --help                 Show this help.

Environment overrides:
  AUTO_UI_ROOT  Path to the auto-ui repo. Default: ${APP_ROOT}/../auto-ui
  AUTO_UI_BIN   Path to the auto-ui binary. Default: $AUTO_UI_ROOT/target/release/auto-ui
EOF
}

while (($# > 0)); do
  case "$1" in
    --session-id)
      SESSION_ID="${2:-}"
      shift 2
      ;;
    --prompt)
      PROMPT="${2:-}"
      shift 2
      ;;
    --provider)
      PROVIDER="${2:-}"
      shift 2
      ;;
    --width)
      WIDTH="${2:-}"
      shift 2
      ;;
    --height)
      HEIGHT="${2:-}"
      shift 2
      ;;
    --trace-timeout)
      TRACE_TIMEOUT="${2:-}"
      shift 2
      ;;
    --settle)
      SETTLE="${2:-}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --keep-front)
      KEEP_FRONT="true"
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "${SESSION_ID}" ]]; then
  printf 'Missing --session-id.\n' >&2
  usage >&2
  exit 1
fi

if [[ ! -x "${AUTO_UI_BIN}" ]]; then
  printf 'auto-ui binary is missing or not executable: %s\n' "${AUTO_UI_BIN}" >&2
  exit 1
fi

readarray -t desktop_env < <(
  APP_ROOT_ENV="${APP_ROOT}" PROVIDER_ENV="${PROVIDER}" python3 - <<'PY'
import os
import pathlib
import subprocess
import sys

app_root = os.environ["APP_ROOT_ENV"]
provider = os.environ["PROVIDER_ENV"].strip().lower()
profiles = ["release", "debug"]
pids = []

search_terms = []
for profile in profiles:
    search_terms.append(f"{app_root}/target/{profile}/rust-chatbot")
if provider:
    search_terms.append(f"rust-chatbot-{provider}")

pids = []
for target in search_terms:
    try:
        found = subprocess.check_output(["pgrep", "-f", target], text=True).split()
        pids.extend(found)
    except subprocess.CalledProcessError:
        pass
pids = list(dict.fromkeys(pids))  # deduplicate, preserve order

for pid in pids:
    try:
        data = pathlib.Path(f"/proc/{pid}/environ").read_bytes().split(b"\0")
    except OSError:
        continue
    env = {}
    for item in data:
        if b"=" not in item:
            continue
        key, value = item.split(b"=", 1)
        env[key.decode(errors="ignore")] = value.decode(errors="ignore")
    display = env.get("DISPLAY")
    if not display:
        continue
    print(display)
    print(env.get("XAUTHORITY", ""))
    print(env.get("XDG_RUNTIME_DIR", ""))
    sys.exit(0)

sys.exit(1)
PY
)

if ((${#desktop_env[@]} < 3)); then
  printf 'Could not recover DISPLAY/XAUTHORITY/XDG_RUNTIME_DIR from a live rust-chatbot process.\n' >&2
  exit 1
fi

DISPLAY_VALUE="${desktop_env[0]}"
XAUTHORITY_VALUE="${desktop_env[1]}"
XDG_RUNTIME_DIR_VALUE="${desktop_env[2]}"

tmp_config="$(mktemp "${TMPDIR:-/tmp}/auto-ui-prompt-debug.XXXXXX.toml")"
cleanup() {
  rm -f -- "${tmp_config}"
}
trap cleanup EXIT

CONFIG_PATH="${tmp_config}" \
PROVIDER_VALUE="${PROVIDER}" \
APP_ROOT_VALUE="${APP_ROOT}" \
SESSION_ID_VALUE="${SESSION_ID}" \
PROMPT_VALUE="${PROMPT}" \
WIDTH_VALUE="${WIDTH}" \
HEIGHT_VALUE="${HEIGHT}" \
TRACE_TIMEOUT_VALUE="${TRACE_TIMEOUT}" \
SETTLE_VALUE="${SETTLE}" \
KEEP_FRONT_VALUE="${KEEP_FRONT}" \
python3 - <<'PY'
import json
import os
import pathlib

config_path = pathlib.Path(os.environ["CONFIG_PATH"])
payload = {
    "provider": os.environ["PROVIDER_VALUE"],
    "app_root": os.environ["APP_ROOT_VALUE"],
    "session_id": os.environ["SESSION_ID_VALUE"],
    "prompt": os.environ["PROMPT_VALUE"],
    "width": int(os.environ["WIDTH_VALUE"]),
    "height": int(os.environ["HEIGHT_VALUE"]),
    "trace_timeout": float(os.environ["TRACE_TIMEOUT_VALUE"]),
    "settle": float(os.environ["SETTLE_VALUE"]),
    "keep_front": os.environ["KEEP_FRONT_VALUE"].lower() == "true",
}

def toml_string(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    escaped = escaped.replace("\n", "\\n")
    return f'"{escaped}"'

lines = [
    f'target = "rust_chatbot"',
    f'scenario = "prompt_debug"',
    "",
    "[app]",
    f'provider = {toml_string(payload["provider"])}',
    f'root = {toml_string(payload["app_root"])}',
    f'session_id = {toml_string(payload["session_id"])}',
    "",
    "[window]",
    f'width = {payload["width"]}',
    f'height = {payload["height"]}',
    f'keep_front = {"true" if payload["keep_front"] else "false"}',
    "",
    "[prompt]",
    f'text = {toml_string(payload["prompt"])}',
    "",
    "[runtime]",
    f'trace_timeout = {payload["trace_timeout"]}',
    f'settle = {payload["settle"]}',
    "",
]
config_path.write_text("\n".join(lines))
PY

printf 'Running auto-ui prompt_debug for session %s\n' "${SESSION_ID}"
printf 'Using config %s\n' "${tmp_config}"

auto_ui_args=(run --config "${tmp_config}")
if [[ -n "${OUTPUT_DIR}" ]]; then
  auto_ui_args+=(--output-dir "${OUTPUT_DIR}")
fi

(
  export DISPLAY="${DISPLAY_VALUE}"
  export XAUTHORITY="${XAUTHORITY_VALUE}"
  export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR_VALUE}"
  export AUTO_UI_LAUNCH_BACKGROUND=1
  exec "${AUTO_UI_BIN}" "${auto_ui_args[@]}"
)
