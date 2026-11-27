# Marketplace Extraction, Vehicle Calendar, Rental and On-Demand — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the vehicle marketplace out of `services/carrier` into its own service. Then give every vehicle one calendar the database protects against double-booking, and add rental as a listing type. On-Demand services become their own product, with workers from field-ops.

**Architecture:**
- **Extraction moves behaviour, it does not change it.** The `carrier.vehicle_listings` and `carrier.marketplace_bookings` code and data move into `services/marketplace` (schema `marketplace`, port 8018), and the gateway's `/v1/marketplace` route switches over.
- **A calendar table** `marketplace.vehicle_reservations`, with a `btree_gist` exclusion constraint, is then written by every booking and, later, every rental.
- **On-Demand** gets a plan of its own once its prerequisites are in. This plan only fixes the decisions it must inherit.

**Tech Stack:** Rust (axum, sqlx, rdkafka via `libs/events`), PostgreSQL with `btree_gist`, Next.js portals.

**Spec:** [ADR-0014](../../adr/0014-marketplace-backend-service.md), specifically its **2026-11-25 amendment**, which restates the 2026-09-19 decisions that were never committed. The amendment is Proposed, and Phase 0 Task 0.1 gets it confirmed.

## Phase map

| Phase | Delivers | Plan detail | Blocked by |
|---|---|---|---|
| 0 | ADR confirmation + prerequisites P1–P4 answered | tasks below | — |
| 1 | `services/marketplace` serving today's behaviour; carrier's marketplace code retired | tasks below | 0.1, merge of `feat/portals/marketplace-real-backend` and `feat/carrier/marketplace-merchant-cancel` |
| 2 | Listings on `fleet.vehicles.id` + vehicle-class registry + vehicle calendar + Dead-Man's Switch | tasks below | P2, P4, Phase 1 |
| 3 | Rental listing type | **outline**; write its own plan once P3 is answered | Phase 2, P3 |
| 4 | On-Demand Services product | **outline**; write its own plan | P1, P4, ADR-0009/0015 pointer amendments |

## Global Constraints

- **Service shape.**
  - The new service is `services/marketplace`, crate `logisticos-marketplace`, port `8018`, database `svc_marketplace`, schema `marketplace`.
  - Per-schema `_sqlx_migrations` (ADR-0012). It exposes `/health`, `/ready` and `/metrics`, with observability routes mounted **outside** the auth layer.
- **Phase 1 changes nothing observable.**
  - Same routes, request and response shapes, status codes, Kafka topics and payloads.
  - Same payment authorize → capture → void sequence and same response-window sweep.
  - **Any behaviour change found necessary is a separate task with its own review.**
- **Kafka.** Topic constants keep their current names (`MARKETPLACE_BOOKING_REQUESTED`, `_ACCEPTED`, `_REJECTED`, and any others carrier publishes). The producer `source` changes from `carrier` to `marketplace`. Consumers must not key on `source`; Task 1.1 verifies this.
- **No closed sets on platform boundaries.** `listing_type`, `size_class`/vehicle class and reservation `source` are validated in application code against data, never by a `CHECK (… IN (…))`.
- **Rust gates.** Run with `CARGO_INCREMENTAL=0`: `cargo check -p`, `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p`. Zero `unwrap()` outside tests. DB tests fail, not skip, when their database variable is missing in CI.
- **New topics.** Add them to `libs/events/src/topics.rs` **and** both topic-creation scripts, and pre-create them on the VPS before deploy.
- **Production rule.** No production data move without the architect's go-ahead on the day. Phase 1's copy step is a runbook step, not a migration.

## Review Focus

1. **A booking created in carrier during the cutover window.** Expected: none can be. Carrier's marketplace write routes return `503` with a stable message once the cutover flag is on, so nothing is written to a table that is no longer the source of truth. (Task 1.4)
2. **A response-window or expiry sweep running in both services at once** during the overlap. Expected: carrier's sweeps stop when its marketplace flag is off. Only one service ever voids a payment hold. (Task 1.4)
3. **Two bookings for the same vehicle with touching windows,** for example 09:00–12:00 and 12:00–15:00. Expected: both succeed, because the ranges are half-open `[)`. Overlapping windows: the second gets `409` with the message `Vehicle already reserved for an overlapping time`. (Task 2.3)
4. **A booking rejected, cancelled or expired after its reservation was written.** Expected: the reservation is released (`released_at` set) in the same transaction as the status change, and the slot can be booked again at once. (Task 2.3)
5. **A legacy listing with no `vehicle_id`.** Expected: it stays browsable and bookable with today's behaviour until its window ends. It writes no reservation, which is logged. New listings require `vehicle_id`. (Tasks 2.2 and 2.3)

