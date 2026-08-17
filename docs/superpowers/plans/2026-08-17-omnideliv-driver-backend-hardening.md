# OmniDeliv Driver Backend — Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the four latent defects that become exploitable the moment an OmniDeliv driver app holds a `field-ops` assignment id, so the driver-facing surface can be built on top of endpoints that are already safe.

**Architecture:** Five self-contained changes across two Rust services. Three concern authorization (a caller must hold the assignment it acts on); one concerns idempotency (a retried delivery must not pay twice, across an ISO-week boundary); one tightens an image magic-byte check that proof photos are about to depend on. No new endpoints, no new columns beyond two denormalised ones on the ledger entry table, no client changes.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL, Kafka (rdkafka), Tokio, `#[tokio::test]`.

**Spec:** `docs/superpowers/specs/2026-08-17-omnideliv-driver-app-design.md` §Hardening (H1–H4).

**This is plan 1 of 3.** Plan 2 is the driver-facing surface (`offer_card`, `courier_user_id` on `Assigned`, `POST /assignments/:id/arrived`, `GET /v1/omnideliv/courier/jobs/{order_id}`). Plan 3 is the Kotlin app. This plan must land first: plan 2 and 3 build clients against these endpoints, and the offline queue in plan 3 is the single feature most likely to trigger the retry defect fixed here.

---

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `services/field-ops/src/application/services/dispatch_service.rs` | Ownership checks on `mark_collected` / `mark_delivered`; cross-period idempotency in `credit_courier`; holder-or-service check in `position_for_assignment` | 1, 2, 4 |
| `services/field-ops/src/api/http/couriers.rs` | Pass `claims.user_id` / `claims` through to the three handlers | 1, 2, 4 |
| `services/field-ops/src/infrastructure/db/ledger_repo.rs` | New `entry_exists_for_job` query; write the denormalised columns | 2 |
| `services/field-ops/migrations/0007_ledger_entry_job_idempotency.sql` | Denormalise `tenant_id`/`courier_id` onto entries, backfill, unique index | 2 |
| `services/omnideliv/src/infrastructure/messaging/courier_consumer.rs` | Make the `Delivered` branch idempotent, symmetric with `Collected` | 3 |
| `services/omnideliv/src/infrastructure/storage.rs` | Correct WebP magic-byte detection | 5 |

---

## Pre-flight

- [ ] **Step 1: Confirm the toolchain and set the incremental guard**

The C: drive fills during long builds and `link.exe` exit 1318 is a disk-full error, not a code error. `cargo check` skips linking and is sufficient here.

```bash
export CARGO_INCREMENTAL=0
cargo check -p logisticos-field-ops -p logisticos-omnideliv
```

Expected: `Finished` with no errors. If crate names differ, read them from `services/field-ops/Cargo.toml` and `services/omnideliv/Cargo.toml` and use those for every command below.

- [ ] **Step 2: Record the baseline test count**

```bash
cargo test -p logisticos-field-ops -p logisticos-omnideliv 2>&1 | grep "test result:"
```

Write the numbers down. Every task below adds tests; none may remove any.

---

## Task 1: Refuse `collected` and `delivered` from a courier who does not hold the assignment

> **Amended after code review.** The helper below was written as
> `held_assignment` and checked `courier_id` only. Review found that ownership
> alone does not gate a milestone: `offer_to_nearest` fans out to five couriers,
> only the winner's row is claimed, and every loser keeps a readable assignment
> id — so a loser could `POST /delivered` on their own `Offered` row, be
> credited, and advance the customer's order.
>
> As landed, the helper is named **`assignment_for_courier`**, documents itself
> as *addressed to* rather than *held by*, and each caller states its own status
> requirement: `Claimed` for `collected` and `arrived`; for `delivered`,
> `Claimed` does the work and `Completed` returns success without re-crediting,
> so an offline queue's retry is absorbed here rather than 404'd. See the spec's
> H2 section. Later tasks call `assignment_for_courier`, not `held_assignment`.

`claim` already does this. These two never got it, and `mark_delivered` completes the assignment, credits the courier ledger and debits COD — so today any authenticated user in the tenant can complete another courier's job by naming its id.

**Files:**
- Modify: `services/field-ops/src/application/services/dispatch_service.rs:387-408` (`mark_collected`), `:412-446` (`mark_delivered`)
- Modify: `services/field-ops/src/api/http/couriers.rs:533-556` (`collected`), `:557-579` (`delivered`)
- Test: new module `milestone_authorization` at the end of `dispatch_service.rs`

- [ ] **Step 1: Write the failing test**

Append to the end of `services/field-ops/src/application/services/dispatch_service.rs`:

```rust
// ─────────────────────────────────────────────────────────────────────────────
// Milestone authorization
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod milestone_authorization {
    use super::*;
    use crate::domain::entities::{AssignmentStatus, Courier, CourierLocation};
    use crate::domain::repositories::CourierRepository;
    use crate::infrastructure::db::{ClaimOutcome, LocationRepository};
    use std::sync::Mutex;

    const TENANT: Uuid = Uuid::from_u128(1);

    struct Couriers { by_user: Vec<(Uuid, Courier)> }

    #[async_trait::async_trait]
    impl CourierRepository for Couriers {
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.by_user.iter().find(|(_, c)| c.id == id).map(|(_, c)| c.clone()))
        }
        async fn find_by_user(&self, _: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.by_user.iter().find(|(u, _)| *u == user_id).map(|(_, c)| c.clone()))
        }
        async fn save(&self, _: &Courier) -> anyhow::Result<()> { Ok(()) }
        async fn find_available_near(&self, _: Uuid, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
    }

    #[derive(Default)]
    struct Assignments { rows: Mutex<Vec<CourierAssignment>> }

    #[async_trait::async_trait]
    impl AssignmentRepository for Assignments {
        async fn save(&self, a: &CourierAssignment) -> anyhow::Result<()> {
            let mut rows = self.rows.lock().unwrap();
            if let Some(row) = rows.iter_mut().find(|r| r.id == a.id) { *row = a.clone(); }
            Ok(())
        }
        async fn try_claim(&self, _: Uuid, _: Uuid) -> anyhow::Result<ClaimOutcome> {
            Ok(ClaimOutcome::Lost)
        }
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<CourierAssignment>> {
            Ok(self.rows.lock().unwrap().iter().find(|a| a.id == id).cloned())
        }
        async fn find_offered_for_courier(&self, _: Uuid, _: Uuid)
            -> anyhow::Result<Vec<CourierAssignment>> { Ok(vec![]) }
    }

    struct NoLocations;
    #[async_trait::async_trait]
    impl LocationRepository for NoLocations {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> { Ok(None) }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64) -> anyhow::Result<Vec<CourierLocation>> { Ok(vec![]) }
    }

    /// Records every entry written, so a test can assert nobody was paid.
    #[derive(Default)]
    struct RecordingLedgers { saved: Mutex<Vec<CourierLedger>> }

    #[async_trait::async_trait]
    impl crate::infrastructure::db::CourierLedgerRepository for RecordingLedgers {
        async fn find_open(&self, _: Uuid, _: Uuid, _: &str)
            -> anyhow::Result<Option<CourierLedger>> { Ok(None) }
        async fn save(&self, ledger: &CourierLedger) -> anyhow::Result<()> {
            self.saved.lock().unwrap().push(ledger.clone());
            Ok(())
        }
        async fn find_all_open(&self, _: &str) -> anyhow::Result<Vec<CourierLedger>> { Ok(vec![]) }
        async fn entry_exists_for_job(&self, _: Uuid, _: Uuid, _: Uuid) -> anyhow::Result<bool> {
            Ok(false)
        }
    }

    /// Records which milestones reached the broker.
    #[derive(Default)]
    struct RecordingEvents { emitted: Mutex<Vec<&'static str>> }

    #[async_trait::async_trait]
    impl crate::infrastructure::messaging::CourierEvents for RecordingEvents {
        async fn publish(&self, e: &CourierEvent) -> anyhow::Result<()> {
            self.emitted.lock().unwrap().push(match e {
                CourierEvent::Assigned  { .. } => "assigned",
                CourierEvent::Collected { .. } => "collected",
                CourierEvent::Delivered { .. } => "delivered",
            });
            Ok(())
        }
    }

    fn courier() -> Courier {
        Courier::new(TENANT, Uuid::new_v4(), "A".into(), "B".into(), "+63".into())
    }

    /// (service, assignment_id, holder_user, other_user, assignments, ledgers, events)
    #[allow(clippy::type_complexity)]
    fn fixture() -> (
        DispatchService, Uuid, Uuid, Uuid,
        Arc<Assignments>, Arc<RecordingLedgers>, Arc<RecordingEvents>,
    ) {
        let holder_user = Uuid::new_v4();
        let other_user  = Uuid::new_v4();
        let holder = courier();
        let other  = courier();

        let mut a = CourierAssignment::offer_with_earnings(
            TENANT, holder.id, ProductKey::new("omnideliv".to_string()),
            Uuid::new_v4(), 3_500, 0, 38_900,
        );
        a.status = AssignmentStatus::Claimed;
        let id = a.id;

        let assignments = Arc::new(Assignments::default());
        assignments.rows.lock().unwrap().push(a);

        let ledgers = Arc::new(RecordingLedgers::default());
        let events  = Arc::new(RecordingEvents::default());

        let svc = DispatchService::new(
            Arc::new(Couriers { by_user: vec![(holder_user, holder), (other_user, other)] }),
            assignments.clone(),
            Arc::new(NoLocations),
            ledgers.clone(),
            events.clone(),
            PayBounds::default(),
        );
        (svc, id, holder_user, other_user, assignments, ledgers, events)
    }

    #[tokio::test]
    async fn the_holder_can_mark_collected() {
        let (svc, id, holder, _, _, _, events) = fixture();
        assert!(svc.mark_collected(TENANT, holder, id, Uuid::new_v4(), None).await.unwrap());
        assert_eq!(*events.emitted.lock().unwrap(), vec!["collected"]);
    }

    #[tokio::test]
    async fn the_holder_can_mark_delivered() {
        let (svc, id, holder, _, _, ledgers, events) = fixture();
        assert!(svc.mark_delivered(TENANT, holder, id, None).await.unwrap());
        assert_eq!(*events.emitted.lock().unwrap(), vec!["delivered"]);
        assert_eq!(ledgers.saved.lock().unwrap().len(), 1, "the holder is paid");
    }

    /// The assignment ids are handed to the dispatching product, so they are
    /// not secret. Another courier naming one must not be able to collect
    /// against it.
    #[tokio::test]
    async fn another_courier_cannot_mark_collected() {
        let (svc, id, _, other, _, _, events) = fixture();
        assert!(!svc.mark_collected(TENANT, other, id, Uuid::new_v4(), None).await.unwrap());
        assert!(events.emitted.lock().unwrap().is_empty(),
                "no milestone may reach the broker for a job the caller does not hold");
    }

    /// The one that moves money: `mark_delivered` completes the assignment,
    /// credits the courier ledger and debits COD.
    #[tokio::test]
    async fn another_courier_cannot_mark_delivered_or_trigger_a_credit() {
        let (svc, id, _, other, assignments, ledgers, events) = fixture();

        assert!(!svc.mark_delivered(TENANT, other, id, None).await.unwrap());
        assert!(events.emitted.lock().unwrap().is_empty());
        assert!(ledgers.saved.lock().unwrap().is_empty(),
                "an unauthorized delivery must not credit anyone");

        let rows = assignments.rows.lock().unwrap();
        assert_eq!(rows[0].status, AssignmentStatus::Claimed,
                   "the assignment must not be completed by a caller who does not hold it");
    }

    #[tokio::test]
    async fn a_user_who_is_not_a_courier_is_refused_both_milestones() {
        let (svc, id, _, _, _, _, _) = fixture();
        let stranger = Uuid::new_v4();
        assert!(!svc.mark_collected(TENANT, stranger, id, Uuid::new_v4(), None).await.unwrap());
        assert!(!svc.mark_delivered(TENANT, stranger, id, None).await.unwrap());
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p logisticos-field-ops milestone_authorization
```

Expected: **compile failure** — `mark_collected` and `mark_delivered` take 4 and 3 arguments, and `entry_exists_for_job` is not a member of `CourierLedgerRepository`. That is the correct first failure; Task 2 adds the trait method, so for now add it to the trait as a stub.

- [ ] **Step 3: Add the trait method stub so the test module compiles**

In `services/field-ops/src/infrastructure/db/ledger_repo.rs`, add to the `CourierLedgerRepository` trait:

```rust
    /// Has this courier already been credited for this job, in **any** period?
    ///
    /// Scanning one period's entries is not enough: `current_period()` is the
    /// ISO week, so a retry that crosses the Sunday→Monday boundary opens a
    /// fresh ledger, finds nothing, and pays twice.
    async fn entry_exists_for_job(
        &self,
        tenant_id: Uuid,
        courier_id: Uuid,
        external_ref: Uuid,
    ) -> anyhow::Result<bool>;
```

And on `impl CourierLedgerRepository for PgCourierLedgerRepository`, a temporary body Task 2 replaces:

```rust
    async fn entry_exists_for_job(
        &self,
        _tenant_id: Uuid,
        _courier_id: Uuid,
        _external_ref: Uuid,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }
```

- [ ] **Step 4: Add the ownership checks**

In `services/field-ops/src/application/services/dispatch_service.rs`, replace `mark_collected` and `mark_delivered` with:

```rust
    /// A vendor's goods are in the bag.
    ///
    /// `user_id` is the authenticated caller and the assignment must be
    /// **theirs**. Assignment ids are handed to the dispatching product, so
    /// they are not secret; without this check any authenticated user in the
    /// tenant could report milestones against another courier's job. `claim`
    /// has had this since it was hardened — these two had not.
    pub async fn mark_collected(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        assignment_id: Uuid,
        vendor_id: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<bool> {
        let Some(a) = self.held_assignment(tenant_id, user_id, assignment_id).await? else {
            return Ok(false);
        };

        self.emit(CourierEvent::Collected {
            tenant_id,
            product: a.product.as_str().to_string(),
            external_ref: a.external_ref,
            courier_id: a.courier_id,
            vendor_id,
            device_timestamp,
        })
        .await;
        Ok(true)
    }

    /// The job is done. Completing the assignment frees the courier for the
    /// next one, which is why it is persisted rather than only published.
    pub async fn mark_delivered(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        assignment_id: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<bool> {
        let Some(mut a) = self.held_assignment(tenant_id, user_id, assignment_id).await? else {
            return Ok(false);
        };

        a.complete();
        self.assignments.save(&a).await?;

        // Credit before publishing. A failed credit surfaces as an error the
        // caller retries; publishing first would tell OmniDeliv the job is done
        // while the courier is unpaid, and nothing downstream would notice.
        //
        // The COD debit rides in the same call for the same reason: the cash is
        // in the courier's hand the moment the door closes, and a delivery
        // recorded without it would show them in credit for money they are
        // holding — which is what a payout run would then hand them again.
        if a.trip_cents > 0 || a.tip_cents > 0 || a.cod_amount_cents > 0 {
            self.credit_courier(&a).await?;
        }

        self.emit(CourierEvent::Delivered {
            tenant_id,
            product: a.product.as_str().to_string(),
            external_ref: a.external_ref,
            courier_id: a.courier_id,
            device_timestamp,
        })
        .await;
        Ok(true)
    }

    /// The assignment, if and only if this user is the courier holding it.
    ///
    /// `None` covers all three refusals — not a courier, no such assignment,
    /// someone else's assignment — because the handler turns every one of them
    /// into the same 404. Distinguishing them would let a caller probe which
    /// assignment ids exist.
    async fn held_assignment(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        assignment_id: Uuid,
    ) -> anyhow::Result<Option<CourierAssignment>> {
        let Some(courier) = self.couriers.find_by_user(tenant_id, user_id).await? else {
            return Ok(None);
        };
        let Some(a) = self.assignments.find_by_id(tenant_id, assignment_id).await? else {
            return Ok(None);
        };
        if a.courier_id != courier.id {
            return Ok(None);
        }
        Ok(Some(a))
    }
```

- [ ] **Step 5: Pass the caller through from the handlers**

In `services/field-ops/src/api/http/couriers.rs`, in `collected` change the `mark_collected` call to:

```rust
        .mark_collected(claims.tenant_id, claims.user_id, id, req.vendor_id, req.device_timestamp)
```

and in `delivered` change the `mark_delivered` call to:

```rust
        .mark_delivered(claims.tenant_id, claims.user_id, id, req.device_timestamp)
```

