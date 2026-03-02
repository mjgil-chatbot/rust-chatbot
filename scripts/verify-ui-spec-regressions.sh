#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="${APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
TARGET_BASE="${XDG_CACHE_HOME:-$HOME/.cache}/rust-chatbot"
TARGET_DIR="${CARGO_TARGET_DIR:-${TARGET_BASE}/target/ui-spec-regressions}"

cd "$APP_ROOT"
mkdir -p "$TARGET_DIR"
export CARGO_TARGET_DIR="${TARGET_DIR}"
unset RUSTC_WRAPPER

echo "[verify-ui-spec-regressions] running focused ui-spec release regressions"
echo "[verify-ui-spec-regressions] target-dir: ${CARGO_TARGET_DIR}"

tests=(
  submit_valid_creates_pending_draft_and_requests_focus
  apply_stream_to_current_clears_pending_draft_on_first_chunk
  apply_stream_to_current_empty_response_keeps_pending_draft
  local_terminal_worker_clears_pending_draft
  local_terminal_worker_clears_pending_draft_even_on_empty_response
  local_terminal_empty_response_requests_focus_and_clears_pending_draft
  remote_terminal_update_clears_pending_draft
  remote_terminal_update_updates_current_session_owned_state
  send_failed_returns_chat_to_recoverable_idle_state
  selected_session_reload_skips_replacement_when_pending_draft_active
  selected_session_reload_proceeds_when_no_pending_draft
  selected_session_reload_proceeds_when_pending_draft_is_for_other_chat
  switching_chats_preserves_pending_draft_for_original_chat
  render_with_pending_draft_does_not_clear_or_panic
  render_with_pending_draft_for_other_chat_hides_indicator
)

for test_name in "${tests[@]}"; do
  cargo test --release "$test_name" -- --nocapture
done