---

## Phase 0 — Confirm and unblock

### Task 0.1: Confirm the ADR-0014 amendment and record the prerequisite answers

**Files:** Modify `docs/adr/0014-marketplace-backend-service.md` (amendment status and prerequisite table).

- [ ] **Step 1: Present the amendment to the deciders.** Its five decisions are: extract first, rental as a listing type, one pattern with two owners, On-Demand as its own product, and listings on fleet ids. Record each as confirmed or changed.
- [ ] **Step 2: Record the P1–P4 answers.**
  - **P1:** is the claim-reclaim sweep live? Name the binary and the bootstrap line.
  - **P2:** owner and target date for the vehicle-class registry.
  - **P3:** NI `AUTH` hold validity in days, and whether `void` is confirmed against NI.
  - **P4:** the result of `SELECT * FROM pg_available_extensions WHERE name = 'btree_gist'` on every environment's Postgres.
- [ ] **Step 3: Commit** `docs(adr): ADR-0014 amendment confirmed; prerequisite answers`.

---

## Phase 1 — Extract `services/marketplace` (behaviour-preserving)

### Task 1.1: Scaffold the service and move the domain

**Files:**
- Create: `services/marketplace/{Cargo.toml,Dockerfile,src/main.rs,src/bootstrap.rs,src/config.rs}`, `services/marketplace/src/domain/entities/mod.rs`, `services/marketplace/src/application/services/marketplace.rs`, `services/marketplace/src/infrastructure/{db,payments_client.rs,messaging.rs,booking_payment_consumer.rs}`, `services/marketplace/migrations/0001_marketplace_tables.sql`
- Modify: the workspace `Cargo.toml` members; `.github/workflows/build-images.yml` (the service list); the compose file used by the VPS deploy (new service on 8018 with `svc_marketplace`)

**Interfaces — Produces:**
- The crate keeps carrier's public names for the moved types (`VehicleListing`, `MarketplaceBooking`, `SizeClass`, `BookingStatus`, the payments trait with `authorize`/`capture`/`void`), so the moved tests compile unchanged.
- The migration `0001` is the union of `carrier` migrations 0004 and 0007 for these two tables, in schema `marketplace`, with three changes:
  1. **No FK to `carrier.carriers`.** The cross-database FK is impossible; `carrier_id` stays as a `UUID NOT NULL` column.
  2. **`listing_type TEXT NOT NULL DEFAULT 'freight_spot'`** on listings, unused until Phase 3.
  3. **`vehicle_id UUID NULL`** on listings, unused until Phase 2.
- **Carrier status read.** Wherever carrier's code reads `carrier.carriers` directly for a marketplace decision (caller → carrier resolution, carrier active), the marketplace service calls carrier over HTTP instead. Use the existing caller-resolution route (`GET /v1/carriers/me`) and an internal `GET /v1/internal/carriers/:id/status` (add it to carrier if absent, guarded the way carrier's other internal routes are). Cache the result for 60 s.

- [ ] **Step 1: Copy carrier's marketplace unit tests** into the new crate unchanged. Run `cargo test -p logisticos-marketplace`; it fails (nothing implemented yet).
- [ ] **Step 2: Move the code.** Replace direct `carrier.carriers` reads with the carrier client. List every such replacement in the commit message body.
- [ ] **Step 3: Verify Kafka consumers ignore `source`.** For each marketplace topic, grep consumers (`grep -rn "MARKETPLACE_BOOKING" services/*/src`) and confirm none matches on `source == "carrier"`. Record the result in the commit body.
- [ ] **Step 4: Run the gates on the new crate.** Expected: all pass, with the moved tests green.
- [ ] **Step 5: Commit** `feat(marketplace): extract the marketplace service from carrier (behaviour-preserving)`.

### Task 1.2: Route parity tests

**Files:**
- Create: `services/marketplace/tests/integration/routes_parity.rs`

