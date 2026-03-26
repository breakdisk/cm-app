# ADR-0006: Kafka Event Streaming Topology

**Status:** Accepted
**Date:** 2026-03-17
**Deciders:** Principal Architect, Staff Engineer — Rust Platform, Data Engineer, Engineering Manager — Platform Core

---

## Context

LogisticOS has 17 microservices that both emit and consume events. As of ADR-0002, Kafka is the chosen event streaming backbone. However, ADR-0002 established the *principle* of event-driven communication without specifying operational conventions. As more services come online, the absence of consistent naming, partitioning, and consumer group discipline is producing real problems:

1. **Topic naming collisions** — two teams independently created `shipment-events` and `shipments.created` for overlapping purposes. Consumers subscribed to the wrong topic for three days before detection in staging.

2. **Ordering violations** — the engagement service processes `shipment.created` and `driver.assigned` events out of order for the same shipment because they landed on different partitions. WhatsApp notifications fire before the driver is confirmed.

3. **Consumer group contention** — two independent consumers within the engagement service share a consumer group, causing half the events to be processed by the wrong consumer instance.

4. **No dead-letter strategy** — a malformed JSON payload from an early-stage service caused the notification consumer to infinite-retry, creating alert fatigue and eventually blocking the partition.

5. **Retention inconsistency** — operational topics are being created with default 1-day retention, causing replay failures during incident postmortems where events older than 24 hours are needed.

This ADR establishes the complete Kafka topology standard for LogisticOS.

---

## Decision

### 1. Topic Naming Convention

All topics follow this scheme:

```
logisticos.<domain>.<entity>.<event>
```

| Segment | Rules | Examples |
|---------|-------|---------|
| `logisticos` | Literal prefix; distinguishes our topics from shared infra topics | — |
| `domain` | Lowercase service domain name | `order`, `dispatch`, `driver`, `engagement`, `payments`, `hub`, `carrier`, `pod`, `identity`, `analytics` |
| `entity` | The primary entity the event concerns | `shipment`, `route`, `driver`, `notification`, `invoice`, `parcel` |
| `event` | Past-tense verb describing what happened | `created`, `updated`, `cancelled`, `assigned`, `completed`, `failed`, `captured` |

**DLQ topics** append `.dlq`:
```
logisticos.<domain>.<entity>.<event>.dlq
```

**Examples:**
```
logisticos.order.shipment.created
logisticos.order.shipment.cancelled
logisticos.dispatch.route.created
logisticos.dispatch.driver.assigned
logisticos.driver.delivery.completed
logisticos.driver.location.updated
logisticos.pod.capture.completed
logisticos.payments.cod.collected
logisticos.notification.outbound
logisticos.notification.outbound.dlq
```

Names are validated in CI via `scripts/kafka/validate-topics.sh` which asserts the four-segment pattern before any topic creation PR is merged.

---

### 2. Partition Key Strategy

The partition key **must always be the primary entity ID** of the entity the event concerns. This guarantees that all events for a given entity land on the same partition, preserving order per entity.

| Topic | Partition Key |
|-------|--------------|
| `logisticos.order.shipment.*` | `shipment_id` (UUID as string) |
| `logisticos.dispatch.driver.*` | `driver_id` |
| `logisticos.dispatch.route.*` | `route_id` |
| `logisticos.driver.location.updated` | `driver_id` |
| `logisticos.driver.delivery.*` | `shipment_id` |
| `logisticos.pod.capture.*` | `shipment_id` |
| `logisticos.payments.cod.*` | `shipment_id` |
| `logisticos.notification.outbound` | `recipient_id` (customer or driver ID) |
| `logisticos.hub.parcel.*` | `parcel_id` |
| `logisticos.carrier.shipment.*` | `shipment_id` |
| `logisticos.identity.tenant.*` | `tenant_id` |
| `logisticos.analytics.events` | `tenant_id` (for even distribution by tenant) |

**There are no ordering guarantees across partitions.** If a consumer needs cross-entity ordering (e.g., "process route created before driver assigned for the same shipment"), that service must implement local sequencing or idempotent processing.

---

### 3. Partition Count

| Category | Partitions | Topics |
|----------|-----------|--------|
| Standard operational | 3 | All `identity.*`, `order.*`, `dispatch.*`, `driver.pickup.*`, `driver.delivery.*`, `pod.*`, `hub.*`, `carrier.*`, `payments.*` |
| High-volume streams | 12 | `logisticos.driver.location.updated`, `logisticos.notification.outbound`, `logisticos.analytics.events` |

