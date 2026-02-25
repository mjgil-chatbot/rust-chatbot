#!/usr/bin/env bash
set -euo pipefail

# Backup LLM chatbot data + logs.
#
# Data backups: timestamped archives, 3-day local rotation, synced to GCS.
# Live sessions: synced to GCS as raw files for direct restore/inspection.
# Logs: uploaded to GCS (kept forever), 3-day local retention.
#
# Sources:
#   1. ~/.rust-chatbot/.*-desktop/  (provider session data under the shared runtime root)
#   2. <repo>/.claude-desktop/      (repo-local Claude Code project data)
#   3. platform log directory       (application logs)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="${RUST_CHATBOT_REPO_DIR:-$(cd "${SCRIPT_DIR}/.." && pwd)}"
BACKUP_ROOT="${RUST_CHATBOT_BACKUP_ROOT:-${HOME}/Documents/llm-chatbot-zipped-files}"
SHARED_RUNTIME_ROOT="${RUST_CHATBOT_SHARED_ROOT:-${RUST_CHATBOT_CONFIG_HOME:-${HOME}/.rust-chatbot}}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
DEST="${BACKUP_ROOT}/${TIMESTAMP}"
RCLONE_REMOTE="gcs:rcc-llm-chatbot-backups"
RCLONE_SESSIONS="gcs:rcc-llm-chatbot-sessions"
RCLONE_LOGS="gcs:rcc-llm-chatbot-logs"
LOCAL_LOG_KEEP_DAYS=3
MCP_RESPAWN_TIMEOUT_SECONDS="${RUST_CHATBOT_MCP_RESPAWN_TIMEOUT_SECONDS:-5}"
declare -a MCP_SERVER_PATTERNS=()

resolve_log_dir() {
    if [[ -n "${RUST_CHATBOT_LOG_DIR:-}" ]]; then
        printf '%s\n' "${RUST_CHATBOT_LOG_DIR}"
        return
    fi

    case "$(uname -s)" in
        Darwin)
            printf '%s\n' "${HOME}/Library/Logs/rust-chatbot"
            ;;
        *)
            if [[ -n "${XDG_STATE_HOME:-}" ]]; then
                printf '%s\n' "${XDG_STATE_HOME}"
            else
                printf '%s\n' "${HOME}/.local/state"
            fi
            ;;
    esac
}

LOG_DIR="$(resolve_log_dir)"

mkdir -p "${DEST}"

echo "=== LLM Chatbot Data Backup ==="
echo "Timestamp : ${TIMESTAMP}"
echo "Destination: ${DEST}"
echo "Shared runtime root: ${SHARED_RUNTIME_ROOT}"
echo ""

init_mcp_server_patterns() {
    local raw_patterns="${RUST_CHATBOT_MCP_SERVER_PATTERNS:-}"

    if [[ -n "${raw_patterns}" ]]; then
        IFS=':' read -r -a MCP_SERVER_PATTERNS <<< "${raw_patterns}"
        return
    fi

    MCP_SERVER_PATTERNS=(
        "chatbot-memory-mcp"
        "chatbot-session-mcp"
        "mcp-server-filesystem"
        "mcp-server-commands"
        "context7-mcp"
        "mcp-server-sequential-thinking"
        "mcp-server-puppeteer"
        "chrome-devtools-mcp"
        "bash-mcp"
        "opus-mcp"
    )
}

list_active_mcp_processes() {
    local filter_pattern="${1:-}"
    local pid
    local args
    local pattern

    while read -r pid args; do
        if [[ -z "${pid}" || "${pid}" == "$$" ]]; then
            continue
        fi

        for pattern in "${MCP_SERVER_PATTERNS[@]}"; do
            if [[ "${args}" != *"${pattern}"* ]]; then
                continue
            fi
            if [[ -n "${filter_pattern}" && "${pattern}" != "${filter_pattern}" ]]; then
                continue
            fi

            printf '%s\t%s\t%s\n' "${pattern}" "${pid}" "${args}"
            break
        done
    done < <(ps -eo pid=,args=)
}

count_active_mcp_processes() {
    local pattern="$1"
    local count=0

    while IFS= read -r _; do
        ((count += 1))
    done < <(list_active_mcp_processes "${pattern}")

    printf '%s\n' "${count}"
}

wait_for_process_exit() {
    local pid="$1"

    for _ in {1..10}; do
        if ! kill -0 "${pid}" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done

    return 1
}

