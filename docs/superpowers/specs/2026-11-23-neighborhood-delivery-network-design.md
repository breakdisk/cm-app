# Neighborhood Delivery Network: OmniDeliv Vendors as Last-Mile Partners

**Status:** Draft, awaiting approval
**Date:** 2026-11-23
**Baseline:** `origin/master` at `bb8d9519`
**Decision record:** [ADR-0018](../../adr/0018-neighborhood-retailer-delivery-network.md)
**Revised:** 2026-11-23, with the owner's answers on API contracts, stop order, timers, totes and seals, territories, driver closure and geocoding

> **Load-bearing invariant.** Every parcel has exactly one accountable party at every moment, and that party changes only on a recorded two-sided scan. Money, liability, customer tracking and dispute handling are all derived from that record.

---

## 1. Summary

The platform sorts parcels at the hub into labelled route bags (A, B, C, D), one label per neighborhood. A platform driver takes the bags to enrolled OmniDeliv stores. Each store checks the bag in, gives the parcels to its own riders, and earns a fee for each completed delivery.

No new portal or mobile app is needed:

| Person | Surface |
|---|---|
| Store owner or manager | Partner Portal, new mobile-first **Store Workspace**, linked from the Merchant Portal Storefront |
| Store clerk | Android driver app, new **Store mode** for offline scanning |
| Store rider | Android driver app, existing delivery and POD flow |
| Hub sorter | Android driver app, existing hub feature with a new tote sort screen |
| Platform transfer driver | Android driver app, new **Tote run** screen |
| Ops and finance | Admin Portal, new **Neighborhood Network** section |
| Customer | Existing tracking page, showing the delivering store |

"Basket" already means a customer's cart in OmniDeliv. In code and data the physical container is a **tote**. Users may still see "basket" or "bag" in the interface.

---

## 2. Intent

**Why build this.** Riders are the largest variable cost in last mile, and density is what makes them affordable. A neighborhood store already has a location, staff, local knowledge, and often a rider. Giving that store a bag of nearby parcels turns idle capacity into delivery capacity, and gives OmniDeliv vendors a second revenue line that makes them less likely to leave.

**What success looks like.** In a pilot zone, store-delivered parcels match or beat the platform's own on-time and first-attempt rates at a lower cost per delivered parcel, with no unexplained parcel or cash loss, and participating stores want to keep doing it.

---

## 3. Goals, non-goals, anti-goals

### Goals

1. Enroll an OmniDeliv vendor as a delivery partner without creating a second business identity.
2. Sort parcels by neighborhood into sealed totes using a deterministic rule.
3. Record custody from hub to store to rider to customer, or back.
4. Let each store assign its own riders to multi-stop batches.
5. Pay stores from an auditable ledger, separate from their OmniDeliv goods revenue.
6. Fall back to platform drivers or a third-party carrier whenever a store cannot perform.

### Non-goals for this design

- Grocery or food fulfilment from central stock. Baskets contain **sealed LogisticOS parcels only**.
- Chilled, perishable, high-value, hazardous, or oversized goods, including Balikbayan boxes.
- Stores collecting first-mile pickups or acting as customer drop-off points.
- Partners across tenants. Vendor, carrier and hub must share a tenant. The alliance model stays with ADR-0013.
- Automatic bank payouts. Finance executes the first payouts manually and records them.
- AI-driven territory design or capacity prediction.

### Anti-goals: this fails even if it technically works when

- A store can see parcels, customers, or earnings belonging to anyone else.
- A store can harvest customer contact data or learn which merchants its neighbors buy from.
- A parcel or peso goes missing and the record cannot say who held it.
- One delivery gets paid twice, to both the store and a platform gig driver.
- A store that goes quiet leaves customers waiting with no automatic fallback.

---

## 4. What exists today

Findings below come from reading `origin/master`. They were not reproduced on the live system.

### Reusable

| Capability | Where |
|---|---|
| Carrier identity, KYB status, SLA records, performance grade | `services/carrier` |
| Rider identity linked to a carrier | `driver_ops.drivers.carrier_id` |
| Rider tasks, delivery, POD, PIN, GPS, offline sync | `services/driver-ops`, `services/pod`, Android app |
| Sealable container lifecycle to model the tote on | `services/hub-ops/src/domain/entities/pallet.rs` |
| Append-only hub scans | `hub_ops` scan domain, migration 0007 |
| Vendor location, ownership and console | `services/omnideliv`, Merchant Portal Storefront |
| Partner Portal shell with carrier context | `apps/partner-portal` |

### Blocking gaps

| # | Gap | Evidence | Why it blocks |
|---|---|---|---|
| G1 | Partner accounts can read tenant-wide shipments | `order-intake/src/api/http/mod.rs:634`, `shipments_tenant_wide` | A store would see every parcel |
| G2 | Partner accounts can read the tenant wallet | `payments/src/api/http/wallet.rs` | A store would see company money |
| G3 | Manifest has no permission check and trusts a caller-supplied `carrier_id` | `driver-ops/src/api/http/tasks.rs:75` | Any caller can read any carrier's manifest |
| G4 | JWT has no partner identity | `libs/auth/src/claims.rs` | Nothing to scope on. `/v1/carriers/me` matches by email |
| G5 | **Finished work never frees a rider** | `AssignmentStatus` has no completed state and no code closes an `accepted` assignment. `uq_driver_active_assignment` then blocks that rider | A rider could only ever run one batch |
| G6 | One rider cannot receive a multi-stop batch | `quick_dispatch` allows one single-stop job. Route assignment emits no `TASK_ASSIGNED`, so the driver gets no tasks | Core delivery path is missing |
| G7 | Hub-out dispatch collides with first-mile state | The queue row is already `dispatched`, and `assignment_events.rs` would add a pickup leg | Reusing `quick_dispatch` would misroute drivers |
| G8 | Four automatic assigners can take any pending shipment | `shipment_consumer`, the 90-second sweep, `driver_available_consumer`, `hub_dispatch_consumer` | They would steal parcels bound for a store |
| G9 | Status flips to `out_for_delivery` on a dispatch *request* | `order-intake/.../status_consumer.rs` | A parcel sitting in a bag would show as out for delivery |
| G10 | Gig payout is added to dispatched tasks | `gig_rate_cents`, `tasks.payout_cents` | Risk of paying twice |
| G11 | Coordinates have no quality marker, and the geocoder accepts the first result of any type | `mapbox_geocoder.rs` reads only `features/0/geometry` | A city-level point would send a parcel to the wrong store |
| G12 | No territory, tote, store ledger, or COD liability per store | Absent | Core of this design |
| G13 | Auto routing fallback passes `carrier_id: None` and the carrier consumer skips it | `hub_dispatch_consumer.rs:125`, `hub_carrier_consumer.rs` | The fallback path silently does nothing |
| G14 | `DELIVERY_FAILED` makes a shipment terminal, and nothing produces `DELIVERY_ATTEMPTED` | `status_consumer.rs:189, 244`, `task_service.rs:316, 480` | A store's second attempt would be blocked, and the customer would see a final failure |
| G15 | `COD_COLLECTED` carries payments' `CodReconciled` shape, not the shared `CodCollected` payload | `payments/src/domain/events/mod.rs:40`, `cod_service.rs:95` | A consumer built on the shared type would misread cash events |
| G16 | Cancelling a route leaves its assignment active | `cancel_route` updates only `dispatch.routes` | The rider stays blocked after the route is cancelled |
| G17 | The booking API cannot accept a customer location pin | `AddressInput` has no coordinates | Poorly geocoded addresses have no way to be corrected |
| G18 | No translation framework exists | No i18n package in any portal, and no `values-*/strings.xml` in the driver app | New screens cannot meet the multi-language requirement without it |
| G19 | Some money responses hard-code `PHP` | `wallet.rs:124`, `partner_commission.rs:52` | A UAE store would see the wrong currency if this is copied |

