# Order → Dispatch → POD Flow — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire six existing Rust services into a runnable end-to-end flow: merchant creates shipment → admin dispatches to driver → driver completes with POD → customer sees delivered status on tracking page.

**Architecture:** Services communicate exclusively via Kafka events. Each service already has domain logic and HTTP handlers; what's missing is the event wiring (consumers + enriched payloads), two new dispatch tables, seed data, and portal API wiring. No new Docker infra is required — `docker-compose.yml` is already complete.

**Tech Stack:** Rust (Axum, SQLx, rdkafka), PostgreSQL (schemas per service), Redis, Kafka, React Native (Expo), Next.js 14

**Spec:** `docs/superpowers/specs/2026-03-26-order-dispatch-pod-flow-design.md`

---

## Service Port Map (local `cargo run`)

> **Canonical source:** These ports are from the actual service configs in `docker-compose.yml` and each service's `config.rs`. They differ from the spec's port table (which listed sequential 8000–8005) — the implementation uses these ports. All curl commands in this plan use these ports.

| Service | Port | Cargo package |
|---------|------|---------------|
| identity | 8001 | `logisticos-identity` |
| order-intake | 8004 | `logisticos-order-intake` |
| dispatch | 8005 | `logisticos-dispatch` |
| driver-ops | 8006 | `logisticos-driver-ops` |
| delivery-experience | 8007 | `logisticos-delivery-experience` |
| pod | 8011 | `logisticos-pod` |

## File Structure

**Create:**
- `services/identity/migrations/0004_seed_dev_data.sql`
- `services/order-intake/migrations/0003_add_customer_fields.sql`
- `services/order-intake/migrations/0004_seed_dev_data.sql`
- `services/order-intake/src/infrastructure/messaging/status_consumer.rs`
- `services/dispatch/migrations/0003_dispatch_queue.sql`
- `services/dispatch/migrations/0004_driver_profiles.sql`
- `services/dispatch/src/infrastructure/db/dispatch_queue_repo.rs`
- `services/dispatch/src/infrastructure/db/driver_profiles_repo.rs`
- `services/dispatch/src/infrastructure/messaging/shipment_consumer.rs`
- `services/dispatch/src/infrastructure/messaging/user_consumer.rs`
- `services/driver-ops/src/infrastructure/messaging/task_consumer.rs`
- Each service: `services/<name>/.env` (local dev config)

**Modify:**
- `scripts/db/init.sql` — add `tracking` schema
- `libs/events/src/topics.rs` — add `USER_CREATED`, `TASK_ASSIGNED`
- `libs/events/src/payloads.rs` — add `UserCreated`, `TaskAssigned`; enrich `ShipmentCreated`
- `services/order-intake/src/domain/entities/shipment.rs` — add `customer_name`, `customer_phone`
- `services/order-intake/src/application/services/shipment_service.rs` — emit enriched event + update Shipment builder
- `services/order-intake/src/bootstrap.rs` — wire status consumer
- `services/identity/src/application/services/tenant_service.rs` — emit `USER_CREATED` on invite
- `services/dispatch/src/infrastructure/db/mod.rs` — export new repos
- `services/dispatch/src/application/services/driver_assignment_service.rs` — add `quick_dispatch()`
- `services/dispatch/src/api/http/mod.rs` — add `POST /v1/queue/:id/dispatch`, `GET /v1/queue`, `GET /v1/drivers`
- `services/dispatch/src/bootstrap.rs` — wire two new consumers + new repos
- `services/driver-ops/src/infrastructure/messaging/mod.rs` — replace stub
- `services/driver-ops/src/bootstrap.rs` — wire task consumer
- `services/pod/src/application/services/pod_service.rs` — add `get_by_id()`
- `services/pod/src/api/http/pod.rs` — fix `get_pod` stub
- `apps/driver-app/src/app/(auth)/login.tsx` — new login screen
- `apps/driver-app/src/lib/api-client.ts` — HTTP client for driver app
- `apps/admin-portal/...` — wire dispatch console
- `apps/merchant-portal/...` — wire shipment creation
- `apps/customer-portal/...` — wire tracking page

---

## Task 1: Dev Environment Bootstrap

**Files:**
- Create: `services/identity/.env`
- Create: `services/order-intake/.env`
- Create: `services/dispatch/.env`
- Create: `services/driver-ops/.env`
- Create: `services/delivery-experience/.env`
- Create: `services/pod/.env`
- Modify: `scripts/db/init.sql`

- [ ] **Step 1: Add `tracking` schema to init.sql**

Open `scripts/db/init.sql` and add after the existing schema list (around line 24):

```sql
CREATE SCHEMA IF NOT EXISTS tracking;
```

- [ ] **Step 2: Create `.env` files for each service**

`services/identity/.env`:
```
APP__HOST=127.0.0.1
APP__PORT=8001
APP__ENV=development
DATABASE__URL=postgres://logisticos:password@localhost:5432/logisticos
DATABASE__MAX_CONNECTIONS=5
REDIS__URL=redis://localhost:6379
KAFKA__BROKERS=localhost:9092
KAFKA__GROUP_ID=identity-dev
AUTH__JWT_SECRET=dev-jwt-secret-CHANGE-IN-PRODUCTION-minimum-32-chars
RUST_LOG=info,sqlx=warn
```

`services/order-intake/.env`:
```
APP__HOST=127.0.0.1
APP__PORT=8004
APP__ENV=development
DATABASE__URL=postgres://logisticos:password@localhost:5432/logisticos
DATABASE__MAX_CONNECTIONS=5
REDIS__URL=redis://localhost:6379
KAFKA__BROKERS=localhost:9092
KAFKA__GROUP_ID=order-intake-dev
AUTH__JWT_SECRET=dev-jwt-secret-CHANGE-IN-PRODUCTION-minimum-32-chars
RUST_LOG=info,sqlx=warn
```

`services/dispatch/.env`:
```
APP__HOST=127.0.0.1
APP__PORT=8005
APP__ENV=development
DATABASE__URL=postgres://logisticos:password@localhost:5432/logisticos
DATABASE__MAX_CONNECTIONS=5
REDIS__URL=redis://localhost:6379
KAFKA__BROKERS=localhost:9092
KAFKA__GROUP_ID=dispatch-dev
AUTH__JWT_SECRET=dev-jwt-secret-CHANGE-IN-PRODUCTION-minimum-32-chars
RUST_LOG=info,sqlx=warn
```

`services/driver-ops/.env`:
```
APP__HOST=127.0.0.1
APP__PORT=8006
APP__ENV=development
DATABASE__URL=postgres://logisticos:password@localhost:5432/logisticos
DATABASE__MAX_CONNECTIONS=5
REDIS__URL=redis://localhost:6379
KAFKA__BROKERS=localhost:9092
KAFKA__GROUP_ID=driver-ops-dev
AUTH__JWT_SECRET=dev-jwt-secret-CHANGE-IN-PRODUCTION-minimum-32-chars
RUST_LOG=info,sqlx=warn
```

`services/delivery-experience/.env`:
```
APP__HOST=127.0.0.1
APP__PORT=8007
APP__ENV=development
DATABASE__URL=postgres://logisticos:password@localhost:5432/logisticos
DATABASE__MAX_CONNECTIONS=5
REDIS__URL=redis://localhost:6379
KAFKA__BROKERS=localhost:9092
KAFKA__GROUP_ID=delivery-experience-dev
AUTH__JWT_SECRET=dev-jwt-secret-CHANGE-IN-PRODUCTION-minimum-32-chars
RUST_LOG=info,sqlx=warn
```

`services/pod/.env`:
```
APP__HOST=127.0.0.1
APP__PORT=8011
APP__ENV=development
DATABASE__URL=postgres://logisticos:password@localhost:5432/logisticos
DATABASE__MAX_CONNECTIONS=5
REDIS__URL=redis://localhost:6379
KAFKA__BROKERS=localhost:9092
KAFKA__GROUP_ID=pod-dev
AUTH__JWT_SECRET=dev-jwt-secret-CHANGE-IN-PRODUCTION-minimum-32-chars
S3__ENDPOINT=http://localhost:9001
S3__BUCKET=pod-evidence
S3__ACCESS_KEY=minioadmin
S3__SECRET_KEY=minioadmin
RUST_LOG=info,sqlx=warn
```

- [ ] **Step 3: Start infrastructure**

> **Note:** `minio` is already defined in `docker-compose.yml` (pre-configured with S3 credentials matching the pod `.env` above). No changes to the compose file are needed.

```bash
cd d:/LogisticOS
docker compose up -d postgres redis kafka zookeeper minio
```

Expected: all containers report `healthy` within 30s:
```bash
docker compose ps
```
All should show `healthy` or `running`.

- [ ] **Step 4: Verify workspace compiles**

```bash
cd d:/LogisticOS
cargo check --workspace
```

Expected: `Finished` with 0 errors. Any warnings about unused imports are acceptable.

- [ ] **Step 5: Commit**

