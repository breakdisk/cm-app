# Neighborhood Network Phase 1: Enrollment and Shadow Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enroll OmniDeliv vendors as store partners, draw territories, and run the sort resolver on real parcels without changing anything physical.

**Architecture:** A vendor's opt-in event creates a linked carrier and store profile. Hub-ops owns versioned PostGIS territories and a pure resolver. Shadow mode records what the resolver would have done.

**Tech Stack:** Rust, Axum, SQLx, PostGIS, Kafka, Next.js 14, Mapbox GL, Terra Draw.

**Spec:** [Neighborhood Delivery Network](../specs/2026-11-23-neighborhood-delivery-network-design.md), section 7 (Enrollment, Carrier, Territories, Sort resolver). **Program plan:** [index](2026-11-23-neighborhood-delivery-network.md).

## Global Constraints

- **Entry condition:** the Phase 0 gate has passed.
- Next migrations: carrier `0008`, omnideliv `0030`, hub-ops `0014`.
- `boundary` is `geometry(MultiPolygon, 4326)`, never `geography`.
- Published territories in a hub must not overlap in area: `ST_Relate(a, b, '2********')` inside a per-hub advisory lock. Shared edges are allowed.
- A point on a shared edge resolves to the lowest label.
- Vendor, carrier and hub must share a tenant.
- `origin_product` and `origin_ref` are opaque. Carrier never imports an OmniDeliv type.
- Topics must be in both `scripts/kafka/create-topics.sh` and `scripts/create-kafka-topics.sh`, and verified on the broker before a consumer depends on them.
- Editor packages: `terra-draw@1.40.0`, `terra-draw-mapbox-gl-adapter@1.5.0`; raise `mapbox-gl` to `^3.9.2`.
- Shadow mode makes **no** physical or status change and no dispatch call.

## Review Focus

1. A vendor double-taps opt-in, or the event replays. Exactly one carrier must exist (Task 1.4).
2. A drawn polygon is self-intersecting or crosses the antimeridian. The API returns a 422, not a 500 from the `ST_IsValid` CHECK (Task 1.6).
3. Two admins publish overlapping territories at the same moment. One must fail (Task 1.7).
4. A parcel sits exactly on a shared edge. It must resolve the same way every time (Task 1.7).
5. Order-intake is down during shadow evaluation. The real sort must still succeed (Task 1.11).

---

### Task 1.1: Contracts, topics and gateway routes

**Files:**
- Modify: `docs/api/openapi/carrier.yaml`, `hub-ops.yaml`, `scripts/openapi-tracked-prefixes.txt`
- Modify: `libs/events/src/topics.rs`, `scripts/kafka/create-topics.sh`, `scripts/create-kafka-topics.sh`
- Modify: `services/api-gateway/src/proxy/mod.rs:23`

**Interfaces:**
- Produces: `topics::OMNIDELIV_VENDOR_PARTNER_REQUESTED`, `topics::PARTNER_STORE_UPDATED`.

- [ ] **Step 1: Write failing tests** in the gateway's inline test module: `/v1/store-partners` routes to carrier, `/v1/territories` to hub-ops, and `/v1/internal/partner-memberships` is refused.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-api-gateway --lib` → FAIL.
- [ ] **Step 3: Implement.** Add the contracts, topic constants, both script entries and gateway prefixes. Add the new topics to the `ALL` list test in `topics.rs`.
- [ ] **Step 4: Confirm they pass.** Same command, `cargo test -p logisticos-events`, `bash scripts/check-kafka-topics.sh`, `python scripts/check-openapi-routes.py` → all pass.
- [ ] **Step 5: Commit.** `feat(network): phase 1 contracts, topics and gateway routes`

---

### Task 1.2: Store partner profiles and rates

**Files:**
- Create: `services/carrier/migrations/0008_store_partners.sql`
- Create: `services/carrier/src/domain/entities/store_partner.rs`
- Create: `services/carrier/src/infrastructure/db/store_partner_repo.rs`
- Test: `services/carrier/tests/store_partner_db.rs`

**Interfaces:**
- Produces:

