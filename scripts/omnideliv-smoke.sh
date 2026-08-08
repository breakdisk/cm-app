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

  # Claim the assignment CHECKOUT already created, found via the courier's own
  # offer list. Do NOT post a second /offer here: that is how this step used to
  # get an id, and it silently created a duplicate assignment declaring
  # trip_cents=0. The courier then claimed and delivered the unpaid duplicate,
  # so the courier-credit path could never fire and the step still reported PASS.
  ASSIGNMENT=$(curl -sf -H "Authorization: Bearer $COURIER_TOKEN" \
    "$FIELD_OPS/v1/field-ops/assignments/mine" \
    | grep -o '"assignment_id":"[^"]*"' | head -1 | cut -d'"' -f4)
  # grep -o rather than sed: a leading `.*` is greedy, so a sed capture takes the
  # LAST match on the line, not the first. With several offers outstanding that
  # silently claims the oldest instead of this order's, and the failure then
  # looks exactly like "the milestone was published but never consumed".

  [ -n "$ASSIGNMENT" ] || {
    echo "     the courier has no offers — checkout's offer never reached them."
    echo "     Re-run scripts/seed-omnideliv.sh: find_available_near only counts"
    echo "     GPS fixes from the last 10 minutes."
    exit 1
  }

  curl -sf -X POST -H "Authorization: Bearer $COURIER_TOKEN" \
    "$FIELD_OPS/v1/field-ops/assignments/$ASSIGNMENT/claim" >/dev/null

  # Poll rather than sleep once: the consumer commits after handling, so the
  # visible delay is broker latency plus one poll interval, not a fixed number
  # anyone can guess correctly.
  for _ in $(seq 1 20); do
    S=$(api "$GW/v1/omnideliv/orders/$ORDER/track" | json status)
    [ "$S" = "collecting" ] && { echo "     order advanced to collecting"; break; }
    sleep 1
  done

  if [ "$S" != "collecting" ]; then
    echo "     order never left '$S' — the milestone was published but not consumed."
    echo "     Check: is omnideliv subscribed (log line 'courier milestone consumer stopped'),"
    echo "     are both services on the same KAFKA__BROKERS, and does"
    echo "     'kafka-consumer-groups --list' return anything? An empty list means"
    echo "     the group coordinator is down — see the runbook in docs/runbooks/."
    exit 1
  fi

  # ── The money legs ────────────────────────────────────────────────────────
  #
  # Asserting the amounts, not just the transitions. A settlement that moves the
  # order along while crediting nobody is the failure worth catching, and it is
  # invisible from the order status alone.
  echo
  echo "8/8  collection and delivery credit both ledgers"

  VENDOR=$(api "$GW/v1/omnideliv/orders/$ORDER/track" >/dev/null 2>&1; echo "$KUYAS")

  curl -sf -X POST -H "Authorization: Bearer $COURIER_TOKEN" -H "Content-Type: application/json"     "$FIELD_OPS/v1/field-ops/assignments/$ASSIGNMENT/collected"     -d "{\"vendor_id\":\"$VENDOR\"}" >/dev/null || {
      echo "     collection call failed"; exit 1; }

  curl -sf -X POST -H "Authorization: Bearer $COURIER_TOKEN" -H "Content-Type: application/json"     "$FIELD_OPS/v1/field-ops/assignments/$ASSIGNMENT/delivered" -d "{}" >/dev/null || {
      echo "     delivery call failed"; exit 1; }

  for _ in $(seq 1 20); do
    S=$(api "$GW/v1/omnideliv/orders/$ORDER/track" | json status)
    [ "$S" = "delivered" ] && break
    sleep 1
  done
  [ "$S" = "delivered" ] || { echo "     order stuck at '$S', expected delivered"; exit 1; }
  echo "     order delivered"

  # Read the money back over HTTP, not psql. A ledger nobody can query is a
  # ledger nobody can verify — and this used to end with instructions to go and
  # look manually, which meant nobody did.
  COURIER_BAL=$(curl -sf -H "Authorization: Bearer $COURIER_TOKEN"     "$FIELD_OPS/v1/field-ops/couriers/me/earnings"     | grep -o '"balance_cents":[0-9-]*' | head -1 | cut -d: -f2)


  # The vendor's own view. Needs a portal login linked to the store via
  # omnideliv.vendors.user_id, so it is checked only when one is supplied.
  if [ -n "${VENDOR_TOKEN:-}" ]; then
    VENDOR_BAL=$(curl -sf -H "Authorization: Bearer $VENDOR_TOKEN"       "$GW/v1/omnideliv/vendors/me/earnings"       | grep -o '"balance_cents":[0-9-]*' | head -1 | cut -d: -f2)
    [ "$VENDOR_BAL" = "28900" ] || {
      echo "     vendor earned '$VENDOR_BAL', expected 28900 (34000 less 15%)"; exit 1; }
    echo "     vendor earned $VENDOR_BAL"
  else
    echo "     vendor payout not checked (set VENDOR_TOKEN to a store's portal login)"
  fi

  # ── COD: the courier is holding the customer's cash ───────────────────────
  #
  # 3500 earned minus 38900 collected = -35400. A positive balance here would
  # mean we were about to pay a courier money they are already holding.
  [ "$COURIER_BAL" = "-35400" ] || {
    echo "     courier balance '$COURIER_BAL', expected -35400 (3500 earned less 38900 cash held)."
    echo "     A positive balance means the COD debit never landed."
    exit 1
  }
  echo "     courier holds the customer's cash: balance $COURIER_BAL"

  curl -sf -X POST -H "Authorization: Bearer $COURIER_TOKEN" -H "Content-Type: application/json"     "$FIELD_OPS/v1/field-ops/couriers/me/remit"     -d '{"amount_cents":38900,"reference":"smoke-test"}' >/dev/null || {
      echo "     remittance failed"; exit 1; }

  AFTER=$(curl -sf -H "Authorization: Bearer $COURIER_TOKEN"     "$FIELD_OPS/v1/field-ops/couriers/me/earnings"     | grep -o '"balance_cents":[0-9-]*' | head -1 | cut -d: -f2)
  [ "$AFTER" = "3500" ] || {
    echo "     after remitting, balance is '$AFTER', expected 3500"; exit 1; }
  echo "     after remitting, the platform owes the courier $AFTER"

  echo
  echo "PASS — full lifecycle: placed, collected, delivered, cash remitted, everyone square."
fi
