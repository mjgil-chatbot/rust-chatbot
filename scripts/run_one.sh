#!/bin/bash

# Build and launch a single chatbot provider instance via chatbot-ctl.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/common.sh"

CTL="$(rust_chatbot_release_dir "$REPO_ROOT")/chatbot-ctl"

usage() {
    echo "Usage: $0 <gemini|claude|codex|minimax> [instance-number]"
    echo ""
    echo "  gemini              Start a Gemini instance"
    echo "  claude [N]          Start a Claude instance (default instance 1)"
    echo "  codex               Start a Codex instance"
    echo "  minimax             Start a MiniMax instance"
    exit 1
}

[ -z "$1" ] && usage

PROVIDER="$1"
INSTANCE="${2:-1}"

# Build
echo "Building release binaries..."
rust_chatbot_build_release "$REPO_ROOT" || {
    echo "Build failed"
    exit 1
}

case "$PROVIDER" in
    gemini|claude|codex|minimax) ;;
    *) usage ;;
esac

echo "Launching $PROVIDER (instance $INSTANCE)..."
"$CTL" launch --provider "$PROVIDER" --instance "$INSTANCE"
