# Neighborhood Network Phase 2: Totes and Custody Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sort parcels into sealed totes, carry them to an internal test store, and record every change of custody with no unexplained pieces.

**Architecture:** Hub-ops owns waves, tote trips, permanent tote assets, single-use seals, transfer runs, and an append-only custody log. Dispatch reserves each parcel synchronously at sort, which removes it from every automatic assigner.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL, Kafka, Kotlin (Compose, Room, WorkManager), Next.js 14.

**Spec:** [Neighborhood Delivery Network](../specs/2026-11-23-neighborhood-delivery-network-design.md), section 7 (Waves, totes, custody; Dispatch reservation; Operational timers). **Program plan:** [index](2026-11-23-neighborhood-delivery-network.md).

## Global Constraints

- **Entry condition:** the Phase 1 gate has passed.
- Next migrations: hub-ops `0018`, dispatch `0020`.
- Code term is **tote**. "Basket" means an OmniDeliv cart.
- Custody event types are a Rust enum on a `TEXT` column.
- `tote_custody_events` and `seal_events` are append-only: `REVOKE UPDATE, DELETE` for the app role.
- Offline scans carry `client_ref`, and replay returns the original result. Physical scans are not version-checked. Commands use `expected_version`.
- The clerk scans every piece. An intact seal is not evidence of what is inside.
- A discrepancy opens a case and moves no money.
- If dispatch or order-intake is unavailable at sort, the parcel is **held**, never bagged on a guess.
- Labels carry no customer name, phone or address.
- All timers are observe-only in this phase.
- `quick_dispatch` must claim its row with a conditional `status = 'pending'` update.

## Review Focus

1. A clerk's offline scan arrives after the tote has moved on. The fact is recorded, and state must not move backward (Task 2.4).
2. Two sorters reserve the store's last capacity slot at once. One must get the next partner or a fallback (Task 2.5).
3. Reservation succeeds and the piece insert then fails. The job and capacity must both be released (Task 2.6).
4. A used or foreign seal is scanned at sealing. It must be refused (Task 2.8).
5. The store is closed at drop-off. The tote stays in platform custody and the store is never liable (Task 2.9).

---

### Task 2.1: Contracts, topics and gateway routes

**Files:**
- Modify: `docs/api/openapi/hub-ops.yaml`, `dispatch.yaml`, `scripts/openapi-tracked-prefixes.txt`
- Modify: `libs/events/src/topics.rs`, `libs/events/src/payloads.rs`, `scripts/kafka/create-topics.sh`, `scripts/create-kafka-topics.sh`
- Modify: `services/api-gateway/src/proxy/mod.rs`
- Modify: `libs/auth/src/rbac.rs`

**Interfaces:**
- Produces topics: `HUB_TOTE_DISPATCHED`, `HUB_TOTE_RECEIVED`, `HUB_TOTE_PIECE_RELEASED`, `HUB_TOTE_PIECE_RETURNED`, `PARTNER_JOB_CHANGED`, `PARTNER_DISCREPANCY_RESOLVED`, with the topic strings from the spec's Events table.
- Produces permission `TOTES_ADMIN = "totes:admin"` for `admin`, `tenant_admin` and `dispatcher`. It covers waves, seal issue, transfer runs and case resolution.
- Grants `TOTES_SORT` to `hub_scanner`. Adds role `transfer_driver` with `TOTES_READ` and `TOTES_CARRY`, assigned alongside `driver`, and adds it to `ASSIGNABLE_ROLES`.

- [ ] **Step 1: Write failing tests:** gateway routes `/v1/sort-waves`, `/v1/totes`, `/v1/seals` and `/v1/transfer-runs` to hub-ops; `transfer_driver` holds exactly its two permissions.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-api-gateway --lib` and `cargo test -p logisticos-auth --lib` → FAIL.
- [ ] **Step 3: Implement** contracts, payloads with `#[serde(default)]` on optional fields, topic scripts, routes and roles. Classify any new handler in `scripts/partner-scope-inventory.yaml`.
- [ ] **Step 4: Confirm they pass.** Same commands plus `cargo test -p logisticos-events`, `bash scripts/check-kafka-topics.sh` → pass.
- [ ] **Step 5: Commit.** `feat(network): phase 2 contracts, topics and roles`