```rust
pub enum StorePartnerStatus { Applied, Active, Paused, Suspended, Offboarded }
pub trait StorePartnerRepository {
    async fn find(&self, tenant: Uuid, carrier: Uuid) -> anyhow::Result<Option<StorePartner>>;
    async fn create_with_carrier(&self, new: &NewStorePartner) -> anyhow::Result<(StorePartner, bool)>; // bool = created
    async fn set_status(&self, tenant: Uuid, carrier: Uuid, to: StorePartnerStatus) -> anyhow::Result<StorePartner>;
    async fn add_rate(&self, rate: &NewStoreRate) -> anyhow::Result<StoreRate>;
    async fn effective_rate(&self, tenant: Uuid, carrier: Uuid, territory: Option<Uuid>, at: DateTime<Utc>) -> anyhow::Result<Option<StoreRate>>;
}
```

- [ ] **Step 1: Write failing tests:** `same_origin_creates_one_profile`, `territory_rate_beats_store_default`, `rate_change_adds_a_row`, `effective_rate_uses_the_rate_at_that_time`, `offboarded_cannot_return_to_active`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-carrier --test store_partner_db` → FAIL.
- [ ] **Step 3: Implement** the spec's two tables, plus `REVOKE UPDATE, DELETE` on rates for the app role.
- [ ] **Step 4: Confirm they pass.** Same command → PASS, non-zero duration.
- [ ] **Step 5: Commit.** `feat(carrier): store partner profiles and append-only rates`

---

### Task 1.3: Vendor opt-in

**Files:**
- Create: `services/omnideliv/migrations/0030_vendor_partner_requests.sql`
- Create: `services/omnideliv/src/api/http/vendor_partner.rs`
- Modify: `services/omnideliv/src/api/http/mod.rs`
- Test: `services/omnideliv/tests/vendor_partner_request.rs`

**Interfaces:**
- Produces: `POST /v1/omnideliv/vendors/me/delivery-partner` and `GET` of the same path.
- Produces event `VendorPartnerRequested { tenant_id, vendor_id, name, contact_email, contact_phone, lat, lng, requested_by, requested_at }`.

- [ ] **Step 1: Write failing tests:** `owner_can_opt_in`, `non_owner_gets_403`, `second_request_returns_the_first`, `vendor_without_coordinates_is_refused`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-omnideliv --test vendor_partner_request` → FAIL.
- [ ] **Step 3: Implement.** Write the request row and publish the event. A republish sweep handles rows with `published_at IS NULL`, because a failed publish must not lose an opt-in.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(omnideliv): vendors can apply to be delivery partners`

---

### Task 1.4: Enrollment consumer

**Files:**
- Create: `services/carrier/src/infrastructure/store_enrollment_consumer.rs`
- Modify: `services/carrier/src/bootstrap.rs`
- Test: `services/carrier/tests/store_partner_db.rs`

**Interfaces:**
- Consumes: `VendorPartnerRequested`, `StorePartnerRepository::create_with_carrier`.
- Produces: a `Carrier` in `pending_verification` plus a store profile in `applied`, with `origin_product = "omnideliv"`, `origin_ref = vendor_id`.

- [ ] **Step 1: Write failing tests:** `event_creates_carrier_and_profile`, `replay_creates_nothing`, `contact_email_is_stored_lowercase`, `carrier_and_profile_commit_together`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-carrier --test store_partner_db enrollment` → FAIL.
- [ ] **Step 3: Implement** with the retry-in-place shape from dispatch's `job_dropped_consumer.rs`.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(carrier): enroll a vendor from its opt-in event`

---

### Task 1.5: Approval, membership and store status

**Files:**
- Modify: `services/carrier/src/application/services/mod.rs:116`, `src/api/http/mod.rs`
- Create: `services/carrier/src/infrastructure/identity_client.rs`, in the shape of `payments_client.rs`

**Interfaces:**
- Consumes: `POST /v1/internal/partner-memberships`.
- Produces: `GET /v1/store-partners`, `GET /v1/store-partners/me`, `PATCH /v1/store-partners/{id}` (admin); event `PartnerStoreUpdated { tenant_id, carrier_id, status, lat, lng, daily_capacity, max_weight_g, max_longest_cm, cod_enabled, cod_cap_cents, operating_hours, rates: Vec<StoreRateRef>, version }`, where `StoreRateRef { id, territory_id, delivery_fee_cents, failed_attempt_fee_cents, currency, effective_from }`. Carrying rates here lets a sort proceed without a synchronous call to carrier.

