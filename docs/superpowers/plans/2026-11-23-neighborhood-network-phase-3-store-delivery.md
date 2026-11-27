# Neighborhood Network Phase 3: Store Delivery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a store assign received parcels to its own riders in ordered batches, deliver them with POD, and handle failed attempts, returns and cancellations.

**Architecture:** Dispatch owns partner jobs and creates one route, N stops and one assignment per batch. A physical release scan in hub-ops is the only thing that makes a store parcel `out_for_delivery`.

**Tech Stack:** Rust, Axum, SQLx, Kafka, Mapbox Matrix API, Kotlin, Next.js 14.

**Spec:** [Neighborhood Delivery Network](../specs/2026-11-23-neighborhood-delivery-network-design.md), section 7 (Dispatch, Events), 8 and 9. **Program plan:** [index](2026-11-23-neighborhood-delivery-network.md).

## Global Constraints

- **Entry condition:** the Phase 2 gate has passed.
- Next migrations: dispatch `0021`, driver-ops `0020`, order-intake `0026`, hub-ops `0024`.
- A batch has at most **24 stops**. Mapbox Matrix `driving`, 25 coordinates, 60 requests a minute, called only at batch creation or re-sequence.
- A Matrix failure orders by straight-line distance with `distance_basis = direct`.
- Task snapshots use an explicit `payout_cents = 0`. `NULL` is wrong.
- Batch assignment never adds a pickup leg and never computes `gig_rate_cents`.
- `max_attempts` defaults to 2. Non-final failure publishes `DELIVERY_ATTEMPTED`, final failure publishes `DELIVERY_FAILED`.
- Non-partner task behavior does not change.
- `piece_released` is the only event that makes a store parcel `out_for_delivery`.
- A failed attempt pays only with a reason code and GPS inside the 200 m geofence.
- A timer or cancellation never reassigns a parcel a store is holding. A recalled parcel returns to the automatic queue only after a platform party scans it.
- No client endpoint completes an assignment.

## Review Focus

1. A late `DELIVERY_ATTEMPTED` arrives after the final `DELIVERY_FAILED`. The current status arm does not deny `failed`, so it would reopen a terminal shipment (Task 3.7).
2. A customer cancels while the parcel is with a rider. The task is cancelled, custody stays with the store until scanned back, and the parcel is not re-queued (Task 3.9).
3. A batch includes a job with missing or `(0,0)` coordinates. It must be refused, not sorted to the front (Task 3.3).
4. A rider from another store, or one with an active assignment, is named in a batch (Task 3.4).
5. A closed job's recipient contact is requested after delivery. It must be gone (Task 3.10).

---

### Task 3.1: Contracts and projections

**Files:**
- Modify: `docs/api/openapi/dispatch.yaml`, `scripts/openapi-tracked-prefixes.txt`, `services/api-gateway/src/proxy/mod.rs`
- Create: `services/dispatch/migrations/0021_store_partner_refs.sql`
- Create: `services/dispatch/src/infrastructure/messaging/partner_store_consumer.rs`, `tote_event_consumer.rs`
- Modify: `libs/events/src/payloads.rs`, `services/dispatch/src/bootstrap.rs`
- Test: `services/dispatch/tests/partner_job_events_db.rs`

**Interfaces:**
- Produces: `dispatch.store_partner_refs(tenant_id, carrier_id, status, lat, lng, version)`
- Produces event `PartnerJobChanged { event_key, tenant_id, carrier_id, shipment_id, tote_id, status, attempt_no, rate_id, delivery_fee_cents, failed_attempt_fee_cents, cod_amount_cents, currency, attempt_evidence_valid, occurred_at }`, published from an outbox in the same transaction as the job change. `event_key` is `{job_id}:{status}:{attempt_no}`.

