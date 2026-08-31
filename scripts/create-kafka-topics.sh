#!/usr/bin/env bash
# Creates all required Kafka topics for LogisticOS on the VPS.
# Run once per environment: sh scripts/create-kafka-topics.sh
# Requires KAFKA_CONTAINER env var or will auto-detect.

set -e

KAFKA_CONTAINER="${KAFKA_CONTAINER:-$(docker ps --format '{{.Names}}' | grep -i kafka | head -1)}"
# The advertised internal listener, not localhost:9092. Both are advertised
# (PLAINTEXT://localhost:9092,PLAINTEXT_INTERNAL://kafka:29092), but the
# localhost one hangs from inside the container on the VPS deployment —
# every create silently blocks until the script is killed. Overridable.
BOOTSTRAP="${KAFKA_BOOTSTRAP:-kafka:29092}"

if [ -z "$KAFKA_CONTAINER" ]; then
  echo "ERROR: No Kafka container found. Set KAFKA_CONTAINER=<name> and retry."
  exit 1
fi

echo "Using Kafka container: $KAFKA_CONTAINER"

# Auto-detect the kafka-topics binary path (differs between image flavors)
KAFKA_BIN=""
for candidate in \
    "/usr/bin/kafka-topics" \
    "/opt/kafka/bin/kafka-topics.sh" \
    "/opt/bitnami/kafka/bin/kafka-topics.sh" \
    "/usr/local/bin/kafka-topics" \
    "$(docker exec "$KAFKA_CONTAINER" find /opt /usr -name 'kafka-topics*' -type f 2>/dev/null | head -1)"; do
  if docker exec "$KAFKA_CONTAINER" test -x "$candidate" 2>/dev/null; then
    KAFKA_BIN="$candidate"
    break
  fi
done

if [ -z "$KAFKA_BIN" ]; then
  echo "ERROR: Could not find kafka-topics binary in container."
  echo "Run: docker exec $KAFKA_CONTAINER find / -name 'kafka-topics*' 2>/dev/null"
  exit 1
fi

echo "Kafka binary: $KAFKA_BIN"

create_topic() {
  local TOPIC="$1"
  local PARTITIONS="${2:-3}"
  local REPLICATION="${3:-1}"
  docker exec "$KAFKA_CONTAINER" "$KAFKA_BIN" \
    --bootstrap-server "$BOOTSTRAP" \
    --create \
    --if-not-exists \
    --topic "$TOPIC" \
    --partitions "$PARTITIONS" \
    --replication-factor "$REPLICATION" \
    2>&1 | grep -v "^$" || true
}

echo ""
echo "=== Creating LogisticOS Kafka Topics ==="
echo ""

# Identity
create_topic "logisticos.identity.tenant.created"
create_topic "logisticos.identity.user.created"
create_topic "logisticos.identity.user.invited"

# Order / Shipment
create_topic "logisticos.order.shipment.created"
create_topic "logisticos.order.shipment.confirmed"
create_topic "logisticos.order.shipment.cancelled"
create_topic "logisticos.order.awb.issued"

# Dispatch — CRITICAL for driver task assignment
create_topic "logisticos.task.assigned"
create_topic "logisticos.dispatch.route.created"
create_topic "logisticos.dispatch.driver.assigned"
create_topic "logisticos.dispatch.route.optimized"

# Driver / Field
create_topic "logisticos.driver.pickup.completed"
create_topic "logisticos.driver.delivery.attempted"
create_topic "logisticos.driver.delivery.completed"
create_topic "logisticos.driver.delivery.failed"
create_topic "logisticos.driver.location.updated"

# Hub
create_topic "logisticos.hub.piece.scanned"
create_topic "logisticos.hub.piece.weight_discrepancy"
create_topic "logisticos.hub.pallet.sealed"

# Fleet
create_topic "logisticos.fleet.container.departed"
create_topic "logisticos.fleet.container.arrived"

