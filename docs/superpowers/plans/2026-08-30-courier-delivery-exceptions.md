# Courier Delivery Exceptions — Phase 1 (field-ops backend)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give a courier a way to record that a delivery could not be completed, without changing any assignment state or moving any money.

**Architecture:** A new append-only table `field_ops.assignment_exceptions`, a `POST /v1/field-ops/assignments/:id/exception` endpoint following the existing `arrived`/`collected` milestone pattern, and a new `CourierEvent::ExceptionRaised` so OmniDeliv can flag the order for ops. The assignment's `status` is deliberately **not** touched and the courier ledger is **not** credited — both are Phase 2, after an ops decision closes the exception.

**Tech Stack:** Rust, Axum, SQLx (runtime `sqlx::query()`, no compile-time DB), Kafka via the existing `CourierEvents` trait, PostgreSQL schema `field_ops`.

---

## Decisions this plan encodes

Settled 2026-08-30. Do not re-litigate these while implementing:

| | Decision | Consequence in this plan |
|---|---|---|
| **D1** | No automatic refund. Record the failure, flag for manual ops resolution. | The endpoint touches no payment path. The Kafka event is what lets OmniDeliv flag the order. |
| **D2** | Do not change assignment status on the courier's report. | `raise_exception` never writes `AssignmentStatus`. The row carries `resolved_at IS NULL` until ops acts. |
| **D3** | A failed attempt earns a partial fee, set per product like `trip_cents`. | **Phase 2.** D2 forbids money moving on the courier's report alone, so the fee is credited at ops resolution, not here. No ledger call in this plan. |
| **D4** | Return-leg custody is out of scope. | The row carries `goods_disposition` as free text. No routing change. |

## Scope

This plan is the **backend only**, and produces working, testable software on its own: an endpoint that records exceptions, an ops read endpoint, and a published event.

Two follow-on plans are **not** covered here and should be written separately:
- **Plan B — courier app.** The reason sheet behind the existing `Report an issue` button, plus offline enqueue through `Outbound`.
- **Plan C — ops console.** The open-exceptions view in admin-portal.

Until Plan B lands, the button in `ManifestRoute.kt:322` is still an empty lambda. Hiding it is a separate one-line change, already recommended.

## File Structure

| File | Responsibility |
|---|---|
| `services/field-ops/migrations/0010_assignment_exceptions.sql` | *Create.* The table, its unique idempotency index, and the open-exception index. |
| `services/field-ops/src/domain/entities/exception.rs` | *Create.* `ExceptionReason` (closed set) and `AssignmentException` (the record). No I/O. |
| `services/field-ops/src/domain/entities/mod.rs` | *Modify.* Re-export the two new types. |
| `services/field-ops/src/infrastructure/db/exception_repo.rs` | *Create.* `ExceptionRepository` trait + `PgExceptionRepository`. |
| `services/field-ops/src/infrastructure/db/mod.rs` | *Modify.* Declare and re-export the module. |
| `services/field-ops/src/infrastructure/messaging/mod.rs` | *Modify.* `CourierEvent::ExceptionRaised` variant and its `key()` arm. |
| `services/field-ops/src/application/services/dispatch_service.rs` | *Modify.* `exceptions` field, constructor param, `raise_exception`, `open_exceptions`, and the test recorder's match arm. |
| `services/field-ops/src/api/http/couriers.rs` | *Modify.* Two routes, two handlers, two DTOs. |
| `services/field-ops/src/bootstrap.rs` | *Modify.* Construct `PgExceptionRepository` and pass it in. |
| `services/field-ops/tests/assignment_exceptions.rs` | *Create.* Integration test against a real Postgres. |

---

### Task 1: The table

**Files:**
- Create: `services/field-ops/migrations/0010_assignment_exceptions.sql`

- [ ] **Step 1: Write the migration**