### External blockers

- `cargomarket.net` is on registrar `clientHold`, so the apps are unreachable until that is lifted.
- PIN delivery needs Twilio. Without it, non-app deliveries are refused.
- Mapbox's Geocoding documentation says temporary results may not be cached, and permanent results need `permanent=true` plus a card on file or an enterprise contract. Our geocoder does not send that flag, yet coordinates are stored on every shipment. The Mapbox account terms need confirming before more provider data is stored.

PR #148 (field-ops exceptions) is **not** a prerequisite. Store riders are `driver_ops` drivers, whose failed-delivery flow already exists.

---

## 5. Approaches considered

| | A. Vendor linked to carrier (**chosen**) | B. New vendor-delivery domain inside OmniDeliv | C. Vendors as field-ops couriers |
|---|---|---|---|
| How | Each vendor gets a linked carrier profile. Hub-ops owns totes. Dispatch, driver app, POD and SLA are reused | OmniDeliv gains riders, routes, POD and custody | Each store is a courier on the field-ops tier |
| Strength | Reuses the logistics stack and KYB. Works for non-OmniDeliv retailers later | Keeps everything in one product | Least new routing |
| Weakness | Carrier data scoping must be fixed first | Rebuilds dispatch, POD, COD and SLA a second time | One live claim per courier, no owner, and no batch or store-level liability |

The owner chose **A**. The link to the vendor uses an opaque `origin_product` and `origin_ref`, so a future non-OmniDeliv retailer is enrolled with data rather than a schema change.

---

## 6. Operating model

```text
 Merchant parcels
       │ first mile (existing)
       ▼
 ┌───────────┐   scan parcel: screen shows "B"
 │    HUB    │──▶ Tote B sealed ──▶ Transfer run (platform driver)
 └───────────┘                              │ two-sided scan
       ▲                                    ▼
       │ returns                    ┌───────────────┐
       │                            │  STORE (B)    │ clerk scans each piece in
       └────────────────────────────│               │ manager assigns batch
             next transfer run      └───────┬───────┘
                                            │ scan: released to rider
                                            ▼
                                     Store rider ──▶ Customer (PIN or photo POD)
                                            │
                                            └─ failed: back to store, retry or return
```

### Daily cycle

1. **Availability.** Before each wave's cutoff, the store confirms it is open and how many parcels it can take. **Silence means no.** An unanswered store receives nothing that wave.
2. **Sort.** Hub scans a parcel. The resolver returns a tote label or a fallback.
3. **Seal.** The tote gets a seal code and a piece count.
4. **Transfer.** A platform driver scans totes out, then hands them over at each store.
5. **Receive.** The clerk scans the seal and each piece. Mismatches open a discrepancy.
6. **Assign.** The manager puts parcels into a rider batch.
7. **Deliver.** The clerk or rider scans each parcel out. The rider delivers with POD.
8. **Close.** Failed parcels return to the store for a retry or go back on the next run.
9. **Settle.** Fees and any cash collected post to the store's ledger.

### Who is liable

| Stage | Accountable party | Transfer happens on |
|---|---|---|
| In hub | Hub | Driver scans tote out |
| In transit | Platform (transfer driver) | Clerk scans tote in |
| At store | Store, per received piece | Scan released to rider |
| With rider | Store | POD, or scan back into store |
| Returning | Store until the transfer driver scans it, then platform | Hub scans it in |

The platform stays accountable **to the customer and merchant** throughout. Store liability is a matter between the platform and the store.

---

## 7. Architecture

### Service ownership

| Service | Owns | Change |
|---|---|---|
| **identity** | Partner membership, `pid` claim, store roles | Extended |
| **omnideliv** | Vendor's opt-in request | Small addition |
| **carrier** | Store partner profile, rates, status | Extended |
| **hub-ops** | Territories, waves, totes, custody, transfer runs | **Main new domain** |
| **dispatch** | Partner delivery jobs, batch assignment, assignment close | Extended |
| **driver-ops** | Tasks, scoped manifest and roster | Scoped, minor additions |
| **order-intake** | Geocode quality, shipment scoping, status mapping | Extended |
| **payments** | Partner ledger, settlements, COD liability | Extended |
| **engagement** | Store alerts, customer "with your local store" messages | Templates and match arms |
| **ai-layer** | Read-only MCP tools | Added last |

No new microservice is created. No service reads another service's new tables.

### Enrollment

```text
Vendor opts in (Merchant Portal Storefront)
  → omnideliv emits logisticos.omnideliv.vendor.partner_requested
  → carrier creates Carrier (pending_verification) + store profile
      idempotent on (tenant_id, origin_product, origin_ref)
  → admin reviews in the existing carrier approval flow
  → identity creates partner membership, grants store_manager
  → carrier emits logisticos.partner.store.updated
  → hub-ops updates its local projection
```

A vendor whose `tenant_id` differs from the hub's is refused.

### Identity and access

```sql
CREATE TABLE identity.partner_memberships (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL,
    user_id     UUID NOT NULL,
    carrier_id  UUID NOT NULL,
    role        TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'active',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, user_id, carrier_id)
);
```

- `Claims` gains `#[serde(default)] pid: Option<Uuid>`, set at token issue from an active membership.
- Scoping is enforced in repositories at the application layer, in line with ADR-0016. This narrows ADR-0013 and does not use its RLS mechanics.
- **A partner-role token without `pid` is denied on partner-scoped routes.** It never falls back to tenant-wide data.

| New role | Permissions |
|---|---|
| `store_manager` | `totes:read`, `totes:receive`, `store_riders:manage`, `store_batches:assign`, `store_ledger:read` |
| `store_clerk` | `totes:read`, `totes:receive`, `totes:release` |
| Rider | Existing `driver` role with `drivers.carrier_id` set |