---

### Task 2.2: Partner jobs and reservation

Moved here from Phase 3, because a parcel placed in a tote must already be out of reach of the automatic assigners.

**Files:**
- Create: `services/dispatch/migrations/0020_partner_delivery_jobs.sql`
- Create: `services/dispatch/src/domain/entities/partner_job.rs`
- Create: `services/dispatch/src/infrastructure/db/partner_job_repo.rs`
- Create: `services/dispatch/src/api/http/partner_jobs_internal.rs`
- Modify: `services/dispatch/src/application/services/driver_assignment_service.rs:556`, `src/infrastructure/db/dispatch_queue_repo.rs`
- Test: `services/dispatch/tests/partner_reserve_db.rs`

**Interfaces:**
- Produces: `POST /v1/internal/partner-jobs/reserve`, body `ReserveJob { tenant_id, shipment_id, carrier_id, tote_id, rate_id, delivery_fee_cents, failed_attempt_fee_cents, cod_amount_cents, dest_lat, dest_lng }`
- Produces: `POST /v1/internal/partner-jobs/{shipment_id}/release`
- Produces: `PartnerJobRepository::reserve(&ReserveJob) -> Result<ReserveOutcome>`, `ReserveOutcome::{Reserved(job), AlreadyReserved(job), Refused(reason)}`
- Produces: `DispatchQueueRepository::claim_pending(shipment_id) -> Result<bool>`

Migration replaces `dispatch_queue_status_check` to add `'partner_fulfilment'`, and creates the spec's table and partial unique index.