The existing `if !found { return Err(StatusCode::NOT_FOUND); }` in both handlers already produces the right status — a refused caller now takes the same path as a stale id, which is the intended indistinguishability.

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cargo test -p logisticos-field-ops milestone_authorization
```

Expected: 5 passed.

```bash
cargo test -p logisticos-field-ops
```

Expected: all pass, count ≥ baseline + 5.

- [ ] **Step 7: Mutation-verify the guard**

Temporarily change `held_assignment` to skip the ownership comparison:

```rust
        if false && a.courier_id != courier.id {
```

```bash
cargo test -p logisticos-field-ops milestone_authorization
```

Expected: **`another_courier_cannot_mark_delivered_or_trigger_a_credit` and `another_courier_cannot_mark_collected` FAIL.** If they pass, the tests are not testing the guard — fix them before reverting.

Revert the `false &&`.

- [ ] **Step 8: Commit**

```bash
git add services/field-ops/src/application/services/dispatch_service.rs services/field-ops/src/api/http/couriers.rs services/field-ops/src/infrastructure/db/ledger_repo.rs
git commit -m "fix(field-ops): refuse milestones from a courier who does not hold the job"
```

---

## Task 2: Make a retried delivery idempotent across a period boundary

`credit_courier` skips a job it has already credited — but it only scans the ledger returned by `find_open(tenant, courier, current_period())`, and `current_period()` is the ISO week. A delivery whose POST succeeded but whose response was lost (the normal case for an offline queue) retries later; if the retry lands after the Sunday→Monday UTC boundary, a *fresh* ledger is opened, the guard finds nothing, and the courier is credited twice and the COD debited twice.

Fixed in two places: the application queries across periods, and a unique index makes it impossible for any future crediting path to get it wrong.

**Files:**
- Create: `services/field-ops/migrations/0007_ledger_entry_job_idempotency.sql`
- Modify: `services/field-ops/src/infrastructure/db/ledger_repo.rs` (real `entry_exists_for_job`; write the two new columns)
- Modify: `services/field-ops/src/application/services/dispatch_service.rs` (`credit_courier`)
- Test: new module `credit_idempotency` at the end of `dispatch_service.rs`

- [ ] **Step 1: Write the failing test**

Append to `services/field-ops/src/application/services/dispatch_service.rs`:

```rust
// ─────────────────────────────────────────────────────────────────────────────
// Credit idempotency across a period boundary
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod credit_idempotency {
    use super::*;
    use crate::domain::entities::{AssignmentStatus, Courier, CourierLocation};
    use crate::domain::repositories::CourierRepository;
    use crate::infrastructure::db::{ClaimOutcome, LocationRepository};
    use std::sync::Mutex;

    const TENANT: Uuid = Uuid::from_u128(1);

    struct Couriers { by_user: Vec<(Uuid, Courier)> }

    #[async_trait::async_trait]
    impl CourierRepository for Couriers {
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.by_user.iter().find(|(_, c)| c.id == id).map(|(_, c)| c.clone()))
        }
        async fn find_by_user(&self, _: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.by_user.iter().find(|(u, _)| *u == user_id).map(|(_, c)| c.clone()))
        }
        async fn save(&self, _: &Courier) -> anyhow::Result<()> { Ok(()) }
        async fn find_available_near(&self, _: Uuid, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
    }

    #[derive(Default)]
    struct Assignments { rows: Mutex<Vec<CourierAssignment>> }

    #[async_trait::async_trait]
    impl AssignmentRepository for Assignments {
        async fn save(&self, a: &CourierAssignment) -> anyhow::Result<()> {
            let mut rows = self.rows.lock().unwrap();
            if let Some(row) = rows.iter_mut().find(|r| r.id == a.id) { *row = a.clone(); }
            Ok(())
        }
        async fn try_claim(&self, _: Uuid, _: Uuid) -> anyhow::Result<ClaimOutcome> {
            Ok(ClaimOutcome::Lost)
        }
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<CourierAssignment>> {
            Ok(self.rows.lock().unwrap().iter().find(|a| a.id == id).cloned())
        }
        async fn find_offered_for_courier(&self, _: Uuid, _: Uuid)
            -> anyhow::Result<Vec<CourierAssignment>> { Ok(vec![]) }
    }

    struct NoLocations;
    #[async_trait::async_trait]
    impl LocationRepository for NoLocations {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> { Ok(None) }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64) -> anyhow::Result<Vec<CourierLocation>> { Ok(vec![]) }
    }

    /// A ledger store that behaves like the real one across a period rollover:
    /// entries persist forever, but `find_open` only ever returns the ledger
    /// for the period it is asked about. That asymmetry is the bug.
    #[derive(Default)]
    struct PeriodAwareLedgers {
        /// (tenant, courier, kind, external_ref) — every entry ever written.
        all_entries: Mutex<Vec<(Uuid, Uuid, &'static str, Uuid)>>,
        ledgers:     Mutex<Vec<CourierLedger>>,
    }

    #[async_trait::async_trait]
    impl crate::infrastructure::db::CourierLedgerRepository for PeriodAwareLedgers {
        async fn find_open(&self, tenant_id: Uuid, courier_id: Uuid, period: &str)
            -> anyhow::Result<Option<CourierLedger>> {
            Ok(self.ledgers.lock().unwrap().iter()
                .find(|l| l.tenant_id == tenant_id && l.courier_id == courier_id && l.period == period)
                .cloned())
        }
        async fn save(&self, ledger: &CourierLedger) -> anyhow::Result<()> {
            {
                let mut all = self.all_entries.lock().unwrap();
                for e in &ledger.entries {
                    if let Some(r) = e.external_ref {
                        let row = (ledger.tenant_id, ledger.courier_id, e.kind.as_str(), r);
                        if !all.contains(&row) { all.push(row); }
                    }
                }
            }
            let mut ls = self.ledgers.lock().unwrap();
            match ls.iter_mut().find(|l| l.id == ledger.id) {
                Some(existing) => *existing = ledger.clone(),
                None => ls.push(ledger.clone()),
            }
            Ok(())
        }
        async fn find_all_open(&self, _: &str) -> anyhow::Result<Vec<CourierLedger>> { Ok(vec![]) }
        async fn entry_exists_for_job(&self, tenant_id: Uuid, courier_id: Uuid, external_ref: Uuid)
            -> anyhow::Result<bool> {
            Ok(self.all_entries.lock().unwrap().iter()
                .any(|(t, c, _, r)| *t == tenant_id && *c == courier_id && *r == external_ref))
        }
    }

    /// The delivery is credited once. Then the ledger for the *next* period is
    /// opened — exactly what `current_period()` returns after the Sunday→Monday
    /// UTC boundary — and the same job is credited again, which is what an
    /// offline queue retrying a lost response does.
    #[tokio::test]
    async fn a_retry_across_a_period_boundary_does_not_pay_twice() {
        let user = Uuid::new_v4();
        let courier = Courier::new(TENANT, user, "A".into(), "B".into(), "+63".into());
        let job = Uuid::new_v4();

        let mut a = CourierAssignment::offer_with_earnings(
            TENANT, courier.id, ProductKey::new("omnideliv".to_string()),
            job, 3_500, 0, 38_900,
        );
        a.status = AssignmentStatus::Claimed;

        let assignments = Arc::new(Assignments::default());
        assignments.rows.lock().unwrap().push(a.clone());

        let ledgers = Arc::new(PeriodAwareLedgers::default());
        let svc = DispatchService::new(
            Arc::new(Couriers { by_user: vec![(user, courier.clone())] }),
            assignments.clone(),
            Arc::new(NoLocations),
            ledgers.clone(),
            Arc::new(crate::infrastructure::messaging::NoopCourierEvents),
            PayBounds::default(),
        );

        svc.credit_courier(&a).await.unwrap();

        let after_first: i64 = ledgers.ledgers.lock().unwrap()
            .iter().map(|l| l.balance_cents).sum();
        assert_eq!(after_first, 3_500 - 38_900, "earned 3500, holding 38900 of our cash");

        // The week rolls over: nothing is open for the new period.
        ledgers.ledgers.lock().unwrap()
            .iter_mut().for_each(|l| l.period = "2026-W33".to_string());

        svc.credit_courier(&a).await.unwrap();

        let after_retry: i64 = ledgers.ledgers.lock().unwrap()
            .iter().map(|l| l.balance_cents).sum();
        assert_eq!(after_retry, after_first,
                   "a retried delivery must not credit the trip or re-debit the COD");

        let trips = ledgers.all_entries.lock().unwrap().iter()
            .filter(|(_, _, kind, r)| *kind == "trip_earning" && *r == job).count();
        assert_eq!(trips, 1, "exactly one trip earning for one job, ever");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p logisticos-field-ops credit_idempotency
```

Expected: **FAIL** — `after_retry` is `2 * (3_500 - 38_900)`, and `trips` is 2. This is the live defect reproduced.

- [ ] **Step 3: Query across periods in `credit_courier`**

In `services/field-ops/src/application/services/dispatch_service.rs`, replace the in-memory guard:

```rust
        // Already credited — a retried delivery must not pay twice. Keyed on
        // the job rather than the assignment so a re-offer of the same job
        // cannot double-pay either.
        if ledger.entries.iter().any(|e| e.external_ref == Some(a.external_ref)) {
            return Ok(());
        }
```

with a cross-period query:

```rust
        // Already credited — a retried delivery must not pay twice. Keyed on
        // the job rather than the assignment so a re-offer of the same job
        // cannot double-pay either.
        //
        // Asked of the *store*, not of `ledger.entries`: the ledger in hand is
        // only the current period's, and `current_period()` is the ISO week. An
        // offline queue retrying a lost response across the Sunday→Monday
        // boundary gets a fresh ledger, and a guard that scanned only it would
        // find nothing and pay a second time.
        if self
            .ledgers
            .entry_exists_for_job(a.tenant_id, a.courier_id, a.external_ref)
            .await?
        {
            return Ok(());
        }
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cargo test -p logisticos-field-ops credit_idempotency
```

Expected: PASS.

- [ ] **Step 5: Write the migration**

Create `services/field-ops/migrations/0007_ledger_entry_job_idempotency.sql`:

```sql
-- Idempotency that survives a period boundary.
--
-- The application guard now queries across periods, but it is one code path.
-- This index is the backstop: any future path that credits a courier is covered
-- whether or not its author knew about the rule.
--
-- The entries table keys on `ledger_id`, and a ledger is per (tenant, courier,
-- period) — so an index on `ledger_id` would not span the boundary the bug lives
-- on. The two columns are denormalised from the owning ledger for exactly that
-- reason.
ALTER TABLE field_ops.courier_ledger_entries
    ADD COLUMN IF NOT EXISTS tenant_id  UUID,
    ADD COLUMN IF NOT EXISTS courier_id UUID;

UPDATE field_ops.courier_ledger_entries e
   SET tenant_id  = l.tenant_id,
       courier_id = l.courier_id
  FROM field_ops.courier_ledgers l
 WHERE e.ledger_id = l.id
   AND (e.tenant_id IS NULL OR e.courier_id IS NULL);

ALTER TABLE field_ops.courier_ledger_entries
    ALTER COLUMN tenant_id  SET NOT NULL,
    ALTER COLUMN courier_id SET NOT NULL;

-- Partial: payouts, remittances and adjustments carry no job reference, and
-- there is no reason a courier cannot have several of those.
--
-- This CREATE **fails** if any courier has already been double-credited. That
-- is the correct outcome: the duplicates are real money and a human has to
-- decide which entry survives. Silently de-duplicating would erase the evidence
-- that it happened. Find them with:
--
--   SELECT tenant_id, courier_id, kind, external_ref, COUNT(*)
--     FROM field_ops.courier_ledger_entries
--    WHERE external_ref IS NOT NULL
--    GROUP BY 1,2,3,4 HAVING COUNT(*) > 1;
CREATE UNIQUE INDEX IF NOT EXISTS uq_courier_ledger_entry_job
    ON field_ops.courier_ledger_entries (tenant_id, courier_id, kind, external_ref)
    WHERE external_ref IS NOT NULL;

COMMENT ON INDEX field_ops.uq_courier_ledger_entry_job IS
  'One entry of each kind per courier per job, across every period. Backstops '
  'DispatchService::credit_courier for retried deliveries.';
```

- [ ] **Step 6: Implement the real query and write the new columns**

In `services/field-ops/src/infrastructure/db/ledger_repo.rs`, replace the stub `entry_exists_for_job` body from Task 1 Step 3 with:

```rust
    async fn entry_exists_for_job(
        &self,
        tenant_id: Uuid,
        courier_id: Uuid,
        external_ref: Uuid,
    ) -> anyhow::Result<bool> {
        let found: Option<(i32,)> = sqlx::query_as(
            "SELECT 1 FROM field_ops.courier_ledger_entries
              WHERE tenant_id = $1 AND courier_id = $2 AND external_ref = $3
              LIMIT 1",
        )
        .bind(tenant_id)
        .bind(courier_id)
        .bind(external_ref)
        .fetch_optional(&self.pool)
        .await?;
        Ok(found.is_some())
    }
```

In the same file, extend the entry INSERT (currently at line ~162) to populate the denormalised columns:

```rust
                INSERT INTO field_ops.courier_ledger_entries (
                    id, ledger_id, tenant_id, courier_id,
                    kind, amount_cents, external_ref, reference, created_at
                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                ON CONFLICT (id) DO NOTHING
                "#,
            )
            .bind(e.id).bind(e.ledger_id).bind(ledger.tenant_id).bind(ledger.courier_id)
            .bind(e.kind.as_str()).bind(e.amount_cents)
            .bind(e.external_ref).bind(&e.reference).bind(e.created_at)
```

Note the bind order shifted — `kind` is now `$5`, not `$3`. sqlx binds positionally and will not warn you; a mis-ordered bind here writes a UUID into `kind` and fails at the CHECK constraint, or worse, silently transposes two UUID columns.

- [ ] **Step 7: Verify it compiles and the suite passes**

```bash
cargo check -p logisticos-field-ops && cargo test -p logisticos-field-ops
```

Expected: `Finished`, all tests pass.

- [ ] **Step 8: Mutation-verify the guard**

Change the guard in `credit_courier` to `if false && self.ledgers...` (keeping the `.await?` valid by short-circuiting on the literal). Run:

```bash
cargo test -p logisticos-field-ops credit_idempotency
```

Expected: **FAIL.** Revert.

- [ ] **Step 9: Commit**

```bash
git add services/field-ops/migrations/0007_ledger_entry_job_idempotency.sql services/field-ops/src/infrastructure/db/ledger_repo.rs services/field-ops/src/application/services/dispatch_service.rs
git commit -m "fix(field-ops): a delivery retried across a week boundary paid twice"
```

---

## Task 3: Make omnideliv's `Delivered` consumer branch idempotent

`Collected` early-returns when the leg is already `PickedUp`, with a comment saying that is the idempotence that matters. `Delivered` calls `order.delivered()?` and propagates a `TransitionError` on a duplicate. A retried delivery — Task 2's scenario — republishes the event, and the consumer errors on a message it should ignore.

**Files:**
- Modify: `services/omnideliv/src/infrastructure/messaging/courier_consumer.rs:135-148`
- Test: existing test module in the same file

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` module in `services/omnideliv/src/infrastructure/messaging/courier_consumer.rs`:

```rust
    /// The sibling branch, `Collected`, has had this since it was written. A
    /// courier's offline queue retrying a delivery whose response was lost
    /// republishes `Delivered`, and a consumer that errors on it turns a normal
    /// retry into a failed message.
    #[test]
    fn a_second_delivered_on_a_delivered_order_is_ignored_not_an_error() {
        let mut order = delivered_order();
        assert_eq!(order.status, OrderStatus::Delivered);

        let outcome = apply_delivered(&mut order);

        assert!(outcome.is_ok(), "a duplicate Delivered must not error");
        assert_eq!(order.status, OrderStatus::Delivered);
    }
```

Add these helpers to the same test module:

```rust
    use crate::domain::entities::{Order, OrderStatus, VendorLeg};

    /// An order carried to Delivered entirely by legitimate transitions, so the
    /// duplicate under test is the only irregular thing about it.
    fn delivered_order() -> Order {
        const TENANT: Uuid = Uuid::from_u128(1);

        // settle(tenant_id, vendor_id, goods_subtotal_cents, commission_bps)
        let leg = VendorLeg::settle(TENANT, Uuid::new_v4(), 10_000, 1_500);

        // place(tenant, customer, basket, plan, legs, delivery_fee, tip,
        //       courier_trip, delivery_lat, delivery_lng)
        let mut o = Order::place(
            TENANT,
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            vec![leg],
            4_900,
            0,
            3_500,
            14.5547,
            121.0244,
        );

        // Placed → Collecting (courier_claimed accepts Placed or AwaitingCourier)
        o.courier_claimed(Uuid::new_v4()).unwrap();
        o.legs[0].mark_picked_up();
        // Collecting → Delivering
        o.all_legs_collected().unwrap();
        // Delivering → Delivered
        o.delivered().unwrap();
        o
    }
```

- [ ] **Step 2: Extract the branch so it is testable**

The current branch body is inline inside `handle`. Extract just the state change into a free function above the test module:

```rust
/// The state change for a `Delivered` milestone, separated from the I/O around
/// it so the idempotence rule can be tested without a broker or a database.
///
/// Returns `Ok(false)` when the order was already delivered — a duplicate to
/// ignore, not a failure. Symmetric with the `Collected` branch, which has
/// early-returned on an already-picked-up leg since it was written.
fn apply_delivered(order: &mut Order) -> Result<bool, TransitionError> {
    if order.status == OrderStatus::Delivered {
        return Ok(false);
    }
    order.delivered()?;
    Ok(true)
}
```

Add whatever imports this needs at the top of the file (`Order`, `OrderStatus`, `TransitionError` from `crate::domain::entities`).

- [ ] **Step 3: Run the test to verify it fails**

```bash
cargo test -p logisticos-omnideliv a_second_delivered
```

Expected: **compile error** or FAIL until Step 4 wires the branch. If `apply_delivered` already compiles from Step 2, the test passes — in that case verify it is testing something by temporarily removing the `if order.status == …` early return and confirming it fails.

- [ ] **Step 4: Use it in the consumer branch**

Replace the `CourierEvent::Delivered` arm in `handle`:

```rust
            CourierEvent::Delivered { courier_id, device_timestamp, .. } => {
                // A duplicate is a retry, not a failure — see `apply_delivered`.
                // Returning early also skips the publish, so a customer cannot
                // be told twice that their order arrived.
                if !apply_delivered(&mut order)? {
                    return Ok(());
                }

                self.append(tenant_id, order_id, event_type::ORDER_DELIVERED,
                            device_timestamp, Some(courier_id), serde_json::json!({})).await;

                // After the state change, before the save below. A publish
                // failure must not stop the order being recorded as delivered:
                // the courier is already paid and the customer already has their
                // food, so losing the notification is the smaller loss.
                if let Err(e) = self.events.order_delivered(&order).await {
                    tracing::error!(err = %e, %order_id, "order.delivered publish failed");
                }
            }
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p logisticos-omnideliv
```

Expected: all pass, count ≥ baseline + 1.

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/src/infrastructure/messaging/courier_consumer.rs
git commit -m "fix(omnideliv): a duplicate Delivered is a retry, not an error"
```

---

## Task 4: Scope the assignment position route to the holder or the product

`GET /v1/field-ops/assignments/:id/position` accepts any valid tenant JWT plus the UUID. It was recorded as safe only because assignment ids never reached a client — and plan 3 puts them in every courier's phone.

Two legitimate callers: the courier holding the assignment, and omnideliv reading it for the customer's tracking screen with its minted service token.

**Files:**
- Modify: `services/field-ops/src/api/http/couriers.rs:505-531` (`assignment_position`)
- Modify: `services/field-ops/src/application/services/dispatch_service.rs:531` (`position_for_assignment`)
- Modify: `services/omnideliv/src/infrastructure/external/field_ops_dispatch.rs:63` (`mint`) and `:276` (its pinning test)
- Test: extend the existing `position_lookup` module in `dispatch_service.rs:915`

- [ ] **Step 1: Write the failing test**

Add to the `position_lookup` module in `services/field-ops/src/application/services/dispatch_service.rs`:

```rust
    /// The route is reachable with any valid tenant JWT plus the id. Once a
    /// driver app holds assignment ids, that lets one courier follow another
    /// around the city.
    #[tokio::test]
    async fn a_courier_cannot_read_another_couriers_position() {
        let (svc, id, _holder_user, other_user) = position_fixture();

        let seen = svc
            .position_for_assignment_as(TENANT, PositionReader::Courier(other_user), id)
            .await
            .unwrap();

        assert!(seen.is_none(), "a courier who does not hold the job gets nothing");
    }

    #[tokio::test]
    async fn the_holder_can_read_their_own_position() {
        let (svc, id, holder_user, _other) = position_fixture();

        let seen = svc
            .position_for_assignment_as(TENANT, PositionReader::Courier(holder_user), id)
            .await
            .unwrap();

        assert!(seen.is_some());
    }

    /// omnideliv reads this for the customer tracking screen and is not a
    /// courier. Its minted token carries the permission instead.
    #[tokio::test]
    async fn the_product_service_can_read_any_assignment_in_its_tenant() {
        let (svc, id, _holder, _other) = position_fixture();

        let seen = svc
            .position_for_assignment_as(TENANT, PositionReader::Service, id)
            .await
            .unwrap();

        assert!(seen.is_some());
    }
```

Add the fixture and its mocks to the same module. The existing `position_lookup` module has its own `NoCouriers` and `Assignments`; these are new types with distinct names so both coexist:

```rust
    struct HeldFix;
    #[async_trait::async_trait]
    impl LocationRepository for HeldFix {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, tenant_id: Uuid, courier_id: Uuid)
            -> anyhow::Result<Option<CourierLocation>> {
            Ok(Some(CourierLocation::new(tenant_id, courier_id, 14.5547, 121.0244, None)))
        }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64)
            -> anyhow::Result<Vec<CourierLocation>> { Ok(vec![]) }
    }

    struct TwoCouriers { by_user: Vec<(Uuid, Courier)> }

    #[async_trait::async_trait]
    impl CourierRepository for TwoCouriers {
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.by_user.iter().find(|(_, c)| c.id == id).map(|(_, c)| c.clone()))
        }
        async fn find_by_user(&self, _: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.by_user.iter().find(|(u, _)| *u == user_id).map(|(_, c)| c.clone()))
        }
        async fn save(&self, _: &Courier) -> anyhow::Result<()> { Ok(()) }
        async fn find_available_near(&self, _: Uuid, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
    }

    #[derive(Default)]
    struct HeldAssignments { rows: Mutex<Vec<CourierAssignment>> }

    #[async_trait::async_trait]
    impl AssignmentRepository for HeldAssignments {
        async fn save(&self, _: &CourierAssignment) -> anyhow::Result<()> { Ok(()) }
        async fn try_claim(&self, _: Uuid, _: Uuid) -> anyhow::Result<ClaimOutcome> {
            Ok(ClaimOutcome::Lost)
        }
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<CourierAssignment>> {
            Ok(self.rows.lock().unwrap().iter().find(|a| a.id == id).cloned())
        }
        async fn find_offered_for_courier(&self, _: Uuid, _: Uuid)
            -> anyhow::Result<Vec<CourierAssignment>> { Ok(vec![]) }
    }

    struct NoLedgersHere;
    #[async_trait::async_trait]
    impl crate::infrastructure::db::CourierLedgerRepository for NoLedgersHere {
        async fn find_open(&self, _: Uuid, _: Uuid, _: &str)
            -> anyhow::Result<Option<CourierLedger>> { Ok(None) }
        async fn save(&self, _: &CourierLedger) -> anyhow::Result<()> { Ok(()) }
        async fn find_all_open(&self, _: &str) -> anyhow::Result<Vec<CourierLedger>> { Ok(vec![]) }
        async fn entry_exists_for_job(&self, _: Uuid, _: Uuid, _: Uuid) -> anyhow::Result<bool> {
            Ok(false)
        }
    }

    /// (service, assignment_id, holder_user, other_user)
    fn position_fixture() -> (DispatchService, Uuid, Uuid, Uuid) {
        let holder_user = Uuid::new_v4();
        let other_user  = Uuid::new_v4();
        let holder = Courier::new(TENANT, holder_user, "A".into(), "B".into(), "+63".into());
        let other  = Courier::new(TENANT, other_user,  "C".into(), "D".into(), "+63".into());

        let mut a = CourierAssignment::offer_with_earnings(
            TENANT, holder.id, ProductKey::new("omnideliv".to_string()),
            Uuid::new_v4(), 3_500, 0, 0,
        );
        a.status = AssignmentStatus::Claimed;
        let id = a.id;

        let assignments = Arc::new(HeldAssignments::default());
        assignments.rows.lock().unwrap().push(a);

        let svc = DispatchService::new(
            Arc::new(TwoCouriers { by_user: vec![(holder_user, holder), (other_user, other)] }),
            assignments,
            Arc::new(HeldFix),
            Arc::new(NoLedgersHere),
            Arc::new(crate::infrastructure::messaging::NoopCourierEvents),
            PayBounds::default(),
        );
        (svc, id, holder_user, other_user)
    }
