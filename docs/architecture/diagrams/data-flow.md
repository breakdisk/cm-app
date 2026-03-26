# LogisticOS — Data Flow Diagrams

**Version:** 1.0
**Last Updated:** 2026-03-17
**Maintained by:** Principal Software Architect

This document captures the detailed data flows for the four most critical operational paths in the LogisticOS platform. Each diagram shows the sequence of service interactions, data stores touched, events published, and async consumers involved.

---

## 1. Order Intake Flow

**Trigger:** Merchant submits a new shipment booking via the Merchant Portal or API.

```
Merchant Portal / API Client
         │
         │  POST /v1/shipments
         │  { merchant_id, consignee, address, items, cod_amount, ... }
         │
         ▼
┌─────────────────────────────────────────────────────┐
│  API Gateway (Service 17)                           │
│  1. Validate JWT                                    │
│  2. Extract tenant_id from JWT claims               │
│  3. Rate limit check (per merchant API key)         │
│  4. Route to order-intake service via gRPC          │
└─────────────────────┬───────────────────────────────┘
                      │ gRPC: CreateShipmentRequest
                      ▼
┌─────────────────────────────────────────────────────┐
│  Order & Shipment Intake (Service 4)                │
│                                                     │
│  Application layer:                                 │
│  1. CreateShipmentCommand validated                 │
│     • Consignee address geocoded (Mapbox API)       │
│     • COD amount validated against merchant limits  │
│     • Merchant credit limit checked (Redis cache)   │
│  2. AWB number generated (ULID-based, tenant-prefix)│
│  3. Shipment entity created, status = BOOKED        │
│                                                     │
│  Infrastructure layer:                              │
│  4. BEGIN TRANSACTION                               │
│     SET LOCAL app.current_tenant_id = <id>          │
│     INSERT INTO shipments (...)       ──────────── ►│──► PostgreSQL
│     INSERT INTO kafka_outbox (...)    ──────────── ►│──► PostgreSQL
│     COMMIT TRANSACTION                              │
│                                                     │
│  5. Outbox publisher reads kafka_outbox             │
│     PUBLISH: logisticos.order.shipment.created ────►│──► Kafka
│                                                     │
│  6. Return: { shipment_id, awb_number, status }    │
└─────────────────────────────────────────────────────┘
         │
         │  Kafka: logisticos.order.shipment.created
         │  partition_key = shipment_id
         │  payload = { shipment_id, tenant_id, awb, consignee, address,
         │              geo_point, cod_amount, merchant_id, created_at }
         │
         ├──────────────────────────────────────────────────────────────┐
         │                            │                                 │
         ▼                            ▼                                 ▼
┌────────────────────┐  ┌─────────────────────────────┐  ┌─────────────────────┐
│  Dispatch Service  │  │  Engagement Engine           │  │  CDP Service         │
│  (Service 5)       │  │  (Service 3)                 │  │  (Service 2)         │
│                    │  │                              │  │                      │
│  Consumer group:   │  │  Consumer group:             │  │  Consumer group:     │
│  dispatch-service  │  │  engagement-service          │  │  cdp-service-        │
│  -new-orders       │  │  -order-notifications        │  │  shipment-events     │
│                    │  │                              │  │                      │
│  1. Create route   │  │  1. Lookup customer profile  │  │  1. Upsert customer  │
│     stub for       │  │     (CDP gRPC call)          │  │     profile          │
│     pending        │  │  2. Select channel (WA/SMS)  │  │  2. Log shipment     │
│     assignment     │  │     per customer preference  │  │     event in CDP     │
│  2. Queue for AI   │  │  3. Generate booking         │  │  3. Update total     │
│     dispatch agent │  │     confirmation message     │  │     shipment count   │
│  3. PUBLISH:       │  │  4. PUBLISH:                 │  │     for CLV model    │
│     dispatch.route │  │     notification.outbound    │  │                      │
│     .created       │  │     (WhatsApp or SMS)        │  └─────────────────────┘
└────────────────────┘  └─────────────────────────────┘
                                    │
                                    │  Kafka: logisticos.notification.outbound
                                    ▼
                       ┌─────────────────────────────────┐
                       │  Engagement Engine               │
                       │  (Notification Sender)           │
                       │  Consumer group:                 │
                       │  engagement-service              │
                       │  -outbound-sender                │
                       │                                  │
                       │  1. Dequeue notification         │
                       │  2. Call WhatsApp Business API   │
                       │     (Twilio) or SMS gateway      │
                       │  3. Record delivery receipt      │
                       │  4. On failure: write to         │
                       │     notification.outbound.dlq    │
                       └─────────────────────────────────┘

                                    │ (in parallel)
                                    ▼
                       ┌─────────────────────────────────┐
                       │  Analytics Service (Service 13)  │
                       │  Consumer group:                 │
                       │  analytics-service               │
                       │  -event-ingestion                │
                       │                                  │
                       │  INSERT INTO ClickHouse:         │
                       │  shipment_events table           │
                       │  (event_type=shipment.created,   │
                       │   tenant_id, merchant_id, etc.)  │
                       └─────────────────────────────────┘
```

