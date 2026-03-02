#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="${APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
TARGET_BASE="${XDG_CACHE_HOME:-$HOME/.cache}/rust-chatbot"
TARGET_DIR="${CARGO_TARGET_DIR:-${TARGET_BASE}/target/spool-session-regressions}"

cd "$APP_ROOT"
mkdir -p "$TARGET_DIR"
export CARGO_TARGET_DIR="${TARGET_DIR}"
unset RUSTC_WRAPPER

echo "[verify-spool-session-regressions] running focused spool/session release regressions"
echo "[verify-spool-session-regressions] target-dir: ${CARGO_TARGET_DIR}"

tests=(
  read_worker_result_treats_unsupported_schema_version_as_malformed
  local_terminal_worker_persists_session_before_worker_cleanup
  local_terminal_worker_keeps_existing_visible_assistant_when_terminal_response_is_think_only
  remote_terminal_update_reloads_newer_disk_session_before_applying_response
  poll_worker_slots_clears_processing_for_stale_request_only_spool_entry
  poll_worker_slots_done_replaces_stale_provider_session_id
  inspect_session_surfaces_worker_spool_truncation_metadata
)

for test_name in "${tests[@]}"; do
  cargo test --release "$test_name" -- --nocapture
done