```

The module needs `use std::sync::Mutex;` and `use crate::domain::entities::AssignmentStatus;` if it does not already import them, plus `PositionReader` from `super::*`.

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p logisticos-field-ops position_lookup
```

Expected: **compile failure** — `PositionReader` and `position_for_assignment_as` do not exist.

- [ ] **Step 3: Add the reader distinction**

In `services/field-ops/src/application/services/dispatch_service.rs`, add above `impl DispatchService`:

```rust
/// Who is asking where a courier is.
///
/// A caller identity, not a permission check — the handler translates the JWT
/// into one of these and this layer decides what it may see. Keeping the
/// decision here rather than in the handler means it is unit-testable without
/// minting tokens.
#[derive(Debug, Clone, Copy)]
pub enum PositionReader {
    /// A courier, by `user_id`. May only read the assignment they hold.
    Courier(Uuid),
    /// A product service holding `field-ops:read-position`. May read any
    /// assignment in its own tenant — it needs this for customer tracking and
    /// has no courier identity of its own.
    Service,
}
```

Then add the authorizing wrapper next to `position_for_assignment`:

```rust
    /// `position_for_assignment`, gated on who is asking.
    ///
    /// `None` for an unauthorized reader, identical to an unknown assignment
    /// and to a courier with no fix yet — all three are a 404, so a caller
    /// cannot use the response to learn which assignment ids exist.
    pub async fn position_for_assignment_as(
        &self,
        tenant_id: Uuid,
        reader: PositionReader,
        assignment_id: Uuid,
    ) -> anyhow::Result<Option<(Uuid, CourierLocation, Option<f64>)>> {
        if let PositionReader::Courier(user_id) = reader {
            // Addressed-to, not status-gated — unlike the milestone calls. The
            // position this returns is the assignment's own courier's, so a
            // courier reading a stale offer of theirs learns only where they
            // already are. Requiring `Claimed` here would buy nothing and would
            // break a legitimate read between claim and first fix.
            if self
                .assignment_for_courier(tenant_id, user_id, assignment_id)
                .await?
                .is_none()
            {
                return Ok(None);
            }
        }
        self.position_for_assignment(tenant_id, assignment_id).await
    }
```