restart_active_mcp_servers() {
    if [[ "${RUST_CHATBOT_SKIP_MCP_RESTART:-0}" == "1" ]]; then
        echo "[SKIP] MCP server restart disabled by RUST_CHATBOT_SKIP_MCP_RESTART=1"
        return
    fi

    local -a matches=()
    mapfile -t matches < <(list_active_mcp_processes)
    if (( ${#matches[@]} == 0 )); then
        echo "[SKIP] no active MCP server processes found"
        return
    fi

    declare -A original_counts=()
    local -a pids=()
    local match
    for match in "${matches[@]}"; do
        IFS=$'\t' read -r pattern pid _ <<< "${match}"
        ((original_counts["${pattern}"] += 1))
        pids+=("${pid}")
    done

    local pattern
    for pattern in "${MCP_SERVER_PATTERNS[@]}"; do
        if [[ -n "${original_counts[${pattern}]:-}" ]]; then
            echo "[RESTART] ${pattern} (${original_counts[${pattern}]} process(es))"
        fi
    done

    if ((${#pids[@]} > 0)); then
        kill "${pids[@]}" >/dev/null 2>&1 || true
    fi

    local pid
    for pid in "${pids[@]}"; do
        wait_for_process_exit "${pid}" || kill -KILL "${pid}" >/dev/null 2>&1 || true
    done

    for pattern in "${MCP_SERVER_PATTERNS[@]}"; do
        local expected_count="${original_counts[${pattern}]:-0}"
        if (( expected_count == 0 )); then
            continue
        fi

        local restarted=0
        local attempt
        for ((attempt=1; attempt<=MCP_RESPAWN_TIMEOUT_SECONDS; attempt++)); do
            if (( $(count_active_mcp_processes "${pattern}") >= expected_count )); then
                restarted=1
                break
            fi
            sleep 1
        done

        if (( restarted )); then
            echo "[OK] ${pattern} restarted within ${MCP_RESPAWN_TIMEOUT_SECONDS}s"
        else
            echo "[WARN] ${pattern} did not fully restart within ${MCP_RESPAWN_TIMEOUT_SECONDS}s"
        fi
    done
}

init_mcp_server_patterns

resolve_compression_cores() {
    if command -v nproc >/dev/null 2>&1; then
        nproc
        return
    fi

    if command -v getconf >/dev/null 2>&1; then
        getconf _NPROCESSORS_ONLN
        return
    fi

    if [[ "$(uname -s)" == "Darwin" ]] && command -v sysctl >/dev/null 2>&1; then
        sysctl -n hw.ncpu
        return
    fi

    printf '1\n'
}

create_archive() {
    local src="$1"
    local archive="$2"
    local parent_dir
    local base_name

    parent_dir="$(dirname "${src}")"
    base_name="$(basename "${src}")"

    if command -v pigz >/dev/null 2>&1; then
        local cores
        cores="$(resolve_compression_cores)"
        tar cf - --warning=no-file-changed -C "${parent_dir}" "${base_name}" 2>/dev/null | \
            pigz -p "${cores}" > "${archive}"
        return
    fi

    tar czf "${archive}" --warning=no-file-changed -C "${parent_dir}" "${base_name}" 2>/dev/null
}

backup_dir() {
    local src="$1"
    local label="$2"
    local archive="${DEST}/${label}.tar.gz"

    if [[ ! -d "${src}" ]]; then
        echo "[SKIP] ${label} — source not found: ${src}"
        return
    fi

    local size
    size="$(du -sh "${src}" | cut -f1)"
    echo "[BACKUP] ${label} (${size}) → ${archive##*/}"

    local retries=3
    for ((i=1; i<=retries; i++)); do
        if create_archive "${src}" "${archive}"; then
            break
        fi
        if ((i < retries)); then
            echo "         [RETRY ${i}/${retries}] archive creation failed, retrying..."
            rm -f "${archive}"
            sleep 2
        else
            echo "[ERROR] ${label} — archive creation failed after ${retries} attempts, removing partial archive"
            rm -f "${archive}"
            return 1
        fi
    done

    local archive_size
    archive_size="$(du -sh "${archive}" | cut -f1)"
    echo "         compressed: ${archive_size}"
}

rclone_sync_dir_if_present() {
    local src="$1"
    local dest="$2"
    local label="$3"

    if [[ ! -d "${src}" ]]; then
        echo "[SKIP] ${label} — source not found: ${src}"
        return
    fi

    echo "[SYNC] ${label} → ${dest}"
    rclone sync "${src}/" "${dest}/" \
        --progress \
        --transfers 4
}

rclone_copy_file_if_present() {
    local src="$1"
    local dest="$2"
    local label="$3"

    if [[ ! -f "${src}" ]]; then
        echo "[SKIP] ${label} — source not found: ${src}"
        return
    fi

    echo "[COPY] ${label} → ${dest}"
    rclone copyto "${src}" "${dest}" \
        --progress
}

rclone_backup_session_store() {
    local root="$1"
    local label="$2"
    local remote_root="${RCLONE_SESSIONS}/${label}"

    echo ""
    echo "Session store: ${label}"
    rclone_sync_dir_if_present "${root}/sessions" "${remote_root}/sessions" "${label} sessions"
    rclone_copy_file_if_present "${root}/sessions.json" "${remote_root}/sessions.json" "${label} sessions.json"
    rclone_sync_dir_if_present "${root}/height_cache" "${remote_root}/height_cache" "${label} height_cache"
}

shared_desktop_label() {
    local root="$1"
    local base

    base="$(basename "${root}")"
    printf '%s\n' "${base#.}"
}

list_shared_desktop_roots() {
    local root

    shopt -s nullglob
    for root in "${SHARED_RUNTIME_ROOT}"/.*-desktop; do
        [[ -d "${root}" ]] || continue
        printf '%s\n' "${root}"
    done
    shopt -u nullglob
}

backup_shared_desktop_roots() {
    local -a roots=()
    local root
    local label

    mapfile -t roots < <(list_shared_desktop_roots)
    if (( ${#roots[@]} == 0 )); then
        echo "[SKIP] no provider session stores found under ${SHARED_RUNTIME_ROOT}"
        return
    fi

    for root in "${roots[@]}"; do
        label="$(shared_desktop_label "${root}")"
        backup_dir "${root}" "${label}"
    done
}

sync_shared_session_stores() {
    local -a roots=()
    local root
    local label

    mapfile -t roots < <(list_shared_desktop_roots)
    if (( ${#roots[@]} == 0 )); then
        echo "[SKIP] no provider session stores found under ${SHARED_RUNTIME_ROOT}"
        return
    fi

    for root in "${roots[@]}"; do
        label="$(shared_desktop_label "${root}")"
        rclone_backup_session_store "${root}" "${label}"
    done
}

echo "=== Restarting MCP Servers ==="
restart_active_mcp_servers
echo ""

backup_shared_desktop_roots
backup_dir "${REPO_DIR}/.claude-desktop" "repo-claude-desktop"

# ── Rotate data backups: keep only the last 3 locally ──

echo ""
echo "=== Rotating local data backups (keep 3) ==="
cd "${BACKUP_ROOT}"
DIRS=($(ls -1d [0-9]*-[0-9]*/ 2>/dev/null | sort))
ZIPS=($(ls -1 [0-9]*-[0-9]*.zip 2>/dev/null | sort))
KEEP=3

if (( ${#DIRS[@]} > KEEP )); then
    DELETE_COUNT=$(( ${#DIRS[@]} - KEEP ))
    for dir in "${DIRS[@]:0:$DELETE_COUNT}"; do
        echo "  Removing old backup dir: ${dir}"
        rm -rf "${dir}"
    done
fi
if (( ${#ZIPS[@]} > KEEP )); then
    DELETE_COUNT=$(( ${#ZIPS[@]} - KEEP ))
    for z in "${ZIPS[@]:0:$DELETE_COUNT}"; do
        echo "  Removing old backup zip: ${z}"
        rm -f "${z}"
    done
fi

# ── Upload to Google Drive ──

if ! command -v rclone &>/dev/null || ! rclone listremotes 2>/dev/null | grep -q '^gcs:'; then
    echo ""
    echo "[SKIP] rclone not installed or 'gcs' remote not configured."
    echo "       Install: curl https://rclone.org/install.sh | sudo bash"
    echo "       Configure: rclone config  (choose Google Cloud Storage, name it 'gcs')"
    echo ""
    echo "=== Done (local only) ==="
    du -sh "${DEST}"
    exit 0
fi

# Upload data backups (synced = mirrors local rotation)
echo ""
echo "=== Uploading data backups to GCS ==="
rclone sync "${BACKUP_ROOT}/" "${RCLONE_REMOTE}/" \
    --progress \
    --transfers 4
echo "Data upload complete."

# Upload live session stores (synced = remote mirrors local state)
echo ""
echo "=== Uploading live session stores to GCS ==="
sync_shared_session_stores
rclone_backup_session_store "${REPO_DIR}/.claude-desktop" "repo-claude-desktop"
echo "Session upload complete."

# Upload logs (copy = append-only, never deletes remote logs)
echo ""
echo "=== Uploading logs to GCS (kept forever) ==="
if [[ ! -d "${LOG_DIR}" ]]; then
    echo "[SKIP] log root not found: ${LOG_DIR}"
elif rclone copy "${LOG_DIR}/" "${RCLONE_LOGS}/" \
    --progress \
    --transfers 4 \
    --include "*.log" \
    --include "*.log.*" \
    --ignore-size; then
    echo "Log upload complete."

    # ── Rotate local logs: delete files older than 3 days ──

    echo ""
    echo "=== Rotating local logs (keep ${LOCAL_LOG_KEEP_DAYS} days) ==="
    find "${LOG_DIR}" -type f \( -name "*.log" -o -name "*.log.*" \) -mtime "+${LOCAL_LOG_KEEP_DAYS}" -print -delete
else
    echo "Log upload failed; skipping local log rotation to avoid data loss." >&2
fi

echo ""
echo "=== Done ==="
echo "Backup location: ${DEST}"
du -sh "${DEST}"
echo "Remote data : ${RCLONE_REMOTE}/"
echo "Remote sessions : ${RCLONE_SESSIONS}/"
echo "Remote logs : ${RCLONE_LOGS}/ (kept forever)"
