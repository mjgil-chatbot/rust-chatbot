#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/common.sh"

rust_chatbot_build_release "$REPO_ROOT"
"$(rust_chatbot_release_dir "$REPO_ROOT")/rcc" codex
