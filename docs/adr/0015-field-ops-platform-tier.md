# ADR-0015: Field-Ops Platform Tier — Minimal Extraction

**Status:** Accepted
**Date:** 2026-08-06
**Deciders:** Principal Architect, Senior Rust Engineer — Driver Operations, Engineering Manager — Logistics Domain, Product Manager — Platform

> **Load-bearing invariant.** A courier is claimed by exactly one assignment at a time. Two products racing to dispatch the same courier must resolve to one winner and one explicit loser — never two accepted assignments.

---

## Context

ADR-0009 established two rules that now collide:

1. **Boundary rule 2:** product services may not call other products' services directly.
2. **"Watch the field-ops cluster":** LogisticOS, Ride-Hailing and Food Delivery share courier identity, GPS ingest, geospatial dispatch, ETA and earnings. *"When the second of these products goes live, extract these into a `field-ops` platform tier rather than copying them."*

OmniDeliv AI is that second product. It needs couriers; every courier capability today lives inside LogisticOS's product tier (`services/driver-ops`, `services/dispatch`). Three options, and only one honours both rules.

## Decision

Extract a **minimal** field-ops platform tier — only what a second field-ops product needs to operate a courier:

| Extracted to `services/field-ops` | Stays in LogisticOS |
|---|---|
| Courier identity (the human in the field) | POD / POP capture |
| Assignment + atomic claim | Hub operations, cross-dock |
| GPS ingest and breadcrumbs (history table + PostGIS GiST + latest-fix view) | Carrier and sub-carrier contracts |
| Earnings ledger (deferred — see below) | Parcel-specific routing and manifests |

Deliberately **not** extracted: ETA prediction, geospatial route optimisation, in-app navigation. Those are still single-consumer; ADR-0009 rule 3 says a service earns platform status only when a second product needs it, and premature platformisation is the second-worst trap after the mega-gateway.

**The earnings ledger is extracted in a later phase**, once OmniDeliv's three-leg settlement model exists. Building it before the order model would mean guessing at the shape.

### The new tier inherits the stronger location model, not a simpler one

`field_ops` reproduces the shape `driver_ops` already has, rather than the cheaper denormalised one:

| Element | Why it is carried forward |
|---|---|
| `courier_locations` history table (TimescaleDB hypertable, 1-day chunks, 7-day compression, 90-day retention) | GPS is a time series. A single mutable last-known column cannot answer "where was this courier at 14:05", which is what dispute resolution and SLA forensics need. |
| PostGIS **GiST** index on `geography(ST_MakePoint(lng, lat))` | Proximity search is `ST_DWithin` against a geography. A btree on `(tenant_id, last_lat, last_lng)` cannot serve it — the planner falls back to a scan, and the index only looks useful. |
| `courier_latest_locations` view (`DISTINCT ON (courier_id) … ORDER BY recorded_at DESC`) | Dispatch reads a view, not a raw table, so the "latest fix" definition lives in one place. `driver_ops` already learned this: the view replaced an ad-hoc subquery. |
| Denormalised `last_lat`/`last_lng` on the courier row | Kept as a **cache** for cheap list rendering, explicitly not the authoritative source. |

This is the one dimension where the existing product-tier table is *ahead* of the proposed platform tier. Shipping the weaker model would force a later choice between downgrading LogisticOS on convergence or rewriting `field-ops` a second time — so the stronger model is the starting point, not a follow-up.

### Product identity is data, not schema

`courier_assignments.product` records which product owns a claim, so completion events route home. It is **not** a `CHECK (product IN (…))` enumeration. A platform tier that needs a schema migration to admit its third consumer is not a platform tier — it is a two-product service with extra steps.

Instead: a `field_ops.products` registry table, with `courier_assignments.product` a foreign key to it. Onboarding a consumer is an `INSERT`, not DDL. The registry also gives the routing destination a home (a bare free-text column yields a label but no way to route a completion event), and the foreign key removes the failure mode a free-text column invites, where `'omnideliv '` and `'omnideliv'` silently fork into two products that no query joins.

## Consequences

### Positive
- OmniDeliv gets couriers without breaching boundary rule 2.
- The extraction the ADR-0009 authors predicted happens at the moment they specified, at minimum scope.
- Courier claim becomes atomic and cross-product, which `driver_ops` never needed and therefore never had.

### Negative — stated plainly, not hidden
- **Two courier tables coexist** (`driver_ops.drivers` and `field_ops.couriers`) until LogisticOS migrates. This is precisely the duplication ADR-0009 rule 4 warns against, and it is only acceptable as a transitional state with a real exit.
- **The exit is a prerequisite, not a promise.** An earlier draft of this ADR committed LogisticOS to migrating onto `field-ops` "within two quarters of OmniDeliv slice one reaching production". That commitment was removed because it would have failed, and the reason it would fail is knowable today: the migration is blocked by the `drivers.id` / `user_id` split-brain described below, and no dated promise survives contact with product pressure while an unaddressed blocker sits in front of it. A deadline with a known obstruction is not a forcing function; it is a scheduled disappointment. The obstruction is removed instead — see **Prerequisite** below.

## Prerequisite: collapse the `drivers.id` / `user_id` split-brain

**This is unblocked work in `driver_ops`, starting now, independent of OmniDeliv's timeline.** It is not scheduled against this extraction and does not wait on it.

