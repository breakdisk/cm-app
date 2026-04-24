# ADR-0014: Marketplace Backend Service — Aggregator with Dead-Man's Switch

**Status:** Proposed
**Date:** 2026-04-24
**Deciders:** Principal Architect, Senior Rust Engineer — Carrier Management, Senior Rust Engineer — Driver Operations, Product Manager — Partner & Carrier, Database Reliability Engineer

> **Load-bearing invariant.** No booking ever accepts against a vehicle whose operational preconditions (driver online, vehicle not in maintenance, listing window still valid) are not currently true at the moment of accept. Listings may surface that later become invalid; bookings may not.

---

## Context

The Marketplace is the broker surface of the platform — merchants browse partner vehicles offered for pickup, partners accept bookings, the agreed job enters dispatch. Today it exists only as a client-side fiction:

- `apps/{admin,merchant,partner}-portal/src/lib/api/marketplace-bus.ts` — a localStorage "bus" that propagates booking state across tabs on the same origin.
- `apps/*-portal/src/lib/api/marketplace.ts` — hardcoded mock listings and stats, no network call.

Every portal's marketplace page reads and writes this shared key. It satisfies the demo, but it exposes three real operational failures the moment the platform serves genuine traffic:

1. **Phantom listings.** A vehicle row appears in marketplace because the partner published it, even though that partner has zero drivers online. Booking accepts → nobody drives → customer refund.
2. **Zombie listings.** A vehicle is in the shop for brake replacement. It's "idle" (not driving) but not operational. No cross-check between `fleet.vehicles.status` and marketplace visibility.
3. **Stale listings.** Partner sets `idle_until = 6pm`. It's 8pm. Listing still shows. No server-side expiry.

### Why an aggregator, not a primary data owner

Every attribute that decides whether a listing is *bookable right now* lives in another service:

| Attribute | Source of truth |
|---|---|
| Driver online status + last ping | `driver-ops` (`drivers.status`, `driver_locations.recorded_at`) |
| Vehicle maintenance state | `fleet` (`vehicles.status`, maintenance events) |
| Partner onboarding state | `carrier` (`carriers.status`) + ADR-0013 partner scope |
| Current active route for a driver | `driver-ops` (`drivers.active_route_id`) |
| Geographic coverage / SLA | `carrier` (`carriers.rate_cards.coverage_zones`, `carriers.sla`) |

Owning a copy of these in a marketplace schema guarantees drift. The platform has already paid this cost once on `shipments.origin_lat/lng` vs `dispatch_queue.origin_lat/lng` (this session, commit `17bc8c1`). **Marketplace must not repeat that pattern.**

The marketplace service owns **two** tables. Everything else it reads live via cross-service calls and cached via Kafka events.

### Why a service at all (why not just validate at booking time from partner-portal)

- **Tenant-scoped visibility.** Admin needs to see every partner's listings regardless of which partner they authenticate as.
- **Atomicity at accept.** A booking accept must move a listing from `available` → `matched` atomically with creating a `marketplace_bookings` row. Client-only state machine can't enforce this across three portals.
- **Audit.** Every listing publish and booking accept is a financial event. Must live on durable storage with a signed event trail, not localStorage.
- **Event integration.** Dispatch needs to know a marketplace booking has been accepted so it can mint a shipment + route. That's a Kafka producer, which is a service concern.

---

## Decision

Ship `services/marketplace` as an **aggregator** with two owned entities and one active invalidator:

### The two owned entities

**`marketplace.vehicle_listings`** — The Offer.

```sql
CREATE TABLE marketplace.vehicle_listings (
  id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id             UUID NOT NULL,
  partner_id            UUID NOT NULL,          -- ADR-0013 partner scope
  vehicle_id            UUID NOT NULL,          -- FK to fleet.vehicles (cross-schema; not enforced as FK)
  size_class            TEXT NOT NULL CHECK (size_class IN ('motorcycle','sedan','van','l300','6wheeler','10wheeler','trailer')),
  max_weight_kg         NUMERIC(10,2) NOT NULL,
  base_price_cents      BIGINT NOT NULL,
  per_km_cents          BIGINT NOT NULL,
  service_area_label    TEXT NOT NULL,
  -- Advertised availability window
  idle_from             TIMESTAMPTZ NOT NULL,
  idle_until            TIMESTAMPTZ NOT NULL,
  -- Status lifecycle: available → matched → expired | withdrawn | suspended
  status                TEXT NOT NULL DEFAULT 'available'
                        CHECK (status IN ('available','matched','expired','withdrawn','suspended')),
  -- Suspended-by-dead-mans-switch reason, null unless status='suspended'
  suspended_reason      TEXT,
  suspended_at          TIMESTAMPTZ,
  created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_listings_partner_status ON marketplace.vehicle_listings(partner_id, status);
CREATE INDEX idx_listings_window         ON marketplace.vehicle_listings(idle_from, idle_until) WHERE status = 'available';
```

RLS per ADR-0013: partners see only their own rows; admin/tenant-scope sees everything within the tenant.

**`marketplace.bookings`** — The Contract.