```bash
git add scripts/db/init.sql services/identity/.env services/order-intake/.env services/dispatch/.env services/driver-ops/.env services/delivery-experience/.env services/pod/.env
git commit -m "chore: add dev env files and tracking schema to init.sql

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 2: Kafka Topics + Event Payload Additions

**Files:**
- Modify: `libs/events/src/topics.rs`
- Modify: `libs/events/src/payloads.rs`

- [ ] **Step 0: Verify existing topic constants**

Before making any changes, confirm that `DRIVER_ASSIGNED`, `DELIVERY_COMPLETED`, and `DELIVERY_FAILED` already exist (they are used by the order-intake status consumer in Task 3):

```bash
grep -E "DRIVER_ASSIGNED|DELIVERY_COMPLETED|DELIVERY_FAILED" libs/events/src/topics.rs
```

Expected: 3 lines showing `pub const DRIVER_ASSIGNED`, `pub const DELIVERY_COMPLETED`, `pub const DELIVERY_FAILED`. If any are missing, add them alongside the new constants in Step 3.

- [ ] **Step 1: Write unit test for new topic constants**

Add at the bottom of `libs/events/src/topics.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_topics_are_lowercase_dot_separated() {
        let topics = [USER_CREATED, TASK_ASSIGNED];
        for t in topics {
            assert!(t.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '_'),
                "Topic '{}' has invalid chars", t);
            assert!(t.starts_with("logisticos."), "Topic '{}' must start with logisticos.", t);
        }
    }
}
```

- [ ] **Step 2: Run test, confirm it fails**

```bash
cargo test -p logisticos-events
```

Expected: FAIL — `USER_CREATED` and `TASK_ASSIGNED` are not defined yet.

- [ ] **Step 3: Add new topic constants**

In `libs/events/src/topics.rs`, add after the existing identity section:

```rust
pub const USER_CREATED:  &str = "logisticos.identity.user.created";
pub const TASK_ASSIGNED: &str = "logisticos.task.assigned";
```

- [ ] **Step 4: Add new payload structs**

In `libs/events/src/payloads.rs`, add to the existing `ShipmentCreated` struct (enrich it) and add two new structs:

```rust
// Enriched ShipmentCreated — add customer details for dispatch_queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipmentCreated {
    pub shipment_id:          Uuid,
    pub merchant_id:          Uuid,
    pub customer_id:          Uuid,
    pub customer_name:        String,
    pub customer_phone:       String,
    pub origin_address:       String,
    pub destination_address:  String,
    pub destination_city:     String,
    pub destination_lat:      Option<f64>,
    pub destination_lng:      Option<f64>,
    pub service_type:         String,
    pub cod_amount_cents:     Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCreated {
    pub user_id:   Uuid,
    pub tenant_id: Uuid,
    pub email:     String,
    pub roles:     Vec<String>,
}

/// Emitted by dispatch when a shipment is assigned to a driver.
/// Contains all data driver-ops needs to create a DriverTask row
/// without querying other services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssigned {
    pub task_id:              Uuid,   // Pre-generated UUID for the task
    pub assignment_id:        Uuid,
    pub shipment_id:          Uuid,
    pub route_id:             Uuid,
    pub driver_id:            Uuid,
    pub tenant_id:            Uuid,
    pub sequence:             u32,
    // Destination (denormalized from dispatch_queue for offline driver app)
    pub address_line1:        String,
    pub address_city:         String,
    pub address_province:     String,
    pub address_postal_code:  String,
    pub address_lat:          Option<f64>,
    pub address_lng:          Option<f64>,
    // Customer (denormalized for driver app display)
    pub customer_name:        String,
    pub customer_phone:       String,
    pub cod_amount_cents:     Option<i64>,
    pub special_instructions: Option<String>,
}
```

> **Note:** The `ShipmentCreated` struct is **replaced** (not added to). Update the existing struct definition.

- [ ] **Step 5: Run tests, confirm they pass**

```bash
cargo test -p logisticos-events
```

Expected: all tests PASS.

- [ ] **Step 6: Verify workspace still compiles**

```bash
cargo check --workspace
```

If any services reference the old `ShipmentCreated` struct fields, fix them (primarily `delivery-experience` which deserialises this payload inline — its handler uses a local struct so no change needed there).

- [ ] **Step 7: Commit**

```bash
git add libs/events/src/topics.rs libs/events/src/payloads.rs
git commit -m "feat(events): add USER_CREATED, TASK_ASSIGNED topics and enriched ShipmentCreated payload

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 3: Order-Intake — Customer Fields + Status Consumer

**Files:**
- Create: `services/order-intake/migrations/0003_add_customer_fields.sql`
- Modify: `services/order-intake/src/domain/entities/shipment.rs`
- Modify: `services/order-intake/src/application/services/shipment_service.rs`
- Create: `services/order-intake/src/infrastructure/messaging/status_consumer.rs`
- Modify: `services/order-intake/src/bootstrap.rs`

- [ ] **Step 1: Write a unit test for the Shipment struct**

Add to `services/order-intake/src/domain/entities/shipment.rs` (at the bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipment_has_customer_fields() {
        // Ensure customer_name and customer_phone are accessible
        let s = Shipment {
            id: logisticos_types::ShipmentId::new(),
            tenant_id: logisticos_types::TenantId::from_uuid(uuid::Uuid::new_v4()),
            merchant_id: logisticos_types::MerchantId::from_uuid(uuid::Uuid::new_v4()),
            customer_id: logisticos_types::CustomerId::new(),
            customer_name: "Test Customer".to_string(),
            customer_phone: "+63912345678".to_string(),
            tracking_number: "LS-TEST".to_string(),
            status: logisticos_types::ShipmentStatus::Pending,
            service_type: crate::domain::value_objects::ServiceType::Standard,
            origin: logisticos_types::Address::default(),
            destination: logisticos_types::Address::default(),
            weight: crate::domain::value_objects::ShipmentWeight::from_grams(1000),
            dimensions: None,
            declared_value: None,
            cod_amount: None,
            special_instructions: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        assert_eq!(s.customer_name, "Test Customer");
        assert_eq!(s.customer_phone, "+63912345678");
    }
}
```

- [ ] **Step 2: Run test, confirm it fails**

```bash
cargo test -p logisticos-order-intake
```

Expected: FAIL — `customer_name` and `customer_phone` fields don't exist on `Shipment`.

- [ ] **Step 3: Add customer fields to Shipment entity**

In `services/order-intake/src/domain/entities/shipment.rs`, add after `customer_id`:

```rust
pub customer_name:  String,
pub customer_phone: String,
```

- [ ] **Step 4: Update shipment builder in shipment_service.rs**

In the `create()` method where `Shipment { ... }` is constructed, add:

```rust
customer_name:  cmd.customer_name.clone(),
customer_phone: cmd.customer_phone.clone(),
```

Also update the `ShipmentCreated` event emission to include the new fields:

```rust
ShipmentCreated {
    shipment_id:         shipment.id.inner(),
    merchant_id:         shipment.merchant_id.inner(),
    customer_id:         shipment.customer_id.inner(),
    customer_name:       shipment.customer_name.clone(),
    customer_phone:      shipment.customer_phone.clone(),
    origin_address:      format!("{}, {}", shipment.origin.line1, shipment.origin.city),
    destination_address: format!("{}, {}", shipment.destination.line1, shipment.destination.city),
    destination_city:    shipment.destination.city.clone(),
    destination_lat:     shipment.destination.coordinates.map(|c| c.lat),
    destination_lng:     shipment.destination.coordinates.map(|c| c.lng),
    service_type:        format!("{:?}", shipment.service_type).to_lowercase(),
    cod_amount_cents:    shipment.cod_amount.as_ref().map(|m| m.amount),
}
```

- [ ] **Step 5: Create migration for new columns**

`services/order-intake/migrations/0003_add_customer_fields.sql`:

```sql
-- Add customer contact fields to shipments for dispatch denormalization
ALTER TABLE order_intake.shipments
    ADD COLUMN IF NOT EXISTS customer_name  TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS customer_phone TEXT NOT NULL DEFAULT '';

-- Remove the defaults after backfill (new inserts will always provide them)
ALTER TABLE order_intake.shipments
    ALTER COLUMN customer_name  DROP DEFAULT,
    ALTER COLUMN customer_phone DROP DEFAULT;
```

- [ ] **Step 6: Create status consumer**

> **Existing topic constants:** `topics::DRIVER_ASSIGNED`, `topics::DELIVERY_COMPLETED`, and `topics::DELIVERY_FAILED` already exist in `libs/events/src/topics.rs` — Task 2 only adds `USER_CREATED` and `TASK_ASSIGNED`. No changes needed for these three.
>
> **ShipmentStatus values:** Before writing the SQL `UPDATE` statements, check `services/order-intake/src/domain/entities/shipment.rs` for the valid `ShipmentStatus` enum variants and what string literals they map to in the DB. The SQL below uses `'pickup_assigned'`, `'delivered'`, and `'failed'` — update these if the actual schema uses different strings (e.g., `'assigned'`, `'out_for_delivery'`).

`services/order-intake/src/infrastructure/messaging/status_consumer.rs`:

```rust
//! Kafka consumer that updates canonical shipment status when downstream
//! services report progress (driver assigned, delivered, failed).
//!
//! All messages are wrapped in EventEnvelope<T> by KafkaProducer — must unwrap before payload.

use rdkafka::{consumer::{CommitMode, Consumer, StreamConsumer}, Message};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;
use logisticos_events::{envelope::EventEnvelope, topics};

#[derive(Deserialize)]
struct DriverAssignedEvt { shipment_id: Uuid }

#[derive(Deserialize)]
struct DeliveryCompletedEvt { shipment_id: Uuid }

#[derive(Deserialize)]
struct DeliveryFailedEvt { shipment_id: Uuid }

pub async fn start_status_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
) -> anyhow::Result<()> {
    use rdkafka::config::ClientConfig;
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-status", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[
        topics::DRIVER_ASSIGNED,
        topics::DELIVERY_COMPLETED,
        topics::DELIVERY_FAILED,
    ])?;

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                let payload = match msg.payload() { Some(p) => p, None => { consumer.commit_message(&msg, CommitMode::Async).ok(); continue; } };
                let topic = msg.topic();
                let result = handle(&pool, topic, payload).await;
                if let Err(e) = result {
                    tracing::warn!(topic, err = %e, "status consumer: handler error (skipping)");
                }
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "status consumer: recv error");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle(pool: &PgPool, topic: &str, payload: &[u8]) -> anyhow::Result<()> {
    // All events are published via KafkaProducer which wraps them in EventEnvelope<T>.
    // Deserialize the envelope first, then extract the payload — same pattern as all other consumers.
    match topic {
        topics::DRIVER_ASSIGNED => {
            let envelope: EventEnvelope<DriverAssignedEvt> = serde_json::from_slice(payload)?;
            let evt = envelope.payload;
            sqlx::query!(
                "UPDATE order_intake.shipments SET status = 'pickup_assigned', updated_at = NOW()
                 WHERE id = $1 AND status NOT IN ('delivered','cancelled','returned')",
                evt.shipment_id
            ).execute(pool).await?;
        }
        topics::DELIVERY_COMPLETED => {
            let envelope: EventEnvelope<DeliveryCompletedEvt> = serde_json::from_slice(payload)?;
            let evt = envelope.payload;
            sqlx::query!(
                "UPDATE order_intake.shipments SET status = 'delivered', updated_at = NOW()
                 WHERE id = $1",
                evt.shipment_id
            ).execute(pool).await?;
        }
        topics::DELIVERY_FAILED => {
            let envelope: EventEnvelope<DeliveryFailedEvt> = serde_json::from_slice(payload)?;
            let evt = envelope.payload;
            sqlx::query!(
                "UPDATE order_intake.shipments SET status = 'failed', updated_at = NOW()
                 WHERE id = $1 AND status NOT IN ('delivered','cancelled')",
                evt.shipment_id
            ).execute(pool).await?;
        }
        _ => {}
    }
    Ok(())
}
```

> **Envelope struct name:** This consumer (and all consumers in Tasks 5, 6, 7) use `EventEnvelope<T>`. Verify the exact struct name in `libs/events/src/envelope.rs` before running (Task 9 Step 1 covers this check). If the struct is named differently (e.g., `Event<T>`), replace `EventEnvelope` throughout all consumer files.

- [ ] **Step 7: Wire status consumer in bootstrap.rs**

In `services/order-intake/src/bootstrap.rs`, after creating `pool`:

```rust
// Add import at top
use crate::infrastructure::messaging::status_consumer::start_status_consumer;

