#!/usr/bin/env bash
# ============================================================
# LogisticOS — Cloudflare R2 Bucket Provisioning
# ============================================================
# Creates the two R2 buckets required for POD and POP photo evidence.
# Run once per environment (staging, production).
#
# Prerequisites:
#   - Cloudflare Wrangler CLI installed: npm install -g wrangler
#   - Authenticated: wrangler login   (or set CLOUDFLARE_API_TOKEN)
#
# Usage:
#   chmod +x scripts/create-r2-buckets.sh
#   ./scripts/create-r2-buckets.sh [--preview]   # --preview creates "...-preview" buckets
#
# After running this script, set the following in your Dokploy
# environment variables (pod service):
#
#   S3_ENDPOINT_URL  = https://<account-id>.r2.cloudflarestorage.com
#   S3_BUCKET        = logisticos-pod-photos
#   POP_S3_BUCKET    = logisticos-pop-photos
#   S3__ACCESS_KEY_ID      = <32-char R2 access key>
#   S3__SECRET_ACCESS_KEY  = <R2 secret key>
# ============================================================

set -euo pipefail

CYAN='\033[0;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
RESET='\033[0m'

info()    { echo -e "${CYAN}[INFO]${RESET}  $*"; }
success() { echo -e "${GREEN}[OK]${RESET}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
die()     { echo -e "${RED}[FAIL]${RESET}  $*" >&2; exit 1; }

# ── Parse args ──────────────────────────────────────────────
PREVIEW=0
for arg in "$@"; do
  case "$arg" in
    --preview) PREVIEW=1 ;;
    *) die "Unknown argument: $arg" ;;
  esac
done

# ── Bucket names ─────────────────────────────────────────────
POD_BUCKET="logisticos-pod-photos"
POP_BUCKET="logisticos-pop-photos"

if [[ $PREVIEW -eq 1 ]]; then
  POD_BUCKET="${POD_BUCKET}-preview"
  POP_BUCKET="${POP_BUCKET}-preview"
  warn "Preview mode — using bucket names: ${POD_BUCKET}, ${POP_BUCKET}"
fi

# ── Verify wrangler is available ─────────────────────────────
if ! command -v wrangler &>/dev/null; then
  die "wrangler CLI not found. Install with: npm install -g wrangler"
fi

# ── Check authentication ─────────────────────────────────────
if [[ -z "${CLOUDFLARE_API_TOKEN:-}" ]]; then
  info "CLOUDFLARE_API_TOKEN not set — using wrangler interactive login state."
  info "If this fails, run: wrangler login"
fi

# ── Create POD bucket ────────────────────────────────────────
info "Creating POD evidence bucket: ${POD_BUCKET}"
if wrangler r2 bucket create "${POD_BUCKET}" 2>&1; then
  success "Created bucket: ${POD_BUCKET}"
else
  warn "Bucket '${POD_BUCKET}' may already exist — continuing."
fi

# ── Create POP bucket ────────────────────────────────────────
info "Creating POP evidence bucket: ${POP_BUCKET}"
if wrangler r2 bucket create "${POP_BUCKET}" 2>&1; then
  success "Created bucket: ${POP_BUCKET}"
else
  warn "Bucket '${POP_BUCKET}' may already exist — continuing."
fi

# ── CORS configuration ───────────────────────────────────────
# R2 presigned URLs are signed at the key level — browsers fetch them via
# <img src="..."> tags which do not send CORS preflight requests.
# CORS is only needed if you use fetch() with credentials to display photos.
# If you encounter CORS errors in browser DevTools when images fail to load,
# configure CORS via the Cloudflare dashboard or wrangler r2 bucket cors put.
info "CORS: not configured automatically — presigned <img> loads do not require CORS."
info "If you use fetch() for photos, configure CORS in the Cloudflare R2 dashboard."

# ── Summary ──────────────────────────────────────────────────
echo ""
echo -e "${GREEN}R2 buckets ready:${RESET}"
echo -e "  • ${POD_BUCKET}  — Proof of Delivery photos"
echo -e "  • ${POP_BUCKET}  — Proof of Pickup photos"
echo ""
echo -e "${YELLOW}Next: Set these env vars in Dokploy (pod service):${RESET}"
echo ""
echo "  S3_ENDPOINT_URL       = https://<account_id>.r2.cloudflarestorage.com"
echo "  S3_BUCKET             = ${POD_BUCKET}"
echo "  POP_S3_BUCKET         = ${POP_BUCKET}"
echo "  S3_REGION             = auto"
echo "  S3__ACCESS_KEY_ID     = <32-char R2 access key>"
echo "  S3__SECRET_ACCESS_KEY = <R2 secret key>"
echo ""
echo "  Generate an R2 API token at:"
echo "  https://dash.cloudflare.com/?to=/:account/r2/api-tokens"
echo "  (Permissions: Object Read & Write on both buckets)"
