#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="${APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
TARGET_DIR="${CARGO_TARGET_DIR:-${APP_ROOT}/target/auto-ui-background-flow}"

cd "$APP_ROOT"
export CARGO_TARGET_DIR="${TARGET_DIR}"

echo "[auto-ui-test-background-flow] running focused background-flow release regressions"
echo "[auto-ui-test-background-flow] target-dir: ${CARGO_TARGET_DIR}"
cargo test --release check_for_response_drains_worker_workspace_and_history_background_channels_together -- --nocapture
cargo test --release worker_discovery_events_are_replayed_by_check_for_response -- --nocapture
cargo test --release poll_history_sync_loads_missing_store_notice_via_background_refresh -- --nocapture
cargo test --release async_apply_workspace_input_updates_state -- --nocapture
cargo test --release async_preview_runsc_input_updates_field -- --nocapture
cargo test --release pressing_enter_in_workspace_input_applies_unique_completion_match -- --nocapture