- [ ] **Step 1: Write failing DB tests:**
  - `reserve_moves_the_queue_row_and_creates_the_job`
  - `no_queue_row_is_legal`
  - `open_driver_task_refuses_the_reservation`
  - `replay_returns_the_same_job`
  - `second_carrier_is_refused_while_a_job_is_open`
  - `auto_assigners_cannot_see_a_reserved_row`: exercises each of `shipment_consumer`, the sweep, `driver_available_consumer` and `hub_dispatch_consumer`.
  - `quick_dispatch_loses_the_race_to_reserve`
  - `release_returns_the_row_to_pending`
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-dispatch --test partner_reserve_db` → FAIL.
- [ ] **Step 3: Implement** in one transaction with `SELECT ... FOR UPDATE` on the queue row. Replace `mark_dispatched` in `quick_dispatch` with `claim_pending` and return a conflict when it reports false.
- [ ] **Step 4: Confirm they pass.** Same command → PASS, non-zero duration.
- [ ] **Step 5: Commit.** `feat(dispatch): reserve parcels for a store, out of every auto-assigner`

---

### Task 2.3: Tote schema

**Files:**
- Create: `services/hub-ops/migrations/0018_sort_waves.sql`, `0019_totes.sql`, `0020_seals.sql`, `0021_transfer_runs.sql`, `0022_discrepancy_cases.sql`
- Test: `services/hub-ops/tests/tote_schema_db.rs`

**Interfaces:**
- Produces the spec's tables exactly, plus:
  - `transfer_runs(id, tenant_id, hub_id, wave_id, driver_user_id, status, planned_depart_at, departed_at, version)`
  - `transfer_run_stops(id, tenant_id, run_id, carrier_id, sequence, planned_arrival_at, arrived_at, outcome, UNIQUE (run_id, sequence))`
  - `transfer_run_totes(run_id, stop_id, tote_id, loaded_at, UNIQUE (tote_id, run_id))`
  - `discrepancy_cases(id, tenant_id, tote_id, shipment_id, kind, status, opened_by, opened_at, liable_party, charge_cents, resolved_by, resolved_at, resolution_note, client_ref, UNIQUE (tote_id, client_ref))`

- [ ] **Step 1: Write failing tests:** `asset_has_one_active_trip`, `shipment_has_one_open_piece`, `reserved_cannot_exceed_capacity`, `seal_serves_one_tote_ever`, `custody_rejects_a_duplicate_client_ref`, `app_role_cannot_update_custody`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test tote_schema_db` → FAIL.
- [ ] **Step 3: Write the migrations.** Apply them twice on a disposable database to confirm a clean first run and no drift.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): wave, tote, seal and transfer schema`

---

### Task 2.4: Tote and piece state machines

**Files:**
- Create: `services/hub-ops/src/domain/entities/tote.rs`, `custody.rs`
- Modify: `services/hub-ops/src/domain/entities/mod.rs`

**Interfaces:**
- Produces:

```rust
pub enum ToteStatus { Open, Sealed, InTransit, AtStore, Received, ReceivedShort, ReturnedUndelivered, Reconciled, Cancelled }
pub enum PieceStatus { Assigned, InTransit, Received, WithRider, Delivered, Failed, ReturnPending, ReturnedToHub, Missing, Damaged, Cancelled }
pub enum CustodyEventType { Sealed, LoadedOut, ArrivedAtStore, StoreClosed, ToteReceived, PieceReceived, PieceReleased, PieceBackAtStore, PieceDelivered, ReturnCollected, ReturnedToHub, MarkedMissing, MarkedDamaged, SealException }
pub enum Party { Hub(Uuid), Platform, Store(Uuid), Rider(Uuid), Customer }
pub enum ScanEffect { Applied(PieceStatus), RecordedOutOfOrder }
pub fn apply_piece_scan(current: PieceStatus, event: CustodyEventType) -> ScanEffect;
pub fn accountable_party(status: PieceStatus, ctx: &CustodyContext) -> Party;
```

- [ ] **Step 1: Write failing tests:** a table of every legal transition from the spec's diagrams; `no_scan_moves_a_delivered_piece`; `late_received_after_with_rider_is_recorded_out_of_order`; `failed_can_be_re_released`; `any_held_state_can_go_missing_or_damaged`; `accountable_party_matches_the_spec_liability_table` for all five stages; `an_out_of_order_scan_never_changes_the_accountable_party`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --lib tote` → FAIL.
- [ ] **Step 3: Implement** as pure functions with no I/O.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): tote and custody state machines`

---

### Task 2.5: Waves, availability and capacity

**Files:**
- Create: `services/hub-ops/src/domain/entities/sort_wave.rs`
- Create: `services/hub-ops/src/infrastructure/db/wave_repo.rs`, `src/api/http/sort_waves.rs`
- Test: `services/hub-ops/tests/wave_capacity_db.rs`

**Interfaces:**
- Produces: `GET, POST /v1/sort-waves`, `PUT /v1/sort-waves/{id}/availability`, with the carrier taken from the JWT.
- Produces: `WaveRepository::{reserve_capacity(wave, carrier, cod_cents) -> Result<bool>, release_capacity(wave, carrier, cod_cents) -> Result<()>}`

- [ ] **Step 1: Write failing tests:** `silence_means_no_capacity`, `availability_after_cutoff_is_refused`, `store_cannot_set_another_stores_availability`, `capacity_over_daily_limit_is_refused`, `twenty_concurrent_reservations_never_overfill`, `release_returns_capacity`, `lowering_capacity_below_reserved_is_refused`, `cutoffs_must_be_in_order`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test wave_capacity_db` → FAIL.
- [ ] **Step 3: Implement** with `UPDATE ... SET reserved = reserved + 1 WHERE reserved < confirmed_capacity`.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): sort waves with store-confirmed capacity`

---

### Task 2.6: Live sort scan

**Files:**
- Create: `services/hub-ops/src/application/services/tote_sort_service.rs`
- Create: `services/hub-ops/src/infrastructure/dispatch_client.rs`, `src/api/http/totes.rs`
- Modify: `services/hub-ops/src/application/services/hub_transfer_service.rs:334`
- Test: `services/hub-ops/tests/tote_sort_db.rs`

**Interfaces:**
- Consumes: `resolve`, `reserve_capacity`, `POST /v1/internal/partner-jobs/reserve`.
- Produces: `POST /v1/totes/sort-scan`, body `{ hub_id, awb, client_ref, device_timestamp }`, response:

```rust
pub enum SortScanResult {
    Tote { label: String, tote_id: Uuid, trip_code: String, piece_id: Uuid },
    OpenNewTote { label: String },
    Hold { reason: String },
    Fallback { reason: String },
}
```

- [ ] **Step 1: Write failing tests:**
  - `resolved_parcel_goes_in_the_open_tote`
  - `full_tote_says_open_another_for_the_same_label`, by weight, by volume and by longest side
  - `parcel_without_dimensions_uses_the_default_volume_and_is_flagged`
  - `dispatch_down_holds_and_releases_capacity`
  - `facts_down_holds`
  - `piece_insert_failure_releases_job_and_capacity`
  - `replayed_client_ref_returns_the_original_result`
  - `partner_without_a_rate_is_skipped`
  - `fallback_uses_the_existing_routing_path_unchanged`
  - `tote_bound_piece_skips_hub_dispatch_requested`
  - `shadow_hub_never_writes_a_tote`
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test tote_sort_db` → FAIL.
- [ ] **Step 3: Implement.** Order: facts, resolve, reserve capacity, reserve job, insert piece and custody event in one transaction. Compensate on failure. In `fan_out_routing`, a `live` hub calls this service first.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): live sort scan into totes`

---

### Task 2.7: Tote assets and labels

**Files:**
- Create: `services/hub-ops/src/infrastructure/db/tote_repo.rs` (extend from Task 2.6)
- Create: `apps/admin-portal/src/components/neighborhood/ToteLabelSheet.tsx`

**Interfaces:**
- Produces: `POST /v1/tote-assets`, `POST /v1/totes/{id}/label-printed` (increments `label_print_count`).
- Label data DTO: `{ trip_code, label, partner_code, zone, qr_value }`, with no recipient fields.

- [ ] **Step 1: Write failing tests:** `asset_codes_are_sequential_and_unique`, `lost_asset_cannot_open_a_trip`, `reprint_is_counted`, and a jest test `label_never_renders_customer_fields`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test tote_sort_db label` and `npm test -- ToteLabel` → FAIL.
- [ ] **Step 3: Implement** using the `react-qr-code` print pattern in `apps/admin-portal/src/components/venues/table-print-sheet.tsx`, at 100 × 150 mm, one label per page.
- [ ] **Step 4: Confirm they pass.** Same commands, and a browser print-preview check → PASS.
- [ ] **Step 5: Commit.** `feat(network): tote assets and trip labels`