```sql
CREATE TABLE marketplace.bookings (
  id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id             UUID NOT NULL,
  listing_id            UUID NOT NULL REFERENCES marketplace.vehicle_listings(id),
  partner_id            UUID NOT NULL,          -- denormalized from listing for RLS
  merchant_id           UUID,                   -- null = consumer booking (ADR-0013)
  consumer_contact      TEXT,                   -- masked until accept
  shipment_id           UUID,                   -- populated after dispatch mints shipment
  size_class            TEXT NOT NULL,
  cargo_weight_kg       NUMERIC(10,2) NOT NULL,
  pickup_label          TEXT NOT NULL,
  dropoff_label         TEXT NOT NULL,
  pickup_at             TIMESTAMPTZ NOT NULL,
  quoted_price_cents    BIGINT NOT NULL,
  status                TEXT NOT NULL DEFAULT 'pending'
                        CHECK (status IN ('pending','accepted','rejected','in_transit','delivered','cancelled','disputed')),
  accepted_at           TIMESTAMPTZ,
  rejected_reason       TEXT,
  created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_bookings_listing ON marketplace.bookings(listing_id);
CREATE INDEX idx_bookings_partner ON marketplace.bookings(partner_id, status);
CREATE INDEX idx_bookings_created ON marketplace.bookings(created_at DESC);
```

### The Dead-Man's Switch

A listing auto-suspends when any of the following holds. Evaluated on every read (cheap join) AND on a background tick every 60 s (sets `status='suspended'` + `suspended_reason`):

| Condition | `suspended_reason` | Source |
|---|---|---|
| Partner has no driver with `status='available'` and last ping within 5 min | `no_online_driver` | driver-ops `drivers` + `driver_locations` |
| `fleet.vehicles.status` for `vehicle_id` is `maintenance` or `damaged` | `vehicle_not_operational` | fleet |
| `NOW() > idle_until` | `window_expired` | self |
| `carrier.carriers.status` ≠ `active` for `partner_id` | `carrier_suspended` | carrier |

Suspended listings do NOT disappear from the admin view — ops sees *why* the listing is no longer bookable. They disappear from the merchant/consumer browse view.

"5 minutes" for the driver heartbeat matches the dispatch availability window already enforced in `services/dispatch/src/infrastructure/db/driver_avail_repo.rs`. Consistency matters — dispatch rejects drivers past 10 min for assignment, marketplace should be stricter (5 min) because the listing promises future availability.

### Validation at publish vs accept

**Publish (`POST /v1/marketplace/listings`)** — validates once:
- `vehicle_id` exists in `fleet.vehicles` and is `operational`
- Partner has ≥ 1 driver matched to the size class (fleet stores `min_size_class_certification` per driver eventually; for MVP: any available driver counts)
- `idle_from < idle_until` and `idle_until <= NOW() + 72h` (no forever-listings)

**Accept (`POST /v1/marketplace/bookings/:id/accept`)** — re-validates, because state may have changed between publish and accept:
- Listing is still `available` (not already matched or auto-suspended)
- All Dead-Man's-Switch conditions still pass
- Same transaction: flips listing → `matched`, booking → `accepted`, emits Kafka `marketplace.booking.accepted` event

Dispatch's marketplace consumer (`services/dispatch/src/infrastructure/messaging/shipment_consumer.rs` already has the hook; see the `marketplace_booking` branch added for ADR-0013 work) mints the shipment + route on that event.

### API surface (minimum viable)

```
POST   /v1/marketplace/listings                 create (partner scope)
GET    /v1/marketplace/listings                 list — filters: status, size_class, zone, partner_id (admin only)
GET    /v1/marketplace/listings/:id
PATCH  /v1/marketplace/listings/:id/withdraw    partner voluntarily removes
POST   /v1/marketplace/bookings                 create (merchant/consumer)
GET    /v1/marketplace/bookings                 list — filters: status, partner_id
GET    /v1/marketplace/bookings/:id
POST   /v1/marketplace/bookings/:id/accept      atomic match (partner scope)
POST   /v1/marketplace/bookings/:id/reject      partner declines
POST   /v1/marketplace/bookings/:id/cancel      merchant/consumer rescinds pre-accept

GET    /v1/marketplace/stats                    aggregate for admin (active_listings, idle_vehicles_next_6h, etc.)
```

### Event production

| Event | Trigger | Consumer |
|---|---|---|
| `marketplace.listing.published`    | successful POST /listings | admin-portal live roster, analytics |
| `marketplace.listing.suspended`    | DMS tick suspends | admin-portal, engagement (notify partner) |
| `marketplace.listing.withdrawn`    | partner PATCH /withdraw | admin-portal |
| `marketplace.booking.created`      | successful POST /bookings | partner-portal (incoming offer), engagement (notify partner) |
| `marketplace.booking.accepted`     | partner accepts | **dispatch** (mint shipment), engagement (notify merchant/consumer) |
| `marketplace.booking.rejected`     | partner rejects | merchant-portal (retry flow), analytics |

### What this service does NOT do