```sql
-- A delivery that could not be completed.
--
-- WHY A ROW, WHEN `Arrived` IS PUBLISHED AND NEVER PERSISTED.
-- The milestone events are informational: they change no state, and anything
-- that missed one can reconcile from the next. An exception is the opposite.
-- It is the start of work somebody has to finish — a refund decision, a return
-- leg, a re-dispatch — and the courier who raised it has already walked away.
-- A published-only exception would be a task that exists for as long as the
-- topic's retention window and then silently stops existing.
--
-- WHY THIS DOES NOT TOUCH courier_assignments.status.
-- Decided 2026-08-30 (D2). The courier's report is a claim, not a verdict: the
-- goods are still in their bag and the money question is unanswered. Closing
-- the assignment here would credit or strand real money on one tap from a
-- phone that may be offline and retrying. `resolved_at IS NULL` is the open
-- queue; ops closes it, and Phase 2 is what moves anything.
--
-- WHY client_ref IS NOT NULL AND UNIQUE PER ASSIGNMENT.
-- The courier app queues writes offline and replays them, so this endpoint
-- WILL be called twice with the same intent. Without a client-supplied key,
-- the retry that a flaky connection guarantees becomes a second open exception
-- for ops to triage. The app generates it once, at the moment the courier taps
-- confirm, and reuses it for every replay of that same tap.
--
-- Dual timestamps per the platform's device_timestamp contract: the hardware
-- clock at the tap, and the cluster clock at receipt. SLA and response-time
-- questions use device_timestamp where it is present.
CREATE TABLE IF NOT EXISTS field_ops.assignment_exceptions (
    id                UUID        PRIMARY KEY,
    tenant_id         UUID        NOT NULL,
    assignment_id     UUID        NOT NULL REFERENCES field_ops.courier_assignments(id),
    courier_id        UUID        NOT NULL REFERENCES field_ops.couriers(id),

    -- Closed set, validated in Rust before it reaches here. TEXT rather than a
    -- Postgres enum so adding a reason is a deploy, not a migration with a
    -- lock — the set is expected to grow as ops learns what it is triaging.
    reason            TEXT        NOT NULL,
    note              TEXT,

    -- D4: where the goods ended up, in the courier's own words, until a return
    -- leg exists to model it properly.
    goods_disposition TEXT,

    capture_lat       DOUBLE PRECISION,
    capture_lng       DOUBLE PRECISION,
    client_ref        UUID        NOT NULL,

    device_timestamp  TIMESTAMPTZ,
    server_timestamp  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Written by Phase 2 only. Present now because `resolved_at IS NULL` is
    -- what makes the open queue a query rather than a scan of everything.
    resolved_at       TIMESTAMPTZ,
    resolved_by       UUID,
    resolution        TEXT
);

-- The idempotency guarantee the offline queue depends on.
CREATE UNIQUE INDEX IF NOT EXISTS assignment_exceptions_client_ref_key
    ON field_ops.assignment_exceptions (assignment_id, client_ref);

-- The ops queue: open exceptions for a tenant, oldest first, because the
-- longest-waiting customer is the one to answer next.
CREATE INDEX IF NOT EXISTS assignment_exceptions_open_idx
    ON field_ops.assignment_exceptions (tenant_id, server_timestamp)
    WHERE resolved_at IS NULL;
```

- [ ] **Step 2: Verify the SQL parses and applies**

Run against a scratch database (skip if you have no Postgres; CI will run it):

```bash
psql "$DATABASE_URL" -f services/field-ops/migrations/0010_assignment_exceptions.sql
```

Expected: `CREATE TABLE`, `CREATE INDEX`, `CREATE INDEX`. Re-running prints the same with no error, because every statement is `IF NOT EXISTS`.

- [ ] **Step 3: Commit**

```bash
git add services/field-ops/migrations/0010_assignment_exceptions.sql
git commit -m "feat(field-ops): table for courier delivery exceptions"
```

---

### Task 2: `ExceptionReason`, a closed set

**Files:**
- Create: `services/field-ops/src/domain/entities/exception.rs`
- Modify: `services/field-ops/src/domain/entities/mod.rs`

- [ ] **Step 1: Write the failing test**

Append to `services/field-ops/src/domain/entities/exception.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reason_round_trips_through_its_wire_string() {
        for r in ExceptionReason::ALL {
            assert_eq!(ExceptionReason::parse(r.as_str()), Some(*r));
        }
    }

    /// The set is closed on purpose. An unrecognised reason is a client that
    /// has drifted from the server, and accepting it would put a value in the
    /// ops queue that no triage rule knows how to route.
    #[test]
    fn an_unknown_reason_is_refused_rather_than_stored() {
        assert_eq!(ExceptionReason::parse("customer_was_rude"), None);
        assert_eq!(ExceptionReason::parse(""), None);
        assert_eq!(ExceptionReason::parse("CUSTOMER_UNREACHABLE"), None);
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops --lib exception
```

Expected: FAIL — `cannot find type ExceptionReason in this scope`.

- [ ] **Step 3: Write the implementation**

Put this **above** the `mod tests` block in `services/field-ops/src/domain/entities/exception.rs`:

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Why a delivery could not be completed.
///
/// Deliberately small. Each value earns its place by changing what ops does
/// next; anything finer belongs in `note`, which a human reads. A set that
/// grows to cover every story a courier might tell becomes a set nobody
/// filters on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExceptionReason {
    /// No answer at the door or by phone.
    CustomerUnreachable,
    /// The pin is wrong, blocked, or cannot be entered.
    AddressUnreachable,
    /// The recipient declined the goods.
    CustomerRefused,
    /// COD order, and the customer has no cash.
    CannotPay,
    /// Damaged in transit or at pickup.
    GoodsDamaged,
    /// Accident, breakdown, or a safety problem. About the courier, not the order.
    CourierBlocked,
}