- [ ] **Step 1: Write failing tests:** `tote_received_moves_jobs_to_at_store`, `event_for_unknown_job_is_logged_and_ignored`, `replay_publishes_no_second_job_changed`, `older_store_version_is_ignored`, `suspended_store_is_projected`, gateway routes `/v1/partner-delivery` to dispatch.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-dispatch --test partner_job_events_db` → FAIL.
- [ ] **Step 3: Implement** the consumers, outbox relay and contract.
- [ ] **Step 4: Confirm they pass.** Same command and `cargo test -p logisticos-api-gateway --lib` → PASS.
- [ ] **Step 5: Commit.** `feat(dispatch): project stores and tote events onto partner jobs`

---

### Task 3.2: Store riders

**Files:**
- Modify: `services/driver-ops/src/api/http/drivers.rs`, `services/identity/src/api/http/partner_memberships.rs`
- Create: `services/driver-ops/src/infrastructure/external/identity_client.rs`
- Modify: `apps/partner-portal/src/app/(dashboard)/drivers/page.tsx`

**Interfaces:**
- `POST /v1/drivers` from a token with `STORE_RIDERS_MANAGE` forces `carrier_id = pid`, ignores any body value, and creates a membership with role `store_rider`.

- [ ] **Step 1: Write failing tests:** `store_rider_is_created_under_the_callers_store`, `body_carrier_id_is_ignored`, `rider_token_contains_pid_after_login`, `removing_a_rider_ends_the_membership`, `store_cannot_edit_another_stores_rider`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-driver-ops --test unit store_rider` → FAIL.
- [ ] **Step 3: Implement.** Key every lookup on `user_id`, per ADR-0015.
- [ ] **Step 4: Confirm they pass.** Same command and `cargo test -p logisticos-identity --test partner_membership_db` → PASS.
- [ ] **Step 5: Commit.** `feat(driver-ops): stores manage their own riders`

---

### Task 3.3: Stop sequencer

**Files:**
- Create: `services/dispatch/src/application/services/stop_sequencer.rs`
- Create: `services/dispatch/src/infrastructure/external/mapbox_matrix.rs`
- Modify: `services/dispatch/src/config.rs`

**Interfaces:**
- Produces:

```rust
pub struct SeqStop { pub job_id: Uuid, pub lat: f64, pub lng: f64, pub window_end: Option<DateTime<Utc>>, pub locked: bool }
pub enum DistanceBasis { Road, Direct }
pub struct Sequence { pub order: Vec<Uuid>, pub basis: DistanceBasis, pub algorithm: &'static str }
#[async_trait] pub trait StopSequencer: Send + Sync {
    async fn sequence(&self, start: (f64, f64), stops: &[SeqStop]) -> Result<Sequence, SequenceError>;
}
pub struct NearestNeighborSequencer<M: DistanceMatrix> { /* */ }
```

- [ ] **Step 1: Write failing tests:** `starts_from_the_store`, `uses_road_distance_over_straight_line` (fixture where the two disagree), `matrix_failure_falls_back_to_direct`, `null_route_in_matrix_falls_back_for_that_pair`, `earlier_window_is_honored`, `locked_stop_stays_first`, `twenty_five_stops_are_refused`, `zero_zero_coordinates_are_refused`, `one_stop_makes_no_matrix_call`, `same_input_gives_the_same_order`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-dispatch --lib stop_sequencer` → FAIL.
- [ ] **Step 3: Implement** greedy nearest-neighbor, tie-breaking on `job_id`. HTTP timeout 3 seconds.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(dispatch): road-distance stop sequencer behind a trait`

---

### Task 3.4: Batch assignment

**Files:**
- Create: `services/dispatch/migrations/0022_route_sequence_history.sql`
- Create: `services/dispatch/src/application/services/partner_batch_service.rs`
- Create: `services/dispatch/src/api/http/partner_delivery.rs`
- Modify: `libs/events/src/payloads.rs:457`
- Test: `services/dispatch/tests/partner_batch_db.rs`