Hub operators gain `totes:sort`, and transfer drivers gain `totes:carry`. `hub_scanner` is deliberately **not** reused, because it holds tenant-wide `shipments:update`. New roles must be added to `rbac.rs` and to the identity role-assignment path (`ASSIGNABLE_ROLES` in `users.rs`).

**Rollout order for scoping:** ship the claim, backfill memberships for existing partners, wait one full token lifetime, then enforce. Enforcing first would lock out every current partner.

### Carrier: store profile

```sql
CREATE TABLE carrier.store_partner_profiles (
    carrier_id        UUID PRIMARY KEY REFERENCES carrier.carriers(id),
    tenant_id         UUID NOT NULL,
    origin_product    TEXT NOT NULL,      -- opaque, e.g. 'omnideliv'
    origin_ref        UUID NOT NULL,      -- opaque, the vendor id
    status            TEXT NOT NULL,      -- applied, active, paused, suspended, offboarded
    lat               DOUBLE PRECISION NOT NULL,
    lng               DOUBLE PRECISION NOT NULL,
    daily_capacity    INT  NOT NULL,
    max_weight_g      INT  NOT NULL,
    max_longest_cm    INT  NOT NULL,
    cod_enabled       BOOLEAN NOT NULL DEFAULT FALSE,
    cod_cap_cents     BIGINT  NOT NULL DEFAULT 0,
    operating_hours   JSONB   NOT NULL DEFAULT '{}',
    UNIQUE (tenant_id, origin_product, origin_ref)
);

CREATE TABLE carrier.store_partner_rates (   -- append-only, a new row per change
    id                        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id                 UUID NOT NULL,
    carrier_id                UUID NOT NULL,
    territory_id              UUID,   -- NULL = this store's default rate
    delivery_fee_cents        BIGINT NOT NULL,
    failed_attempt_fee_cents  BIGINT NOT NULL,
    currency                  TEXT   NOT NULL,
    effective_from            TIMESTAMPTZ NOT NULL
);
```

The existing `RateCard` is not reused because it has no failed-attempt price and no version to snapshot. A territory-specific rate wins over the store default. The Admin territory editor can set fees and hours, but it writes them through carrier, which remains their owner. Currency comes from the tenant (`Claims.currency`) and is never hard-coded. The first pilot tenant settles in **AED**.

### Hub-ops: territories

```sql
CREATE TABLE hub_ops.delivery_territories (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL,
    hub_id      UUID NOT NULL,
    name        TEXT NOT NULL,          -- 'Khalifa City'
    label       TEXT NOT NULL,          -- 'A', 'B', ...
    version     INT  NOT NULL,
    boundary    geometry(MultiPolygon, 4326) NOT NULL,
    status      TEXT NOT NULL,          -- draft, published, retired
    effective_from  TIMESTAMPTZ,
    effective_until TIMESTAMPTZ,
    UNIQUE (tenant_id, hub_id, label, version),
    CHECK (ST_IsValid(boundary))
);
CREATE INDEX ON hub_ops.delivery_territories USING GIST (boundary);

CREATE TABLE hub_ops.territory_partner_assignments (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    territory_id    UUID NOT NULL,
    carrier_id      UUID NOT NULL,
    priority        SMALLINT NOT NULL,  -- 1 primary, 2+ backups
    effective_from  TIMESTAMPTZ NOT NULL,
    effective_until TIMESTAMPTZ
);
```

Rules:

- The column is `geometry`, not `geography`, because `ST_Relate` does not accept `geography`. The API exchanges GeoJSON in WGS84 (EPSG:4326).
- Published territories within a hub **must not overlap in area**. Publishing checks this inside a per-hub advisory lock using `ST_Relate(a, b, '2********')`. Shared edges are allowed. The editor shows overlaps and coverage gaps before publishing.
- Within one territory, two assignments cannot hold the same priority over overlapping dates. The publish step enforces this under the same lock.
- A point on a shared edge resolves to the lowest label, so the result is deterministic.
- Every tote piece records the territory id and version used. Editing a territory never rewrites past allocations.

**Editor.** Use **Terra Draw** on the **existing Mapbox GL** stack through `terra-draw-mapbox-gl-adapter`. All four map-using portals already run `mapbox-gl`, and the design system specifies Mapbox Dark. MapLibre is not adopted because it would add a second map stack for one screen. The adapter needs `mapbox-gl ^3.9.2`. The lockfile has 3.20.0, so only the `package.json` range needs raising. Terra Draw also has a MapLibre adapter, so a future portal-wide move stays cheap.
- The Postgres image includes PostGIS, but hub-ops has never used it. Its migration must create the extension in `svc_hub_ops` and be tested on a disposable database before deployment.

### Sort resolver

One pure function, called by both sort entry points (`/v1/inductions/:id/sort` and `deconsolidate → fan_out_routing`):

```text
resolve(parcel, wave) → Tote(label, carrier) | Fallback(reason)

 1. Network enabled for this hub?                  else Fallback(not_enabled)
 2. Location quality acceptable (see Address
    quality)?                                      else Fallback(bad_location)
 3. Parcel eligible (size, weight, value, type)?   else Fallback(ineligible)
 4. Inside a published territory?                  else Fallback(no_territory)
 5. Scanned before this wave's sort cutoff?        else use the next wave
 6. SLA achievable through that wave?              else Fallback(sla_risk)
 7. For each partner in priority order:
      active, confirmed for wave, capacity left,
      COD within cap (if the parcel has COD)?      → Tote
 8.                                                Fallback(no_partner)
```

Every result is recorded with its reason. **Fallback** continues through the existing `HubRoutingConfig` path, unchanged. Partner capacity is reserved **at allocation** with a conditional update, so two sorters cannot overfill a store, and is released if the parcel is removed.

Parcel facts come from a new internal order-intake read at scan time: coordinates, location quality, weight, dimensions, service type, declared value and COD. If it is unavailable the parcel is held, the same as when dispatch is down.

### Hub-ops: waves, totes, custody

