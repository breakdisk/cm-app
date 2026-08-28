# Network International Shipping-Fee Payment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let AE-region (AED) tenant customers pay the shipping fee for a parcel/courier/balikbayan booking online via Network International, from the Customer App, as an alternative to the existing cash-at-pickup flow — without ever letting a shipment reach dispatch until payment is confirmed.

**Architecture:** `services/order-intake` prices the shipment (new AE tariff, a signed short-TTL quote token) and, when a quote token is presented at booking, defers publishing the three Kafka events that trigger dispatch/engagement/analytics until payment clears. `services/payments` owns a new `payment_intents` table and a `PaymentGateway` trait implemented against Network International's hosted checkout; a webhook it receives is the only thing that can mark an intent `Captured`. The two services talk over a new mesh-internal (`/v1/internal`, Istio mTLS) HTTP endpoint and a new pair of Kafka topics — no synchronous cross-service call sits in the customer's request path once checkout has started.

**Tech Stack:** Rust (Axum, SQLx/Postgres, rdkafka, reqwest, hmac/sha2/base64/subtle), React Native (Expo, WebView), Kafka.

**Reference spec:** [docs/superpowers/specs/2026-08-26-ni-shipping-fee-payment-design.md](../specs/2026-08-26-ni-shipping-fee-payment-design.md)

---

## Design note not in the spec: how "don't dispatch until paid" is actually implemented

The spec's D6 describes a `PendingPayment` shipment status. Investigation during planning found a materially simpler mechanism that produces the *exact same observable behavior* without touching the shared `logisticos_types::ShipmentStatus` enum (which is consumed by many services — a new variant would ripple through every exhaustive `match` on it platform-wide):

- `ShipmentService::create()` already builds and publishes three Kafka events at the end of a successful booking: `AwbIssued`, `ShipmentCreated` (the event `services/dispatch`'s `shipment_consumer.rs` subscribes to — this is the actual dispatch trigger), and `ShipmentConfirmed`. The row's `status` column stays `'pending'` regardless — a comment in the existing code already states this invariant.
- So: withholding those three event publications is sufficient to prevent dispatch from ever seeing an unpaid shipment. The shipment row itself is created immediately (AWB exists right away, matching the spec's intent), tagged with a new `payment_status = 'awaiting_payment'`, and the three already-built event payloads are stored verbatim in a new `pending_dispatch_events JSONB` column instead of being published.
- On `payment.intent_captured`, a new consumer re-publishes those exact stored payloads unchanged, and clears the column. On `payment.intent_failed`/expiry, the existing `ShipmentService::cancel()` method is called (no new cancellation path needed).

This is the same "value change, not a new branch" preference the codebase already states for itself in `checkout_service.rs`. Every task below implements this mechanism; where a task's wording differs from the spec's literal `PendingPayment` status, this note is the reconciling authority.

---

# Phase 0 — Shared foundations

### Task 1: Add `AED` to `Currency`

**Files:**
- Modify: `libs/types/src/lib.rs:105-111`

- [ ] **Step 1: Add the variant and Display arm**

Read the current enum and its `Display` impl first (`libs/types/src/lib.rs:105-120`ish) to find the exact match arms, then add `AED` alongside the existing five:

```rust
pub enum Currency {
    PHP,
    USD,
    SGD,
    MYR,
    IDR,
    AED,
}
```

And in the `impl std::fmt::Display for Currency` block, add the matching arm following the existing style for the other variants (e.g. `Currency::AED => write!(f, "AED"),`).

- [ ] **Step 2: Confirm it compiles across the workspace**

Run: `cargo check -p logisticos-types`
Expected: no errors (adding an enum variant is additive; it will NOT break existing exhaustive matches elsewhere unless one exists over `Currency` specifically — check with the next command).

Run: `cargo check --workspace 2>&1 | grep -i "non-exhaustive\|currency" `
Expected: no compile errors referencing `Currency` (this crate's enum is normally matched with a `_ =>` fallback wherever it's used for formatting).

- [ ] **Step 3: Commit**

```bash
git add libs/types/src/lib.rs
git commit -m "feat(types): add AED currency for AE-region tenants"
```

---

### Task 2: Add `currency` to JWT `Claims`

**Files:**
- Modify: `libs/auth/src/claims.rs`

- [ ] **Step 1: Add the field**

In the `Claims` struct (`libs/auth/src/claims.rs:8-59`), add a new field mirroring the existing `phone` field's `#[serde(default)]` pattern (old tokens must still deserialize):

```rust
    /// The tenant's billing currency (e.g. "AED", "PHP"), from `Tenant.currency`.
    /// Carried on the token so services that price or charge money don't need a
    /// cross-service call to identity per request. `None` for a draft tenant that
    /// hasn't finished onboarding, or a token minted before this field existed.
    #[serde(default)]
    pub currency: Option<String>,
```

Also add `currency: None` to the field list inside `Claims::new(...)` (`libs/auth/src/claims.rs:76-94`), next to the existing `phone: None,` line.

- [ ] **Step 2: Add the chainable builder**

Immediately after `with_phone` (`libs/auth/src/claims.rs:104-109`), add:

```rust
    /// Attach the tenant's billing currency. Chainable, like [`Self::with_phone`].
    #[must_use]
    pub fn with_currency(mut self, currency: Option<String>) -> Self {
        self.currency = currency;
        self
    }
```

- [ ] **Step 3: Write a unit test for round-trip (de)serialization of old tokens**

Add to the bottom of `libs/auth/src/claims.rs` (create a `#[cfg(test)] mod tests` block if none exists yet — check first with `grep -n "mod tests" libs/auth/src/claims.rs`):

```rust
#[cfg(test)]
mod claims_currency_tests {
    use super::*;

    #[test]
    fn old_token_json_without_currency_field_still_deserializes() {
        // Simulates a JWT minted before this field existed — no `currency` key at all.
        let json = r#"{
            "sub": "11111111-1111-1111-1111-111111111111",
            "iat": 0, "exp": 0, "jti": "x",
            "tenant_id": "11111111-1111-1111-1111-111111111111",
            "tenant_slug": "acme",
            "subscription_tier": "starter",
            "user_id": "11111111-1111-1111-1111-111111111111",
            "email": "a@b.com",
            "roles": [], "permissions": []
        }"#;
        let claims: Claims = serde_json::from_str(json).expect("must deserialize");
        assert_eq!(claims.currency, None);
    }

    #[test]
    fn with_currency_sets_the_field() {
        let claims = Claims::new(
            Uuid::new_v4(), Uuid::new_v4(), "acme".into(), "starter".into(),
            "a@b.com".into(), vec![], vec![], 3600,
        ).with_currency(Some("AED".into()));
        assert_eq!(claims.currency.as_deref(), Some("AED"));
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p logisticos-auth claims`
Expected: both new tests pass.

- [ ] **Step 5: Commit**

```bash
git add libs/auth/src/claims.rs
git commit -m "feat(auth): carry tenant currency on JWT claims"
```

---

### Task 3: Populate the `currency` claim at JWT mint time

**Files:**
- Modify: `services/identity/src/application/services/auth_service.rs:908-919`

- [ ] **Step 1: Chain `.with_currency(...)` onto the claims builder**

The `build_exchange_result` method builds claims like this today:

```rust
        let claims = Claims::new(
            user.id.inner(),
            tenant.id.inner(),
            tenant.slug.clone(),
            tier_str,
            user.email.clone(),
            user.roles.clone(),
            permissions,
            self.jwt.access_expiry_seconds(),
        )
        .with_onboarding(onboarding_required)
        .with_features(enabled_features).with_phone(user.phone_number.clone());
```

Change the last line to also chain `.with_currency(...)`:

```rust
        .with_onboarding(onboarding_required)
        .with_features(enabled_features)
        .with_phone(user.phone_number.clone())
        .with_currency(tenant.currency.clone());
```

(`Tenant.currency` is already `Option<String>` — see `services/identity/src/domain/entities/tenant.rs:46` — so no conversion is needed.)

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p logisticos-identity`
Expected: no errors.

- [ ] **Step 3: Add a service-level test**

Find the existing test module for `auth_service.rs` (`grep -n "mod tests" services/identity/src/application/services/auth_service.rs`) and add:

```rust
    #[tokio::test]
    async fn build_exchange_result_carries_tenant_currency_onto_the_jwt() {
        let (service, _, tenant_repo, user_repo) = make_test_auth_service(); // use whatever the file's existing test harness constructor is named — check the surrounding tests for the exact helper
        let mut tenant = make_active_tenant(); // reuse whichever existing test helper builds a Tenant fixture in this file
        tenant.currency = Some("AED".into());
        tenant_repo.insert(tenant.clone()).await;
        let user = make_test_user(tenant.id.inner()); // reuse the existing user fixture helper
        user_repo.insert(user.clone()).await;

        let result = service
            .build_exchange_result(&tenant, &user, vec![], false)
            .await
            .expect("exchange result");

        let claims = service.jwt.decode_access_token(&result.access_token).expect("decode");
        assert_eq!(claims.currency.as_deref(), Some("AED"));
    }
```

Adjust the fixture-helper names to whatever this file's existing tests actually call — read 2-3 of the neighboring tests in the same file first and match their exact setup pattern rather than inventing new helper names.

- [ ] **Step 4: Run it**

Run: `cargo test -p logisticos-identity auth_service`
Expected: the new test passes alongside the existing ones.

- [ ] **Step 5: Commit**

```bash
git add services/identity/src/application/services/auth_service.rs
git commit -m "feat(identity): stamp tenant currency onto minted JWTs"
```

---

# Phase 1 — payments: payment intents + Network International adapter

### Task 4: `payment_intents` table

**Files:**
- Create: `services/payments/migrations/0015_create_payment_intents.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Migration: 0015 — Payments: payment_intents (gateway-agnostic charge ledger)
--
-- `purpose` is intentionally an open TEXT value, not a fixed enum: this table
-- is shared by every future payment surface (subscription billing, storefront
-- checkout, truck & recovery booking), each adding its own purpose value
-- rather than a parallel table. Only 'shipping_fee' is used today.
--
-- No RLS: per migrations 0014 (identity) and 0011 (order-intake), RLS here
-- was found decorative (the connection pool never sets app.tenant_id) and is
-- not re-added.

CREATE TABLE payments.payment_intents (
    id                    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id             UUID        NOT NULL,
    purpose               TEXT        NOT NULL,
    reference_type        TEXT        NOT NULL,
    reference_id          UUID        NOT NULL,
    amount_cents          BIGINT      NOT NULL CHECK (amount_cents > 0),
    currency              TEXT        NOT NULL,
    status                TEXT        NOT NULL DEFAULT 'created'
                                      CHECK (status IN (
                                          'created','pending','captured','failed','refunded','expired'
                                      )),
    gateway               TEXT        NOT NULL,
    gateway_order_ref     TEXT,
    gateway_payment_ref   TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at            TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_payment_intents_reference
    ON payments.payment_intents (reference_type, reference_id);

-- Idempotent webhook capture: a replayed webhook for the same gateway
-- transaction must not create a second row or double-process.
CREATE UNIQUE INDEX idx_payment_intents_gateway_payment_ref
    ON payments.payment_intents (gateway_payment_ref)
    WHERE gateway_payment_ref IS NOT NULL;

CREATE INDEX idx_payment_intents_tenant_status
    ON payments.payment_intents (tenant_id, status);
```

- [ ] **Step 2: Run it locally against the payments DB**

Run: `cd services/payments && cargo sqlx migrate run` (or however this repo runs service migrations locally — check `services/payments/README.md` or fall back to letting `bootstrap::run()` apply it on next service start, which is what `logisticos_common::migrations::run(...)` in `bootstrap.rs:48` already does automatically).

- [ ] **Step 3: Commit**

```bash
git add services/payments/migrations/0015_create_payment_intents.sql
git commit -m "feat(payments): add payment_intents table"
```

---

### Task 5: `PaymentIntent` domain entity

**Files:**
- Create: `services/payments/src/domain/entities/payment_intent.rs`
- Modify: `services/payments/src/domain/entities/mod.rs`

- [ ] **Step 1: Write the entity with its state-transition methods**

```rust
//! PaymentIntent — a gateway-agnostic record of "charge this much, for this
//! reason, tied to this thing." One row exists per attempted online charge,
//! regardless of which gateway or which product surface initiated it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentIntentStatus {
    Created,
    Pending,
    Captured,
    Failed,
    Refunded,
    Expired,
}

impl PaymentIntentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created  => "created",
            Self::Pending  => "pending",
            Self::Captured => "captured",
            Self::Failed   => "failed",
            Self::Refunded => "refunded",
            Self::Expired  => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "created"  => Some(Self::Created),
            "pending"  => Some(Self::Pending),
            "captured" => Some(Self::Captured),
            "failed"   => Some(Self::Failed),
            "refunded" => Some(Self::Refunded),
            "expired"  => Some(Self::Expired),
            _          => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentIntent {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
    pub status: PaymentIntentStatus,
    pub gateway: String,
    pub gateway_order_ref: Option<String>,
    pub gateway_payment_ref: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl PaymentIntent {
    pub fn new(
        tenant_id: Uuid,
        purpose: impl Into<String>,
        reference_type: impl Into<String>,
        reference_id: Uuid,
        amount_cents: i64,
        currency: impl Into<String>,
        gateway: impl Into<String>,
        ttl: chrono::Duration,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            purpose: purpose.into(),
            reference_type: reference_type.into(),
            reference_id,
            amount_cents,
            currency: currency.into(),
            status: PaymentIntentStatus::Created,
            gateway: gateway.into(),
            gateway_order_ref: None,
            gateway_payment_ref: None,
            created_at: now,
            updated_at: now,
            expires_at: now + ttl,
        }
    }

    /// Attach the gateway's own session/order reference once the hosted
    /// checkout session has been created.
    pub fn with_gateway_order_ref(mut self, gateway_order_ref: String) -> Self {
        self.gateway_order_ref = Some(gateway_order_ref);
        self.status = PaymentIntentStatus::Pending;
        self.updated_at = Utc::now();
        self
    }

    /// Business rule: only a `Created`/`Pending` intent can be captured, and
    /// capture is idempotent — replaying the same `gateway_payment_ref` on an
    /// already-`Captured` intent is a no-op, not an error, since a webhook can
    /// legitimately be delivered more than once.
    pub fn capture(&mut self, gateway_payment_ref: String) -> Result<(), &'static str> {
        match self.status {
            PaymentIntentStatus::Captured => {
                if self.gateway_payment_ref.as_deref() == Some(gateway_payment_ref.as_str()) {
                    return Ok(()); // idempotent replay
                }
                return Err("Intent already captured under a different gateway reference");
            }
            PaymentIntentStatus::Created | PaymentIntentStatus::Pending => {}
            _ => return Err("Intent is not in a capturable state"),
        }
        self.status = PaymentIntentStatus::Captured;
        self.gateway_payment_ref = Some(gateway_payment_ref);
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn fail(&mut self) -> Result<(), &'static str> {
        match self.status {
            PaymentIntentStatus::Failed => return Ok(()), // idempotent
            PaymentIntentStatus::Captured | PaymentIntentStatus::Refunded => {
                return Err("Cannot fail an intent that already captured");
            }
            _ => {}
        }
        self.status = PaymentIntentStatus::Failed;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn expire(&mut self) -> Result<(), &'static str> {
        match self.status {
            PaymentIntentStatus::Created | PaymentIntentStatus::Pending => {
                self.status = PaymentIntentStatus::Expired;
                self.updated_at = Utc::now();
                Ok(())
            }
            PaymentIntentStatus::Expired => Ok(()), // idempotent
            _ => Err("Cannot expire an intent that already reached a final state"),
        }
    }

    pub fn refund(&mut self) -> Result<(), &'static str> {
        if self.status != PaymentIntentStatus::Captured {
            return Err("Only a captured intent can be refunded");
        }
        self.status = PaymentIntentStatus::Refunded;
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_intent() -> PaymentIntent {
        PaymentIntent::new(
            Uuid::new_v4(), "shipping_fee", "shipment", Uuid::new_v4(),
            5_000, "AED", "network_international", chrono::Duration::minutes(30),
        )
    }

    #[test]
    fn new_intent_starts_created() {
        let intent = make_intent();
        assert_eq!(intent.status, PaymentIntentStatus::Created);
        assert!(intent.gateway_payment_ref.is_none());
    }

    #[test]
    fn capture_transitions_to_captured_and_stores_the_reference() {
        let mut intent = make_intent();
        intent.capture("ni-txn-123".into()).unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Captured);
        assert_eq!(intent.gateway_payment_ref.as_deref(), Some("ni-txn-123"));
    }

    #[test]
    fn capture_is_idempotent_on_replay_of_the_same_reference() {
        let mut intent = make_intent();
        intent.capture("ni-txn-123".into()).unwrap();
        // A replayed webhook for the same transaction must not error.
        intent.capture("ni-txn-123".into()).unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Captured);
    }

    #[test]
    fn capture_rejects_a_conflicting_second_reference() {
        let mut intent = make_intent();
        intent.capture("ni-txn-123".into()).unwrap();
        assert!(intent.capture("ni-txn-999".into()).is_err());
    }

    #[test]
    fn fail_after_captured_is_rejected() {
        let mut intent = make_intent();
        intent.capture("ni-txn-123".into()).unwrap();
        assert!(intent.fail().is_err());
    }

    #[test]
    fn expire_only_applies_to_created_or_pending() {
        let mut intent = make_intent();
        intent.capture("ni-txn-123".into()).unwrap();
        assert!(intent.expire().is_err());
    }

    #[test]
    fn refund_requires_captured_state() {
        let mut intent = make_intent();
        assert!(intent.refund().is_err());
        intent.capture("ni-txn-123".into()).unwrap();
        intent.refund().unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Refunded);
    }
}
```

- [ ] **Step 2: Register the module**

Read `services/payments/src/domain/entities/mod.rs` first to match its existing `pub mod ...; pub use ...::...;` style, then add:

```rust
pub mod payment_intent;
pub use payment_intent::{PaymentIntent, PaymentIntentStatus};
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p logisticos-payments payment_intent`
Expected: all 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add services/payments/src/domain/entities/payment_intent.rs services/payments/src/domain/entities/mod.rs
git commit -m "feat(payments): add PaymentIntent domain entity with capture/fail/expire/refund"
```

