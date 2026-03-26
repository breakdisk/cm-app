# Kafka Operations Runbook

**Owner:** Staff Platform Engineer / SRE Lead
**Last Reviewed:** 2026-03-17
**Applies To:** AWS MSK cluster `logisticos-production` (ap-southeast-1)
**Related Runbooks:** [incident-response.md](incident-response.md), [deployment.md](deployment.md)

---

## Table of Contents

1. [Topic Inventory and Partition Scheme](#1-topic-inventory-and-partition-scheme)
2. [Consumer Group Registry](#2-consumer-group-registry)
3. [Common Operations](#3-common-operations)
4. [Under-Replicated Partitions](#4-under-replicated-partitions)
5. [MSK Broker Restart Procedure](#5-msk-broker-restart-procedure)
6. [Adding Partitions to a Topic](#6-adding-partitions-to-a-topic)
7. [Dead Letter Queue Pattern](#7-dead-letter-queue-pattern)
8. [Monitoring and Alerting](#8-monitoring-and-alerting)

---

## Environment Setup

All Kafka operations are performed from inside the `kafka-client-pod` running in
the `logisticos-core` namespace. This pod has the Kafka CLI tools and the
authenticated client configuration pre-loaded.

```bash
# Access the Kafka client pod
kubectl exec -it -n logisticos-core kafka-client-pod -- bash

# Inside the pod, use these aliases (already set in the pod's .bashrc):
# kc = kafka-consumer-groups.sh --bootstrap-server ${KAFKA_BOOTSTRAP} --command-config /etc/kafka/client.properties
# kt = kafka-topics.sh          --bootstrap-server ${KAFKA_BOOTSTRAP} --command-config /etc/kafka/client.properties
# kp = kafka-console-producer.sh --bootstrap-server ${KAFKA_BOOTSTRAP} --producer.config /etc/kafka/client.properties
# kcon = kafka-console-consumer.sh --bootstrap-server ${KAFKA_BOOTSTRAP} --consumer.config /etc/kafka/client.properties

# Or prefix manually:
KAFKA_CMD="--bootstrap-server ${KAFKA_BOOTSTRAP} --command-config /etc/kafka/client.properties"
```

---

## 1. Topic Inventory and Partition Scheme

### Naming Convention

```
logisticos.<domain>.<entity>.<event>
```

All topics use:
- **Replication factor:** 3 (MSK default; ensures fault tolerance across 3 AZs)
- **Min in-sync replicas (min.ISR):** 2 (producer `acks=all` requires 2/3 replicas before ack)
- **Retention:** 7 days (default); extended to 30 days for financial topics
- **Compression:** `lz4` (best throughput/latency balance for JSON payloads)
- **Cleanup policy:** `delete` (not `compact`) except for profile topics noted below

---

### `logisticos.shipment.events`

**Domain:** Logistics Operations
**Producer:** order-intake service
**Purpose:** Every state change in a shipment's lifecycle. This is the highest-volume
topic in the platform. Downstream services consume it to maintain derived state.

| Parameter | Value |
|-----------|-------|
| Partitions | 24 |
| Replication Factor | 3 |
| Retention | 7 days |
| Partition Key | `shipment_id` (consistent routing for ordered processing per shipment) |
| Message Format | JSON (Avro migration planned for Q3 2026) |

**Event types:** `shipment.created`, `shipment.confirmed`, `shipment.pickup_assigned`,
`shipment.picked_up`, `shipment.at_hub`, `shipment.out_for_delivery`, `shipment.delivered`,
`shipment.delivery_failed`, `shipment.rescheduled`, `shipment.cancelled`

**Consumers:** analytics, engagement, cdp, delivery-experience, payments, driver-ops

---

### `logisticos.delivery.updates`

**Domain:** Driver Operations
**Producer:** driver-ops service
**Purpose:** Real-time delivery status updates and ETA recalculations streamed from the Driver App.
High-frequency; expected throughput 5,000–50,000 messages/minute during peak hours.

| Parameter | Value |
|-----------|-------|
| Partitions | 48 |
| Replication Factor | 3 |
| Retention | 24 hours (short — raw GPS data not stored long-term here) |
| Partition Key | `driver_id` (ordered updates per driver) |
| Message Format | JSON |

**Event types:** `stop.arrived`, `stop.completed`, `stop.failed`, `eta.updated`

**Consumers:** delivery-experience (live tracking), engagement (proactive ETA notifications)

---

### `logisticos.cod.collected`

**Domain:** Payments
**Producer:** driver-ops service (on POD submission with COD flag)
**Purpose:** COD collection events triggering reconciliation workflows in the Payments service.
Financial topic — extended retention to 30 days for audit compliance.

| Parameter | Value |
|-----------|-------|
| Partitions | 12 |
| Replication Factor | 3 |
| Retention | **30 days** |
| Partition Key | `driver_id` |
| Message Format | JSON |

**Event types:** `cod.collected`, `cod.batch.submitted`, `cod.batch.verified`, `cod.reconciled`

**Consumers:** payments (COD reconciliation), analytics (COD revenue tracking)

---

### `logisticos.notification.outbound`

**Domain:** Engagement
**Producer:** engagement service, marketing service
**Purpose:** Outbound notification dispatch queue. Messages are consumed by
channel-specific sender workers (WhatsApp, SMS, email, push).

| Parameter | Value |
|-----------|-------|
| Partitions | 24 |
| Replication Factor | 3 |
| Retention | 7 days |
| Partition Key | `customer_id` (ordered notifications per customer) |
| Message Format | JSON |

**Event types:** `notification.queued` (one per notification to send)

**Consumers:** engagement (channel sender workers — one consumer group per channel)

---

### `logisticos.driver.locations`

**Domain:** Fleet / Driver Operations
**Producer:** driver-ops service (GPS ingestion from Driver App, 15s intervals)
**Purpose:** Raw GPS location stream for live tracking and route deviation detection.
Very high frequency. Consumed by the delivery-experience service for live map updates.

| Parameter | Value |
|-----------|-------|
| Partitions | 48 |
| Replication Factor | 3 |
| Retention | **4 hours** (raw GPS not retained; TimescaleDB stores 90-day history) |
| Partition Key | `driver_id` |
| Message Format | JSON (compact: `{driver_id, lat, lng, speed, heading, ts}`) |

**Event types:** `driver.location.updated`

**Consumers:** delivery-experience (live map), fleet (telemetry), dispatch (ETA recalculation)

---

### `logisticos.agent.triggers`

**Domain:** AI Intelligence Layer
**Producer:** business-logic service, engagement service, order-intake service
**Purpose:** Triggers for AI agent activation. When a business rule fires an agent action
(e.g., "dispatch agent: re-assign timed-out driver", "support agent: customer asked for ETA"),
the event is published here. The ai-layer service consumes and invokes the appropriate agent.

| Parameter | Value |
|-----------|-------|
| Partitions | 12 |
| Replication Factor | 3 |
| Retention | 7 days |
| Partition Key | `agent_type` (ordered per agent class) |
| Message Format | JSON |

**Event types:** `agent.dispatch.trigger`, `agent.support.trigger`, `agent.marketing.trigger`,
`agent.fraud.trigger`, `agent.logistics_planner.trigger`

**Consumers:** ai-layer (agent orchestration)

---

### `logisticos.cdp.events`

**Domain:** Customer Data Platform
**Producer:** cdp service (after persisting behavioral events)
**Purpose:** CDP behavioral event stream consumed by the ML scoring pipeline and
Marketing Automation Engine for real-time audience updates.

| Parameter | Value |
|-----------|-------|
| Partitions | 24 |
| Replication Factor | 3 |
| Retention | 30 days |
| Partition Key | `customer_profile_id` |
| Cleanup Policy | `delete` |
| Message Format | JSON |

**Event types:** All `EventType` values from `cdp.proto` (`cdp.event.recorded` wrapper)

**Consumers:** marketing (audience re-segmentation), ai-layer (churn scoring triggers)

---

### Dead Letter Queue Topics

Each primary consumer group has a corresponding DLQ topic:

| Primary Topic | DLQ Topic |
|--------------|-----------|
| logisticos.shipment.events | logisticos.shipment.events.dlq |
| logisticos.delivery.updates | logisticos.delivery.updates.dlq |
| logisticos.cod.collected | logisticos.cod.collected.dlq |
| logisticos.notification.outbound | logisticos.notification.outbound.dlq |
| logisticos.agent.triggers | logisticos.agent.triggers.dlq |

DLQ topics use 4 partitions, 30-day retention. See [Section 7](#7-dead-letter-queue-pattern).

---

## 2. Consumer Group Registry

| Consumer Group | Service Owner | Primary Topic(s) | Namespace |
|----------------|-------------|-----------------|-----------|
| `logisticos-analytics-consumer` | analytics | shipment.events | logisticos-analytics |
| `logisticos-engagement-consumer` | engagement | shipment.events, cdp.events | logisticos-engagement |
| `logisticos-cdp-consumer` | cdp | shipment.events | logisticos-engagement |
| `logisticos-delivery-exp-consumer` | delivery-experience | delivery.updates, driver.locations | logisticos-logistics |
| `logisticos-payments-consumer` | payments | cod.collected, shipment.events | logisticos-payments |
| `logisticos-dispatch-consumer` | dispatch | delivery.updates | logisticos-logistics |
| `logisticos-fleet-consumer` | fleet | driver.locations | logisticos-logistics |
| `logisticos-ai-agent-consumer` | ai-layer | agent.triggers | logisticos-ai |
| `logisticos-marketing-consumer` | marketing | cdp.events | logisticos-engagement |
| `logisticos-notification-whatsapp` | engagement | notification.outbound | logisticos-engagement |
| `logisticos-notification-sms` | engagement | notification.outbound | logisticos-engagement |
| `logisticos-notification-email` | engagement | notification.outbound | logisticos-engagement |
| `logisticos-notification-push` | engagement | notification.outbound | logisticos-engagement |
| `logisticos-dlq-monitor` | SRE tooling | all DLQ topics | logisticos-core |

---

## 3. Common Operations

### List All Topics

```bash
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --list
```

### Describe a Topic

```bash
# Full topic metadata: partitions, replication factor, leader, ISR
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --topic logisticos.shipment.events
```

### Describe Consumer Group Lag

```bash
# Describe a specific consumer group
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --group logisticos-analytics-consumer

# Describe all consumer groups (sorted by lag descending)
kubectl exec -it -n logisticos-core kafka-client-pod -- bash -c '
  kafka-consumer-groups.sh \
    --bootstrap-server ${KAFKA_BOOTSTRAP} \
    --command-config /etc/kafka/client.properties \
    --describe --all-groups 2>/dev/null \
  | sort -k6 -rn \
  | head -30
'
```

### Reset Consumer Group Offset

**WARNING:** Resetting offsets can cause duplicate processing or missed messages.
Requires Engineering Manager approval before execution in production.
Always stop the consumer service before resetting, and restart it after.

```bash
# Step 1: Scale down the consumer deployment
kubectl scale deployment/<service> -n <namespace> --replicas=0
kubectl rollout status deployment/<service> -n <namespace>

# Step 2: Reset offset to the beginning of the topic (reprocess all messages)
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --group logisticos-<service>-consumer \
  --topic logisticos.<topic> \
  --reset-offsets \
  --to-earliest \
  --execute

# Step 2 (alternative): Reset to a specific datetime (ISO 8601)
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --group logisticos-<service>-consumer \
  --topic logisticos.<topic> \
  --reset-offsets \
  --to-datetime 2026-03-17T08:00:00.000 \
  --execute

# Step 2 (alternative): Reset by shifting offset back N messages
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --group logisticos-<service>-consumer \
  --topic logisticos.<topic> \
  --reset-offsets \
  --shift-by -1000 \
  --execute

# Step 3: Scale the consumer back up
kubectl scale deployment/<service> -n <namespace> --replicas=<original-count>
kubectl rollout status deployment/<service> -n <namespace>

# Step 4: Monitor lag to confirm it is draining
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --group logisticos-<service>-consumer
```

### Delete a Topic

**DANGER:** Topic deletion is irreversible and permanently destroys all messages.
Requires CISO-level approval in production. Should only be used for DLQ cleanup,
test topic cleanup, or retiring decommissioned services.

```bash
# Safety check 1: Confirm the topic is not actively consumed
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe --all-groups 2>/dev/null \
  | grep "<topic-name>"

# Safety check 2: Confirm the topic name exactly (avoid partial matches)
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --list | grep "<topic-name>"

# Delete (requires delete.topic.enable=true on the MSK cluster — enabled in LogisticOS MSK config)
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --delete \
  --topic <topic-name>
```

---

## 4. Under-Replicated Partitions

Under-replicated partitions (URP count > 0) indicate that one or more partition
replicas are lagging behind the leader or are offline. While URPs persist, message
durability guarantees are weakened — a broker failure during URP could cause data loss.

### Detect URPs

```bash
# Check URP count via CloudWatch (primary alert source)
aws cloudwatch get-metric-statistics \
  --namespace AWS/Kafka \
  --metric-name UnderReplicatedPartitions \
  --dimensions "Name=Cluster Name,Value=logisticos-production" \
  --start-time $(date -u -d '-15 minutes' +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -v-15M +%Y-%m-%dT%H:%M:%SZ) \
  --end-time $(date -u +%Y-%m-%dT%H:%M:%SZ) \
  --period 60 \
  --statistics Maximum \
  --region ap-southeast-1

# Identify which topics and partitions are under-replicated
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --under-replicated-partitions

# Check ISR (In-Sync Replicas) for a specific topic
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --topic logisticos.shipment.events \
  | grep -v "^Topic:" | awk '{print $1, $3, $7, $9}'
```

### Resolution Procedure

**Self-healing (most common):** MSK automatically reassigns and re-replicates partitions
when a broker recovers or a new broker replacement is provisioned. URPs typically
resolve within 5–30 minutes without manual intervention.

1. **Check broker health in AWS Console:**
   `AWS Console → MSK → logisticos-production → Brokers`
   Identify any broker in `DEGRADED` or `REBOOTING` state.

2. **If a broker is unresponsive > 30 minutes:** Open an MSK support case via AWS Console.
   Do not attempt to manually restart MSK brokers — see [Section 5](#5-msk-broker-restart-procedure).

3. **If URPs persist after broker recovery:** Check replication throttle settings.

```bash
# Check if replication throttle is set (a leftover throttle can delay recovery)
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-configs.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --entity-type brokers \
  --entity-default \
  --describe \
  | grep -i throttle

# Remove replication throttle if set and recovery has completed
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-configs.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --entity-type brokers \
  --entity-default \
  --alter \
  --delete-config follower.replication.throttled.rate \
  --delete-config leader.replication.throttled.rate
```

4. **If URP count is not reducing after 1 hour:** Escalate to SEV2 (financial/delivery
   topics) or SEV3 (analytics/marketing topics). Page the on-call SRE.

---

## 5. MSK Broker Restart Procedure

**AWS MSK is a fully managed service — do not restart brokers directly via EC2 or AWS CLI.**
All broker maintenance is handled by AWS. LogisticOS's responsibility is to:

1. Monitor and alert on broker health via CloudWatch.
2. Ensure producer and consumer clients retry gracefully during broker unavailability.
3. Coordinate application-level maintenance (draining consumers) for planned MSK maintenance windows.

### Planned MSK Maintenance Window Coordination

When AWS notifies of a planned MSK maintenance window:

```bash
# 1. Check when the maintenance window is scheduled (AWS Console → MSK → Maintenance)

# 2. Scale down non-critical consumers to reduce broker load during maintenance
#    (preserve financial topics consumers — payments, cod)
kubectl scale deployment/analytics -n logisticos-analytics --replicas=0
kubectl scale deployment/marketing -n logisticos-engagement --replicas=0

# 3. Notify stakeholders: post in #deployments at least 2 hours before window
#    "MSK maintenance window: {time}. Analytics and marketing consumers paused.
#     No impact on delivery, payments, or notifications."

# 4. Monitor CloudWatch during maintenance window for URP and offline partition alerts

# 5. After maintenance completes: scale consumers back up
kubectl scale deployment/analytics -n logisticos-analytics --replicas=2
kubectl scale deployment/marketing -n logisticos-engagement --replicas=2

# 6. Verify consumer groups recover and lag drains within 10 minutes
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe --all-groups 2>/dev/null | sort -k6 -rn | head -20
```

### MSK Connection String Update

If the MSK bootstrap string changes (e.g., after a cluster replacement), update it
in Vault and trigger a rolling restart of all services:

```bash
# Update the Vault secret (requires VAULT_TOKEN with write access to the path)
vault kv put secret/logisticos/kafka \
  bootstrap_servers="<new-bootstrap-string>"

# Services read KAFKA_BOOTSTRAP from Vault via the init container on pod start.
# Trigger a rolling restart of all services that use Kafka:
for ns in logisticos-core logisticos-logistics logisticos-engagement logisticos-payments logisticos-ai logisticos-analytics; do
  kubectl rollout restart deployment -n $ns -l kafka-consumer=true
done
```

---

## 6. Adding Partitions to a Topic

**Important:** Partition count can only be increased, never decreased.
Adding partitions changes the key-to-partition mapping for new messages, which can
break ordering guarantees for key-partitioned topics. Evaluate carefully before proceeding.

**Requires:** Engineering Manager + Staff Platform Engineer approval.

### Procedure

```bash
# 1. Describe the current partition count and key strategy
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --topic logisticos.shipment.events

# 2. Check current throughput to justify the partition increase
#    (Rule of thumb: 1 partition ≈ 10 MB/s write throughput max)
#    Review Grafana: Kafka dashboard → Bytes In/Out per partition

# 3. Plan the new partition count (always a multiple of 3 for even distribution across 3 AZs)

# 4. Increase partitions
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --alter \
  --topic logisticos.shipment.events \
  --partitions 36

# 5. Verify the change
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --topic logisticos.shipment.events

# 6. Scale up consumer replicas to match new partition count
#    (consumers cannot exceed partition count; set replicas = new partition count / 2 for balance)
kubectl scale deployment/analytics -n logisticos-analytics --replicas=18
```

### Partition Reassignment (Rebalance)

After adding brokers or if partition distribution is uneven:

```bash
# Generate a reassignment plan (review before executing)
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-reassign-partitions.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --topics-to-move-json-file /tmp/topics-to-move.json \
  --broker-list "1,2,3" \
  --generate

# Execute the plan (apply reassignment.json generated in the previous step)
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-reassign-partitions.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --reassignment-json-file /tmp/reassignment.json \
  --throttle 50000000 \
  --execute

# Monitor reassignment progress
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-reassign-partitions.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --reassignment-json-file /tmp/reassignment.json \
  --verify
```

---

## 7. Dead Letter Queue Pattern

### Overview

Every consumer in LogisticOS applies the following retry and DLQ pattern:

```
Primary Topic
    ↓ (consumer processes message)
    → SUCCESS: commit offset
    → ERROR (transient): exponential backoff retry (max 3 retries, cap 30s)
    → ERROR (permanent / max retries exceeded): publish to DLQ topic, commit offset
```

DLQ messages are wrapped in an envelope with the original message and error metadata:

```json
{
  "original_topic": "logisticos.shipment.events",
  "original_partition": 4,
  "original_offset": 182934,
  "original_key": "ship_abc123",
  "original_payload": { "...": "original message body" },
  "error_type": "DeserializationError",
  "error_message": "missing required field: tenant_id",
  "failed_at": "2026-03-17T10:23:45Z",
  "retry_count": 3,
  "consumer_group": "logisticos-analytics-consumer",
  "service_version": "sha-abc1234"
}
```

### Inspecting DLQ Messages

```bash
# Read DLQ messages from the beginning (view recent failures)
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-console-consumer.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --consumer.config /etc/kafka/client.properties \
  --topic logisticos.shipment.events.dlq \
  --from-beginning \
  --max-messages 50 \
  --property print.key=true \
  --property print.timestamp=true \
  | python3 -m json.tool

# Count messages in a DLQ (offset-based estimate)
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-run-class.sh kafka.tools.GetOffsetShell \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --topic logisticos.shipment.events.dlq \
  --time -1

# Describe the DLQ consumer group lag (SRE DLQ monitor group)
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --group logisticos-dlq-monitor
```

### Replaying DLQ Messages

Replaying moves DLQ messages back to the primary topic for reprocessing. Only do this
after confirming the root cause of the original failure is fixed (e.g., a bug fix deployed).

```bash
# Option A: Use the LogisticOS DLQ replay CLI tool (recommended)
# This tool filters by error_type, validates messages, and publishes to the primary topic.
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  /opt/logisticos/dlq-replay \
  --source logisticos.shipment.events.dlq \
  --target logisticos.shipment.events \
  --filter-error-type DeserializationError \
  --dry-run   # review output first, then re-run without --dry-run

# Option B: Manual replay using kafka-console-producer (for small batches only)
# 1. Save DLQ messages to a temp file
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-console-consumer.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --consumer.config /etc/kafka/client.properties \
  --topic logisticos.shipment.events.dlq \
  --from-beginning \
  --max-messages 100 \
  > /tmp/dlq-messages.json

# 2. Extract original_payload from each envelope (requires jq)
cat /tmp/dlq-messages.json | jq -c '.original_payload' > /tmp/replay-messages.json

# 3. Publish back to primary topic
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-console-producer.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --producer.config /etc/kafka/client.properties \
  --topic logisticos.shipment.events \
  < /tmp/replay-messages.json
```

### DLQ Alert Thresholds

| DLQ Topic | Alert Threshold | Severity |
|-----------|----------------|---------|
| shipment.events.dlq | > 100 messages unprocessed | SEV2 |
| cod.collected.dlq | > 10 messages unprocessed | SEV1 (financial impact) |
| notification.outbound.dlq | > 500 messages | SEV3 |
| delivery.updates.dlq | > 200 messages | SEV3 |
| agent.triggers.dlq | > 50 messages | SEV3 |

---

## 8. Monitoring and Alerting

### Primary Prometheus Metrics

These metrics are scraped from the MSK CloudWatch exporter (`prometheus-msk-exporter`
deployed in the `logisticos-core` namespace).

#### Consumer Group Lag

```promql
# Total lag across all partitions for a consumer group
sum(kafka_consumer_group_lag{group="logisticos-analytics-consumer"}) by (group)

# Lag per partition (useful for diagnosing hot partitions)
kafka_consumer_group_lag{group="logisticos-analytics-consumer"}

# Alert: any consumer group lag > 10,000 for > 5 minutes
alert: KafkaConsumerLagHigh
expr: sum(kafka_consumer_group_lag) by (group) > 10000
for: 5m
labels:
  severity: warning
annotations:
  summary: "Consumer group {{ $labels.group }} lag is {{ $value }} messages"
```

#### Broker Network Processor Utilisation

```promql
# Average idle percentage of the broker network processor threads.
# Healthy: > 30% idle. Below 20% idle = broker is saturated.
kafka_broker_network_processor_avg_idle_percent{broker="1"}

# Alert: broker network processor avg idle < 20% for > 3 minutes
alert: KafkaBrokerNetworkSaturated
expr: kafka_broker_network_processor_avg_idle_percent < 0.20
for: 3m
labels:
  severity: warning
```

#### Under-Replicated Partitions

```promql
# Alert: any under-replicated partitions for > 10 minutes (after broker recovery time)
alert: KafkaUnderReplicatedPartitions
expr: kafka_cluster_partition_underminnisrsec > 0
for: 10m
labels:
  severity: critical
```

#### Bytes In / Out

```promql
# Bytes written to a topic (ingest rate)
rate(kafka_topic_bytes_in_total{topic="logisticos.shipment.events"}[1m])

# Bytes read from a topic (consumption rate)
rate(kafka_topic_bytes_out_total{topic="logisticos.shipment.events"}[1m])
```

#### Offline Partitions

```promql
# Any offline partition is a critical alert — data may be unavailable
alert: KafkaOfflinePartitions
expr: kafka_cluster_partition_offline > 0
for: 1m
labels:
  severity: critical
annotations:
  summary: "{{ $value }} Kafka partition(s) are offline"
```

### Grafana Dashboard

All Kafka metrics are visualised in the **Kafka / MSK** dashboard:
`https://grafana.logisticos.internal/d/logisticos-kafka`

Key panels to review during incidents:
- **Consumer Group Lag** — per group, trending over time
- **Under-Replicated Partitions** — should be 0 always
- **Broker Network Processor Idle %** — should be > 30%
- **Topic Throughput** — bytes in/out per topic
- **DLQ Message Count** — rising DLQ count indicates a persistent consumer failure

### AWS CloudWatch Alarms

The following CloudWatch alarms are configured on the MSK cluster and route to PagerDuty
via the `logisticos-msk-alerts` SNS topic:

| Alarm | Threshold | Severity |
|-------|----------|---------|
| `logisticos-msk-urp` | UnderReplicatedPartitions > 0 for 10m | P2 |
| `logisticos-msk-offline-partitions` | OfflinePartitionsCount > 0 for 1m | P1 |
| `logisticos-msk-disk-usage` | KafkaDataLogsDiskUsed > 80% | P2 |
| `logisticos-msk-cpu` | CpuUser > 70% for 10m | P3 |
| `logisticos-msk-connection-count` | ConnectionCount > 5000 | P3 |

CloudWatch Alarms console:
`https://ap-southeast-1.console.aws.amazon.com/cloudwatch/home?region=ap-southeast-1#alarmsV2:?search=logisticos-msk`
