# Gig Offer Broadcast ("Grab") + Driver Earnings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Broadcast gig task offers to multiple drivers with an atomic first-claim-wins grab, and surface earnings + COD cash position in the driver app Profile tab.

**Architecture:** New `task_offers`/`task_offer_candidates` tables in dispatch with a Postgres CAS claim; per-candidate payout snapshot (rates are per-driver); driver-ops consumes offer events for FCM fan-out; earnings are an indexed query over completed delivery tasks with snapshotted `payout_cents`; payments exposes a driver-scoped ledger read under `/v1/cod/` (gateway already routes that prefix).

**Tech Stack:** Rust (Axum/SQLx/rdkafka), Kotlin (Compose/Hilt/Retrofit), Postgres.

**Spec:** `docs/superpowers/specs/2026-06-12-gig-offer-broadcast-earnings-design.md`

**Design deviation (documented):** the spec put `payout_cents` on `task_offers`; rates are per-driver (`per_delivery_rate_cents`), so the snapshot lives on `task_offer_candidates` instead — each candidate sees their own contractual price, the winner's is copied to the task.

**Verification commands:** `$env:CARGO_INCREMENTAL='0'; cargo check -p logisticos-events -p dispatch -p driver-ops -p payments` and `cargo test -p dispatch --lib` (Android validated by GitHub Actions CI per project convention).

---

### Task 1: Events — `payout_cents` on TaskAssigned + offer payloads + topics

**Files:**
- Modify: `libs/events/src/payloads.rs`
- Modify: `libs/events/src/topics.rs`
- Modify: `services/dispatch/src/application/services/driver_assignment_service.rs` (test literal)
- Modify: `services/dispatch/tests/integration/main.rs` (test literal, if it constructs TaskAssigned)

- [ ] Add to `TaskAssigned` struct (serde-default, additive): `#[serde(default)] pub payout_cents: Option<i64>,`
- [ ] Add new payloads:

```rust
/// Broadcast gig offer — fanned out to N candidate drivers simultaneously.
/// driver-ops consumes this to send per-candidate FCM pushes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOfferCreated {
    pub offer_id:     Uuid,
    pub tenant_id:    Uuid,
    pub shipment_id:  Uuid,
    pub wave:         i32,
    pub expires_at:   chrono::DateTime<chrono::Utc>,
    /// (driver_id, payout_cents) — payout snapshotted per candidate from their
    /// per_delivery_rate_cents at offer creation; contractual once claimed.
    pub candidates:   Vec<OfferCandidate>,
    // Card display fields (mirror TaskAssigned card enrichment)
    #[serde(default)] pub merchant_name:     String,
    #[serde(default)] pub delivery_category: String,
    #[serde(default)] pub weight_grams:      u32,
    #[serde(default)] pub tracking_number:   String,
    #[serde(default)] pub customer_name:     String,
    #[serde(default)] pub pickup_address:    String,
    #[serde(default)] pub delivery_address:  String,
    #[serde(default)] pub cod_amount_cents:  Option<i64>,
    #[serde(default)] pub pickup_lat:        Option<f64>,
    #[serde(default)] pub pickup_lng:        Option<f64>,
    #[serde(default)] pub delivery_lat:      Option<f64>,
    #[serde(default)] pub delivery_lng:      Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferCandidate {
    pub driver_id:    Uuid,
    pub payout_cents: Option<i64>,
}

/// Offer reached a terminal state — losers' cards flip to "Taken" / dismiss.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOfferClosed {
    pub offer_id:    Uuid,
    pub tenant_id:   Uuid,
    pub shipment_id: Uuid,
    /// "claimed" | "expired" | "cancelled"
    pub reason:      String,
    pub claimed_by:  Option<Uuid>,
    /// Everyone who was ever offered it (all waves) — FCM fan-in targets.
    pub candidate_driver_ids: Vec<Uuid>,
}
```

- [ ] Topics: `pub const TASK_OFFER_CREATED: &str = "logisticos.dispatch.offer.created";` and `pub const TASK_OFFER_CLOSED: &str = "logisticos.dispatch.offer.closed";`
- [ ] Update every literal `TaskAssigned { ... }` constructor (dispatch service + tests) with `payout_cents: None` placeholder (real value wired in Task 2).
- [ ] Run `cargo check -p logisticos-events -p dispatch` → PASS. Commit: `feat(events): payout snapshot on TaskAssigned + task-offer payloads/topics`