- [ ] **Step 1: Write failing tests:** `approving_grants_store_manager`, `identity_failure_leaves_the_store_applied`, `retry_after_failure_succeeds`, `me_uses_pid_not_email`, `store_token_cannot_patch`, `every_status_change_publishes_with_a_higher_version`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-carrier --test store_partner_db approval` → FAIL.
- [ ] **Step 3: Implement.** Order: create the membership, then activate, then publish. The membership call is idempotent, so a retry is safe.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(carrier): approve a store and grant its manager access`

---

### Task 1.6: Territories

**Files:**
- Create: `services/hub-ops/migrations/0014_delivery_territories.sql`
- Create: `services/hub-ops/src/domain/entities/territory.rs`
- Create: `services/hub-ops/src/infrastructure/db/territory_repo.rs`
- Create: `services/hub-ops/src/api/http/territories.rs`
- Test: `services/hub-ops/tests/territory_db.rs`

**Interfaces:**
- Produces: `GET, POST /v1/territories`, `TerritoryRepository::{create_draft, find, list_for_hub, new_version_from}`.

The migration starts with `CREATE EXTENSION IF NOT EXISTS postgis;`. Test it on a disposable `postgis/postgis:16-3.4` database, and confirm the app role can create or use the extension before deployment.

- [ ] **Step 1: Write failing tests:** `polygon_is_stored_as_multipolygon`, `self_intersecting_returns_422`, `out_of_range_coordinates_return_422`, `antimeridian_crossing_returns_422`, `editing_published_creates_a_new_version`, `old_version_is_unchanged`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test territory_db` → FAIL.
- [ ] **Step 3: Implement.** Validate with `ST_IsValid(ST_GeomFromGeoJSON($1))` before insert and store with `ST_Multi`. Limit the request body to 1 MB and 5,000 vertices.
- [ ] **Step 4: Confirm they pass.** Same command → PASS, non-zero duration.
- [ ] **Step 5: Commit.** `feat(hub-ops): versioned PostGIS delivery territories`

---

### Task 1.7: Publish and point lookup

**Files:**
- Modify: `services/hub-ops/src/infrastructure/db/territory_repo.rs`, `src/api/http/territories.rs`
- Create: `services/hub-ops/migrations/0015_territory_assignments.sql`
- Test: `services/hub-ops/tests/territory_db.rs`

**Interfaces:**
- Produces: `POST /v1/territories/{id}/publish`
- Produces: `TerritoryRepository::publish(tenant, id) -> Result<Territory, PublishError>`, with `PublishError::{Overlap { with: Vec<Uuid> }, PriorityConflict, Other}`
- Produces: `find_covering(tenant, hub, lat, lng, at) -> Result<Option<TerritoryMatch { id, version, label, partners }>>`, partners ordered by priority.

- [ ] **Step 1: Write failing tests:**
  - `shared_edge_may_publish`, `area_overlap_is_refused`
  - `concurrent_overlapping_publishes_have_one_winner`, using two real connections
  - `duplicate_priority_on_overlapping_dates_is_refused`
  - `point_on_shared_edge_returns_the_lowest_label`, repeated 20 times
  - `point_outside_every_territory_returns_none`
  - `publishing_a_new_version_retires_the_old_one`
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test territory_db publish` → FAIL.
- [ ] **Step 3: Implement.** Take `pg_advisory_xact_lock(hashtextextended($tenant || ':' || $hub, 0))`. Lookup uses `ST_Covers` and `ORDER BY label COLLATE "C" LIMIT 1`.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): publish non-overlapping territories and resolve a point`

---

### Task 1.8: Local store projection

**Files:**
- Create: `services/hub-ops/migrations/0016_store_network.sql`
- Create: `services/hub-ops/src/infrastructure/store_partner_consumer.rs`. This is hub-ops' first Kafka consumer. `rdkafka` is already a dependency. Spawn it in `bootstrap.rs` with the shutdown watch pattern from dispatch
- Test: `services/hub-ops/tests/store_projection_db.rs`

**Interfaces:**
- Produces tables: `hub_ops.store_partner_projection` (fields of `PartnerStoreUpdated`, with `rates` as JSONB), and `hub_ops.store_network_config(tenant_id, hub_id, enabled, mode, allow_cod, max_declared_value_cents, require_declared_value, default_parcel_volume_cm3, shadow_wave_times JSONB)`.
- `mode` values are `off`, `shadow`, `live`.

- [ ] **Step 1: Write failing tests:** `store_update_is_projected`, `older_version_is_ignored`, `replay_changes_nothing`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test store_projection_db` → FAIL.
- [ ] **Step 3: Implement** with `ON CONFLICT ... WHERE excluded.version > projection.version`.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): keep a local projection of store partners`

---

### Task 1.9: Parcel facts read

**Files:**
- Modify: `services/order-intake/src/api/http/mod.rs`, `src/infrastructure/db/mod.rs`
- Create: `services/hub-ops/src/infrastructure/parcel_facts_client.rs`

**Interfaces:**
- Produces: `GET /v1/internal/shipments/{id}/sort-facts` returning `ParcelFacts`:

```rust
pub struct ParcelFacts {
    pub shipment_id: Uuid, pub tenant_id: Uuid, pub awb: String,
    pub lat: Option<f64>, pub lng: Option<f64>,
    pub quality: String, pub location_source: Option<String>,
    pub weight_g: i32, pub length_cm: Option<i32>, pub width_cm: Option<i32>, pub height_cm: Option<i32>,
    pub service_type: String, pub delivery_category: String,
    pub declared_value_cents: Option<i64>, pub cod_cents: i64,
    pub promised_by: Option<DateTime<Utc>>,
}
```

- [ ] **Step 1: Write failing tests:** `sort_facts_return_the_shipment`, `sort_facts_are_refused_for_another_tenant`, `client_timeout_is_an_error_not_a_default`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-order-intake sort_facts` → FAIL.
- [ ] **Step 3: Implement.** The route takes a required `tenant_id` query. The client uses a 2-second timeout.
- [ ] **Step 4: Confirm they pass.** Same command and `cargo test -p logisticos-hub-ops --lib parcel_facts` → PASS.
- [ ] **Step 5: Commit.** `feat(order-intake): internal sort-facts read for the hub`