**Interfaces:**
- Consumes: `StopSequencer`, `PartnerJobRepository`.
- Produces: `GET /v1/partner-delivery/jobs`, `POST /v1/partner-delivery/batches`, body `{ rider_user_id, job_ids[] }`, with an `Idempotency-Key` header.
- Produces: `TaskAssigned.partner_retry_allowed: bool` with `#[serde(default)]`.

- [ ] **Step 1: Write failing DB tests:**
  - `batch_creates_one_route_n_stops_one_assignment`
  - `publishes_one_delivery_task_per_stop_and_no_pickup`
  - `payout_is_explicit_zero_not_null`
  - `another_stores_job_is_refused`, `another_stores_rider_is_refused`
  - `rider_with_an_active_assignment_is_refused`
  - `job_not_at_store_is_refused`, `failed_job_with_no_attempts_left_is_refused`
  - `twenty_five_jobs_are_refused`
  - `same_idempotency_key_returns_the_same_batch`
  - `two_managers_racing_for_one_job_have_one_winner`
  - `finishing_the_batch_frees_the_rider` (uses Task 0.12)
  - `suspended_store_cannot_create_a_batch`
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-dispatch --test partner_batch_db` → FAIL.
- [ ] **Step 3: Implement.** One transaction with `FOR UPDATE` on the jobs. Publish `TASK_ASSIGNED` from an outbox. Set `partner_retry_allowed = attempt_no < max_attempts`, and carry the partner flag through driver-ops so `to_summary` shows zero.
- [ ] **Step 4: Confirm they pass.** Same command → PASS, non-zero duration.
- [ ] **Step 5: Commit.** `feat(dispatch): store batch assignment with no gig payout`

---

### Task 3.5: Manual reorder

**Files:**
- Modify: `services/dispatch/src/application/services/partner_batch_service.rs`, `src/api/http/partner_delivery.rs`

**Interfaces:**
- Produces: `PUT /v1/partner-delivery/batches/{id}/sequence`, body `{ job_ids[], expected_version }`.

- [ ] **Step 1: Write failing tests:** `manager_can_reorder`, `in_progress_stop_is_locked`, `dispatcher_override_may_move_it_and_is_recorded`, `missing_or_extra_job_is_refused`, `stale_version_gets_409`, `every_sequence_is_kept_in_history`, `rider_receives_the_new_order`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-dispatch --test partner_batch_db sequence` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(dispatch): manual stop reorder with history`

---

### Task 3.6: Physical release to rider

**Files:**
- Modify: `services/hub-ops/src/application/services/tote_receipt_service.rs`, `src/api/http/totes.rs`
- Modify: `services/order-intake/src/infrastructure/messaging/status_consumer.rs`
- Test: `services/hub-ops/tests/tote_receipt_db.rs`, `services/order-intake/tests/partner_status_db.rs`

**Interfaces:**
- Produces: `POST /v1/totes/{id}/pieces/{pieceId}/release`, body `{ rider_user_id, client_ref, device_timestamp }`.
- Publishes `HUB_TOTE_PIECE_RELEASED { tenant_id, tote_id, shipment_id, carrier_id, rider_user_id, attempt_no, device_timestamp }`.

- [ ] **Step 1: Write failing tests:** `release_moves_custody_to_the_rider`, `release_to_a_rider_not_on_the_batch_is_refused`, `clerk_sees_cod_only_at_release`, `released_sets_out_for_delivery`, `release_after_delivery_attempted_sets_out_for_delivery`, `release_cannot_reopen_a_terminal_shipment`, `sealed_tote_does_not_set_out_for_delivery`, `replay_is_harmless`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test tote_receipt_db release` and `cargo test -p logisticos-order-intake --test partner_status_db` → FAIL.
- [ ] **Step 3: Implement** the handler and a status arm that denies the terminal statuses and accepts `delivery_attempted`.
- [ ] **Step 4: Confirm they pass.** Same commands → PASS.
- [ ] **Step 5: Commit.** `feat(network): only a physical release puts a parcel out for delivery`