---

### Task 6: `PaymentIntentRepository`

**Files:**
- Modify: `services/payments/src/domain/repositories/mod.rs`
- Create: `services/payments/src/infrastructure/db/payment_intent_repo.rs`
- Modify: `services/payments/src/infrastructure/db/mod.rs`

- [ ] **Step 1: Add the trait**

In `services/payments/src/domain/repositories/mod.rs`, following the exact `#[async_trait]` style already used by `InvoiceRepository` (see lines 1-36 of that file), add:

```rust
#[async_trait]
pub trait PaymentIntentRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<PaymentIntent>>;
    /// Idempotent capture lookup — used by the webhook handler to avoid
    /// creating a second record when NI redelivers the same transaction.
    async fn find_by_gateway_payment_ref(&self, gateway_payment_ref: &str) -> anyhow::Result<Option<PaymentIntent>>;
    async fn save(&self, intent: &PaymentIntent) -> anyhow::Result<()>;
    /// Intents past `expires_at` still in `created`/`pending` — the sweep target.
    async fn list_expired(&self, before: DateTime<Utc>) -> anyhow::Result<Vec<PaymentIntent>>;
}
```

Add `use crate::domain::entities::PaymentIntent;`, `use chrono::{DateTime, Utc};` to the file's existing import block if not already present.

- [ ] **Step 2: Write the Postgres implementation**

```rust
//! Postgres implementation of `PaymentIntentRepository`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{PaymentIntent, PaymentIntentStatus};
use crate::domain::repositories::PaymentIntentRepository;

pub struct PgPaymentIntentRepository {
    pub pool: PgPool,
}

impl PgPaymentIntentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_intent(row: &sqlx::postgres::PgRow) -> PaymentIntent {
    PaymentIntent {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        purpose: row.get("purpose"),
        reference_type: row.get("reference_type"),
        reference_id: row.get("reference_id"),
        amount_cents: row.get("amount_cents"),
        currency: row.get("currency"),
        status: PaymentIntentStatus::parse(row.get::<String, _>("status").as_str())
            .expect("status CHECK constraint guarantees a known value"),
        gateway: row.get("gateway"),
        gateway_order_ref: row.get("gateway_order_ref"),
        gateway_payment_ref: row.get("gateway_payment_ref"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        expires_at: row.get("expires_at"),
    }
}

const INTENT_COLS: &str = "id, tenant_id, purpose, reference_type, reference_id, \
    amount_cents, currency, status, gateway, gateway_order_ref, gateway_payment_ref, \
    created_at, updated_at, expires_at";

#[async_trait]
impl PaymentIntentRepository for PgPaymentIntentRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<PaymentIntent>> {
        let query = format!("SELECT {INTENT_COLS} FROM payments.payment_intents WHERE id = $1");
        let row = sqlx::query(&query).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.as_ref().map(row_to_intent))
    }

    async fn find_by_gateway_payment_ref(&self, gateway_payment_ref: &str) -> anyhow::Result<Option<PaymentIntent>> {
        let query = format!("SELECT {INTENT_COLS} FROM payments.payment_intents WHERE gateway_payment_ref = $1");
        let row = sqlx::query(&query).bind(gateway_payment_ref).fetch_optional(&self.pool).await?;
        Ok(row.as_ref().map(row_to_intent))
    }

    async fn save(&self, intent: &PaymentIntent) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO payments.payment_intents (
                id, tenant_id, purpose, reference_type, reference_id,
                amount_cents, currency, status, gateway, gateway_order_ref, gateway_payment_ref,
                created_at, updated_at, expires_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            ON CONFLICT (id) DO UPDATE SET
                status               = EXCLUDED.status,
                gateway_order_ref    = EXCLUDED.gateway_order_ref,
                gateway_payment_ref  = EXCLUDED.gateway_payment_ref,
                updated_at           = EXCLUDED.updated_at"#,
        )
        .bind(intent.id)
        .bind(intent.tenant_id)
        .bind(&intent.purpose)
        .bind(&intent.reference_type)
        .bind(intent.reference_id)
        .bind(intent.amount_cents)
        .bind(&intent.currency)
        .bind(intent.status.as_str())
        .bind(&intent.gateway)
        .bind(intent.gateway_order_ref.as_deref())
        .bind(intent.gateway_payment_ref.as_deref())
        .bind(intent.created_at)
        .bind(intent.updated_at)
        .bind(intent.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_expired(&self, before: DateTime<Utc>) -> anyhow::Result<Vec<PaymentIntent>> {
        let query = format!(
            "SELECT {INTENT_COLS} FROM payments.payment_intents \
             WHERE status IN ('created','pending') AND expires_at < $1"
        );
        let rows = sqlx::query(&query).bind(before).fetch_all(&self.pool).await?;
        Ok(rows.iter().map(row_to_intent).collect())
    }
}
```

- [ ] **Step 3: Register the module**

In `services/payments/src/infrastructure/db/mod.rs`, add `pub mod payment_intent_repo;` and re-export `PgPaymentIntentRepository` following whatever re-export style the file already uses for `PgInvoiceRepository` etc. (check the top of that file for the pattern before adding).

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p logisticos-payments`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add services/payments/src/domain/repositories/mod.rs services/payments/src/infrastructure/db/payment_intent_repo.rs services/payments/src/infrastructure/db/mod.rs
git commit -m "feat(payments): add PaymentIntentRepository and its Postgres implementation"
```

---

### Task 7: `PaymentGateway` trait + Network International adapter

**Files:**
- Create: `services/payments/src/domain/repositories/payment_gateway.rs`
- Modify: `services/payments/src/domain/repositories/mod.rs`
- Modify: `services/payments/src/config.rs`
- Create: `services/payments/src/infrastructure/external/network_international.rs`
- Modify: `services/payments/src/infrastructure/external/mod.rs`
- Modify: `services/payments/Cargo.toml`

- [ ] **Step 1: Write the gateway-agnostic trait**

