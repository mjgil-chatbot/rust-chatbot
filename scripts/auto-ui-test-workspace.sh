#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="${APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"

cd "$APP_ROOT"

echo "[auto-ui-test-workspace] running focused workspace picker release regressions"
cargo test --release pressing_enter_in_workspace_input_applies_unique_completion_match -- --nocapture
cargo test --release selecting_workspace_completion_updates_recent_dropdown -- --nocapture
cargo test --release apply_workspace_input_uses_unique_completion_match -- --nocapture