### Task 2: Dispatch — gig rate lookup + payout snapshot in quick_dispatch

**Files:**
- Modify: `services/dispatch/src/domain/repositories/mod.rs` (DriverAvailabilityRepository trait)
- Modify: `services/dispatch/src/infrastructure/db/driver_avail_repo.rs`
- Modify: `services/dispatch/src/application/services/driver_assignment_service.rs`
- Modify: test fakes implementing the trait (grep `impl DriverAvailabilityRepository`)

- [ ] Trait method:

```rust
/// Gig payout snapshot source: Some(rate) when the driver is part_time,
/// None for full_time (the app must never show full-timers a price).
async fn gig_rate_cents(&self, driver_id: &DriverId) -> anyhow::Result<Option<i64>>;
```

- [ ] Pg impl (cross-schema read, same precedent as `find_available_near`):

```rust
async fn gig_rate_cents(&self, driver_id: &DriverId) -> anyhow::Result<Option<i64>> {
    let row: Option<(String, i32)> = sqlx::query_as(
        "SELECT driver_type, per_delivery_rate_cents
         FROM driver_ops.drivers WHERE user_id = $1",
    )
    .bind(driver_id.inner())
    .fetch_optional(&self.pool)
    .await?;
    Ok(row.and_then(|(t, rate)| (t == "part_time").then_some(i64::from(rate))))
}
```

- [ ] In `quick_dispatch`, after driver selection (step 3 compliance gate), fetch `let payout_cents = self.driver_avail_repo.gig_rate_cents(&driver_id).await.unwrap_or(None);` and set it on both TaskAssigned events.
- [ ] `cargo check -p dispatch` + `cargo test -p dispatch --lib` → PASS. Commit: `feat(dispatch): snapshot gig payout on TaskAssigned`

### Task 3: driver-ops — `payout_cents` column, snapshot persistence, summary reads snapshot

**Files:**
- Create: `services/driver-ops/migrations/0012_task_payout_snapshot.sql`
- Modify: `services/driver-ops/src/domain/entities/task.rs` (`DriverTask` + `payout_cents: Option<i64>`)
- Modify: `services/driver-ops/src/infrastructure/db/task_repo.rs` (SELECT/INSERT columns)
- Modify: `services/driver-ops/src/infrastructure/messaging/task_consumer.rs` (bind `t.payout_cents`)
- Modify: `services/driver-ops/src/application/services/task_service.rs`

- [ ] Migration:

```sql
-- Contractual payout snapshot: copied from the offer/assignment at creation.
-- The price shown to the gig driver at grab time is what they are paid —
-- later rate changes never alter accepted work. Earnings history reads this.
ALTER TABLE driver_ops.tasks ADD COLUMN IF NOT EXISTS payout_cents BIGINT;
-- Earnings range queries: SUM/GROUP BY over (driver, completed window).
CREATE INDEX IF NOT EXISTS idx_tasks_driver_status_completed
    ON driver_ops.tasks (driver_id, status, completed_at);
```

- [ ] `to_summary`: snapshot wins, live rate is fallback for pre-migration rows: `payout_cents: t.payout_cents.or(payout)`.
- [ ] `cargo check -p driver-ops` → PASS. Commit: `feat(driver-ops): persist payout snapshot on tasks`

### Task 4: driver-ops — earnings API

**Files:**
- Modify: `services/driver-ops/src/infrastructure/db/task_repo.rs`
- Modify: `services/driver-ops/src/domain/repositories/mod.rs`
- Modify: `services/driver-ops/src/api/http/tasks.rs` (new handler)
- Modify: `services/driver-ops/src/api/http/mod.rs` (route — NOTE: `/drivers/me/earnings` must be declared BEFORE `/drivers/:id`)

- [ ] Repo: earnings rows joined via `user_id` (tasks.driver_id is drivers.id, claims carry user_id):

```rust
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct EarningEntry {
    pub task_id:           Uuid,
    pub tracking_number:   Option<String>,
    pub merchant_name:     String,
    pub delivery_category: String,
    pub completed_at:      Option<chrono::DateTime<chrono::Utc>>,
    pub payout_cents:      Option<i64>,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct DailyEarning { pub day: chrono::NaiveDate, pub total_cents: i64, pub deliveries: i64 }
```