---

### Task 3.7: Failed attempts and retries

**Files:**
- Modify: `services/driver-ops/src/application/services/task_service.rs:287,391`
- Create: `services/driver-ops/migrations/0020_partner_task_fields.sql`
- Modify: `libs/events/src/payloads.rs`
- Modify: `services/order-intake/src/infrastructure/messaging/status_consumer.rs:253`
- Modify: `services/dispatch/src/infrastructure/messaging/tote_event_consumer.rs`

**Interfaces:**
- Produces: `DeliveryAttempted { shipment_id, task_id, tenant_id, driver_id, attempt_no, reason_code, lat, lng, device_timestamp }`. Its `shipment_id` must decode as `DeliveryAttemptedEvt` in `status_consumer.rs:68`.
- Adds `driver_ops.tasks.partner_retry_allowed BOOLEAN NOT NULL DEFAULT FALSE`.

- [ ] **Step 1: Write failing tests:**
  - `partner_task_with_attempts_left_publishes_attempted_not_failed`
  - `final_attempt_publishes_failed`
  - `non_partner_task_still_publishes_failed`, for both `fail_task` and `leave`
  - `late_attempted_after_failed_does_not_reopen`
  - `attempt_inside_200m_is_valid_evidence`, `outside_or_missing_gps_goes_to_review`
  - `job_returns_to_at_store_and_can_be_rebatched`
  - `third_attempt_is_impossible`
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-driver-ops --test unit attempt`, `cargo test -p logisticos-order-intake --test partner_status_db attempted`, `cargo test -p logisticos-dispatch --test partner_job_events_db attempt` → FAIL.
- [ ] **Step 3: Implement.** Add `"failed"` to the deny list of the `DELIVERY_ATTEMPTED` status arm. No producer exists today, so this cannot affect current traffic.
- [ ] **Step 4: Confirm they pass.** Same commands → PASS.
- [ ] **Step 5: Commit.** `feat(driver-ops): store riders can retry a failed delivery`

---

### Task 3.8: Returns

**Files:**
- Modify: `services/hub-ops/src/api/http/transfer_runs.rs`, `tote_receipt_service.rs`
- Modify: `services/order-intake/src/infrastructure/messaging/status_consumer.rs`
- Modify: `services/dispatch/src/infrastructure/messaging/tote_event_consumer.rs`

**Interfaces:**
- Produces: `POST /v1/totes/{id}/pieces/{pieceId}/back-at-store`, `POST /v1/transfer-runs/{id}/stops/{stopId}/collect-returns`, `POST /v1/totes/returns/receive`.
- Publishes `HUB_TOTE_PIECE_RETURNED`.

- [ ] **Step 1: Write failing tests:** `rider_scan_back_moves_custody_to_the_store`, `collection_moves_custody_to_platform`, `hub_receipt_closes_the_piece`, `failed_shipment_can_become_returned`, `delivered_shipment_cannot_become_returned`, `tote_reconciles_when_every_piece_is_closed`, `store_with_no_returns_is_a_no_op`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test tote_receipt_db return` and `cargo test -p logisticos-order-intake --test partner_status_db returned` → FAIL.
- [ ] **Step 3: Implement.** The `piece_returned` arm explicitly allows `failed`, so the shared terminal guard does not drop it.
- [ ] **Step 4: Confirm they pass.** Same commands → PASS.
- [ ] **Step 5: Commit.** `feat(network): return path from store to hub`

---

### Task 3.9: Cancellation and recall

**Files:**
- Modify: `services/dispatch/src/infrastructure/db/dispatch_queue_repo.rs:220`, `src/infrastructure/messaging/shipment_cancelled_consumer.rs`
- Modify: `services/hub-ops/src/application/services/tote_receipt_service.rs`
- Create: `services/hub-ops/src/infrastructure/partner_job_consumer.rs`

