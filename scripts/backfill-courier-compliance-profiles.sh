#!/usr/bin/env bash
# Opens a compliance profile for every courier who registered before field-ops
# started announcing them.
#
# WHY THIS EXISTS
# The compliance service has subscribed to `driver.registered` since the day it
# shipped, and nothing in the platform ever published it. So no courier has a
# compliance profile, the review queue has always been empty, and the gate in
# field-ops has nothing to gate on. field-ops now publishes on registration —
# but registration is idempotent and only fires for a *new* courier, so nobody
# who already signed up will ever be announced. This replays them once.
#
# WHAT IT DOES NOT DO
# It does not make anyone compliant. Each courier lands at `pending_submission`
# and stays there until they submit documents and an admin approves them. That
# is the point: the admin console can finally see who is outstanding.
#
# SAFE TO RE-RUN. Compliance's handler checks for an existing profile before
# creating one, so a second run is a no-op rather than a duplicate.
#
# ⚠ DO NOT run this and then immediately set ENFORCE_COMPLIANCE=true. Every
# courier backfilled here becomes non-assignable the moment enforcement is on,
# because `pending_submission` is not an assignable status. Backfill, let the
# fleet submit documents, watch "Compliance blocked" on the admin couriers page
# fall to zero, and only then enforce.
#
#   Usage: bash scripts/backfill-courier-compliance-profiles.sh [--apply]
#
# Without --apply it prints what it would publish and exits (dry run).

set -euo pipefail

APPLY="${1:-}"
JURISDICTION="${DEFAULT_JURISDICTION:-PH}"
TOPIC="driver"

# Prefer the platform's own containers by exact name, and only then fall back to
# a fuzzy match.
#
# `grep -i postgres | head -1` on its own is a live foot-gun on the VPS: Dokploy
# runs its **own** control-plane Postgres, and its container sorts first, so the
# script aimed a backfill at the orchestrator's database. It failed safely here
# only because that database has no `logisticos` role — a box where the names
# collide differently would have run the query somewhere real.
#
# The fallback deliberately excludes anything named `dokploy`. If more than one
# candidate survives, say so and stop rather than picking: guessing which
# database to write to is not a decision a script should make silently.
pick_container() {
    local exact="$1" pattern="$2" kind="$3"
    if docker ps --format '{{.Names}}' | grep -qx "$exact"; then
        echo "$exact"; return
    fi
    local matches
    matches="$(docker ps --format '{{.Names}}' | grep -i "$pattern" | grep -vi dokploy || true)"
    local n
    n="$(printf '%s\n' "$matches" | grep -c . || true)"
    if [ "$n" -gt 1 ]; then
        echo "ERROR: several $kind containers match and none is named '$exact':" >&2
        printf '  %s\n' $matches >&2
        echo "Set ${kind^^}_CONTAINER=<name> explicitly." >&2
        exit 1
    fi
    printf '%s' "$matches" | head -1
}

PG_CONTAINER="${PG_CONTAINER:-$(pick_container logisticos-postgres postgres pg)}"
KAFKA_CONTAINER="${KAFKA_CONTAINER:-$(pick_container logisticos-kafka kafka kafka)}"
# The internal listener, not localhost:9092 — see create-kafka-topics.sh for
# why localhost hangs from inside the container on the VPS deployment.
BOOTSTRAP="${KAFKA_BOOTSTRAP:-kafka:29092}"

[ -n "$PG_CONTAINER" ]    || { echo "ERROR: no postgres container found. Set PG_CONTAINER=<name>."; exit 1; }
[ -n "$KAFKA_CONTAINER" ] || { echo "ERROR: no kafka container found. Set KAFKA_CONTAINER=<name>."; exit 1; }

echo "Postgres container: $PG_CONTAINER"
echo "Kafka container:    $KAFKA_CONTAINER"
echo "Jurisdiction:       $JURISDICTION"
echo

# `user_id`, not `id`. Compliance stores the announced id as the profile's
# `entity_id` and sends it back on every status change, and field-ops looks a
# courier up by `user_id` when that verdict arrives. ADR-0015 makes the two
# equal today; depending on that here would break silently the day it changed.
ROWS="$(docker exec -i "$PG_CONTAINER" psql -U logisticos -d svc_field_ops -At -F'|' -c \
  "SELECT tenant_id, user_id FROM field_ops.couriers ORDER BY created_at")"

if [ -z "$ROWS" ]; then
  echo "No couriers found. Nothing to backfill."
  exit 0
fi

COUNT="$(printf '%s\n' "$ROWS" | wc -l | tr -d ' ')"
echo "$COUNT courier(s) to announce."
echo

# Find the console producer the same way create-kafka-topics.sh finds
# kafka-topics: the path differs between image flavours.
PRODUCER_BIN=""
for candidate in \
    "/usr/bin/kafka-console-producer" \
    "/opt/kafka/bin/kafka-console-producer.sh" \
    "/opt/bitnami/kafka/bin/kafka-console-producer.sh" \
    "/usr/local/bin/kafka-console-producer"; do
  if docker exec "$KAFKA_CONTAINER" test -x "$candidate" 2>/dev/null; then
    PRODUCER_BIN="$candidate"
    break
  fi
done
[ -n "$PRODUCER_BIN" ] || { echo "ERROR: kafka-console-producer not found in $KAFKA_CONTAINER."; exit 1; }

PAYLOADS=""
while IFS='|' read -r TENANT USER_ID; do
  [ -n "$TENANT" ] || continue
  # Matches libs/events Event<T> exactly. `data.tenant_id` is duplicated from
  # the envelope on purpose: compliance's DriverRegisteredPayload declares it
  # without #[serde(default)], so omitting it fails deserialisation on their
  # side and silently creates nothing.
  #
  # `id` is derived from the courier so a re-run produces the same event id
  # rather than a fresh one each time.
  EVENT_ID="$(printf '%s' "$USER_ID" | sed 's/^\(........\)-\(....\)-....-/\1-\2-4000-/')"
  PAYLOADS="${PAYLOADS}{\"id\":\"${EVENT_ID}\",\"source\":\"logisticos/backfill\",\"event_type\":\"driver.registered\",\"time\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"tenant_id\":\"${TENANT}\",\"data\":{\"driver_id\":\"${USER_ID}\",\"tenant_id\":\"${TENANT}\",\"jurisdiction\":\"${JURISDICTION}\"}}
"
done <<EOF
$ROWS
EOF

if [ "$APPLY" != "--apply" ]; then
  echo "DRY RUN — nothing published. Would send to topic '$TOPIC':"
  echo
  printf '%s' "$PAYLOADS"
  echo
  echo "Re-run with --apply to publish."
  exit 0
fi

printf '%s' "$PAYLOADS" | docker exec -i "$KAFKA_CONTAINER" \
  "$PRODUCER_BIN" --bootstrap-server "$BOOTSTRAP" --topic "$TOPIC"

echo
echo "Published $COUNT registration event(s) to '$TOPIC'."
echo
echo "Verify — profiles should appear as entity_type='driver':"
echo "  docker exec $PG_CONTAINER psql -U logisticos -d svc_compliance -c \\"
echo "    \"SELECT entity_type, overall_status, count(*) FROM compliance.compliance_profiles GROUP BY 1,2;\""
echo
echo "If the count does not move, check the compliance service log for"
echo "'Failed to deserialize driver event' — that is a payload-shape mismatch,"
echo "not a delivery failure, and it is silent on the producer side."
