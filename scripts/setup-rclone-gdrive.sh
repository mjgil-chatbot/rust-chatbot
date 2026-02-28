#!/usr/bin/env bash
set -euo pipefail

# Set up a dedicated GCP project + OAuth client for rclone Google Drive access.
# Automates project creation and API enablement via gcloud CLI.
# OAuth consent screen + client must be created in the browser (no CLI support).

PROJECT_ID="${1:-}"
ACCOUNT="$(gcloud auth list --filter=status:ACTIVE --format='value(account)' 2>/dev/null | head -1)"

if [[ -z "$ACCOUNT" ]]; then
    echo "No active gcloud account. Run: gcloud auth login"
    exit 1
fi

# ── Step 1: Create project + enable API (skip if project ID provided) ──

if [[ -z "$PROJECT_ID" ]]; then
    PROJECT_ID="rclone-backup-$(date +%s | tail -c 7)"
    echo "=== Setting up rclone GDrive OAuth ==="
    echo "Account : ${ACCOUNT}"
    echo "Project : ${PROJECT_ID}"
    echo ""

    echo "[1/2] Creating project..."
    gcloud projects create "${PROJECT_ID}" --name="rclone-backup" --quiet
    gcloud config set project "${PROJECT_ID}" --quiet

    echo "[2/2] Enabling Google Drive API..."
    gcloud services enable drive.googleapis.com --project="${PROJECT_ID}"
else
    echo "Using existing project: ${PROJECT_ID}"
fi

# ── Step 2: Manual browser steps ──

echo ""
echo "=== Manual steps (browser required) ==="
echo ""
echo "1. Consent screen:"
echo "   https://console.cloud.google.com/apis/credentials/consent?project=${PROJECT_ID}"
echo "   → Get Started → App name: rclone-backup"
echo "   → Email: ${ACCOUNT} → Audience: External → Agree & Create"
echo ""
echo "2. Data Access (left panel) → Add scopes → Manually add:"
echo "   https://www.googleapis.com/auth/drive"
echo "   → Add to table → Update → Save"
echo ""
echo "3. Audience (left panel) → + Add users → ${ACCOUNT} → Save"
echo ""
echo "4. Create OAuth client:"
echo "   https://console.cloud.google.com/apis/credentials/oauthclient?project=${PROJECT_ID}"
echo "   → Application type: Desktop app → Create"
echo "   → Copy Client ID and Client Secret"
echo ""
echo "5. Run:"
echo ""
echo "   rclone config create gdrive drive \\"
echo "     client_id=YOUR_CLIENT_ID \\"
echo "     client_secret=YOUR_CLIENT_SECRET \\"
echo "     scope=drive"
echo ""
echo "6. Test: rclone lsd gdrive:"