```sql
CREATE TABLE hub_ops.sort_waves (
    id UUID PRIMARY KEY, tenant_id UUID NOT NULL, hub_id UUID NOT NULL,
    availability_cutoff_at TIMESTAMPTZ NOT NULL,
    sort_cutoff_at TIMESTAMPTZ NOT NULL,   -- parcels scanned later join the next wave
    seal_deadline_at TIMESTAMPTZ NOT NULL,
    depart_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL
);

CREATE TABLE hub_ops.wave_partner_capacity (
    wave_id UUID NOT NULL, carrier_id UUID NOT NULL,
    confirmed_capacity INT NOT NULL, reserved INT NOT NULL DEFAULT 0,
    reserved_cod_cents BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (wave_id, carrier_id),
    CHECK (reserved <= confirmed_capacity)
);

CREATE TABLE hub_ops.tote_assets (       -- the permanent physical container
    id UUID PRIMARY KEY, tenant_id UUID NOT NULL, hub_id UUID NOT NULL,
    asset_code TEXT NOT NULL,            -- 'TOTE-000123', encoded in its QR
    max_weight_g INT NOT NULL,
    max_volume_cm3 BIGINT NOT NULL,
    max_longest_cm INT NOT NULL,
    status TEXT NOT NULL,                -- in_service, lost, retired
    UNIQUE (tenant_id, asset_code)
);

CREATE TABLE hub_ops.totes (             -- one trip of one asset
    id UUID PRIMARY KEY, tenant_id UUID NOT NULL, hub_id UUID NOT NULL,
    tote_asset_id UUID NOT NULL REFERENCES hub_ops.tote_assets(id),
    wave_id UUID NOT NULL, territory_id UUID NOT NULL, carrier_id UUID NOT NULL,
    trip_code TEXT NOT NULL,             -- 'BSK-2026-00123'
    label TEXT NOT NULL,                 -- 'B-2'
    status TEXT NOT NULL,
    piece_count INT NOT NULL DEFAULT 0,
    total_weight_g BIGINT NOT NULL DEFAULT 0,
    total_volume_cm3 BIGINT NOT NULL DEFAULT 0,
    label_print_count INT NOT NULL DEFAULT 0,
    version INT NOT NULL DEFAULT 1,
    UNIQUE (tenant_id, trip_code)
);
CREATE UNIQUE INDEX uq_one_active_trip_per_asset ON hub_ops.totes (tote_asset_id)
    WHERE status NOT IN ('reconciled', 'cancelled');

CREATE TABLE hub_ops.security_seals (
    id UUID PRIMARY KEY, tenant_id UUID NOT NULL, hub_id UUID NOT NULL,
    serial TEXT NOT NULL,
    status TEXT NOT NULL,                -- available, issued, assigned, applied, consumed, voided
    tote_id UUID UNIQUE,                 -- single use: a seal serves one trip, ever
    issued_to UUID,
    UNIQUE (tenant_id, serial)
);

CREATE TABLE hub_ops.seal_events (       -- append-only
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL, seal_id UUID NOT NULL, tote_id UUID,
    event_type TEXT NOT NULL,            -- issued, assigned, applied, verified,
                                         -- mismatch, broken, damaged, missing, replaced, consumed
    actor_id UUID NOT NULL, client_ref TEXT NOT NULL,
    device_timestamp TIMESTAMPTZ,
    server_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL DEFAULT '{}',
    UNIQUE (seal_id, client_ref)
);

CREATE TABLE hub_ops.tote_pieces (
    id UUID PRIMARY KEY, tenant_id UUID NOT NULL, tote_id UUID NOT NULL,
    shipment_id UUID NOT NULL, awb TEXT NOT NULL,
    territory_id UUID NOT NULL, territory_version INT NOT NULL,
    status TEXT NOT NULL
);
CREATE UNIQUE INDEX uq_one_open_tote_piece ON hub_ops.tote_pieces (shipment_id)
    WHERE status NOT IN ('delivered', 'returned_to_hub', 'cancelled');

CREATE TABLE hub_ops.tote_custody_events (   -- append-only
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL, tote_id UUID NOT NULL, shipment_id UUID,
    event_type TEXT NOT NULL,
    from_party TEXT NOT NULL, to_party TEXT NOT NULL,
    actor_id UUID NOT NULL, client_ref TEXT NOT NULL,
    device_timestamp TIMESTAMPTZ,
    server_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    lat DOUBLE PRECISION, lng DOUBLE PRECISION,
    payload JSONB NOT NULL DEFAULT '{}',
    PRIMARY KEY (id),
    UNIQUE (tote_id, client_ref)
);
```

```text
Tote:   open → sealed → in_transit → at_store → received ─────────┐
                  │          │          └────→ received_short ────┤
                  │          └→ returned_undelivered              ├→ reconciled
                  └→ cancelled                                    │
Piece:  assigned → in_transit → received → with_rider → delivered ┘
                                   ▲            │
                                   └─ failed ◀──┘ → return_pending → returned_to_hub
        any held state → missing | damaged (opens a case)
```

- Event types are a Rust enum on a `TEXT` column, so adding one needs no DDL.
- `client_ref` makes offline replays harmless. `REVOKE UPDATE, DELETE` on custody events for the app role.
- The clerk **scans every piece**. An intact seal alone is not evidence of what is inside.
- A discrepancy is a claim, not a verdict. It opens a case and moves no money until ops resolves it.

**Tote capacity.** A tote fills against weight, volume and longest side, never count alone. Parcels without dimensions (they are optional on a shipment) use a configured default volume and are flagged at sort.

**Labels.** Two labels, because a reusable tote changes destination every trip:

| Label | Lifetime | Contents |
|---|---|---|
| **Asset label** | Permanent, on the tote | Platform, `TOTE-000123`, QR of that id |
| **Trip label** | One trip, in a sleeve or replaceable | `BSK-2026-00123`, large label such as **B-2**, partner code, zone, QR, readable code |

No customer name, phone or address appears on either. Stock is 100 × 150 mm synthetic with durable ribbon. The trip label prints when the tote is opened and reprints are counted. For the pilot, print from the Admin Portal to a desktop thermal printer using the `react-qr-code` print-sheet pattern in `table-print-sheet.tsx`. ZPL and industrial printers come later, when volume justifies them.

**Sealing at the hub**

1. A supervisor issues a run of serialized seals to the hub or sorter.
2. The sorter scans the tote, then scans an unused seal. The system checks authorization and assigns the seal.
3. The sorter applies the seal and confirms with a scan of both.
4. The tote becomes `sealed`, and the seal event and custody event are recorded together.

**Receiving at the store**

1. The clerk scans the tote and the seal, and the system compares it with the expected serial.
2. The clerk confirms the seal's condition.
3. **Match:** the seal is consumed, then the clerk scans every piece.
4. **Mismatch or broken seal:** the system records a seal exception with a photo. The store may accept the tote with the exception or refuse it, and in both cases the platform stays liable for what cannot be proven.

A seal history is never overwritten. A replacement seal is a new seal plus a `replaced` event.

**Transfer consolidation.** One transfer run may carry totes for several stores, as `transfer_runs` with ordered `transfer_run_stops`. Each stop has its own handover, and a store sees only its own totes.

### Dispatch: partner delivery jobs

A separate table keeps store-bound parcels out of every automatic assigner **by construction**. Filtering the existing queue would need four guards, and a fifth assigner added later would silently steal parcels.

