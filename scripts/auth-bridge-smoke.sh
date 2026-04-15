#!/usr/bin/env bash
# ============================================================
# Auth bridge smoke test
# ============================================================
# Curl-only sanity check for the Firebase → LogisticOS JWT bridge.
# Run against any environment (local / staging / prod read-only checks).
#
# Exits non-zero on any failure so it can be wired into a post-deploy
# step. Prints a summary table.
#
# Usage:
#   IDENTITY_URL=... LANDING_URL=... \
#   LOGISTICOS_INTERNAL_SECRET=... \
#   E2E_ACTIVE_FIREBASE_UID=... E2E_ACTIVE_EMAIL=... \
#   ./scripts/auth-bridge-smoke.sh
# ============================================================

set -euo pipefail

: "${IDENTITY_URL:?}"
: "${LANDING_URL:?}"
: "${LOGISTICOS_INTERNAL_SECRET:?}"
: "${E2E_ACTIVE_FIREBASE_UID:?}"
: "${E2E_ACTIVE_EMAIL:?}"

pass=0
fail=0

check() {
  local name="$1"
  local expected="$2"
  local actual="$3"
  if [[ "$actual" == "$expected" ]]; then
    printf '  ✓ %s (%s)\n' "$name" "$actual"
    pass=$((pass + 1))
  else
    printf '  ✗ %s — expected %s, got %s\n' "$name" "$expected" "$actual" >&2
    fail=$((fail + 1))
  fi
}

echo "== Auth bridge smoke =="
echo "Identity: $IDENTITY_URL"
echo "Landing:  $LANDING_URL"
echo

# 1. Health
status="$(curl -sS -o /dev/null -w '%{http_code}' "$IDENTITY_URL/health" || true)"
check "identity /health" 200 "$status"

# 2. Internal exchange rejects missing secret
status="$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$IDENTITY_URL/v1/internal/auth/exchange-firebase" \
  -H 'Content-Type: application/json' \
  -d '{"firebase_uid":"x","email":"x@example.com","email_verified":true,"role":"merchant"}' || true)"
check "internal exchange w/o secret → 401" 401 "$status"

# 3. Internal exchange succeeds with secret
body="$(curl -sS -X POST "$IDENTITY_URL/v1/internal/auth/exchange-firebase" \
  -H 'Content-Type: application/json' \
  -H "X-Internal-Secret: $LOGISTICOS_INTERNAL_SECRET" \
  -d "{\"firebase_uid\":\"$E2E_ACTIVE_FIREBASE_UID\",\"email\":\"$E2E_ACTIVE_EMAIL\",\"email_verified\":true,\"role\":\"merchant\"}" || true)"
if echo "$body" | grep -q '"access_token"'; then
  printf '  ✓ internal exchange w/ secret → access_token present\n'
  pass=$((pass + 1))
  refresh="$(echo "$body" | python3 -c 'import sys,json; print(json.load(sys.stdin)["refresh_token"])')"
else
  printf '  ✗ internal exchange w/ secret failed: %s\n' "$body" >&2
  fail=$((fail + 1))
  refresh=""
fi

# 4. Refresh route rejects missing cookie
status="$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$LANDING_URL/api/auth/refresh" || true)"
check "landing /api/auth/refresh w/o cookie → 401" 401 "$status"

# 5. Refresh route accepts valid refresh cookie
if [[ -n "$refresh" ]]; then
  status="$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$LANDING_URL/api/auth/refresh" \
    --cookie "__los_rt=$refresh" || true)"
  check "landing /api/auth/refresh w/ cookie → 200" 200 "$status"
fi

# 6. Tenants finalize rejects missing session
status="$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$LANDING_URL/api/tenants/finalize" \
  -H 'Content-Type: application/json' \
  -d '{"business_name":"x","currency":"USD","region":"US"}' || true)"
check "landing /api/tenants/finalize w/o session → 401" 401 "$status"

echo
printf '== Summary: %d passed, %d failed ==\n' "$pass" "$fail"
exit $(( fail > 0 ? 1 : 0 ))