# Payments / Billing
create_topic "logisticos.payments.invoice.generated"
create_topic "logisticos.payments.invoice.finalized"
create_topic "logisticos.payments.payment.received"
create_topic "logisticos.payments.cod.collected"
create_topic "logisticos.payments.cod.remittance_ready"
create_topic "logisticos.payments.invoice.weight_adjustment"

# Engagement / CDP
create_topic "logisticos.engagement.notification.queued"
create_topic "logisticos.marketing.campaign.triggered"
create_topic "logisticos.cdp.segment.updated"

# POD / Tracking
create_topic "logisticos.pod.captured"
create_topic "logisticos.tracking.receipt.email.requested"

# Carrier
create_topic "logisticos.carrier.onboarded"
create_topic "logisticos.carrier.status_changed"
create_topic "logisticos.carrier.allocated"

# Compliance (internal)
create_topic "compliance"

# `driver` is where compliance looks for `driver.registered` to open a profile
# for a new field worker. It sat here as a consumer with NO producer anywhere in
# the platform from the day the compliance service shipped, so no field worker
# ever had a profile created for them and the review queue was permanently
# empty. field-ops now publishes it on courier registration.
#
# Note the topic guard in check-kafka-topics.sh does not cover this one: it
# scans for `topics::CONSTANT` inside `subscribe(&[...])`, and both compliance
# and field-ops subscribe using their own crate-local constants. Adding it here
# is manual until that guard reads local constants too.
create_topic "driver"

# Field-ops (platform tier) — courier milestones, consumed by every product that
# dispatches through it. Listed here rather than left to auto-create: the topic
# is only created by the first PUBLISH, so a consumer that starts first logs
# UnknownTopicOrPartition and does not recover on its own. On 2026-08-07 that
# left omnideliv silently deaf until it was restarted after the first claim.
#
# One partition on purpose. Ordering per job is what matters (assigned ->
# collected -> delivered for a given external_ref), and these are keyed on
# external_ref, so a single partition gives total order for free at this volume.
create_topic "fieldops.courier" 1

# OmniDeliv order bookends, consumed by engagement for the customer's
# confirmation and delivery notice. One partition: keyed on the order, so a
# "delivered" push can never overtake its own "placed".
create_topic "omnideliv.order.placed" 1
create_topic "omnideliv.order.delivered" 1

# OmniDeliv vendor legs (ADR-0017) — one message per vendor per order, keyed on
# vendor_id so a stall's own work arrives in order. Created here for the same
# reason as fieldops.courier above: the vendor console and the notification
# consumers subscribe at startup, and a topic that does not exist yet leaves
# them logging UnknownTopicOrPartition with no recovery short of a restart.
#
# One partition. Ordering per vendor is what matters, these are keyed on
# vendor_id, and a single partition gives total order for free at this volume —
# a stall must never see "accepted" before the "received" it answers.
create_topic "omnideliv.vendor.leg.received" 1
create_topic "omnideliv.vendor.leg.accepted" 1
create_topic "omnideliv.vendor.leg.rejected" 1

# ── Topics that had consumers but no topic ───────────────────────────────────
#
# A topic is created by its first PUBLISH. A consumer that subscribes before
# that logs `UnknownTopicOrPartition` and **does not recover on its own** — it
# stays subscribed to nothing until the process restarts after the topic exists.
#
# Every one of these had a live consumer subscribed to a topic the broker had
# never heard of, so the feature behind it was silently inert:
#
#   dispatch.assignment.rejected            a driver declining an assignment
#   dispatch.offer.created / offer.closed   the gig broadcast-wave offers
#   hub.container.customs_cleared           customs clearance
#   hub.shipment.dispatch_requested         hub handing a shipment to dispatch
#   hub.shipment.carrier_booking_requested  hub booking an outbound carrier
#   marketing.campaign.completed            campaign completion
#   engagement.campaign.opened / clicked    CRM auto-enrolment on engagement
#
# `driver.available` is here too. It already existed on the broker — auto-created
# by a publish that happened to land first — which is exactly the coin-flip this
# file exists to remove.
create_topic "logisticos.dispatch.assignment.rejected"
create_topic "logisticos.dispatch.offer.created"
create_topic "logisticos.dispatch.offer.closed"
create_topic "logisticos.driver.available"
create_topic "logisticos.hub.container.customs_cleared"
create_topic "logisticos.hub.shipment.dispatch_requested"
create_topic "logisticos.hub.shipment.carrier_booking_requested"
create_topic "logisticos.marketing.campaign.completed"
create_topic "logisticos.engagement.campaign.opened"
create_topic "logisticos.engagement.campaign.clicked"