impl ExceptionReason {
    pub const ALL: &'static [ExceptionReason] = &[
        ExceptionReason::CustomerUnreachable,
        ExceptionReason::AddressUnreachable,
        ExceptionReason::CustomerRefused,
        ExceptionReason::CannotPay,
        ExceptionReason::GoodsDamaged,
        ExceptionReason::CourierBlocked,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ExceptionReason::CustomerUnreachable => "customer_unreachable",
            ExceptionReason::AddressUnreachable  => "address_unreachable",
            ExceptionReason::CustomerRefused     => "customer_refused",
            ExceptionReason::CannotPay           => "cannot_pay",
            ExceptionReason::GoodsDamaged        => "goods_damaged",
            ExceptionReason::CourierBlocked      => "courier_blocked",
        }
    }

    /// Case-sensitive on purpose: the wire format is one spelling, and
    /// accepting variants of it invites two clients that disagree.
    pub fn parse(s: &str) -> Option<ExceptionReason> {
        ExceptionReason::ALL.iter().copied().find(|r| r.as_str() == s)
    }
}
```

- [ ] **Step 4: Register the module**

In `services/field-ops/src/domain/entities/mod.rs`, add alongside the existing module declarations and re-exports:

```rust
pub mod exception;
pub use exception::{AssignmentException, ExceptionReason};
```

`AssignmentException` does not exist yet — Task 3 adds it. Do Task 3 before compiling, or add only the `ExceptionReason` half of the re-export now and widen it in Task 3.

- [ ] **Step 5: Run the test to verify it passes**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops --lib exception
```

Expected: PASS, 2 tests.

- [ ] **Step 6: Commit**

```bash
git add services/field-ops/src/domain/entities/exception.rs services/field-ops/src/domain/entities/mod.rs
git commit -m "feat(field-ops): closed set of delivery-exception reasons"
```

---

### Task 3: `AssignmentException`, the record

**Files:**
- Modify: `services/field-ops/src/domain/entities/exception.rs`

- [ ] **Step 1: Write the failing test**

Add inside the existing `mod tests` block:

```rust
#[test]
fn a_new_exception_is_open_and_stamps_the_server_clock() {
    let e = AssignmentException::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        ExceptionReason::CannotPay,
        Some("no cash, asked to pay by card".to_owned()),
        Some("left with the customer's neighbour".to_owned()),
        Some((14.5995, 120.9842)),
        Uuid::new_v4(),
        None,
    );

    assert!(e.resolved_at.is_none(), "a new exception is open");
    assert_eq!(e.reason, ExceptionReason::CannotPay);
    assert_eq!(e.capture_lat, Some(14.5995));
    assert!(e.device_timestamp.is_none());
}

/// The courier's own clock is kept even when it disagrees with ours: a phone
/// that queued this offline is the only witness to when it actually happened.
#[test]
fn a_device_timestamp_is_preserved_rather_than_replaced() {
    let tapped = chrono::Utc::now() - chrono::Duration::hours(2);
    let e = AssignmentException::new(
        Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
        ExceptionReason::CustomerUnreachable,
        None, None, None, Uuid::new_v4(), Some(tapped),
    );
    assert_eq!(e.device_timestamp, Some(tapped));
    assert!(e.server_timestamp > tapped);
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops --lib exception
```

Expected: FAIL — `cannot find struct AssignmentException`.

- [ ] **Step 3: Write the implementation**

Add above `mod tests` in the same file:

```rust
/// One report from a courier that a delivery could not be completed.
///
/// Append-only in practice: nothing in Phase 1 updates a row after it is
/// written. Phase 2 sets the `resolved_*` trio and nothing else.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentException {
    pub id:            Uuid,
    pub tenant_id:     Uuid,
    pub assignment_id: Uuid,
    pub courier_id:    Uuid,
    pub reason:        ExceptionReason,
    pub note:          Option<String>,
    /// D4: where the goods ended up, in the courier's words.
    pub goods_disposition: Option<String>,
    pub capture_lat:   Option<f64>,
    pub capture_lng:   Option<f64>,
    /// Supplied by the app, stable across offline replays of the same tap.
    pub client_ref:    Uuid,
    /// The phone's clock at the tap. Absent for anything not raised on a device.
    pub device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub server_timestamp: chrono::DateTime<chrono::Utc>,
    pub resolved_at:   Option<chrono::DateTime<chrono::Utc>>,
    pub resolved_by:   Option<Uuid>,
    pub resolution:    Option<String>,
}

impl AssignmentException {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id: Uuid,
        assignment_id: Uuid,
        courier_id: Uuid,
        reason: ExceptionReason,
        note: Option<String>,
        goods_disposition: Option<String>,
        capture: Option<(f64, f64)>,
        client_ref: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            assignment_id,
            courier_id,
            reason,
            note,
            goods_disposition,
            capture_lat: capture.map(|c| c.0),
            capture_lng: capture.map(|c| c.1),
            client_ref,
            device_timestamp,
            server_timestamp: chrono::Utc::now(),
            resolved_at: None,
            resolved_by: None,
            resolution: None,
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops --lib exception
```

Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add services/field-ops/src/domain/entities/exception.rs services/field-ops/src/domain/entities/mod.rs
git commit -m "feat(field-ops): AssignmentException record"
```

---

### Task 4: The repository

**Files:**
- Create: `services/field-ops/src/infrastructure/db/exception_repo.rs`
- Modify: `services/field-ops/src/infrastructure/db/mod.rs`

- [ ] **Step 1: Write the repository**

There is no unit test here: this is SQL, and the behaviour that matters (the idempotent insert) is proved against a real database in Task 9. Create `services/field-ops/src/infrastructure/db/exception_repo.rs`:

```rust
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{AssignmentException, ExceptionReason};