```sql
CREATE TABLE dispatch.partner_delivery_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL, shipment_id UUID NOT NULL,
    carrier_id UUID NOT NULL, tote_id UUID NOT NULL,
    status TEXT NOT NULL,        -- reserved, at_store, batched, out_for_delivery,
                                 -- delivered, failed, return_pending, returned, cancelled
    attempt_no INT NOT NULL DEFAULT 0,
    rider_user_id UUID, route_id UUID,
    rate_id UUID NOT NULL,
    delivery_fee_cents BIGINT NOT NULL,
    failed_attempt_fee_cents BIGINT NOT NULL,
    cod_amount_cents BIGINT NOT NULL DEFAULT 0,
    dest_lat DOUBLE PRECISION NOT NULL,
    dest_lng DOUBLE PRECISION NOT NULL,
    max_attempts SMALLINT NOT NULL DEFAULT 2,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE UNIQUE INDEX uq_one_open_partner_job ON dispatch.partner_delivery_jobs (shipment_id)
    WHERE status NOT IN ('delivered', 'returned', 'cancelled');
```

**Reservation** is a synchronous internal call from hub-ops, because the sorter needs an immediate answer before the parcel goes in the bag:

```text
POST /v1/internal/partner-jobs/reserve   (service token; /v1/internal is blocked at the gateway)

in one transaction:
  lock the queue row if there is one          (no row is legal, e.g. cross-border)
  refuse if any open driver task exists for this shipment
  set queue status → 'partner_fulfilment'
  insert the job with the rate snapshot
```

If dispatch is unavailable, the sorter sees "hold this parcel" and it goes to the exception cage. `quick_dispatch` must claim its row with a conditional `status = 'pending'` update so it cannot race a reservation.

**Batch assignment:**

```text
POST /v1/partner-delivery/batches  { rider_user_id, job_ids[] }

  every job belongs to claims.pid and is at_store or failed-and-retryable
  rider's drivers.carrier_id = claims.pid          (keyed on user_id, per ADR-0015)
  rider has no active assignment
  → one route, N stops, one assignment
  → TASK_ASSIGNED per stop, task_type = delivery, payout = 0
```

This path never calls `assignment_events` for a pickup leg and never computes `gig_rate_cents`. The task snapshot must be an explicit `payout_cents = 0`. `NULL` is wrong, because it means "full-time" and `to_summary` falls through to another payout value. The route is new on the gateway and must be added to `resolve_upstream`.

**Stop order.** Dispatch gets a `StopSequencer` trait with a nearest-neighbor implementation for the pilot, so a VRP solver can replace it later without changing the delivery module.

- Start from the store. The existing `Route::sort_stops_by_proximity` is not reused, because it starts from the first stop, uses straight-line distance, and treats missing coordinates as `(0, 0)`.
- Use road distance from the Mapbox Matrix API with the `driving` profile. Its limit of 25 coordinates sets a **maximum of 24 stops per batch**. Its limit of 60 requests per minute means it is called only when a batch is created or re-sequenced.
- If the Matrix call fails, order by straight-line distance and record `distance_basis = direct`. A routing outage must not stop deliveries.
- `route_stops` already has time-window columns, but bookings do not capture windows yet. The sequencer honors a window when one is present.
- A store manager or authorized dispatcher may reorder stops. The stop a rider is already heading to (`in_progress`) is locked unless dispatch overrides it.
- Every sequence is kept in append-only `dispatch.route_sequence_history`, with its source, algorithm and actor.

**Retries.** `TaskAssigned` gains `#[serde(default)] partner_retry_allowed: bool`, true while `attempt_no < max_attempts`.

| Rider reports a failed stop | Event | Shipment status | Parcel |
|---|---|---|---|
| Attempts remain | `DELIVERY_ATTEMPTED` (new producer in driver-ops) | `delivery_attempted` | Back to store, can be re-batched |
| Final attempt | `DELIVERY_FAILED` | `failed` | `return_pending` |
| Any non-partner task | `DELIVERY_FAILED`, unchanged | `failed` | Existing behavior |

The new event must match `DeliveryAttemptedEvt` in `status_consumer.rs`, and carries the reason code, GPS and `device_timestamp`. Dispatch checks the GPS against the job's destination using the existing 200 m geofence. If GPS is missing or outside it, no fee posts automatically and the attempt goes to ops review.

**Closing assignments (fixes G5 and G16 for all drivers).** Add a `completed` status. When a task reaches a terminal state, close the assignment and route in one conditional transaction if no open task remains, reusing the predicate in `dispatch_queue_repo.rs`. `cancel_route` must close its assignment too.

| Phase 0 question | Answer from `origin/master` |
|---|---|
| Does `drivers.active_route_id` exist? | Yes, but **nothing writes it**. `assign_route` and `clear_route` have no callers |
| Can a driver hold several active assignments? | No. A unique partial index allows one |
| Is driver status independent of route status? | Yes. Assignment does not change `drivers.status` |
| What if a driver goes offline mid-route? | It is allowed, because the guard depends on the unused column. Tasks stay open and the rider-inactive timer catches it |
| What if a route is cancelled? | The route closes but the assignment stays active (G16) |
| Can a driver get another route before closing? | No. The assignment row is the real busy signal |

So closing the assignment is what frees the rider, and there is no status to recalculate. Do **not** start writing `active_route_id` as part of this fix, because `go_offline` and `set_status` would begin refusing riders. No new driver statuses are added: `is_active = false` already covers suspended, and `on_break` and `returning` exist. If the column is wired later, clear it conditionally with `WHERE user_id = $1 AND active_route_id = $2`, keyed on `user_id` per ADR-0015.

> **Rollout trap.** `parse_status` maps any unknown string to `Pending`. An old dispatch image reading a `completed` row would treat it as active. Ship a reader that understands `completed` first, then start writing it.

**Cancellation and recall.** A cancelled shipment cancels the job and any rider task. The physical parcel becomes `return_pending`. A recalled parcel returns to the automatic queue **only after a platform party has scanned it into custody**. Software never reassigns a parcel that a store is still holding.

### Payments: partner ledger