```rust
//! `PaymentGateway` — the port every payment gateway adapter implements.
//! `services/payments` calls this; it never talks to a specific gateway's SDK
//! outside an `infrastructure/external/*` module.

use async_trait::async_trait;

pub struct CreateSessionRequest<'a> {
    pub amount_cents: i64,
    pub currency: &'a str,
    /// Our own `payment_intents.id` — passed through as the gateway's
    /// merchant-supplied order reference so the webhook can be matched back
    /// to a row without a database round trip keyed on anything gateway-issued.
    pub intent_id: uuid::Uuid,
    /// Where the gateway's hosted page redirects the customer's browser/WebView
    /// after payment. This is a UX signal only — never trusted as proof of payment.
    pub return_url: &'a str,
}

pub struct GatewaySession {
    pub checkout_url: String,
    pub gateway_order_ref: String,
}

/// The result of successfully verifying an inbound webhook payload.
pub enum WebhookEvent {
    Captured { gateway_order_ref: String, gateway_payment_ref: String },
    Failed { gateway_order_ref: String },
}

#[async_trait]
pub trait PaymentGateway: Send + Sync {
    async fn create_session(&self, req: CreateSessionRequest<'_>) -> anyhow::Result<GatewaySession>;

    /// Verifies the webhook's authenticity (signature check) and parses it.
    /// Returns `Err` for a payload that fails signature verification — the
    /// caller must never act on an unverified webhook.
    fn verify_webhook(&self, headers: &reqwest::header::HeaderMap, raw_body: &[u8]) -> anyhow::Result<WebhookEvent>;

    async fn refund(&self, gateway_payment_ref: &str, amount_cents: i64) -> anyhow::Result<()>;
}
```

- [ ] **Step 2: Register the module**

Add `pub mod payment_gateway;` and `pub use payment_gateway::{PaymentGateway, CreateSessionRequest, GatewaySession, WebhookEvent};` to `services/payments/src/domain/repositories/mod.rs`, matching the file's existing style.

- [ ] **Step 3: Add NI config**

In `services/payments/src/config.rs`, add a new config struct next to `OrderIntakeConfig` and register it on `Config`:

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct NetworkInternationalConfig {
    /// Base URL of NI's API — sandbox or production, set per environment.
    pub base_url: String,
    pub api_key: String,
    /// Shared secret used to verify inbound webhook signatures.
    pub webhook_secret: String,
    /// NI outlet reference this tenant's charges post against.
    pub outlet_ref: String,
}
```

Add `pub network_international: NetworkInternationalConfig,` to the `Config` struct's field list. Env vars will be `NETWORK_INTERNATIONAL__BASE_URL`, `NETWORK_INTERNATIONAL__API_KEY`, `NETWORK_INTERNATIONAL__WEBHOOK_SECRET`, `NETWORK_INTERNATIONAL__OUTLET_REF` (matching the `__`-separator convention already used by `AUTH__JWT_SECRET` etc.) — document these in a comment above the struct.

- [ ] **Step 4: Add the `hmac`/`sha2`/`subtle`/`base64` dependencies**

These are already pinned at the workspace level (`Cargo.toml:98-101`) but not yet listed under `services/payments/Cargo.toml`'s `[dependencies]`. Add:

```toml
hmac.workspace   = true
sha2.workspace   = true
subtle.workspace = true
base64.workspace = true
```

- [ ] **Step 5: Write the adapter**

```rust
//! Network International (NI) hosted-checkout adapter.
//!
//! LogisticOS never collects card data directly — the customer is redirected
//! to NI's own hosted payment page. This keeps `services/payments` at PCI
//! SAQ-A instead of pulling it into full PCI scope.
//!
//! NOTE: the exact request/response JSON shapes and webhook signature header
//! name below follow NI's (N-Genius) publicly documented hosted-order pattern
//! as of this writing. Confirm field names and the signature scheme against
//! NI's live API/sandbox docs during integration testing before going live —
//! this was an explicit, stated boundary in the design spec: it fixes the
//! contract `services/payments` exposes to the rest of the platform, not NI's
//! wire format, which is verified against the real sandbox in Task 9.

use async_trait::async_trait;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::config::NetworkInternationalConfig;
use crate::domain::repositories::payment_gateway::{
    CreateSessionRequest, GatewaySession, PaymentGateway, WebhookEvent,
};

type HmacSha256 = Hmac<Sha256>;

pub struct NetworkInternationalGateway {
    cfg: NetworkInternationalConfig,
    http: reqwest::Client,
}

impl NetworkInternationalGateway {
    pub fn new(cfg: NetworkInternationalConfig) -> Self {
        Self { cfg, http: reqwest::Client::new() }
    }
}

#[derive(Serialize)]
struct CreateOrderRequest<'a> {
    action: &'a str,
    amount: OrderAmount<'a>,
    merchant_order_reference: String,
    merchant_attributes: MerchantAttributes<'a>,
}

#[derive(Serialize)]
struct OrderAmount<'a> {
    #[serde(rename = "currencyCode")]
    currency_code: &'a str,
    value: i64,
}

#[derive(Serialize)]
struct MerchantAttributes<'a> {
    #[serde(rename = "redirectUrl")]
    redirect_url: &'a str,
}

#[derive(Deserialize)]
struct CreateOrderResponse {
    reference: String,
    #[serde(rename = "_links")]
    links: OrderLinks,
}

#[derive(Deserialize)]
struct OrderLinks {
    payment: LinkHref,
}

#[derive(Deserialize)]
struct LinkHref {
    href: String,
}

#[async_trait]
impl PaymentGateway for NetworkInternationalGateway {
    async fn create_session(&self, req: CreateSessionRequest<'_>) -> anyhow::Result<GatewaySession> {
        let url = format!(
            "{}/transactions/outlets/{}/orders",
            self.cfg.base_url.trim_end_matches('/'),
            self.cfg.outlet_ref,
        );
        let body = CreateOrderRequest {
            action: "SALE",
            amount: OrderAmount { currency_code: req.currency, value: req.amount_cents },
            merchant_order_reference: req.intent_id.to_string(),
            merchant_attributes: MerchantAttributes { redirect_url: req.return_url },
        };
        let resp = self.http
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<CreateOrderResponse>()
            .await?;

        Ok(GatewaySession {
            checkout_url: resp.links.payment.href,
            gateway_order_ref: resp.reference,
        })
    }

    fn verify_webhook(&self, headers: &reqwest::header::HeaderMap, raw_body: &[u8]) -> anyhow::Result<WebhookEvent> {
        let signature_b64 = headers
            .get("x-ni-signature")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow::anyhow!("missing webhook signature header"))?;

        let expected = base64::engine::general_purpose::STANDARD
            .decode(signature_b64)
            .map_err(|_| anyhow::anyhow!("malformed webhook signature"))?;

        let mut mac = HmacSha256::new_from_slice(self.cfg.webhook_secret.as_bytes())
            .map_err(|_| anyhow::anyhow!("invalid webhook secret length"))?;
        mac.update(raw_body);
        let computed = mac.finalize().into_bytes();

        if computed.as_slice().ct_eq(&expected).unwrap_u8() != 1 {
            anyhow::bail!("webhook signature verification failed");
        }

        let payload: WebhookPayload = serde_json::from_slice(raw_body)?;
        Ok(match payload.status.as_str() {
            "CAPTURED" | "AUTHORISED" => WebhookEvent::Captured {
                gateway_order_ref: payload.order_reference,
                gateway_payment_ref: payload.transaction_reference,
            },
            _ => WebhookEvent::Failed { gateway_order_ref: payload.order_reference },
        })
    }

    async fn refund(&self, gateway_payment_ref: &str, amount_cents: i64) -> anyhow::Result<()> {
        let url = format!(
            "{}/transactions/{}/refund",
            self.cfg.base_url.trim_end_matches('/'),
            gateway_payment_ref,
        );
        self.http
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&serde_json::json!({ "amount": { "value": amount_cents } }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

#[derive(Deserialize)]
struct WebhookPayload {
    status: String,
    #[serde(rename = "orderReference")]
    order_reference: String,
    #[serde(rename = "transactionReference")]
    transaction_reference: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_gateway() -> NetworkInternationalGateway {
        NetworkInternationalGateway::new(NetworkInternationalConfig {
            base_url: "https://example.invalid".into(),
            api_key: "test-key".into(),
            webhook_secret: "test-secret".into(),
            outlet_ref: "outlet-1".into(),
        })
    }

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    #[test]
    fn verify_webhook_accepts_a_correctly_signed_captured_payload() {
        let gateway = test_gateway();
        let body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let event = gateway.verify_webhook(&headers, body).expect("must verify");
        match event {
            WebhookEvent::Captured { gateway_order_ref, gateway_payment_ref } => {
                assert_eq!(gateway_order_ref, "ord-1");
                assert_eq!(gateway_payment_ref, "txn-1");
            }
            _ => panic!("expected Captured"),
        }
    }

    #[test]
    fn verify_webhook_rejects_a_tampered_body() {
        let gateway = test_gateway();
        let signed_body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        let sig = sign("test-secret", signed_body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let tampered = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-EVIL"}"#;
        assert!(gateway.verify_webhook(&headers, tampered).is_err());
    }

    #[test]
    fn verify_webhook_rejects_a_missing_signature_header() {
        let gateway = test_gateway();
        let headers = reqwest::header::HeaderMap::new();
        let body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        assert!(gateway.verify_webhook(&headers, body).is_err());
    }

    #[test]
    fn verify_webhook_maps_a_non_captured_status_to_failed() {
        let gateway = test_gateway();
        let body = br#"{"status":"DECLINED","orderReference":"ord-2","transactionReference":"txn-2"}"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        match gateway.verify_webhook(&headers, body).expect("must verify") {
            WebhookEvent::Failed { gateway_order_ref } => assert_eq!(gateway_order_ref, "ord-2"),
            _ => panic!("expected Failed"),
        }
    }
}
```

- [ ] **Step 6: Replace the placeholder comment**

`services/payments/src/infrastructure/external/mod.rs` currently reads:

```rust
// Payment gateway adapters: PayMongo, GCash, Maya (future).
// PayMongo integration for card payments and GCash e-wallet withdrawals.
```

Replace its contents with:

```rust
// Payment gateway adapters: PayMongo, GCash, Maya (future).
// PayMongo integration for card payments and GCash e-wallet withdrawals.

pub mod network_international;
pub use network_international::NetworkInternationalGateway;
```

- [ ] **Step 7: Run the tests**

Run: `cargo test -p logisticos-payments network_international`
Expected: all 4 webhook-verification tests pass.

- [ ] **Step 8: Commit**

```bash
git add services/payments/src/domain/repositories/payment_gateway.rs services/payments/src/domain/repositories/mod.rs services/payments/src/config.rs services/payments/src/infrastructure/external/network_international.rs services/payments/src/infrastructure/external/mod.rs services/payments/Cargo.toml
git commit -m "feat(payments): add PaymentGateway trait and Network International adapter"
```

---

### Task 8: New Kafka topics and payload types

**Files:**
- Modify: `libs/events/src/topics.rs`
- Modify: `libs/events/src/payloads.rs`

- [ ] **Step 1: Add the topic constants**

In `libs/events/src/topics.rs`, in the `// Payments` section (after `WALLET_WITHDRAWAL_REJECTED` at line 84):

```rust
pub const PAYMENT_INTENT_CAPTURED:        &str = "logisticos.payments.intent.captured";
pub const PAYMENT_INTENT_FAILED:          &str = "logisticos.payments.intent.failed";
```

Add both to the `all_topics_are_lowercase_dot_separated` test's `topics` array (around line 152-154, alongside `WALLET_WITHDRAWAL_DISBURSED, WALLET_WITHDRAWAL_REJECTED`).

- [ ] **Step 2: Add the payload structs**

In `libs/events/src/payloads.rs`, following the style of the existing `PickupCaptured` struct, add:

```rust
/// Published by `services/payments` when a Network-International-backed
/// `payment_intents` row transitions to `captured`. `reference_id` is
/// whatever the intent's `purpose` says it is — for `purpose = "shipping_fee"`
/// it is the order-intake `shipment_id`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PaymentIntentCaptured {
    pub intent_id: uuid::Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: uuid::Uuid,
    pub amount_cents: i64,
    pub currency: String,
}

/// Published when an intent is marked `failed` (declined at the gateway) —
/// distinct from `expired`, which the consumer treats identically but which
/// is driven by the sweep, not a webhook.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PaymentIntentFailed {
    pub intent_id: uuid::Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: uuid::Uuid,
    pub reason: String,
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p logisticos-events`
Expected: no errors.

Run: `cargo test -p logisticos-events topics`
Expected: `all_topics_are_lowercase_dot_separated` still passes with the two new entries.

- [ ] **Step 4: Commit**

```bash
git add libs/events/src/topics.rs libs/events/src/payloads.rs
git commit -m "feat(events): add payment intent captured/failed topics and payloads"
```

---

### Task 9: `PaymentIntentService` (application layer)

**Files:**
- Create: `services/payments/src/application/services/payment_intent_service.rs`
- Modify: `services/payments/src/application/services/mod.rs`

- [ ] **Step 1: Write the service**

```rust
//! PaymentIntentService — orchestrates creating a gateway session, and
//! transitioning an intent on webhook capture/failure or sweep expiry,
//! publishing the corresponding Kafka event each time.

use std::sync::Arc;

use chrono::Duration;
use logisticos_events::{envelope::Event, payloads::{PaymentIntentCaptured, PaymentIntentFailed}, producer::KafkaProducer, topics};
use uuid::Uuid;

use crate::domain::entities::PaymentIntent;
use crate::domain::repositories::{
    payment_gateway::{CreateSessionRequest, PaymentGateway, WebhookEvent},
    PaymentIntentRepository,
};

/// Hosted-checkout sessions must be completed within this window before the
/// sweep expires them — matches the spec's stated 30-minute figure.
pub const INTENT_TTL: Duration = Duration::minutes(30);

pub struct PaymentIntentService {
    repo: Arc<dyn PaymentIntentRepository>,
    gateway: Arc<dyn PaymentGateway>,
    kafka: Arc<KafkaProducer>,
}

pub struct CreateIntentCommand {
    pub tenant_id: Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
    pub return_url: String,
}

pub struct CreatedIntent {
    pub intent_id: Uuid,
    pub checkout_url: String,
}

impl PaymentIntentService {
    pub fn new(
        repo: Arc<dyn PaymentIntentRepository>,
        gateway: Arc<dyn PaymentGateway>,
        kafka: Arc<KafkaProducer>,
    ) -> Self {
        Self { repo, gateway, kafka }
    }

    pub async fn create_intent(&self, cmd: CreateIntentCommand) -> anyhow::Result<CreatedIntent> {
        let intent = PaymentIntent::new(
            cmd.tenant_id,
            &cmd.purpose,
            &cmd.reference_type,
            cmd.reference_id,
            cmd.amount_cents,
            &cmd.currency,
            "network_international",
            INTENT_TTL,
        );
        self.repo.save(&intent).await?;

        let session = self.gateway.create_session(CreateSessionRequest {
            amount_cents: cmd.amount_cents,
            currency: &cmd.currency,
            intent_id: intent.id,
            return_url: &cmd.return_url,
        }).await?;

        let intent = intent.with_gateway_order_ref(session.gateway_order_ref);
        self.repo.save(&intent).await?;

        Ok(CreatedIntent { intent_id: intent.id, checkout_url: session.checkout_url })
    }

    /// Verifies and applies a webhook payload — the only path by which an
    /// intent can reach `captured`.
    pub async fn handle_webhook(&self, headers: &reqwest::header::HeaderMap, raw_body: &[u8]) -> anyhow::Result<()> {
        let event = self.gateway.verify_webhook(headers, raw_body)?;
        match event {
            WebhookEvent::Captured { gateway_order_ref, gateway_payment_ref } => {
                self.apply_captured(&gateway_order_ref, &gateway_payment_ref).await
            }
            WebhookEvent::Failed { gateway_order_ref } => {
                self.apply_failed(&gateway_order_ref, "gateway_declined").await
            }
        }
    }

    async fn find_by_order_ref(&self, gateway_order_ref: &str) -> anyhow::Result<PaymentIntent> {
        // gateway_order_ref is not separately indexed (it's 1:1 with the intent
        // we minted it for, always looked up right after creation in practice);
        // for the webhook path we instead re-derive by trying the payment ref
        // first, then fall back to a full scan-free path: NI's merchant_order_reference
        // IS our intent_id (see network_international.rs::create_session), so the
        // gateway_order_ref parameter here is actually the intent id round-tripped.
        let intent_id: Uuid = gateway_order_ref.parse()
            .map_err(|_| anyhow::anyhow!("webhook order reference is not a valid intent id"))?;
        self.repo.find_by_id(intent_id).await?
            .ok_or_else(|| anyhow::anyhow!("no payment_intent found for id {intent_id}"))
    }

    async fn apply_captured(&self, gateway_order_ref: &str, gateway_payment_ref: &str) -> anyhow::Result<()> {
        // Idempotency: a replay of the same transaction reference is a no-op.
        if let Some(existing) = self.repo.find_by_gateway_payment_ref(gateway_payment_ref).await? {
            if existing.status == crate::domain::entities::PaymentIntentStatus::Captured {
                return Ok(());
            }
        }

        let mut intent = self.find_by_order_ref(gateway_order_ref).await?;
        intent.capture(gateway_payment_ref.to_string())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.repo.save(&intent).await?;

        let evt = Event::new(
            "logisticos/payments",
            "payment.intent.captured",
            intent.tenant_id,
            PaymentIntentCaptured {
                intent_id: intent.id,
                purpose: intent.purpose.clone(),
                reference_type: intent.reference_type.clone(),
                reference_id: intent.reference_id,
                amount_cents: intent.amount_cents,
                currency: intent.currency.clone(),
            },
        );
        self.kafka.publish_event(topics::PAYMENT_INTENT_CAPTURED, &evt).await?;
        Ok(())
    }

    async fn apply_failed(&self, gateway_order_ref: &str, reason: &str) -> anyhow::Result<()> {
        let mut intent = self.find_by_order_ref(gateway_order_ref).await?;
        intent.fail().map_err(|e| anyhow::anyhow!("{e}"))?;
        self.repo.save(&intent).await?;

        let evt = Event::new(
            "logisticos/payments",
            "payment.intent.failed",
            intent.tenant_id,
            PaymentIntentFailed {
                intent_id: intent.id,
                purpose: intent.purpose.clone(),
                reference_type: intent.reference_type.clone(),
                reference_id: intent.reference_id,
                reason: reason.to_string(),
            },
        );
        self.kafka.publish_event(topics::PAYMENT_INTENT_FAILED, &evt).await?;
        Ok(())
    }

    /// Called by the periodic sweep in `bootstrap.rs`. Expires every
    /// `created`/`pending` intent past its TTL and publishes the same
    /// `payment.intent.failed` event a declined payment would — order-intake's
    /// consumer treats both identically (cancel the shipment).
    pub async fn sweep_expired(&self) -> anyhow::Result<usize> {
        let expired = self.repo.list_expired(chrono::Utc::now()).await?;
        let count = expired.len();
        for mut intent in expired {
            if intent.expire().is_err() {
                continue; // raced with a webhook that captured it — leave it alone
            }
            self.repo.save(&intent).await?;
            let evt = Event::new(
                "logisticos/payments",
                "payment.intent.failed",
                intent.tenant_id,
                PaymentIntentFailed {
                    intent_id: intent.id,
                    purpose: intent.purpose.clone(),
                    reference_type: intent.reference_type.clone(),
                    reference_id: intent.reference_id,
                    reason: "expired".into(),
                },
            );
            if let Err(e) = self.kafka.publish_event(topics::PAYMENT_INTENT_FAILED, &evt).await {
                tracing::warn!(intent_id = %intent.id, error = %e, "failed to publish expiry event (will retry next sweep tick — intent stays expired)");
            }
        }
        Ok(count)
    }

    pub async fn refund(&self, intent_id: Uuid) -> anyhow::Result<()> {
        let mut intent = self.repo.find_by_id(intent_id).await?
            .ok_or_else(|| anyhow::anyhow!("no payment_intent {intent_id}"))?;
        let gateway_payment_ref = intent.gateway_payment_ref.clone()
            .ok_or_else(|| anyhow::anyhow!("intent {intent_id} has no captured payment to refund"))?;
        self.gateway.refund(&gateway_payment_ref, intent.amount_cents).await?;
        intent.refund().map_err(|e| anyhow::anyhow!("{e}"))?;
        self.repo.save(&intent).await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Register the module**

Add `pub mod payment_intent_service;` and `pub use payment_intent_service::PaymentIntentService;` to `services/payments/src/application/services/mod.rs`, matching its existing style (check the file first — it currently re-exports `BillingAggregationService, CodRemittanceService, CodService, InvoiceService, WalletService`, per `bootstrap.rs:6-9`).

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p logisticos-payments`
Expected: no errors. (No unit tests here — this service is a thin orchestration layer over already-tested `PaymentIntent` transitions and a mocked gateway; its behavior is exercised by the HTTP/webhook integration test in Task 11.)

- [ ] **Step 4: Commit**

```bash
git add services/payments/src/application/services/payment_intent_service.rs services/payments/src/application/services/mod.rs
git commit -m "feat(payments): add PaymentIntentService orchestration layer"
```

---

### Task 10: Internal intents endpoint + public webhook endpoint

**Files:**
- Create: `services/payments/src/api/http/payment_intents.rs`
- Create: `services/payments/src/api/http/payment_webhooks.rs`
- Modify: `services/payments/src/api/http/mod.rs`

- [ ] **Step 1: Write the internal handler**

```rust
//! POST /v1/internal/payments/intents — mesh-internal only (Istio mTLS gates
//! caller identity, same as every other route under /v1/internal). Callable
//! by order-intake to create a payment session for an amount order-intake has
//! already priced and verified — payments trusts the caller's amount here
//! specifically because this route is unreachable from any tenant-facing
//! credential, per the design spec's D3.

use axum::{extract::State, response::{IntoResponse, Json}, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use logisticos_errors::AppError;

use crate::api::http::AppState;
use crate::application::services::payment_intent_service::CreateIntentCommand;

#[derive(Deserialize)]
pub struct CreateIntentRequest {
    pub tenant_id: Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
    pub return_url: String,
}

#[derive(Serialize)]
pub struct CreateIntentResponse {
    pub intent_id: Uuid,
    pub checkout_url: String,
}

pub async fn create_intent(
    State(s): State<Arc<AppState>>,
    Json(req): Json<CreateIntentRequest>,
) -> impl IntoResponse {
    let result = s.payment_intent_service.create_intent(CreateIntentCommand {
        tenant_id: req.tenant_id,
        purpose: req.purpose,
        reference_type: req.reference_type,
        reference_id: req.reference_id,
        amount_cents: req.amount_cents,
        currency: req.currency,
        return_url: req.return_url,
    }).await;

    match result {
        Ok(created) => Ok::<_, AppError>((
            StatusCode::CREATED,
            Json(CreateIntentResponse { intent_id: created.intent_id, checkout_url: created.checkout_url }),
        )),
        Err(e) => {
            tracing::error!(error = ?e, "create_intent failed");
            Err(AppError::Internal(e))
        }
    }
}
```

- [ ] **Step 2: Write the public webhook handler**

```rust
//! POST /v1/payments/webhooks/network-international — public, no JWT (the
//! gateway cannot hold a LogisticOS session). Authenticated instead by NI's
//! own webhook signature, verified inside `PaymentIntentService::handle_webhook`.
//! State changes only happen after that verification succeeds.

use axum::{extract::State, response::IntoResponse, http::{HeaderMap, StatusCode}, body::Bytes};
use std::sync::Arc;

use crate::api::http::AppState;

pub async fn network_international_webhook(
    State(s): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    match s.payment_intent_service.handle_webhook(&headers, &body).await {
        Ok(()) => StatusCode::OK,
        Err(e) => {
            // Signature failures and processing errors both return 4xx/5xx so
            // NI's own retry policy redelivers — never silently swallow a webhook.
            tracing::warn!(error = %e, "network_international_webhook rejected or failed");
            StatusCode::BAD_REQUEST
        }
    }
}
```

- [ ] **Step 3: Wire both routes and extend `AppState`**

In `services/payments/src/api/http/mod.rs`:

- Add `pub mod payment_intents;` and `pub mod payment_webhooks;` to the top module list.
- Add to `AppState`:
  ```rust
      pub payment_intent_service: Arc<crate::application::services::payment_intent_service::PaymentIntentService>,
  ```
- Add the public webhook route directly on the top-level `router()` (it must NOT sit behind `auth_layer` or mTLS-only `/v1/internal` — it needs to be reachable from the public internet where NI's servers live):
  ```rust
  pub fn router(state: Arc<AppState>) -> Router {
      Router::new()
          .route("/health", get(health::health))
          .route("/ready",  get(health::ready))
          .route("/v1/payments/webhooks/network-international", post(payment_webhooks::network_international_webhook))
          .nest("/v1/internal", internal_router(state.clone()))
          .nest("/v1", protected_router(state.clone()))
          .with_state(state)
  }
  ```
- Add the internal route inside `internal_router`:
  ```rust
      .route("/payments/intents", post(payment_intents::create_intent))
  ```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p logisticos-payments`
Expected: fails until Task 11 wires `payment_intent_service` into the actual `AppState` construction in `bootstrap.rs` — that's fine, do Task 11 immediately after this one before running the check.

- [ ] **Step 5: Commit** (after Task 11's bootstrap wiring makes it compile — see that task's own commit step; do not commit Task 10 in isolation if it leaves the crate non-compiling. If you are executing tasks strictly in order, skip this step here and let Task 11's commit include both.)

---

### Task 11: Wire it all into `bootstrap.rs`

**Files:**
- Modify: `services/payments/src/bootstrap.rs`

- [ ] **Step 1: Construct the new dependencies**

After the existing repo constructions (around `bootstrap.rs:72`, right after `driver_ledger_repo`), add:

```rust
    let payment_intent_repo = Arc::new(
        crate::infrastructure::db::payment_intent_repo::PgPaymentIntentRepository::new(pool.clone())
    );
    let ni_gateway = Arc::new(
        crate::infrastructure::external::NetworkInternationalGateway::new(cfg.network_international.clone())
    );
```

After the existing service constructions (around `bootstrap.rs:107-111`, right after `billing_service`), add:

```rust
    let payment_intent_service = Arc::new(
        crate::application::services::payment_intent_service::PaymentIntentService::new(
            Arc::clone(&payment_intent_repo) as _,
            Arc::clone(&ni_gateway) as _,
            Arc::clone(&kafka),
        )
    );
```

- [ ] **Step 2: Add it to the `AppState` construction**

In the `let state = Arc::new(AppState { ... })` block (`bootstrap.rs:126-139`), add:

```rust
        payment_intent_service:            Arc::clone(&payment_intent_service),
```

- [ ] **Step 3: Add the expiry sweep**

After the existing "Nightly COD auto-batching" `tokio::spawn` block (`bootstrap.rs:226-243`), add a new spawned loop following the exact same `tokio::time::interval` shape:

```rust
    // Payment-intent expiry sweep — every 5 minutes, expire any `created`/
    // `pending` intent past its TTL. Publishes payment.intent.failed so
    // order-intake's consumer cancels the shipment the same way a declined
    // card would. Deliberately more frequent than the 30-minute TTL itself so
    // a customer never waits much longer than the TTL to see the cancellation.
    let intent_svc_for_sweep = Arc::clone(&payment_intent_service);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(5 * 60));
        loop {
            tick.tick().await;
            match intent_svc_for_sweep.sweep_expired().await {
                Ok(count) if count > 0 => tracing::info!(count, "Payment intent sweep: expired stale intents"),
                Ok(_) => {}
                Err(e) => tracing::error!(err = %e, "Payment intent sweep failed"),
            }
        }
    });
```

- [ ] **Step 4: Verify the whole service compiles**

Run: `cargo check -p logisticos-payments`
Expected: no errors.

- [ ] **Step 5: Add the env vars to the local dev env file**

Find whatever `.env`/`docker-compose.yml` entry configures `services/payments` locally (check `docker-compose.yml` for the `payments` service's `environment:` block) and add, matching the existing `AUTH__JWT_SECRET`-style naming:

```
NETWORK_INTERNATIONAL__BASE_URL=https://api-gateway.sandbox.ngenius-payments.com
NETWORK_INTERNATIONAL__API_KEY=<sandbox-key>
NETWORK_INTERNATIONAL__WEBHOOK_SECRET=<sandbox-webhook-secret>
NETWORK_INTERNATIONAL__OUTLET_REF=<sandbox-outlet-ref>
```

- [ ] **Step 6: Commit (covers Tasks 10 and 11 together, since Task 10 alone doesn't compile)**

```bash
git add services/payments/src/api/http/payment_intents.rs services/payments/src/api/http/payment_webhooks.rs services/payments/src/api/http/mod.rs services/payments/src/bootstrap.rs docker-compose.yml
git commit -m "feat(payments): wire payment intent HTTP routes, webhook, and expiry sweep"
```

---

# Phase 2 — order-intake: AE tariff, quote token, quote endpoint

### Task 12: AE tariff calculation

**Files:**
- Modify: `services/order-intake/src/domain/entities/shipment.rs`

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` block in `services/order-intake/src/domain/entities/shipment.rs` (append after the existing tests, reusing whatever `make_awb`/fixture helpers are already defined there — read the existing tests first, around line 122-140, to match the construction pattern exactly):

```rust
    #[test]
    fn ae_tariff_standard_matches_base_plus_surcharge() {
        let mut s = make_test_shipment(); // reuse the file's existing shipment fixture helper — check its exact name in the surrounding tests
        s.service_type = ServiceType::Standard;
        s.weight = ShipmentWeight::from_grams(1_500); // 1.5kg → 1 surcharge step over 1kg
        let fee = s.compute_base_fee_aed();
        // AED 20.00 base + 1 step * AED 2.00 = AED 22.00
        assert_eq!(fee.amount, 2_200);
        assert_eq!(fee.currency, Currency::AED);
    }

    #[test]
    fn ae_tariff_balikbayan_prices_per_piece_with_overweight_surcharge() {
        let mut s = make_test_shipment();
        s.service_type = ServiceType::Balikbayan;
        let pieces = vec![
            make_test_piece(20_000), // 20kg — no surcharge (<=25kg)
            make_test_piece(27_000), // 27kg — 4 steps of 0.5kg over 25kg
        ];
        let fee = s.compute_base_fee_aed_with_pieces(&pieces);
        // box1: 120.00, box2: 120.00 + 4*5.00 = 140.00 → total 260.00
        assert_eq!(fee.amount, 26_000);
        assert_eq!(fee.currency, Currency::AED);
    }
```

If the file has no existing `make_test_piece`-style helper, add a small one near the top of the test module:

```rust
    fn make_test_piece(weight_grams: u32) -> super::super::piece::Piece {
        // adjust field names to whatever Piece actually requires — check
        // domain/entities/piece.rs before writing this; the shape must match
        // the real struct exactly, this is illustrative of intent only until
        // you've confirmed the real fields
    }
```

Read `services/order-intake/src/domain/entities/piece.rs` before writing this helper — copy the exact field list `Piece` requires (already seen once in `shipment_service.rs`'s piece-construction block: `id, shipment_id, piece_number, piece_awb, declared_weight, actual_weight, dimensions, description, status, last_hub_id, last_scanned_at, created_at, updated_at`) and construct a minimal fixture with reasonable dummy values for the fields the test doesn't care about.

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test -p logisticos-order-intake ae_tariff`
Expected: FAIL — `compute_base_fee_aed` doesn't exist yet.

- [ ] **Step 3: Implement the AE tariff**

Add next to `compute_base_fee_with_pieces` (`services/order-intake/src/domain/entities/shipment.rs:100-118`):

```rust
    /// AE-region tariff (AED-denominated), used only for tenants whose JWT
    /// claims carry `currency == "AED"`. A first-cut rate table pending
    /// business/finance sign-off — same status the existing PHP table had
    /// when it shipped; both are fully functional, not placeholders.
    pub fn compute_base_fee_aed(&self) -> Money {
        use logisticos_types::Currency;
        let base = match self.service_type {
            ServiceType::Standard      => 2_000,   // AED 20.00
            ServiceType::Express       => 3_500,   // AED 35.00
            ServiceType::SameDay       => 4_500,   // AED 45.00
            ServiceType::Balikbayan    => 12_000,  // AED 120.00 per box (fallback)
            ServiceType::International => 15_000,  // AED 150.00 per box (fallback)
        };
        let weight_kg = self.weight.grams as f64 / 1000.0;
        let surcharge = if weight_kg > 1.0 {
            ((weight_kg - 1.0) / 0.5).ceil() as i64 * 200 // AED 2.00 per 0.5kg over 1kg
        } else {
            0
        };
        Money::new(base + surcharge, Currency::AED)
    }

    /// AE-region equivalent of `compute_base_fee_with_pieces`.
    pub fn compute_base_fee_aed_with_pieces(&self, pieces: &[super::piece::Piece]) -> logisticos_types::Money {
        use logisticos_types::Currency;
        match self.service_type {
            ServiceType::Balikbayan | ServiceType::International if !pieces.is_empty() => {
                let total: i64 = pieces.iter().map(|p| {
                    let kg = p.billable_weight_grams() as f64 / 1000.0;
                    let surcharge = if kg > 25.0 {
                        ((kg - 25.0) / 0.5).ceil() as i64 * 500 // AED 5.00 per 0.5kg over 25kg
                    } else {
                        0
                    };
                    12_000i64 + surcharge
                }).sum();
                logisticos_types::Money::new(total, Currency::AED)
            }
            _ => self.compute_base_fee_aed(),
        }
    }
```

- [ ] **Step 4: Run the tests again**

Run: `cargo test -p logisticos-order-intake ae_tariff`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add services/order-intake/src/domain/entities/shipment.rs
git commit -m "feat(order-intake): add AE-region AED tariff calculation"
```

---

### Task 13: Signed quote token

**Files:**
- Create: `services/order-intake/src/domain/value_objects/quote_token.rs`
- Modify: `services/order-intake/src/domain/value_objects/mod.rs`
- Modify: `services/order-intake/Cargo.toml`
- Modify: `services/order-intake/src/config.rs`

- [ ] **Step 1: Add the crypto dependencies**

Read `services/order-intake/src/config.rs` first to find its existing `Config`/env-loading pattern (same `config::Config::builder()...Environment::default().separator("__")` shape as `services/payments/src/config.rs`, already read in Task 7). Add a `quote_token_secret: String` field to whatever top-level config struct holds simple app-wide secrets there (if none exists, add a small `pub app: AppConfig` field the way `services/payments` has one — check first).

In `services/order-intake/Cargo.toml`, add under `[dependencies]`:

```toml
hmac.workspace   = true
sha2.workspace   = true
subtle.workspace = true
base64.workspace = true
```

- [ ] **Step 2: Write the failing tests**

```rust
//! Signed, short-TTL quote token. `POST /v1/shipments/quote` mints one;
//! `POST /v1/shipments` re-verifies one before trusting its amount to charge.
//! No database row is created for a quote nobody completes.

use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuoteTokenPayload {
    pub tenant_id: Uuid,
    pub service_type: String,
    pub weight_grams: u32,
    pub amount_cents: i64,
    pub currency: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum QuoteTokenError {
    #[error("malformed quote token")]
    Malformed,
    #[error("quote token signature is invalid")]
    BadSignature,
    #[error("quote token has expired")]
    Expired,
}

/// Sign a payload into `base64(json).base64(hmac-sha256)`.
pub fn sign(secret: &[u8], payload: &QuoteTokenPayload) -> String {
    let json = serde_json::to_vec(payload).expect("QuoteTokenPayload always serializes");
    let json_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&json);

    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(json_b64.as_bytes());
    let sig_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    format!("{json_b64}.{sig_b64}")
}

/// Verify a token's signature and expiry. Does NOT check that the payload
/// matches the shipment actually being booked — the caller does that, since
/// only it knows what "matches" means for the request in hand.
pub fn verify(secret: &[u8], token: &str) -> Result<QuoteTokenPayload, QuoteTokenError> {
    let (json_b64, sig_b64) = token.split_once('.').ok_or(QuoteTokenError::Malformed)?;

    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(json_b64.as_bytes());
    let expected = mac.finalize().into_bytes();

    let provided = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| QuoteTokenError::Malformed)?;

    if expected.as_slice().ct_eq(&provided).unwrap_u8() != 1 {
        return Err(QuoteTokenError::BadSignature);
    }

    let json = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(json_b64)
        .map_err(|_| QuoteTokenError::Malformed)?;
    let payload: QuoteTokenPayload = serde_json::from_slice(&json)
        .map_err(|_| QuoteTokenError::Malformed)?;

    if payload.expires_at < Utc::now() {
        return Err(QuoteTokenError::Expired);
    }

    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_payload(ttl_minutes: i64) -> QuoteTokenPayload {
        QuoteTokenPayload {
            tenant_id: Uuid::new_v4(),
            service_type: "standard".into(),
            weight_grams: 1_500,
            amount_cents: 2_200,
            currency: "AED".into(),
            expires_at: Utc::now() + Duration::minutes(ttl_minutes),
        }
    }

    #[test]
    fn sign_then_verify_round_trips_the_payload() {
        let secret = b"test-secret";
        let payload = make_payload(15);
        let token = sign(secret, &payload);
        let verified = verify(secret, &token).expect("must verify");
        assert_eq!(verified, payload);
    }

    #[test]
    fn verify_rejects_a_tampered_payload() {
        let secret = b"test-secret";
        let token = sign(secret, &make_payload(15));
        let (_, sig) = token.split_once('.').unwrap();
        let tampered_payload = QuoteTokenPayload { amount_cents: 1, ..make_payload(15) };
        let tampered_json_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&tampered_payload).unwrap());
        let tampered_token = format!("{tampered_json_b64}.{sig}");
        assert!(matches!(verify(secret, &tampered_token), Err(QuoteTokenError::BadSignature)));
    }

    #[test]
    fn verify_rejects_the_wrong_secret() {
        let token = sign(b"secret-a", &make_payload(15));
        assert!(matches!(verify(b"secret-b", &token), Err(QuoteTokenError::BadSignature)));
    }

    #[test]
    fn verify_rejects_an_expired_token() {
        let secret = b"test-secret";
        let token = sign(secret, &make_payload(-1)); // already expired
        assert!(matches!(verify(secret, &token), Err(QuoteTokenError::Expired)));
    }

    #[test]
    fn verify_rejects_a_malformed_token() {
        assert!(matches!(verify(b"secret", "not-a-valid-token"), Err(QuoteTokenError::Malformed)));
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p logisticos-order-intake quote_token`
Expected: all 5 pass immediately (this task writes the implementation and tests together since the logic is short enough to review as one unit — unlike Task 12, which had a real "does the tariff math check out" question worth isolating first).

- [ ] **Step 4: Register the module**

Add `pub mod quote_token;` to `services/order-intake/src/domain/value_objects/mod.rs`, matching its existing style.

- [ ] **Step 5: Commit**

```bash
git add services/order-intake/src/domain/value_objects/quote_token.rs services/order-intake/src/domain/value_objects/mod.rs services/order-intake/Cargo.toml services/order-intake/src/config.rs
git commit -m "feat(order-intake): add signed short-TTL quote token"
```

---

### Task 14: `POST /v1/shipments/quote`

**Files:**
- Create: `services/order-intake/src/api/http/quote.rs`
- Modify: `services/order-intake/src/api/http/mod.rs`

- [ ] **Step 1: Write the handler**

```rust
//! POST /v1/shipments/quote — authoritative, server-priced quote for a
//! shipment the customer is about to book. Returns a signed, short-TTL token
//! carrying the priced amount; `POST /v1/shipments` re-verifies it rather
//! than trusting a client-supplied amount. AE-region (AED) tenants only —
//! other currencies keep using the existing cash-at-pickup flow and never
//! call this endpoint.

use axum::{extract::State, response::{IntoResponse, Json}, http::StatusCode};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::api::http::AppState;
use crate::domain::entities::piece::Piece;
use crate::domain::entities::shipment::Shipment;
use crate::domain::value_objects::quote_token::{self, QuoteTokenPayload};
use crate::domain::value_objects::ServiceType;

/// Quote token validity — short enough that a stale review screen can't be
/// used to lock in a price from an hour ago, long enough to cover filling out
/// the rest of the booking form.
const QUOTE_TTL_MINUTES: i64 = 15;

#[derive(Deserialize)]
pub struct QuoteRequest {
    pub service_type: String,
    pub weight_grams: u32,
    #[serde(default)]
    pub pieces: Option<Vec<QuotePieceInput>>,
}

#[derive(Deserialize)]
pub struct QuotePieceInput {
    pub weight_grams: u32,
}

#[derive(Serialize)]
pub struct QuoteResponse {
    pub amount_cents: i64,
    pub currency: String,
    pub quote_token: String,
    pub expires_at: chrono::DateTime<Utc>,
}

pub async fn get_quote(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<QuoteRequest>,
) -> impl IntoResponse {
    let currency = claims.currency.as_deref();
    if currency != Some("AED") {
        return Err::<_, AppError>(AppError::Validation(
            "Online quotes are only available for AE-region (AED) tenants".into(),
        ));
    }

    let service_type = match req.service_type.as_str() {
        "standard"      => ServiceType::Standard,
        "express"       => ServiceType::Express,
        "same_day"      => ServiceType::SameDay,
        "balikbayan"    => ServiceType::Balikbayan,
        "international" => ServiceType::International,
        other => return Err(AppError::Validation(format!("Unknown service type: {other}"))),
    };

    // A throwaway Shipment carrying only what the fee calculation reads —
    // never persisted, exists purely so this endpoint reuses the exact same
    // `compute_base_fee_aed*` methods POST /v1/shipments will use to verify.
    let shell = Shipment::for_quote(service_type, req.weight_grams);

    let amount_cents = match (&req.pieces, service_type) {
        (Some(inputs), ServiceType::Balikbayan | ServiceType::International) if !inputs.is_empty() => {
            let pieces: Vec<Piece> = inputs.iter().map(|p| Piece::for_quote(p.weight_grams)).collect();
            shell.compute_base_fee_aed_with_pieces(&pieces).amount
        }
        _ => shell.compute_base_fee_aed().amount,
    };

    let expires_at = Utc::now() + Duration::minutes(QUOTE_TTL_MINUTES);
    let payload = QuoteTokenPayload {
        tenant_id: claims.tenant_id,
        service_type: req.service_type.clone(),
        weight_grams: req.weight_grams,
        amount_cents,
        currency: "AED".into(),
        expires_at,
    };
    let quote_token = quote_token::sign(s.quote_token_secret.as_bytes(), &payload);

    Ok((StatusCode::OK, Json(QuoteResponse {
        amount_cents,
        currency: "AED".into(),
        quote_token,
        expires_at,
    })))
}
```

- [ ] **Step 2: Add the two quote-only constructors this handler needs**

In `services/order-intake/src/domain/entities/shipment.rs`, add near the other `impl Shipment` methods:

```rust
    /// A minimal, never-persisted `Shipment` carrying only what
    /// `compute_base_fee_aed*` reads. Exists so the quote endpoint and the
    /// real booking path share one fee calculation instead of two.
    pub fn for_quote(service_type: ServiceType, weight_grams: u32) -> Self {
        Self {
            id: ShipmentId::new(),
            tenant_id: TenantId::from_uuid(uuid::Uuid::nil()),
            merchant_id: MerchantId::from_uuid(uuid::Uuid::nil()),
            customer_id: CustomerId::new(),
            customer_name: String::new(),
            customer_phone: String::new(),
            customer_email: None,
            booked_by_customer: true,
            auto_dispatch: true,
            awb: logisticos_types::awb::Awb::placeholder_for_quote(),
            piece_count: 1,
            status: ShipmentStatus::Pending,
            service_type,
            origin: Address::default_for_quote(),
            destination: Address::default_for_quote(),
            weight: ShipmentWeight::from_grams(weight_grams),
            dimensions: None,
            declared_value: None,
            cod_amount: None,
            special_instructions: None,
            merchant_reference: None,
            source_platform: None,
            external_order_id: None,
            payment_intent_id: None,
            payment_status: PaymentRequirement::NotRequired,
            pending_dispatch_events: None,
            idempotency_key: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
```

This references `Awb::placeholder_for_quote()` and `Address::default_for_quote()`, and `PaymentRequirement`/`payment_intent_id`/`payment_status`/`pending_dispatch_events`/`idempotency_key`, which don't exist until Task 16 adds the payment fields to `Shipment` and Task 12's neighbors add quote-friendly constructors to `Awb`/`Address` if they don't already have a cheap default. **Reorder: do this step only after Task 16**, or — simpler — skip building a full `Shipment` here and instead extract the fee math into two free functions that don't need a `Shipment` at all:

```rust
    // Simpler alternative used instead of the constructor above: two free
    // functions taking only what they need, so the quote endpoint never has
    // to fabricate a throwaway Shipment (and doesn't depend on Task 16's
    // fields existing yet).
```

Replace the `Shipment::for_quote` approach with two free functions in `shipment.rs` (outside `impl Shipment`, near `compute_base_fee_aed`'s definition):

```rust
/// AE tariff for Standard/Express/SameDay, independent of any `Shipment`
/// instance — used by both `POST /v1/shipments/quote` (no shipment exists
/// yet) and, indirectly, `Shipment::compute_base_fee_aed` below.
pub fn ae_base_fee_for(service_type: ServiceType, weight_grams: u32) -> Money {
    use logisticos_types::Currency;
    let base = match service_type {
        ServiceType::Standard      => 2_000,
        ServiceType::Express       => 3_500,
        ServiceType::SameDay       => 4_500,
        ServiceType::Balikbayan    => 12_000,
        ServiceType::International => 15_000,
    };
    let weight_kg = weight_grams as f64 / 1000.0;
    let surcharge = if weight_kg > 1.0 {
        ((weight_kg - 1.0) / 0.5).ceil() as i64 * 200
    } else {
        0
    };
    Money::new(base + surcharge, Currency::AED)
}

/// AE tariff for a Balikbayan/International piece list, independent of any
/// `Shipment` instance.
pub fn ae_piece_fee_for(piece_weights_grams: &[u32]) -> Money {
    use logisticos_types::Currency;
    let total: i64 = piece_weights_grams.iter().map(|&grams| {
        let kg = grams as f64 / 1000.0;
        let surcharge = if kg > 25.0 { ((kg - 25.0) / 0.5).ceil() as i64 * 500 } else { 0 };
        12_000i64 + surcharge
    }).sum();
    Money::new(total, Currency::AED)
}
```

And simplify `compute_base_fee_aed`/`compute_base_fee_aed_with_pieces` (written in Task 12) to delegate to these:

```rust
    pub fn compute_base_fee_aed(&self) -> Money {
        ae_base_fee_for(self.service_type, self.weight.grams)
    }

    pub fn compute_base_fee_aed_with_pieces(&self, pieces: &[super::piece::Piece]) -> Money {
        match self.service_type {
            ServiceType::Balikbayan | ServiceType::International if !pieces.is_empty() => {
                let weights: Vec<u32> = pieces.iter().map(|p| p.billable_weight_grams()).collect();
                ae_piece_fee_for(&weights)
            }
            _ => self.compute_base_fee_aed(),
        }
    }
```

(Go back and apply this simplification to Task 12's implementation before moving on — it removes the need for the `for_quote` constructor entirely, and Task 12's existing tests keep passing unchanged since the public method signatures don't change.)

Then rewrite `quote.rs`'s amount calculation to call the free functions directly instead of constructing a `Shipment`:

```rust
    let amount_cents = match (&req.pieces, service_type) {
        (Some(inputs), ServiceType::Balikbayan | ServiceType::International) if !inputs.is_empty() => {
            let weights: Vec<u32> = inputs.iter().map(|p| p.weight_grams).collect();
            crate::domain::entities::shipment::ae_piece_fee_for(&weights).amount
        }
        _ => crate::domain::entities::shipment::ae_base_fee_for(service_type, req.weight_grams).amount,
    };
```

Drop the `Shipment`/`Piece` imports from `quote.rs` that are no longer needed (`Shipment`, `Piece`) once this simplification is applied.

- [ ] **Step 3: Add `quote_token_secret` to `AppState` and wire the route**

In `services/order-intake/src/api/http/mod.rs`:
- Add `pub mod quote;` to the top module list.
- Add `pub quote_token_secret: String,` to `AppState` (`mod.rs:29-35`).
- Add the route inside `protected_router` (wherever it's defined — this file nests shipment routes under `/v1`, following the same auth-gated pattern as `create_shipment`; place it right before the existing `.route("/shipments", ...)` entry):
  ```rust
      .route("/shipments/quote", post(quote::get_quote))
  ```
  (Declared before `/shipments/:id` would matter for a path-conflicting segment, but `/shipments/quote` is a sibling literal segment to `/shipments`, not a child of `/shipments/:id`, so ordering relative to `/shipments/:id` doesn't matter here — axum resolves `/shipments/quote` as its own literal route regardless of where it's listed, same as the existing `/shipments/bulk`.)

- [ ] **Step 4: Wire the secret through `bootstrap.rs`**

In `services/order-intake/src/bootstrap.rs` (read it first to find the `AppState { ... }` construction, following the same pattern as payments' `bootstrap.rs`), add `quote_token_secret: cfg.quote_token_secret.clone(),` (or wherever Task 13's Step 1 ended up putting the field on `Config`) to the state struct literal.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p logisticos-order-intake`
Expected: no errors.

- [ ] **Step 6: Write an integration test for the endpoint**

Follow whatever HTTP-handler test pattern already exists in this crate (per `Cargo.toml`'s `[dev-dependencies]` comment: `tower::ServiceExt::oneshot` + `http-body-util`, not `axum-test`). Find an existing test exercising `create_shipment` or a similarly auth-gated route (`grep -rn "oneshot" services/order-intake/tests`) and copy its exact request-building/claims-injection pattern, then add:

```rust
#[tokio::test]
async fn quote_rejects_a_non_aed_tenant() {
    // build the test app the same way the existing create_shipment test does,
    // but with claims.currency = Some("PHP".into())
    // POST /v1/shipments/quote with a standard 1500g request
    // assert the response status is 400/422 per this crate's AppError::Validation mapping
}

#[tokio::test]
async fn quote_returns_a_verifiable_signed_token_for_an_aed_tenant() {
    // claims.currency = Some("AED".into())
    // POST /v1/shipments/quote { service_type: "standard", weight_grams: 1500 }
    // assert response.amount_cents == 2200
    // assert quote_token::verify(secret, &response.quote_token) succeeds and its
    // amount_cents matches
}
```

Write these fully once you've read the existing test file's exact helper names — do not invent a different test harness pattern for this one endpoint.

- [ ] **Step 7: Run the tests**

Run: `cargo test -p logisticos-order-intake quote`
Expected: both pass.

- [ ] **Step 8: Commit**

```bash
git add services/order-intake/src/api/http/quote.rs services/order-intake/src/api/http/mod.rs services/order-intake/src/domain/entities/shipment.rs services/order-intake/src/bootstrap.rs
git commit -m "feat(order-intake): add POST /v1/shipments/quote for AE-region tenants"
```

---

# Phase 3 — order-intake: payment-aware shipment creation

### Task 15: Migration — payment columns on `shipments`

**Files:**
- Create: `services/order-intake/migrations/0012_add_payment_fields.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Migration: 0012 — Order Intake: online-payment fields on shipments
--
-- `pending_dispatch_events` holds the AwbIssued/ShipmentCreated/ShipmentConfirmed
-- event payloads verbatim when payment_status = 'awaiting_payment', so the
-- payment-captured consumer can republish them unchanged rather than
-- reconstructing them from scratch (some of their fields, like sender_name,
-- aren't persisted anywhere else on this table).
--
-- No RLS: per migration 0011 (drop_decorative_rls), RLS was found decorative
-- on this table and is not re-added here.

ALTER TABLE order_intake.shipments
    ADD COLUMN payment_intent_id       UUID,
    ADD COLUMN payment_status          TEXT NOT NULL DEFAULT 'not_required'
                                        CHECK (payment_status IN (
                                            'not_required','awaiting_payment','paid','payment_failed'
                                        )),
    ADD COLUMN pending_dispatch_events JSONB,
    ADD COLUMN idempotency_key         TEXT;

-- Sweep target: shipments stuck awaiting payment past their TTL.
CREATE INDEX idx_shipments_awaiting_payment
    ON order_intake.shipments (payment_status, created_at)
    WHERE payment_status = 'awaiting_payment';

-- Idempotent re-submission of the same booking request must return the
-- existing shipment, scoped per tenant (two tenants could coincidentally
-- generate the same client-side UUID).
CREATE UNIQUE INDEX idx_shipments_idempotency
    ON order_intake.shipments (tenant_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
```

- [ ] **Step 2: Commit**

```bash
git add services/order-intake/migrations/0012_add_payment_fields.sql
git commit -m "feat(order-intake): add payment tracking columns to shipments"
```

---

### Task 16: Extend the `Shipment` entity and repository

**Files:**
- Modify: `services/order-intake/src/domain/entities/shipment.rs`
- Modify: `services/order-intake/src/application/services/shipment_service.rs`
- Modify: `services/order-intake/src/infrastructure/db/mod.rs`

- [ ] **Step 1: Add the fields and the `PaymentRequirement` enum**

In `services/order-intake/src/domain/entities/shipment.rs`, add near the top (after the `use` block, before `struct Shipment`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentRequirement {
    NotRequired,
    AwaitingPayment,
    Paid,
    PaymentFailed,
}

impl PaymentRequirement {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotRequired    => "not_required",
            Self::AwaitingPayment => "awaiting_payment",
            Self::Paid           => "paid",
            Self::PaymentFailed  => "payment_failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "not_required"     => Some(Self::NotRequired),
            "awaiting_payment" => Some(Self::AwaitingPayment),
            "paid"             => Some(Self::Paid),
            "payment_failed"   => Some(Self::PaymentFailed),
            _                  => None,
        }
    }
}
```

Add to the `Shipment` struct (after `external_order_id`, before `created_at`):

```rust
    pub payment_intent_id: Option<Uuid>,
    pub payment_status: PaymentRequirement,
    /// Serialized AwbIssued/ShipmentCreated/ShipmentConfirmed event payloads,
    /// held here while `payment_status == AwaitingPayment` instead of being
    /// published — see the plan's design note on why this replaces a new
    /// ShipmentStatus variant. `None` once paid (or if payment was never required).
    pub pending_dispatch_events: Option<serde_json::Value>,
    pub idempotency_key: Option<String>,
```

Add `use uuid::Uuid;` to the file's imports if not already present (it likely already is, via `ShipmentId` usage elsewhere — check first).

- [ ] **Step 2: Extend `ShipmentRepository`**

In `services/order-intake/src/application/services/shipment_service.rs`, add two methods to the `ShipmentRepository` trait (`shipment_service.rs:55-87`), matching its existing hand-written-boxed-future style exactly:

```rust
    /// Idempotent re-submission lookup, scoped per tenant.
    fn find_by_idempotency_key<'a>(
        &'a self,
        tenant_id: uuid::Uuid,
        idempotency_key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<Shipment>>> + Send + 'a>>;

    /// Shipments still `awaiting_payment` past the given cutoff — the sweep target.
    fn find_awaiting_payment_older_than<'a>(
        &'a self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<Shipment>>> + Send + 'a>>;
```

- [ ] **Step 3: Implement both in `PgShipmentRepository`**

In `services/order-intake/src/infrastructure/db/mod.rs`:

Add the four new fields to `ShipmentRow` (after `external_order_id`):

```rust
    payment_intent_id:       Option<Uuid>,
    payment_status:          String,
    pending_dispatch_events: Option<serde_json::Value>,
    idempotency_key:         Option<String>,
```

Add them to `SHIPMENT_COLS` (`mod.rs:189-...`) — append `, payment_intent_id, payment_status, pending_dispatch_events, idempotency_key` before the closing of the constant.

Add them to `row_to_shipment_row` (`mod.rs:204-...`) — add the corresponding `.get(...)` calls:

```rust
        payment_intent_id:       r.get("payment_intent_id"),
        payment_status:          r.get("payment_status"),
        pending_dispatch_events: r.get("pending_dispatch_events"),
        idempotency_key:         r.get("idempotency_key"),
```

Add the corresponding fields to `ShipmentRow::into_shipment()`'s constructed `Shipment { ... }` literal:

```rust
            payment_intent_id: self.payment_intent_id,
            payment_status: PaymentRequirement::parse(&self.payment_status)
                .expect("payment_status CHECK constraint guarantees a known value"),
            pending_dispatch_events: self.pending_dispatch_events,
            idempotency_key: self.idempotency_key,
```

Extend the `save()` INSERT (`mod.rs:332-366`) — add four columns to the column list, four placeholders (`$44,$45,$46,$47`), and their `ON CONFLICT` update:

```sql
                    merchant_reference, source_platform, external_order_id,
                    payment_intent_id, payment_status, pending_dispatch_events, idempotency_key,
                    created_at, updated_at
                ) VALUES (
                    $1,$2,$3,$4,$5,$6,$7,$8,$9,
                    $10,$11,$12,$13,
                    $14,$15,$16,$17,$18,$19,$20,$21,$22,
                    $23,$24,$25,$26,$27,$28,$29,$30,$31,
                    $32,$33,$34,$35,$36,$37,$38,$39,$40,$41,$42,
                    $43,$44,$45,$46,$47,$48
                )
                ON CONFLICT (id) DO UPDATE SET
                    status               = EXCLUDED.status,
                    customer_name        = EXCLUDED.customer_name,
                    customer_phone       = EXCLUDED.customer_phone,
                    customer_email       = EXCLUDED.customer_email,
                    booked_by_customer   = EXCLUDED.booked_by_customer,
                    auto_dispatch        = EXCLUDED.auto_dispatch,
                    origin_lat           = EXCLUDED.origin_lat,
                    origin_lng           = EXCLUDED.origin_lng,
                    dest_lat             = EXCLUDED.dest_lat,
                    dest_lng             = EXCLUDED.dest_lng,
                    special_instructions = EXCLUDED.special_instructions,
                    merchant_reference   = EXCLUDED.merchant_reference,
                    source_platform      = EXCLUDED.source_platform,
                    external_order_id    = EXCLUDED.external_order_id,
                    payment_intent_id       = EXCLUDED.payment_intent_id,
                    payment_status          = EXCLUDED.payment_status,
                    pending_dispatch_events = EXCLUDED.pending_dispatch_events,
                    idempotency_key         = EXCLUDED.idempotency_key,
                    updated_at           = EXCLUDED.updated_at"#,
```

(Note the original had 43 placeholders ending at `$43` for `updated_at` — recount against the actual current file rather than trusting this arithmetic blindly, since Task 15 didn't change column order upstream of these four; insert the four new columns right before `created_at, updated_at` in both the column list and the numbered placeholders, then renumber `created_at`/`updated_at` to the next two numbers after them.)

Add the four new `.bind(...)` calls (`mod.rs:409-412` area) right before `.bind(s.created_at)`:

```rust
            .bind(s.payment_intent_id)
            .bind(s.payment_status.as_str())
            .bind(&s.pending_dispatch_events)
            .bind(s.idempotency_key.as_deref())
```

Implement the two new trait methods (add after the existing `save()` implementation, before `record_event`):

```rust
    fn find_by_idempotency_key<'a>(
        &'a self,
        tenant_id: Uuid,
        idempotency_key: &'a str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<Shipment>>> + Send + 'a>> {
        Box::pin(async move {
            let query = format!(
                "SELECT {SHIPMENT_COLS} FROM order_intake.shipments WHERE tenant_id = $1 AND idempotency_key = $2"
            );
            let row = sqlx::query(&query)
                .bind(tenant_id)
                .bind(idempotency_key)
                .fetch_optional(&self.pool)
                .await?;
            Ok(row.map(|r| row_to_shipment_row(&r).into_shipment()))
        })
    }

    fn find_awaiting_payment_older_than<'a>(
        &'a self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<Shipment>>> + Send + 'a>> {
        Box::pin(async move {
            let query = format!(
                "SELECT {SHIPMENT_COLS} FROM order_intake.shipments \
                 WHERE payment_status = 'awaiting_payment' AND created_at < $1"
            );
            let rows = sqlx::query(&query).bind(cutoff).fetch_all(&self.pool).await?;
            Ok(rows.iter().map(|r| row_to_shipment_row(r).into_shipment()).collect())
        })
    }
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p logisticos-order-intake`
Expected: fails at every other `Shipment { ... }` struct literal in the crate that doesn't yet set the four new fields (Rust requires every field on a non-`..Default::default()` literal). Find them all:

Run: `cargo check -p logisticos-order-intake 2>&1 | grep "missing field"`

Add `payment_intent_id: None, payment_status: PaymentRequirement::NotRequired, pending_dispatch_events: None, idempotency_key: None,` to each one reported (these will be in `shipment_service.rs`'s `create()` method's `Shipment { ... }` literal, and any test-fixture constructors found via the earlier `grep -rn "oneshot"` search in Task 14).

Run: `cargo check -p logisticos-order-intake`
Expected: no errors now.

- [ ] **Step 5: Commit**

```bash
git add services/order-intake/src/domain/entities/shipment.rs services/order-intake/src/application/services/shipment_service.rs services/order-intake/src/infrastructure/db/mod.rs
git commit -m "feat(order-intake): add payment tracking fields to Shipment and its repository"
```

---

### Task 17: `PaymentsClient` (order-intake → payments)

**Files:**
- Create: `services/order-intake/src/infrastructure/http/payments_client.rs`
- Modify: `services/order-intake/src/infrastructure/http/mod.rs` (create this file if it doesn't exist yet — check first with `Glob services/order-intake/src/infrastructure/http/*`)
- Modify: `services/order-intake/src/config.rs`

- [ ] **Step 1: Add `payments_url` to config**

Following the exact `OrderIntakeConfig { pub url: String }` pattern from `services/payments/src/config.rs` (Task 7's Step 3 reference), add the mirror-image config to `services/order-intake/src/config.rs`:

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct PaymentsConfig {
    /// Base URL of the payments service, e.g. http://payments:8012
    pub url: String,
}
```

And add `pub payments: PaymentsConfig,` to the top-level `Config` struct.

- [ ] **Step 2: Write the client**

```rust
//! HTTP client for the payments service's mesh-internal payment-intent endpoint.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub struct PaymentsClient {
    base_url: String,
    http: reqwest::Client,
}

impl PaymentsClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), http: reqwest::Client::new() }
    }
}

#[derive(Serialize)]
struct CreateIntentRequest {
    tenant_id: Uuid,
    purpose: String,
    reference_type: String,
    reference_id: Uuid,
    amount_cents: i64,
    currency: String,
    return_url: String,
}

#[derive(Deserialize)]
pub struct CreatedIntent {
    pub intent_id: Uuid,
    pub checkout_url: String,
}

impl PaymentsClient {
    pub async fn create_shipping_fee_intent(
        &self,
        tenant_id: Uuid,
        shipment_id: Uuid,
        amount_cents: i64,
        currency: &str,
        return_url: &str,
    ) -> anyhow::Result<CreatedIntent> {
        let url = format!("{}/v1/internal/payments/intents", self.base_url.trim_end_matches('/'));
        let resp = self.http
            .post(&url)
            .json(&CreateIntentRequest {
                tenant_id,
                purpose: "shipping_fee".into(),
                reference_type: "shipment".into(),
                reference_id: shipment_id,
                amount_cents,
                currency: currency.into(),
                return_url: return_url.into(),
            })
            .send()
            .await?
            .error_for_status()?
            .json::<CreatedIntent>()
            .await?;
        Ok(resp)
    }
}
```

- [ ] **Step 3: Register the module**

If `services/order-intake/src/infrastructure/http/mod.rs` doesn't exist yet, create it with `pub mod payments_client; pub use payments_client::PaymentsClient;`. If it exists, add the same two lines matching its style. Also confirm `pub mod http;` is declared in `services/order-intake/src/infrastructure/mod.rs` (check first — add it if missing).

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p logisticos-order-intake`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add services/order-intake/src/infrastructure/http/payments_client.rs services/order-intake/src/infrastructure/http/mod.rs services/order-intake/src/infrastructure/mod.rs services/order-intake/src/config.rs
git commit -m "feat(order-intake): add PaymentsClient for the internal payment-intents call"
```

---

### Task 18: Payment-aware `ShipmentService::create()`

**Files:**
- Modify: `services/order-intake/src/application/commands/mod.rs`
- Modify: `services/order-intake/src/application/services/shipment_service.rs`
- Modify: `services/order-intake/src/api/http/mod.rs`

- [ ] **Step 1: Extend `CreateShipmentCommand`**

In `services/order-intake/src/application/commands/mod.rs`, add to `CreateShipmentCommand`'s field list (read the struct first to match its existing `#[serde(default)]` conventions for optional fields):

```rust
    /// A signed quote token from `POST /v1/shipments/quote`. When present,
    /// the shipment is created in `awaiting_payment` and dispatch is held
    /// until the corresponding payment intent captures.
    #[serde(default)]
    pub quote_token: Option<String>,
    /// Client-generated idempotency key — a retry with the same key returns
    /// the shipment already created for it instead of creating a duplicate
    /// (and a duplicate charge).
    #[serde(default)]
    pub idempotency_key: Option<String>,
```

- [ ] **Step 2: Add `PaymentsClient` and the quote secret to `ShipmentService`**

In `shipment_service.rs`, extend the `ShipmentService` struct and its constructor (`shipment_service.rs:143-158`):

```rust
pub struct ShipmentService {
    pub repo:          Arc<dyn ShipmentRepository>,
    pub publisher:     Arc<dyn EventPublisher>,
    pub normalizer:    Arc<dyn AddressNormalizer>,
    pub awb_generator: Arc<dyn AwbGenerator>,
    pub payments_client: Arc<crate::infrastructure::http::PaymentsClient>,
    pub quote_token_secret: String,
    pub shipment_return_url_base: String,
}

impl ShipmentService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo:          Arc<dyn ShipmentRepository>,
        publisher:     Arc<dyn EventPublisher>,
        normalizer:    Arc<dyn AddressNormalizer>,
        awb_generator: Arc<dyn AwbGenerator>,
        payments_client: Arc<crate::infrastructure::http::PaymentsClient>,
        quote_token_secret: String,
        shipment_return_url_base: String,
    ) -> Self {
        Self { repo, publisher, normalizer, awb_generator, payments_client, quote_token_secret, shipment_return_url_base }
    }
```

- [ ] **Step 3: Add the idempotency short-circuit at the top of `create()`**

At the very start of `create()`, right after the `tracing::info!(step = "enter", ...)` line (`shipment_service.rs:161`), add:

```rust
        if let Some(key) = cmd.idempotency_key.as_deref() {
            if let Some(existing) = self.repo.find_by_idempotency_key(cmd.tenant_id, key).await
                .map_err(AppError::Internal)?
            {
                tracing::info!(shipment_id = %existing.id, "create: idempotent replay — returning existing shipment");
                return Ok(existing);
            }
        }
```

(This changes `create()`'s effective return type contract nowhere — it's still `AppResult<Shipment>`; the checkout_url piece is handled at the HTTP layer in Step 5 below, not by widening this method's return type, since only the *new*-shipment path has a checkout_url to report and a replay by definition already went through checkout once.)

- [ ] **Step 4: Branch the event-publishing block on `quote_token`**

Verify the quote token (if present) right after building the `Shipment` and before persisting anything, so a bad/tampered/expired token fails the request before any row is written. Insert this right after the `let shipment = Shipment { ... };` literal (`shipment_service.rs:344-370`) and before `self.repo.save(&shipment)`:

```rust
        let (payment_status, verified_amount_cents) = match &cmd.quote_token {
            None => (PaymentRequirement::NotRequired, None),
            Some(token) => {
                let payload = crate::domain::value_objects::quote_token::verify(
                    self.quote_token_secret.as_bytes(), token,
                ).map_err(|e| AppError::Validation(format!("Invalid quote: {e}")))?;
                if payload.tenant_id != cmd.tenant_id {
                    return Err(AppError::Validation("Quote token does not belong to this tenant".into()));
                }
                if payload.service_type != cmd.service_type || payload.weight_grams != cmd.weight_grams {
                    return Err(AppError::Validation(
                        "Quote token does not match this booking's service type or weight".into(),
                    ));
                }
                (PaymentRequirement::AwaitingPayment, Some(payload.amount_cents))
            }
        };
```

Set `shipment.payment_status = payment_status;` and `shipment.idempotency_key = cmd.idempotency_key.clone();` on the `Shipment { ... }` literal itself (add these two fields to that literal directly, alongside the other field assignments already there).

Now replace the existing unconditional "── Publish AwbIssued ──" / "── Publish ShipmentCreated ──" / "── Publish ShipmentConfirmed ──" blocks (`shipment_service.rs:416-509`) with a branch. Keep the three `Event::new(...)` constructions exactly as they are today (nothing about their payload changes), but wrap the *publishing* in:

```rust
        let awb_json = serde_json::to_value(&awb_event).map_err(|e| AppError::Internal(e.into()))?;
        let created_json = serde_json::to_value(&event).map_err(|e| AppError::Internal(e.into()))?;
        let confirmed_json = serde_json::to_value(&confirmed_event).map_err(|e| AppError::Internal(e.into()))?;

        let mut checkout_url: Option<String> = None;

        if payment_status == PaymentRequirement::AwaitingPayment {
            // Hold every dispatch/engagement/analytics-triggering event until
            // payment.intent.captured republishes them unchanged.
            shipment.pending_dispatch_events = Some(serde_json::json!({
                "awb_issued": awb_json,
                "shipment_created": created_json,
                "shipment_confirmed": confirmed_json,
            }));

            let amount_cents = verified_amount_cents
                .expect("AwaitingPayment always comes from a verified quote token carrying an amount");
            let return_url = format!("{}/payment/return?shipment_id={}", self.shipment_return_url_base, shipment.id);
            let intent = self.payments_client
                .create_shipping_fee_intent(cmd.tenant_id, shipment.id.inner(), amount_cents, "AED", &return_url)
                .await
                .map_err(AppError::Internal)?;
            shipment.payment_intent_id = Some(intent.intent_id);
            checkout_url = Some(intent.checkout_url);
        } else {
            // Unchanged behavior for every non-prepaid booking: publish immediately.
            if let Ok(payload) = serde_json::to_string(&awb_event) {
                let _ = self.publisher.publish(topics::AWB_ISSUED, master_awb.as_str(), &payload).await;
            }
            if let Err(e) = self.publisher.publish(topics::SHIPMENT_CREATED, &shipment.id.to_string(),
                &serde_json::to_string(&event).map_err(|e| AppError::Internal(e.into()))?).await
            {
                tracing::warn!(error = %e, shipment_id = %shipment.id, "ShipmentCreated event publish failed (non-fatal)");
            }
            if let Ok(p) = serde_json::to_string(&confirmed_event) {
                if let Err(e) = self.publisher.publish(topics::SHIPMENT_CONFIRMED, &shipment.id.to_string(), &p).await {
                    tracing::warn!(error = %e, shipment_id = %shipment.id, "ShipmentConfirmed event publish failed (non-fatal)");
                }
            }
        }

        // Persist (moved here from its original earlier position — must
        // happen after pending_dispatch_events/payment_intent_id are set,
        // and after the payments call, so a failed payments call leaves no
        // row behind for an awaiting-payment shipment that never got a
        // checkout session).
        self.repo.save(&shipment).await.map_err(|e| {
            tracing::error!(error = ?e, "shipment_repo.save failed");
            AppError::Internal(e)
        })?;
        self.repo.save_pieces(&pieces).await.map_err(|e| {
            tracing::error!(error = ?e, "shipment_repo.save_pieces failed");
            AppError::Internal(e)
        })?;
```

This requires moving the original `self.repo.save(&shipment)` / `self.repo.save_pieces(&pieces)` calls (currently at `shipment_service.rs:373-382`, immediately after the `Shipment { ... }` literal) down to *after* this new block, and removing the timeline-event-stamping block's dependency on the shipment already being saved if it has one (check: `record_event` calls reference `shipment.id`, not any saved state, so they can stay wherever they are relative to `save()` — but move them after `save()` for consistency with "the row exists before anything referencing its id fires," matching the original ordering intent).

Concretely: delete the original `self.repo.save(&shipment).await...` / `self.repo.save_pieces(&pieces).await...` block from its current position (right after the `Shipment {...}` literal), leave the timeline-event-stamping block (`"── Stamp the opening timeline milestones ──"`) where it is (it only needs `shipment.id`, which exists as soon as the literal is built), and insert the new combined block above (quote verification → conditional publish-or-defer → save) to replace the original *later* AwbIssued/ShipmentCreated/ShipmentConfirmed publish section in place.

- [ ] **Step 5: Return `checkout_url` to the HTTP layer**

`create()`'s signature changes from `pub async fn create(&self, cmd: CreateShipmentCommand) -> AppResult<Shipment>` to:

```rust
pub struct CreateShipmentResult {
    pub shipment: Shipment,
    pub checkout_url: Option<String>,
}
```

```rust
    pub async fn create(&self, cmd: CreateShipmentCommand) -> AppResult<CreateShipmentResult> {
```

and its final `Ok(shipment)` becomes `Ok(CreateShipmentResult { shipment, checkout_url })`. The idempotent-replay early return in Step 3 becomes `return Ok(CreateShipmentResult { shipment: existing, checkout_url: None });` (a replay never re-issues a new checkout session — the original one from the first call is still what the client should use, and the client already has it from that first response).

Update `create_shipment` in `services/order-intake/src/api/http/mod.rs` (`mod.rs:102-108`):

```rust
    match s.svc.create(cmd).await {
        Ok(result) => {
            #[derive(serde::Serialize)]
            struct Response {
                #[serde(flatten)]
                shipment: crate::domain::entities::shipment::Shipment,
                #[serde(skip_serializing_if = "Option::is_none")]
                checkout_url: Option<String>,
            }
            Ok::<_, AppError>((StatusCode::CREATED, Json(Response { shipment: result.shipment, checkout_url: result.checkout_url })))
        }
        Err(e) => {
            tracing::error!(error = ?e, "create_shipment handler: service returned error");
            Err(e)
        }
    }
```

Update the two other call sites of `ShipmentService::create()` found via `grep -rn "\.svc\.create(\|self\.create(\|\.create(cmd)" services/order-intake/src` (`bulk_create_shipments`'s per-row loop and `internal_create_shipment`, if either calls the shared `create()` rather than duplicating logic) to use `result.shipment` instead of the old `shipment` binding, discarding `checkout_url` (those paths never set `quote_token`, so it's always `None`).

- [ ] **Step 6: Verify it compiles**

Run: `cargo check -p logisticos-order-intake`
Expected: no errors once all call sites are updated.

- [ ] **Step 7: Update/write tests**

The existing `ShipmentService::create()` test suite (find it via `grep -rn "async fn create" services/order-intake/src/application/services/shipment_service.rs` or in a sibling `tests/` module) will need every `svc.create(cmd).await.unwrap()` changed to `svc.create(cmd).await.unwrap().shipment` to keep compiling. Do that first, confirm the existing suite passes unchanged (`cargo test -p logisticos-order-intake shipment_service`), then add:

```rust
    #[tokio::test]
    async fn create_with_a_valid_quote_token_defers_dispatch_events_and_calls_payments() {
        // Build a ShipmentService with test doubles for repo/publisher/normalizer/
        // awb_generator (reuse whatever the existing create() tests already use
        // for these) plus a PaymentsClient pointed at a mock HTTP server (use
        // whatever HTTP-mocking crate, if any, is already a dev-dependency in
        // this crate — check Cargo.toml's [dev-dependencies] first; if none
        // exists, stand up a `tokio::net::TcpListener` + a minimal hand-rolled
        // axum server returning a fixed CreatedIntent JSON body, matching the
        // level of test infrastructure this crate already uses elsewhere).
        //
        // sign a valid quote token for the same tenant/service_type/weight the
        // command uses, set cmd.quote_token = Some(token)
        //
        // call svc.create(cmd).await, assert:
        //   - result.checkout_url is Some(...)
        //   - result.shipment.payment_status == PaymentRequirement::AwaitingPayment
        //   - the fake EventPublisher recorded ZERO publishes to AWB_ISSUED/
        //     SHIPMENT_CREATED/SHIPMENT_CONFIRMED
        //   - result.shipment.pending_dispatch_events is Some and contains all
        //     three expected keys
    }

    #[tokio::test]
    async fn create_without_a_quote_token_publishes_immediately_as_before() {
        // cmd.quote_token = None (the existing tests already cover this path
        // implicitly — this test just asserts payment_status == NotRequired
        // and pending_dispatch_events == None explicitly, as a regression guard)
    }

    #[tokio::test]
    async fn create_rejects_a_quote_token_for_a_different_tenant() {
        // sign a token with a random tenant_id != cmd.tenant_id
        // assert svc.create(cmd).await is Err
    }

    #[tokio::test]
    async fn create_is_idempotent_on_a_repeated_idempotency_key() {
        // call svc.create(cmd.clone()).await once, then again with the same
        // idempotency_key — assert the second call returns the same shipment.id
        // and the fake publisher/payments-client only recorded ONE call each
    }
```

Write these fully once you've read the existing test doubles in this file (`shipment_service.rs`'s own test module, or a sibling `tests/` integration file) — match their exact fixture-construction style rather than inventing a new one.

- [ ] **Step 8: Run the tests**

Run: `cargo test -p logisticos-order-intake shipment_service`
Expected: all pass, old and new.

- [ ] **Step 9: Commit**

```bash
git add services/order-intake/src/application/commands/mod.rs services/order-intake/src/application/services/shipment_service.rs services/order-intake/src/api/http/mod.rs
git commit -m "feat(order-intake): defer dispatch events and open a payment session when a quote token is presented"
```

---

### Task 19: Payment-captured/failed Kafka consumer

**Files:**
- Create: `services/order-intake/src/infrastructure/messaging/payment_consumer.rs`
- Modify: `services/order-intake/src/infrastructure/messaging/mod.rs`

- [ ] **Step 1: Write the consumer**

```rust
//! Kafka consumer for payment.intent.captured / payment.intent.failed
//! (`purpose = "shipping_fee"` only — other purposes belong to other future
//! consumers on other services). On captured: republish the shipment's
//! stored dispatch events unchanged and mark it paid. On failed: cancel it
//! via the existing ShipmentService::cancel(), same as a merchant-initiated
//! cancellation.

use std::sync::Arc;

use logisticos_events::{consumer::KafkaConsumer, topics};
use sqlx::PgPool;
use uuid::Uuid;

use crate::application::commands::CancelShipmentCommand;
use crate::application::services::shipment_service::ShipmentService;
use crate::domain::entities::shipment::PaymentRequirement;

pub struct PaymentConsumer {
    inner: KafkaConsumer,
    svc: Arc<ShipmentService>,
    pool: PgPool,
}

impl PaymentConsumer {
    pub fn new(brokers: &str, group_id: &str, svc: Arc<ShipmentService>, pool: PgPool) -> anyhow::Result<Self> {
        let inner = KafkaConsumer::new(
            brokers,
            &format!("{group_id}-payment"),
            &[topics::PAYMENT_INTENT_CAPTURED, topics::PAYMENT_INTENT_FAILED],
        )?;
        Ok(Self { inner, svc, pool })
    }

    pub async fn run(self) {
        let svc = self.svc;
        let pool = self.pool;
        let result = self.inner.run(move |topic, json| {
            let svc = Arc::clone(&svc);
            let pool = pool.clone();
            async move { handle(&topic, json, &svc, &pool).await }
        }).await;
        if let Err(e) = result {
            tracing::error!("PaymentConsumer loop exited with error: {e}");
        }
    }
}

async fn handle(topic: &str, json: serde_json::Value, svc: &ShipmentService, pool: &PgPool) -> anyhow::Result<()> {
    let data = json.get("data").cloned().unwrap_or(json.clone());
    let purpose = data.get("purpose").and_then(|v| v.as_str()).unwrap_or_default();
    if purpose != "shipping_fee" {
        return Ok(()); // not ours — a future purpose's own consumer handles it
    }
    let reference_id: Uuid = data.get("reference_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("payment event missing reference_id"))?;

    if topic == logisticos_events::topics::PAYMENT_INTENT_CAPTURED {
        handle_captured(reference_id, svc, pool).await
    } else {
        let reason = data.get("reason").and_then(|v| v.as_str()).unwrap_or("payment_failed");
        svc.cancel(CancelShipmentCommand { shipment_id: reference_id, reason: reason.to_string() }).await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

async fn handle_captured(shipment_id: Uuid, svc: &ShipmentService, pool: &PgPool) -> anyhow::Result<()> {
    use crate::domain::entities::shipment::Shipment;
    let id = logisticos_types::ShipmentId::from_uuid(shipment_id);
    let mut shipment = svc.repo.find_by_id(&id).await?
        .ok_or_else(|| anyhow::anyhow!("no shipment {shipment_id} for captured payment"))?;

    if shipment.payment_status != PaymentRequirement::AwaitingPayment {
        tracing::info!(shipment_id = %shipment_id, status = ?shipment.payment_status, "payment.intent.captured — already processed, idempotent skip");
        return Ok(());
    }

    let events = shipment.pending_dispatch_events.take()
        .ok_or_else(|| anyhow::anyhow!("shipment {shipment_id} is awaiting_payment but has no pending_dispatch_events"))?;

    shipment.payment_status = PaymentRequirement::Paid;
    svc.repo.save(&shipment).await?;

    for (topic, payload) in [
        (logisticos_events::topics::AWB_ISSUED, events.get("awb_issued")),
        (logisticos_events::topics::SHIPMENT_CREATED, events.get("shipment_created")),
        (logisticos_events::topics::SHIPMENT_CONFIRMED, events.get("shipment_confirmed")),
    ] {
        if let Some(p) = payload {
            if let Err(e) = svc.publisher.publish(topic, &shipment_id.to_string(), &p.to_string()).await {
                tracing::error!(shipment_id = %shipment_id, topic, error = %e, "failed to republish held dispatch event after payment capture");
            }
        }
    }

    let _ = pool; // reserved: only needed if a future purpose requires a raw query this consumer doesn't already have via svc.repo
    Ok(())
}
```

`ShipmentService.repo` and `.publisher` need to be `pub` (they already are — see `shipment_service.rs:144-145`: `pub repo: Arc<dyn ShipmentRepository>, pub publisher: Arc<dyn EventPublisher>,`), so this consumer can use them directly rather than adding new service methods purely for this one call site.

Drop the unused `pool: PgPool` field/parameter if, after review, nothing in `handle`/`handle_captured` ends up needing a raw connection beyond what `svc.repo` already provides (it doesn't, per the implementation above) — simplify the constructor to not take `pool` at all rather than keep a genuinely unused field. Do this cleanup before running the compiler, not after seeing a warning.

- [ ] **Step 2: Register the module**

Add `pub mod payment_consumer;` to `services/order-intake/src/infrastructure/messaging/mod.rs`, matching `pub mod status_consumer;`'s existing style.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p logisticos-order-intake`
Expected: no errors (after removing the unused `pool` per Step 1's cleanup note).

- [ ] **Step 4: Write a test for the idempotent-skip and purpose-filter behavior**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // Reuse whatever fake ShipmentRepository/EventPublisher test doubles
    // Task 18's shipment_service tests already defined — import them rather
    // than redefining new ones.

    #[tokio::test]
    async fn non_shipping_fee_purpose_is_ignored() {
        let json = serde_json::json!({
            "data": { "purpose": "subscription", "reference_id": uuid::Uuid::new_v4() }
        });
        // handle(PAYMENT_INTENT_CAPTURED, json, &svc, &pool).await must return Ok(())
        // and must NOT touch the repo at all
    }

    #[tokio::test]
    async fn captured_on_an_already_paid_shipment_is_a_no_op() {
        // seed a shipment with payment_status = Paid, pending_dispatch_events = None
        // handle_captured must return Ok(()) without erroring on the missing events
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p logisticos-order-intake payment_consumer`
Expected: both pass.

- [ ] **Step 6: Commit**

```bash
git add services/order-intake/src/infrastructure/messaging/payment_consumer.rs services/order-intake/src/infrastructure/messaging/mod.rs
git commit -m "feat(order-intake): consume payment.intent.captured/failed to release or cancel a shipment"
```

---

### Task 20: Expiry sweep + wire everything into `bootstrap.rs`

**Files:**
- Modify: `services/order-intake/src/application/services/shipment_service.rs`
- Modify: `services/order-intake/src/bootstrap.rs`

- [ ] **Step 1: Add a sweep method to `ShipmentService`**

```rust
    /// Cancels every shipment still `awaiting_payment` past `ttl_minutes`.
    /// Called by the periodic sweep in `bootstrap.rs`. A shipment that
    /// captures payment concurrently with this running is not double-handled:
    /// `cancel()` checks `can_cancel()`, and a `Paid` shipment's status has
    /// already moved to `Confirmed`-eligible territory the same way any other
    /// booking's does, so `can_cancel()` still gates correctly either way —
    /// but the primary defense is timing: payments' own sweep interval (5 min)
    /// is deliberately shorter than this TTL, so by the time this sweep looks,
    /// a captured intent has already published its event and this consumer's
    /// `find_awaiting_payment_older_than` won't select it (payment_status is
    /// already `paid`, not `awaiting_payment`, by then in the normal case).
    pub async fn sweep_expired_payments(&self, ttl_minutes: i64) -> AppResult<usize> {
        let cutoff = Utc::now() - chrono::Duration::minutes(ttl_minutes);
        let stale = self.repo.find_awaiting_payment_older_than(cutoff).await.map_err(AppError::Internal)?;
        let count = stale.len();
        for shipment in stale {
            if let Err(e) = self.cancel(CancelShipmentCommand {
                shipment_id: shipment.id.inner(),
                reason: "payment_expired".into(),
            }).await {
                tracing::error!(shipment_id = %shipment.id, error = ?e, "sweep: failed to cancel expired-payment shipment");
            }
        }
        Ok(count)
    }
```

- [ ] **Step 2: Wire `PaymentsClient`, the quote secret, and the new consumer/sweep into `bootstrap.rs`**

Read `services/order-intake/src/bootstrap.rs` first to find its `ShipmentService::new(...)` call site and its `AppState { ... }` construction (same general shape as `services/payments/src/bootstrap.rs`, already read in Task 7/11 — order-intake's file follows the same repo→service→AppState ordering).

Add, near the other client/repo constructions:

```rust
    let payments_client = Arc::new(
        crate::infrastructure::http::PaymentsClient::new(&cfg.payments.url)
    );
```

Update the existing `ShipmentService::new(...)` call to pass the three new constructor arguments added in Task 18 Step 2:

```rust
    let shipment_service = Arc::new(ShipmentService::new(
        Arc::clone(&shipment_repo) as _,
        Arc::clone(&publisher) as _,
        Arc::clone(&normalizer) as _,
        Arc::clone(&awb_generator) as _,
        Arc::clone(&payments_client),
        cfg.quote_token_secret.clone(),
        cfg.app.public_base_url.clone(), // the base URL the NI return_url is built from — add this field to AppConfig if it doesn't already exist, following the same env-driven pattern as every other config field
    ));
```

(Match the exact existing variable names in this file rather than the illustrative ones above — read the surrounding lines before editing.)

Add `pub quote_token_secret: String,` to `AppState` in `services/order-intake/src/api/http/mod.rs` if Task 14 didn't already add it there (it did — this is just the bootstrap-side wiring: set it in the `AppState { ... }` literal to `cfg.quote_token_secret.clone()`).

- [ ] **Step 3: Spawn the payment consumer and the sweep**

Following the exact `tokio::spawn` shape order-intake already uses for `status_consumer` (find that spawn site first — `grep -n "status_consumer\|tokio::spawn" services/order-intake/src/bootstrap.rs`), add:

```rust
    let payment_consumer = crate::infrastructure::messaging::payment_consumer::PaymentConsumer::new(
        &cfg.kafka.brokers,
        &cfg.kafka.group_id,
        Arc::clone(&shipment_service),
        pool.clone(),
    )?;
    tokio::spawn(async move { payment_consumer.run().await });

    // Payment-expiry sweep — checks every 5 minutes for shipments left
    // awaiting_payment past their TTL and cancels them. TTL matches the
    // payments-service intent TTL (30 minutes, see PaymentIntentService::INTENT_TTL).
    let svc_for_sweep = Arc::clone(&shipment_service);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(5 * 60));
        loop {
            tick.tick().await;
            match svc_for_sweep.sweep_expired_payments(30).await {
                Ok(count) if count > 0 => tracing::info!(count, "Shipment payment sweep: cancelled stale bookings"),
                Ok(_) => {}
                Err(e) => tracing::error!(err = ?e, "Shipment payment sweep failed"),
            }
        }
    });
```

- [ ] **Step 4: Add the env vars**

In `docker-compose.yml`'s `order-intake` service `environment:` block, add:

```
QUOTE_TOKEN_SECRET=<local-dev-secret>
PAYMENTS__URL=http://payments:8012
APP__PUBLIC_BASE_URL=http://localhost:8004
```

(Confirm the actual payments service port and the correct env var prefix for `quote_token_secret`/`public_base_url` against whatever Task 13/17 Step 1 actually named those config fields — this must match exactly, not be guessed independently here.)

- [ ] **Step 5: Verify the whole service compiles and its existing test suite still passes**

Run: `cargo check -p logisticos-order-intake`
Expected: no errors.

Run: `cargo test -p logisticos-order-intake`
Expected: full existing suite plus every test added in Tasks 12-19 passes.

- [ ] **Step 6: Commit**

```bash
git add services/order-intake/src/application/services/shipment_service.rs services/order-intake/src/bootstrap.rs docker-compose.yml
git commit -m "feat(order-intake): wire PaymentsClient, payment consumer, and the payment-expiry sweep"
```

---

# Phase 4 — Customer App

### Task 21: Quote + payment-aware `createShipment` in the API layer

**Files:**
- Modify: `apps/customer-app/src/services/api/shipments.ts`

- [ ] **Step 1: Add the quote types and call**

```typescript
// ── Quote (AE-region only) ──────────────────────────────────────────────────

export interface QuotePieceInput {
  weight_grams: number;
}

export interface QuoteRequest {
  service_type: 'standard' | 'express' | 'same_day' | 'balikbayan';
  weight_grams: number;
  pieces?: QuotePieceInput[];
}

export interface QuoteResponse {
  amount_cents: number;
  currency: string;
  quote_token: string;
  expires_at: string;
}

export async function getShipmentQuote(request: QuoteRequest): Promise<QuoteResponse> {
  const client = getOrderClient();
  const response = await client.post<QuoteResponse>('/v1/shipments/quote', request);
  return response.data;
}
```

- [ ] **Step 2: Extend the create-shipment request/response types**

Add to `CreateShipmentRequest`:

```typescript
  quote_token?: string;
  idempotency_key?: string;
```

Add to `ShipmentResponse`:

```typescript
  payment_status?: 'not_required' | 'awaiting_payment' | 'paid' | 'payment_failed';
  checkout_url?: string;
```

- [ ] **Step 3: Verify it type-checks**

Run: `cd apps/customer-app && npx tsc --noEmit`
Expected: no new errors.

- [ ] **Step 4: Commit**

```bash
git add apps/customer-app/src/services/api/shipments.ts
git commit -m "feat(customer-app): add shipment quote API and payment fields to createShipment"
```

---

### Task 22: Payment WebView screen

**Files:**
- Create: `apps/customer-app/src/screens/booking/PaymentWebView.tsx`
- Modify: `apps/customer-app/src/navigation/AppNavigator.tsx`

- [ ] **Step 1: Check whether `react-native-webview` is already a dependency**

Run: `grep -n "react-native-webview" apps/customer-app/package.json`

If absent, add it: `cd apps/customer-app && npx expo install react-native-webview`

- [ ] **Step 2: Write the screen**

```tsx
/**
 * Payment WebView — hosts Network International's checkout page.
 *
 * The NI redirect back to `return_url` is a UX signal only (see the design
 * spec's sequence step 5) — this screen never treats the redirect itself as
 * proof of payment. It navigates to the tracking/confirmation screen either
 * way, which polls the shipment's `payment_status` to show the real outcome.
 */