- **It does not store driver state.** Driver availability is fetched from driver-ops on publish validate, accept validate, and DMS tick.
- **It does not store vehicle state.** Fleet owns maintenance; marketplace reads it live.
- **It does not execute dispatch.** The accepted booking emits an event; dispatch consumes and mints a shipment + route in its own schema.
- **It does not handle payment settlement.** Payments owns that. Marketplace writes the quoted price at accept; payments reconciles on delivered.

### Migration path from marketplace-bus.ts

1. Ship the service (read-only endpoints first — list, get).
2. Swap admin-portal /marketplace reads onto live endpoints; writes still go to localStorage bus. Verify parity with the bus.
3. Swap merchant-portal + partner-portal writes onto the service.
4. Delete `marketplace-bus.ts` from all three portals.
5. Remove ADR-0013's "synthetic marketplace row in dispatch queue" workaround (commented in `apps/admin-portal/src/app/(dashboard)/dispatch/page.tsx`); real `marketplace.booking.accepted` event replaces it.

Each step is independently shippable; the demo doesn't break during the transition.

---

## Consequences

### Positive

- **Data integrity at booking time.** A booking cannot accept against a vehicle that isn't actually operable — the single biggest ops risk in the current demo.
- **Admin sees the why.** Suspended listings carry `suspended_reason`, so ops can fix the underlying cause (driver offline, vehicle in maintenance) rather than guess.
- **No schema drift.** Marketplace doesn't copy driver / vehicle / carrier data; it references. One source of truth per attribute.
- **Event-driven downstream.** Dispatch picks up accepted bookings the same way it picks up merchant-created shipments (via Kafka), keeping ADR-0006 invariant.
- **Partner fairness.** A partner with drivers offline stops surfacing in merchant browse until they recover, protecting the platform's NPS.

### Negative

- **Cross-service reads on every listing fetch.** Mitigated by: (a) caching driver availability in Redis with a 60s TTL refreshed by the DMS tick, (b) the DMS tick itself updating `status` so callers can filter `WHERE status='available'` without live joins.
- **Cache staleness window.** Between a driver going offline and the 60s DMS tick, a merchant could see and book a listing that's no longer valid. The accept-time re-validation catches this; worst case is a 500 on accept, not a bad dispatch. Acceptable.
- **New service = new port, new deployment, new observability surface.** Port 8018. Follows existing service scaffold (Cargo, bootstrap, health/ready, Kafka producer, Dockerfile). Baseline cost.
- **RLS complexity.** Partner-scoped rows under ADR-0013, which is still partially rolled out. Marketplace service must gate its partner_id RLS behind the same GUC contract. Delay risk if ADR-0013 rollout slips.

### Neutral

- **Marketplace is per-tenant.** No cross-tenant discovery. Deliberate — Tenant A's merchants don't see Tenant B's partners. Cross-tenant marketplace is a separate ADR if ever needed.
- **Consumer contact stays masked pre-accept.** Partners see anonymized consumer labels on pending bookings to prevent around-the-platform deals. Existing marketplace-bus already models this.

---

## Implementation sequencing

**Phase 0 (done before this ADR):** Shadow Marketplace — a read-only admin view that stitches partner + driver-ops data client-side. Proves the aggregation model and surfaces real capacity signals. Zero backend.

**Phase 1 (3 sessions):** Service scaffold + vehicle_listings table + publish/list/get endpoints + DMS tick. Shadow marketplace reads flip to the real endpoints.

**Phase 2 (2 sessions):** bookings table + create/accept/reject + Kafka events. Dispatch consumer hook for `marketplace.booking.accepted`. Partner + merchant portals swap writes.

**Phase 3 (1 session):** Observability — Grafana dashboard for DMS suspend rate, listing publish → accept match time, rejected booking reasons. Delete marketplace-bus.ts.

**Phase 4 (future):** Consumer-grade features — saved searches, push notifications on matching listings, rating/review of partners post-delivery.

---

## Open questions

1. **Over-list prevention.** A partner could list the same vehicle twice for overlapping windows. DB unique constraint on `(vehicle_id, tstzrange)`? Or application-level?
2. **Multi-booking per listing.** Can a 10wheeler serve two merchant pickups on the same trip? MVP says no (one listing → one booking); revisit if demand emerges.
3. **Price negotiation.** Merchant counters a quote. MVP: no, list price is take-it-or-leave-it. Phase 4 candidate.
4. **Partner over-rides the DMS.** Partner swears they'll be back in 3 min. Allow a `force_resume` endpoint? No — abuse risk outweighs convenience.

---

## References

- ADR-0006 Kafka event streaming topology — governs downstream dispatch integration.
- ADR-0008 RLS multi-tenancy strategy — baseline for scope isolation.
- ADR-0013 Partner scoping + Alliance model — defines `partner_id` scope and RLS extension used by this service.
- `services/dispatch/src/application/services/driver_assignment_service.rs` — precedent for "no available drivers" error style + 10 min driver freshness window.
- `apps/*-portal/src/lib/api/marketplace-bus.ts` — the current client-only implementation being replaced.