- [ ] **Step 4: Grant omnideliv the permission and update its pinning test**

In `services/omnideliv/src/infrastructure/external/field_ops_dispatch.rs:63`, `mint` passes two empty vecs to `Claims::new` — roles then permissions. Replace the permissions vec and rewrite the doc comment, which currently states the opposite of what the code will do:

```rust
    /// A single-purpose token scoped to one tenant.
    ///
    /// No roles, and exactly one permission. field-ops' offer route reads
    /// `tenant_id` and nothing else, but its assignment-position route is no
    /// longer open to any tenant token — it now admits either the courier
    /// holding the assignment or a service holding this permission, and
    /// omnideliv is the latter: it reads positions for the customer tracking
    /// screen and has no courier identity of its own.
    fn mint(&self, tenant_id: Uuid) -> anyhow::Result<String> {
        let claims = Claims::new(
            OMNIDELIV_SERVICE_USER,
            tenant_id,
            "service".to_string(),
            "internal".to_string(),
            "omnideliv@service.internal".to_string(),
            Vec::new(),
            vec!["field-ops:read-position".to_string()],
            SERVICE_TOKEN_TTL_SECS,
        );
        self.jwt
            .issue_access_token(claims)
            .map_err(|e| anyhow::anyhow!("could not mint a field-ops service token: {e}"))
    }
```