import React, { useCallback } from 'react';
import { View, StyleSheet } from 'react-native';
import { WebView, WebViewNavigation } from 'react-native-webview';
import { useNavigation, useRoute } from '@react-navigation/native';

const CANVAS = '#050810';

export function PaymentWebViewScreen() {
  const navigation = useNavigation<any>();
  const route = useRoute<any>();
  const { checkoutUrl, shipmentId, returnUrlPrefix } = route.params as {
    checkoutUrl: string;
    shipmentId: string;
    returnUrlPrefix: string;
  };

  const handleNavigationChange = useCallback((navState: WebViewNavigation) => {
    if (navState.url.startsWith(returnUrlPrefix)) {
      // Reached the return_url — payment attempt finished (success or
      // failure, we don't know which from the URL alone). Hand off to the
      // confirmation screen, which polls GET /v1/shipments/:id for the
      // authoritative payment_status set by the webhook-driven consumer.
      navigation.replace('BookingConfirmationPending', { shipmentId });
    }
  }, [navigation, returnUrlPrefix, shipmentId]);

  return (
    <View style={styles.container}>
      <WebView
        source={{ uri: checkoutUrl }}
        onNavigationStateChange={handleNavigationChange}
        startInLoadingState
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: CANVAS },
});
```

- [ ] **Step 3: Write the pending-confirmation screen it hands off to**

```tsx
/**
 * Shown right after the WebView closes. Polls the shipment until
 * payment_status leaves `awaiting_payment`, then routes to the existing
 * BookingConfirmation (paid) or an error state (failed/expired) — never
 * trusts the WebView redirect alone.
 */
