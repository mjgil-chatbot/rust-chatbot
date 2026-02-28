#!/bin/bash

# Restart a single provider instance by stopping it and launching a new one.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/common.sh"

CTL="$(rust_chatbot_release_dir "$REPO_ROOT")/chatbot-ctl"

usage() {
    echo "Usage: $0 <claude|gemini|geminiforge|geminicliforge|codex|minimax|minimaxforge>"
    echo ""
    echo "  claude              Restart a Claude instance"
    echo "  gemini              Restart a Gemini instance"
    echo "  geminiforge         Restart a GeminiForge instance"
    echo "  geminicliforge      Restart a GeminiCliForge instance"
    echo "  codex               Restart a Codex instance"
    echo "  minimax             Restart a MiniMax instance"
    echo "  minimaxforge        Restart a MiniMaxForge instance"
    exit 1
}

[ -z "$1" ] && usage

PROVIDER="$1"

case "$PROVIDER" in
    claude|gemini|geminiforge|geminicliforge|codex|minimax|minimaxforge) ;;
    *) usage ;;
esac

echo "Stopping running $PROVIDER instances..."
"$CTL" stop-provider "$PROVIDER" 2>/dev/null || true
sleep 1

echo "Building release binaries..."
rust_chatbot_build_release "$REPO_ROOT" || {
    echo "Build failed"
    exit 1
}

echo "Launching new $PROVIDER instance..."
"$CTL" launch --provider "$PROVIDER" --instance 1

echo "Done."