Methods on `TaskRepository`: `list_earnings(user_id, from, to, limit, offset) -> Vec<EarningEntry>` (completed delivery tasks, `ORDER BY completed_at DESC`), `daily_earnings(user_id, from, to) -> Vec<DailyEarning>` (`SUM(payout_cents)`, `COUNT(*)`, `GROUP BY completed_at::date`). Earnings count **delivery** tasks only — a shipment's pickup+delivery pair pays once.
- [ ] Handler `GET /v1/drivers/me/earnings?from&to&limit&offset` (defaults: from = today−30d, to = now, limit 50 cap 200): response `{ data: { today_cents, week_cents, daily: [...], entries: [...] } }` where today/week are computed from `daily`.
- [ ] `cargo check -p driver-ops` → PASS. Commit: `feat(driver-ops): driver earnings endpoint`

### Task 5: payments — driver-scoped ledger read

**Files:**
- Modify: `services/payments/src/domain/repositories/mod.rs` (`DriverLedgerRepository + list_recent_for_driver`)
- Modify: `services/payments/src/infrastructure/db/driver_ledger_repo.rs`
- Create: `services/payments/src/api/http/driver_ledger.rs`
- Modify: `services/payments/src/api/http/mod.rs` (route `/cod/driver-ledger/me`, AppState + `driver_ledger_repo`)
- Modify: `services/payments/src/bootstrap.rs` (inject repo into AppState)

- [ ] Repo method: `async fn list_recent_for_driver(&self, tenant_id: &TenantId, driver_id: Uuid, limit: i64) -> anyhow::Result<Vec<DriverLedger>>;` — ledgers (with entries) ordered `created_at DESC LIMIT $n`.
- [ ] Handler: driver identity = `claims.user_id`; response `{ data: { open_balance_cents, open_entries: [...], recent_ledgers: [{id, status, balance_cents, created_at, entry_count}] } }`. Route under `/v1/cod/` so the existing gateway prefix routes it — no gateway change.
- [ ] `cargo check -p payments` → PASS. Commit: `feat(payments): driver-scoped COD ledger endpoint`

### Task 6: Android — Profile earnings + cash UI

**Files:**
- Modify: `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/service/DriverOpsApiService.kt` (earnings models + `@GET("v1/drivers/me/earnings")`)
- Create: `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/service/PaymentsApiService.kt` (`@GET("v1/cod/driver-ledger/me")` + models)
- Modify: `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/di/NetworkModule.kt` (provide PaymentsApiService)
- Create: `apps/driver-app-android/feature/profile/.../presentation/EarningsViewModel.kt`
- Create: `apps/driver-app-android/feature/profile/.../ui/EarningsScreen.kt`
- Modify: `apps/driver-app-android/feature/profile/.../ui/ProfileScreen.kt` (Earnings summary card + Cash-to-Remit card between identity card and Verification Documents)
- Modify: navigation host that registers ComplianceScreen (grep `ComplianceScreen(` in app/navigation module) — add `earnings` route

- [ ] ProfileScreen additions: gig drivers (from `GET v1/drivers/me` driverType) see Earnings card (Today / This Week, tap → EarningsScreen); ALL drivers see Cash-to-Remit card when `open_balance_cents > 0` (amber glow, "₱X.XX from N COD deliveries").
- [ ] EarningsScreen: two tabs (*Earnings* / *Cash*); Earnings tab gated to gig; entries grouped by day (`AWB · merchant · ₱amount`); Cash tab shows ledger entries (debits red, remittance green) + open balance header. Match existing dark-glass styling constants from ProfileScreen.
- [ ] Commit: `feat(driver-app): earnings & COD cash history in Profile`

### Task 7: dispatch — offers schema + double-grab unique index

**Files:**
- Create: `services/dispatch/migrations/0012_task_offers.sql`

- [ ] Migration: `task_offers` + `task_offer_candidates` per spec (candidates carry `payout_cents BIGINT` and `seen_at TIMESTAMPTZ`), plus indexes `idx_offers_status_expires ON task_offers (status, expires_at)` and `idx_offer_candidates_driver ON task_offer_candidates (driver_id)`. Before the partial unique index, cancel pre-existing duplicate active assignments (keep newest):

