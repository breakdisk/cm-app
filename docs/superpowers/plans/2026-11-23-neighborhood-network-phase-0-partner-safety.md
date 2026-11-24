# Neighborhood Network Phase 0: Partner Safety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make partner access and rider dispatch safe enough to put a store on the platform, and close the live data exposures found while planning.

**Architecture:** A `pid` claim issued from `identity.partner_memberships` narrows every partner, store and store-rider token. Order-intake keeps a local shipment-to-partner link table. Dispatch gains a `completed` assignment state, written only after every replica can read it.

**Tech Stack:** Rust, Axum, SQLx, Kafka, PostgreSQL, Next.js 14, `next-intl`, Android string resources.

**Spec:** [Neighborhood Delivery Network](../specs/2026-11-23-neighborhood-delivery-network-design.md), sections 4, 7 and 10. **Program plan:** [index](2026-11-23-neighborhood-delivery-network.md).

## Global Constraints

- Baseline is `origin/master` at `bb8d9519`. Work in a fresh worktree on `feat/neighborhood/phase-0-partner-safety`.
- A partner-role token without `pid` is denied on partner-scoped routes once `AUTH__ENFORCE_PARTNER_SCOPE=true`. It never falls back to tenant-wide data after that.
- Rollout order: ship the claim, backfill memberships, wait one full token lifetime, then enforce.
- `partner_id` is never accepted from a store in a path or body.
- Readers ship before writers: `parse_status` maps unknown assignment statuses to `Pending`.
- Do **not** write `drivers.active_route_id`.
- `dest_geocoding_quality` has no SQL `CHECK`. Unrecognized values read as `UNKNOWN`.
- Location quality gates only the store resolver. It never blocks existing dispatch.
- Next migrations: identity `0018`, order-intake `0024`, dispatch `0019`.
- Rust checks use `CARGO_INCREMENTAL=0`. Lint with `cargo clippy -p <crate> --all-targets -- -D warnings`.
- DB-backed tests use the `database_url()` helper from `services/field-ops/tests/claim_race.rs`: skip locally, fail when `CI` is set.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. A store rider holds the `driver` role, which has tenant-wide `shipments:read` and `dispatch:view`. A rider token with `pid` must not list the queue or other partners' shipments (Tasks 0.7, 0.8).
2. A driver accepted an assignment and its `TASK_ASSIGNED` has not been consumed yet. "No open tasks" is then true, and the assignment must **not** close (Task 0.12).
3. A merchant passes another merchant's id to `GET /v1/shipments?merchant_id=`. It must be ignored (Task 0.1).
4. A user belongs to two carriers. Token issue must be deterministic and must not pick one by accident (Task 0.4).
5. A new handler is added next year with `shipments:read` and no scope check. CI must name it (Task 0.10).

---

### Task 0.1: Owner-scope the shipment list

This fixes a live exposure and can ship on its own, ahead of the rest of the phase.

**Files:**
- Modify: `services/order-intake/src/application/queries/mod.rs:76-93`
- Modify: `services/order-intake/src/api/http/mod.rs:634-645`
- Modify: `services/order-intake/src/domain/value_objects/cancel_authority.rs`

**Interfaces:**
- Produces: `pub fn effective_merchant_filter(actor: &Actor, requested: Option<Uuid>) -> Option<Uuid>`
- Produces: `ShipmentQueryService::list(&self, actor: &Actor, q: ListShipmentsQuery)`

- [ ] **Step 1: Write failing tests** in `cancel_authority.rs`:

```rust
#[test] fn merchant_list_is_pinned_to_self() {
    let me = Uuid::new_v4();
    let a = Actor { tenant_id: Uuid::new_v4(), user_id: me, tenant_wide: false };
    assert_eq!(effective_merchant_filter(&a, None), Some(me));
}
#[test] fn merchant_cannot_widen_by_naming_another_merchant() {
    let me = Uuid::new_v4();
    let a = Actor { tenant_id: Uuid::new_v4(), user_id: me, tenant_wide: false };
    assert_eq!(effective_merchant_filter(&a, Some(Uuid::new_v4())), Some(me));
}
#[test] fn operator_keeps_the_requested_filter() {
    let wanted = Uuid::new_v4();
    let a = Actor { tenant_id: Uuid::new_v4(), user_id: Uuid::new_v4(), tenant_wide: true };
    assert_eq!(effective_merchant_filter(&a, Some(wanted)), Some(wanted));
    assert_eq!(effective_merchant_filter(&a, None), None);
}
```

- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-order-intake --lib cancel_authority` → compile error, `effective_merchant_filter` not found.
- [ ] **Step 3: Implement** the function, change `list` to take `&Actor`, and in the handler build the actor with the existing `acting_as` rule.
- [ ] **Step 4: Confirm they pass.** Same command → PASS. Then `cargo clippy -p logisticos-order-intake --all-targets -- -D warnings` → no errors.
- [ ] **Step 5: Commit.** `fix(order-intake): limit the shipment list to the caller's own bookings`

---

### Task 0.2: Phase 0 contracts and route check

**Files:**
- Modify: `docs/api/openapi/identity.yaml`, `order-intake.yaml`, `driver-ops.yaml`, `dispatch.yaml`, `payments.yaml`
- Create: `scripts/check-openapi-routes.py`, `scripts/openapi-tracked-prefixes.txt`
- Modify: `.github/workflows/ci-rust.yml` (lint job)

**Interfaces:**
- Produces: `python scripts/check-openapi-routes.py` exits 1 and prints `method path` for any route under a tracked prefix that is missing from its contract.

- [ ] **Step 1: Add** to the contracts: `GET /v1/partner-memberships/me`, `POST /v1/internal/partner-memberships`, the `PARTNER_CONTEXT_REQUIRED` 403, the optional address pin fields, and the scoped behavior of `/v1/shipments`, `/v1/tasks/manifest` and `/v1/drivers`.
- [ ] **Step 2: Write the checker.** It parses `.route("...")` and nested prefixes for each service and compares only paths under `openapi-tracked-prefixes.txt`, because existing specs have drifted and this must not fail on old routes.
- [ ] **Step 3: Mutation-test it.** Temporarily add `.route("/v1/partner-memberships/ghost", get(x))`, run the script, expect exit 1 naming that path, and revert.
- [ ] **Step 4: Add it to the lint job** and run it on the clean tree. Expected: exit 0.
- [ ] **Step 5: Commit.** `docs(api): phase 0 partner contracts and tracked-route check`

---

### Task 0.3: `pid` claim, scope resolver and store roles

**Files:**
- Modify: `libs/auth/src/claims.rs`, `libs/auth/src/rbac.rs`
- Modify: `services/identity/src/api/http/users.rs:155`

**Interfaces:**
- Produces: `Claims.pid: Option<Uuid>` with `#[serde(default)]`; `Claims::with_partner(self, pid: Option<Uuid>) -> Self`
- Produces:

```rust
pub enum PartnerScope { NotPartner, Partner(Uuid), MissingPid }
pub enum PartnerAccess { Unscoped, Scoped(Uuid) }
impl Claims { pub fn partner_scope(&self) -> PartnerScope; }
pub fn resolve_partner_access(c: &Claims, enforce: bool) -> Result<PartnerAccess, AppError>;
pub fn partner_access(c: &Claims) -> Result<PartnerAccess, AppError>; // reads AUTH__ENFORCE_PARTNER_SCOPE
```

- Produces permissions: `TOTES_READ`, `TOTES_RECEIVE`, `TOTES_RELEASE`, `TOTES_SORT`, `TOTES_CARRY`, `STORE_RIDERS_MANAGE`, `STORE_BATCHES_ASSIGN`, `STORE_LEDGER_READ`, with the string values from spec section 7.