```sql
CREATE TABLE payments.partner_ledger_entries (   -- append-only
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL, carrier_id UUID NOT NULL,
    entry_type TEXT NOT NULL,
    bucket TEXT NOT NULL,          -- earnings | cod_held
    amount_cents BIGINT NOT NULL,  -- signed
    currency TEXT NOT NULL,
    shipment_id UUID, tote_id UUID, attempt_no INT,
    settlement_id UUID,
    idempotency_key TEXT NOT NULL UNIQUE,
    source_event_id UUID,
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

| Entry | Bucket | Trigger | Idempotency key |
|---|---|---|---|
| `delivery_fee` | earnings + | Job delivered | `fee:{shipment}` |
| `failed_attempt_fee` | earnings + | Valid failed attempt | `failfee:{shipment}:{attempt}` |
| `cod_collected` | cod_held + | `reconcile_cod` on a shipment with a partner job | `cod:{shipment}` |
| `cod_remitted` | cod_held − | Finance confirms deposit | `remit:{reference}` |
| `shortage`, `damage`, `sla_penalty` | earnings − | **Ops resolves a case** | `case:{case_id}` |
| `adjustment` | either | Finance, with a reason | `adj:{id}` |
| `payout` | earnings − | Finance records a bank reference | `payout:{settlement}` |

Rules:

- One fee per delivered shipment, however many attempts it took.
- A failed attempt pays only with valid evidence: a reason code plus GPS inside the existing geofence.
- `earnings` and `cod_held` are reported separately. Netting COD against a payout must be an explicit line on the settlement.
- Payments keeps a local `partner_job_refs` projection from `logisticos.partner.job.changed`. `reconcile_cod` uses it to post cash to the store's ledger in the same transaction, so payments never needs to read dispatch tables.
- Store riders create **no** platform `earning_credits` and **no** `DriverLedger` COD debt. The store carries the cash liability, and pays its riders itself.
- A store at or over its COD cap receives no COD parcels until it remits.
- This ledger is separate from `vendor_ledger`, and neither nets against the other. The vendor sees two clear statements.
- The Partner Portal Payouts page moves from the tenant wallet to this ledger **before** partner `payments:read` is removed.

### Events

All topics must be added to `scripts/kafka/create-topics.sh` and `scripts/create-kafka-topics.sh`, and verified on the broker before a consumer depends on them.

| Topic | Producer | Consumers |
|---|---|---|
| `logisticos.omnideliv.vendor.partner_requested` | omnideliv | carrier |
| `logisticos.partner.store.updated` | carrier | hub-ops, dispatch |
| `logisticos.hub.tote.dispatched` | hub-ops | dispatch, engagement |
| `logisticos.hub.tote.received` | hub-ops | dispatch, engagement, delivery-experience |
| `logisticos.hub.tote.piece_released` | hub-ops | order-intake (→ `out_for_delivery`), dispatch, delivery-experience |
| `logisticos.hub.tote.piece_returned` | hub-ops | order-intake (`failed` → `returned`), dispatch |
| `logisticos.partner.job.changed` | dispatch | payments, carrier |
| `logisticos.partner.discrepancy.resolved` | hub-ops | payments |

`piece_released` is the only event that makes a store parcel `out_for_delivery`, which resolves G9. Its status arm denies terminal statuses and accepts `delivery_attempted`, so a retry works. The `piece_returned` arm explicitly allows `failed`, because the shared terminal guard would otherwise drop it. Tote-bound pieces skip `HUB_DISPATCH_REQUESTED` entirely. No new `ShipmentStatus` is added. Tote stages reach the customer as tracking milestones.

### API contract

Contract-first: new endpoints are added to the existing OpenAPI 3.1 files in `docs/api/openapi/` and reviewed before implementation. CI compares new routes against the contract, because at least the existing dispatch spec has drifted from its live route names.

| Convention | Rule |
|---|---|
| Prefix | `/v1/...`, the gateway's existing convention. `/api/v1` is not introduced |
| DTOs | Explicit request and response types. Database entities are never exposed |
| IDs and time | UUIDs, UTC ISO 8601 |
| Money | Integer `*_cents` plus an ISO 4217 `currency` from the tenant |
| Pagination | Existing `Pagination` and `PaginatedResponse<T>` |
| Errors | Existing `{ "error": { "code", "message" } }` from `AppError` |
| Idempotency | `Idempotency-Key` on financial and operational commands |
| Offline scans | `client_ref`, replay returns the original result |
| Concurrency | `expected_version` on tote, batch and assignment **commands**. Physical scans are not version-checked, because an offline clerk's scan is a fact that already happened |
| Tenancy and partner | Always from the JWT. `partner_id` is **never** accepted in a path or body from a store |
| Clients | Generated TypeScript types for the Next.js portals. Kotlin stays hand-written Retrofit with fixtures from the contract. The repo has no Flutter app |

| Service | Routes |
|---|---|
| hub-ops | `GET, POST /v1/territories`, `POST /v1/territories/{id}/publish` |
| hub-ops | `GET, POST /v1/sort-waves`, `PUT /v1/sort-waves/{id}/availability` |
| hub-ops | `POST /v1/totes/sort-scan`, `GET /v1/totes`, `GET /v1/totes/{id}`, `POST /v1/totes/{id}/seal` |
| hub-ops | `POST /v1/totes/{id}/receive`, `POST /v1/totes/{id}/pieces/{pieceId}/release`, `POST /v1/totes/{id}/discrepancies` |
| hub-ops | `POST /v1/seals/issue`, `POST /v1/transfer-runs`, `POST /v1/transfer-runs/{id}/load`, `POST /v1/transfer-runs/{id}/stops/{stopId}/handover` |
| carrier | `GET /v1/store-partners`, `GET /v1/store-partners/me`, `PATCH /v1/store-partners/{id}` (admin) |
| dispatch | `GET /v1/partner-delivery/jobs`, `POST /v1/partner-delivery/batches`, `PUT /v1/partner-delivery/batches/{id}/sequence` |
| payments | `GET /v1/partner-ledger/me`, `GET, POST /v1/partner-settlements` |
| Unchanged | POD stays on `/v1/pod`, shipments on `/v1/shipments` |

All new prefixes must be added to the gateway's `resolve_upstream`. There is deliberately **no** client endpoint to complete an assignment, because the server derives that from task outcomes.

### Operational timers

Timers are configuration, stored per owning service and evaluated by a background sweep. PostgreSQL holds the authoritative deadlines. `business-logic` is not used here, because its trigger entity is a stub and its scheduler ticks hourly.

```sql
CREATE TABLE hub_ops.operational_sla_rules (   -- same shape in dispatch
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    carrier_id UUID,              -- NULL = tenant default
    service_type TEXT,            -- NULL = any
    warning_minutes INT NOT NULL,
    escalation_minutes INT NOT NULL,
    critical_minutes INT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    effective_from TIMESTAMPTZ NOT NULL
);