// After creating the pool and publisher, spawn the status consumer:
let pool_for_consumer = pool.clone();
let brokers_for_consumer = cfg.kafka.brokers.clone();
let group_for_consumer = cfg.kafka.group_id.clone();
tokio::spawn(async move {
    if let Err(e) = start_status_consumer(
        &brokers_for_consumer,
        &group_for_consumer,
        pool_for_consumer,
    ).await {
        tracing::error!("Status consumer error: {e}");
    }
});
```

Also add `pub mod status_consumer;` to `services/order-intake/src/infrastructure/messaging/mod.rs`.

- [ ] **Step 8: Run tests**

```bash
cargo test -p logisticos-order-intake
```

Expected: all PASS (including the new customer_fields test).

- [ ] **Step 9: Commit**

```bash
git add services/order-intake/
git commit -m "feat(order-intake): add customer fields to shipment; wire status update consumer

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 4: Identity — Seed Data + USER_CREATED Event

**Files:**
- Create: `services/identity/migrations/0004_seed_dev_data.sql`
- Modify: `services/identity/src/application/services/tenant_service.rs`

**Fixed UUIDs for seed data** (canonical for this plan — supersedes spec's UUID table):
```
Tenant:   00000000-0000-0000-0000-000000000001
Admin:    00000000-0000-0000-0000-000000000002
Merchant: 00000000-0000-0000-0000-000000000003
Driver:   00000000-0000-0000-0000-000000000004
Customer: 00000000-0000-0000-0000-000000000005
```

- [ ] **Step 1: Generate real Argon2id hash for seed users**

> **Do this first** — the migration SQL needs a real hash before it's written or run. Never commit placeholder hashes; they will cause silent login failures.

Add a temporary test to `services/identity/src/lib.rs` (or any test file):

```rust
#[cfg(test)]
mod seed_hash_gen {
    #[test]
    fn print_seed_hash() {
        let hash = logisticos_auth::password::hash_password("LogisticOS1!")
            .expect("hash failed");
        println!("SEED HASH: {}", hash);
    }
}
```

Run:
```bash
cargo test -p logisticos-identity seed_hash_gen -- --nocapture
```

Copy the printed hash (looks like `$argon2id$v=19$m=...`). You'll use it in Step 2. Remove the temporary test after copying.

- [ ] **Step 2: Create seed migration with real hashes**

`services/identity/migrations/0004_seed_dev_data.sql`:

Replace `<ARGON2ID_HASH_FROM_STEP_1>` with the actual hash from Step 1 (same hash for all three users — the password `"LogisticOS1!"` is identical for all seed users).

```sql
-- Dev seed data — fixed UUIDs for reproducible testing.
-- All passwords: "LogisticOS1!" hashed with Argon2id via logisticos_auth::password::hash_password.
-- Hash generated in Step 1 above.

DO $$
BEGIN
  -- Tenant
  INSERT INTO identity.tenants (id, name, slug, subscription_tier, is_active)
  VALUES (
    '00000000-0000-0000-0000-000000000001',
    'Demo Logistics Co',
    'demo',
    'business',
    true
  ) ON CONFLICT DO NOTHING;

  -- Admin user (tenant_admin role, email_verified = true so can_login() passes)
  INSERT INTO identity.users (id, tenant_id, email, password_hash, first_name, last_name, roles, email_verified, is_active)
  VALUES (
    '00000000-0000-0000-0000-000000000002',
    '00000000-0000-0000-0000-000000000001',
    'admin@demo.com',
    '<ARGON2ID_HASH_FROM_STEP_1>',
    'Admin',
    'User',
    ARRAY['tenant_admin'],
    true,
    true
  ) ON CONFLICT DO NOTHING;

  -- Driver user (driver role, email_verified = true)
  INSERT INTO identity.users (id, tenant_id, email, password_hash, first_name, last_name, roles, email_verified, is_active)
  VALUES (
    '00000000-0000-0000-0000-000000000004',
    '00000000-0000-0000-0000-000000000001',
    'driver@demo.com',
    '<ARGON2ID_HASH_FROM_STEP_1>',
    'Ahmed',
    'Al-Rashid',
    ARRAY['driver'],
    true,
    true
  ) ON CONFLICT DO NOTHING;

  -- Merchant user
  INSERT INTO identity.users (id, tenant_id, email, password_hash, first_name, last_name, roles, email_verified, is_active)
  VALUES (
    '00000000-0000-0000-0000-000000000003',
    '00000000-0000-0000-0000-000000000001',
    'merchant@demo.com',
    '<ARGON2ID_HASH_FROM_STEP_1>',
    'Sarah',
    'Merchant',
    ARRAY['merchant'],
    true,
    true
  ) ON CONFLICT DO NOTHING;
END $$;
```

- [ ] **Step 3: Add USER_CREATED emission to invite_user**

In `services/identity/src/application/services/tenant_service.rs`, after `self.user_repo.save(&user).await`:

```rust
// Emit USER_CREATED so dispatch service can populate driver_profiles cache
use logisticos_events::{payloads::UserCreated, topics, envelope::Event};
let event = Event::new(
    "identity",
    "user.created",
    tenant_id.inner(),
    UserCreated {
        user_id:   user.id.inner(),
        tenant_id: tenant_id.inner(),
        email:     user.email.clone(),
        roles:     user.roles.clone(),
    },
);
self.kafka.publish_event(topics::USER_CREATED, &event).await
    .map_err(AppError::Internal)?;
tracing::info!(user_id = %user.id, roles = ?user.roles, "User created, USER_CREATED event emitted");
```

> The `UserCreated` payload and `USER_CREATED` topic were added in Task 2.

- [ ] **Step 4: Start identity service and smoke test login**

```bash
cargo run -p logisticos-identity
```

In another terminal:
```bash
# Create tenant and admin via the API (bypasses seed data for auth test)
curl -s -X POST http://localhost:8001/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Demo Logistics Co",
    "slug": "demo-test",
    "owner_email": "admin2@demo.com",
    "owner_password": "LogisticOS1!",
    "owner_first_name": "Admin",
    "owner_last_name": "Test"
  }' | jq .
```

Expected: `{"data": {"tenant_id": "...", "slug": "demo-test"}}` with HTTP 201.

```bash
# Login with the created admin
curl -s -X POST http://localhost:8001/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "admin2@demo.com", "password": "LogisticOS1!", "tenant_slug": "demo-test"}' | jq .
```

Expected: `{"access_token": "eyJ...", "refresh_token": "...", "expires_in": 3600}`.

> **Note on seed migration:** The seed migration uses raw Argon2id hashes; if the login with seed users (admin@demo.com) fails with wrong password, re-run step 2 to get the correct hash, update the migration, and drop/recreate the DB (or delete the sqlx migrations table row for 0004).

- [ ] **Step 5: Commit**

```bash
git add services/identity/
git commit -m "feat(identity): seed dev users; emit USER_CREATED event on invite

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 5: Dispatch — dispatch_queue + driver_profiles Tables + Consumers

**Files:**
- Create: `services/dispatch/migrations/0003_dispatch_queue.sql`
- Create: `services/dispatch/migrations/0004_driver_profiles.sql`
- Create: `services/dispatch/src/infrastructure/db/dispatch_queue_repo.rs`
- Create: `services/dispatch/src/infrastructure/db/driver_profiles_repo.rs`
- Create: `services/dispatch/src/infrastructure/messaging/shipment_consumer.rs`
- Create: `services/dispatch/src/infrastructure/messaging/user_consumer.rs`
- Modify: `services/dispatch/src/infrastructure/db/mod.rs`
- Modify: `services/dispatch/src/infrastructure/messaging/mod.rs` (create if doesn't exist as module)
- Modify: `services/dispatch/src/bootstrap.rs`

- [ ] **Step 1: Create dispatch_queue migration**

`services/dispatch/migrations/0003_dispatch_queue.sql`:

```sql
-- dispatch_queue: shipments awaiting driver assignment.
-- Populated by consuming SHIPMENT_CREATED events from order-intake.
CREATE TABLE IF NOT EXISTS dispatch.dispatch_queue (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    shipment_id         UUID        NOT NULL UNIQUE,
    -- Customer info (denormalized from SHIPMENT_CREATED event)
    customer_name       TEXT        NOT NULL,
    customer_phone      TEXT        NOT NULL,
    -- Destination
    dest_address_line1  TEXT        NOT NULL,
    dest_city           TEXT        NOT NULL,
    dest_province       TEXT        NOT NULL DEFAULT '',
    dest_postal_code    TEXT        NOT NULL DEFAULT '',
    dest_lat            DOUBLE PRECISION,
    dest_lng            DOUBLE PRECISION,
    -- Parcel
    cod_amount_cents    BIGINT,
    special_instructions TEXT,
    service_type        TEXT        NOT NULL DEFAULT 'standard',
    -- Queue state
    status              TEXT        NOT NULL DEFAULT 'pending'
                                    CHECK (status IN ('pending','dispatched','cancelled')),
    queued_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    dispatched_at       TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_dispatch_queue_tenant_status
    ON dispatch.dispatch_queue (tenant_id, status, queued_at);
```

- [ ] **Step 2: Create driver_profiles migration**

`services/dispatch/migrations/0004_driver_profiles.sql`:

```sql
-- driver_profiles: local cache of driver identities in the dispatch service.
-- Populated by consuming USER_CREATED events from identity service
-- where role contains 'driver'.
CREATE TABLE IF NOT EXISTS dispatch.driver_profiles (
    id          UUID        PRIMARY KEY,  -- Same UUID as identity.users.id
    tenant_id   UUID        NOT NULL,
    email       TEXT        NOT NULL,
    first_name  TEXT        NOT NULL DEFAULT '',
    last_name   TEXT        NOT NULL DEFAULT '',
    is_active   BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_driver_profiles_tenant
    ON dispatch.driver_profiles (tenant_id, is_active);
```

- [ ] **Step 3: Create dispatch_queue_repo.rs**

`services/dispatch/src/infrastructure/db/dispatch_queue_repo.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DispatchQueueRow {
    pub id:                  Uuid,
    pub tenant_id:           Uuid,
    pub shipment_id:         Uuid,
    pub customer_name:       String,
    pub customer_phone:      String,
    pub dest_address_line1:  String,
    pub dest_city:           String,
    pub dest_province:       String,
    pub dest_postal_code:    String,
    pub dest_lat:            Option<f64>,
    pub dest_lng:            Option<f64>,
    pub cod_amount_cents:    Option<i64>,
    pub special_instructions: Option<String>,
    pub service_type:        String,
    pub status:              String,
}

pub struct PgDispatchQueueRepository {
    pool: PgPool,
}

impl PgDispatchQueueRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn upsert(&self, row: &DispatchQueueRow) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO dispatch.dispatch_queue (
                id, tenant_id, shipment_id,
                customer_name, customer_phone,
                dest_address_line1, dest_city, dest_province, dest_postal_code,
                dest_lat, dest_lng,
                cod_amount_cents, special_instructions, service_type, status
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
            ON CONFLICT (shipment_id) DO NOTHING
            "#,
            row.id, row.tenant_id, row.shipment_id,
            row.customer_name, row.customer_phone,
            row.dest_address_line1, row.dest_city, row.dest_province, row.dest_postal_code,
            row.dest_lat, row.dest_lng,
            row.cod_amount_cents, row.special_instructions, row.service_type, row.status,
        ).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<DispatchQueueRow>> {
        let row = sqlx::query_as!(DispatchQueueRow,
            "SELECT id, tenant_id, shipment_id, customer_name, customer_phone,
                    dest_address_line1, dest_city, dest_province, dest_postal_code,
                    dest_lat, dest_lng, cod_amount_cents, special_instructions, service_type, status
             FROM dispatch.dispatch_queue WHERE shipment_id = $1",
            shipment_id
        ).fetch_optional(&self.pool).await?;
        Ok(row)
    }

    pub async fn list_pending(&self, tenant_id: Uuid) -> anyhow::Result<Vec<DispatchQueueRow>> {
        let rows = sqlx::query_as!(DispatchQueueRow,
            "SELECT id, tenant_id, shipment_id, customer_name, customer_phone,
                    dest_address_line1, dest_city, dest_province, dest_postal_code,
                    dest_lat, dest_lng, cod_amount_cents, special_instructions, service_type, status
             FROM dispatch.dispatch_queue
             WHERE tenant_id = $1 AND status = 'pending'
             ORDER BY queued_at ASC",
            tenant_id
        ).fetch_all(&self.pool).await?;
        Ok(rows)
    }

    pub async fn mark_dispatched(&self, shipment_id: Uuid) -> anyhow::Result<()> {
        sqlx::query!(
            "UPDATE dispatch.dispatch_queue
             SET status = 'dispatched', dispatched_at = NOW()
             WHERE shipment_id = $1",
            shipment_id
        ).execute(&self.pool).await?;
        Ok(())
    }
}
```

- [ ] **Step 4: Create driver_profiles_repo.rs**

`services/dispatch/src/infrastructure/db/driver_profiles_repo.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DriverProfileRow {
    pub id:        Uuid,
    pub tenant_id: Uuid,
    pub email:     String,
}

pub struct PgDriverProfilesRepository {
    pool: PgPool,
}

impl PgDriverProfilesRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn upsert(&self, row: &DriverProfileRow) -> anyhow::Result<()> {
        sqlx::query!(
            r#"INSERT INTO dispatch.driver_profiles (id, tenant_id, email)
               VALUES ($1, $2, $3)
               ON CONFLICT (id) DO NOTHING"#,
            row.id, row.tenant_id, row.email
        ).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_by_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Vec<DriverProfileRow>> {
        let rows = sqlx::query_as!(DriverProfileRow,
            "SELECT id, tenant_id, email FROM dispatch.driver_profiles
             WHERE tenant_id = $1 AND is_active = true",
            tenant_id
        ).fetch_all(&self.pool).await?;
        Ok(rows)
    }
}
```

- [ ] **Step 5: Create shipment_consumer.rs**

`services/dispatch/src/infrastructure/messaging/shipment_consumer.rs`:

```rust
//! Consumes SHIPMENT_CREATED events → inserts into dispatch_queue.

use logisticos_events::{envelope::EventEnvelope, payloads::ShipmentCreated, topics};
use rdkafka::{consumer::{CommitMode, Consumer, StreamConsumer}, config::ClientConfig, Message};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::infrastructure::db::dispatch_queue_repo::{DispatchQueueRow, PgDispatchQueueRepository};

pub async fn start_shipment_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-shipment", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::SHIPMENT_CREATED])?;
    let repo = Arc::new(PgDispatchQueueRepository::new(pool));

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Some(payload) = msg.payload() {
                    if let Err(e) = handle_shipment_created(payload, &repo).await {
                        tracing::warn!(err = %e, "shipment consumer: handler error (skipping)");
                    }
                }
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "shipment consumer: recv error");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle_shipment_created(
    payload: &[u8],
    repo: &PgDispatchQueueRepository,
) -> anyhow::Result<()> {
    // The KafkaProducer wraps events in an EventEnvelope — unwrap before deserialising.
    let envelope: EventEnvelope<ShipmentCreated> = serde_json::from_slice(payload)?;
    let d = envelope.payload;

    let row = DispatchQueueRow {
        id:                  Uuid::new_v4(),
        tenant_id:           envelope.tenant_id,
        shipment_id:         d.shipment_id,
        customer_name:       d.customer_name,
        customer_phone:      d.customer_phone,
        dest_address_line1:  d.destination_address.clone(),
        dest_city:           d.destination_city,
        dest_province:       String::new(),
        dest_postal_code:    String::new(),
        dest_lat:            d.destination_lat,
        dest_lng:            d.destination_lng,
        cod_amount_cents:    d.cod_amount_cents,
        special_instructions: None,
        service_type:        d.service_type,
        status:              "pending".to_string(),
    };

    repo.upsert(&row).await?;
    tracing::info!(shipment_id = %d.shipment_id, "Shipment added to dispatch queue");
    Ok(())
}
```

> **Note:** Check `libs/events/src/envelope.rs` for the exact struct name — it may be `Event` with a `payload` field of type `T`. Adjust the deserialization accordingly.

- [ ] **Step 6: Create user_consumer.rs**

`services/dispatch/src/infrastructure/messaging/user_consumer.rs`:

```rust
//! Consumes USER_CREATED events → inserts drivers into driver_profiles.

use logisticos_events::{envelope::EventEnvelope, payloads::UserCreated, topics};
use rdkafka::{consumer::{CommitMode, Consumer, StreamConsumer}, config::ClientConfig, Message};
use sqlx::PgPool;
use std::sync::Arc;

use crate::infrastructure::db::driver_profiles_repo::{DriverProfileRow, PgDriverProfilesRepository};

pub async fn start_user_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-users", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::USER_CREATED])?;
    let repo = Arc::new(PgDriverProfilesRepository::new(pool));

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Some(payload) = msg.payload() {
                    if let Err(e) = handle_user_created(payload, &repo).await {
                        tracing::warn!(err = %e, "user consumer: handler error (skipping)");
                    }
                }
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "user consumer: recv error");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle_user_created(
    payload: &[u8],
    repo: &PgDriverProfilesRepository,
) -> anyhow::Result<()> {
    let envelope: EventEnvelope<UserCreated> = serde_json::from_slice(payload)?;
    let d = envelope.payload;

    // Only cache driver-role users
    if !d.roles.iter().any(|r| r == "driver") {
        return Ok(());
    }

    let row = DriverProfileRow {
        id:        d.user_id,
        tenant_id: d.tenant_id,
        email:     d.email,
    };
    repo.upsert(&row).await?;
    tracing::info!(user_id = %d.user_id, "Driver profile cached in dispatch");
    Ok(())
}
```

- [ ] **Step 7: Export new repos from db/mod.rs**

In `services/dispatch/src/infrastructure/db/mod.rs`, add:

```rust
pub mod dispatch_queue_repo;
pub mod driver_profiles_repo;
pub use dispatch_queue_repo::PgDispatchQueueRepository;
pub use driver_profiles_repo::PgDriverProfilesRepository;
```

- [ ] **Step 8: Export new consumers from messaging/mod.rs**

In `services/dispatch/src/infrastructure/messaging/mod.rs`, check if this file exists and has the compliance_consumer already. Add:

```rust
pub mod shipment_consumer;
pub mod user_consumer;
pub use shipment_consumer::start_shipment_consumer;
pub use user_consumer::start_user_consumer;
```

- [ ] **Step 9: Wire consumers in bootstrap.rs**

In `services/dispatch/src/bootstrap.rs`, add after the compliance consumer spawn:

```rust
use crate::infrastructure::messaging::{start_shipment_consumer, start_user_consumer};