The test at `field_ops_dispatch.rs:276` pins the old behaviour and will now fail. Replace it — asserting the **exact** set, never merely that it is non-empty, so the next widening also has to change a test:

```rust
    /// The token's authority is exactly what the callees read: `tenant_id` for
    /// the offer route, plus one permission for the position route. Asserted as
    /// an exact set — "not empty" would stop guarding anything the moment a
    /// second permission is added for an unrelated reason.
    #[test]
    fn the_service_token_grants_no_roles_and_exactly_one_permission() {
        let d = dispatch();
        let c = d.jwt.validate_access_token(&d.mint(Uuid::new_v4()).unwrap()).unwrap().claims;

        assert!(c.roles.is_empty());
        assert_eq!(c.permissions, vec!["field-ops:read-position".to_string()]);
        assert_eq!(c.user_id, OMNIDELIV_SERVICE_USER);
    }
```

- [ ] **Step 5: Use the wrapper in the handler**

In `services/field-ops/src/api/http/couriers.rs`, replace the body of `assignment_position`:

```rust
async fn assignment_position(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(assignment_id): Path<Uuid>,
) -> Result<Json<PositionResponse>, StatusCode> {
    // A service token carries the permission and has no courier identity; a
    // courier carries no permission and is checked against the assignment.
    let reader = if claims.has_permission("field-ops:read-position") {
        crate::application::services::dispatch_service::PositionReader::Service
    } else {
        crate::application::services::dispatch_service::PositionReader::Courier(claims.user_id)
    };

    let (courier_id, fix, smoothed) = st
        .dispatch
        .position_for_assignment_as(claims.tenant_id, reader, assignment_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, %assignment_id, "position lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(PositionResponse {
        courier_id,
        lat:                fix.lat,
        lng:                fix.lng,
        speed_kph:          fix.speed_kph,
        smoothed_speed_kph: smoothed,
        heading_deg:        fix.heading_deg,
        device_timestamp:   fix.device_timestamp,
        recorded_at:        fix.recorded_at,
        age_seconds:        fix.age_seconds(chrono::Utc::now()),
    }))
}
```