**Latency Targets:**
- API Gateway → Order Intake response: P99 < 300ms
- Kafka event published after DB commit: < 100ms (transactional outbox pattern)
- WhatsApp confirmation delivered to customer: < 5s from event publish

---

## 2. AI Dispatch Flow

**Trigger:** A new route stub is created in the dispatch service queue, or the AI agent is triggered by a `logisticos.agent.dispatch.triggered` event.

```
Kafka: logisticos.dispatch.route.created
(emitted by dispatch service when a shipment needs driver assignment)
         │
         │  partition_key = route_id
         │  payload = { route_id, tenant_id, shipment_ids[], pickup_location,
         │              delivery_stops[], priority, sla_deadline }
         │
         ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  AI Intelligence Layer — Dispatch Consumer                              │
│  (Service 16, Python)                                                   │
│  Consumer group: ai-layer-dispatch-trigger                              │
│                                                                         │
│  1. Event deserialized, enriched with tenant context                    │
│  2. AgentRunner.run(agent="dispatch_agent", context=route_context)      │
└──────────────────────────┬──────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  Dispatch Agent (LangGraph stateful agent)                              │
│                                                                         │
│  System prompt includes:                                                │
│  • Tenant-specific routing rules from business-logic service            │
│  • SLA deadline for this delivery batch                                 │
│  • Current time, day of week, weather context                           │
│                                                                         │
│  Agent planning steps:                                                  │
│                                                                         │
│  Step 1: Gather context                                                 │
│  ┌──────────────────────────────────────────────────────────┐          │
│  │  Tool call → get_available_drivers                        │          │
│  │  { zone: "manila-north", vehicle_type: "motorcycle",     │          │
│  │    max_capacity_kg: 30 }                                  │          │
│  │                                                           │          │
│  │  MCP → dispatch-mcp-server → dispatch service            │          │
│  │  Response: [{ driver_id, name, location, workload,       │          │
│  │              current_route_eta }]                         │          │
│  └──────────────────────────────────────────────────────────┘          │
│                                                                         │
│  Step 2: Optimize route                                                 │
│  ┌──────────────────────────────────────────────────────────┐          │
│  │  Tool call → optimize_route                               │          │
│  │  { driver_id, stops: [...], algorithm: "vrp_aco" }       │          │
│  │                                                           │          │
│  │  MCP → dispatch-mcp-server → dispatch service            │          │
│  │  (Calls ONNX VRP solver internally)                       │          │
│  │  Response: { optimized_stops, total_km,                  │          │
│  │              estimated_completion_time }                  │          │
│  └──────────────────────────────────────────────────────────┘          │
│                                                                         │
│  Step 3: Verify ETA against SLA                                         │
│  • If estimated_completion_time > sla_deadline:                        │
│    → Retry with different driver or split route                        │
│    → If no solution: escalate (send_notification to ops team)          │
│                                                                         │
│  Step 4: Assign driver                                                  │
│  ┌──────────────────────────────────────────────────────────┐          │
│  │  Tool call → assign_driver                                │          │
│  │  { route_id, driver_id, optimized_stops }                │          │
│  │                                                           │          │
│  │  MCP → dispatch-mcp-server → dispatch service            │          │
│  │  (dispatch service: UPDATE route, PUBLISH driver.assigned)│          │
│  │  Response: { assignment_id, driver_notified: true }       │          │
│  └──────────────────────────────────────────────────────────┘          │
│                                                                         │
│  Step 5: Notify driver                                                  │
│  ┌──────────────────────────────────────────────────────────┐          │
│  │  Tool call → send_notification                            │          │
│  │  { recipient_id: driver_id, channel: "push",             │          │
│  │    template: "new_route_assigned",                        │          │
│  │    data: { route_id, stop_count, first_pickup_address } } │          │
│  │                                                           │          │
│  │  MCP → engagement-mcp-server → engagement service        │          │
│  └──────────────────────────────────────────────────────────┘          │
│                                                                         │
│  6. Agent outcome logged:                                               │
│     { agent_id, tenant_id, route_id, driver_id, tool_calls[],         │
│       total_tokens, outcome: "success", duration_ms }                  │
│     → PostgreSQL audit_log table (via ai-layer internal write)         │
└─────────────────────────────────────────────────────────────────────────┘
         │
         │  Kafka: logisticos.dispatch.driver.assigned
         │  (emitted by dispatch service upon successful assign_driver MCP call)
         │
         ├────────────────────────────────────┐
         │                                    │
         ▼                                    ▼
┌────────────────────────┐        ┌─────────────────────────┐
│  Driver App (push      │        │  Engagement Service      │
│  notification via      │        │  Consumer group:         │
│  Expo Push / FCM)      │        │  engagement-service      │
│                        │        │  -delivery-confirmations │
│  Driver receives:      │        │                          │
│  "New route ready"     │        │  Sends merchant:         │
│  • route_id            │        │  "Driver [name] assigned │
│  • stop_count          │        │   to your delivery batch"│
│  • first pickup addr.  │        │  (WhatsApp or email)     │
└────────────────────────┘        └─────────────────────────┘
```

