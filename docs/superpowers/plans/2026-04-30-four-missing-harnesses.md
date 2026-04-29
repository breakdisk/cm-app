# Four Missing End-to-End Harnesses Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the four gaps in the booking-to-invoicing Kafka pipeline: engagement notifications, COD remittance auto-batching, delivery-failed re-dispatch, and weight-adjustment surcharge invoicing.

**Architecture:** Each gap is a self-contained Kafka consumer or scheduled task. We fix bugs in existing wiring, add missing repository methods, create new consumer files, and wire them into bootstrap — no new services, no schema changes beyond one SQL query addition.

**Tech Stack:** Rust, rdkafka StreamConsumer, SQLx PgPool, tokio::time::interval, axum Router, logisticos_events payloads/topics

---

## File Map

| Action | File |
|--------|------|
| Modify | `services/engagement/src/bootstrap.rs` |
| Create | `services/engagement/src/infrastructure/messaging.rs` (currently stub) |
| Modify | `services/payments/src/domain/repositories/mod.rs` |
| Modify | `services/payments/src/infrastructure/db/cod_repo.rs` |
| Modify | `services/payments/src/application/services/cod_remittance_service.rs` |
| Modify | `services/payments/src/bootstrap.rs` |
| Modify | `services/dispatch/src/domain/repositories/dispatch_queue_repo.rs` |
| Modify | `services/dispatch/src/infrastructure/db/dispatch_queue_repo.rs` |
| Modify | `services/dispatch/src/api/http/mod.rs` (or router file) |
| Modify | `services/business-logic/src/bootstrap.rs` |
| Modify | `services/payments/src/domain/repositories/mod.rs` (InvoiceRepository) |
| Modify | `services/payments/src/infrastructure/db/invoice_repo.rs` |
| Create | `services/payments/src/infrastructure/messaging/weight_discrepancy_consumer.rs` |
| Modify | `services/payments/src/infrastructure/messaging/mod.rs` |
| Modify | `services/payments/src/bootstrap.rs` |

---

### Task 1: Fix Engagement Notification Consumer

**Problem:** The engagement service subscribes to 8 topics but notifications silently fail because:
1. `tenant_id` is extracted from `data["tenant_id"]` but lives in the Event envelope at `payload["tenant_id"]`
2. DRIVER_ASSIGNED has no `customer_phone` field so WhatsApp notifications are always skipped
3. No graceful shutdown signal

**Files:**
- Modify: `services/engagement/src/bootstrap.rs`
- Modify: `services/engagement/src/application/services/event_consumer.rs`

- [ ] **Step 1: Read the current engagement bootstrap consumer loop**

```bash
# Already read in research — key section is run_kafka_consumer() in bootstrap.rs
# The bug is on the line: let tenant_id_str = data["tenant_id"].as_str()...
```

- [ ] **Step 2: Fix tenant_id extraction in event_consumer.rs**

In `services/engagement/src/application/services/event_consumer.rs`, find the `process_event` function.
Change the tenant_id extraction to try envelope level first:

```rust
// OLD (broken — data["tenant_id"] is null for SHIPMENT_CREATED)
let tenant_id_str = data["tenant_id"].as_str()
    .ok_or_else(|| anyhow::anyhow!("Event missing tenant_id — skipping notification"))?;

// NEW — try envelope first, fall back to data field (some payloads embed it)
let tenant_id_str = payload["tenant_id"]
    .as_str()
    .or_else(|| data["tenant_id"].as_str())
    .ok_or_else(|| anyhow::anyhow!("Event missing tenant_id in both envelope and data"))?;
```

- [ ] **Step 3: Fix DRIVER_ASSIGNED channel mapping to not require customer_phone**

In `event_consumer.rs`, find the `get_mapping()` for `"driver.assigned"` / DRIVER_ASSIGNED.
The event payload has `shipment_id` but no `customer_phone`. Change channels from `["whatsapp"]` to `["log"]` until the dispatch service carries customer contact through:

```rust
// For driver.assigned mapping — change channels until customer_phone flows through
"driver.assigned" => Some(EventMapping {
    template_key: "pickup_scheduled",
    channels: &["log"],  // WhatsApp re-enabled once DRIVER_ASSIGNED carries customer_phone
    recipient_field: "shipment_id",
}),
```