# Payment intents. Published by services/payments; subscribed by order-intake
# (shipping fees), omnideliv (orders) and carrier (marketplace bookings) via
# KafkaConsumer::new rather than a bare subscribe(), which is why these were
# absent here while check-kafka-topics.sh reported a clean bill of health.
#
# Nothing publishes them until someone actually pays, so on a fresh broker all
# three consumers subscribe to topics that do not exist yet, log
# UnknownTopicOrPartition once, and stay subscribed to nothing until they are
# restarted -- which is to say every online payment made before the first
# restart is taken and never acted on.
create_topic "logisticos.payments.intent.authorized"
create_topic "logisticos.payments.intent.captured"
create_topic "logisticos.payments.intent.failed"

# SaaS subscription lifecycle. Consumed by engagement for renewal and
# dunning notices -- a renewal nobody is told about is a lapse.
create_topic "logisticos.payments.subscription.activated"
create_topic "logisticos.payments.subscription.renewal_due"
create_topic "logisticos.payments.subscription.past_due"
create_topic "logisticos.payments.subscription.lapsed"

# Marketplace (spot truck) bookings.
create_topic "logisticos.carrier.marketplace.booking.requested"
create_topic "logisticos.carrier.marketplace.booking.accepted"
create_topic "logisticos.carrier.marketplace.booking.rejected"
create_topic "logisticos.carrier.marketplace.pickup.recorded"

# ---------------------------------------------------------------------------
# Found 2026-08-31, all by the same defect in check-kafka-topics.sh.
#
# That guard matched `subscribe(&[...])` WITHOUT `-z`, so it only ever saw a
# single-line call -- and every real subscribe in this repo spans lines. It also
# required a literal `topics::` prefix, so a bare imported constant was
# invisible. Between them it was reading almost nothing while reporting a clean
# bill of health. Both holes are closed; these are the topics that were hiding
# behind them, twelve of which did not exist on the live broker either.
#
# Each line below is a feature that was inert with no symptom past one startup
# line, exactly as in the 2026-08-09 sweep.

# Proof of Pickup. Published by pod; subscribed by payments (Track A driver
# ledger debit -- money), order-intake and delivery-experience (the canonical
# chain-of-custody transition to picked_up).
create_topic "logisticos.pod.pickup.captured"

# Money out. COD remittance and wallet withdrawal settlement.
create_topic "logisticos.payments.cod.remitted"
create_topic "logisticos.payments.wallet.withdrawal_disbursed"
create_topic "logisticos.payments.wallet.withdrawal_rejected"

# Hub: consolidation, inbound piece scans, and the cross-border customs legs.
create_topic "logisticos.hub.consolidation.plan_loaded"
create_topic "logisticos.hub.piece.scanned_inbound"
create_topic "logisticos.hub.container.arrived_at_port"
create_topic "logisticos.hub.container.customs_hold"
create_topic "logisticos.hub.container.deconsolidated"

# Support tickets and the AI escalation loop that closes them.
create_topic "logisticos.support.ticket.opened"
create_topic "logisticos.support.ticket.closed"
create_topic "logisticos.ai.escalation.resolved"

# Identity. Both of these DID exist on the broker -- auto-created because a
# publish happened to land before the consumer subscribed. That is the coin
# flip this script exists to remove, not a reprieve.
create_topic "logisticos.identity.otp.requested"
create_topic "logisticos.identity.tenant.finalized"

echo ""
echo "=== Done. Verify with: ==="
echo "docker exec $KAFKA_CONTAINER $KAFKA_BIN --bootstrap-server localhost:9092 --list"
echo ""