import React, { useEffect, useState } from 'react';
import { View, Text, ActivityIndicator, StyleSheet } from 'react-native';
import { useNavigation, useRoute } from '@react-navigation/native';
import { getShipment } from '../../services/api/shipments';

const CANVAS = '#050810';
const POLL_MS = 2000;
const MAX_POLLS = 30; // 60 seconds — the webhook path is normally sub-second;
                       // this bounds how long a customer waits before being
                       // told to check back rather than spinning forever.

export function BookingConfirmationPendingScreen() {
  const navigation = useNavigation<any>();
  const route = useRoute<any>();
  const { shipmentId } = route.params as { shipmentId: string };
  const [attempt, setAttempt] = useState(0);
  const [timedOut, setTimedOut] = useState(false);

  useEffect(() => {
    let cancelled = false;
    async function poll() {
      const shipment = await getShipment(shipmentId).catch(() => null);
      if (cancelled || !shipment) return;
      if (shipment.payment_status === 'paid') {
        navigation.replace('BookingConfirmation', { awb: shipment.awb });
        return;
      }
      if (shipment.payment_status === 'payment_failed') {
        navigation.replace('Booking', { paymentFailed: true });
        return;
      }
      if (attempt >= MAX_POLLS) {
        setTimedOut(true);
        return;
      }
      setTimeout(() => !cancelled && setAttempt(a => a + 1), POLL_MS);
    }
    poll();
    return () => { cancelled = true; };
  }, [attempt, shipmentId, navigation]);

  return (
    <View style={styles.container}>
      <ActivityIndicator size="large" color="#00E5FF" />
      <Text style={styles.text}>
        {timedOut ? 'Still confirming — check My Shipments shortly.' : 'Confirming your payment…'}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: CANVAS, alignItems: 'center', justifyContent: 'center', gap: 16 },
  text: { color: 'rgba(255,255,255,0.7)', fontSize: 14 },
});
```

- [ ] **Step 4: Register both screens in the navigator**

Read `apps/customer-app/src/navigation/AppNavigator.tsx` first to match its existing `Stack.Screen` registration style, then add:

```tsx
<Stack.Screen name="PaymentWebView" component={PaymentWebViewScreen} />
<Stack.Screen name="BookingConfirmationPending" component={BookingConfirmationPendingScreen} />
```

with the matching imports at the top of the file.

- [ ] **Step 5: Verify it type-checks**

Run: `cd apps/customer-app && npx tsc --noEmit`
Expected: no new errors.

- [ ] **Step 6: Commit**

```bash
git add apps/customer-app/src/screens/booking/PaymentWebView.tsx apps/customer-app/src/navigation/AppNavigator.tsx apps/customer-app/package.json
git commit -m "feat(customer-app): add payment WebView and pending-confirmation screens"
```

---

### Task 23: Wire "Pay Online" into `BookingScreen`

**Files:**
- Modify: `apps/customer-app/src/screens/booking/BookingScreen.tsx`

- [ ] **Step 1: Add state for the online-quote and payment-method choice**

Near the existing Step-3 state declarations (`BookingScreen.tsx:320-330`), add:

```tsx
  // ── Payment method (AE-region only) ───────────────────────────────────────
  const isAedTenant = useSelector((s: RootState) => s.auth.tenantCurrency === 'AED'); // confirm the exact Redux slice/field name against apps/customer-app/src/store's auth slice before wiring this — the currency claim from Task 3 needs to already be threaded into the session/auth state by whatever already reads other JWT-derived fields there (e.g. however loyaltyPoints/authEmail at the top of this file are populated); if no such field exists yet on the auth slice, add `tenantCurrency: string | null` to it now, populated the same way `authEmail` already is from the decoded session.
  const [payOnline, setPayOnline] = useState(false);
  const [onlineQuote, setOnlineQuote] = useState<{ amountCents: number; token: string } | null>(null);
  const [quoteLoading, setQuoteLoading] = useState(false);