**Interfaces:**
- Consumes: `PARTNER_JOB_CHANGED` with `status = cancelled`.
- Produces: `POST /v1/totes/{id}/pieces/{pieceId}/recall` (`TOTES_ADMIN`).

- [ ] **Step 1: Write failing tests**, one for each row of the spec's "Cancellations and returns" table, asserting physical party, fee eligibility and liable party. Also: `cancel_with_rider_cancels_the_task`, `recalled_parcel_is_not_queued_until_a_platform_scan`, `platform_scan_returns_it_to_pending`, `suspended_store_keeps_open_jobs_until_recalled`, `no_software_path_requeues_a_store_held_parcel`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-dispatch --test partner_job_events_db cancel` and `cargo test -p logisticos-hub-ops --test tote_receipt_db recall` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Confirm they pass.** Same commands → PASS.
- [ ] **Step 5: Commit.** `feat(network): cancellation and physical recall`

---

### Task 3.10: Recipient data limits

**Files:**
- Create: `services/dispatch/migrations/0023_recipient_access_log.sql`
- Modify: `services/dispatch/src/api/http/partner_delivery.rs`, `services/driver-ops/src/application/services/task_service.rs:136`

**Interfaces:**
- Produces: DTOs `ClerkJobView`, `RiderStopView` and `ManagerJobView`, each built explicitly from the spec's visibility table.

- [ ] **Step 1: Write failing tests:** table test of every cell in the spec's "What a store may see" table; `contact_is_gone_after_job_close`; `every_contact_read_is_logged_with_actor_tenant_ip`; `merchant_name_is_absent_from_rider_tasks`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-dispatch --test partner_batch_db visibility` → FAIL.
- [ ] **Step 3: Implement.** For partner tasks, the task snapshot sets `merchant_name` to an empty string and the declared value to `None`.
- [ ] **Step 4: Confirm they pass.** Same command and `cargo test -p logisticos-driver-ops --test unit` → PASS.
- [ ] **Step 5: Commit.** `feat(network): stores see only the recipient data they need`

---

### Task 3.11: Dispatch timers

**Files:**
- Create: `services/dispatch/migrations/0024_operational_timers.sql`
- Create: `services/dispatch/src/application/services/timer_sweep.rs`
- Test: `services/dispatch/tests/partner_timers_db.rs`

**Interfaces:**
- Consumes: `logisticos_op_timers::decide`.

- [ ] **Step 1: Write failing tests:** spec values for "not assigned to a rider" (30/60/90), "rider inactive" (10/20/30) and "delivery overdue" (10/20/30); `last_scan_resets_inactivity_even_without_gps`; `one_batch_raises_one_alert`; `paused_outside_store_hours`; `observe_only_never_reassigns`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-dispatch --test partner_timers_db` → FAIL.
- [ ] **Step 3: Implement** `dispatch.operational_sla_rules` and `dispatch.sla_escalations` in the spec's shape, with the sweep from hub-ops Task 2.12.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(dispatch): observe-only delivery timers`

---

### Task 3.12: Customer messages and tracking

**Files:**
- Modify: `services/engagement/src/application/services/event_consumer.rs`
- Modify: `services/delivery-experience/src/application/handlers/mod.rs`, `src/bootstrap.rs` (subscribed topics)
- Modify: `apps/landing/src/app/track/page.tsx`, the public tracking page, and `apps/customer-portal/src/lib/api/tracking.ts`, with `en` and `ar` messages

**Interfaces:**
- Consumes: `HUB_TOTE_RECEIVED`, `HUB_TOTE_PIECE_RELEASED`.

The engagement match arm is required, because `event_consumer.rs` does not read the DB template registry.