---

### Task 1.10: Sort resolver

**Files:**
- Create: `services/hub-ops/src/domain/algorithms/sort_resolver.rs`
- Modify: `services/hub-ops/src/domain/algorithms/mod.rs`

**Interfaces:**
- Produces:

```rust
pub enum FallbackReason { NotEnabled, BadLocation, Ineligible(&'static str), NoTerritory, SlaRisk, NoPartner }
pub enum Resolution {
    Tote { label: String, carrier_id: Uuid, territory_id: Uuid, territory_version: i32, wave: WaveRef },
    Fallback(FallbackReason),
}
pub fn resolve(parcel: &ParcelFacts, ctx: &ResolverContext, now: DateTime<Utc>) -> Resolution;
```

`ResolverContext` holds config, the covering territory, candidate partners with remaining capacity and COD headroom, and ordered waves. The function does no I/O.

- [ ] **Step 1: Write failing table tests**, one per spec step in spec order: disabled hub; each location quality including `customer_pin`; overweight, oversized, non-`parcel` category, `balikbayan`, value over cap, missing value when required; no territory; missed cutoff joins the next wave; no wave left gives `SlaRisk`; next wave misses SLA gives `SlaRisk`; primary full uses backup; inactive primary is skipped; COD over cap is skipped; COD with `allow_cod = false` is `Ineligible("cod")`; no partner left. Add `same_inputs_give_the_same_result`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --lib sort_resolver` → FAIL.
- [ ] **Step 3: Implement** in the spec's order. `INTERPOLATED`, `STREET`, `LOCALITY` and `UNKNOWN` fall back unless the source is `customer_pin`.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): deterministic sort resolver`

---

### Task 1.11: Shadow mode at both sort entry points

**Files:**
- Create: `services/hub-ops/migrations/0017_sort_decisions.sql`
- Create: `services/hub-ops/src/application/services/sort_shadow_service.rs`
- Modify: `services/hub-ops/src/api/http/mod.rs:233`
- Modify: `services/hub-ops/src/application/services/hub_transfer_service.rs:334`
- Test: `services/hub-ops/tests/sort_shadow_db.rs`

