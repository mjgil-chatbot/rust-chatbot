#!/usr/bin/env bash
set -euo pipefail

provider="${1:?provider is required}"
phase="${2:?phase is required}"

uid="$(id -u)"
export DISPLAY="${DISPLAY:-:1}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/${uid}}"
export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=${XDG_RUNTIME_DIR}/bus}"

kill_count=0

kill_provider_instances() {
  local provider_flag
  case "${provider}" in
    "MiniMax") provider_flag="--minimax" ;;
    *) return 0 ;;
  esac

  local pids=()
  while IFS= read -r line; do
    [ -n "${line}" ] || continue
    local pid="${line%% *}"
    local args="${line#* }"
    case "${args}" in
      *rust-chatbot*"${provider_flag}"*)
        pids+=("${pid}")
        ;;
    esac
  done < <(ps -eo pid=,args=)

  [ "${#pids[@]}" -gt 0 ] || return 0

  kill "${pids[@]}" 2>/dev/null || true
  sleep 2

  local survivors=()
  local pid
  for pid in "${pids[@]}"; do
    if kill -0 "${pid}" 2>/dev/null; then
      survivors+=("${pid}")
    fi
  done

  if [ "${#survivors[@]}" -gt 0 ]; then
    kill -9 "${survivors[@]}" 2>/dev/null || true
  fi

  kill_count="${#pids[@]}"
}

case "${provider}:${phase}" in
  "MiniMax:start")
    title="MiniMax peak started"
    body="MiniMax peak window is active until 5:30 AM ET."
    ;;
  "MiniMax:stop")
    title="MiniMax peak ended"
    body="MiniMax peak window ended at 5:30 AM ET."
    ;;
  *)
    echo "unsupported provider/phase: ${provider} ${phase}" >&2
    exit 1
    ;;
esac

if [ "${phase}" = "start" ]; then
  kill_provider_instances
  if [ "${kill_count}" -gt 0 ]; then
    body="${body} Stopped ${kill_count} running ${provider} instance(s)."
  fi
fi

exec /usr/bin/notify-send \
  --app-name="Peak Windows" \
  --urgency=normal \
  --expire-time=30000 \
  "${title}" \
  "${body}"
