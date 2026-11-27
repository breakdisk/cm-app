#!/usr/bin/env bash
# Backfill order_intake.shipment_partner_links for shipments that a carrier
# already handled before the link consumer existed.
#
# Why: a partner token is scoped to shipments linked to its carrier. The
# consumer links new work as it happens; this covers everything before it, so a
# partner does not see its own history vanish when scoping is enforced.
#
# Sources, both already recorded elsewhere:
#   carrier.sla_records            a carrier allocation for a shipment
#   driver_ops.tasks + drivers     a task given to one of a carrier's drivers
#
# Dry-run unless --apply. Idempotent: re-running adds nothing. A link to a
# shipment that no longer exists is harmless (nothing reads a link without its
# shipment), so existence is not checked.
#
#   PSQL          psql command prefix (default: psql). On the VPS:
#                 PSQL="docker exec -i logisticos-postgres psql -U logisticos"
#   CARRIER_DB / DRIVER_OPS_DB / ORDER_INTAKE_DB   database names
#   TENANT_ID     restrict every read to one tenant (optional)
#
# Usage: partner-shipment-links.sh [--apply]
set -euo pipefail

APPLY=0
for arg in "$@"; do
  case "$arg" in
    --apply)   APPLY=1 ;;
    --dry-run) APPLY=0 ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed 's/^# \{0,1\}//' | head -n -1; exit 0 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

# sort and comm must agree on collation or `comm` silently mis-pairs rows.
export LC_ALL=C

PSQL=${PSQL:-psql}
CARRIER_DB=${CARRIER_DB:-svc_carrier}
DRIVER_OPS_DB=${DRIVER_OPS_DB:-svc_driver_ops}
ORDER_INTAKE_DB=${ORDER_INTAKE_DB:-svc_order_intake}
TENANT_ID=${TENANT_ID:-}

UUID_RE='^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
if [ -n "$TENANT_ID" ] && ! [[ "$TENANT_ID" =~ $UUID_RE ]]; then
  echo "TENANT_ID is not a lower-case uuid: $TENANT_ID" >&2; exit 2
fi
tenant_filter() { # tenant_filter <column>
  [ -n "$TENANT_ID" ] && printf "AND %s = '%s'" "$1" "$TENANT_ID" || true
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

q() {
  # PSQL is deliberately word-split: it is a command prefix.
  # shellcheck disable=SC2086
  $PSQL -d "$1" -v ON_ERROR_STOP=1 -At -F '|' -c "$2"
}

q "$CARRIER_DB" "SELECT DISTINCT tenant_id, shipment_id, carrier_id FROM carrier.sla_records WHERE true $(tenant_filter tenant_id)" > "$work/sla"
q "$DRIVER_OPS_DB" "SELECT DISTINCT d.tenant_id, t.shipment_id, d.carrier_id FROM driver_ops.tasks t JOIN driver_ops.drivers d ON d.id = t.driver_id WHERE d.carrier_id IS NOT NULL $(tenant_filter d.tenant_id)" > "$work/tasks"
q "$ORDER_INTAKE_DB" "SELECT tenant_id, shipment_id, carrier_id FROM order_intake.shipment_partner_links WHERE true $(tenant_filter tenant_id)" > "$work/existing"

# Well-formed ids only, before anything nears SQL.
invalid=0
: > "$work/clean"
while IFS='|' read -r t s c; do
  if [[ "$t" =~ $UUID_RE && "$s" =~ $UUID_RE && "$c" =~ $UUID_RE ]]; then
    printf '%s|%s|%s\n' "$t" "$s" "$c" >> "$work/clean"
  else
    invalid=$((invalid + 1))
  fi
done < <(cat "$work/sla" "$work/tasks")

sort -u "$work/clean" > "$work/unique"
sort -u "$work/existing" > "$work/existing.sorted"
# New = in unique, not in existing. comm needs both inputs sorted the same way.
comm -23 "$work/unique" "$work/existing.sorted" > "$work/insert"

count() { wc -l < "$1" | tr -d ' '; }
echo "from_sla=$(count "$work/sla") from_tasks=$(count "$work/tasks") unique=$(count "$work/unique")"
echo "  already_present=$(( $(count "$work/unique") - $(count "$work/insert") )) would_insert=$(count "$work/insert") skipped_invalid=$invalid"

to_insert=$(count "$work/insert")
if [ "$APPLY" -ne 1 ]; then
  echo "dry-run: nothing written. Re-run with --apply to insert $to_insert link(s)."
  exit 0
fi
if [ "$to_insert" -eq 0 ]; then
  echo "apply: nothing to insert."
  exit 0
fi

# 500 rows per statement keeps each one small enough for any psql -c limit.
split -l 500 "$work/insert" "$work/chunk."
inserted=0
for chunk in "$work"/chunk.*; do
  values=$(awk -F'|' '{ printf "%s('"'"'%s'"'"','"'"'%s'"'"','"'"'%s'"'"','"'"'backfill'"'"')", (NR > 1 ? "," : ""), $1, $2, $3 }' "$chunk")
  n=$(q "$ORDER_INTAKE_DB" "WITH ins AS (INSERT INTO order_intake.shipment_partner_links (tenant_id, shipment_id, carrier_id, source) VALUES $values ON CONFLICT DO NOTHING RETURNING 1) SELECT count(*) FROM ins")
  inserted=$((inserted + n))
done
echo "apply: inserted=$inserted (of $to_insert candidates)"