CREATE TABLE hub_ops.sla_escalations (         -- append-only, same shape in dispatch
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL, rule_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    level TEXT NOT NULL,          -- warning, escalation, critical
    deadline_at TIMESTAMPTZ NOT NULL,
    raised_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (aggregate_type, aggregate_id, rule_id, level)
);
```

A small shared crate holds the pure decision function, following `leg_recovery::decide`. Proposed starting values, in minutes after the clock starts:

| Timer | Clock starts at | Warning | Escalation | Critical |
|---|---|---|---|---|
| Parcel not sorted | Wave sort cutoff | 5 | 10 | 20 |
| Tote not sealed | Wave seal deadline | 5 | 10 | 15 |
| Sealed tote not loaded | Sealed | 10 | 20 | 30 |
| Vehicle not departed | `depart_at` | 10 | 20 | 30 |
| Transfer late | Planned arrival | 10 | 20 | 30 |
| Store not accepting | Driver arrives | 5 | 10 | 15 |
| Parcels not assigned to a rider | Tote received | 30 | 60 | 90 |
| Rider inactive | Last GPS **or** scan | 10 | 20 | 30 |
| Delivery overdue | Promised time | 10 | 20 | 30 |

Rules:

- Parcels legitimately wait for a wave, so sort and seal clocks start at wave deadlines, not when the parcel arrives.
- Rider assignment is 30/60/90 because a store batches a whole bag. Rider inactivity is 10/20/30 because phones drop signal at doorsteps, and dispatch already treats a GPS fix as fresh for 10 minutes.
- Alert **per tote or batch**, not per parcel. One late bag of 30 is one alert.
- Timers pause outside store operating hours and stop on any resolved state.
- Level 1 notifies the responsible person, level 2 needs a supervisor's acknowledgment and action plan, and level 3 opens an exception and alerts the manager.
- **Recovery is human-confirmed.** A timer never reassigns a parcel a store is holding.
- The dashboard is the record and a push is a hint, because FCM credentials are not yet on the VPS.
- All timers run observe-only until the pilot produces a baseline.

### Address quality

```sql
-- destination columns on the order-intake shipments table
dest_geocoding_quality     TEXT NOT NULL DEFAULT 'UNKNOWN',
dest_geocoding_source      TEXT,          -- provider, customer_pin, ops_corrected
dest_geocoding_provider    TEXT,
dest_geocoding_place_id    TEXT,          -- mapbox_id
dest_geocoding_confidence  TEXT,          -- provider's own value, when it gives one
dest_geocoded_at           TIMESTAMPTZ
```

| Mapbox v6 result | Quality | Store network treatment |
|---|---|---|
| `address` + `rooftop` | `ROOFTOP` | Allocate |
| `address` + `point` | `BUILDING` | Allocate |
| `address` + `parcel` | `PARCEL` | Allocate |
| `address` + `interpolated` | `INTERPOLATED` | Needs a customer pin, otherwise fallback |
| `street`, or `intersection` | `STREET` | Needs a customer pin, otherwise fallback |
| `approximate`, neighborhood, locality or place | `LOCALITY` | Request a pin, fallback meanwhile |
| None or invalid | `UNKNOWN` | Fallback |

- `match_code.confidence` of `medium` caps quality at `INTERPOLATED`, and `low` caps it at `LOCALITY`.
- A confirmed customer pin (`dest_geocoding_source = customer_pin`) is accepted for allocation. `AddressInput` gains optional pin coordinates, plus optional building, unit and area fields for UAE addresses.
- **This gate applies only to the store resolver.** Every existing shipment starts as `UNKNOWN`, so blocking dispatch on quality would stop current deliveries.
- No SQL `CHECK` on quality. Values cross service boundaries, and a new level must be data. Any unrecognized value reads as `UNKNOWN`, which fails closed.
- Keep the provider's id, accuracy and confidence rather than its full response, which duplicates personal data. Storing any of it depends on the Mapbox `permanent` question in section 4.
- A quality rating does not prove the pin is the customer's entrance, so riders can still report a wrong location.

### Agent access (MCP)

Expose read-only tools first: `get_tote_status`, `get_partner_capacity`, `get_territory_performance`. Tools that move parcels or money stay off the remote MCP surface until per-tool RBAC exists, because any caller past the Enterprise gate can currently invoke any tool. All flows work without AI.

---

## 8. Surfaces

All new screens are dark-first, WCAG 2.1 AA, and checked at phone, tablet and desktop widths. All strings are externalized through a new translation layer (G18), with layouts that work right-to-left. Launch languages are listed in section 12.

### Store Workspace (Partner Portal, phone-first)

```text
┌──────────────────────────┐
│ Neighborhood Grocery     │
│ Wave 2 · confirm by 9:30 │
│ [ Yes, take up to 25 ]   │
├──────────────────────────┤
│ Basket B        18 of 18 │
│ ✓ Received 10:42         │
│ 6 to assign · 12 out     │
├──────────────────────────┤
│ Needs attention        2 │
│ Today's earnings AED ▒▒▒ │
│ Cash to remit    AED ▒▒▒ │
├──────────────────────────┤
│ Today Riders Cash Payouts│
└──────────────────────────┘
```

Screens: Today, Receive status, Assign batch, Riders, Exceptions, Cash, Earnings and statements, Settings.

### Android

- **Store mode** (clerk): today's totes, scan seal, scan pieces, report discrepancy, release to rider. Fully offline with the existing sync queue. `device_timestamp` is captured at the physical scan.
- **Rider:** existing task and POD flow, with batch stop order.
- **Tote run** (transfer driver): scan out at hub, stop list, handover at store, returns pickup.
- **Hub sort:** scan parcel, full-screen label such as **B**, or a clear fallback.

### Admin Portal: Neighborhood Network

Partner approvals, territory map editor (draw, name, assign hub and partners, set fees and hours, view overlaps and gaps, publish), wave board, tote and seal tracker, label printing, discrepancy queue, escalation queue, partner scorecards, settlement run.

### What a store may see

| Data | Clerk | Assigned rider | Manager |
|---|---|---|---|
| AWB, tote label | Yes | Yes | Yes |
| Recipient name and address | No | Active stops only | Active jobs only |
| Recipient phone | No | Active stops only | No |
| COD amount | At release | Yes | Yes |
| Merchant name, contents, declared value | **No** | **No** | **No** |

Contact details disappear once the job closes, and access to them is audit-logged. Before a UAE pilot, Compliance and Legal Counsel need to review this data sharing under the UAE's data protection law, and confirm what licensing or permits store riders need in that emirate.

---

## 9. Failure modes

| What goes wrong | System response |
|---|---|
| Store does not confirm availability | Nothing sent. Backup, then fallback |
| Store closed at drop-off | Driver records it. Tote stays in platform custody and returns |
| Bag arrives short | Discrepancy case. Store is not liable for unreceived pieces |
| Store receives but never assigns | Alert, then ops escalation, then physical recall. Never auto-rejected |
| Rider disappears mid-route | Manager reassigns. Store stays liable until a scan changes custody |
| Customer not home | Failed attempt with evidence. Retry or return |
| Same scan replayed offline | `client_ref` returns the original result |
| Dispatch down during sorting | Parcel held in the exception cage, not bagged |
| Order cancelled while at store | Job cancelled, parcel returned on next run |
| Store over COD cap | No new COD parcels |
| Bad geocode | Fallback, never a guessed store |
| Store suspended with parcels in hand | No new work. Open jobs finish or are recalled |
| Territory edited mid-day | Existing totes keep the version they were sorted under |
| Kafka event delivered twice | Idempotency key blocks duplicate money or state changes |
| Seal does not match or is broken | Seal exception with photo. Platform stays liable for what cannot be proven |
| Tote full | Sorter is told to open another tote for the same label |
| Parcel misses the wave cutoff | Joins the next wave, or falls back if its SLA would be missed |
| Location is only neighborhood-level | Customer is asked for a pin. Parcel uses fallback meanwhile |
| Matrix routing unavailable | Stops are ordered by straight-line distance and marked as such |

### Cancellations and returns: who is responsible

| When it happens | Physical responsibility | Store fee | Loss or damage |
|---|---|---|---|
| Before sealing | Hub | None | Platform |
| Sealed or in transit | Platform | None | Platform |
| Received, not yet with a rider | Store until the driver collects it | None | Store |
| With rider, no attempt made | Store until scanned back | None | Store |
| After a valid failed attempt | Store until the driver collects it | Failed-attempt fee | Store |
| Collected for return | Platform | Unchanged | Platform |
| Delivered, later disputed | Closed by POD | Delivery fee stands unless a case reverses it | Decided by the case |

Return shipping cost stays with the platform toward the store. Whatever the merchant or customer is charged follows their existing terms.

---

## 10. Delivery phases

Phases end on **gates, not dates**. Phase 0 is valuable by itself and fixes live weaknesses.

### Phase 0: Make partner access and dispatch safe

OpenAPI contracts for Phase 0 changes; partner membership and `pid`; scoping for shipments, wallet, manifest, roster and scans; assignment and route close; Auto fallback fix; geocoding quality and customer pin; translation layer.

**Gate:** tests prove a partner token cannot read another partner's shipments, wallet, manifest, or riders. A rider can complete a job and receive another. The domain is reachable and PIN delivery works.

### Phase 1: Enrollment, territories, shadow mode

Opt-in, approval, profiles, rates, territory editor, resolver running in **shadow mode** on real parcels. Nothing physically changes.

**Gate:** shadow data shows enough eligible parcels per territory per day to meet the economics in section 11.

### Phase 2: Totes and custody

Waves, sort screen, tote assets, trip labels, seal inventory, transfer runs, store receipt, discrepancy cases, and hub timers in observe-only mode, using an internal test store.

**Gate:** a full tote cycle, including a short bag, a mismatched seal and an offline replay, reconciles to zero unexplained pieces.

### Phase 3: Store delivery

Partner jobs, batch assignment, stop sequencer with manual reorder, store and rider flows, failure and return paths, and dispatch timers in observe-only mode.

**Gate:** end-to-end run on real devices, with proof that automatic assigners cannot take a reserved parcel and that a store rider earns no platform gig pay.

### Phase 4: Partner money

Ledger, statements, settlement run, COD liability and cap.

**Gate:** replaying every event twice yields identical balances, and each statement line traces to a custody event.

### Phase 5: Pilot

3 to 5 stores in one zone, against a comparable control zone, behind a per-hub flag. Prepaid parcels first.

**Gate:** section 11 pilot exit criteria.

### Phase 6: Expand

COD by store after a clean record, more zones, other retailers through `origin_product`, automated payouts, a VRP solver such as OR-Tools behind `StopSequencer`, and industrial label printing.

---

## 11. Economics and pilot measures

The network is worth running only if:

```text
store fee + (transfer run cost ÷ parcels per run) + failed-delivery cost + support cost
    < current cost per delivered parcel
