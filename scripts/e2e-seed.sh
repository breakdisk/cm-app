#!/usr/bin/env bash
# ============================================================
# E2E auth seed script
# ============================================================
# Mints LogisticOS JWT pairs for the Playwright suite by calling the
# identity service's internal exchange endpoint. Exports four env vars
# that match what e2e/fixtures/cookies.ts reads:
#   TEST_LOS_ACCESS_TOKEN / TEST_LOS_REFRESH_TOKEN   (active tenant)
#   TEST_DRAFT_ACCESS_TOKEN / TEST_DRAFT_REFRESH_TOKEN (draft tenant)
#
# Expected env in:
#   IDENTITY_URL                  — e.g. https://identity.staging.logisticos.io
#   LOGISTICOS_INTERNAL_SECRET    — shared secret for the /internal router
#   E2E_ACTIVE_FIREBASE_UID       — UID for a pre-seeded active tenant
#   E2E_ACTIVE_EMAIL              — email for the same
#   E2E_DRAFT_FIREBASE_UID        — UID whose first exchange creates a draft
#   E2E_DRAFT_EMAIL               — email for the draft
#
# Output sink:
#   $GITHUB_ENV if set (CI)       — exports persist across steps
#   otherwise: stdout eval-able   — use: `eval "$(./scripts/e2e-seed.sh)"`
# ============================================================

set -euo pipefail

: "${IDENTITY_URL:?IDENTITY_URL is required}"
: "${LOGISTICOS_INTERNAL_SECRET:?LOGISTICOS_INTERNAL_SECRET is required}"
: "${E2E_ACTIVE_FIREBASE_UID:?E2E_ACTIVE_FIREBASE_UID is required}"
: "${E2E_ACTIVE_EMAIL:?E2E_ACTIVE_EMAIL is required}"
: "${E2E_DRAFT_FIREBASE_UID:?E2E_DRAFT_FIREBASE_UID is required}"
: "${E2E_DRAFT_EMAIL:?E2E_DRAFT_EMAIL is required}"

exchange() {
  local firebase_uid="$1"
  local email="$2"
  local role="$3"
  curl -sS -X POST "${IDENTITY_URL}/v1/internal/auth/exchange-firebase" \
    -H "Content-Type: application/json" \
    -H "X-Internal-Secret: ${LOGISTICOS_INTERNAL_SECRET}" \
    -d "$(cat <<JSON
{
  "firebase_uid":   "${firebase_uid}",
  "email":          "${email}",
  "email_verified": true,
  "role":           "${role}"
}
JSON
)"
}

emit() {
  local key="$1"
  local value="$2"
  if [[ -n "${GITHUB_ENV:-}" ]]; then
    printf '%s=%s\n' "$key" "$value" >> "$GITHUB_ENV"
  else
    printf 'export %s=%q\n' "$key" "$value"
  fi
}

active_response="$(exchange "$E2E_ACTIVE_FIREBASE_UID" "$E2E_ACTIVE_EMAIL" "merchant")"
draft_response="$(exchange  "$E2E_DRAFT_FIREBASE_UID"  "$E2E_DRAFT_EMAIL"  "merchant")"

active_access="$(echo  "$active_response" | python3 -c 'import sys,json; print(json.load(sys.stdin)["access_token"])')"
active_refresh="$(echo "$active_response" | python3 -c 'import sys,json; print(json.load(sys.stdin)["refresh_token"])')"
draft_access="$(echo   "$draft_response"  | python3 -c 'import sys,json; print(json.load(sys.stdin)["access_token"])')"
draft_refresh="$(echo  "$draft_response"  | python3 -c 'import sys,json; print(json.load(sys.stdin)["refresh_token"])')"

if [[ -z "$active_access" || -z "$draft_access" ]]; then
  echo "Seed failed — missing tokens in identity response" >&2
  exit 1
fi

emit TEST_LOS_ACCESS_TOKEN    "$active_access"
emit TEST_LOS_REFRESH_TOKEN   "$active_refresh"
emit TEST_DRAFT_ACCESS_TOKEN  "$draft_access"
emit TEST_DRAFT_REFRESH_TOKEN "$draft_refresh"

echo "E2E seed complete" >&2