---

### Task 2.8: Seal inventory and sealing

**Files:**
- Create: `services/hub-ops/src/domain/entities/security_seal.rs`
- Create: `services/hub-ops/src/infrastructure/db/seal_repo.rs`, `src/api/http/seals.rs`
- Test: `services/hub-ops/tests/seal_db.rs`

**Interfaces:**
- Produces: `POST /v1/seals/issue` (`TOTES_ADMIN`), `POST /v1/totes/{id}/seal`, body `{ seal_serial, client_ref, device_timestamp, expected_version }`.

- [ ] **Step 1: Write failing tests:** `unissued_seal_is_refused`, `used_seal_is_refused`, `other_hubs_seal_is_refused`, `sealing_writes_seal_event_and_custody_together`, `empty_tote_cannot_be_sealed`, `stale_version_gets_409`, `replay_returns_the_original_result`, `replacement_is_a_new_seal_plus_a_replaced_event`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test seal_db` → FAIL.
- [ ] **Step 3: Implement.** A single transaction updates tote and seal and appends both event rows.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): serialized single-use tote seals`

---

### Task 2.9: Transfer runs and handover

**Files:**
- Create: `services/hub-ops/src/domain/entities/transfer_run.rs`
- Create: `services/hub-ops/src/infrastructure/db/transfer_run_repo.rs`, `src/api/http/transfer_runs.rs`
- Test: `services/hub-ops/tests/transfer_run_db.rs`

**Interfaces:**
- Produces: `POST /v1/transfer-runs`, `POST /v1/transfer-runs/{id}/load`, `POST /v1/transfer-runs/{id}/stops/{stopId}/handover`, body `{ outcome: "arrived" | "store_closed", client_ref, device_timestamp, lat, lng }`.
- Publishes `HUB_TOTE_DISPATCHED` when loaded.

- [ ] **Step 1: Write failing tests:** `unsealed_tote_cannot_be_loaded`, `load_moves_custody_to_platform`, `arrival_does_not_move_custody_to_the_store`, `store_closed_returns_tote_with_platform_custody`, `driver_sees_only_their_run`, `store_sees_only_its_own_totes_on_a_shared_run`, `replay_is_harmless`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test transfer_run_db` → FAIL.
- [ ] **Step 3: Implement.** Use a transactional outbox row for each published event so a crash cannot lose it.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): transfer runs with per-stop handover`