> `AuthClaims` must expose `has_permission`. If it is a newtype over `Claims`,
> call through to the inner value. `Claims::has_permission` also returns true
> for the `"*"` superadmin wildcard — that is existing platform behaviour and
> is intentional here.

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cargo test -p logisticos-field-ops position_lookup && cargo test -p logisticos-omnideliv
```

Expected: all pass.

- [ ] **Step 7: Mutation-verify**

Change the wrapper's guard to `if let PositionReader::Courier(_user_id) = reader { }` (an empty body). Run:

```bash
cargo test -p logisticos-field-ops position_lookup
```

Expected: **`a_courier_cannot_read_another_couriers_position` FAILS.** Revert.

- [ ] **Step 8: Commit**

```bash
git add services/field-ops/src/application/services/dispatch_service.rs services/field-ops/src/api/http/couriers.rs services/omnideliv/src
git commit -m "fix(field-ops): scope assignment position to the holder or the product"
```

---

## Task 5: Detect WebP correctly

`("image/webp", b"RIFF")` matches WAV and AVI too — both are RIFF containers. Proof photos are about to route through this sniffer, and it decides the `Content-Type` the file is later served back with.

**Files:**
- Modify: `services/omnideliv/src/infrastructure/storage.rs:23-36`
- Test: new `#[cfg(test)] mod sniffing` in the same file

- [ ] **Step 1: Write the failing test**

Append to `services/omnideliv/src/infrastructure/storage.rs`:

```rust
#[cfg(test)]
mod sniffing {
    use super::*;

    fn riff(form_type: &[u8; 4], payload_len: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&payload_len.to_le_bytes());
        v.extend_from_slice(form_type);
        v.extend_from_slice(&[0u8; 8]);
        v
    }

    #[test]
    fn a_real_webp_is_accepted() {
        assert_eq!(sniff_image(&riff(b"WEBP", 32)), Some("image/webp"));
    }

    /// RIFF is a container, not a format. A WAV stored as `image/webp` is
    /// served back with that Content-Type — a content-type confusion the
    /// sniffer exists to prevent.
    #[test]
    fn a_wav_is_not_a_webp() {
        assert_eq!(sniff_image(&riff(b"WAVE", 32)), None);
    }

    #[test]
    fn an_avi_is_not_a_webp() {
        assert_eq!(sniff_image(&riff(b"AVI ", 32)), None);
    }

    /// Four bytes of "RIFF" and nothing else must not index past the end.
    #[test]
    fn a_truncated_riff_header_does_not_panic() {
        assert_eq!(sniff_image(b"RIFF"), None);
        assert_eq!(sniff_image(b"RIFF\x20\x00\x00\x00WEB"), None);
    }

    #[test]
    fn jpeg_and_png_still_work() {
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image(&[0x89, b'P', b'N', b'G', 0x0D]), Some("image/png"));
    }

    #[test]
    fn empty_input_is_not_an_image() {
        assert_eq!(sniff_image(&[]), None);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p logisticos-omnideliv sniffing
```

Expected: `a_wav_is_not_a_webp` and `an_avi_is_not_a_webp` **FAIL** — both return `Some("image/webp")`.

- [ ] **Step 3: Fix the sniffer**

Replace the `ALLOWED` table and `sniff_image` in `services/omnideliv/src/infrastructure/storage.rs`:

```rust
/// What a client may send. Checked against the *sniffed* bytes, not the
/// caller's Content-Type, which is a claim.
const ALLOWED_PREFIX: &[(&str, &[u8])] = &[
    ("image/jpeg", &[0xFF, 0xD8, 0xFF]),
    ("image/png", &[0x89, b'P', b'N', b'G']),
];

/// Content type implied by the leading bytes, or `None` if it is not an image
/// we accept.
///
/// WebP is not a prefix match. It is a RIFF container — so are WAV and AVI —
/// and the format is named by the four bytes at offset 8, not at 0. Matching
/// `RIFF` alone would store a WAV as `image/webp` and serve it back under that
/// Content-Type.
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    ALLOWED_PREFIX
        .iter()
        .find(|(_, magic)| bytes.starts_with(magic))
        .map(|(ct, _)| *ct)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p logisticos-omnideliv sniffing
```

Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/src/infrastructure/storage.rs
git commit -m "fix(omnideliv): RIFF alone is not a WebP"
```

---

## Task 6: Full verification

- [ ] **Step 1: Clippy, both services**

```bash
cargo clippy -p logisticos-field-ops -p logisticos-omnideliv --all-targets -- -D warnings
```

Expected: no warnings. The repo denies `clippy::all`.

- [ ] **Step 2: Full test run**

```bash
cargo test -p logisticos-field-ops -p logisticos-omnideliv 2>&1 | grep "test result:"
```

Expected: all pass, totals ≥ baseline + 15.

- [ ] **Step 3: Apply the migration against a scratch database**

Do not run this against production. The `CREATE UNIQUE INDEX` in migration 0007 fails if duplicates already exist, and that failure is the point.

```bash
docker run --rm -d --name fo-scratch -e POSTGRES_PASSWORD=x -p 55432:5432 postgres:16
sleep 5
PGPASSWORD=x psql -h localhost -p 55432 -U postgres -c "CREATE DATABASE svc_field_ops_scratch;"
PGPASSWORD=x psql -h localhost -p 55432 -U postgres -d svc_field_ops_scratch -c "CREATE EXTENSION IF NOT EXISTS postgis; CREATE EXTENSION IF NOT EXISTS pgcrypto; CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"; CREATE EXTENSION IF NOT EXISTS pg_trgm;"
```

A hand-created database has no extensions; `init.sql` only runs on a fresh volume, and field-ops' first migration dies on `st_makepoint` without postgis.

Then run the service's migrations against `postgres://postgres:x@localhost:55432/svc_field_ops_scratch` and confirm 0007 applies.

- [ ] **Step 4: Check production for pre-existing duplicates before deploying**

```bash
ssh root@75.119.138.135 "docker exec logisticos-postgres psql -U logisticos -d svc_field_ops -c \"SELECT tenant_id, courier_id, kind, external_ref, COUNT(*) FROM field_ops.courier_ledger_entries WHERE external_ref IS NOT NULL GROUP BY 1,2,3,4 HAVING COUNT(*) > 1;\""
```

Expected: **0 rows.** If any come back, the double-credit has already happened in production — stop, and resolve those entries with a human decision before deploying. A service whose migration cannot apply silently pins itself to its last-good image.

- [ ] **Step 5: Tear down the scratch database**

```bash
docker rm -f fo-scratch
```

---

## Done when

- [ ] All five fixes committed, each with its own test
- [ ] Three mutation checks performed and reverted (Tasks 1, 2, 4)
- [ ] Clippy clean on both services
- [ ] Migration 0007 applies to a scratch database with extensions
- [ ] Production checked for pre-existing duplicate ledger entries
- [ ] Baseline test count increased by at least 15

Then plan 2 — the driver-facing surface — can begin.
