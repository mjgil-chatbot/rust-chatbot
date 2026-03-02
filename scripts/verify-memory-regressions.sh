#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="${APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
TARGET_BASE="${XDG_CACHE_HOME:-$HOME/.cache}/rust-chatbot"
TARGET_DIR="${CARGO_TARGET_DIR:-${TARGET_BASE}/target/memory-regressions}"

cd "$APP_ROOT"
mkdir -p "$TARGET_DIR"
export CARGO_TARGET_DIR="${TARGET_DIR}"
unset RUSTC_WRAPPER

echo "[verify-memory-regressions] running focused memory release regressions"
echo "[verify-memory-regressions] target-dir: ${CARGO_TARGET_DIR}"

tests=(
  workspace_memory_status_routes_successfully
  search_memory_routes_successfully
  list_memory_injections_returns_persisted_user_turn_previews
  list_memory_retrievals_returns_candidate_hits_and_injected_preview
  invalidate_doc_key_routes_successfully
  restore_doc_key_routes_successfully
  remember_fact_rejects_invalid_priority
  service_error_for_workspace_outside_allowed_roots
)

for test_name in "${tests[@]}"; do
  cargo test --release "$test_name" -- --nocapture
done