- [ ] **Step 1: Write the integration tests** against a real `svc_marketplace` test database, one per route carrier serves today. Each asserts status code and response JSON keys:
  - `GET /v1/marketplace/listings`, `POST /v1/marketplace/listings`, `PUT` and `DELETE /v1/marketplace/listings/:id`
  - `GET /v1/marketplace/available`
  - `POST /v1/marketplace/bookings`, `GET /v1/marketplace/my-bookings`, `GET /v1/marketplace/bookings`
  - `POST /v1/marketplace/bookings/:id/{accept,reject,pickup,cancel}`

  The expected shapes are copied from carrier's handler return types, not invented.
- [ ] **Step 2: Run them.** Expected: PASS. A failure is a parity bug: fix it in the moved code, never in the test's expectation.
- [ ] **Step 3: Commit** `test(marketplace): route parity with carrier's marketplace surface`.

### Task 1.3: Gateway switch behind a flag

**Files:**
- Modify: `services/api-gateway/src/proxy/mod.rs` (`resolve_upstream`, tests), gateway config

**Interfaces — Produces:** config `MARKETPLACE_URL` and `ROUTE_MARKETPLACE_TO_SERVICE` (default `false`). When it is true, `/v1/marketplace` resolves to `MARKETPLACE_URL`; otherwise it goes to carrier as today.

- [ ] **Step 1: Write the tests:** `marketplace_routes_to_carrier_when_flag_off` and `marketplace_routes_to_service_when_flag_on`.
- [ ] **Step 2: Confirm they fail. Step 3: Implement. Step 4: Confirm they pass and run the gateway gates.**
- [ ] **Step 5: Commit** `feat(api-gateway): route /v1/marketplace to the marketplace service behind a flag`.

### Task 1.4: Freeze carrier's marketplace writes and sweeps; cutover runbook

**Files:**
- Modify: `services/carrier/src/bootstrap.rs` (sweeps), carrier's marketplace HTTP module
- Create: `docs/runbooks/marketplace-cutover.md`

**Interfaces — Produces:** carrier config `CARRIER_MARKETPLACE_ENABLED` (default `true`). When `false`:
- Every marketplace **write** route returns `503 {"error":"Marketplace is moving; try again in a few minutes"}`. Read routes still answer.
- The response-window and expiry sweeps and the booking-payment consumer do not start.

**The runbook,** in order, each step with its verification command:
1. Set `CARRIER_MARKETPLACE_ENABLED=false`, then confirm a `POST /v1/marketplace/bookings` via carrier gets `503`.
2. Copy the rows: `pg_dump --data-only -t carrier.vehicle_listings -t carrier.marketplace_bookings svc_carrier`, then rewrite the schema name and load into `svc_marketplace`. Verify row counts and `max(updated_at)` match.
3. Start the marketplace service with its sweeps.
4. Set `ROUTE_MARKETPLACE_TO_SERVICE=true`, then confirm `GET /v1/marketplace/my-bookings` returns the copied rows.
5. **Rollback:** reverse steps 4 and 1. No data was deleted in carrier.

- [ ] **Step 1: Write the tests:** `writes_return_503_when_disabled`, `reads_still_answer_when_disabled`, and `sweeps_not_spawned_when_disabled` (assert on the bootstrap decision function).
- [ ] **Step 2: Confirm they fail. Step 3: Implement and write the runbook. Step 4: Run the carrier gates.**
- [ ] **Step 5: Commit** `feat(carrier): freeze switch for the marketplace cutover; runbook`.

### Task 1.5: Retire carrier's marketplace code (one release after cutover)

**Files:**
- Modify: carrier's marketplace modules (delete them), `services/carrier/Cargo.toml` (deps only the marketplace used)
- Create: `services/carrier/migrations/00NN_drop_marketplace_tables.sql`, **written but held**

- [ ] **Step 1: Delete the code.** The carrier gates pass, and `grep -rn "marketplace" services/carrier/src` returns only the internal carrier-status route from Task 1.1.
- [ ] **Step 2: Write the drop migration but do not merge it** until the architect confirms the rollback window is over. Record the hold in the PR description.
- [ ] **Step 3: Commit** `refactor(carrier): remove the marketplace now served by services/marketplace`.

---

## Phase 2 — Vehicle identity, one calendar, Dead-Man's Switch

### Task 2.1: Fleet vehicle-class registry (prerequisite P2)

**Files:**
- Create: `services/fleet/migrations/0004_vehicle_classes.sql`, fleet domain + repo + `GET /v1/fleet/vehicle-classes`
- Modify: the fleet vehicle create/update path

