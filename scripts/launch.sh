#!/bin/bash

# Build and launch Rust Chatbot through the `rcc` provider wrapper.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/common.sh"

RCC_BIN="$(rust_chatbot_release_dir "$REPO_ROOT")/rcc"
PROVIDER="${1:-claude}"

if [ $# -gt 0 ]; then
    shift
fi

echo "Building Rust Chatbot..."

if ! command -v cargo >/dev/null 2>&1; then
    echo "Error: cargo not found"
    exit 1
fi

if [ ! -f "$REPO_ROOT/Cargo.toml" ]; then
    echo "Error: Cargo.toml not found in $REPO_ROOT"
    exit 1
fi

rust_chatbot_build_release "$REPO_ROOT"

echo "Launching $PROVIDER..."
exec "$RCC_BIN" "$PROVIDER" "$@"