```

- [ ] **Step 2: Refresh the quote when inputs change (AE tenants only)**

Add an effect near the other `useEffect`s in the file:

```tsx
  React.useEffect(() => {
    if (!isAedTenant || !payOnline) { setOnlineQuote(null); return; }
    const w = isIntl
      ? pieces.reduce((s, p) => s + (parseFloat(p.weight || '0') || 0), 0)
      : parseFloat(weight || '0');
    if (!w || w <= 0) return;
    const timer = setTimeout(async () => {
      setQuoteLoading(true);
      try {
        const quote = await shipmentsService.getShipmentQuote({
          service_type: isIntl ? (freightMode === 'sea' ? 'balikbayan' : 'express') : 'standard',
          weight_grams: Math.round(w * 1000),
          pieces: isIntl ? pieces.map(p => ({ weight_grams: Math.round(parseFloat(p.weight || '0') * 1000) })) : undefined,
        });
        setOnlineQuote({ amountCents: quote.amount_cents, token: quote.quote_token });
      } catch {
        showToast('Could not fetch a live quote — try again.', 'error');
        setOnlineQuote(null);
      } finally {
        setQuoteLoading(false);
      }
    }, 500);
    return () => clearTimeout(timer);
  }, [isAedTenant, payOnline, isIntl, weight, pieces, freightMode]);
