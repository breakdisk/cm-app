#!/usr/bin/env bash
# Test for scripts/backfill/partner-shipment-links.sh against a DISPOSABLE
# Postgres. Seeds rows under a fresh random tenant and scopes the run to it, so
# it deletes nothing. It still writes to the database it is pointed at, so it
# only runs against a docker container you name.
#
#   TEST_CONTAINER=los-phase0-pg bash scripts/backfill/test-partner-shipment-links.sh
#
# The container needs the identity, carrier, driver_ops and order_intake schemas
# migrated, in one database named by TEST_DB (default postgres).
set -euo pipefail

CONTAINER=${TEST_CONTAINER:?set TEST_CONTAINER to a disposable postgres container}
DB=${TEST_DB:-postgres}
HERE=$(cd "$(dirname "$0")" && pwd)

sql() { docker exec -i "$CONTAINER" psql -U postgres -d "$DB" -v ON_ERROR_STOP=1 -At -q -c "$1"; }
uuid() { sql "SELECT gen_random_uuid()" | head -n 1; }
seed_tenant() {
  local id; id=$(uuid)
  sql "INSERT INTO identity.tenants (id, name, slug, owner_email) VALUES ('$id', 'links-test', 'ln-$id', 'owner@ln.test')" >/dev/null
  echo "$id"
}
run() { # run <tenant> [args...]
  local tenant=$1; shift
  PSQL="docker exec -i $CONTAINER psql -U postgres" \
  CARRIER_DB="$DB" DRIVER_OPS_DB="$DB" ORDER_INTAKE_DB="$DB" TENANT_ID="$tenant" \
    bash "$HERE/partner-shipment-links.sh" "$@"
}
fail=0
check() { if [ "$2" == "$3" ]; then echo "ok   - $1"; else echo "FAIL - $1: expected [$2] got [$3]"; fail=1; fi; }
contains() { if [[ "$3" == *"$2"* ]]; then echo "ok   - $1"; else echo "FAIL - $1: [$2] not in output:"; echo "$3"; fail=1; fi; }
links() { sql "SELECT count(*) FROM order_intake.shipment_partner_links WHERE tenant_id = '$1'"; }

seed_carrier() { # tenant code
  sql "INSERT INTO carrier.carriers (id, tenant_id, name, code, contact_email) VALUES (gen_random_uuid(), '$1', 'c-$2', '$2', '$2@x.test') RETURNING id" | head -n 1
}
seed_driver() { # tenant carrier-or-NULL -> driver id
  local id; id=$(uuid)
  local carrier=${2:+"'$2'"}; carrier=${carrier:-NULL}
  sql "INSERT INTO driver_ops.drivers (id, tenant_id, user_id, first_name, last_name, phone, carrier_id) VALUES ('$id', '$1', '$id', 'D', 'D', '+971500000000', $carrier)" >/dev/null
  echo "$id"
}
seed_task() { # driver shipment
  sql "INSERT INTO driver_ops.tasks (driver_id, route_id, shipment_id, task_type, sequence, address_line1, city, customer_name, customer_phone) VALUES ('$1', gen_random_uuid(), '$2', 'delivery', 1, 'x', 'x', 'x', 'x')" >/dev/null
}
seed_sla() { # tenant carrier shipment
  sql "INSERT INTO carrier.sla_records (tenant_id, carrier_id, shipment_id, zone, promised_by) VALUES ('$1', '$2', '$3', 'AUH', now())" >/dev/null
}

T=$(seed_tenant)
C1=$(seed_carrier "$T" c1); C2=$(seed_carrier "$T" c2)
S_SLA=$(uuid); S_TASK=$(uuid); S_BOTH=$(uuid); S_PLATFORM=$(uuid)

seed_sla "$T" "$C1" "$S_SLA"                  # allocation only
D1=$(seed_driver "$T" "$C2")
seed_task "$D1" "$S_TASK"                      # carrier driver's task only
seed_sla "$T" "$C1" "$S_BOTH"                 # both sources, same carrier
D1b=$(seed_driver "$T" "$C1")
seed_task "$D1b" "$S_BOTH"
DP=$(seed_driver "$T" "")                      # a platform driver: no carrier
seed_task "$DP" "$S_PLATFORM"

out=$(run "$T")
contains "dry-run finds three unique links" "unique=3" "$out"
contains "dry-run counts both sources" "from_sla=2 from_tasks=2" "$out"
check "dry-run writes nothing" "0" "$(links "$T")"

out=$(run "$T" --apply)
contains "apply inserts three" "inserted=3" "$out"
check "three links exist" "3" "$(links "$T")"
check "allocation links its carrier" "$C1" "$(sql "SELECT carrier_id FROM order_intake.shipment_partner_links WHERE shipment_id = '$S_SLA'")"
check "driver task links the driver's carrier" "$C2" "$(sql "SELECT carrier_id FROM order_intake.shipment_partner_links WHERE shipment_id = '$S_TASK'")"
check "a shipment in both sources is one link" "1" "$(sql "SELECT count(*) FROM order_intake.shipment_partner_links WHERE shipment_id = '$S_BOTH'")"
check "platform driver's shipment is not linked" "0" "$(sql "SELECT count(*) FROM order_intake.shipment_partner_links WHERE shipment_id = '$S_PLATFORM'")"

out=$(run "$T" --apply)
contains "re-run finds nothing to insert" "nothing to insert" "$out"
contains "re-run reports them present" "already_present=3" "$out"
check "re-run adds no rows" "3" "$(links "$T")"

# A link the consumer already made is not duplicated or re-labelled.
S_LIVE=$(uuid)
sql "INSERT INTO order_intake.shipment_partner_links (tenant_id, shipment_id, carrier_id, source) VALUES ('$T', '$S_LIVE', '$C1', 'task_assigned')"
seed_sla "$T" "$C1" "$S_LIVE"
run "$T" --apply >/dev/null
check "consumer-made link keeps its source" "task_assigned" "$(sql "SELECT source FROM order_intake.shipment_partner_links WHERE shipment_id = '$S_LIVE'")"

if [ "$fail" -ne 0 ]; then echo "SOME CHECKS FAILED"; exit 1; fi
echo "all checks passed"