**AI Agent Guardrails:**
- All MCP tool calls are RBAC-governed: the dispatch agent cannot call payment or CDP tools
- Agent has a maximum of 10 tool calls per run (prevents runaway loops)
- All tool invocations are audit-logged with full input/output for compliance review
- If the agent fails to assign within 30 seconds, the route falls back to manual dispatch queue

---

## 3. POD Capture Flow

**Trigger:** Driver arrives at delivery address and captures Proof of Delivery in the Driver App.

```
Driver App (React Native — offline-first, ADR-0007)
         │
         │  Driver actions:
         │  1. Mark task "in_progress" (SQLite write, sync queue enqueued)
         │  2. Capture photo → saved to device FileSystem cache
         │  3. Capture customer signature → SVG saved to SQLite
         │  4. Confirm OTP (customer shows OTP from SMS) → saved to SQLite
         │  5. Mark task "completed" (SQLite write, sync queue enqueued)
         │
         │  [All above happens instantly — no network required]
         │
         │  On network reconnect:
         │  SyncService.drainQueue() begins
         │
         ├──────────────────────────────────────────────────────────────────┐
         │                                                                  │
         ▼                                                                  ▼
┌───────────────────────┐                                    ┌─────────────────────────┐
│  Driver Ops Service   │                                    │  POD Service             │
│  (Service 6)          │                                    │  (Service 11)            │
│                       │                                    │                          │
│  POST /sync/events    │                                    │  POST /pod/presigned-url │
│                       │                                    │                          │
│  Receives:            │                                    │  1. Validates driver +   │
│  • task_status updates│                                    │     shipment ownership   │
│  • location_pings     │                                    │  2. Generates S3         │
│  • cod_collected      │                                    │     presigned URL        │
│                       │                                    │     (15-min TTL)         │
│  1. Validates sync    │                                    │  3. Returns:             │
│     events (server-   │                                    │     { presigned_url,     │
│     authoritative     │                                    │       s3_key }           │
│     conflict check)   │                                    └─────────────────────────┘
│  2. Applies task      │                                              │
│     state transitions │                                              │
│  3. Writes to         │                                    Driver App uploads photo
│     TimescaleDB       │                                    directly to S3 via
│     (location pings)  │                                    presigned URL (bypasses
│  4. Writes to         │                                    API servers for large files)
│     PostgreSQL        │                                              │
│     (task status)     │                                              ▼
│  5. PUBLISH:          │                               ┌─────────────────────────────┐
│     driver.delivery   │                               │  S3 Object Storage           │
│     .completed ──────►├──► Kafka                      │  pod-photos/<tenant>/<id>.jpg│
│     (or .failed)      │                               └─────────────────────────────┘
└───────────────────────┘                                              │
         │                                                             │
         │  Driver App: POST /pod/captures                            │
         │  { pod_id, task_id, s3_key, signature_svg,                │
         │    otp_value, captured_at, lat, lng }                      │
         │                                                             │
         ▼                                                             │
┌──────────────────────────────────────────────────────────────────────┘
│  POD Service (Service 11)
│
│  1. Validate: s3_key exists in S3 (HEAD request)
│  2. Validate: OTP matches expected value (Redis lookup, TTL check)
│  3. Validate: GPS coordinates within acceptable radius of delivery address
│  4. BEGIN TRANSACTION (tenant-scoped via RLS)
│     INSERT INTO pod_captures (id, shipment_id, tenant_id,
│       photo_s3_key, signature_svg, otp_confirmed, lat, lng, captured_at)
│     UPDATE shipments SET status = 'DELIVERED', delivered_at = now()
│     INSERT INTO kafka_outbox (event: pod.capture.completed)
│     COMMIT
│  5. On any validation failure: INSERT INTO pod_captures with status='DISPUTED'
│                                Write to pod.capture.completed.dlq
└──────────────────────────────────────────────────────────────────────
         │
         │  Kafka: logisticos.pod.capture.completed
         │  partition_key = shipment_id
         │  payload = { pod_id, shipment_id, tenant_id, driver_id,
         │              consignee_id, photo_s3_key, delivered_at,
         │              cod_amount_collected, cod_currency }
         │
         ├─────────────────┬──────────────────────────┬─────────────────────┐
         │                 │                          │                     │
         ▼                 ▼                          ▼                     ▼
┌──────────────┐  ┌────────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  Engagement  │  │  Analytics Service  │  │  Payments Service │  │  CDP Service     │
│  Service     │  │  (Service 13)       │  │  (Service 12)     │  │  (Service 2)     │
│              │  │                    │  │                   │  │                  │
│  Sends       │  │  Records delivery  │  │  If COD:          │  │  Updates         │
│  delivery    │  │  event in          │  │  → PUBLISH:       │  │  customer        │
│  confirmation│  │  ClickHouse:       │  │    cod.collected  │  │  delivery history│
│  to customer │  │  • Delivery time   │  │  (triggers COD    │  │  Updates CLV     │
│  (WhatsApp): │  │  • SLA compliance  │  │  reconciliation   │  │  score input     │
│  "Your       │  │  • Zone            │  │  flow — see       │  │  Increments      │
│  package     │  │    performance     │  │  Flow 4 below)    │  │  successful      │
│  arrived!"   │  │  • Driver perf.    │  │                   │  │  delivery count  │
│  + POD link  │  │    record          │  │  If prepaid:      │  │  Triggers next-  │
│              │  │                    │  │  → no action      │  │  shipment        │
│  Triggers    │  │                    │  │                   │  │  campaign eval   │
│  post-       │  │                    │  │                   │  └──────────────────┘
│  delivery    │  │                    │  └──────────────────┘
│  feedback    │  └────────────────────┘
│  request     │
│  (1hr delay) │
└──────────────┘
```