```sql
WITH ranked AS (
    SELECT id, ROW_NUMBER() OVER (PARTITION BY driver_id ORDER BY assigned_at DESC) rn
    FROM dispatch.driver_assignments WHERE status IN ('pending','accepted')
)
UPDATE dispatch.driver_assignments a SET status = 'cancelled'
FROM ranked r WHERE a.id = r.id AND r.rn > 1;

CREATE UNIQUE INDEX IF NOT EXISTS idx_one_active_assignment_per_driver
    ON dispatch.driver_assignments (driver_id)
    WHERE status IN ('pending', 'accepted');
```

- [ ] Commit: `feat(dispatch): task_offers schema + one-active-assignment invariant`

### Task 8: dispatch — offer repository + OfferService (broadcast, claim CAS, pass, seen, sweeper logic)

**Files:**
- Create: `services/dispatch/src/infrastructure/db/task_offer_repo.rs`
- Create: `services/dispatch/src/application/services/offer_service.rs`
- Modify: `services/dispatch/src/infrastructure/db/mod.rs`, `services/dispatch/src/application/services/mod.rs`
- Modify: `services/dispatch/src/application/services/driver_assignment_service.rs` (make candidate-pipeline + event-emission reusable: extract `pub(crate) async fn emit_assignment_events(&self, queue_item, assignment, route_id, payout_cents)` from quick_dispatch steps 6–7 and reuse in both paths)

- [ ] Repo key methods (raw SQL, pool-owned):
  - `create_offer(offer_row, candidates)` — insert offer + candidate rows in one tx.
  - `claim(offer_id, driver_id) -> ClaimOutcome` — the heart:

```rust
pub enum ClaimOutcome {
    Won { shipment_id: Uuid, queue_id: Uuid, payout_cents: Option<i64>,
          assignment_id: Uuid, route_id: Uuid,
          all_candidate_ids: Vec<Uuid>, tenant_id: Uuid },
    AlreadyTaken,
    Expired,
    DriverBusy,       // 23505 on idx_one_active_assignment_per_driver
    NotACandidate,
}
```

  Inside one transaction, in order: `SET LOCAL lock_timeout = '250ms'`; verify caller is a candidate (`SELECT payout_cents FROM task_offer_candidates WHERE offer_id=$1 AND driver_id=$2`); CAS `UPDATE task_offers SET status='claimed', claimed_by=$2, claimed_at=now() WHERE id=$1 AND status='open' AND expires_at > now() RETURNING shipment_id, queue_id, tenant_id`; on 0 rows → re-select status to distinguish `AlreadyTaken` vs `Expired`; insert route (Planned, nil vehicle, mirroring quick_dispatch step 4) + assignment (status `'accepted'`, `accepted_at=now()`) — map `23505` on the partial index to `DriverBusy` (tx rolls back, offer stays open); `UPDATE dispatch.dispatch_queue SET status='dispatched', dispatched_at=now() WHERE shipment_id=...`. **No network I/O inside the tx.** Collect `all_candidate_ids` after commit.
  - `mark_seen(offer_id, driver_id)` — `UPDATE task_offer_candidates SET seen_at = COALESCE(seen_at, now()) WHERE ...`.
  - `record_pass(offer_id, driver_id)` — `SET response='passed'`.
  - `find_open_for_driver(driver_id) -> Vec<OpenOfferView>` — open, unexpired, where driver is a candidate and hasn't passed; includes card fields snapshotted on the offer row (store merchant_name, category, weight, tracking, customer, addresses, cod, coords on `task_offers` at creation so this query needs no queue join).
  - `list_expired_open() -> Vec<...>`, `escalate_wave(offer_id, new_wave, new_expires, new_candidates)`, `expire(offer_id) -> Vec<Uuid>` (all candidate ids).
  - `offer_stats_for_driver(driver_id) -> (seen: i64, claimed: i64)` — for acceptance rate.
