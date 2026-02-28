#!/bin/bash

# Rebuild and restart the currently running UI instances.
# Falls back to the default multi-provider profile if nothing is running.
# When the live app was launched from an installed ~/.local/bin copy, refresh
# that install before restarting so the relaunched UI actually picks up the new
# code instead of reusing stale installed binaries.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/common.sh"

echo ""
echo "Building release binaries..."
rust_chatbot_build_release "$REPO_ROOT" || {
    echo "Build failed"
    exit 1
}

CTL="$(rust_chatbot_release_dir "$REPO_ROOT")/chatbot-ctl"
INSTALL_ROOT="${CARGO_INSTALL_ROOT:-$HOME/.local}"
INSTALLED_CTL="$INSTALL_ROOT/bin/chatbot-ctl"
INSTALLED_UI="$INSTALL_ROOT/bin/rust-chatbot"

if [ ! -x "$CTL" ]; then
    echo "Release chatbot-ctl not found at $CTL"
    exit 1
fi

LIVE_PIDS=$("$CTL" pids 2>/dev/null)
if [ -n "$LIVE_PIDS" ]; then
    USE_INSTALLED_BINARIES=0
    for pid in $LIVE_PIDS; do
        cmd="$(ps -o command= -p "$pid" 2>/dev/null || true)"
        case "$cmd" in
            "$INSTALLED_UI"*)
                USE_INSTALLED_BINARIES=1
                break
                ;;
        esac
    done

    if [ "$USE_INSTALLED_BINARIES" -eq 1 ]; then
        echo ""
        echo "Refreshing installed binaries under $INSTALL_ROOT/bin..."
        rust_chatbot_install_release_binaries "$REPO_ROOT" "$INSTALL_ROOT" || {
            echo "Install sync failed"
            exit 1
        }
        CTL="$INSTALLED_CTL"
    fi

    echo ""
    echo "Restarting running instances..."
    "$CTL" restart-running
    sleep 1

    echo ""
    "$CTL" status
else
    echo ""
    echo "No running instances found. Launching default profile..."
    "$SCRIPT_DIR/start-all.sh"
fi
