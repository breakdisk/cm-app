#!/usr/bin/env bash
# Test for scripts/backfill/partner-memberships.sh against a DISPOSABLE Postgres.
#
# It seeds rows under fresh random tenants and scopes every run to them with
# TENANT_ID, so it deletes nothing and a re-run starts clean. It still writes
# to the database it is pointed at, so it only runs against a docker container
# you name: never give it a production container.
#
#   TEST_CONTAINER=los-phase0-pg bash scripts/backfill/test-partner-memberships.sh
#
# The container needs the identity, carrier and driver_ops schemas migrated, in
# one database named by TEST_DB (default postgres).
set -euo pipefail

CONTAINER=${TEST_CONTAINER:?set TEST_CONTAINER to a disposable postgres container}
DB=${TEST_DB:-postgres}
HERE=$(cd "$(dirname "$0")" && pwd)

sql() { docker exec -i "$CONTAINER" psql -U postgres -d "$DB" -v ON_ERROR_STOP=1 -At -q -c "$1"; }
uuid() { sql "SELECT gen_random_uuid()" | head -n 1; }
# identity.users.tenant_id is a foreign key, so a test tenant must really exist.
seed_tenant() {
  local id; id=$(uuid)
  sql "INSERT INTO identity.tenants (id, name, slug, owner_email) VALUES ('$id', 'backfill-test', 'bf-$id', 'owner@bf.test')" >/dev/null
  echo "$id"
}
run() { # run <tenant> [args...]
  local tenant=$1; shift
  PSQL="docker exec -i $CONTAINER psql -U postgres" \
  IDENTITY_DB="$DB" CARRIER_DB="$DB" DRIVER_OPS_DB="$DB" TENANT_ID="$tenant" \
    bash "$HERE/partner-memberships.sh" "$@"
}
fail=0
check() { # check <name> <expected> <actual>
  if [ "$2" == "$3" ]; then echo "ok   - $1"; else echo "FAIL - $1: expected [$2] got [$3]"; fail=1; fi
}
contains() { # contains <name> <needle> <haystack>
  if [[ "$3" == *"$2"* ]]; then echo "ok   - $1"; else echo "FAIL - $1: [$2] not in output:"; echo "$3"; fail=1; fi
}
members() { sql "SELECT count(*) FROM identity.partner_memberships WHERE tenant_id = '$1'"; }

seed_user() { # tenant email roles-literal
  sql "INSERT INTO identity.users (tenant_id, email, first_name, last_name, roles, is_active) VALUES ('$1', '$2', 'T', 'T', $3, true) RETURNING id" | head -n 1
}
seed_carrier() { # tenant code email
  sql "INSERT INTO carrier.carriers (id, tenant_id, name, code, contact_email) VALUES (gen_random_uuid(), '$1', 'c-$2', '$2', '$3') RETURNING id" | head -n 1
}

# ── Tenant 1: the plan's scenario ────────────────────────────────────────────
T1=$(seed_tenant)
UA=$(seed_user "$T1" 'A@Example.com' "ARRAY['partner']")   # mixed-case login email
UB=$(seed_user "$T1" 'b@example.com' "ARRAY['partner']")
UC=$(seed_user "$T1" 'c@example.com' "ARRAY['partner']")   # will match two carriers
CA=$(seed_carrier "$T1" ca 'a@example.com')
CB=$(seed_carrier "$T1" cb 'b@example.com')
seed_carrier "$T1" cx 'nobody@example.com' >/dev/null      # no user
seed_carrier "$T1" cc1 'c@example.com' >/dev/null
seed_carrier "$T1" cc2 'C@example.com' >/dev/null          # mixed-case carrier email

out=$(run "$T1")
contains "dry-run counts" "matched=2 unmatched_email=1 multiple_carriers=1" "$out"
check "dry-run writes nothing" "0" "$(members "$T1")"

out=$(run "$T1" --apply)
contains "apply inserts two" "inserted=2" "$out"
check "two memberships exist" "2" "$(members "$T1")"
check "mixed-case user bound to its carrier" "$CA" "$(sql "SELECT carrier_id FROM identity.partner_memberships WHERE user_id = '$UA'")"
check "second user bound to its carrier" "$CB" "$(sql "SELECT carrier_id FROM identity.partner_memberships WHERE user_id = '$UB'")"
check "user on two carriers is not inserted" "0" "$(sql "SELECT count(*) FROM identity.partner_memberships WHERE user_id = '$UC'")"

out=$(run "$T1" --apply)
contains "re-run finds nothing to insert" "nothing to insert" "$out"
contains "re-run reports them present" "already_present=2" "$out"
check "re-run adds no rows" "2" "$(members "$T1")"

# ── Tenant 2: only a user WITHOUT the partner role shares a carrier's email ──
T2=$(seed_tenant)
seed_user "$T2" 'd@example.com' "ARRAY['merchant']" >/dev/null
seed_carrier "$T2" cd 'd@example.com' >/dev/null
out=$(run "$T2" --apply)
contains "non-partner user is not matched" "matched=0 unmatched_email=1" "$out"
check "non-partner user gets no membership" "0" "$(members "$T2")"

# ── Tenant 3: an existing active membership is never changed ─────────────────
T3=$(seed_tenant)
UE=$(seed_user "$T3" 'e@example.com' "ARRAY['partner']")
CE=$(seed_carrier "$T3" ce 'e@example.com')
OTHER=$(uuid)
sql "INSERT INTO identity.partner_memberships (tenant_id, user_id, carrier_id, role) VALUES ('$T3', '$UE', '$OTHER', 'partner')"
out=$(run "$T3" --apply)
contains "held by another carrier is reported" "held_by_other_carrier=1" "$out"
check "existing membership is untouched" "$OTHER" "$(sql "SELECT carrier_id FROM identity.partner_memberships WHERE user_id = '$UE'")"

# ── Tenant 4: riders are opt-in ──────────────────────────────────────────────
T4=$(seed_tenant)
UR=$(seed_user "$T4" 'rider@driver.test' "ARRAY['driver']")
CR=$(uuid)
sql "INSERT INTO driver_ops.drivers (id, tenant_id, user_id, first_name, last_name, phone, carrier_id) VALUES ('$UR', '$T4', '$UR', 'R', 'R', '+971500000000', '$CR')"
out=$(run "$T4")
contains "riders are off by default" "matched=0" "$out"
out=$(run "$T4" --riders --apply)
contains "riders matched when asked" "matched=1" "$out"
check "rider bound to its carrier as store_rider" "$CR|store_rider" "$(sql "SELECT carrier_id || '|' || role FROM identity.partner_memberships WHERE user_id = '$UR'")"

if [ "$fail" -ne 0 ]; then echo "SOME CHECKS FAILED"; exit 1; fi
echo "all checks passed"
