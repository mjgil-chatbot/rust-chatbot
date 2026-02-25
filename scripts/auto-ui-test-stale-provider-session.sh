#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APP_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
PROMPT_DEBUG_ASSERT_SCRIPT="${APP_ROOT}/scripts/auto-ui-test-prompt-debug.sh"

PROVIDER="${PROVIDER:-minimax}"
SESSION_ID="${SESSION_ID:-}"
SESSION_JSON="${SESSION_JSON:-}"
PROMPT="${PROMPT:-Reply with STALE-RESUME-OK and nothing else.}"
OUTPUT_DIR="${OUTPUT_DIR:-}"
TRACE_TIMEOUT="${TRACE_TIMEOUT:-}"
STALE_PROVIDER_SESSION_ID="${STALE_PROVIDER_SESSION_ID:-}"
REQUIRED_SUBSTRINGS=()
WORK_DIR_CREATED=0

usage() {
  cat <<'EOF'
Usage:
  auto-ui-test-stale-provider-session.sh [options]

Options:
  --session-id <id>                 Target rust-chatbot session id.
  --session-json <path>             Saved session JSON to modify and verify.
  --provider <name>                 Provider name. Default: minimax
  --prompt <text>                   Prompt to send through prompt_debug.
  --output-dir <path>               Output directory for auto-ui artifacts.
  --trace-timeout <sec>             Override prompt_debug trace timeout.
  --stale-provider-session-id <id>  Synthetic stale provider session id to seed.
  --require-substring <txt>         Required substring in final assistant message. Repeatable.
  --help                            Show this help.

Environment overrides:
  PROVIDER, SESSION_ID, SESSION_JSON, PROMPT, OUTPUT_DIR, TRACE_TIMEOUT,
  STALE_PROVIDER_SESSION_ID
EOF
}

while (($# > 0)); do
  case "$1" in
    --session-id)
      SESSION_ID="${2:-}"
      shift 2
      ;;
    --session-json)
      SESSION_JSON="${2:-}"
      shift 2
      ;;
    --provider)
      PROVIDER="${2:-}"
      shift 2
      ;;
    --prompt)
      PROMPT="${2:-}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --trace-timeout)
      TRACE_TIMEOUT="${2:-}"
      shift 2
      ;;
    --stale-provider-session-id)
      STALE_PROVIDER_SESSION_ID="${2:-}"
      shift 2
      ;;
    --require-substring)
      REQUIRED_SUBSTRINGS+=("${2:-}")
      shift 2
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
  exit 1
fi

if [[ -z "${SESSION_JSON}" ]]; then
  printf 'Missing --session-json.\n' >&2
  exit 1
fi

if [[ ! -f "${SESSION_JSON}" ]]; then
  printf 'Session JSON does not exist: %s\n' "${SESSION_JSON}" >&2
  exit 1
fi

if [[ -z "${OUTPUT_DIR}" ]]; then
  OUTPUT_DIR="$(mktemp -d -t rust-chatbot-stale-resume-XXXXXX)"
  WORK_DIR_CREATED=1
else
  mkdir -p "${OUTPUT_DIR}"
fi

backup_json="$(mktemp "${TMPDIR:-/tmp}/rust-chatbot-session-backup.XXXXXX.json")"
cp "${SESSION_JSON}" "${backup_json}"
restore_on_exit=1

cleanup() {
  if [[ "${restore_on_exit}" == "1" && -f "${backup_json}" ]]; then
    cp "${backup_json}" "${SESSION_JSON}"
  fi
  rm -f "${backup_json}"
  if [[ "${WORK_DIR_CREATED}" == "1" && "${KEEP_OUT:-0}" != "1" ]]; then
    rm -rf "${OUTPUT_DIR}"
  fi
}
trap cleanup EXIT

if [[ -z "${STALE_PROVIDER_SESSION_ID}" ]]; then
  STALE_PROVIDER_SESSION_ID="stale-$(python3 - <<'PY'
import uuid
print(uuid.uuid4().hex)
PY
)"
fi

mapfile -t seed_result < <(
  SESSION_JSON_PATH="${SESSION_JSON}" \
  PROVIDER_NAME="${PROVIDER}" \
  STALE_SESSION_ID="${STALE_PROVIDER_SESSION_ID}" \
  python3 - <<'PY'
import json
import os
import pathlib

path = pathlib.Path(os.environ["SESSION_JSON_PATH"])
provider = os.environ["PROVIDER_NAME"].strip().lower()
stale = os.environ["STALE_SESSION_ID"].strip()
field_map = {
    "claude": "claude_session_id",
    "minimax": "minimax_session_id",
    "minimaxforge": "minimax_session_id",
    "qwen": "qwen_session_id",
    "codex": "codex_session_id",
    "nvidia": "nvidia_session_id",
    "gemini": "gemini_session_id",
    "openrouter": "openrouter_session_id",
    "omniforge": "omniforge_session_id",
    "kimi": "kimi_session_id",
    "hermes": "hermes_session_id",
}
field = field_map.get(provider)
if field is None:
    raise SystemExit(f"Unsupported provider for stale-session test: {provider}")

payload = json.loads(path.read_text(encoding="utf-8"))
previous = payload.get(field) or ""
payload[field] = stale
path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
print(field)
print(previous)
PY
)

provider_field="${seed_result[0]}"
previous_provider_session_id="${seed_result[1]:-}"

printf 'Seeded %s with stale provider session id %s (previous=%s)\n' \
  "${provider_field}" \
  "${STALE_PROVIDER_SESSION_ID}" \
  "${previous_provider_session_id:-<none>}"

prompt_debug_args=(
  "${PROMPT_DEBUG_ASSERT_SCRIPT}"
  --session-id "${SESSION_ID}"
  --session-json "${SESSION_JSON}"
  --provider "${PROVIDER}"
  --prompt "${PROMPT}"
  --output-dir "${OUTPUT_DIR}"
)
if [[ -n "${TRACE_TIMEOUT}" ]]; then
  prompt_debug_args+=(--trace-timeout "${TRACE_TIMEOUT}")
fi
for required in "${REQUIRED_SUBSTRINGS[@]}"; do
  prompt_debug_args+=(--require-substring "${required}")
done

"${prompt_debug_args[@]}"

mapfile -t verify_result < <(
  SESSION_JSON_PATH="${SESSION_JSON}" \
  PROVIDER_FIELD="${provider_field}" \
  STALE_SESSION_ID="${STALE_PROVIDER_SESSION_ID}" \
  python3 - <<'PY'
import json
import os
import pathlib

path = pathlib.Path(os.environ["SESSION_JSON_PATH"])
field = os.environ["PROVIDER_FIELD"]
stale = os.environ["STALE_SESSION_ID"]
payload = json.loads(path.read_text(encoding="utf-8"))
current = (payload.get(field) or "").strip()
if not current:
    raise SystemExit(f"{field} is still empty after prompt_debug")
if current == stale:
    raise SystemExit(f"{field} was not replaced; still {current}")
print(current)
PY
)

new_provider_session_id="${verify_result[0]}"
restore_on_exit=0

printf 'PASS: %s replaced stale provider session id %s -> %s\n' \
  "${provider_field}" \
  "${STALE_PROVIDER_SESSION_ID}" \
  "${new_provider_session_id}"
printf 'Artifacts: %s\n' "${OUTPUT_DIR}"
