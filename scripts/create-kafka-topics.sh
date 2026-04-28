#!/usr/bin/env bash
# Creates all required Kafka topics for LogisticOS on the VPS.
# Run once per environment: sh scripts/create-kafka-topics.sh
# Requires KAFKA_CONTAINER env var or will auto-detect.

set -e

KAFKA_CONTAINER="${KAFKA_CONTAINER:-$(docker ps --format '{{.Names}}' | grep -i kafka | head -1)}"
BOOTSTRAP="localhost:9092"

if [ -z "$KAFKA_CONTAINER" ]; then
  echo "ERROR: No Kafka container found. Set KAFKA_CONTAINER=<name> and retry."
  exit 1
fi

echo "Using Kafka container: $KAFKA_CONTAINER"

create_topic() {
  local TOPIC="$1"
  local PARTITIONS="${2:-3}"
  local REPLICATION="${3:-1}"
  docker exec "$KAFKA_CONTAINER" kafka-topics.sh \
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

echo ""
echo "=== Done. Verify with: ==="
echo "docker exec $KAFKA_CONTAINER kafka-topics.sh --bootstrap-server localhost:9092 --list"
echo ""