**Interfaces:**
- Produces: `SortShadowService::observe(tenant, hub, shipment_id, source)`, spawned and never awaited by the sort path.
- Produces table `hub_ops.sort_decisions(id, tenant_id, hub_id, shipment_id, source, outcome, reason, territory_id, territory_version, carrier_id, simulated_wave, decided_at, UNIQUE (shipment_id, source, simulated_wave))`.

- [ ] **Step 1: Write failing tests:** `shadow_records_a_decision`, `off_records_nothing`, `facts_outage_records_held_and_sort_succeeds`, `slow_facts_do_not_delay_the_sort`, `shadow_changes_no_status_and_publishes_nothing`, `shadow_capacity_counts_by_simulated_wave`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test sort_shadow_db` → FAIL.
- [ ] **Step 3: Implement** with a bounded queue. When it is full, drop the observation and increment a metric.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(hub-ops): run the resolver in shadow on real parcels`

---

### Task 1.12: Admin Neighborhood Network: approvals and territory editor

**Files:**
- Create: `apps/admin-portal/src/app/(dashboard)/neighborhood-network/page.tsx`, `territories/page.tsx`, `partners/page.tsx`
- Create: `apps/admin-portal/src/components/neighborhood/TerritoryEditor.tsx`
- Create: `apps/admin-portal/src/lib/api/neighborhood.ts`
- Modify: `apps/admin-portal/package.json`, `src/app/(dashboard)/layout.tsx`

**Interfaces:**
- Consumes: carrier and territory routes from Tasks 1.5–1.7. Types are generated from OpenAPI with `openapi-typescript`.

- [ ] **Step 1: Write a failing jest test** for `findOverlapWarnings(features)` and `validatePolygon(feature)`.
- [ ] **Step 2: Confirm it fails.** `npm test -- neighborhood` in `apps/admin-portal` → FAIL.
- [ ] **Step 3: Implement** the editor: draw, name, label, hub, partner priority, fees and hours through carrier, overlap and gap view, publish. The server's overlap answer is the authority. All strings go through `useTranslations`.
- [ ] **Step 4: Verify.** `npm run lint && npm run typecheck && npm test && npm run build`. Screenshot at 375, 768 and 1280 px. On a phone the map is read-only with a "use a larger screen to edit" message.
- [ ] **Step 5: Commit.** `feat(admin-portal): neighborhood network approvals and territory editor`

---

### Task 1.13: Vendor opt-in and Store Workspace shell

**Files:**
- Modify: `apps/merchant-portal/src/app/(dashboard)/storefront/page.tsx`
- Create: `apps/partner-portal/src/app/(dashboard)/store/layout.tsx`, `store/page.tsx`
- Modify: `apps/partner-portal/src/app/(dashboard)/layout.tsx`

**Interfaces:**
- Consumes: `POST /v1/omnideliv/vendors/me/delivery-partner`, `GET /v1/store-partners/me`.

- [ ] **Step 1: Build** the "Earn from local deliveries" card with applied, approved and paused states, and a Store Workspace shell with bottom navigation for Today, Riders, Cash and Payouts.
- [ ] **Step 2: Verify.** `npm run lint && npm run typecheck && npm run build` in both portals, with screenshots at 375, 768 and 1280 px in `en` and `ar`, and a keyboard-only pass.
- [ ] **Step 3: Commit.** `feat(portals): vendor opt-in and store workspace shell`

---

### Task 1.14: Shadow report and gate

**Files:**
- Create: `scripts/reports/shadow-network-report.sql`
- Create: `docs/runbooks/neighborhood-shadow-mode.md`

- [ ] **Step 1: Write the report:** per territory per day, eligible parcels, fallback by reason, parcels per simulated run, and the cost formula from spec section 11 with finance's inputs as parameters.
- [ ] **Step 2: Test it** on seeded data with hand-computed totals.
- [ ] **Step 3: Commit.** `docs(network): shadow report and runbook`
- [ ] **Step 4: With the owner's go-ahead,** deploy, verify the three topics on the broker, set one hub to `mode = shadow`, and collect at least two full weeks.
- [ ] **Step 5: Gate.** Shadow data shows enough eligible parcels per territory per day to meet the section 11 economics, using finance's current cost per delivered parcel in AED.
