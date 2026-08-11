#!/usr/bin/env bash
# Every permission a handler demands must be reachable by some role.
#
# `require_permission!(claims, permissions::X)` and
# `default_permissions_for_role()` are two hand-maintained lists that have to
# agree, and nothing made them. When they disagree the endpoint does not fail
# loudly — it returns a clean, plausible 403 to every caller forever, which
# reads as "you lack access" rather than "this is unreachable by anyone".
#
# Found twice on 2026-08-11, both long-standing:
#
#   * the engagement service declared `engagement:read` / `engagement:send` /
#     `engagement:templates:write` as private string literals in its own
#     handler file. They matched no catalogue entry, so all 11 of its
#     endpoints — including the customer app's notification history — were
#     dead. (Those constants now live in the catalogue and are imported, so
#     that half is a compile error rather than a silent mismatch.)
#
#   * `tenants:manage` gated the tenant profile update and the white-label
#     branding write, and no role granted it. Combined with an ambiguous-column
#     500 on the public read, the entire white-label feature was unreachable
#     from both ends at once — which is precisely why neither end got reported.
#
# The fix for the second was NOT to grant `tenants:manage`: the same constant
# gates `PUT /v1/pricing/features/:key/tiers`, which takes no tenant id and
# rewrites the pricing matrix for every tenant, and the tier upgrade, which
# would be a free self-upgrade to Enterprise. Those stay platform-only, reached
# through the `*` wildcard. Permissions in that position are listed in
# PLATFORM_ONLY below and must carry a reason.
#
# Reads the same files a reviewer would; needs no database and no cluster.
set -euo pipefail

cd "$(dirname "$0")/.."

RBAC="libs/auth/src/rbac.rs"
[ -f "$RBAC" ] || { echo "FAIL: $RBAC not found"; exit 1; }

# Permissions that intentionally belong to no role. Each needs a reason.
declare -A PLATFORM_ONLY=(
  [TENANT_MANAGE]="platform-scoped: rewrites the global pricing matrix and grants tier upgrades; platform admins hold the '*' wildcard"
  [BILLING_SETUP]="minted directly by AuthService::exchange_firebase for draft tenants during onboarding, never via a role"
)

# Resolve every constant to the string it actually carries. This has to compare
# *values*, not names: the catalogue deliberately aliases several constants onto
# one string (FLEET_VIEW is "fleet:read", the same as FLEET_READ; BILLING_VIEW
# is "payments:read"). A name-based check calls those unreachable and fails CI
# on three working endpoints.
declare -A VALUE_OF
while IFS='|' read -r name value; do
  [ -n "$name" ] && VALUE_OF[$name]="$value"
done < <(grep -oE 'pub const [A-Z0-9_]+:\s*&str\s*=\s*"[^"]+"' "$RBAC" \
           | sed -E 's/pub const ([A-Z0-9_]+):\s*&str\s*=\s*"([^"]+)"/\1|\2/')

# What roles can actually be granted. Scoped to the body of the role table so a
# constant merely *defined* above does not count as granted.
granted=$(
  for n in $(sed -n '/pub fn default_permissions_for_role/,/^}/p' "$RBAC" \
               | grep -oE 'permissions::[A-Z0-9_]+' | sed 's/permissions:://' | sort -u); do
    echo "${VALUE_OF[$n]:-$n}"
  done | sort -u
)

# What handlers actually demand.
#
# Matching `permissions::NAME` is not enough: a grouped import spreads the names
# across lines away from the `permissions::` prefix —
#
#     use logisticos_auth::rbac::permissions::{
#         ENGAGEMENT_READ as PERM_READ,
#         ...
#     };
#
# — so a prefix-anchored pattern silently finds nothing, and the check passes on
# exactly the code that motivated it. It did: revoking ENGAGEMENT_READ from every
# role still reported OK until this was fixed. Match the catalogue names as bare
# words instead. They are unique SCREAMING_SNAKE identifiers, and `_` is a word
# character, so ENGAGEMENT_READ does not match inside ENGAGEMENT_READ_OWN.
required=$(
  for n in "${!VALUE_OF[@]}"; do
    if grep -rqE "\b${n}\b" --include='*.rs' services/ 2>/dev/null; then echo "$n"; fi
  done | sort -u
)

fail=0
for perm in $required; do
  # Defined-but-unused constants are a different (harmless) thing; we only care
  # about ones a handler demands. Skip anything the role table itself grants.
  value="${VALUE_OF[$perm]:-$perm}"
  if grep -qxF "$value" <<<"$granted"; then
    continue
  fi

  # Where it is demanded, for the failure message.
  used=$(grep -rlE "\b${perm}\b" --include='*.rs' services/ 2>/dev/null || true)
  [ -n "$used" ] || continue

  if [ -n "${PLATFORM_ONLY[$perm]+x}" ]; then
    echo "  ok (platform-only)  $perm — ${PLATFORM_ONLY[$perm]}"
    continue
  fi

  echo "FAIL: $perm (\"$value\") is required by a handler but granted by no role."
  echo "      Every caller gets 403 and the endpoint is unreachable:"
  sed 's/^/        /' <<<"$used"
  echo "      Grant it in default_permissions_for_role(), or — if it is meant"
  echo "      to be platform-admin only — add it to PLATFORM_ONLY in this"
  echo "      script with the reason why."
  fail=1
done

if [ "$fail" -ne 0 ]; then
  exit 1
fi

echo "OK: every permission demanded by a handler is grantable."