`driver_ops.drivers` carries two candidate keys: `id` (PK, `gen_random_uuid()`) and `user_id` (`UNIQUE`, referencing `identity.users`). The hot paths have quietly settled on `user_id`, while `id` persists as a second identity that rows can still be written with:

- `services/dispatch/src/infrastructure/db/driver_avail_repo.rs` joins on `d.user_id` in all three places — the latest-location view, the active-stop-count subquery, and the exclusion of already-assigned drivers.
- `services/driver-ops/src/application/services/location_service.rs` resolves a GPS ping through `find_by_user_id(driver_id)`, so `driver_locations.driver_id` holds a **user_id** despite the column name.
- `services/driver-ops/src/infrastructure/db/task_repo.rs:145` joins through `drivers.user_id` with a comment stating it does so "regardless of whether the driver's primary id matches the identity user_id (they may differ for API-registered drivers)" — a defensive join written because the caller genuinely cannot know which of the two a row holds.

**This is a latent correctness bug on its own merits, and would be worth fixing if `field-ops` did not exist.** A column named `driver_id` that means `users.id` in the GPS and dispatch paths but may mean `drivers.id` in a task row is a silent-wrong-answer waiting for the first row that takes the other branch: proximity search returns a courier who is not there, or drops one who is, with no error raised. It is being called out here because it is also the thing standing between this tier and convergence.

**Why fixing it changes the cost of convergence.** With one unambiguous courier identity, migrating LogisticOS onto `field-ops` stops being a multi-quarter data-model programme and becomes swapping a repository implementation behind an unchanged trait — `driver_ops` reads couriers from `field-ops` instead of its own table, and the ids already agree. That is a reviewable change of a size a team will actually pick up, which is the entire point: the exit condition should be cheap enough that nobody has to be held to a date.

**Decided: `user_id` wins, and `driver_ops` owns the execution.**

`user_id` rather than `drivers.id` because the hot paths have already chosen it — dispatch's three joins, the GPS ping resolution, and the defensive task join all key on `user_id` today. Collapsing onto `drivers.id` would mean rewriting the paths that are currently correct in order to preserve the one that is vestigial. It is also the identity that already means something outside this service: it is `identity.users.id`, so a courier, a portal login and an audit actor are the same id end to end, which `drivers.id` can never be.

`driver_ops` owns it because `driver_ops` owns the table. The work is not OmniDeliv's to schedule and must not be sequenced behind it — that is the whole point of calling it a prerequisite rather than a migration step. It is justified as a latent correctness bug on its own merits (see above); convergence is a beneficiary, not the reason.

**Done means:** one identity for a field worker across `driver_ops` and `dispatch`; `driver_locations.driver_id` and `tasks.driver_id` provably hold the same thing; the defensive join in `task_repo.rs` deleted rather than left as documentation of an ambiguity that no longer exists. `LocationService::load_driver_by_user_id` — present, correct, and currently unreachable — becomes the only lookup.

### Neutral
- Tenant isolation in the new service is application-layer (`WHERE tenant_id = $n`), matching how every other service in this repo actually behaves. See the RLS note in the implementation plan; making RLS genuinely enforce needs its own ADR.

### API contract: the tier is addressed by prefix

Every field-ops route is served under **`/v1/field-ops/...`**, and the API gateway resolves that prefix before any flat resource name.

This is forced by the tier being shared. `/v1/assignments` already belongs to dispatch and is called in production by the driver app (`PUT /v1/assignments/:id/accept`); the gateway's resolver is a first-match-wins chain over one flat `/v1` namespace, so an unprefixed `/v1/assignments/offer` reaches dispatch and never arrives at field-ops. Arbitrating that by branch order would re-break silently the next time a branch is added above another.

The prefix is also stable under every gateway topology being considered — one gateway, per-product gateways on separate subdomains, or host-based routing — so it does not need revisiting when that decision lands. Under the per-product-subdomain plan the *product* prefixes become redundant and may be dropped; the field-ops prefix does not, because it is what makes the tier addressable identically from every product that consumes it.

Adding a product to the platform means adding one prefix at the gateway, not auditing the twenty existing branches for a collision.

## Alternatives Considered

### Alternative 1: Temporary documented exception — OmniDeliv calls LogisticOS directly
**Rejected.** Fastest to the hero flow and zero risk to production dispatch, but it is a known boundary breach with no forcing function to remove it. Boundary exceptions of this kind historically become permanent; the ADR-0009 authors wrote rule 2 specifically to prevent this shape.

### Alternative 2: OmniDeliv builds its own thin courier module
**Rejected.** No boundary violation and full independence, but it produces two courier systems, two driver-facing apps (or one confused one), and duplicated earnings logic — exactly the copy rule 4 forbids. It also makes the eventual convergence strictly harder than doing it now.

### Alternative 3: Full field-ops extraction (dispatch, ETA, navigation, earnings)
**Rejected for now.** Architecturally the endgame, but it would block OmniDeliv slice one entirely, and rule 3 argues against extracting capabilities with one consumer. Note that the prerequisite above narrows the gap: once courier identity is unambiguous, the remaining distance to full extraction is mostly repository swaps rather than data-model surgery. Revisit when a third field-ops product appears.

## References
- ADR-0009: Multi-Product Platform Gateway Topology
- ADR-0012: Schema-Isolated SQLx Migrations
- `docs/superpowers/specs/2026-08-06-omnideliv-ai-design.md` §3.1
