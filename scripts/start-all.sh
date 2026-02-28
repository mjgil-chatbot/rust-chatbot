#!/bin/bash

# Build and launch the default multi-provider profile.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/common.sh"

RELEASE_DIR="$(rust_chatbot_release_dir "$REPO_ROOT")"
CTL="$RELEASE_DIR/chatbot-ctl"
DEFAULT_SPECS=(
    "claude:1"
    "codex:1"
    "gemini:1"
)

# Build if needed
if [ ! -f "$CTL" ] || [ ! -f "$RELEASE_DIR/rust-chatbot" ] || [ ! -f "$RELEASE_DIR/rcc" ] || [ ! -f "$RELEASE_DIR/chatbot-worker" ] || [ ! -f "$(rust_chatbot_adapter_release_binary_path "$REPO_ROOT")" ]; then
    echo "Building release binaries..."
    rust_chatbot_build_release "$REPO_ROOT" || {
        echo "Build failed"
        exit 1
    }
fi

LIVE_PIDS=$("$CTL" pids 2>/dev/null)
if [ -n "$LIVE_PIDS" ]; then
    echo "Instances already running:"
    "$CTL" status
    echo ""
    echo "Run stop-all.sh first."
    exit 1
fi

echo "Launching default profile..."
for spec in "${DEFAULT_SPECS[@]}"; do
    provider="${spec%%:*}"
    instance="${spec##*:}"
    "$CTL" launch --provider "$provider" --instance "$instance"
done

sleep 1
echo ""
"$CTL" status