**Interfaces — Produces:**
- **Table** `fleet.vehicle_classes (id UUID PK, tenant_id UUID NULL, code TEXT NOT NULL, label TEXT NOT NULL, max_weight_kg NUMERIC(10,2) NOT NULL, max_volume_m3 NUMERIC(10,2) NULL, active BOOL NOT NULL DEFAULT true)`. A NULL `tenant_id` marks a platform default. It has `UNIQUE (COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'), code)`.
- **Seeded platform codes:** `motorcycle`, `sedan`, `van`, `l300`, `6wheeler`, `10wheeler`, `trailer`.
- **`fleet.vehicles.vehicle_class_id UUID NULL`.** The backfill sets it where `lower(vehicle_type)` equals a platform code and leaves the rest NULL. Report the counts.
- **`GET /v1/fleet/vehicle-classes`** returns the platform defaults plus the caller tenant's classes.

- [ ] **Step 1: Write the tests:** `tenant_sees_platform_and_own_classes_not_others`, `backfill_maps_matching_vehicle_type_only` (DB test), and `duplicate_code_within_tenant_rejected`.
- [ ] **Step 2: Confirm they fail. Step 3: Implement. Step 4: Run the fleet gates.**
- [ ] **Step 5: Commit** `feat(fleet): tenant-extensible vehicle-class registry`.

### Task 2.2: Listings reference a fleet vehicle

**Files:**
- Modify: the marketplace listing create/update handler and service; partner-portal listing form (`apps/partner-portal/.../marketplace`)

**Interfaces — Produces:**
- `POST /v1/marketplace/listings` requires `vehicle_id`.
  - The service fetches the vehicle from fleet: it must belong to the tenant and have a `vehicle_class_id`.
  - It sets `size_class` from the class `code` and `max_weight_kg` from the class unless the request supplies a lower value. It keeps `vehicle_plate` from fleet for display.
  - An unknown vehicle, a foreign vehicle, or a vehicle with no class → `422` with a message naming the problem.
- **Legacy rows** with NULL `vehicle_id` stay valid (Review Focus 5).
- **The partner portal** listing form picks from the carrier's fleet vehicles instead of typing a plate.

- [ ] **Step 1: Write the tests:** `create_listing_requires_vehicle_id`, `create_listing_rejects_foreign_vehicle`, `size_class_comes_from_vehicle_class`, and `legacy_listing_without_vehicle_still_bookable`. Run the partner-portal gates.
- [ ] **Step 2: Confirm they fail. Step 3: Implement. Step 4: Run the gates.**
- [ ] **Step 5: Commit** `feat(marketplace): listings reference fleet vehicles and their class`.

### Task 2.3: The vehicle calendar

**Files:**
- Create: `services/marketplace/migrations/0002_vehicle_reservations.sql`, `services/marketplace/src/infrastructure/db/reservations.rs`
- Modify: the booking accept, reject, cancel and expire paths in `application/services/marketplace.rs`

**Migration:**
```sql
CREATE EXTENSION IF NOT EXISTS btree_gist;
CREATE TABLE IF NOT EXISTS marketplace.vehicle_reservations (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id   UUID NOT NULL,
  vehicle_id  UUID NOT NULL,
  during      TSTZRANGE NOT NULL,
  source      TEXT NOT NULL,          -- 'freight_booking' now, 'rental' in Phase 3; validated in code
  ref_id      UUID NOT NULL,          -- booking id
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  released_at TIMESTAMPTZ NULL,
  CONSTRAINT no_double_booking EXCLUDE USING gist
    (vehicle_id WITH =, during WITH &&) WHERE (released_at IS NULL)
);
CREATE INDEX IF NOT EXISTS reservations_ref ON marketplace.vehicle_reservations (ref_id);
```

**Interfaces — Produces:**
- `reserve(tx, tenant_id, vehicle_id, during: (DateTime<Utc>, DateTime<Utc>), source, ref_id) -> Result<Uuid, ReservationError>`. It maps SQLSTATE `23P01` (exclusion violation) to `ReservationError::Overlap`, which the HTTP layer returns as `409` with `Vehicle already reserved for an overlapping time`.
- `release_by_ref(tx, ref_id) -> Result<u64, sqlx::Error>`
- **Freight reservation window:** `[pickup_at, min(listing.idle_until, pickup_at + MARKETPLACE__FREIGHT_HOLD_HOURS))`, with `MARKETPLACE__FREIGHT_HOLD_HOURS` defaulting to `4`. It is written **on accept**, in the same transaction that flips the booking to `accepted`.
- **Release** happens on reject, cancel, expire and delivered, each in the same transaction as the status change. A booking whose listing has NULL `vehicle_id` reserves nothing and logs `info`.