Partition count is chosen based on expected throughput and maximum consumer parallelism. The high-volume topics support 12 parallel consumer instances (one per partition) per consumer group.

Partitions are **never decreased** after creation — only increased (and only with careful consumer rebalance planning). Changes require a production change request and an offline consumer rebalance window.

---

### 4. Retention Policy

| Topic Class | Retention | Rationale |
|-------------|-----------|-----------|
| Standard operational | 7 days | Sufficient for consumer lag recovery and incident replay within standard on-call SLA |
| `logisticos.analytics.events` | 30 days | Analytics pipelines may need to replay a full month for model retraining |
| `logisticos.driver.location.updated` | 24 hours | Location data is high-volume; older raw locations are not needed (TimescaleDB holds the canonical store) |
| `*.dlq` | 14 days | DLQ events require manual inspection; 14 days gives ops team time to investigate and replay |

Retention is set via topic-level `retention.ms` configuration, not at the broker default level.

---

### 5. Consumer Group Naming

Consumer group IDs follow this pattern:

```
<service-name>-<purpose>
```

**Rules:**
- One consumer group per consuming service per logical purpose.
- Two independent consumers within the same service that process the same topic for different purposes **must use different consumer group IDs**.
- Consumer group IDs are registered in `docs/kafka/consumer-groups.md` (the canonical registry). Creating an unregistered consumer group fails CI via `scripts/kafka/validate-consumer-groups.sh`.

**Examples:**
```
engagement-service-order-notifications
engagement-service-delivery-confirmations
analytics-service-event-ingestion
dispatch-service-new-orders
ai-layer-dispatch-trigger
ai-layer-cod-reconciliation
payments-service-cod-collection
cdp-service-shipment-events
```

**Anti-patterns (rejected):**
- `engagement-service` — too broad; no indication of what events are consumed
- `shared-notifications` — never share a consumer group across services
- `temp-debug-consumer` — consumer groups leak in Kafka; all groups must be registered and owned

---

### 6. Dead Letter Queue (DLQ) Pattern

High-criticality consumers implement the DLQ pattern. A consumer is classified as high-criticality if message processing failure has direct customer impact (missed notification, failed payment reconciliation, lost POD event).

**DLQ-enabled topics:**
- `logisticos.notification.outbound` → `logisticos.notification.outbound.dlq`
- `logisticos.order.shipment.created` → `logisticos.order.shipment.created.dlq`
- `logisticos.payments.cod.collected` → `logisticos.payments.cod.collected.dlq`
- `logisticos.pod.capture.completed` → `logisticos.pod.capture.completed.dlq`

**DLQ message schema:**
```json
{
  "original_topic": "logisticos.order.shipment.created",
  "original_partition": 2,
  "original_offset": 104823,
  "failed_at": "2026-03-17T10:23:45Z",
  "error_type": "DeserializationError",
  "error_message": "missing field `tenant_id` at line 1 column 42",
  "retry_count": 3,
  "original_payload_bytes": "<base64>",
  "consumer_group": "dispatch-service-new-orders",
  "service_version": "1.4.2"
}
```

**DLQ consumer behavior:**
1. On deserialization failure: write to DLQ immediately (no retry); continue processing.
2. On transient infrastructure failure (DB unavailable, Redis timeout): retry with exponential backoff (max 3 attempts, 500ms → 2s → 8s), then write to DLQ.
3. On business logic failure (e.g., entity not found): write to DLQ with `error_type: "BusinessRuleViolation"`.

DLQ events are processed by the `logisticos-ops` team via a dedicated dashboard. Automated replay is triggered via `scripts/kafka/replay-dlq.sh <topic>`.

---

### 7. No Request/Response over Kafka

Kafka is used exclusively for **fire-and-forget event streaming**. It is not used for:
- Synchronous request/response patterns
- RPC-style calls where the producer waits for a consumer's result
- Distributed sagas where compensating events form a tight back-and-forth loop

**Use gRPC (Tonic) for synchronous inter-service calls.** Mixed patterns (using Kafka for commands + gRPC for response correlation) are permitted only within the dispatch saga and must be documented with a sequence diagram.

---

### 8. Event Schema Standards

All events are serialized as **JSON** with these mandatory envelope fields:

```json
{
  "event_id": "01HZ4V3K9QJ8P5X7YR2M6NW3BD",
  "event_type": "logisticos.order.shipment.created",
  "schema_version": "1.0",
  "occurred_at": "2026-03-17T10:23:45.123Z",
  "tenant_id": "550e8400-e29b-41d4-a716-446655440000",
  "aggregate_id": "7d8e9f10-...",
  "aggregate_type": "Shipment",
  "produced_by": "order-intake-service",
  "payload": { ... }
}
```

`event_id` uses ULID format (sortable, monotonic). Schema is validated at the producer side; consumers treat unknown fields as non-fatal (forward-compatible Postel's Law).

Schema evolution rules:
- **Additive changes** (new optional fields in `payload`): allowed, bump minor version.
- **Breaking changes** (remove or rename fields): require a new `schema_version`, topic migration period of 30 days, and deprecation notice in `docs/kafka/schema-changelog.md`.

---

## Full Topic Inventory

| Topic | Partitions | Retention | DLQ |
|-------|-----------|-----------|-----|
| `logisticos.identity.tenant.created` | 3 | 7 days | — |
| `logisticos.identity.user.invited` | 3 | 7 days | — |
| `logisticos.order.shipment.created` | 3 | 7 days | Yes |
| `logisticos.order.shipment.confirmed` | 3 | 7 days | — |
| `logisticos.order.shipment.cancelled` | 3 | 7 days | — |
| `logisticos.dispatch.route.created` | 3 | 7 days | — |
| `logisticos.dispatch.driver.assigned` | 3 | 7 days | — |
| `logisticos.driver.pickup.completed` | 3 | 7 days | — |
| `logisticos.driver.delivery.completed` | 3 | 7 days | — |
| `logisticos.driver.delivery.failed` | 3 | 7 days | — |
| `logisticos.driver.location.updated` | 12 | 24 hours | — |
| `logisticos.pod.capture.completed` | 3 | 7 days | Yes |
| `logisticos.payments.cod.collected` | 3 | 7 days | Yes |
| `logisticos.payments.invoice.generated` | 3 | 7 days | — |
| `logisticos.hub.parcel.inducted` | 3 | 7 days | — |
| `logisticos.carrier.shipment.allocated` | 3 | 7 days | — |
| `logisticos.notification.outbound` | 12 | 7 days | Yes |
| `logisticos.analytics.events` | 12 | 30 days | — |
| `logisticos.notification.outbound.dlq` | 3 | 14 days | — |
| `logisticos.order.shipment.created.dlq` | 3 | 14 days | — |
| `logisticos.payments.cod.collected.dlq` | 3 | 14 days | — |
| `logisticos.pod.capture.completed.dlq` | 3 | 14 days | — |

---

## Consequences

### Positive

- **Predictable event lineage** — four-segment naming makes the topic's domain, entity, and event immediately parseable by tooling and humans.
- **Per-entity ordering guaranteed** — partition-by-entity-ID eliminates the ordering class of bugs seen in staging.
- **Consumer group isolation** — no consumer group sharing means a slow or failed consumer in one service never starves another.
- **Ops visibility** — DLQ pattern with structured error metadata enables the operations team to investigate and replay failed events without digging through service logs.
- **Analytics replay** — 30-day retention on `analytics.events` enables model retraining pipelines to replay a full calendar month.

### Negative

- **No global ordering** — events for different entities on the same topic are not globally ordered. Services with cross-entity ordering requirements must implement their own sequencing. Accepted — global ordering at Kafka scale is impractical.
- **Schema versioning discipline required** — producers must increment `schema_version` on breaking changes; enforcement relies on code review and the schema registry integration (planned Q3 2026 with Confluent Schema Registry).
- **DLQ operational overhead** — DLQ topics require an ops runbook and monitoring alert. Cost is justified by the reliability improvement.
- **Topic proliferation** — 22 initial topics, growing to ~40 by full service build-out. Managed via the topic inventory table in this ADR and the `create-topics.sh` script.

---

## Related ADRs

- [ADR-0002](0002-event-driven-inter-service-communication.md) — Event-driven inter-service communication (establishes Kafka as the backbone)
- [ADR-0005](0005-hexagonal-architecture-for-microservices.md) — Hexagonal architecture (EventPublisher is a domain port; Kafka is the infrastructure adapter)
- ADR-0009 (planned) — Saga pattern for dispatch and payment workflows

## Related Scripts

- `scripts/kafka/create-topics.sh` — provisions all topics per this ADR
- `scripts/kafka/validate-topics.sh` — CI validation of topic naming compliance
- `scripts/kafka/replay-dlq.sh` — operational DLQ replay tool