```

- [ ] **Step 3: Add the toggle UI in Step 3 (local package details)**

Immediately after the existing "Fragile Item" toggle block (`BookingScreen.tsx:833-840`), add (gated on `isAedTenant`, matching how the rest of the screen already gates on `isIntl`):

```tsx
            {isAedTenant && (
              <View style={s.toggleRow}>
                <View style={{ flex: 1 }}>
                  <Text style={s.toggleLabel}>Pay Online</Text>
                  <Text style={s.toggleSub}>
                    {quoteLoading ? 'Getting price…' : onlineQuote ? `AED ${(onlineQuote.amountCents / 100).toFixed(2)}` : 'Pay by card instead of cash on pickup'}
                  </Text>
                </View>
                <Switch value={payOnline} onValueChange={setPayOnline}
                  trackColor={{ false: BORDER, true: GREEN + "60" }} thumbColor={payOnline ? GREEN : "rgba(255,255,255,0.3)"} />
              </View>
            )}
```

- [ ] **Step 4: Send `quote_token` and handle `checkout_url` in `handleBook`**

In `handleBook()`'s `shipmentsService.createShipment({...})` call (`BookingScreen.tsx:478-508`), add:

```tsx
        quote_token: payOnline && onlineQuote ? onlineQuote.token : undefined,
        idempotency_key: bookingIdempotencyKey.current, // see Step 5