```

Shadow mode supplies parcels per run. Finance supplies current cost per delivered parcel.

| Measure | Pilot exit target (proposed) |
|---|---|
| On-time delivery | At or above control zone |
| First-attempt success | At or above control zone |
| Cost per delivered parcel | Below control zone |
| Unexplained parcel loss | Zero open beyond 48 hours |
| COD variance (once enabled) | Zero unresolved beyond 48 hours |
| Fallback rate | Stable or falling |
| Store retention through the pilot | Every store willing to continue |
| Customer complaints per 1,000 | At or below control zone |

---

## 12. Decisions

### Made by the owner

- Baskets contain LogisticOS parcels.
- Vendors are linked to carrier profiles. Hub-ops owns baskets.
- The platform transfers baskets to stores.
- OpenAPI 3.1 contract-first, with explicit DTOs.
- Nearest-neighbor stop order for the pilot behind a replaceable engine, VRP later.
- Configurable three-level timers, with an immutable escalation history.
- Reusable QR totes, single-use serialized seals, and thermal labels.
- Versioned PostGIS territories with Terra Draw.
- Conditional driver closure, verified against the existing state machine.
- Normalized geocoding quality with a provider adapter.
- AED as the initial settlement currency.

### Where the owner's proposal was adapted to the codebase

| Proposal | Applied as | Why |
|---|---|---|
| `/api/v1/` | `/v1/` | The gateway and every client already use it |
| Flutter clients | TypeScript and Kotlin | The repo has no Flutter app |
| `/partners/{id}/accept-basket`, `partner_id` in DTOs | Partner taken from the JWT | A store must not be able to name another store |
| `POST /assignments/{id}/complete` | Derived on the server | A client must not be able to free a rider with open work |
| `/orders`, `/deliveries/{id}/proof` | Existing `/v1/shipments`, `/v1/pod` | Those services already own them |
| Timers from arrival | Anchored to wave deadlines | Parcels legitimately wait for a wave |
| Rider-assignment 5/10/15, rider-inactive 5/10/15 | 30/60/90 and 10/20/30 | Avoids constant false alarms |
| Destination on the durable tote label | Permanent asset label plus trip label | A reusable tote changes destination every trip |
| Six seal and label tables | Four tables | Sealing and audit events are one append-only log |
| MapLibre | Terra Draw on existing Mapbox GL | Avoids a second map stack |
| New driver statuses and status recalculation | Close the assignment only | Status is independent of assignment today |
| SQL `CHECK` on quality | Text with fail-closed parsing | A new level should be data |
| Block dispatch on `UNKNOWN` | Gate only the store network | Otherwise all current deliveries would stop |

### Calls made in this design, open to correction

- Physical container is a **tote** in code.
- Enrollment is opt-in, and silence on a wave means no parcels.
- One primary and optional backup per territory, with no overlapping territories.
- Piece-level receipt at the store.
- Two delivery attempts by the store, then return to hub.
- A separate partner-job table rather than flags on the dispatch queue.
- Store pays its own riders. The platform pays the store.
- Merchant identity and parcel contents are hidden from stores.

### Defaults awaiting the owner's confirmation

| # | Decision | Default in this design |
|---|---|---|
| 1 | COD in the pilot | **Prepaid only** at first, then COD store by store |
| 2 | Delivery fee and failed-attempt fee | Not set. Needs finance's current cost per parcel |
| 3 | Security deposit or bond | **No deposit** during prepaid. Required before COD is enabled |
| 4 | Mapbox permanent geocoding | **Enable `permanent=true`** once the account terms and cost are confirmed |
| 5 | Pilot languages | **English and Arabic** for customers, **English** for staff, with the translation layer ready for more |

---

## 13. Left to the builder

- Field-level DTOs, written in OpenAPI during each phase.
- Specific printer, label and seal suppliers.
- Default tote dimensions and default parcel volume, from the physical totes bought.
- Final timer values, tuned from observe-only data.
- Which Android screens share code between hub sort and store receipt.
