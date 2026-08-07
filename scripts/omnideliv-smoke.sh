#!/usr/bin/env bash
# End-to-end smoke test through the gateway, exercising the manual order path.
#
# Deliberately the manual path, not the mesh: it must pass with no Claude
# credentials, so CI can run it on every push without an API key or spend.
#
# Run scripts/seed-omnideliv.sh first — and re-run it if this last passed more
# than ten minutes ago, because find_available_near only considers GPS fixes
# from the last ten minutes and a stale courier reads as "no courier available".
set -euo pipefail

GW="${GW:-http://localhost:8100}"
OMNIDELIV="${OMNIDELIV:-http://localhost:8091}"
FIELD_OPS="${FIELD_OPS:-http://localhost:8090}"
TOKEN="${TOKEN:?set TOKEN to a customer JWT for the dev tenant}"

KUYAS="11111111-0000-0000-0000-000000000001"
TAPSILOG="22222222-0000-0000-0000-000000000001"

api() {
  curl -sf -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" "$@"
}

json() { sed -n "s/.*\"$1\":\"\?\([^\",}]*\)\"\?.*/\1/p"; }

echo "1/6  health"
curl -sf "$OMNIDELIV/health" >/dev/null && echo "     omnideliv healthy"
curl -sf "$FIELD_OPS/health" >/dev/null && echo "     field-ops healthy"

echo "2/6  catalog search"
# Not /v1/omnideliv/vendors: that endpoint does not exist yet (Plan 9 Task 4).
# Searching a known vendor's catalog proves the same thing this step is for —
# that the gateway routes to omnideliv and the data is there.
api "$GW/v1/omnideliv/catalog/search?vendor_id=$KUYAS&q=Tapsilog" | grep -q "$TAPSILOG" \
  && echo "     Tapsilog found at Kuya's"

echo "3/6  create basket"
BASKET=$(api -X POST "$GW/v1/omnideliv/baskets" -d '{}' | json id)
[ -n "$BASKET" ] || { echo "     no basket returned"; exit 1; }
echo "     basket $BASKET"

echo "4/6  add a line"
api -X POST "$GW/v1/omnideliv/baskets/$BASKET/lines" \
    -d "{\"vendor_id\":\"$KUYAS\",\"item_id\":\"$TAPSILOG\",\"qty\":2}" >/dev/null
TOTAL=$(api "$GW/v1/omnideliv/baskets/$BASKET" | sed -n 's/.*"goods_total_cents":\([0-9]*\).*/\1/p')
# 2 x Tapsilog at 170.00. Asserting the number, not just a 200: a basket that
# accepts the line and prices it wrong is the failure worth catching.
[ "$TOTAL" = "34000" ] || { echo "     expected 34000, got $TOTAL"; exit 1; }
echo "     total $TOTAL"

echo "5/6  checkout"
CHECKOUT=$(api -X POST "$GW/v1/omnideliv/orders/checkout" \
  -d "{\"basket_id\":\"$BASKET\",\"tip_cents\":0,\"delivery_lat\":14.5995,\"delivery_lng\":120.9842}") || {
    echo "     checkout failed — if this is a 503, the seeded courier's GPS fix has aged out;"
    echo "     re-run scripts/seed-omnideliv.sh. If 500, check SERVICE_TOKEN: field-ops"
    echo "     returns 401 without it and that surfaces here as a failure to dispatch."
    exit 1
  }
ORDER=$(echo "$CHECKOUT" | json order_id)
[ -n "$ORDER" ] || { echo "     checkout produced no order"; exit 1; }
echo "     order $ORDER"

echo "6/6  tracking"
STATUS=$(api "$GW/v1/omnideliv/orders/$ORDER/track" | json status)
# awaiting_courier, not placed: checkout marks the order offered once field-ops
# accepts it. Seeing `placed` here means the offer never landed.
[ "$STATUS" = "awaiting_courier" ] || { echo "     expected awaiting_courier, got '$STATUS'"; exit 1; }
echo "     status $STATUS"

echo
echo "PASS — an order was placed with no LLM in the path."

# ── Optional: the Kafka round trip ──────────────────────────────────────────
#
# Everything above is synchronous HTTP. The event pipeline — field-ops
# publishing a claim, omnideliv consuming it and advancing the order — is the
# largest untested surface in this build, and it needs a courier token to
# exercise. Skipped by default so the required part of this script stays
# runnable in CI without one.
if [ -n "${COURIER_TOKEN:-}" ]; then
  echo
  echo "7/7  courier claim → order advances (the Kafka round trip)"

  ASSIGNMENT=$(curl -sf -H "Authorization: Bearer $COURIER_TOKEN" \
    "$FIELD_OPS/v1/field-ops/assignments/offer" -X POST -H "Content-Type: application/json" \
    -d "{\"product\":\"omnideliv\",\"external_ref\":\"$ORDER\",\"lat\":14.5995,\"lng\":120.9842}" \
    | sed -n 's/.*"assignment_ids":\["\([^"]*\)".*/\1/p')

  [ -n "$ASSIGNMENT" ] || { echo "     no assignment offered"; exit 1; }

  curl -sf -X POST -H "Authorization: Bearer $COURIER_TOKEN" \
    "$FIELD_OPS/v1/field-ops/assignments/$ASSIGNMENT/claim" >/dev/null

  # Poll rather than sleep once: the consumer commits after handling, so the
  # visible delay is broker latency plus one poll interval, not a fixed number
  # anyone can guess correctly.
  for _ in $(seq 1 20); do
    S=$(api "$GW/v1/omnideliv/orders/$ORDER/track" | json status)
    [ "$S" = "collecting" ] && { echo "     order advanced to collecting"; exit 0; }
    sleep 1
  done

  echo "     order never left '$S' — the milestone was published but not consumed."
  echo "     Check: is omnideliv subscribed (log line 'courier milestone consumer stopped'),"
  echo "     and are both services pointed at the same KAFKA__BROKERS?"
  exit 1
fi