#[async_trait::async_trait]
pub trait ExceptionRepository: Send + Sync {
    /// Returns false when this `(assignment_id, client_ref)` was already
    /// recorded — the offline queue replaying a tap, not a second failure.
    async fn record(&self, e: &AssignmentException) -> anyhow::Result<bool>;

    /// Open exceptions for a tenant, oldest first.
    async fn list_open(&self, tenant_id: Uuid, limit: i64)
        -> anyhow::Result<Vec<AssignmentException>>;
}

pub struct PgExceptionRepository {
    pool: PgPool,
}

impl PgExceptionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ExceptionRepository for PgExceptionRepository {
    async fn record(&self, e: &AssignmentException) -> anyhow::Result<bool> {
        // ON CONFLICT DO NOTHING against the (assignment_id, client_ref) index.
        // The app replays queued writes, so a duplicate is the expected case
        // and not an error: it returns false and the caller stays quiet.
        let done = sqlx::query(
            "INSERT INTO field_ops.assignment_exceptions
                 (id, tenant_id, assignment_id, courier_id, reason, note,
                  goods_disposition, capture_lat, capture_lng, client_ref,
                  device_timestamp, server_timestamp)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (assignment_id, client_ref) DO NOTHING",
        )
        .bind(e.id)
        .bind(e.tenant_id)
        .bind(e.assignment_id)
        .bind(e.courier_id)
        .bind(e.reason.as_str())
        .bind(e.note.as_deref())
        .bind(e.goods_disposition.as_deref())
        .bind(e.capture_lat)
        .bind(e.capture_lng)
        .bind(e.client_ref)
        .bind(e.device_timestamp)
        .bind(e.server_timestamp)
        .execute(&self.pool)
        .await?;

        Ok(done.rows_affected() == 1)
    }