- [ ] **Step 1: Write failing tests** in `claims.rs`:
  - `old_token_without_pid_still_decodes`: JSON without `pid` decodes to `None`.
  - `partner_role_without_pid_is_missing`, for `partner`, `store_manager`, `store_clerk`.
  - `platform_driver_without_pid_is_not_a_partner`.
  - `store_rider_with_pid_is_scoped`: `driver` role plus `pid` gives `Partner(pid)`.
  - `missing_pid_is_denied_when_enforced`: error code `PARTNER_CONTEXT_REQUIRED`.
  - `missing_pid_stays_unscoped_before_enforcement`, and logs `partner_scope_missing_pid`.
  - `pid_always_scopes_even_before_enforcement`.
  - In `rbac.rs`: `store_manager` and `store_clerk` hold exactly the permissions in the spec table, and neither holds `shipments:update`, `shipments:read` or `payments:read`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-auth --lib` → compile errors on the new names.
- [ ] **Step 3: Implement.** `partner_scope` treats `partner`, `store_manager` and `store_clerk` as partner roles, and any token with `pid` as `Partner(pid)`. Add `store_manager` and `store_clerk` to `ASSIGNABLE_ROLES`.
- [ ] **Step 4: Confirm they pass.** `cargo test -p logisticos-auth --lib` → PASS. `cargo check -p logisticos-identity` → clean.
- [ ] **Step 5: Commit.** `feat(auth): partner claim, fail-closed scope resolver and store roles`

---

### Task 0.4: Partner memberships and token issue

**Files:**
- Create: `services/identity/migrations/0018_partner_memberships.sql`
- Create: `services/identity/src/infrastructure/db/partner_membership_repo.rs`
- Create: `services/identity/src/api/http/partner_memberships.rs`
- Modify: `services/identity/src/domain/repositories/mod.rs`
- Modify: `services/identity/src/application/services/auth_service.rs:141,205,605,916`
- Test: `services/identity/tests/partner_membership_db.rs`

**Interfaces:**
- Produces: `PartnerMembershipRepository::{find_active(tenant_id, user_id) -> Result<Option<PartnerMembership>>, upsert_active(&NewPartnerMembership) -> Result<PartnerMembership>}`
- Produces: `GET /v1/partner-memberships/me`, `POST /v1/internal/partner-memberships`

Migration uses the spec's table plus this index, so a user can act for only one partner at a time:

```sql
CREATE UNIQUE INDEX uq_one_active_partner_membership
    ON identity.partner_memberships (tenant_id, user_id) WHERE status = 'active';