---

### Task 2.10: Store receipt

**Files:**
- Create: `services/hub-ops/src/application/services/tote_receipt_service.rs`
- Modify: `services/hub-ops/src/api/http/totes.rs`
- Test: `services/hub-ops/tests/tote_receipt_db.rs`

**Interfaces:**
- Produces: `POST /v1/totes/{id}/receive`, with actions `verify_seal`, `scan_piece` and `finish`; `GET /v1/totes`, `GET /v1/totes/{id}`, filtered to `pid` for store tokens.
- Publishes `HUB_TOTE_RECEIVED` on `finish`.

- [ ] **Step 1: Write failing tests:**
  - `matching_seal_is_consumed`
  - `mismatched_seal_records_an_exception_and_platform_stays_liable`
  - `store_may_refuse_a_broken_seal_tote`
  - `each_piece_scan_moves_custody_for_that_piece_only`
  - `finish_with_all_pieces_is_received`, `finish_short_is_received_short_and_opens_a_case`
  - `extra_piece_opens_a_case_and_is_not_accepted`
  - `another_stores_token_gets_404`
  - `clerk_response_has_no_recipient_fields`
  - `replay_returns_the_original_result`
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test tote_receipt_db` → FAIL.
- [ ] **Step 3: Implement** the receipt flow in spec order.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): piece-level store receipt with seal verification`

---

### Task 2.11: Discrepancy cases

**Files:**
- Create: `services/hub-ops/src/domain/entities/discrepancy_case.rs`
- Create: `services/hub-ops/src/infrastructure/db/discrepancy_repo.rs`, `src/api/http/discrepancies.rs`

**Interfaces:**
- Produces: `POST /v1/totes/{id}/discrepancies`, `GET /v1/discrepancies`, `POST /v1/discrepancies/{id}/resolve` (`TOTES_ADMIN`), body `{ liable_party, charge_cents, note, expected_version }`.
- Publishes `PartnerDiscrepancyResolved { tenant_id, case_id, carrier_id, shipment_id, tote_id, liable_party, charge_cents, currency, resolved_at }`.

- [ ] **Step 1: Write failing tests:** `store_can_open_a_case_on_its_own_tote`, `opening_a_case_publishes_no_money_event`, `only_ops_can_resolve`, `store_cannot_be_charged_for_an_unreceived_piece`, `resolved_case_cannot_be_resolved_again`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test tote_receipt_db discrepancy` → FAIL.
- [ ] **Step 3: Implement** with an outbox event on resolution only.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): discrepancy cases that move no money until resolved`

---

### Task 2.12: Timer crate and hub timers

**Files:**
- Create: `libs/op-timers/Cargo.toml`, `libs/op-timers/src/lib.rs`
- Modify: root `Cargo.toml`, and the Docker build context lists for hub-ops and dispatch if they enumerate libs
- Create: `services/hub-ops/migrations/0023_operational_timers.sql`
- Create: `services/hub-ops/src/application/services/timer_sweep.rs`
- Test: `services/hub-ops/tests/timer_sweep_db.rs`

**Interfaces:**
- Produces in `logisticos-op-timers`:

```rust
pub enum Level { Warning, Escalation, Critical }
pub struct Thresholds { pub warning: i64, pub escalation: i64, pub critical: i64 }
pub fn decide(clock_start: DateTime<Utc>, now: DateTime<Utc>, t: &Thresholds, already: &[Level], window: Option<&OperatingHours>) -> Option<Level>;
```

- [ ] **Step 1: Write failing tests:** the spec's values at each boundary for all hub timers; `no_level_is_raised_twice`; `sweep_gap_raises_only_the_highest_due_level`; `clock_pauses_outside_operating_hours`; `one_late_tote_of_30_is_one_alert`; `resolved_aggregate_raises_nothing`; `observe_only_changes_no_state`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-op-timers` and `cargo test -p logisticos-hub-ops --test timer_sweep_db` → FAIL.
- [ ] **Step 3: Implement.** The migration creates the spec's `hub_ops.operational_sla_rules` and `hub_ops.sla_escalations`. Seed tenant defaults from the spec's table for the hub timers. Run the sweep once a minute behind a Postgres advisory lock.
- [ ] **Step 4: Confirm they pass.** Same commands → PASS. `docker build -f services/hub-ops/Dockerfile .` succeeds with the new lib.
- [ ] **Step 5: Commit.** `feat(hub-ops): observe-only operational timers`