```

Right after the existing `const response = await shipmentsService.createShipment({...});` line, before the rest of the current success-handling logic runs, add:

```tsx
      if (response.checkout_url) {
        navigation.navigate('PaymentWebView', {
          checkoutUrl: response.checkout_url,
          shipmentId: response.id,
          returnUrlPrefix: `${ORDER_INTAKE_RETURN_URL_BASE}/payment/return`, // confirm this constant's exact name/location — it must match whatever apps/customer-app already uses for its API base URL config (check services/api/client.ts), since it has to match the `return_url` prefix order-intake's ShipmentService.create() builds in Task 18
        });
        return; // don't run the immediate success UI below — the pending screen owns navigation from here
      }
```

leaving every existing line below that (the `dispatch(shipmentsActions.addShipment(...))`, loyalty points, `setConfirmedAwb`, etc.) untouched for the non-online-payment path.

- [ ] **Step 5: Add the idempotency key**

Near the top of `BookingScreen()` (with the other `useState`/`useRef` declarations), add:

```tsx
  const bookingIdempotencyKey = React.useRef(crypto.randomUUID?.() ?? Math.random().toString(36).slice(2));
```

Reset it in `handleBookAnother()` (`BookingScreen.tsx:565-572`) alongside the other state resets:

```tsx
    bookingIdempotencyKey.current = crypto.randomUUID?.() ?? Math.random().toString(36).slice(2);
```

Confirm `crypto.randomUUID` is actually available in this Expo/RN runtime before relying on it (check `apps/customer-app`'s Expo SDK version — if it needs a polyfill, check whether `expo-crypto`'s `randomUUID()` is already a dependency elsewhere in this app and use that instead, matching whatever the app already does for generating client-side UUIDs, e.g. wherever `SyncQueueEntity`-style offline records get their ids in the driver app's equivalent pattern — if this app has no existing UUID generation anywhere, add `expo-crypto` and use `Crypto.randomUUID()`).

- [ ] **Step 6: Verify it type-checks**

Run: `cd apps/customer-app && npx tsc --noEmit`
Expected: no new errors.

- [ ] **Step 7: Manual smoke test**

This step cannot be scripted — follow the project's `run` skill or start the Expo dev server (`cd apps/customer-app && npx expo start`) against a local stack with an AE-region tenant JWT (currency=AED), and walk through: Step 3 → toggle Pay Online → confirm a live quote appears → Book → WebView opens NI's sandbox checkout → complete a sandbox card payment → confirm the app lands on the real `BookingConfirmation` screen with a valid AWB, and that the shipment does NOT appear in `services/dispatch`'s queue until that payment completed (check `dispatch_queue` in the DB, or watch `payments`/`order-intake` logs for the `payment.intent.captured` → republish sequence).

- [ ] **Step 8: Commit**

```bash
git add apps/customer-app/src/screens/booking/BookingScreen.tsx
git commit -m "feat(customer-app): wire Pay Online into the booking flow for AE-region tenants"
```

---

### Task 24: Refund on cancellation of an already-paid shipment

Spec step 9 requires this and no earlier task wires it. Reusing `SHIPMENT_CANCELLED` (already published by `ShipmentService::cancel()`, unconditionally, for every cancellation reason) keeps this event-driven rather than adding a synchronous payments call inside `cancel()`'s request path — consistent with how the rest of this plan avoids synchronous cross-service calls once a booking exists.

**Files:**
- Create: `services/payments/src/infrastructure/messaging/shipment_cancelled_consumer.rs`
- Modify: `services/payments/src/infrastructure/messaging/mod.rs`
- Modify: `services/payments/src/domain/repositories/mod.rs` (extend `PaymentIntentRepository`)
- Modify: `services/payments/src/infrastructure/db/payment_intent_repo.rs`
- Modify: `services/payments/src/bootstrap.rs`

- [ ] **Step 1: Add a lookup the consumer needs**

`PaymentIntentRepository` has no way to find "the captured shipping_fee intent for this shipment." Add to the trait (`services/payments/src/domain/repositories/mod.rs`):

```rust
    /// The captured intent for a given (purpose, reference), if one exists —
    /// used by the shipment-cancellation consumer to decide whether a refund
    /// is owed. Returns `None` for a shipment that was never paid online
    /// (cash-at-pickup) — cancelling those must not attempt a refund call.
    async fn find_captured_by_reference(
        &self,
        purpose: &str,
        reference_type: &str,
        reference_id: Uuid,
    ) -> anyhow::Result<Option<PaymentIntent>>;
```

Implement it in `services/payments/src/infrastructure/db/payment_intent_repo.rs`:

```rust
    async fn find_captured_by_reference(
        &self,
        purpose: &str,
        reference_type: &str,
        reference_id: Uuid,
    ) -> anyhow::Result<Option<PaymentIntent>> {
        let query = format!(
            "SELECT {INTENT_COLS} FROM payments.payment_intents \
             WHERE purpose = $1 AND reference_type = $2 AND reference_id = $3 AND status = 'captured'"
        );
        let row = sqlx::query(&query)
            .bind(purpose).bind(reference_type).bind(reference_id)
            .fetch_optional(&self.pool).await?;
        Ok(row.as_ref().map(row_to_intent))
    }
```

- [ ] **Step 2: Write the consumer**

```rust
//! Kafka consumer for logisticos.order.shipment.cancelled. If the cancelled
//! shipment had a captured shipping_fee payment intent, refund it. A
//! shipment that was never paid online (the common case — cash at pickup)
//! has no captured intent and this is a no-op.
//!
//! A failed refund call must not be silently dropped (the design spec's
//! error-handling section is explicit about this) but must also not fail
//! the shipment's cancellation, which has already happened by the time this
//! consumer runs — the cancellation itself is not this consumer's job. It
//! logs at error level so it surfaces in alerting; a dedicated retry queue
//! is a reasonable follow-up once this has real production volume to justify
//! one, not a day-one requirement for the first AE-region rollout.

use std::sync::Arc;
use anyhow::Context;
use logisticos_events::{consumer::KafkaConsumer, topics};
use uuid::Uuid;

use crate::application::services::payment_intent_service::PaymentIntentService;
use crate::domain::repositories::PaymentIntentRepository;

pub struct ShipmentCancelledConsumer {
    inner: KafkaConsumer,
    intent_repo: Arc<dyn PaymentIntentRepository>,
    intent_service: Arc<PaymentIntentService>,
}

impl ShipmentCancelledConsumer {
    pub fn new(
        brokers: &str,
        group_id: &str,
        intent_repo: Arc<dyn PaymentIntentRepository>,
        intent_service: Arc<PaymentIntentService>,
    ) -> anyhow::Result<Self> {
        let inner = KafkaConsumer::new(brokers, &format!("{group_id}-shipment-cancelled"), &[topics::SHIPMENT_CANCELLED])
            .context("Failed to create ShipmentCancelledConsumer")?;
        Ok(Self { inner, intent_repo, intent_service })
    }

    pub async fn run(self) {
        let intent_repo = self.intent_repo;
        let intent_service = self.intent_service;
        let result = self.inner.run(move |_topic, json| {
            let intent_repo = Arc::clone(&intent_repo);
            let intent_service = Arc::clone(&intent_service);
            async move { handle(json, &intent_repo, &intent_service).await }
        }).await;
        if let Err(e) = result {
            tracing::error!("ShipmentCancelledConsumer loop exited with error: {e}");
        }
    }
}

async fn handle(
    json: serde_json::Value,
    intent_repo: &dyn PaymentIntentRepository,
    intent_service: &PaymentIntentService,
) -> anyhow::Result<()> {
    let data = json.get("data").cloned().unwrap_or(json.clone());
    let shipment_id: Uuid = data.get("shipment_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("shipment.cancelled event missing shipment_id"))?;

    let Some(intent) = intent_repo.find_captured_by_reference("shipping_fee", "shipment", shipment_id).await? else {
        return Ok(()); // never paid online — nothing to refund
    };

    if let Err(e) = intent_service.refund(intent.id).await {
        tracing::error!(shipment_id = %shipment_id, intent_id = %intent.id, error = %e, "refund failed after shipment cancellation — needs manual follow-up");
        // Deliberately Ok(()): the shipment is already cancelled regardless
        // of refund outcome (per the design spec), and returning Err here
        // would just cause Kafka to redeliver a refund call NI already
        // received once, risking a duplicate refund attempt rather than
        // fixing anything. The error above is what makes this visible.
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // Reuse whatever fake PaymentIntentRepository test double already exists
    // for PaymentIntentService's own tests, if any were added in Task 9 —
    // if Task 9 only had unit tests on PaymentIntent itself (it did — Task 9
    // has no PaymentIntentService-level tests), define a minimal in-memory
    // fake here: a Vec<PaymentIntent> behind a Mutex is sufficient.

    #[tokio::test]
    async fn shipment_never_paid_online_triggers_no_refund_call() {
        // repo.find_captured_by_reference returns None
        // handle(...) must return Ok(()) and the fake gateway must record zero refund calls
    }

    #[tokio::test]
    async fn shipment_with_a_captured_intent_triggers_exactly_one_refund_call() {
        // seed a Captured intent for (shipping_fee, shipment, shipment_id)
        // handle(...) must call intent_service.refund with that intent's id exactly once
    }
}
```

- [ ] **Step 3: Register and wire it**

Add `pub mod shipment_cancelled_consumer;` to `services/payments/src/infrastructure/messaging/mod.rs`, matching the file's existing style (check it first — Task 7-11 didn't touch this file, so its current contents are whatever `pod_consumer`/`weight_discrepancy_consumer`/`pickup_consumer`/`customs_duty_consumer` already established).

In `services/payments/src/bootstrap.rs`, after the existing `customs_duty_consumer` spawn block (`bootstrap.rs:277-286`), add:

```rust
    // Spawn shipment.cancelled consumer — refunds a captured shipping_fee
    // payment intent when its shipment is cancelled after payment cleared.
    let shipment_cancelled_consumer = crate::infrastructure::messaging::shipment_cancelled_consumer::ShipmentCancelledConsumer::new(
        &cfg.kafka.brokers,
        &cfg.kafka.group_id,
        Arc::clone(&payment_intent_repo) as _,
        Arc::clone(&payment_intent_service),
    )
    .context("Failed to create ShipmentCancelledConsumer")?;
    tokio::spawn(async move { shipment_cancelled_consumer.run().await });
```

- [ ] **Step 4: Verify it compiles and the tests pass**

Run: `cargo check -p logisticos-payments`
Expected: no errors.

Run: `cargo test -p logisticos-payments shipment_cancelled_consumer`
Expected: both new tests pass.

- [ ] **Step 5: Commit**

```bash
git add services/payments/src/infrastructure/messaging/shipment_cancelled_consumer.rs services/payments/src/infrastructure/messaging/mod.rs services/payments/src/domain/repositories/mod.rs services/payments/src/infrastructure/db/payment_intent_repo.rs services/payments/src/bootstrap.rs
git commit -m "feat(payments): refund a captured shipping-fee intent when its shipment is cancelled"
```

---

# Final verification (run after all 24 tasks)

- [ ] `cargo check --workspace` — confirm nothing outside the touched crates broke (a shared-crate change like `Currency`/`Claims` is the most likely source of a distant breakage).
- [ ] `cargo test -p logisticos-types -p logisticos-auth -p logisticos-events -p logisticos-identity -p logisticos-payments -p logisticos-order-intake`
- [ ] `cd apps/customer-app && npx tsc --noEmit`
- [ ] Re-read the design spec's "Testing" section and confirm each bullet has a corresponding task above: AE tariff unit tests (Task 12) ✓, quote-token sign/verify unit tests (Task 13) ✓, intent state-machine tests (Task 5) ✓, replay-safety (Task 5, Task 9's `apply_captured` idempotency, Task 19's consumer test) ✓, refund-on-cancel (Task 24) ✓. `PaymentIntentService::sweep_expired` has no direct unit test in Task 9 (it's exercised only transitively) — **add one** in `services/payments/src/application/services/payment_intent_service.rs` covering `sweep_expired` transitioning a `Created` intent past its `expires_at` to `Expired` and publishing `PAYMENT_INTENT_FAILED`, against a mocked `PaymentIntentRepository`/`PaymentGateway`/in-memory Kafka producer double, before considering this plan done. NI sandbox contract test and the full end-to-end test are **not** included as automated tasks above — they require live sandbox credentials this plan cannot obtain; leave them as the manual Step 7 in Task 23 plus a follow-up ticket to script the sandbox contract test once real NI sandbox credentials exist.