    async fn list_open(&self, tenant_id: Uuid, limit: i64)
        -> anyhow::Result<Vec<AssignmentException>> {
        let rows = sqlx::query(
            "SELECT id, tenant_id, assignment_id, courier_id, reason, note,
                    goods_disposition, capture_lat, capture_lng, client_ref,
                    device_timestamp, server_timestamp,
                    resolved_at, resolved_by, resolution
               FROM field_ops.assignment_exceptions
              WHERE tenant_id = $1 AND resolved_at IS NULL
              ORDER BY server_timestamp ASC
              LIMIT $2",
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| {
                let reason: String = r.try_get("reason")?;
                Ok(AssignmentException {
                    id:            r.try_get("id")?,
                    tenant_id:     r.try_get("tenant_id")?,
                    assignment_id: r.try_get("assignment_id")?,
                    courier_id:    r.try_get("courier_id")?,
                    // A row written by an older deploy can hold a reason this
                    // build no longer knows. Failing the whole ops queue over
                    // one unrecognised string is the wrong trade, so it lands
                    // as CourierBlocked — the value that means "a human has to
                    // look" — rather than dropping the row silently.
                    reason: ExceptionReason::parse(&reason)
                        .unwrap_or(ExceptionReason::CourierBlocked),
                    note:              r.try_get("note")?,
                    goods_disposition: r.try_get("goods_disposition")?,
                    capture_lat:       r.try_get("capture_lat")?,
                    capture_lng:       r.try_get("capture_lng")?,
                    client_ref:        r.try_get("client_ref")?,
                    device_timestamp:  r.try_get("device_timestamp")?,
                    server_timestamp:  r.try_get("server_timestamp")?,
                    resolved_at:       r.try_get("resolved_at")?,
                    resolved_by:       r.try_get("resolved_by")?,
                    resolution:        r.try_get("resolution")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(Into::into)
    }
}
```

- [ ] **Step 2: Register the module**

In `services/field-ops/src/infrastructure/db/mod.rs`, alongside the existing `pub mod` lines and re-exports:

```rust
pub mod exception_repo;
pub use exception_repo::{ExceptionRepository, PgExceptionRepository};
```

- [ ] **Step 3: Verify it compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops
```

Expected: no errors. (`cargo check`, not `build` — linking is what fills the dev disk.)

- [ ] **Step 4: Commit**

```bash
git add services/field-ops/src/infrastructure/db/exception_repo.rs services/field-ops/src/infrastructure/db/mod.rs
git commit -m "feat(field-ops): exception repository with idempotent insert"
```

---

### Task 5: The event

**Files:**
- Modify: `services/field-ops/src/infrastructure/messaging/mod.rs`
- Modify: `services/field-ops/src/application/services/dispatch_service.rs` (test recorder only)

- [ ] **Step 1: Add the variant**

In `services/field-ops/src/infrastructure/messaging/mod.rs`, add to `enum CourierEvent` after the `Delivered` variant:

```rust
    /// A courier reported that a delivery could not be completed.
    ///
    /// Unlike the other four this one is persisted as well as published — see
    /// migration 0010. The event exists so the offering product can flag its
    /// own order: OmniDeliv needs to know an order needs a human, and D1 says
    /// the refund decision is made out of band rather than here.
    ///
    /// Carries no money and implies no state change. A consumer that treats
    /// this as a terminal status is wrong: the assignment is still `claimed`.
    ExceptionRaised { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid,
                      exception_id: Uuid, reason: String,
                      device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
```

- [ ] **Step 2: Add the partition-key arm**

In the same file, in `CourierEvent::key()`, extend the match so the new variant keys by `external_ref` like the rest:

```rust
            CourierEvent::Assigned { external_ref, .. }
            | CourierEvent::Arrived { external_ref, .. }
            | CourierEvent::Collected { external_ref, .. }
            | CourierEvent::Delivered { external_ref, .. }
            | CourierEvent::ExceptionRaised { external_ref, .. } => *external_ref,
```

- [ ] **Step 3: Fix the test recorder the new variant breaks**

The match in `dispatch_service.rs` (around line 1774, inside `mod tests`) is exhaustive and will now fail to compile. Add the arm:

```rust
                CourierEvent::Assigned  { .. } => "assigned",
                CourierEvent::Arrived   { .. } => "arrived",
                CourierEvent::Collected { .. } => "collected",
                CourierEvent::Delivered { .. } => "delivered",
                CourierEvent::ExceptionRaised { .. } => "exception_raised",
```

- [ ] **Step 4: Verify it compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops --all-targets
```

Expected: no errors. `--all-targets` matters — without it the test module is not compiled and the broken match is not seen.

- [ ] **Step 5: Commit**

```bash
git add services/field-ops/src/infrastructure/messaging/mod.rs services/field-ops/src/application/services/dispatch_service.rs
git commit -m "feat(field-ops): CourierEvent::ExceptionRaised"
```

---

### Task 6: `raise_exception` on the dispatch service

**Files:**
- Modify: `services/field-ops/src/application/services/dispatch_service.rs`
- Modify: `services/field-ops/src/bootstrap.rs`

- [ ] **Step 1: Write the failing test**

Add inside the existing `mod tests` in `dispatch_service.rs`. It needs an in-memory `ExceptionRepository`; define it in the test module:

```rust
    #[derive(Default)]
    struct MemExceptions {
        recorded: Mutex<Vec<AssignmentException>>,
    }

    #[async_trait::async_trait]
    impl crate::infrastructure::db::ExceptionRepository for MemExceptions {
        async fn record(&self, e: &AssignmentException) -> anyhow::Result<bool> {
            let mut g = self.recorded.lock().unwrap();
            if g.iter().any(|x| x.assignment_id == e.assignment_id && x.client_ref == e.client_ref) {
                return Ok(false);
            }
            g.push(e.clone());
            Ok(true)
        }
        async fn list_open(&self, tenant_id: Uuid, _limit: i64)
            -> anyhow::Result<Vec<AssignmentException>> {
            Ok(self.recorded.lock().unwrap().iter()
                .filter(|e| e.tenant_id == tenant_id).cloned().collect())
        }
    }
```

Then the behaviour tests:

```rust
    /// The load-bearing guarantee of D2: the report is a claim, not a verdict.
    /// If this ever fails, a courier's tap is settling an assignment whose
    /// goods are still in their bag.
    #[tokio::test]
    async fn raising_an_exception_leaves_the_assignment_claimed_and_pays_nothing() {
        let h = Harness::new().await;
        let (assignment_id, user_id) = h.claimed_job().await;

        let raised = h.svc.raise_exception(
            h.tenant, user_id, assignment_id,
            ExceptionReason::CannotPay,
            Some("no cash".into()), None, None, Uuid::new_v4(), None,
        ).await.unwrap();

        assert!(raised);
        let a = h.assignments.find_by_id(h.tenant, assignment_id).await.unwrap().unwrap();
        assert_eq!(a.status, AssignmentStatus::Claimed, "status must not move");
        assert!(h.ledger_entries().is_empty(), "no money moves on a courier's report");
        assert!(h.emitted().contains(&"exception_raised"));
    }

    /// The offline queue replays. A replayed tap is one exception, and it must
    /// not publish a second event either — ops would triage the same failure twice.
    #[tokio::test]
    async fn replaying_the_same_client_ref_records_once_and_emits_once() {
        let h = Harness::new().await;
        let (assignment_id, user_id) = h.claimed_job().await;
        let client_ref = Uuid::new_v4();

        for _ in 0..3 {
            h.svc.raise_exception(
                h.tenant, user_id, assignment_id,
                ExceptionReason::CustomerUnreachable,
                None, None, None, client_ref, None,
            ).await.unwrap();
        }

        assert_eq!(h.svc.open_exceptions(h.tenant, 50).await.unwrap().len(), 1);
        assert_eq!(h.emitted().iter().filter(|e| **e == "exception_raised").count(), 1);
    }

    /// Assignment ids are handed to the offering product, so they are not
    /// secret. Without the ownership check any authenticated user in the tenant
    /// could raise an exception against somebody else's job.
    #[tokio::test]
    async fn another_couriers_job_cannot_be_failed() {
        let h = Harness::new().await;
        let (assignment_id, _owner) = h.claimed_job().await;
        let stranger = Uuid::new_v4();

        let raised = h.svc.raise_exception(
            h.tenant, stranger, assignment_id,
            ExceptionReason::GoodsDamaged,
            None, None, None, Uuid::new_v4(), None,
        ).await.unwrap();

        assert!(!raised);
        assert!(h.svc.open_exceptions(h.tenant, 50).await.unwrap().is_empty());
    }
```

**Corrected during execution.** There is no `Harness`. The milestone tests live in `mod milestone_authorization` and use `fixture()` / `fixture_in(AssignmentStatus)` returning a `Fixture { svc, assignment, holder, holder_courier, other, assignments, ledgers, events }`, with a module-level `const TENANT`. Assert through `f.assignments.rows`, `f.ledgers.saved` and `f.events.emitted`, and read the queue back through `f.svc.open_exceptions(TENANT, 50)` rather than holding a second handle on the store — an unused fixture field fails clippy.

- [ ] **Step 2: Run and watch it fail**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops --lib raise_exception
```

Expected: FAIL — `no method named raise_exception`.

- [ ] **Step 3: Add the repository to the service**

In `DispatchService`, add the field:

```rust
    exceptions:  Arc<dyn ExceptionRepository>,
```

Add `ExceptionRepository` to the existing `use crate::infrastructure::db::{...}` import, add a matching parameter to `DispatchService::new`, and assign it. Then update every call site:

```bash
grep -rn "DispatchService::new" --include="*.rs" services/field-ops/
```

**Corrected during execution.** There are **eleven** call sites: ten across the test modules and one in `bootstrap.rs`. Add the parameter **after `events`, not beside the other repositories** — every test site is positional, and grouping it with the repos means editing ten call sites to move one argument. Nine of the eleven test modules `use super::*`, so a single `#[cfg(test)] struct NoExceptions` at file scope covers all of them. `DispatchService::new` then has 8 arguments and needs `#[allow(clippy::too_many_arguments)]`, or CI's clippy gate fails. In `bootstrap.rs`, construct it next to the other repositories:

```rust
    let exceptions = Arc::new(PgExceptionRepository::new(pool.clone()));
```

and pass `exceptions.clone()` into `DispatchService::new`, importing `PgExceptionRepository` from `crate::infrastructure::db`.

- [ ] **Step 4: Write the two service methods**

Add to `impl DispatchService`, directly after `mark_arrived` so the milestone methods stay together:

```rust
    /// A courier reports that this job cannot be completed.
    ///
    /// Records and publishes. Deliberately changes no assignment status and
    /// credits no ledger — see D2 and D3, 2026-08-30. The goods are still with
    /// the courier and the money question is open; both are settled when ops
    /// resolves the exception, not here.
    ///
    /// Returns false when the job is not this courier's, or is not `Claimed` —
    /// the same shape `mark_arrived` uses, which the handler turns into a 404.
    #[allow(clippy::too_many_arguments)]
    pub async fn raise_exception(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        assignment_id: Uuid,
        reason: ExceptionReason,
        note: Option<String>,
        goods_disposition: Option<String>,
        capture: Option<(f64, f64)>,
        client_ref: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<bool> {
        let Some(a) = self.assignment_for_courier(tenant_id, user_id, assignment_id).await? else {
            return Ok(false);
        };
        // Being offered a job is not carrying it, and a completed job has
        // already ended. Only a claimed job can fail.
        if a.status != AssignmentStatus::Claimed {
            return Ok(false);
        }

        let e = AssignmentException::new(
            tenant_id, assignment_id, a.courier_id, reason,
            note, goods_disposition, capture, client_ref, device_timestamp,
        );

        // Persist before publishing. The row is the ops queue and the event is
        // a notification; a published exception with no row is a task nobody
        // can find once the topic's retention window closes.
        let fresh = self.exceptions.record(&e).await?;
        if !fresh {
            // The offline queue replaying a tap already recorded. Not an
            // error, and emphatically not a second event.
            return Ok(true);
        }

        self.emit(CourierEvent::ExceptionRaised {
            tenant_id,
            product: a.product.as_str().to_string(),
            external_ref: a.external_ref,
            courier_id: a.courier_id,
            exception_id: e.id,
            reason: reason.as_str().to_string(),
            device_timestamp,
        })
        .await;
        Ok(true)
    }

    /// The ops queue: unresolved exceptions, oldest first.
    pub async fn open_exceptions(&self, tenant_id: Uuid, limit: i64)
        -> anyhow::Result<Vec<AssignmentException>> {
        self.exceptions.list_open(tenant_id, limit).await
    }
```

Add `AssignmentException` and `ExceptionReason` to the `use crate::domain::entities::{...}` import at the top of the file.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops --lib
```

Expected: PASS, including the three new tests and every pre-existing one.

- [ ] **Step 6: Commit**

```bash
git add services/field-ops/src/application/services/dispatch_service.rs services/field-ops/src/bootstrap.rs
git commit -m "feat(field-ops): raise_exception records and publishes, moves nothing"
```

---

### Task 7: The courier endpoint

**Files:**
- Modify: `services/field-ops/src/api/http/couriers.rs`

- [ ] **Step 1: Add the request DTO**

Next to the existing `ArrivedRequest` / `CollectedRequest` structs:

```rust
#[derive(serde::Deserialize)]
struct ExceptionRequest {
    /// One of the values in `ExceptionReason`. An unknown string is a 400,
    /// not a stored row.
    reason: String,
    #[serde(default)] note: Option<String>,
    /// D4: where the goods ended up, in the courier's words.
    #[serde(default)] goods_disposition: Option<String>,
    #[serde(default)] lat: Option<f64>,
    #[serde(default)] lng: Option<f64>,
    /// Generated once by the app at the moment of the tap and reused for every
    /// offline replay of it. Required: without it a retry is a second exception.
    client_ref: Uuid,
    #[serde(default)] device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}
```

- [ ] **Step 2: Add the handler**

Next to `async fn arrived`:

```rust
async fn raise_exception(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(req): Json<ExceptionRequest>,
) -> Result<StatusCode, StatusCode> {
    let Some(reason) = ExceptionReason::parse(&req.reason) else {
        return Err(StatusCode::BAD_REQUEST);
    };

    // Both or neither. A half-supplied fix is a point on the equator.
    let capture = match (req.lat, req.lng) {
        (Some(lat), Some(lng)) => Some((lat, lng)),
        (None, None) => None,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let found = st
        .dispatch
        .raise_exception(
            claims.tenant_id, claims.user_id, id, reason,
            req.note, req.goods_disposition, capture,
            req.client_ref, req.device_timestamp,
        )
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "raise exception failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !found {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::ACCEPTED)
}
```

Add `ExceptionReason` to the file's `use crate::domain::entities::{...}` import.

- [ ] **Step 3: Add the route**

In the router builder, after the `delivered` route:

```rust
        .route("/v1/field-ops/assignments/:id/exception", post(raise_exception))
```

The api-gateway already routes the `/v1/field-ops` prefix, so no gateway change is needed.

- [ ] **Step 4: Verify it compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops --all-targets
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add services/field-ops/src/api/http/couriers.rs
git commit -m "feat(field-ops): POST /v1/field-ops/assignments/:id/exception"
```

---

### Task 8: The ops read endpoint

**Files:**
- Modify: `services/field-ops/src/api/http/couriers.rs`

- [ ] **Step 1: Add the response DTO and handler**

Following the shape of the existing `list_couriers` admin handler:

```rust
#[derive(serde::Serialize)]
struct OpenExceptionRow {
    id:                Uuid,
    assignment_id:     Uuid,
    courier_id:        Uuid,
    reason:            String,
    note:              Option<String>,
    goods_disposition: Option<String>,
    device_timestamp:  Option<chrono::DateTime<chrono::Utc>>,
    server_timestamp:  chrono::DateTime<chrono::Utc>,
}

async fn list_open_exceptions(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<Vec<OpenExceptionRow>>, StatusCode> {
    // Same permission the courier roster uses: this is ops-facing data about
    // couriers and the jobs they hold.
    if !claims.has_permission(logisticos_auth::rbac::permissions::DRIVER_READ) {
        return Err(StatusCode::FORBIDDEN);
    }

    let rows = st
        .dispatch
        .open_exceptions(claims.tenant_id, 200)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "listing open exceptions failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(rows.into_iter().map(|e| OpenExceptionRow {
        id:                e.id,
        assignment_id:     e.assignment_id,
        courier_id:        e.courier_id,
        reason:            e.reason.as_str().to_string(),
        note:              e.note,
        goods_disposition: e.goods_disposition,
        device_timestamp:  e.device_timestamp,
        server_timestamp:  e.server_timestamp,
    }).collect()))
}
```

Confirm the permission constant before using it:

```bash
grep -rn "COURIERS_READ\|pub const COURIERS" libs/auth/src/rbac/
```

**Corrected during execution:** `COURIERS_READ` does not exist. `list_couriers` checks `DRIVER_READ`, and that is what this handler uses. Do not invent a new permission — one no role holds produces a clean 403 that reads as "you lack access".

- [ ] **Step 2: Add the route**

```rust
        .route("/v1/field-ops/admin/exceptions", get(list_open_exceptions))
```

- [ ] **Step 3: Verify it compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops --all-targets
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add services/field-ops/src/api/http/couriers.rs
git commit -m "feat(field-ops): ops endpoint listing open delivery exceptions"
```

---

### Task 9: Integration test against a real database

**Files:**
- Create: `services/field-ops/tests/assignment_exceptions.rs`

- [ ] **Step 1: Write the test**

Copy the `database_url()` helper from `services/field-ops/tests/claim_race.rs` verbatim — it skips locally without `DATABASE_URL` and is fatal in CI, and that asymmetry is deliberate.

```rust
//! The idempotency guarantee the courier app's offline queue depends on,
//! proved against a real database rather than an in-memory double.

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use logisticos_field_ops::domain::entities::{
    AssignmentException, Courier, CourierAssignment, ExceptionReason, ProductKey,
};
use logisticos_field_ops::domain::repositories::CourierRepository;
use logisticos_field_ops::infrastructure::db::{
    AssignmentRepository, ExceptionRepository, PgAssignmentRepository, PgCourierRepository,
    PgExceptionRepository,
};

// ... paste database_url() from claim_race.rs here ...

#[tokio::test]
async fn a_replayed_client_ref_inserts_exactly_once() {
    let Some(url) = database_url() else { return };
    let pool = PgPoolOptions::new().connect(&url).await.expect("connect");

    let tenant = Uuid::new_v4();
    let couriers = PgCourierRepository::new(pool.clone());
    let assignments = PgAssignmentRepository::new(pool.clone());
    let exceptions = PgExceptionRepository::new(pool.clone());

    let courier = Courier::new(tenant, Uuid::new_v4(), "A".into(), "B".into(), "+63".into());
    couriers.save(&courier).await.unwrap();

    let a = CourierAssignment::offer_with_earnings(
        tenant, courier.id, ProductKey::new("omnideliv"), Uuid::new_v4(), 4_500, 0, 9_100,
    );
    assignments.save(&a).await.unwrap();

    let client_ref = Uuid::new_v4();
    let mut inserted = 0;
    for _ in 0..3 {
        let e = AssignmentException::new(
            tenant, a.id, courier.id, ExceptionReason::CannotPay,
            Some("no cash at the door".into()), None, Some((14.5995, 120.9842)),
            client_ref, None,
        );
        if exceptions.record(&e).await.unwrap() {
            inserted += 1;
        }
    }

    assert_eq!(inserted, 1, "a replayed tap is one exception");

    let open = exceptions.list_open(tenant, 50).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].reason, ExceptionReason::CannotPay);
    assert_eq!(open[0].note.as_deref(), Some("no cash at the door"));
    assert!(open[0].resolved_at.is_none());
}

#[tokio::test]
async fn two_distinct_failures_on_one_assignment_are_both_kept() {
    let Some(url) = database_url() else { return };
    let pool = PgPoolOptions::new().connect(&url).await.expect("connect");

    let tenant = Uuid::new_v4();
    let couriers = PgCourierRepository::new(pool.clone());
    let assignments = PgAssignmentRepository::new(pool.clone());
    let exceptions = PgExceptionRepository::new(pool.clone());

    let courier = Courier::new(tenant, Uuid::new_v4(), "A".into(), "B".into(), "+63".into());
    couriers.save(&courier).await.unwrap();
    let a = CourierAssignment::offer_with_earnings(
        tenant, courier.id, ProductKey::new("omnideliv"), Uuid::new_v4(), 4_500, 0, 0,
    );
    assignments.save(&a).await.unwrap();

    for reason in [ExceptionReason::CustomerUnreachable, ExceptionReason::AddressUnreachable] {
        let e = AssignmentException::new(
            tenant, a.id, courier.id, reason, None, None, None, Uuid::new_v4(), None,
        );
        assert!(exceptions.record(&e).await.unwrap());
    }

    assert_eq!(exceptions.list_open(tenant, 50).await.unwrap().len(), 2);
}
```

- [ ] **Step 2: Run it**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops --test assignment_exceptions
```

Expected with a Postgres available and migration 0010 applied: PASS, 2 tests. Expected without `DATABASE_URL` locally: PASS, 2 tests, both having returned early — this is the documented skip, and CI will fail loudly if the database is missing there.

- [ ] **Step 3: Run the whole service suite**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops
```

Expected: PASS, with no pre-existing test broken.

- [ ] **Step 4: Commit**

```bash
git add services/field-ops/tests/assignment_exceptions.rs
git commit -m "test(field-ops): exception idempotency against a real database"
```

---

## Done when

- [ ] `POST /v1/field-ops/assignments/:id/exception` records a reason, note, disposition, fix and both timestamps, and 400s on an unknown reason.
- [ ] Replaying the same `client_ref` records one row and publishes one event.
- [ ] Another courier's assignment cannot be failed (404).
- [ ] `AssignmentStatus` is unchanged and the courier ledger is untouched by every path in this plan.
- [ ] `GET /v1/field-ops/admin/exceptions` returns the open queue, oldest first.
- [ ] `CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops` is green.

## Deliberately not in this plan

- **No refund.** D1(b): out of band. Nothing here touches a payment path.
- **No ledger credit.** D3's attempt fee is Phase 2, because D2 forbids money moving on the courier's report alone.
- **No status transition.** D2(b). `AssignmentStatus` gains no variant.
- **No return leg.** D4. `goods_disposition` is free text.
- **No courier app change.** Plan B.
- **No ops console UI.** Plan C — this plan only provides the endpoint it will read.