- [ ] `OfferService::broadcast(tenant_id, shipment_id, wave)` — loads queue item (must be `pending` for wave 1), candidate pipeline identical to quick_dispatch (anchor coords → `find_available_near(radius_for_wave)` → `vehicle_can_carry` → compliance gate) **plus gig filter**: keep only candidates whose `gig_rate_cents()` returns `Some` (part-time); take top 10 by the existing `0.7*distance + 0.3*stops` score; snapshot each candidate's rate; insert offer (TTL 30 s) + candidates; publish `TASK_OFFER_CREATED` after commit. Wave radii: `const WAVE_RADIUS_KM: [f64; 3] = [3.0, 6.0, 10.0];` `const OFFER_TTL_SECS: i64 = 30;` `const OFFER_WAVE_SIZE: usize = 10;` `const OFFER_MAX_WAVES: i32 = 3;`
- [ ] `OfferService::claim(driver_id, offer_id)` — repo claim; on `Won`: re-load queue row, reuse `emit_assignment_events` (TaskAssigned ×2 legs with winner's `payout_cents` + DRIVER_ASSIGNED), publish `TASK_OFFER_CLOSED{reason:"claimed"}`. Map outcomes: `AlreadyTaken|Expired → AppError::BusinessRule("OFFER_TAKEN")`, `DriverBusy → AppError::BusinessRule("DRIVER_BUSY")` (handlers map BusinessRule with these markers to 409).
- [ ] `OfferService::sweep()` — for each expired open offer: wave < 3 → `broadcast` next wave (exclude drivers with `response='passed'` and all prior candidates of this offer; on zero new candidates fall through to expiry); wave ≥ 3 or no candidates → `expire`, `record_failed_attempt(shipment_id, "gig broadcast expired unclaimed after N waves")`, publish `TASK_OFFER_CLOSED{reason:"expired"}`.
- [ ] Unit tests in offer_service.rs: wave-radius table, outcome→error mapping. `cargo check -p dispatch` → PASS. Commit: `feat(dispatch): offer broadcast + atomic claim service`

### Task 9: dispatch — HTTP surface + sweeper task + gateway route

**Files:**
- Create: `services/dispatch/src/api/http/offers.rs`
- Modify: `services/dispatch/src/api/http/mod.rs` (AppState += `offer_service`; routes)
- Modify: `services/dispatch/src/bootstrap.rs` (construct OfferService; spawn sweeper interval)
- Modify: `services/api-gateway/src/proxy/mod.rs` (add `/v1/offers` → dispatch_url)

- [ ] Routes (driver JWT, inside protected_router):

```rust
.route("/offers/open",       get(offers::list_open))
.route("/offers/:id/claim",  post(offers::claim))
.route("/offers/:id/pass",   post(offers::pass))
.route("/offers/:id/seen",   post(offers::seen))
// Ops console action — broadcast instead of 1:1 quick dispatch
.route("/queue/:shipment_id/broadcast", post(offers::broadcast))
```

- [ ] Claim handler returns `200 {data:{assignment_id, shipment_id}}` on win; 409 `{"error":"OFFER_TAKEN"}` / `{"error":"DRIVER_BUSY"}` otherwise. `seen`/`pass` are 204 fire-and-forget.
- [ ] Sweeper in bootstrap (pattern: existing orphan-cleanup task): `tokio::spawn` loop, `interval(Duration::from_secs(10))`, call `offer_service.sweep()`, log-and-continue on error.
- [ ] Gateway: add to the dispatch branch `|| path.starts_with("/v1/offers")`.
- [ ] `cargo check -p dispatch -p api-gateway` → PASS. Commit: `feat(dispatch): offer endpoints, broadcast action, expiry sweeper`

### Task 10: driver-ops — offer FCM fan-out consumer + acceptance rate on /drivers/me

**Files:**
- Create: `services/driver-ops/src/infrastructure/messaging/offer_consumer.rs`
- Modify: `services/driver-ops/src/infrastructure/messaging/mod.rs`, `services/driver-ops/src/bootstrap.rs` (spawn consumer)
- Modify: `services/driver-ops/src/infrastructure/external/mod.rs` (FcmClient: generic data push `notify_data(user_id, data: &HashMap<String,String>)` if only typed `notify_task_assigned` exists)
- Modify: `services/driver-ops/src/api/http/drivers.rs` (`get_me_driver` += `offers_seen`, `offers_claimed` via cross-schema read of `dispatch.task_offer_candidates`/`task_offers` — same-instance precedent as dispatch reading `driver_ops.drivers`; wrap in `unwrap_or((0,0))` so a missing dispatch schema never breaks the profile)

- [ ] Consumer subscribes `TASK_OFFER_CREATED` + `TASK_OFFER_CLOSED` (group `{group_id}-offers`, same shutdown/commit pattern as task_consumer):
  - Created → per candidate: FCM data push `type="task_offer"` with `offer_id, shipment_id, expires_at_ms, payout_cents (candidate's own, empty when None), merchant_name, delivery_category, weight_grams, tracking_number, customer_name, pickup/delivery addresses + coords, cod_amount_cents`.
  - Closed → per candidate except `claimed_by`: `type="offer_closed"`, `offer_id`, `reason`.
- [ ] `cargo check -p driver-ops` → PASS. Commit: `feat(driver-ops): offer FCM fan-out + gig acceptance stats`

### Task 11: Android — grab card flow

**Files:**
- Modify: `core/common/.../PendingAssignmentBus.kt` — `AssignmentPayload` += `offerId: String = ""`, `payoutCents: Long? = null`, `expiresAtMillis: Long? = null`; bus += `markTaken(offerId)` (sets a `takenOfferId` StateFlow) and `clearIfOffer(offerId)`
- Modify: `feature/notifications/.../DriverMessagingService.kt` — handle `"task_offer"` (post payload with offerId/payout/expiry) and `"offer_closed"` (`PendingAssignmentBus.markTaken(offerId)`)
- Modify: `core/network/.../DriverOpsApiService.kt` — offers API (gateway-routed): `@GET("v1/offers/open")`, `@POST("v1/offers/{id}/claim")`, `@POST("v1/offers/{id}/pass")`, `@POST("v1/offers/{id}/seen")` + `OpenOffersResponse` models mirroring backend
- Modify: `feature/home/.../presentation/HomeViewModel.kt` — `claimOffer()` (on HTTP 409 → `offerTaken=true` state, auto-dismiss after ~1.5 s), `passOffer()` (no decline-counter bump — passes are penalty-free), `reportSeen()`, restore open offers on init for gig drivers via `GET /v1/offers/open`, 1 s countdown ticker, expiry auto-dismiss
- Modify: `feature/home/.../ui/TaskCards.kt` — offer card in grab mode when `offerId` non-blank: countdown ring (sweep arc, amber < 10 s), prominent payout chip, full-width **GRAB** button (green/glow) + quiet "Pass" text button; "Taken" overlay state (red flash → dismiss); fire `reportSeen()` once via `LaunchedEffect(offerId)`
- Modify: performance strip in TaskCards.kt — gig: show `Acceptance n%` (from offers_seen/claimed on profile) instead of `Declines n/20`; full-time strip unchanged
- Modify: `core/network/.../DriverOpsApiService.kt` `DriverProfileData` += `offersSeen`/`offersClaimed` (serde defaults 0)

- [ ] 1:1 `task_assigned` flow (full-time / targeted) is untouched — Accept/Decline card renders exactly as today when `offerId` is blank.
- [ ] Commit: `feat(driver-app): gig offer grab card with countdown + atomic claim UX`

### Task 12: Admin portal — Broadcast button on dispatch console

**Files:**
- Modify: admin-portal dispatch console queue actions (grep `cancel-dispatch` or `quick_dispatch` usage under `apps/admin-portal/src`)

- [ ] Add "Broadcast to gig drivers" action beside Quick Dispatch on pending queue rows → `POST /v1/queue/{shipmentId}/broadcast`; toast on success/failure. Keep it minimal — same fetch helper/pattern as the existing dispatch button.
- [ ] Commit: `feat(admin-portal): broadcast-to-gig action on dispatch queue`

### Task 13: Verification + integration

- [ ] `$env:CARGO_INCREMENTAL='0'; cargo check -p logisticos-events -p dispatch -p driver-ops -p payments -p api-gateway` → all PASS
- [ ] `cargo test -p dispatch --lib`, `cargo test -p driver-ops --lib`, `cargo test -p payments --lib` → PASS
- [ ] `npx tsc --noEmit` in admin-portal if it has a typecheck script
- [ ] Final commit, then: `git pull --rebase origin master` (or merge per repo flow), push, confirm CI (Android workflow + Rust checks) goes green.