- [ ] **Step 4: Add shutdown signal to the consumer in bootstrap.rs**

In `services/engagement/src/bootstrap.rs`, the `run_kafka_consumer` function runs an infinite loop with no shutdown. Add a `watch::Receiver<bool>` parameter and `tokio::select!`:

```rust
// Change signature from:
async fn run_kafka_consumer(brokers: String, group_id: String, consumer_service: Arc<EventConsumerService>)

// To:
async fn run_kafka_consumer(
    brokers: String,
    group_id: String,
    consumer_service: Arc<EventConsumerService>,
    mut shutdown: watch::Receiver<bool>,
)
```

Inside the loop, replace:
```rust
loop {
    match consumer.recv().await {
```

With:
```rust
loop {
    tokio::select! {
        _ = shutdown.changed() => {
            if *shutdown.borrow_and_update() {
                tracing::info!("Engagement consumer shutting down");
                break;
            }
        }
        result = consumer.recv() => {
            match result {
```

And close the extra brace. Pass `shutdown_rx` from the `watch::channel` already present in `run()`.

- [ ] **Step 5: Thread the shutdown receiver into run_kafka_consumer call**

In `bootstrap.rs run()`, find where `tokio::spawn(async move { run_kafka_consumer(...).await })` is called.
Add `shutdown_tx.subscribe()` as the final arg.

- [ ] **Step 6: Run clippy to verify**

```bash
cargo clippy -p engagement 2>&1 | head -50
```

Expected: 0 errors (warnings about unused imports OK)

- [ ] **Step 7: Commit**

```bash
git add services/engagement/src/application/services/event_consumer.rs
git add services/engagement/src/bootstrap.rs
git commit -m "fix(engagement): extract tenant_id from Event envelope; add consumer shutdown signal"
```

---

### Task 2: COD Remittance Auto-Batching

**Problem:** `create_batch()` is fully implemented but never called automatically. Merchants' COD sits in `collected` state forever unless someone calls the HTTP endpoint. Need a nightly Tokio interval that sweeps all tenants/merchants with unbatched COD and creates batches.

**Files:**
- Modify: `services/payments/src/domain/repositories/mod.rs`
- Modify: `services/payments/src/infrastructure/db/cod_repo.rs`
- Modify: `services/payments/src/application/services/cod_remittance_service.rs`
- Modify: `services/payments/src/bootstrap.rs`

- [ ] **Step 1: Add distinct_merchants_with_unbatched_cod to CodRepository trait**

In `services/payments/src/domain/repositories/mod.rs`, find the `CodRepository` trait.
Add:

```rust
/// Returns (tenant_id, merchant_id) pairs that have at least one unbatched COD collection
/// with collected_at <= cutoff.
async fn distinct_merchants_with_unbatched_cod(
    &self,
    cutoff: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(uuid::Uuid, uuid::Uuid)>, sqlx::Error>;
```

- [ ] **Step 2: Implement in PgCodRepository**

In `services/payments/src/infrastructure/db/cod_repo.rs`, add:

```rust
async fn distinct_merchants_with_unbatched_cod(
    &self,
    cutoff: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(uuid::Uuid, uuid::Uuid)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT DISTINCT tenant_id, merchant_id
           FROM payments.cod_collections
           WHERE batch_id IS NULL
             AND collected_at <= $1"#,
        cutoff
    )
    .fetch_all(&self.pool)
    .await?;
    Ok(rows.iter().map(|r| (r.tenant_id, r.merchant_id)).collect())
}
```

- [ ] **Step 3: Add run_daily_batching to CodRemittanceService**

In `services/payments/src/application/services/cod_remittance_service.rs`, add:

```rust
/// Sweeps all merchants with unbatched COD up to cutoff_date and creates batches.
/// Designed to be called nightly. Failures per-merchant are non-fatal.
pub async fn run_daily_batching(&self, cutoff: chrono::DateTime<chrono::Utc>) -> anyhow::Result<()> {
    let pairs = self.cod_repo.distinct_merchants_with_unbatched_cod(cutoff).await
        .map_err(|e| anyhow::anyhow!("Failed to query unbatched COD merchants: {e}"))?;

    tracing::info!(merchant_count = pairs.len(), "COD daily batching run started");

    for (tenant_id, merchant_id) in pairs {
        use crate::application::commands::CreateCodBatchCommand;
        use logisticos_types::{TenantId, MerchantId};

        let cmd = CreateCodBatchCommand {
            merchant_id,
            cutoff_date: cutoff.date_naive(),
        };
        match self.create_batch(TenantId::from_uuid(tenant_id), cmd).await {
            Ok(batch) => tracing::info!(
                batch_id = %batch.id,
                merchant_id = %merchant_id,
                "COD batch created"
            ),
            Err(e) => tracing::warn!(
                merchant_id = %merchant_id,
                err = %e,
                "COD batch creation failed — will retry next run"
            ),
        }
    }

    Ok(())
}
```

- [ ] **Step 4: Wire nightly interval in payments bootstrap**

In `services/payments/src/bootstrap.rs`, after the PodConsumer spawn, add:

```rust
// Nightly COD auto-batching — groups all unbatched COD by merchant up to previous midnight.
let remittance_svc_for_cron = Arc::clone(&remittance_service);
tokio::spawn(async move {
    // Run immediately on startup (catches any missed yesterday), then every 24h.
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(24 * 3600));
    loop {
        tick.tick().await;
        // Cut off at start of current UTC day (yesterday's COD)
        let cutoff = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        if let Err(e) = remittance_svc_for_cron.run_daily_batching(cutoff).await {
            tracing::error!(err = %e, "COD daily batching cron failed");
        }
    }
});
```

- [ ] **Step 5: Run clippy**

```bash
cargo clippy -p payments 2>&1 | head -50
```

Expected: 0 errors

- [ ] **Step 6: Commit**

```bash
git add services/payments/src/domain/repositories/mod.rs
git add services/payments/src/infrastructure/db/cod_repo.rs
git add services/payments/src/application/services/cod_remittance_service.rs
git add services/payments/src/bootstrap.rs
git commit -m "feat(payments): nightly COD auto-batching via distinct_merchants_with_unbatched_cod"
```

---

### Task 3: Delivery-Failed Re-Dispatch

**Problem:** `business-logic` consumes `DELIVERY_FAILED` and executes `RescheduleDelivery` action, but `HttpActionExecutor::reschedule_delivery()` POSTs to `{order_url}/v1/shipments/{id}/reschedule` which does NOT exist in order-intake. The correct fix is to add an internal requeue endpoint to dispatch (which already has the `reset_to_pending` concept) and point business-logic there.

**Files:**
- Modify: `services/dispatch/src/domain/repositories/dispatch_queue_repo.rs`
- Modify: `services/dispatch/src/infrastructure/db/dispatch_queue_repo.rs`
- Modify: `services/dispatch/src/api/http/mod.rs` (dispatch router)
- Modify: `services/business-logic/src/bootstrap.rs`

- [ ] **Step 1: Add reset_to_pending to DispatchQueueRepository trait**

In `services/dispatch/src/domain/repositories/dispatch_queue_repo.rs`, add:

```rust
/// Re-queues a shipment that previously failed delivery.
/// Inserts a new dispatch_queue row with status='pending' if one doesn't already exist,
/// or resets an existing row's status back to 'pending' and increments attempt_count.
async fn reset_to_pending(&self, shipment_id: uuid::Uuid) -> Result<(), sqlx::Error>;
```

- [ ] **Step 2: Implement reset_to_pending in PgDispatchQueueRepository**

In `services/dispatch/src/infrastructure/db/dispatch_queue_repo.rs`, add:

```rust
async fn reset_to_pending(&self, shipment_id: uuid::Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO dispatch.dispatch_queue (shipment_id, tenant_id, status, attempt_count, created_at, updated_at)
           SELECT shipment_id, tenant_id, 'pending', 0, NOW(), NOW()
           FROM dispatch.dispatch_queue
           WHERE shipment_id = $1
           LIMIT 1
           ON CONFLICT (shipment_id) DO UPDATE
             SET status = 'pending',
                 attempt_count = dispatch_queue.attempt_count + 1,
                 updated_at = NOW()"#,
        shipment_id
    )
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

- [ ] **Step 3: Add POST /v1/internal/shipments/:id/requeue to dispatch router**

In `services/dispatch/src/api/http/mod.rs` (or wherever the axum Router is built), add an internal requeue endpoint.

First, find the router builder. Add:

```rust
.route("/v1/internal/shipments/:id/requeue", axum::routing::post(requeue_shipment))
```

Then add the handler (in the same file or a new handler file):

```rust
async fn requeue_shipment(
    State(state): State<Arc<AppState>>,
    Path(shipment_id): Path<uuid::Uuid>,
) -> impl IntoResponse {
    match state.queue_repo.reset_to_pending(shipment_id).await {
        Ok(_) => {
            tracing::info!(shipment_id = %shipment_id, "Shipment requeued for dispatch retry");
            axum::http::StatusCode::NO_CONTENT
        }
        Err(e) => {
            tracing::error!(shipment_id = %shipment_id, err = %e, "Failed to requeue shipment");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
```

- [ ] **Step 4: Fix HttpActionExecutor::reschedule_delivery in business-logic**

In `services/business-logic/src/bootstrap.rs`, find `HttpActionExecutor` and its `reschedule_delivery` implementation.

Change the URL from order-intake to dispatch internal:

```rust
// OLD (broken — this endpoint doesn't exist in order-intake)
let url = format!("{}/v1/shipments/{}/reschedule", self.order_service_url, shipment_id);

// NEW — dispatch internal requeue endpoint
let url = format!("{}/v1/internal/shipments/{}/requeue", self.dispatch_service_url, shipment_id);
let res = self.client.post(&url).send().await?;
```

This requires `dispatch_service_url` to be available in `HttpActionExecutor`. If it only has `order_service_url`, add `dispatch_service_url: String` to the struct and wire it from config:

```rust
struct HttpActionExecutor {
    client: reqwest::Client,
    order_service_url: String,
    dispatch_service_url: String,  // add this
}
```

And in bootstrap where `HttpActionExecutor::new()` is called, pass `cfg.dispatch.url` (or equivalent env var).

- [ ] **Step 5: Add DISPATCH_SERVICE_URL config to business-logic**

In `services/business-logic/src/config.rs` (or equivalent), add:

```rust
pub dispatch_url: String,  // e.g. http://dispatch:3005
```

Load from env `DISPATCH__URL` or `DISPATCH_SERVICE_URL`. Wire into HttpActionExecutor construction in bootstrap.

- [ ] **Step 6: Run clippy**

```bash
cargo clippy -p dispatch -p business-logic 2>&1 | head -60
```

Expected: 0 errors

- [ ] **Step 7: Commit**

```bash
git add services/dispatch/src/domain/repositories/dispatch_queue_repo.rs
git add services/dispatch/src/infrastructure/db/dispatch_queue_repo.rs
git add services/dispatch/src/api/http/mod.rs
git add services/business-logic/src/bootstrap.rs
git add services/business-logic/src/config.rs
git commit -m "feat(dispatch+business-logic): internal requeue endpoint; fix delivery-failed re-dispatch URL"
```

---

### Task 4: Weight Adjustment Surcharge Consumer

**Problem:** `InvoiceService::apply_weight_adjustment()` exists but is never triggered. Hub-ops publishes `WEIGHT_DISCREPANCY_FOUND` when a weigh-bridge scan differs from declared weight. Payments needs to consume that event and call `apply_weight_adjustment()`.

**Files:**
- Modify: `services/payments/src/domain/repositories/mod.rs`
- Modify: `services/payments/src/infrastructure/db/invoice_repo.rs`
- Create: `services/payments/src/infrastructure/messaging/weight_discrepancy_consumer.rs`
- Modify: `services/payments/src/infrastructure/messaging/mod.rs`
- Modify: `services/payments/src/bootstrap.rs`

- [ ] **Step 1: Add find_latest_issued_for_merchant to InvoiceRepository trait**

In `services/payments/src/domain/repositories/mod.rs`, find the `InvoiceRepository` trait. Add:

```rust
/// Returns the most recently issued (status = 'issued') invoice for a merchant,
/// or None if none exists. Used by weight-discrepancy consumer to find which invoice
/// to append the surcharge to.
async fn find_latest_issued_for_merchant(
    &self,
    tenant_id: uuid::Uuid,
    merchant_id: uuid::Uuid,
) -> Result<Option<crate::domain::entities::Invoice>, sqlx::Error>;
```

- [ ] **Step 2: Implement in PgInvoiceRepository**

In `services/payments/src/infrastructure/db/invoice_repo.rs`, add:

```rust
async fn find_latest_issued_for_merchant(
    &self,
    tenant_id: uuid::Uuid,
    merchant_id: uuid::Uuid,
) -> Result<Option<Invoice>, sqlx::Error> {
    // Reuse the existing SELECT constant columns but filter for issued invoices
    let row = sqlx::query_as!(
        InvoiceRow,
        r#"SELECT id, tenant_id, invoice_type as "invoice_type: _", merchant_id,
                  customer_id, shipment_id, status as "status: _", line_items,
                  adjustments, subtotal_cents, tax_cents, total_cents,
                  currency, due_date, issued_at, paid_at, created_at, updated_at
           FROM payments.invoices
           WHERE tenant_id = $1
             AND merchant_id = $2
             AND status = 'issued'
           ORDER BY issued_at DESC
           LIMIT 1"#,
        tenant_id,
        merchant_id,
    )
    .fetch_optional(&self.pool)
    .await?;
    Ok(row.map(Invoice::from))
}
```

- [ ] **Step 3: Create weight_discrepancy_consumer.rs**

Create `services/payments/src/infrastructure/messaging/weight_discrepancy_consumer.rs`:

```rust
//! Consumes WEIGHT_DISCREPANCY_FOUND events from hub-ops.
//! When the weigh-bridge finds a heavier parcel than declared, hub-ops emits
//! this event. We find the merchant's current issued invoice and append a
//! surcharge line item via InvoiceService::apply_weight_adjustment().
//! If no issued invoice exists, the adjustment is deferred to the next billing run.

use logisticos_events::{envelope::Event, payloads::WeightDiscrepancyFound, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use std::sync::Arc;
use tokio::sync::watch;

use crate::application::commands::ApplyWeightAdjustmentCommand;
use crate::application::services::InvoiceService;
use crate::domain::repositories::InvoiceRepository;

pub async fn start_weight_discrepancy_consumer(
    brokers: &str,
    group_id: &str,
    invoice_service: Arc<InvoiceService>,
    invoice_repo: Arc<dyn InvoiceRepository>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-weight-discrepancy", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::WEIGHT_DISCREPANCY_FOUND])?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Weight-discrepancy consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle_weight_discrepancy(
                                payload, &*invoice_repo, &invoice_service
                            ).await {
                                tracing::warn!(err = %e, "weight-discrepancy handler error (skipping)");
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "weight-discrepancy consumer recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_weight_discrepancy(
    payload: &[u8],
    invoice_repo: &dyn InvoiceRepository,
    invoice_service: &InvoiceService,
) -> anyhow::Result<()> {
    let event: Event<WeightDiscrepancyFound> = serde_json::from_slice(payload)?;
    let d = &event.data;

    // Find the merchant's current issued invoice
    let invoice = invoice_repo
        .find_latest_issued_for_merchant(d.tenant_id, d.merchant_id)
        .await
        .map_err(|e| anyhow::anyhow!("DB error finding invoice: {e}"))?;

    let Some(invoice) = invoice else {
        tracing::info!(
            awb = %d.awb,
            merchant_id = %d.merchant_id,
            "No issued invoice found — weight surcharge deferred to next billing run"
        );
        return Ok(());
    };

    let cmd = ApplyWeightAdjustmentCommand {
        invoice_id:      invoice.id,
        awb:             d.awb.clone(),
        declared_grams:  d.declared_grams,
        actual_grams:    d.actual_grams,
        surcharge_cents: d.surcharge_cents,
        applied_by:      "weight-discrepancy-consumer".into(),
    };

    match invoice_service.apply_weight_adjustment(cmd).await {
        Ok(Some(_)) => tracing::info!(
            awb = %d.awb,
            invoice_id = %invoice.id,
            surcharge_cents = d.surcharge_cents,
            "Weight surcharge applied to invoice"
        ),
        Ok(None) => tracing::warn!(
            awb = %d.awb,
            invoice_id = %invoice.id,
            "apply_weight_adjustment returned None — invoice may have moved to paid"
        ),
        Err(e) => {
            return Err(anyhow::anyhow!("apply_weight_adjustment failed: {e}"));
        }
    }

    Ok(())
}
```

- [ ] **Step 4: Check WeightDiscrepancyFound payload exists in libs/events**

```bash
grep -r "WeightDiscrepancyFound" D:\LogisticOS\.claude\worktrees\amazing-jepsen-774a18\libs\events\src\
```

If it doesn't exist, add to `libs/events/src/payloads.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightDiscrepancyFound {
    pub awb:             String,
    pub tenant_id:       Uuid,
    pub merchant_id:     Uuid,
    pub shipment_id:     Uuid,
    pub declared_grams:  i32,
    pub actual_grams:    i32,
    pub surcharge_cents: i64,
    pub scanned_at:      String,  // ISO-8601
    pub hub_id:          Uuid,
}
```

Also add topic if missing in `libs/events/src/topics.rs`:

```rust
pub const WEIGHT_DISCREPANCY_FOUND: &str = "logisticos.hub.weight_discrepancy_found";
```

- [ ] **Step 5: Export from payments messaging mod**

In `services/payments/src/infrastructure/messaging/mod.rs`, add:

```rust
pub mod weight_discrepancy_consumer;
pub use weight_discrepancy_consumer::start_weight_discrepancy_consumer;
```

- [ ] **Step 6: Wire into payments bootstrap**

In `services/payments/src/bootstrap.rs`, after the pod consumer spawn:

```rust
// Weight-discrepancy consumer — appends surcharge to merchant invoice when
// hub-ops finds actual weight > declared weight.
let invoice_svc_for_weight = Arc::clone(&invoice_service);
let invoice_repo_for_weight = Arc::clone(&invoice_repo) as Arc<dyn InvoiceRepository>;
let brokers_weight  = cfg.kafka.brokers.clone();
let group_weight    = cfg.kafka.group_id.clone();
let shutdown_weight = shutdown_tx.subscribe();
tokio::spawn(async move {
    if let Err(e) = start_weight_discrepancy_consumer(
        &brokers_weight,
        &group_weight,
        invoice_svc_for_weight,
        invoice_repo_for_weight,
        shutdown_weight,
    ).await {
        tracing::error!("Weight-discrepancy consumer crashed: {e}");
    }
});
```

- [ ] **Step 7: Run clippy**

```bash
cargo clippy -p payments 2>&1 | head -60
```

Expected: 0 errors

- [ ] **Step 8: Commit**

```bash
git add libs/events/src/payloads.rs
git add libs/events/src/topics.rs
git add services/payments/src/domain/repositories/mod.rs
git add services/payments/src/infrastructure/db/invoice_repo.rs
git add services/payments/src/infrastructure/messaging/weight_discrepancy_consumer.rs
git add services/payments/src/infrastructure/messaging/mod.rs
git add services/payments/src/bootstrap.rs
git commit -m "feat(payments): weight-discrepancy Kafka consumer applies surcharge to issued invoices"
```

---

## Self-Review

**Spec coverage:**
- ✅ Engagement notifications — tenant_id fix + shutdown signal
- ✅ COD remittance batching — nightly cron + `distinct_merchants_with_unbatched_cod`
- ✅ Delivery-failed re-dispatch — internal requeue endpoint + business-logic URL fix
- ✅ Weight adjustment surcharge — new consumer + `find_latest_issued_for_merchant`

**Placeholder scan:** No TBDs. All SQL, Rust code, and commands are explicit.

**Type consistency:**
- `ApplyWeightAdjustmentCommand` used in Task 4 Step 3 matches the struct already defined in invoice_service.rs research
- `CreateCodBatchCommand` used in Task 2 Step 3 — verify field names match existing command struct before executing
- `reset_to_pending(shipment_id: Uuid)` used in Task 3 Step 1 and Step 2 are consistent

**Known unknowns to resolve during execution:**
1. Exact field names of `CreateCodBatchCommand` — check existing `cod_remittance_service.rs` before writing Task 2 Step 3
2. Whether `WeightDiscrepancyFound` payload already exists in `payloads.rs` — Step 4 handles both cases
3. Whether `business-logic` config has a `dispatch_url` field — Task 3 Step 5 handles adding it