- [ ] **Step 1: Write the DB tests:**
  - `touching_windows_both_accept` (Review Focus 3)
  - `overlapping_accept_is_409`
  - `released_slot_is_bookable_again` (Review Focus 4)
  - `reject_releases_in_same_tx`: with a forced failure after the status update, neither change persists.
  - `legacy_listing_reserves_nothing`
- [ ] **Step 2: Confirm they fail. Step 3: Implement. Step 4: Run the gates.**
- [ ] **Step 5: Commit** `feat(marketplace): one vehicle calendar with a database-enforced no-overlap rule`.

### Task 2.4: Dead-Man's Switch

**Files:**
- Create: `services/marketplace/src/application/services/dms.rs`, `services/marketplace/migrations/0003_listing_suspension.sql`
- Modify: bootstrap (60 s tick), the accept path, and the browse query

**Interfaces — Produces:**
- Columns `suspended_reason TEXT NULL` and `suspended_at TIMESTAMPTZ NULL` on listings. The listing status gains the value `suspended`, as data, matching however carrier stored statuses.
- A pure `fn dms_verdict(input: &DmsInput, now: DateTime<Utc>) -> Option<&'static str>` returning the ADR's reasons, checked in this order:
  1. `window_expired`
  2. `carrier_suspended`
  3. `vehicle_not_operational` (fleet status `maintenance` or `damaged`)
  4. `no_online_driver` (no driver of the carrier `available` with a ping in the last 300 s)
  5. `vehicle_reserved`: an active reservation overlapping the listing window, for a listing not yet matched.
- **The tick** suspends and un-suspends: a listing whose verdict clears returns to `active`. Browse filters out `suspended`; the admin list shows it with its reason. **Accept re-evaluates `dms_verdict` live** and refuses with `409` and the reason.
- **Inputs** come from driver-ops (driver availability + last ping), fleet (vehicle status) and carrier (status), each cached for 60 s.

- [ ] **Step 1: Write the tests.**
  - **Unit:** one test per reason, plus `first_failing_reason_wins` and `healthy_listing_has_no_verdict`.
  - **Integration:** `accept_refused_when_driver_went_offline`.
- [ ] **Step 2: Confirm they fail. Step 3: Implement. Step 4: Run the gates.**
- [ ] **Step 5: Commit** `feat(marketplace): dead-man's switch suspends listings that cannot be honoured`.

---

## Phase 3 — Rental listing type (outline; gets its own plan)

Decisions its plan must carry, from the amendment:
- **No separate rental table.** A rental is a listing with `listing_type = 'rental'` and rental pricing fields (`daily_rate_cents`, `min_days`, `deposit_cents`).
- **A rental booking reserves at creation,** for `[start, end)` with `source = 'rental'`, not at carrier accept. Freight and rental therefore contend in the same exclusion constraint. The reservation is released on carrier reject or response-window timeout.
- **The payment shape depends on P3:**
  - **start ≤ hold validity:** an `AUTH` hold at booking.
  - **start beyond it:** the booking is created `pending_payment`, and a scheduled authorization at `start − hold_validity` captures or releases it. A failed authorization releases the reservation and notifies the merchant.
  - The plan cannot be written until P3 gives the number of days and confirms `void`.
- **Merchant and partner portals** get a rental tab on the same pages, not a new page tree.

## Phase 4 — On-Demand Services product (outline; gets its own plan)

Decisions its plan must carry, from the amendment:
- **`services/on-demand`** consolidates ADR-0009's Carwash and Maintenance. The service type is data (a catalog table), and the product host is per ADR-0009's product-gateway rule.
- **Workers come from field-ops.** Add `field_ops.worker_reservations` with the same `EXCLUDE USING gist (courier_id WITH =, during WITH &&) WHERE (released_at IS NULL)` shape, plus skills and shifts in field-ops. On-Demand never stores worker availability.
- **Assignment uses field-ops' atomic claim** (ADR-0015 invariant: one claim per courier). It depends on P1: a crashed client must not hold a worker forever.
- **Pointer amendments to ADR-0009 and ADR-0015** are written and confirmed before the plan starts.