```

- [ ] **Step 1: Write failing DB tests:** `active_membership_is_found`, `suspended_membership_is_not_found`, `second_active_membership_for_a_user_is_refused`, `upsert_is_idempotent`.
- [ ] **Step 2: Confirm they fail.** With a disposable `DATABASE_URL`: `cargo test -p logisticos-identity --test partner_membership_db` → FAIL, relation missing.
- [ ] **Step 3: Implement** the migration and repository. Add one private `partner_context(tenant, user)` helper and chain `.with_partner(...)` at all four `Claims::new` sites.
- [ ] **Step 4: Write a guard test** `every_token_site_sets_partner_context` that reads `auth_service.rs` and asserts the count of `.with_currency(` equals the count of `.with_partner(`.
- [ ] **Step 5: Confirm they pass.** `cargo test -p logisticos-identity --test partner_membership_db` and `cargo test -p logisticos-identity --lib` → PASS, DB tests with non-zero duration.
- [ ] **Step 6: Commit.** `feat(identity): partner memberships and pid at token issue`

---

### Task 0.5: Membership and link backfill tool

**Files:**
- Create: `scripts/backfill/partner-memberships.sh`, `scripts/backfill/README.md`

**Interfaces:**
- Consumes: `carrier.carriers.contact_email`, `driver_ops.drivers(user_id, carrier_id)`, `identity.users(id, email)`.
- Produces: a report of `matched`, `unmatched_email`, `multiple_carriers`, and idempotent `ON CONFLICT DO NOTHING` inserts.

- [ ] **Step 1: Write the script.** Default is `--dry-run`, and `--apply` is required to write. Emails match with `lower()`. A user who maps to two carriers is reported and **not** inserted.
- [ ] **Step 2: Test on a disposable database** seeded with one match, one mixed-case email, one unmatched carrier and one user on two carriers. Expected dry-run counts: `matched=2 unmatched_email=1 multiple_carriers=1`.
- [ ] **Step 3: Run `--apply` twice.** Expected: row count unchanged after the second run.
- [ ] **Step 4: Commit.** `chore(identity): idempotent partner membership backfill`

Do not run this on the VPS as part of this task. It is a rollout step in Task 0.17.

---

### Task 0.6: Shipment-to-partner links

Order-intake has no carrier column today, so partner scoping has nothing to filter on.

**Files:**
- Create: `services/order-intake/migrations/0024_shipment_partner_links.sql`
- Create: `services/order-intake/src/infrastructure/messaging/partner_link_consumer.rs`
- Create: `scripts/backfill/partner-shipment-links.sh`
- Modify: `libs/events/src/payloads.rs:457` (`TaskAssigned`)
- Modify: every `TaskAssigned {` site in `services/dispatch/src`
- Modify: `services/order-intake/src/bootstrap.rs`
- Test: `services/order-intake/tests/partner_links_db.rs`

**Interfaces:**
- Produces: `TaskAssigned.driver_carrier_id: Option<Uuid>` with `#[serde(default)]`
- Produces table `order_intake.shipment_partner_links(tenant_id, shipment_id, carrier_id, source, created_at, PRIMARY KEY (shipment_id, carrier_id))`
- Produces: `ShipmentRepository::is_linked(tenant_id, shipment_id, carrier_id) -> Result<bool>`

- [ ] **Step 1: Write failing tests:** `carrier_allocated_links_the_shipment`, `task_for_a_carrier_driver_links_the_shipment`, `task_for_a_platform_driver_links_nothing`, `replayed_event_adds_no_row`, `old_task_assigned_payload_without_the_field_decodes`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-order-intake --test partner_links_db` → FAIL.
- [ ] **Step 3: Implement.** Consumer group `order-intake-partner-links`, `auto.offset.reset=earliest`, subscribing to `CARRIER_ALLOCATED` and `TASK_ASSIGNED`. Dispatch reads `drivers.carrier_id` by `user_id` when it builds the event.
- [ ] **Step 4: Add the backfill** from `carrier.sla_records` and `driver_ops.tasks JOIN drivers`, dry-run by default, in the same shape as Task 0.5.
- [ ] **Step 5: Confirm they pass.** Same test command plus `cargo check -p logisticos-dispatch -p logisticos-events` → PASS.
- [ ] **Step 6: Commit.** `feat(order-intake): project which partner handles each shipment`

---

### Task 0.7: Scope shipment reads by partner

**Files:**
- Modify: `services/order-intake/src/domain/value_objects/cancel_authority.rs` (`Actor`)
- Modify: `services/order-intake/src/application/services/shipment_service.rs:18`
- Modify: `services/order-intake/src/application/queries/mod.rs`, `src/infrastructure/db/mod.rs:312-345`
- Modify: `services/order-intake/src/api/http/mod.rs:30`

**Interfaces:**
- Consumes: `partner_access`, `is_linked`.
- Produces: `Actor.partner_id: Option<Uuid>`; `ShipmentListFilter.partner_id: Option<Uuid>`.

- [ ] **Step 1: Write failing DB tests:** `partner_lists_only_linked_shipments`, `partner_gets_404_for_an_unlinked_shipment_by_id`, `store_rider_with_pid_cannot_list_the_tenant`, `partner_cannot_widen_with_merchant_id`, `operator_view_is_unchanged`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-order-intake --test partner_links_db` → FAIL.
- [ ] **Step 3: Implement.** `get_for` checks `is_linked` when `partner_id` is set, before `may_act_on`. An unlinked id returns the same `NotFound` as a missing one. List adds an `EXISTS` on links, in both the `COUNT` and the page query.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `fix(order-intake): partner tokens read only their own shipments`

---

### Task 0.8: Scope manifest, roster, queue and AWB lookup

**Files:**
- Modify: `services/driver-ops/src/api/http/tasks.rs:75`, `services/driver-ops/src/api/http/drivers.rs:95`
- Modify: `services/dispatch/src/api/http/queue.rs:17,41`
- Modify: `services/hub-ops/src/bootstrap.rs:2169`
- Test: `services/driver-ops/tests/unit/partner_scope.rs` (declare it in `tests/unit/mod.rs`), `services/dispatch/tests/integration/main.rs`, `services/hub-ops/tests/unit/mod.rs`

**Interfaces:**
- Produces: `pub fn manifest_carrier(access: &PartnerAccess, requested: Option<Uuid>) -> Option<Uuid>`, which returns `pid` for a scoped caller whatever was requested.

- [ ] **Step 1: Write failing tests:**
  - `manifest_ignores_a_requested_carrier_for_a_scoped_token`
  - `manifest_requires_a_permission`: a token with no fleet or carrier permission and no `pid` gets 403.
  - `partner_roster_lists_only_its_riders`, using `carriers:manage-own` plus `pid`.
  - `rider_with_pid_gets_403_on_queue`, `rider_with_pid_gets_403_on_dispatch_drivers`, `platform_driver_keeps_queue_access`.
  - `scoped_token_gets_403_on_shipment_by_awb`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-driver-ops --test unit` and `cargo test -p logisticos-dispatch --test integration` → FAIL.
- [ ] **Step 3: Implement.** Manifest needs `FLEET_READ`, `CARRIERS_MANAGE` or a scoped token. The Android app does not call the dispatch queue, the dispatch roster or the collection `GET /v1/shipments`, so refusing scoped tokens there is safe. It does call `shipment-by-awb` for hub scans, so the refusal applies only to tokens with `pid`.
- [ ] **Step 4: Confirm they pass.** Run the commands above plus `cargo test -p logisticos-hub-ops --test unit` → PASS.
- [ ] **Step 5: Commit.** `fix(partner): scope manifest and roster, deny queue to store tokens`

---

### Task 0.9: Close partner access to tenant money and analytics

The portal change ships **before** the server denial, or the pages show errors.

**Files:**
- Modify: `apps/partner-portal/src/app/(dashboard)/payouts/page.tsx`, `sla/page.tsx`, `page.tsx:135-141`
- Modify: `apps/partner-portal/src/lib/api/payments.ts`
- Modify: `services/payments/src/api/http/wallet.rs:15,28,87,110`, and the invoice list handler
- Modify: `services/analytics/src/api/http/mod.rs`
- Modify: `libs/auth/src/rbac.rs:222`

**Interfaces:**
- Consumes: `partner_access`, existing `GET /v1/carriers/:id/sla-summary`.

- [ ] **Step 1: Portal.** Payouts shows a clear "Statements arrive when your store joins the delivery network" state and stops calling `/v1/wallet*` and `/v1/invoices`. The tenant wallet was never this partner's money. SLA and the dashboard use `sla-summary` and the scoped manifest.
- [ ] **Step 2: Verify the portal.** `npm run lint && npm run typecheck && npm run build` in `apps/partner-portal`. Take screenshots of Payouts, SLA and Dashboard at 375, 768 and 1280 px.
- [ ] **Step 3: Write failing server tests:** `scoped_token_gets_403_on_wallet`, `_on_transactions`, `_on_withdrawals`, `_on_cod_balance`, `_on_invoices`, `_on_analytics_kpis`, `finance_role_is_unchanged`.
- [ ] **Step 4: Confirm they fail.** `cargo test -p logisticos-payments --test unit` and `cargo test -p logisticos-analytics` → FAIL.
- [ ] **Step 5: Implement** the denial, and remove `PAYMENTS_READ` and `ANALYTICS_VIEW` from the `partner` role in `rbac.rs`.
- [ ] **Step 6: Confirm they pass.** Same commands plus `cargo test -p logisticos-auth --lib` → PASS.
- [ ] **Step 7: Commit twice.** `feat(partner-portal): stop reading tenant money and KPIs`, then `fix(payments,analytics): deny tenant-wide reads to partner tokens`

---

### Task 0.10: Partner access guard in CI

**Files:**
- Create: `scripts/check-partner-scope.py`, `scripts/partner-scope-inventory.yaml`
- Modify: `.github/workflows/ci-rust.yml`

**Interfaces:**
- Produces: the script finds every handler requiring a permission held by `partner`, `store_manager`, `store_clerk` or `driver`, and fails on any handler absent from the inventory. Each inventory entry is `scoped`, `denied` or `not_sensitive`, with a one-line reason.

- [ ] **Step 1: Write the script and inventory** covering every current handler.
- [ ] **Step 2: Mutation-test.** Add a throwaway handler requiring `SHIPMENT_READ`, confirm exit 1 naming it, and revert.
- [ ] **Step 3: Run on the clean tree.** Expected: exit 0.
- [ ] **Step 4: Commit.** `ci(security): every partner-reachable handler must be classified`

---

### Task 0.11: Teach dispatch to read `completed`

Deploy this before Task 0.12 can write the value.

**Files:**
- Create: `services/dispatch/migrations/0019_assignment_completed.sql`
- Modify: `services/dispatch/src/domain/entities/driver_assignment.rs:22`
- Modify: `services/dispatch/src/infrastructure/db/assignment_repo.rs:31,40`

**Interfaces:**
- Produces: `AssignmentStatus::Completed`, stored as `completed`.

Migration replaces `driver_assignments_status_check` to add `'completed'`. The two partial unique indexes already exclude it.

- [ ] **Step 1: Write failing tests:** `completed_round_trips`, `completed_is_not_active`, `unknown_status_is_not_silently_active`. The last one changes `parse_status` so that an unknown value returns `Cancelled` and logs an error, rather than `Pending`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-dispatch --lib assignment` → FAIL.
- [ ] **Step 3: Implement** the variant, parser, writer and migration.
- [ ] **Step 4: Confirm they pass.** Same command, plus applying the migration on a disposable database → PASS.
- [ ] **Step 5: Commit.** `feat(dispatch): assignments can be completed (reader)`

---

### Task 0.12: Close assignments and routes when work ends

**Files:**
- Create: `services/dispatch/src/infrastructure/messaging/task_outcome_consumer.rs`
- Modify: `services/dispatch/src/infrastructure/db/assignment_repo.rs`, `src/domain/repositories/mod.rs`
- Modify: `services/dispatch/src/application/services/driver_assignment_service.rs:571`
- Modify: `services/dispatch/src/bootstrap.rs`, `src/config.rs`
- Test: `services/dispatch/tests/assignment_close_db.rs`

**Interfaces:**
- Produces: `DriverAssignmentRepository::close_if_finished(tenant_id: Uuid, route_id: Uuid) -> Result<bool>`; `close_finished_assignments(limit: i64) -> Result<u64>`
- Config: `DISPATCH__CLOSE_ASSIGNMENTS`, default `false`, so the writer turns on only after every replica has Task 0.11.

Close only when **all** of these hold, in one transaction:

```sql
-- at least one task exists, none open, and every route stop has a task
EXISTS (SELECT 1 FROM driver_ops.tasks t WHERE t.route_id = $1)
AND NOT EXISTS (SELECT 1 FROM driver_ops.tasks t
                 WHERE t.route_id = $1 AND t.status IN ('pending','in_progress'))
AND (SELECT COUNT(*) FROM driver_ops.tasks t WHERE t.route_id = $1)
    >= (SELECT COUNT(*) FROM dispatch.route_stops s WHERE s.route_id = $1)
```

- [ ] **Step 1: Write failing DB tests.** The test creates a minimal `driver_ops.tasks` fixture, because CI migrates only the service under test:
  - `last_task_done_closes_assignment_and_route`
  - `open_task_keeps_it_active`
  - `accepted_with_no_tasks_yet_stays_active`
  - `partially_arrived_tasks_stay_active`: 3 stops, 1 finished task.
  - `failed_and_cancelled_count_as_finished`
  - `replayed_outcome_changes_nothing`
  - `rider_can_be_assigned_again_after_close`
  - `cancel_route_closes_its_assignment`
  - `flag_off_writes_nothing`
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-dispatch --test assignment_close_db` → FAIL.
- [ ] **Step 3: Implement** the repository methods, a consumer of `DELIVERY_COMPLETED`, `DELIVERY_FAILED` and `PICKUP_COMPLETED` with the retry shape in `job_dropped_consumer.rs`, a repair pass in the existing 90-second sweep, and the `cancel_route` close.
- [ ] **Step 4: Confirm they pass.** Same command → PASS, non-zero duration.
- [ ] **Step 5: Commit.** `fix(dispatch): free riders when their work ends or a route is cancelled`

---

### Task 0.13: Make Auto routing's carrier fallback work

**Files:**
- Modify: `libs/events/src/payloads.rs:943`, `services/hub-ops/src/application/services/hub_transfer_service.rs:386`
- Modify: `services/dispatch/src/infrastructure/messaging/hub_dispatch_consumer.rs:125`

**Interfaces:**
- Produces: `HubDispatchRequested.fallback_carrier_id: Option<Uuid>` with `#[serde(default)]`.

- [ ] **Step 1: Write failing tests:** `auto_fallback_carries_the_configured_carrier`, `auto_without_a_carrier_stays_queued_and_logs`, `old_payload_without_the_field_decodes`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-dispatch --lib hub_dispatch` → FAIL.
- [ ] **Step 3: Implement.** Extract `fn fallback_event(d: &HubDispatchRequested) -> Option<HubCarrierBookingRequested>`. With no carrier, publish nothing and log an error naming the hub.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `fix(dispatch): auto routing falls back to the configured carrier`

---

### Task 0.14: Geocoding quality

**Files:**
- Create: `services/order-intake/migrations/0025_geocoding_quality.sql`
- Create: `libs/types/src/geocode_quality.rs`, because hub-ops reads the same value in Phase 1
- Modify: `services/order-intake/src/infrastructure/external/mapbox_geocoder.rs`
- Modify: `services/order-intake/src/application/services/shipment_service.rs:199`, `src/infrastructure/db/mod.rs`, `src/config.rs`

**Interfaces:**
- Produces in `logisticos_types`: `enum GeocodeQuality { Rooftop, Building, Parcel, Interpolated, Street, Locality, Unknown }` with `parse(&str) -> Self`, unknown input giving `Unknown`, and `as_str(&self) -> &'static str`
- Produces in `mapbox_geocoder.rs`: `pub fn quality_from_mapbox(feature_type: &str, accuracy: Option<&str>, confidence: Option<&str>) -> GeocodeQuality`
- Produces: `AddressNormalizer::normalize_detailed`, default implementation wraps `normalize` with `Unknown`
- Config: `MAPBOX__PERMANENT_GEOCODING`, default `false`

- [ ] **Step 1: Write failing table tests** with every row of the spec's Address quality table, plus: `medium_confidence_caps_at_interpolated`, `low_confidence_caps_at_locality`, `cap_never_raises_quality`, `unrecognized_quality_reads_as_unknown`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-types geocode_quality` and `cargo test -p logisticos-order-intake --lib mapbox` → FAIL.
- [ ] **Step 3: Implement.** Add the spec's six columns. Send `permanent=true` only when the flag is on. With the flag off, store only quality and source, and leave place id and confidence `NULL`.
- [ ] **Step 4: Confirm they pass.** Same command, plus the migration on a disposable database → PASS.
- [ ] **Step 5: Commit.** `feat(order-intake): record geocoding quality for each destination`

---

### Task 0.15: Customer location pin

**Files:**
- Modify: `services/order-intake/src/application/commands/mod.rs` (`AddressInput`)
- Modify: `services/order-intake/src/application/services/shipment_service.rs`

**Interfaces:**
- Produces: `AddressInput.{pin_lat, pin_lng, building, unit, area}: Option<...>` with `#[serde(default)]`.

- [ ] **Step 1: Write failing tests:** `pin_overrides_geocoder_and_marks_customer_pin`, `pin_with_only_one_coordinate_is_rejected`, `pin_out_of_range_is_rejected`, `pin_at_zero_zero_is_rejected`, `booking_without_a_pin_is_unchanged`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-order-intake --lib pin` → FAIL.
- [ ] **Step 3: Implement.** A valid pin sets `dest_geocoding_source = customer_pin`.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(order-intake): accept a customer pin and UAE address fields`

---

### Task 0.16: Translation layer

**Files:**
- Create in `apps/partner-portal`, `apps/admin-portal`, `apps/customer-portal`, `apps/landing` (it hosts public tracking): `src/i18n/request.ts`, `messages/en.json`, `messages/ar.json`
- Modify: each portal's `next.config.js`, `package.json`, `src/app/layout.tsx`
- Create: `apps/driver-app-android/app/src/main/res/values-ar/strings.xml`
- Create: `scripts/check-i18n-keys.mjs`

**Interfaces:**
- Produces: `useTranslations(namespace)` in every portal, locale from the `NEXT_LOCALE` cookie, no locale segment in URLs, because each portal already has a `basePath`.

- [ ] **Step 1: Install** `next-intl@3` in the four apps and commit the lockfile changes.
- [ ] **Step 2: Wire the provider** and set `<html lang dir>` from the locale.
- [ ] **Step 3: Write `check-i18n-keys.mjs`**, which fails when `en` and `ar` keys differ. Mutation-test it by deleting one Arabic key.
- [ ] **Step 4: Convert the Payouts page from Task 0.9** as the worked example, using logical CSS properties (`ms-`, `me-`, `text-start`).
- [ ] **Step 5: Verify.** `npm run lint && npm run typecheck && npm run build` per portal. Take screenshots at 375, 768 and 1280 px in `en` and `ar`, and confirm no horizontal scroll.
- [ ] **Step 6: Commit.** `feat(i18n): translation layer with English and Arabic, RTL-ready`

---

### Task 0.17: Rollout and gate

Each deploy step needs the owner's go-ahead. Nothing here runs without it.

- [ ] **Step 1:** Deploy all images with `AUTH__ENFORCE_PARTNER_SCOPE=false` and `DISPATCH__CLOSE_ASSIGNMENTS=false`.
- [ ] **Step 2:** Run both backfills with `--dry-run`, review unmatched rows with the owner, then `--apply`.
- [ ] **Step 3:** Wait one full token lifetime, then confirm the `partner_scope_missing_pid` log count is zero for that period.
- [ ] **Step 4:** Set `AUTH__ENFORCE_PARTNER_SCOPE=true`. Confirm an existing partner login still loads Dashboard, Orders, Drivers, Manifests, SLA and Payouts.
- [ ] **Step 5:** Set `DISPATCH__CLOSE_ASSIGNMENTS=true`. Confirm a real rider finishes a job and receives another.
- [ ] **Step 6: Gate.** All must be true:
  - Tests prove a partner token cannot read another partner's shipments, wallet, manifest or riders.
  - A rider can complete a job and receive another.
  - `cargomarket.net` is reachable.
  - PIN delivery works through Twilio.
