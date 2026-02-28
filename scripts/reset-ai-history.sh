#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHARED_ROOT="${RUST_CHATBOT_SHARED_ROOT:-${HOME}/git/.rust-chatbot}"
PRIMARY_ROOT="${SHARED_ROOT}/ai-history"
BACKUP_ROOT="${SHARED_ROOT}/ai-history-backup"

ASSUME_YES=0
RESTORE_ALL=0
SKIP_INTEGRITY_CHECK=0
ALLOW_RUNNING_APP=0
WORKSPACE_ROOT=""
WORKSPACE_ID=""

usage() {
    cat <<'EOF'
Usage: scripts/reset-ai-history.sh [options]

Delete the primary ai-history root, recreate it, and restore from
ai-history-backup.

Options:
  --workspace-root PATH      Restore the workspace at PATH after reset.
  --workspace-id ID          Restore the workspace backup with ID after reset.
  --restore-all              Restore every workspace backup into ai-history.
  --allow-running-app        Skip the rust-chatbot process guard.
  --skip-integrity-check     Skip sqlite3 integrity checks on restored store.db.
  --yes                      Do not prompt before deleting ai-history.
  -h, --help                 Show this help text.
EOF
}

log() {
    printf '[reset-ai-history] %s\n' "$*"
}

fail() {
    printf '[reset-ai-history] %s\n' "$*" >&2
    exit 1
}

require_command() {
    local command_name="$1"
    command -v "$command_name" >/dev/null 2>&1 || fail "missing required command: ${command_name}"
}

detect_running_app() {
    if [[ "${ALLOW_RUNNING_APP}" -eq 1 ]]; then
        return 0
    fi

    if ! command -v ps >/dev/null 2>&1; then
        return 0
    fi

    local matches
    matches="$(ps -eo comm=,pid=,args= | awk '$1 == "rust-chatbot" { print }' || true)"
    if [[ -n "$matches" ]]; then
        printf '%s\n' "$matches" >&2
        fail "rust-chatbot appears to be running; stop the app before resetting ai-history"
    fi
}

confirm_reset() {
    if [[ "${ASSUME_YES}" -eq 1 ]]; then
        return 0
    fi

    printf 'Delete %s and recreate it from backup? [y/N] ' "${PRIMARY_ROOT}"
    local reply
    read -r reply
    case "${reply}" in
        y|Y|yes|YES)
            ;;
        *)
            fail "aborted"
            ;;
    esac
}

format_size() {
    local path="$1"
    if [[ ! -e "${path}" ]]; then
        printf 'missing\n'
        return 0
    fi
    du -sh "${path}" 2>/dev/null | awk '{print $1}'
}

canonical_workspace_path() {
    local workspace_path="$1"
    python3 - "$workspace_path" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1]).resolve(strict=True)
print(path)
PY
}

workspace_id_for_path() {
    local workspace_path="$1"
    python3 - "$workspace_path" <<'PY'
from pathlib import Path
import sys

FNV_OFFSET_BASIS = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3

path = Path(sys.argv[1]).resolve(strict=True)
hash_value = FNV_OFFSET_BASIS
for byte in str(path).encode():
    hash_value ^= byte
    hash_value = (hash_value * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF

print(f"{hash_value:016x}")
PY
}

copy_workspace_backup() {
    local workspace_id="$1"
    local source_dir="${BACKUP_ROOT}/${workspace_id}"
    local target_dir="${PRIMARY_ROOT}/${workspace_id}"

    if [[ ! -d "${source_dir}" ]]; then
        return 1
    fi

    mkdir -p "${target_dir}"
    cp -a "${source_dir}/." "${target_dir}/"
}

verify_store_integrity() {
    local workspace_id="$1"
    local store_path="${PRIMARY_ROOT}/${workspace_id}/store.db"

    if [[ "${SKIP_INTEGRITY_CHECK}" -eq 1 || ! -f "${store_path}" ]]; then
        return 0
    fi

    if ! command -v sqlite3 >/dev/null 2>&1; then
        log "sqlite3 not available; skipping integrity check for ${store_path}"
        return 0
    fi

    local result
    result="$(sqlite3 "${store_path}" 'pragma integrity_check;' | tr -d '\r')"
    if [[ "${result}" != "ok" ]]; then
        fail "sqlite integrity check failed for ${store_path}: ${result}"
    fi
}

restore_current_workspace() {
    local workspace_id
    if [[ -n "${WORKSPACE_ID}" ]]; then
        workspace_id="${WORKSPACE_ID}"
        log "workspace id: ${workspace_id}"
    else
        local canonical_workspace
        canonical_workspace="$(canonical_workspace_path "${WORKSPACE_ROOT}")"
        workspace_id="$(workspace_id_for_path "${canonical_workspace}")"
        log "workspace root: ${canonical_workspace}"
        log "workspace id: ${workspace_id}"
    fi

    if copy_workspace_backup "${workspace_id}"; then
        verify_store_integrity "${workspace_id}"
        log "restored workspace backup into ${PRIMARY_ROOT}/${workspace_id}"
    else
        log "no backup exists for workspace ${workspace_id}; leaving primary ai-history root empty for lazy app reinitialization"
    fi
}

restore_all_workspaces() {
    if [[ ! -d "${BACKUP_ROOT}" ]]; then
        log "backup root missing; leaving primary ai-history root empty"
        return 0
    fi

    local restored_count=0
    local backup_dir
    for backup_dir in "${BACKUP_ROOT}"/*; do
        [[ -d "${backup_dir}" ]] || continue
        local workspace_id
        workspace_id="$(basename "${backup_dir}")"
        copy_workspace_backup "${workspace_id}"
        restored_count=$((restored_count + 1))
    done

    log "restored ${restored_count} workspace backup(s)"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --workspace-root)
            [[ $# -ge 2 ]] || fail "--workspace-root requires a path"
            WORKSPACE_ROOT="$2"
            shift 2
            ;;
        --workspace-id)
            [[ $# -ge 2 ]] || fail "--workspace-id requires an id"
            WORKSPACE_ID="$2"
            shift 2
            ;;
        --restore-all)
            RESTORE_ALL=1
            shift
            ;;
        --allow-running-app)
            ALLOW_RUNNING_APP=1
            shift
            ;;
        --skip-integrity-check)
            SKIP_INTEGRITY_CHECK=1
            shift
            ;;
        --yes)
            ASSUME_YES=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
done

require_command python3

if [[ "${RESTORE_ALL}" -eq 1 && ( -n "${WORKSPACE_ROOT}" || -n "${WORKSPACE_ID}" ) ]]; then
    fail "--restore-all cannot be combined with --workspace-root or --workspace-id"
fi

if [[ -n "${WORKSPACE_ROOT}" && -n "${WORKSPACE_ID}" ]]; then
    fail "--workspace-root cannot be combined with --workspace-id"
fi

detect_running_app
confirm_reset

log "primary root before reset: $(format_size "${PRIMARY_ROOT}")"
log "backup root: ${BACKUP_ROOT} ($(format_size "${BACKUP_ROOT}"))"

rm -rf "${PRIMARY_ROOT}"
mkdir -p "${PRIMARY_ROOT}"

if [[ "${RESTORE_ALL}" -eq 1 ]]; then
    restore_all_workspaces
elif [[ -n "${WORKSPACE_ROOT}" || -n "${WORKSPACE_ID}" ]]; then
    restore_current_workspace
else
    log "primary ai-history root recreated; the desktop app can lazily restore or recreate workspace stores on demand"
fi

log "primary root after reset: $(format_size "${PRIMARY_ROOT}")"