- [ ] **Step 1: Write failing tests:** `received_sends_with_your_local_store`, `released_sends_out_for_delivery_once`, `opted_out_customer_gets_nothing`, `tracking_shows_the_store_name_and_no_merchant_data`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-engagement --lib tote` and `cargo test -p logisticos-delivery-experience --lib tote` → FAIL.
- [ ] **Step 3: Implement** match arms, milestones and translated copy.
- [ ] **Step 4: Confirm they pass.** Same commands plus `npm run lint && npm run build` in `apps/landing` and `apps/customer-portal`, with screenshots at 375 and 1280 px in `en` and `ar`.
- [ ] **Step 5: Commit.** `feat(engagement): tell customers their parcel is with a local store`

---

### Task 3.13: Store Workspace

**Files:**
- Create: `apps/partner-portal/src/app/(dashboard)/store/assign/page.tsx`, `riders/page.tsx`, `exceptions/page.tsx`, `receive/page.tsx`, `settings/page.tsx`
- Create: `apps/partner-portal/src/lib/api/store.ts`
- Modify: `apps/partner-portal/src/app/(dashboard)/store/page.tsx`

**Interfaces:**
- Consumes: dispatch and hub-ops routes. Types are generated from OpenAPI.

- [ ] **Step 1: Write failing jest tests:** `availability_card_defaults_to_unanswered`, `batch_builder_stops_at_24`, `conflict_shows_refresh_prompt`.
- [ ] **Step 2: Confirm they fail.** `npm test -- store` in `apps/partner-portal` → FAIL.
- [ ] **Step 3: Implement** Today, Receive status, Assign batch, Riders, Exceptions and Settings from the spec's phone-first wireframe. Use glass panels, neon state, and motion that respects `prefers-reduced-motion`.
- [ ] **Step 4: Verify.** `npm run lint && npm run typecheck && npm test && npm run build`. Screenshots at 320, 375, 768 and 1280 px in `en` and `ar`, keyboard pass, contrast at WCAG AA.
- [ ] **Step 5: Commit.** `feat(partner-portal): store workspace for receiving and batching`

---

### Task 3.14: Android rider and clerk release

**Files:**
- Modify: `apps/driver-app-android/feature/route/` (batch stop order), `feature/delivery/` (retry and scan-back)
- Modify: `feature/hub/.../store/StoreReceiveScreen.kt`, `StoreReceiveViewModel.kt` (release)
- Modify: `app/src/main/res/values/strings.xml`, `values-ar/strings.xml`

**Interfaces:**
- Consumes: release, back-at-store and existing task routes. Reads `partner_retry_allowed`.

- [ ] **Step 1: Write failing tests:** `rider_sees_batch_order`, `retry_allowed_shows_try_again_later`, `final_attempt_shows_return_to_store`, `zero_payout_shows_no_earnings_line`, `release_is_queued_offline_with_device_timestamp`.
- [ ] **Step 2: Confirm they fail.** `./gradlew :feature:route:testDebugUnitTest :feature:delivery:testDebugUnitTest :feature:hub:testDebugUnitTest` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Confirm they pass.** The same tasks plus their `testReleaseUnitTest` variants → PASS. `./gradlew :app:assembleDebug` succeeds.
- [ ] **Step 5: Commit.** `feat(driver-app): store rider batch, retry and release`

---

### Task 3.15: Device run and gate

**Files:**
- Create: `e2e/neighborhood/store-delivery.spec.ts`, `docs/runbooks/store-delivery.md`

- [ ] **Step 1: Write E2E scenarios:** deliver, fail then retry then deliver, final failure then return, cancel at store, rider goes silent.
- [ ] **Step 2: Run them** on a local stack. Expected: all pass, custody reconciliation returns zero rows.
- [ ] **Step 3: Commit.** `test(network): store delivery end to end`
- [ ] **Step 4: Gate.** With the owner's go-ahead, run on staging with real devices. Show that automatic assigners cannot take a reserved parcel, and that a store rider earns no platform gig pay.
