#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="${APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
TARGET_BASE="${XDG_CACHE_HOME:-$HOME/.cache}/rust-chatbot"
TARGET_DIR="${CARGO_TARGET_DIR:-${TARGET_BASE}/target/history-regressions}"

cd "$APP_ROOT"
mkdir -p "$TARGET_DIR"
export CARGO_TARGET_DIR="${TARGET_DIR}"
unset RUSTC_WRAPPER

echo "[verify-history-regressions] running focused history/checkpoint release regressions"
echo "[verify-history-regressions] target-dir: ${CARGO_TARGET_DIR}"

tests=(
  poll_history_sync_loads_missing_store_notice_via_background_refresh
  drain_history_results_clears_stuck_refresh_on_disconnect
  drain_history_results_clears_stuck_checkpoint_on_disconnect
  load_history_snapshot_restores_deleted_store_byte_for_byte
  build_checkpoint_manifest_skips_invalid_cargo_manifest_for_companion_discovery
  build_checkpoint_manifest_ignores_unrelated_dirty_sibling_repo_patches
  build_checkpoint_manifest_omits_oversized_untracked_repo_patch
  build_checkpoint_manifest_omits_repo_patch_when_remaining_quota_is_too_small
  sync_chat_history_backfills_missing_auto_turn_checkpoints
  load_history_snapshot_exposes_checkpoint_repo_summaries
)

for test_name in "${tests[@]}"; do
  cargo test --release "$test_name" -- --nocapture
done
