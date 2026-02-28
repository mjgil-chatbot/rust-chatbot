#!/bin/bash

# Stop all running chatbot instances + broker

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/common.sh"

# Resolve chatbot-ctl from the same build profile as the running app.
# Prefer debug if a debug-profile binary is running; otherwise release.
CTL=""
if pgrep -f "$REPO_ROOT/target/debug/rust-chatbot" >/dev/null 2>&1; then
    CTL="$REPO_ROOT/target/debug/chatbot-ctl"
fi
if [ -z "$CTL" ] || [ ! -f "$CTL" ]; then
    CTL="$(rust_chatbot_release_dir "$REPO_ROOT")/chatbot-ctl"
fi
if [ -z "$CTL" ] || [ ! -f "$CTL" ]; then
    CTL="$REPO_ROOT/target/release/chatbot-ctl"
fi

if [ ! -f "$CTL" ]; then
    echo "chatbot-ctl not found. Falling back to pkill..."
    pkill -x "rust-chatbot" 2>/dev/null && echo "Killed all rust-chatbot processes." || echo "No rust-chatbot processes found."
    pkill -x "chatbot-broker" 2>/dev/null && echo "Killed broker." || echo "No broker found."
    exit 0
fi

echo "Current status:"
"$CTL" status
echo ""

echo "Stopping all instances + broker..."
"$CTL" stop-all

# Give processes a moment to exit
sleep 1

# Verify UI instances are gone
REMAINING=$("$CTL" pids 2>/dev/null)
if [ -n "$REMAINING" ]; then
    echo "Some instances still running, sending SIGKILL..."
    for PID in $REMAINING; do
        kill -9 "$PID" 2>/dev/null && echo "  Killed PID $PID (SIGKILL)"
    done
    sleep 1
fi

echo ""
echo "Final status:"
"$CTL" status
echo "Done."
