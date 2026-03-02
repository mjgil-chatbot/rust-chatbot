#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="${APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
TARGET_BASE="${XDG_CACHE_HOME:-$HOME/.cache}/rust-chatbot"
TARGET_DIR="${CARGO_TARGET_DIR:-${TARGET_BASE}/target/session-lifecycle-regressions}"

cd "$APP_ROOT"
mkdir -p "$TARGET_DIR"
export CARGO_TARGET_DIR="${TARGET_DIR}"
unset RUSTC_WRAPPER

echo "[verify-session-lifecycle-regressions] running focused lifecycle release regressions"
echo "[verify-session-lifecycle-regressions] target-dir: ${CARGO_TARGET_DIR}"

tests=(
  submit_valid_creates_pending_draft_and_requests_focus
  apply_stream_to_current_clears_pending_draft_on_first_chunk
  local_terminal_worker_clears_pending_draft
  local_terminal_worker_clears_pending_draft_even_on_empty_response
  remote_terminal_update_clears_pending_draft
  local_terminal_worker_persists_session_before_worker_cleanup
  poll_worker_slots_clears_processing_for_stale_request_only_spool_entry
  selected_session_reload_skips_replacement_when_pending_draft_active
  selected_session_reload_drops_divergent_newer_snapshot_for_current_chat
  selected_session_reload_accepts_strict_extension_even_with_equal_timestamp
)

for test_name in "${tests[@]}"; do
  cargo test --release "$test_name" -- --nocapture
done
