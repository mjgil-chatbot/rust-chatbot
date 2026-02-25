#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APP_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
PROMPT_DEBUG_SCRIPT="${APP_ROOT}/scripts/auto_ui_prompt_debug.sh"
ASSERT_SCRIPT="${APP_ROOT}/scripts/auto-ui-assertions.py"
PROVIDER="${PROVIDER:-minimaxforge}"
SESSION_ID="${SESSION_ID:-}"
SESSION_JSON="${SESSION_JSON:-}"
PROMPT="${PROMPT:-:muse create a short rollout plan and include a fenced rust code block with fn main() {}}"
OUTPUT_DIR="${OUTPUT_DIR:-}"
TRACE_TIMEOUT="${TRACE_TIMEOUT:-}"
ASSERT_ONLY=0
REQUIRED_SUBSTRINGS=()
REQUIRED_TOOL_TRANSCRIPT_COUNT="${REQUIRED_TOOL_TRANSCRIPT_COUNT:-0}"
REQUIRED_TOOL_TRANSCRIPT_SUBSTRINGS=()
FORBIDDEN_TOOL_TRANSCRIPT_SUBSTRINGS=()

usage() {
  cat <<'EOF'
Usage:
  auto-ui-test-prompt-debug.sh [options]

Options:
  --session-id <id>          Target rust-chatbot session id.
  --session-json <path>      Saved session JSON to validate.
  --provider <name>          Provider name. Default: minimaxforge
  --prompt <text>            Prompt to send for prompt_debug.
  --output-dir <path>        Output directory for auto-ui artifacts.
  --trace-timeout <sec>      Override prompt_debug trace timeout.
  --require-substring <txt>  Required substring in final assistant message. Repeatable.
  --require-tool-transcript-count <n>
                             Minimum number of transcript-style tool lines required.
  --require-tool-transcript-substring <txt>
                             Required substring in transcript-style tool lines. Repeatable.
  --forbid-tool-transcript-substring <txt>
                             Forbidden substring in transcript-style tool lines. Repeatable.
  --assert-only              Skip launching auto-ui; only assert an existing report/session pair.
  --help                     Show this help.

Environment overrides:
  PROVIDER, SESSION_ID, SESSION_JSON, PROMPT, OUTPUT_DIR, TRACE_TIMEOUT
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
    --require-substring)
      REQUIRED_SUBSTRINGS+=("${2:-}")
      shift 2
      ;;
    --require-tool-transcript-count)
      REQUIRED_TOOL_TRANSCRIPT_COUNT="${2:-}"
      shift 2
      ;;
    --require-tool-transcript-substring)
      REQUIRED_TOOL_TRANSCRIPT_SUBSTRINGS+=("${2:-}")
      shift 2
      ;;
    --forbid-tool-transcript-substring)
      FORBIDDEN_TOOL_TRANSCRIPT_SUBSTRINGS+=("${2:-}")
      shift 2
      ;;
    --assert-only)
      ASSERT_ONLY=1
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

if [[ -z "${OUTPUT_DIR}" ]]; then
  OUTPUT_DIR="$(mktemp -d -t rust-chatbot-prompt-debug-XXXXXX)"
  created_output_dir=1
else
  mkdir -p "${OUTPUT_DIR}"
  created_output_dir=0
fi

cleanup() {
  if [[ "${created_output_dir:-0}" == "1" && "${KEEP_OUT:-0}" != "1" ]]; then
    rm -rf "${OUTPUT_DIR}"
  fi
}
trap cleanup EXIT

report_json="${OUTPUT_DIR}/report.json"

if [[ "${ASSERT_ONLY}" != "1" ]]; then
  if [[ -z "${SESSION_ID}" ]]; then
    printf 'Missing --session-id for live prompt_debug run.\n' >&2
    exit 1
  fi
  if [[ -z "${SESSION_JSON}" ]]; then
    printf 'Missing --session-json for live prompt_debug run.\n' >&2
    exit 1
  fi
  prompt_debug_args=(
    "${PROMPT_DEBUG_SCRIPT}"
    --session-id "${SESSION_ID}"
    --provider "${PROVIDER}"
    --prompt "${PROMPT}"
    --output-dir "${OUTPUT_DIR}"
  )
  if [[ -n "${TRACE_TIMEOUT}" ]]; then
    prompt_debug_args+=(--trace-timeout "${TRACE_TIMEOUT}")
  fi
  "${prompt_debug_args[@]}"
else
  if [[ -z "${SESSION_JSON}" ]]; then
    printf 'Missing --session-json for --assert-only mode.\n' >&2
    exit 1
  fi
  if [[ ! -f "${report_json}" ]]; then
    printf 'Missing report.json in output dir for --assert-only mode: %s\n' "${report_json}" >&2
    exit 1
  fi
fi

assert_args=("${ASSERT_SCRIPT}" "${report_json}" "--prompt-debug-session-json" "${SESSION_JSON}")
for required in "${REQUIRED_SUBSTRINGS[@]}"; do
  assert_args+=("--require-substring" "${required}")
done
if [[ "${REQUIRED_TOOL_TRANSCRIPT_COUNT}" != "0" ]]; then
  assert_args+=("--require-tool-transcript-count" "${REQUIRED_TOOL_TRANSCRIPT_COUNT}")
fi
for required in "${REQUIRED_TOOL_TRANSCRIPT_SUBSTRINGS[@]}"; do
  assert_args+=("--require-tool-transcript-substring" "${required}")
done
for forbidden in "${FORBIDDEN_TOOL_TRANSCRIPT_SUBSTRINGS[@]}"; do
  assert_args+=("--forbid-tool-transcript-substring" "${forbidden}")
done

python3 "${assert_args[@]}"
printf 'prompt_debug assertions passed: %s\n' "${report_json}"