**Failure Scenarios:**
- OTP validation fails → `pod_captures.status = DISPUTED`, alert sent to ops, driver instructed to retry or escalate
- S3 upload fails → SyncService retries with exponential backoff (30s, 60s, 120s, 240s, 480s)
- Pod service unreachable → events remain in SQLite sync queue; processed when service recovers

---

## 4. COD Reconciliation Flow

**Trigger:** `logisticos.payments.cod.collected` event emitted after driver collects cash payment at the doorstep.

```
[POD Capture Flow above emits logisticos.payments.cod.collected]
OR
[Driver explicitly submits COD via Driver App COD confirmation screen]
         │
         │  Kafka: logisticos.payments.cod.collected
         │  partition_key = shipment_id
         │  payload = { shipment_id, driver_id, tenant_id, merchant_id,
         │              collected_amount, currency, collected_at,
         │              pod_id, driver_location_at_collection }
         │
         ├──────────────────────────────────────────────────────────────────┐
         │                                                                  │
         ▼                                                                  ▼
┌───────────────────────────────────┐               ┌─────────────────────────────────┐
│  Payments Service (Service 12)    │               │  AI Intelligence Layer           │
│  Consumer group:                  │               │  Consumer group:                 │
│  payments-service-cod-collection  │               │  ai-layer-cod-reconciliation     │
│                                   │               │                                  │
│  Immediate accounting:            │               │  AI Reconciliation Agent:        │
│  1. Validate: amount matches       │               │  1. Checks driver's total COD    │
│     shipment COD value            │               │     collected today vs expected  │
│  2. Credit merchant wallet        │               │  2. Detects anomalies:           │
│     (minus platform fee)          │               │     • Partial collection         │
│  3. Debit driver COD float ledger │               │     • Excess cash (data error)   │
│  4. Record transaction in         │               │     • Duplicate event            │
│     PostgreSQL (RLS-scoped)       │               │  3. Tool call → get_cod_balance  │
│  5. PUBLISH:                      │               │     (payments MCP server)        │
│     payments.invoice.generated    │               │  4. If anomaly:                  │
│     (if threshold crossed for     │               │     Tool call → send_notification│
│     merchant payout run)          │               │     (alert to finance ops team)  │
│                                   │               │  5. Log outcome to audit_log     │
└───────────────────────────────────┘               └─────────────────────────────────┘
         │
         │  Kafka: logisticos.payments.invoice.generated
         │  (if automated payout threshold reached, e.g., weekly or ₱50,000)
         │
         ▼
┌───────────────────────────────────┐
│  Payments Service — Invoice Runner│
│                                   │
│  1. Aggregate COD transactions    │
│     for merchant in period        │
│  2. Calculate platform fees       │
│     (per-shipment + % of COD)     │
│  3. Generate PDF invoice          │
│     (stored in S3)                │
│  4. Record invoice in PostgreSQL  │
│  5. Trigger payout via payment    │
│     gateway (Stripe / PayMongo)   │
│  6. PUBLISH:                      │
│     (internal): payout.initiated  │
└───────────────────────────────────┘
         │
         ├─────────────────────────────────────────────────────────────────┐
         │                                                                 │
         ▼                                                                 ▼
┌───────────────────────────────────┐          ┌──────────────────────────────────────┐
│  Engagement Service               │          │  Analytics Service                    │
│  (Merchant Notification)          │          │                                       │
│                                   │          │  Records to ClickHouse:               │
│  Sends merchant:                  │          │  • cod_reconciliation_events          │
│  "Your ₱28,450 payout has been    │          │  • driver_cod_compliance (for driver  │
│  initiated. Invoice: [link]"      │          │    performance scoring)               │
│  (Email + WhatsApp)               │          │  • merchant_billing_events            │
│                                   │          │  (feeds BI dashboard COD summaries)   │
└───────────────────────────────────┘          └──────────────────────────────────────┘
```