// Spawn shipment consumer
let pool_for_shipment = pool.clone();
let brokers_shipment = cfg.kafka.brokers.clone();
let group_shipment = cfg.kafka.group_id.clone();
tokio::spawn(async move {
    if let Err(e) = start_shipment_consumer(&brokers_shipment, &group_shipment, pool_for_shipment).await {
        tracing::error!("Shipment consumer crashed: {e}");
    }
});

// Spawn user consumer
let pool_for_users = pool.clone();
let brokers_users = cfg.kafka.brokers.clone();
let group_users = cfg.kafka.group_id.clone();
tokio::spawn(async move {
    if let Err(e) = start_user_consumer(&brokers_users, &group_users, pool_for_users).await {
        tracing::error!("User consumer crashed: {e}");
    }
});
```

- [ ] **Step 10: Add GET /v1/queue and GET /v1/drivers endpoints**

Create `services/dispatch/src/api/http/queue.rs`:

```rust
use axum::{extract::State, Json};
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use crate::api::http::AppState;

pub async fn list_queue(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let items = state.queue_repo
        .list_pending(claims.tenant_id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": items, "count": items.len() })))
}

pub async fn list_drivers(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let drivers = state.drivers_repo
        .list_by_tenant(claims.tenant_id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": drivers })))
}
```

Add `queue_repo` and `drivers_repo` to `AppState` in `services/dispatch/src/api/http/mod.rs`:

```rust
pub struct AppState {
    pub dispatch_service: Arc<DriverAssignmentService>,
    pub queue_repo:       Arc<PgDispatchQueueRepository>,
    pub drivers_repo:     Arc<PgDriverProfilesRepository>,
    pub jwt:              Arc<logisticos_auth::jwt::JwtService>,
}
```

Add routes in the router:

```rust
.route("/v1/queue",   get(queue::list_queue))
.route("/v1/drivers", get(queue::list_drivers))
```

Update `bootstrap.rs` to create and pass these repos to `AppState`:

```rust
let queue_repo   = Arc::new(PgDispatchQueueRepository::new(pool.clone()));
let drivers_repo = Arc::new(PgDriverProfilesRepository::new(pool.clone()));

let state = Arc::new(AppState {
    dispatch_service,
    queue_repo,
    drivers_repo,
    jwt: Arc::clone(&jwt),
});
```

- [ ] **Step 11: Build and verify**

```bash
cargo build -p logisticos-dispatch
```

Expected: compiles with no errors.

- [ ] **Step 12: Commit**

```bash
git add services/dispatch/
git commit -m "feat(dispatch): dispatch_queue + driver_profiles tables, SHIPMENT_CREATED and USER_CREATED consumers

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 6: Dispatch — quick_dispatch Endpoint

**Files:**
- Modify: `services/dispatch/src/application/services/driver_assignment_service.rs`
- Create: `services/dispatch/src/api/http/dispatch_ops.rs`
- Modify: `services/dispatch/src/api/http/mod.rs`
- Modify: `libs/events/src/payloads.rs` — ensure `TaskAssigned` fields match (already added in Task 2)

The `quick_dispatch` method consolidates: find shipment in queue → create route → add stop → find best driver → create assignment → emit `TASK_ASSIGNED` → mark queue item dispatched.

- [ ] **Step 1: Write test for quick_dispatch**

Add to `services/dispatch/src/application/services/driver_assignment_service.rs` (bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_assigned_event_has_required_fields() {
        // Validate that TaskAssigned payload compiles and has all fields
        let _ = logisticos_events::payloads::TaskAssigned {
            task_id:             uuid::Uuid::new_v4(),
            assignment_id:       uuid::Uuid::new_v4(),
            shipment_id:         uuid::Uuid::new_v4(),
            route_id:            uuid::Uuid::new_v4(),
            driver_id:           uuid::Uuid::new_v4(),
            tenant_id:           uuid::Uuid::new_v4(),
            sequence:            1,
            address_line1:       "123 Test St".into(),
            address_city:        "Manila".into(),
            address_province:    "Metro Manila".into(),
            address_postal_code: "1000".into(),
            address_lat:         Some(14.5995),
            address_lng:         Some(120.9842),
            customer_name:       "Test Customer".into(),
            customer_phone:      "+63912345678".into(),
            cod_amount_cents:    None,
            special_instructions: None,
        };
    }
}
```

```bash
cargo test -p logisticos-dispatch
```

Expected: PASS (the struct was defined in Task 2).

- [ ] **Step 2: Add `shipment_id` to the `DriverAssigned` domain event struct**

The legacy `DRIVER_ASSIGNED` event (emitted for delivery-experience) must include `shipment_id` so delivery-experience can map the event to a shipment tracking record.

Open `services/dispatch/src/domain/events/mod.rs` and add `shipment_id: Uuid` to the `DriverAssigned` struct:

```rust
// Before (existing):
pub struct DriverAssigned {
    pub assignment_id: Uuid,
    pub route_id:      Uuid,
    pub driver_id:     Uuid,
    pub tenant_id:     Uuid,
}

// After (add shipment_id):
pub struct DriverAssigned {
    pub assignment_id: Uuid,
    pub shipment_id:   Uuid,   // ADD THIS
    pub route_id:      Uuid,
    pub driver_id:     Uuid,
    pub tenant_id:     Uuid,
}
```

Run `cargo check -p logisticos-dispatch` and fix any call sites that construct `DriverAssigned` without `shipment_id`.

- [ ] **Step 3: Add QuickDispatchCommand**

In `services/dispatch/src/application/commands/mod.rs`, add:

```rust
#[derive(Debug)]
pub struct QuickDispatchCommand {
    pub shipment_id:          uuid::Uuid,
    pub preferred_driver_id:  Option<uuid::Uuid>,
}
```

- [ ] **Step 4: Add quick_dispatch to DriverAssignmentService**

`quick_dispatch` needs access to `queue_repo` and `driver_profiles_repo`. Add them to the struct:

```rust
pub struct DriverAssignmentService {
    route_repo:       Arc<dyn RouteRepository>,
    assignment_repo:  Arc<dyn DriverAssignmentRepository>,
    driver_avail_repo: Arc<dyn DriverAvailabilityRepository>,
    kafka:            Arc<KafkaProducer>,
    compliance_cache: Arc<Mutex<ComplianceCache>>,
    // NEW:
    queue_repo:       Arc<PgDispatchQueueRepository>,
    driver_profiles:  Arc<PgDriverProfilesRepository>,
}
```

Update `DriverAssignmentService::new` to accept the two new repos.

Add the method:

```rust
/// Convenience: dispatch a single shipment end-to-end in one call.
/// Creates a minimal single-stop route, assigns the best available driver,
/// emits TASK_ASSIGNED event with full customer details.
pub async fn quick_dispatch(
    &self,
    tenant_id: TenantId,
    cmd: QuickDispatchCommand,
) -> AppResult<DriverAssignment> {
    use logisticos_events::{payloads::TaskAssigned, topics, envelope::Event};
    use uuid::Uuid;

    // 1. Load shipment from queue
    let queue_item = self.queue_repo
        .find_by_shipment(cmd.shipment_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Shipment in dispatch queue", id: cmd.shipment_id.to_string() })?;

    if queue_item.status != "pending" {
        return Err(AppError::BusinessRule(format!(
            "Shipment {} is already {} — cannot dispatch again", cmd.shipment_id, queue_item.status
        )));
    }

    // 2. Find driver (explicit or auto)
    let driver_id = match cmd.preferred_driver_id {
        Some(id) => DriverId::from_uuid(id),
        None => {
            // Use proximity scoring from existing auto_assign logic
            let anchor = logisticos_types::Coordinates {
                lat: queue_item.dest_lat.unwrap_or(14.5995),
                lng: queue_item.dest_lng.unwrap_or(120.9842),
            };
            let candidates = self.driver_avail_repo
                .find_available_near(&tenant_id, anchor, DEFAULT_DRIVER_SEARCH_RADIUS_KM)
                .await
                .map_err(AppError::Internal)?;

            if candidates.is_empty() {
                return Err(AppError::BusinessRule("No available drivers nearby".into()));
            }

            candidates.iter()
                .min_by(|a, b| {
                    let sa = a.distance_km * 0.7 + a.active_stop_count as f64 * 0.3;
                    let sb = b.distance_km * 0.7 + b.active_stop_count as f64 * 0.3;
                    sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap()
                .driver_id
                .clone()
        }
    };

    // 3. Compliance check (reuse existing logic)
    let is_assignable = {
        let mut cache = self.compliance_cache.lock().await;
        match cache.get_status(driver_id.inner()).await {
            Ok(Some((_, assignable))) => assignable,
            Ok(None) => true,
            Err(e) => { tracing::warn!("Compliance cache error: {e}"); true }
        }
    };
    if !is_assignable {
        return Err(AppError::BusinessRule(format!("Driver {driver_id} is not compliance-cleared")));
    }

    // 4. Create a minimal single-stop route (placeholder vehicle_id)
    // NOTE: Before running, check services/dispatch/migrations/ to see if dispatch.routes has a
    // FK constraint on vehicle_id. If so, either seed a vehicle row and use its UUID here,
    // or make vehicle_id nullable (ALTER TABLE dispatch.routes ALTER COLUMN vehicle_id DROP NOT NULL).
    // Uuid::nil() is safe only if the column is nullable or has no FK constraint.
    let route_id = RouteId::new();
    let route = Route {
        id: route_id.clone(),
        tenant_id: tenant_id.clone(),
        driver_id: driver_id.clone(),
        vehicle_id: VehicleId::from_uuid(Uuid::nil()), // placeholder — see note above
        stops: vec![],
        status: RouteStatus::Planned,
        total_distance_km: 0.0,
        estimated_duration_minutes: 0,
        created_at: chrono::Utc::now(),
        started_at: None,
        completed_at: None,
    };
    self.route_repo.save(&route).await.map_err(AppError::Internal)?;

    // 5. Create assignment
    let assignment = DriverAssignment::new(tenant_id.clone(), driver_id.clone(), route_id.clone());
    self.assignment_repo.save(&assignment).await.map_err(AppError::Internal)?;

    // 6. Emit TASK_ASSIGNED with full customer details from dispatch_queue
    // Note: assignment.id and driver_id are domain newtypes (AssignmentId, DriverId) — call .inner() to get Uuid.
    let task_id = Uuid::new_v4();
    let task_event = Event::new("dispatch", "task.assigned", tenant_id.inner(), TaskAssigned {
        task_id,
        assignment_id: assignment.id.inner(),
        shipment_id:   cmd.shipment_id,
        route_id:      route_id.inner(),
        driver_id:     driver_id.inner(),
        tenant_id:     tenant_id.inner(),
        sequence:      1,
        address_line1:       queue_item.dest_address_line1.clone(),
        address_city:        queue_item.dest_city.clone(),
        address_province:    queue_item.dest_province.clone(),
        address_postal_code: queue_item.dest_postal_code.clone(),
        address_lat:         queue_item.dest_lat,
        address_lng:         queue_item.dest_lng,
        customer_name:       queue_item.customer_name.clone(),
        customer_phone:      queue_item.customer_phone.clone(),
        cod_amount_cents:    queue_item.cod_amount_cents,
        special_instructions: queue_item.special_instructions.clone(),
    });
    self.kafka.publish_event(topics::TASK_ASSIGNED, &task_event).await
        .map_err(AppError::Internal)?;

    // 7. Also emit legacy DRIVER_ASSIGNED for delivery-experience consumer
    // Note: delivery-experience expects shipment_id in this event — add it to DriverAssigned domain event if missing.
    let legacy_event = Event::new("dispatch", "driver.assigned", tenant_id.inner(), crate::domain::events::DriverAssigned {
        assignment_id: assignment.id.inner(),
        shipment_id:   cmd.shipment_id,
        route_id:      route_id.inner(),
        driver_id:     driver_id.inner(),
        tenant_id:     tenant_id.inner(),
    });
    self.kafka.publish_event(topics::DRIVER_ASSIGNED, &legacy_event).await
        .map_err(AppError::Internal)?;

    // 8. Mark queue item as dispatched
    self.queue_repo.mark_dispatched(cmd.shipment_id).await.map_err(AppError::Internal)?;

    tracing::info!(
        shipment_id = %cmd.shipment_id,
        driver_id = %driver_id,
        assignment_id = %assignment.id,
        "Quick dispatch complete"
    );
    Ok(assignment)
}
```

- [ ] **Step 5: Create HTTP handler for quick dispatch**

`services/dispatch/src/api/http/dispatch_ops.rs`:

```rust
use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_types::TenantId;
use crate::{api::http::AppState, application::commands::QuickDispatchCommand};

pub async fn quick_dispatch(
    AuthClaims(claims): AuthClaims,
    Path(shipment_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_ASSIGN);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let preferred_driver_id = body.get("preferred_driver_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Uuid>().ok());

    let cmd = QuickDispatchCommand { shipment_id, preferred_driver_id };
    let assignment = state.dispatch_service.quick_dispatch(tenant_id, cmd).await?;

    Ok(Json(serde_json::json!({
        "data": {
            "assignment_id": assignment.id.inner(),
            "driver_id": assignment.driver_id.inner(),
            "status": "pending"
        }
    })))
}
```

Add route in `mod.rs`:

```rust
.route("/v1/queue/:shipment_id/dispatch", post(dispatch_ops::quick_dispatch))
```

- [ ] **Step 6: Update AppState and bootstrap wiring**

Update `DriverAssignmentService::new` signature in bootstrap.rs to pass `queue_repo` and `driver_profiles`.

- [ ] **Step 7: Build and test**

```bash
cargo test -p logisticos-dispatch
cargo build -p logisticos-dispatch
```

Expected: all PASS.

- [ ] **Step 8: Commit**

```bash
git add services/dispatch/ libs/
git commit -m "feat(dispatch): quick_dispatch endpoint; TASK_ASSIGNED event with full customer details

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 7: Driver-Ops — TASK_ASSIGNED Consumer Creates Tasks

**Files:**
- Create: `services/driver-ops/src/infrastructure/messaging/task_consumer.rs`
- Modify: `services/driver-ops/src/infrastructure/messaging/mod.rs`
- Modify: `services/driver-ops/src/bootstrap.rs`

- [ ] **Step 1: Write test for task consumer handler**

Create `services/driver-ops/src/infrastructure/messaging/task_consumer.rs` with an embedded test:

```rust
//! Consumes TASK_ASSIGNED events → creates DriverTask rows in driver_ops.tasks.

use logisticos_events::{envelope::EventEnvelope, payloads::TaskAssigned, topics};
use rdkafka::{consumer::{CommitMode, Consumer, StreamConsumer}, config::ClientConfig, Message};
use sqlx::PgPool;

pub async fn start_task_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-tasks", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::TASK_ASSIGNED])?;

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Some(payload) = msg.payload() {
                    if let Err(e) = handle_task_assigned(payload, &pool).await {
                        tracing::warn!(err = %e, "task consumer: handler error (skipping)");
                    }
                }
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "task consumer: recv error");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle_task_assigned(payload: &[u8], pool: &PgPool) -> anyhow::Result<()> {
    let envelope: EventEnvelope<TaskAssigned> = serde_json::from_slice(payload)?;
    let t = envelope.payload;

    // Ensure driver profile row exists (may arrive before USER_CREATED is processed)
    sqlx::query!(
        r#"INSERT INTO driver_ops.drivers (id, tenant_id, name, phone, status)
           VALUES ($1, $2, 'Driver', '', 'offline')
           ON CONFLICT (id) DO NOTHING"#,
        t.driver_id,
        t.tenant_id,
    ).execute(pool).await?;

    // Insert the task
    sqlx::query!(
        r#"
        INSERT INTO driver_ops.tasks (
            id, driver_id, route_id, shipment_id,
            task_type, sequence, status,
            address_line1, address_line2, city, province, postal_code, country,
            lat, lng,
            customer_name, customer_phone, cod_amount_cents, special_instructions
        ) VALUES (
            $1, $2, $3, $4,
            'delivery', $5, 'pending',
            $6, '', $7, $8, $9, 'PH',
            $10, $11,
            $12, $13, $14, $15
        )
        ON CONFLICT (id) DO NOTHING
        "#,
        t.task_id,
        t.driver_id,
        t.route_id,
        t.shipment_id,
        t.sequence as i32,
        t.address_line1,
        t.address_city,
        t.address_province,
        t.address_postal_code,
        t.address_lat,
        t.address_lng,
        t.customer_name,
        t.customer_phone,
        t.cod_amount_cents,
        t.special_instructions,
    ).execute(pool).await?;

    tracing::info!(
        task_id = %t.task_id,
        driver_id = %t.driver_id,
        shipment_id = %t.shipment_id,
        "Task created for driver"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn task_assigned_payload_deserializes() {
        let json = r#"{
            "id":"evt1","source":"dispatch","event_type":"task.assigned",
            "tenant_id":"00000000-0000-0000-0000-000000000001",
            "occurred_at":"2026-01-01T00:00:00Z",
            "payload":{
                "task_id":"00000000-0000-0000-0000-000000000010",
                "assignment_id":"00000000-0000-0000-0000-000000000011",
                "shipment_id":"00000000-0000-0000-0000-000000000012",
                "route_id":"00000000-0000-0000-0000-000000000013",
                "driver_id":"00000000-0000-0000-0000-000000000004",
                "tenant_id":"00000000-0000-0000-0000-000000000001",
                "sequence":1,
                "address_line1":"123 Test St",
                "address_city":"Manila",
                "address_province":"Metro Manila",
                "address_postal_code":"1000",
                "address_lat":14.5995,
                "address_lng":120.9842,
                "customer_name":"Test Customer",
                "customer_phone":"+63912345678",
                "cod_amount_cents":null,
                "special_instructions":null
            }
        }"#;

        let result: Result<logisticos_events::envelope::EventEnvelope<logisticos_events::payloads::TaskAssigned>, _>
            = serde_json::from_str(json);
        assert!(result.is_ok(), "Deserialization failed: {:?}", result.err());
        assert_eq!(result.unwrap().payload.customer_name, "Test Customer");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p logisticos-driver-ops
```

Expected: the new deserialization test PASSES. (The struct is defined correctly.)

- [ ] **Step 3: Update messaging/mod.rs**

Replace the stub content in `services/driver-ops/src/infrastructure/messaging/mod.rs`:

```rust
pub mod task_consumer;
pub use task_consumer::start_task_consumer;
```

- [ ] **Step 4: Wire consumer in bootstrap.rs**

In `services/driver-ops/src/bootstrap.rs`, after `let app = router(state);`:

```rust
use crate::infrastructure::messaging::start_task_consumer;

let pool_for_tasks = pool.clone();
let brokers_for_tasks = cfg.kafka.brokers.clone();
let group_for_tasks = cfg.kafka.group_id.clone();
tokio::spawn(async move {
    if let Err(e) = start_task_consumer(&brokers_for_tasks, &group_for_tasks, pool_for_tasks).await {
        tracing::error!("Task consumer crashed: {e}");
    }
});
```

- [ ] **Step 5: Build**

```bash
cargo build -p logisticos-driver-ops
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add services/driver-ops/
git commit -m "feat(driver-ops): TASK_ASSIGNED consumer creates DriverTask on dispatch

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 8: POD — Fix get_pod Stub

**Files:**
- Modify: `services/pod/src/application/services/pod_service.rs`
- Modify: `services/pod/src/api/http/pod.rs`

- [ ] **Step 1: Add get_by_id to PodService**

In `services/pod/src/application/services/pod_service.rs`, add:

```rust
/// Retrieve a POD record by ID (for admin/ops views).
pub async fn get_by_id(&self, pod_id: uuid::Uuid) -> AppResult<crate::domain::entities::ProofOfDelivery> {
    self.load_pod(pod_id).await
}
```

- [ ] **Step 2: Fix get_pod handler**

Replace the stub in `services/pod/src/api/http/pod.rs`:

```rust
pub async fn get_pod(
    AuthClaims(_claims): AuthClaims,
    Path(pod_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pod = state.pod_service.get_by_id(pod_id).await?;
    Ok(Json(serde_json::json!({ "data": pod })))
}
```

- [ ] **Step 3: Build**

```bash
cargo build -p logisticos-pod
```

- [ ] **Step 4: Commit**

```bash
git add services/pod/
git commit -m "fix(pod): implement get_pod endpoint (was a stub)

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 9: End-to-End Smoke Test

Start all 6 services and walk the full flow with `curl`.

- [ ] **Step 1: Check envelope struct name**

Before starting services, verify the `EventEnvelope` type name in `libs/events/src/envelope.rs`. All consumers in Tasks 5, 6, and 7 use `EventEnvelope<T>`. If the actual struct name is different (e.g. `Event<T>` with a `payload` field), update all consumer files to match.

```bash
cat d:/LogisticOS/libs/events/src/envelope.rs
```

Adjust consumer deserialization code in tasks 3, 5, 6, 7 accordingly.

- [ ] **Step 2: Start infrastructure**

```bash
docker compose up -d postgres redis kafka zookeeper minio
```

Wait for healthy state:
```bash
docker compose ps
```

- [ ] **Step 3: Start all services (6 terminals)**

```bash
# Terminal 1
cd services/identity       && cargo run

# Terminal 2
cd services/order-intake   && cargo run

# Terminal 3
cd services/dispatch       && cargo run

# Terminal 4
cd services/driver-ops     && cargo run

# Terminal 5
cd services/pod            && cargo run

# Terminal 6
cd services/delivery-experience && cargo run
```

Wait for all to log "service listening".

- [ ] **Step 4: Create tenant and login**

```bash
# Create tenant
curl -s -X POST http://localhost:8001/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{
    "name":"Demo Logistics","slug":"demo",
    "owner_email":"admin@demo.com","owner_password":"LogisticOS1!",
    "owner_first_name":"Admin","owner_last_name":"User"
  }' | jq '{tenant_id: .data.id}'
```

Save `TENANT_ID`.

```bash
# Login as admin
TOKEN=$(curl -s -X POST http://localhost:8001/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@demo.com","password":"LogisticOS1!","tenant_slug":"demo"}' \
  | jq -r '.access_token')
echo "TOKEN: $TOKEN"
```

- [ ] **Step 5: Invite a driver**

```bash
INVITE_RESP=$(curl -s -X POST http://localhost:8001/v1/users \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "email":"driver@demo.com",
    "first_name":"Ahmed","last_name":"Al-Rashid",
    "roles":["driver"]
  }')
echo "$INVITE_RESP" | jq '{driver_id: .data.user_id, temp_password: .data.temp_password}'
DRIVER_ID=$(echo "$INVITE_RESP" | jq -r '.data.user_id')
DRIVER_PASSWORD=$(echo "$INVITE_RESP" | jq -r '.data.temp_password')
echo "DRIVER_ID=$DRIVER_ID  DRIVER_PASSWORD=$DRIVER_PASSWORD"
```

Save both `DRIVER_ID` and `DRIVER_PASSWORD`. Check dispatch service logs for "Driver profile cached in dispatch".

> **Note:** The field name in the response may be `temp_password`, `temporary_password`, or `initial_password` — check the identity service `invite_user` handler response to confirm the exact field name.

- [ ] **Step 6: Create a shipment**

```bash
SHIPMENT=$(curl -s -X POST http://localhost:8004/v1/shipments \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "customer_name":"Maria Santos",
    "customer_phone":"+63912345678",
    "origin":{"line1":"123 Merchant St","city":"Makati","province":"Metro Manila","postal_code":"1200","country_code":"PH"},
    "destination":{"line1":"456 Customer Ave","city":"Pasig","province":"Metro Manila","postal_code":"1600","country_code":"PH"},
    "service_type":"standard",
    "weight_grams":500
  }' | jq .)
echo "$SHIPMENT" | jq '{shipment_id: .data.id, tracking: .data.tracking_number}'
SHIPMENT_ID=$(echo "$SHIPMENT" | jq -r '.data.id')
```

Check dispatch service logs for "Shipment added to dispatch queue".

- [ ] **Step 7: Dispatch to driver**

```bash
curl -s -X POST "http://localhost:8005/v1/queue/$SHIPMENT_ID/dispatch" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"preferred_driver_id\":\"$DRIVER_ID\"}" | jq .
```

Expected: `{"data": {"assignment_id": "...", "driver_id": "...", "status": "pending"}}`.

Check driver-ops logs for "Task created for driver".

- [ ] **Step 8: Driver logs in and starts task**

```bash
# Login as driver — $DRIVER_PASSWORD was captured in Step 5 above
DRIVER_TOKEN=$(curl -s -X POST http://localhost:8001/v1/auth/login \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"driver@demo.com\",\"password\":\"$DRIVER_PASSWORD\",\"tenant_slug\":\"demo\"}" \
  | jq -r '.access_token')

# List tasks (should show the task created by the TASK_ASSIGNED consumer)
TASKS_RESP=$(curl -s http://localhost:8006/v1/tasks \
  -H "Authorization: Bearer $DRIVER_TOKEN")
echo "$TASKS_RESP" | jq .
TASK_ID=$(echo "$TASKS_RESP" | jq -r '.data[0].id')
echo "TASK_ID=$TASK_ID"

# Start first task
curl -s -X POST "http://localhost:8006/v1/tasks/$TASK_ID/start" \
  -H "Authorization: Bearer $DRIVER_TOKEN" | jq .
```

- [ ] **Step 9: Initiate POD and submit**

```bash
# Initiate POD
POD_ID=$(curl -s -X POST http://localhost:8011/v1/pods \
  -H "Authorization: Bearer $DRIVER_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"shipment_id\":\"$SHIPMENT_ID\",
    \"task_id\":\"$TASK_ID\",
    \"recipient_name\":\"Maria Santos\",
    \"capture_lat\":14.5764,
    \"capture_lng\":121.0851,
    \"delivery_lat\":14.5764,
    \"delivery_lng\":121.0851
  }" | jq -r '.data.pod_id')

# Submit POD
curl -s -X PUT "http://localhost:8011/v1/pods/$POD_ID/submit" \
  -H "Authorization: Bearer $DRIVER_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"pod_id\":\"$POD_ID\",\"otp_code\":null,\"cod_collected_cents\":null}" | jq .
```

- [ ] **Step 10: Complete task**

```bash
curl -s -X POST "http://localhost:8006/v1/tasks/$TASK_ID/complete" \
  -H "Authorization: Bearer $DRIVER_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"task_id\":\"$TASK_ID\",\"pod_id\":\"$POD_ID\"}" | jq .
```

Check order-intake logs for "status updated to delivered".

- [ ] **Step 11: Check tracking**

```bash
TRACKING=$(echo "$SHIPMENT" | jq -r '.data.tracking_number')
curl -s "http://localhost:8007/track/$TRACKING" | jq '{status: .status, label: .status_label}'
```

Expected: `{"status": "delivered", "label": "Delivered"}`.

- [ ] **Step 12: Commit**

```bash
git add .
git commit -m "test: end-to-end smoke test passes — full order→dispatch→POD→tracking flow

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 10: Driver App — Login Screen + Task Wiring

**Files:**
- Create: `apps/driver-app/src/app/(auth)/login.tsx`
- Create: `apps/driver-app/src/app/(auth)/_layout.tsx`
- Create: `apps/driver-app/src/lib/api-client.ts`
- Modify: `apps/driver-app/src/app/(tabs)/index.tsx` (home/tasks tab)
- Modify: `apps/driver-app/src/store/index.ts` (add auth slice if needed)

- [ ] **Step 1: Create API client**

`apps/driver-app/src/lib/api-client.ts`:

```typescript
import AsyncStorage from "@react-native-async-storage/async-storage";

const BASE_URL = process.env.EXPO_PUBLIC_API_URL ?? "http://localhost:8001";
const DRIVER_OPS_URL = process.env.EXPO_PUBLIC_DRIVER_OPS_URL ?? "http://localhost:8006";
const POD_URL = process.env.EXPO_PUBLIC_POD_URL ?? "http://localhost:8011";

async function getToken(): Promise<string | null> {
  return AsyncStorage.getItem("access_token");
}

async function authFetch(baseUrl: string, path: string, init?: RequestInit) {
  const token = await getToken();
  const res = await fetch(`${baseUrl}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...(init?.headers ?? {}),
    },
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`${res.status}: ${text}`);
  }
  return res.json();
}

export const apiClient = {
  login: (email: string, password: string, tenantSlug: string) =>
    fetch(`${BASE_URL}/v1/auth/login`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ email, password, tenant_slug: tenantSlug }),
    }).then((r) => r.json()),

  getTasks: () => authFetch(DRIVER_OPS_URL, "/v1/tasks"),
  startTask: (taskId: string) => authFetch(DRIVER_OPS_URL, `/v1/tasks/${taskId}/start`, { method: "POST" }),
  completeTask: (taskId: string, podId: string) =>
    authFetch(DRIVER_OPS_URL, `/v1/tasks/${taskId}/complete`, {
      method: "POST",
      body: JSON.stringify({ task_id: taskId, pod_id: podId }),
    }),

  initiatePod: (body: object) => authFetch(POD_URL, "/v1/pods", { method: "POST", body: JSON.stringify(body) }),
  submitPod: (podId: string, body: object) => authFetch(POD_URL, `/v1/pods/${podId}/submit`, { method: "PUT", body: JSON.stringify(body) }),
};
```

- [ ] **Step 2: Create auth layout**

`apps/driver-app/src/app/(auth)/_layout.tsx`:

```tsx
import { Stack } from "expo-router";
export default function AuthLayout() {
  return <Stack screenOptions={{ headerShown: false }} />;
}
```

- [ ] **Step 3: Create login screen**

`apps/driver-app/src/app/(auth)/login.tsx`:

```tsx
import { View, Text, TextInput, Pressable, StyleSheet, ActivityIndicator, Alert } from "react-native";
import { router } from "expo-router";
import { useState } from "react";
import AsyncStorage from "@react-native-async-storage/async-storage";
import { apiClient } from "../../lib/api-client";
import Animated, { FadeInDown } from "react-native-reanimated";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const PURPLE = "#A855F7";

export default function LoginScreen() {
  const [email,      setEmail]      = useState("");
  const [password,   setPassword]   = useState("");
  const [tenantSlug, setTenantSlug] = useState("demo");
  const [loading,    setLoading]    = useState(false);

  async function handleLogin() {
    if (!email.trim() || !password.trim()) return;
    setLoading(true);
    try {
      const data = await apiClient.login(email.trim(), password, tenantSlug.trim());
      if (!data.access_token) throw new Error(data.error ?? "Login failed");
      await AsyncStorage.setItem("access_token",  data.access_token);
      await AsyncStorage.setItem("refresh_token", data.refresh_token ?? "");
      router.replace("/(tabs)");
    } catch (err: unknown) {
      Alert.alert("Login Failed", err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  }

  return (
    <View style={s.container}>
      <Animated.View entering={FadeInDown.springify()} style={s.card}>
        <Text style={s.title}>Driver Login</Text>
        <Text style={s.sub}>LogisticOS Driver App</Text>

        <TextInput
          style={s.input}
          placeholder="Tenant slug (e.g. demo)"
          placeholderTextColor="rgba(255,255,255,0.2)"
          value={tenantSlug}
          onChangeText={setTenantSlug}
          autoCapitalize="none"
        />
        <TextInput
          style={s.input}
          placeholder="Email"
          placeholderTextColor="rgba(255,255,255,0.2)"
          value={email}
          onChangeText={setEmail}
          autoCapitalize="none"
          keyboardType="email-address"
        />
        <TextInput
          style={s.input}
          placeholder="Password"
          placeholderTextColor="rgba(255,255,255,0.2)"
          value={password}
          onChangeText={setPassword}
          secureTextEntry
        />

        <Pressable
          onPress={handleLogin}
          disabled={loading}
          style={({ pressed }) => [s.btn, { opacity: pressed || loading ? 0.6 : 1 }]}
        >
          {loading
            ? <ActivityIndicator color={CYAN} />
            : <Text style={s.btnText}>Sign In →</Text>}
        </Pressable>
      </Animated.View>
    </View>
  );
}

const s = StyleSheet.create({
  container: { flex: 1, backgroundColor: CANVAS, justifyContent: "center", padding: 20 },
  card:      { backgroundColor: "rgba(255,255,255,0.04)", borderRadius: 16, padding: 24,
               borderWidth: 1, borderColor: "rgba(0,229,255,0.15)" },
  title:     { fontSize: 22, fontFamily: "SpaceGrotesk-SemiBold", color: "#fff", marginBottom: 4 },
  sub:       { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", marginBottom: 24 },
  input:     { backgroundColor: "rgba(255,255,255,0.04)", borderWidth: 1, borderColor: "rgba(255,255,255,0.08)",
               borderRadius: 8, padding: 12, marginBottom: 12,
               fontSize: 13, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.8)" },
  btn:       { borderRadius: 12, paddingVertical: 14, alignItems: "center",
               backgroundColor: "rgba(168,85,247,0.18)", borderWidth: 1, borderColor: "rgba(168,85,247,0.35)" },
  btnText:   { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", color: "#fff" },
});
```

- [ ] **Step 4: Wire task list in home tab to real API**

In `apps/driver-app/src/app/(tabs)/index.tsx`, import `apiClient` and replace mock data:

```tsx
// Add at the top of the component
const [tasks, setTasks] = useState([]);
const [loading, setLoading] = useState(false);

useEffect(() => {
  setLoading(true);
  apiClient.getTasks()
    .then(data => setTasks(data.data ?? []))
    .catch(console.error)
    .finally(() => setLoading(false));
}, []);
```

Render a `FlatList` of tasks showing `customer_name`, `address`, `status` badge.

- [ ] **Step 5: Add EXPO_PUBLIC env vars**

In `apps/driver-app/.env.local` (create if not exists):

```
EXPO_PUBLIC_API_URL=http://localhost:8001
EXPO_PUBLIC_DRIVER_OPS_URL=http://localhost:8006
EXPO_PUBLIC_POD_URL=http://localhost:8011
```

- [ ] **Step 6: Build web export and verify login works**

```bash
cd apps/driver-app
npx expo export --platform web --output-dir web-dist3
npx serve web-dist3 --listen 8083 --config serve.json
```

Open http://localhost:8083 — login screen should appear. Log in with driver credentials.

- [ ] **Step 7: Commit**

```bash
git add apps/driver-app/
git commit -m "feat(driver-app): login screen + task list wired to real driver-ops API

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 11: Admin Portal — Dispatch Console Wiring

**Files:**
- Modify: `apps/admin-portal/src/app/dispatch/page.tsx` (or create if not existing)

- [ ] **Step 1: Find the dispatch console page**

```bash
find d:/LogisticOS/apps/admin-portal/src -name "*.tsx" | grep -i dispatch | head -5
```

If it exists, read it to understand the current mock data structure. If not, create it.

- [ ] **Step 2: Add a server action or client fetch for dispatch queue**

> **Auth token — discovery required first:** Before wiring the fetch, check how the admin portal handles authentication. Run:
> ```bash
> find apps/admin-portal/src -name "auth*.ts" -o -name "session*.ts" | head -5
> grep -r "getServerSession\|useSession\|getToken\|access_token" apps/admin-portal/src --include="*.ts" --include="*.tsx" -l | head -5
> ```
> - If using `next-auth` / `Auth.js`: use `const session = await getServerSession(authOptions)` in server components; `session.accessToken` holds the JWT.
> - If using a custom auth context: find the hook (e.g. `useAuth()`) and extract the token from it.
> - If no auth system exists yet: add a temporary server-side login call at the top of the page component: `const token = await fetch(...login...).then(r => r.json()).then(d => d.access_token)`.
>
> Once you know how to get the token, replace `${adminToken}` below with the correct expression.

In the dispatch page, replace mock data with real API calls:

```typescript
// Fetch pending queue items
const res = await fetch(`${process.env.DISPATCH_SERVICE_URL}/v1/queue`, {
  headers: { Authorization: `Bearer ${adminToken}` },
});
const { data: queueItems } = await res.json();
```

Add a "Dispatch" button per queue item that calls `POST /v1/queue/{shipment_id}/dispatch`.

- [ ] **Step 3: Add active routes view**

Wire `GET /v1/routes` from dispatch service (which already exists via the `list_routes` method and HTTP handler) to show currently active driver routes.

- [ ] **Step 4: Test in browser**

Start admin portal:
```bash
cd apps/admin-portal && npm run dev
```

Navigate to `/dispatch` — should show pending shipments from queue and active routes.

- [ ] **Step 5: Commit**

```bash
git add apps/admin-portal/
git commit -m "feat(admin-portal): wire dispatch console to real dispatch service API

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 12: Merchant Portal + Customer Portal Wiring

**Files:**
- Modify: `apps/merchant-portal/src/app/shipments/...` pages
- Modify: `apps/customer-portal/src/app/track/[tracking_number]/page.tsx` (or equivalent)

- [ ] **Step 1: Merchant portal — wire shipment creation**

> **Auth token — discovery required first:** Same pattern as Task 11 Step 2. Check how the merchant portal handles auth:
> ```bash
> grep -r "getServerSession\|useSession\|access_token" apps/merchant-portal/src --include="*.ts" --include="*.tsx" -l | head -5
> ```
> Replace `${merchantToken}` below with the correct expression from your auth system.

Find the shipment create form in the merchant portal and wire it to `POST /v1/shipments` on order-intake:

```typescript
const res = await fetch(`${process.env.ORDER_INTAKE_URL}/v1/shipments`, {
  method: "POST",
  headers: {
    "Content-Type": "application/json",
    Authorization: `Bearer ${merchantToken}`,
  },
  body: JSON.stringify({
    customer_name:   form.customerName,
    customer_phone:  form.customerPhone,
    origin:          { line1: form.originLine1, city: form.originCity, province: form.originProvince, postal_code: form.originPostal, country_code: "PH" },
    destination:     { line1: form.destLine1, city: form.destCity, province: form.destProvince, postal_code: form.destPostal, country_code: "PH" },
    service_type:    form.serviceType,
    weight_grams:    form.weightGrams,
    cod_amount_cents: form.codAmountCents ?? undefined,
  }),
});
```

- [ ] **Step 2: Merchant portal — wire shipment list**

Wire the shipment list page to `GET /v1/shipments?limit=50&offset=0` from order-intake. Show `tracking_number`, `status`, `destination.city`, `created_at`.

- [ ] **Step 3: Customer portal — wire tracking page**

Find or create `apps/customer-portal/src/app/track/[tracking_number]/page.tsx`.

Wire to `GET /track/:tracking_number` on delivery-experience (port 8007):

```typescript
const res = await fetch(
  `${process.env.DELIVERY_EXPERIENCE_URL}/track/${params.tracking_number}`
);
const tracking = await res.json();
```

Display: status label, origin, destination, estimated delivery, status history timeline.

- [ ] **Step 4: Test end-to-end via portals**

1. Open merchant portal → create a shipment → copy tracking number
2. Open admin portal → see shipment in dispatch queue → click Dispatch
3. Open customer portal → enter tracking number → see "Driver Assigned" status
4. Simulate completion via curl (Task 9 steps 8-11)
5. Refresh customer portal → see "Delivered" status

- [ ] **Step 5: Commit**

```bash
git add apps/merchant-portal/ apps/customer-portal/
git commit -m "feat(portals): wire merchant shipment creation and customer tracking to real APIs

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Notes for Agentic Execution

### Kafka EventEnvelope shape
Before implementing consumers (Tasks 3, 5, 6, 7), read `libs/events/src/envelope.rs` to confirm the exact field names. The producers use `Event::new(...)` — check what the serialized form looks like. The consumers deserialize with `EventEnvelope<T>` — this name must match.

### Driver login password
The identity `invite_user` generates a temporary password that is returned in the API response. The driver uses this temp password to log in. In the smoke test (Task 9), save the temp password from the invite response.

### RLS bypass in development
The Docker Compose uses `POSTGRES_USER: logisticos` which is a PostgreSQL superuser. Superusers bypass RLS policies, so tenant isolation queries run without setting `app.tenant_id`. This is intentional for development simplicity.

### TASK_ASSIGNED vs DRIVER_ASSIGNED duality
`quick_dispatch` emits BOTH events:
- `TASK_ASSIGNED` → driver-ops creates the DriverTask (full customer details)
- `DRIVER_ASSIGNED` → delivery-experience updates tracking status to "AssignedToDriver"

The `DRIVER_ASSIGNED` event payload used by delivery-experience expects `shipment_id`. The dispatch service's domain event struct (`DriverAssigned` in `dispatch/src/domain/events/mod.rs`) may not include `shipment_id` — add it when updating `quick_dispatch`.

### Compilation failures
If `cargo check --workspace` fails on any task, fix that task's compilation before moving to the next. Do not skip compilation errors by commenting out code — investigate and fix the root cause.