---

### Task 2.13: Android hub sort, Tote run and Store mode

**Files:**
- Create under `apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/`: `tote/ToteRepository.kt`, `tote/ToteSortViewModel.kt`, `tote/ToteSortScreen.kt`, `run/ToteRunViewModel.kt`, `run/ToteRunScreen.kt`, `store/StoreReceiveViewModel.kt`, `store/StoreReceiveScreen.kt`
- Modify: the app navigation graph and the sync handler in `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/worker/OutboundSyncWorker.kt`
- Modify: `apps/driver-app-android/app/src/main/res/values/strings.xml`, `values-ar/strings.xml`
- Test: `feature/hub/src/test/kotlin/.../tote/*Test.kt`, with JSON fixtures copied from the OpenAPI examples

**Interfaces:**
- Consumes: Tasks 2.6, 2.8, 2.9 and 2.10 routes through hand-written Retrofit.
- Produces: role-based entry points. `totes:sort` opens Hub sort, `totes:carry` opens Tote run, `totes:receive` opens Store mode.

- [ ] **Step 1: Write failing tests:**
  - `sort_shows_the_large_label`, `hold_result_shows_exception_cage`
  - `device_timestamp_is_captured_at_scan_callback`
  - `offline_scan_is_enqueued_with_a_stable_client_ref`
  - `replayed_sync_uses_the_same_client_ref`
  - `conflict_response_does_not_retry_forever`
- [ ] **Step 2: Confirm they fail.** From `apps/driver-app-android` with JBR 21: `./gradlew :feature:hub:testDebugUnitTest` → FAIL.
- [ ] **Step 3: Implement.** Sort needs a network, and shows "Hold this parcel" offline. Tote run and Store mode work fully offline through the existing sync queue.
- [ ] **Step 4: Confirm they pass.** Run both variants: `./gradlew :feature:hub:testDebugUnitTest :feature:hub:testReleaseUnitTest` → PASS. `./gradlew :app:assembleDebug` succeeds.
- [ ] **Step 5: Commit.** `feat(driver-app): hub tote sort, tote run and store receive`

---

### Task 2.14: Admin wave board, tote tracker and discrepancy queue

**Files:**
- Create: `apps/admin-portal/src/app/(dashboard)/neighborhood-network/waves/page.tsx`, `totes/page.tsx`, `discrepancies/page.tsx`
- Modify: `apps/admin-portal/src/lib/api/neighborhood.ts`

**Interfaces:**
- Consumes: routes from Tasks 2.5–2.12.

- [ ] **Step 1: Build** the wave board, tote and seal tracker, reprint action, discrepancy resolution form with confirmation, and observe-only escalation list.
- [ ] **Step 2: Verify.** `npm run lint && npm run typecheck && npm run build`, with screenshots at 375, 768 and 1280 px.
- [ ] **Step 3: Commit.** `feat(admin-portal): wave board, tote tracker and discrepancy queue`

---

### Task 2.15: End-to-end rehearsal and gate

**Files:**
- Create: `e2e/neighborhood/tote-cycle.spec.ts`, `scripts/reports/custody-reconciliation.sql`
- Create: `docs/runbooks/tote-operations.md`

- [ ] **Step 1: Write the reconciliation query.** Every piece must have an accountable party and a closed or open reason. It returns unexplained pieces.
- [ ] **Step 2: Write the E2E scenarios** against a local stack: clean tote, short bag, mismatched seal, offline replay, store closed.
- [ ] **Step 3: Run them.** Expected: all pass and the reconciliation query returns zero rows.
- [ ] **Step 4: Commit.** `test(network): full tote cycle reconciles to zero`
- [ ] **Step 5: Gate.** With the owner's go-ahead, repeat on staging with an internal test store and real devices. A full tote cycle, including a short bag, a mismatched seal and an offline replay, reconciles to zero unexplained pieces.