**COD Compliance Controls:**
- Driver COD float maximum enforced per tenant configuration (e.g., ₱5,000 per day max before mandatory remittance)
- AI agent detects drivers with abnormally high or low collection rates and flags for review
- All COD transactions are immutable ledger records — corrections are made via contra-entries, never by updating existing records
- `payments.cod.collected.dlq` catches malformed events or failed validation; finance ops team reviews within 1 business day SLA

---

## Event Lineage Summary

The table below shows how a single shipment progresses through Kafka events from creation to payout:

```
Time  Event                                    Producer           Primary Consumers
────  ──────────────────────────────────────  ─────────────────  ──────────────────────────────────────
t+0   order.shipment.created                  order-intake       dispatch, engagement, cdp, analytics
t+2   dispatch.route.created                  dispatch           ai-layer (dispatch agent)
t+8   dispatch.driver.assigned               dispatch (via MCP)  driver-app (push), engagement, analytics
t+15  notification.outbound (booking conf.)  engagement          engagement-sender → WhatsApp API
t+Xm  driver.pickup.completed                driver-ops          engagement, analytics, hub-ops
t+Ym  hub.parcel.inducted                    hub-ops             carrier, analytics
t+Zm  carrier.shipment.allocated             carrier             analytics, engagement
t+Wm  driver.delivery.completed              driver-ops          pod, engagement, analytics, cdp
t+Wm  pod.capture.completed                  pod                 engagement, analytics, payments, cdp
t+Wm  payments.cod.collected                 payments            payments (reconciler), ai-layer, analytics
t+Wm  notification.outbound (delivered)      engagement          engagement-sender → WhatsApp API
t+1h  notification.outbound (feedback req.)  engagement          engagement-sender → WhatsApp API
t+7d  payments.invoice.generated             payments            engagement, analytics
```

---

## Data Store Access Patterns by Flow

| Flow | PostgreSQL | Redis | Kafka | ClickHouse | TimescaleDB | S3 |
|------|-----------|-------|-------|-----------|------------|-----|
| Order Intake | W (shipment) | R (credit limit cache) | W (shipment.created) | — | — | — |
| AI Dispatch | R (route via MCP) | R (driver location) | R (trigger) W (assigned) | — | — | — |
| POD Capture | W (pod, shipment) | R (OTP validation) | W (pod.completed) | — | — | W (photo) |
| COD Reconciliation | W (ledger, invoice) | — | R (cod.collected) W (invoice.generated) | W (billing events) | — | W (invoice PDF) |

---

## Related Documents

- [system-overview.md](system-overview.md) — Full architecture diagram
- [ADR-0002](../../adr/0002-event-driven-inter-service-communication.md) — Event-driven communication principles
- [ADR-0004](../../adr/0004-mcp-for-ai-interoperability.md) — MCP tool layer (used in AI dispatch flow)
- [ADR-0006](../../adr/0006-kafka-event-streaming-topology.md) — Kafka topic naming and partitioning
- [ADR-0007](../../adr/0007-offline-first-driver-app.md) — Offline-first driver app (POD capture flow)
- [ADR-0008](../../adr/0008-multi-tenancy-rls-strategy.md) — RLS (all PostgreSQL writes in all flows are tenant-scoped)
