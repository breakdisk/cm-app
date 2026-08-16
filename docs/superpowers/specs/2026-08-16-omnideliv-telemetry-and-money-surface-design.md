# OmniDeliv — Live Telemetry, Status Map, and the Customer Money Surface

**Date:** 2026-08-16
**Status:** Approved section-by-section, ready for an implementation plan
**Scope:** `services/field-ops`, `services/omnideliv`, `apps/omnideliv-app`

---

## What triggered this

Two pending gaps were raised against the OmniDeliv customer app:

1. **Proactive live telemetry and status map** — live geospatial coordinates blended
   with predictive textual milestones and dynamic ETAs.
2. **Unified digital wallet and balance ledger** — a financial hub on the home
   canvas to manage funds, track payment rails, and apply incentives such as
   carbon offset options and agent consolidation bonuses.

Investigation found that (1) is a genuine wiring job whose cost is backend, not
UI, and that (2) as literally worded collides with a decision already recorded on
2026-08-11 and has no payment rail beneath it. The two were separated, and (2)
was rescoped to a read-only surface over money that already exists.

---

## Findings that shaped the design

These are the facts the design is built on. Each was verified against the code,
not assumed.

| Finding | Evidence |
|---|---|
| An order cannot reach a courier position. `Order.courier_task_id` is a field-ops **assignment** id; field-ops owns the courier and the location. Nothing joins them. | `services/omnideliv/src/domain/entities/order.rs:155` |
| field-ops can already answer "where is this courier" and exposes it nowhere. `LocationRepository::latest()` is implemented; `/v1/field-ops/couriers/:id/position` is **write-only**. | `services/field-ops/src/infrastructure/db/location_repo.rs:10`, `services/field-ops/src/api/http/couriers.rs:140` |
| Nothing computes an ETA at any layer. `speed_kph` and `heading_deg` are captured on `CourierLocation` and read by nobody. | `services/field-ops/src/domain/entities/location.rs` |
| `TrackResponse` carries no coordinates at all — not the courier, not the stops, not the destination. | `services/omnideliv/src/api/http/tracking.rs:23` |
| The app has **zero** native map dependencies. `mapbox-gl` exists only in the admin portal (web). | `apps/omnideliv-app/package.json` |
| The admin portal already solved graceful degradation: no token → canvas GPS plot, with a stated notice. | `apps/admin-portal/src/components/maps/live-dispatch-map.tsx:351` |
| `pollIntervalMs` — a status-aware poll schedule — has **zero call sites**. The track screen hardcodes `POLL_MS = 8000` instead. | `apps/omnideliv-app/src/api/tracking.ts:35` vs `apps/omnideliv-app/app/track/[id].tsx:20` |
| Vendors carry `lat`/`lng`; `VendorLeg` carries only `vendor_id`. Stop coordinates need a vendor lookup. | `services/omnideliv/src/domain/entities/vendor.rs:60`, `order.rs:56` |
| `Order.delivery_lat`/`delivery_lng` are `Option<f64>` — orders placed before migration 0013 have none. | `services/omnideliv/src/domain/entities/order.rs:159` |
| Only two customer notifications exist, and the omission of the rest is argued in code: *"a phone buzzing four times for one dinner is worse than silence."* | `services/omnideliv/src/infrastructure/messaging/order_events.rs:1` |
| Push registration is wired app-side and posts to identity with `app: "customer"`. | `apps/omnideliv-app/src/api/push.ts:48` |
| OmniDeliv is COD-only. `cod_amount_cents` is documented as "0 when there is nothing to collect — a prepaid order, *once that rail exists*." Payment capture is deferred; refunds do not exist. | `services/field-ops/src/domain/entities/assignment.rs` |
| Zero primitives for carbon offset, incentives, promos, loyalty, vouchers, top-up, or stored value across `services/omnideliv`, `services/field-ops`, and the app. | repo-wide grep |
| Consolidation economics point away from a customer bonus: *"Consolidation is the margin lever, not a customer perk"*, with a test asserting a three-stop plan never costs more than a one-stop plan. | `services/omnideliv/src/domain/entities/consolidation.rs:3` |
| `Order` persists a full money breakdown; both `TrackResponse` and `OrderListItem` expose only `grand_total_cents`. | `order.rs:150`, `tracking.rs:24`, `tracking.rs:102` |

---

## Decisions

### D1 — The money surface is read-only. No stored value.

**Rejected:** a customer-keyed balance ledger with top-up and withdrawal.

A decision recorded 2026-08-11 removed the LogisticOS customer app's Wallet
screen rather than repointing it, because the wallet domain is the **merchant
settlement wallet**, keyed by `tenant_id` end to end — there is no customer
dimension to scope, only one to invent. A test now asserts the `customer` role
holds none of `BILLING_MANAGE` / `PAYMENTS_RECONCILE` / `BILLING_ADMIN`,
specifically so that "my wallet screen 403s" is not repaired by granting a
permission that moves merchant money.

Nothing has changed since. OmniDeliv has no capture rail, no top-up rail, and no
refund path — "manage funds" would have no funds to manage. A consumer
stored-value balance in the PH market is BSP e-money territory: a regulated
product, not a gap to wire.

**What ships instead:** a panel over money that already exists — cash due at the
door on the in-flight order, spend this month, and a per-order receipt breakdown.

### D2 — Canvas plot now; tiles behind a component seam.

**Rejected for now:** `@rnmapbox/maps` and `react-native-maps`.

The expensive part of this feature is the data path, and it is identical under
every rendering choice. Tiles over no data are a blank map. Both native options
cost a config plugin, a build secret wired into the Gradle step of the APK
workflow, and a per-MAU bill; the Mapbox token is in any case still listed as
unprovisioned on the VPS.

The admin portal already ships the pattern this needs — tiles when a token is
present, a canvas GPS plot when it is not. This builds the fallback first and
leaves the tile layer as an additive swap behind one component interface.

### D3 — ETA is a narrowing range from observed speed, computed server-side.

**Rejected:** a fixed speed constant (drifts worst in the last five minutes,
exactly when someone is watching); a Directions API (a keyed third-party call in
the tracking hot path, per refresh per in-flight order, on a token that does not
exist yet); and no number at all (concedes the headline of the request).

The `courier_supply` precedent — return `null` rather than fabricate — is honoured
by *hiding* the estimate when it cannot be earned, not by refusing to estimate.

### D4 — Milestones live on the screen, not in the notification tray.

The two-event notification design is deliberate and argued in code. The same
comment says `collecting` and `delivering` are *"progress a tracking screen shows
well and a push notification shows badly"* — which is precisely the surface being
built. Milestones are derived client-side from data already in `TrackResponse`.
`order_events.rs` is not touched.

---

## Architecture — backend data path

### field-ops: one new read

```
GET /v1/field-ops/assignments/:id/position
→ 200 { courier_id, lat, lng, speed_kph, heading_deg,
        device_timestamp, recorded_at, age_seconds }
→ 404 when the assignment is unknown, or has no fix on record
```

**Keyed on the assignment, not the courier.** omnideliv holds an assignment id
and nothing else; a courier-keyed route would force omnideliv to learn field-ops'
internal identity, which would then put a courier id on a customer-facing
surface. field-ops still never learns what an order is — the tier stays
product-agnostic.

**Service-token authed**, alongside `offer` and `claim`. It must not be reachable
with a customer token.

`LocationRepository::latest()` already exists and is the whole implementation;
this route is the missing read, plus an assignment → `courier_id` resolution.

### omnideliv: one new outbound port

A `CourierTelemetry` trait mirroring `CourierDispatch`
(`application/services/checkout_service.rs:56`), with an HTTP implementation
mirroring `FieldOpsDispatch`
(`infrastructure/external/field_ops_dispatch.rs:45`) and a `NoopCourierTelemetry`
for an unreachable field-ops — the same shape as `NoopOrderEvents`.

```rust
#[async_trait]
pub trait CourierTelemetry: Send + Sync {
    async fn position(&self, tenant_id: Uuid, assignment_id: Uuid)
        -> anyhow::Result<Option<CourierFix>>;
}
```

**Auth reuses the per-call minted token, not a static secret.** `FieldOpsDispatch`
already holds an `Arc<JwtService>` built from `AUTH__JWT_SECRET` (shared with
field-ops) and mints a 60-second, role-less, permission-less token **per call**,
carrying the caller's tenant. A static service token cannot work here for two
reasons already documented in that file: it expires, and it would carry one fixed
tenant — which would offer every tenant's orders to one tenant's couriers. The new
telemetry call mints exactly the same way, so no new configuration is introduced;
`field_ops_url` and `AUTH__JWT_SECRET` already exist.

### Two rules the handler enforces

**Absent or stale ⇒ `courier: null`.** No last-known position rendered as live.
A frozen dot a customer believes is moving is worse than an honest gap — the same
reasoning that makes `courier_supply` return `null` rather than a fabricated
count. Staleness threshold: **120 seconds** on `sla_timestamp()`.

**Position is gated to `collecting` | `delivering`, checked before the outbound
call.** Not `placed`, not `awaiting_courier`, and specifically **not after
`delivered`** — a courier's live location must not remain readable from a
completed order. One gate, one place, and it also saves the call.

**A telemetry failure degrades the map and never fails `/track`.** The handler
already models this for the timeline: *"A missing timeline is not a missing
order."* Same treatment — log, return `courier: null`, serve the rest.

### `TrackResponse` grows additively

Existing clients are unaffected; every field is new and optional or defaulted.

```rust
pub courier:            Option<CourierFix>,   // null unless in motion and fresh
pub eta:                Option<EtaEstimate>,  // null without a usable fix
pub destination:        Option<LatLng>,       // null for pre-0013 orders
pub stops:              Vec<StopView>,        // vendor legs: name, lat, lng, picked_up
pub goods_total_cents:  i64,                  // money breakdown — see below
pub delivery_fee_cents: i64,
pub tip_cents:          i64,
```

`stops` needs vendor coordinates, which live on `vendors` while `VendorLeg`
carries only `vendor_id`. **One query for all of an order's vendors**, not a
lookup per leg — the same N+1 that `CatalogRepository::find_items` was introduced
to avoid on the basket screen.

### ETA as a pure domain function

`services/omnideliv/src/domain/entities/eta.rs`, no DB and no broker:

```
distance = haversine over remaining uncollected stops, then to destination
         × ROAD_FACTOR (1.3)                     // straight line flatters
speed    = EWMA of the courier's recent speed_kph,
           clamped to [8, 40] km/h               // a red light is not infinity
dwell    = DWELL_PER_STOP (4 min) × uncollected stops
minutes  = distance / speed + dwell
→ EtaEstimate { low_minutes, high_minutes }      // width scales with distance
→ None when the fix is older than 120s
```

The range narrows on its own as distance falls, so "20–30 min" becomes "5 min"
without a second mechanism. Purity matters here beyond taste: the DB-backed tests
in this service have still never executed against a real Postgres, so the ETA
logic must be provable without infrastructure.

---

## Architecture — the app

### The map seam

```tsx
// src/components/map/MapSurface.tsx
export interface MapSurfaceProps {
  courier: LatLng | null;
  stops: StopView[];
  destination: LatLng | null;
}
```

`CanvasPlot` ships now: pure React Native. All points normalize into a padded
bounding box, then to screen coordinates by absolute positioning. The courier dot
pulses with RN `Animated` — Reanimated was deliberately dropped from this app's
dependencies and is not being reintroduced for one pulse.

`MapboxSurface` later takes the same props. The data path does not change.

### Track screen

```
┌────────────────────────────────┐
│ On the way              ← back │  status headline (existing SAY map)
│ Arriving in 12–18 min          │  ETA range — hidden, never faked
├────────────────────────────────┤
│        ○ Kuya's ✓              │  CanvasPlot
│           ╲                    │    ○ stop   ✓ collected
│            ● ← courier         │    ● courier (pulsing)
│             ╲                  │    ▣ destination
│              ▣ You             │
├────────────────────────────────┤
│ ✓ Order placed          18:02  │  milestone strip — derived,
│ ✓ Courier accepted      18:04  │  in-screen, not pushed
│ ✓ Picked up · Kuya's    18:19  │
│ ○ Picked up · Suki mart        │  next step, dimmed
│ ○ Delivered                    │
├────────────────────────────────┤
│ Cash due at the door   ₱412.00 │
└────────────────────────────────┘
```

Milestones are derived from `status`, `stops_collected` and the timeline already
in `TrackResponse`, extending the existing `EVENT_LABEL` map rather than
replacing it. No new endpoint.

### The degrade ladder

The screen must be correct in four states, not one.

| State | Behaviour |
|---|---|
| Fresh fix | Plot with courier + narrowing ETA range |
| No fix, or stale > 120s | Stops and destination plot, **courier absent**, ETA hidden, "Waiting for the courier's location" |
| No `delivery_lat`/`delivery_lng` (pre-0013 order) | No plot at all; milestone strip and money only |
| `delivered` / `cancelled` | Plot removed; milestones and receipt remain |

### `pollIntervalMs` gets its first caller

The hardcoded `POLL_MS = 8000` is replaced by the schedule that already exists and
has never been called: `delivering` → 5s, terminal → stop polling entirely,
otherwise 15s. It encodes the right policy; it was simply never wired.

### Responsiveness

Required by CLAUDE.md. The plot is the risk — absolute positioning inside a
fixed-aspect box. It scales to its container with a min-height floor so it cannot
collapse on a small viewport, and the milestone strip scrolls independently rather
than pushing the plot off-screen. Verified across simulated viewport sizes before
the work is called done.

---

## Architecture — the money surface

**No new endpoints.** That zero-route delta is the evidence the scope is right: a
read-only surface over existing data needs none.

`TrackResponse` and `OrderListItem` each gain `goods_total_cents`,
`delivery_fee_cents` and `tip_cents` — already columns on `orders`, so no new join
and no per-row query.

### Home canvas panel

```
┌────────────────────────────────┐
│ CASH DUE AT THE DOOR           │
│ ₱412.00                        │  in-flight order only
│ Kuya's + Suki mart · on the way│  taps through to /track/[id]
├────────────────────────────────┤
│ This month  ·  12 orders       │
│ ₱3,240.00                      │  delivered only
└────────────────────────────────┘
```

With nothing in flight the panel collapses to the month line. There is no empty
state dressed up as a balance.

### Per-order receipt

Shown on the track screen and in order history:

```
Goods                    ₱340.00
Delivery fee              ₱49.00
Tip                       ₱23.00
────────────────────────────────
Total                    ₱412.00
Paid in cash on delivery
```

The final line is load-bearing. A money panel that does not name its rail is what
invites the assumption that a balance sits behind it.

### The guard

A test asserting the OmniDeliv customer surface exposes no wallet, balance,
top-up or withdraw route.

The 2026-08-11 decision is currently guarded at the *identity role* layer. This is
the second surface on which "the wallet screen is missing" would tempt the same
wrong repair, so the guard is extended to where the defect could next appear
rather than left only where it was first caught.

---

## Testing

| Layer | Coverage |
|---|---|
| `eta.rs` | Pure unit tests: known distance and speed → known range; a stopped courier clamps rather than diverging; a stale fix returns `None`; the range narrows as distance falls; dwell scales with uncollected stops |
| omnideliv track handler | Position is `null` in `placed`, `awaiting_courier`, `delivered` and `cancelled`; present only in `collecting` and `delivering`; a telemetry error still returns 200 with the rest of the payload |
| Customer scoping | The existing "a second user gets 404" assertion extended to cover the new coordinate fields, so a leak cannot be introduced through the additive payload |
| field-ops route | A customer token is rejected; the service token is accepted; an assignment with no fix returns 404 rather than a zero coordinate |
| Money guard | No wallet / balance / top-up / withdraw route on the OmniDeliv customer surface |
| App | `CanvasPlot` renders each of the four degrade states; `pollIntervalMs` is actually called and returns `null` on terminal states |

Every guard is mutation-tested — the assertion must fail when the line it guards
is deleted. Three CI guards in one prior session each passed the bug they were
written for; a guard that has not been seen to fail has not been shown to work.

---

## Not building

**Carbon offset options.** No primitive exists anywhere for an offset, a levy, or
an environmental attribute. It needs a price, a counterparty, a unit of account,
and a claim someone can substantiate — none of which is a screen.

**Agent consolidation bonuses.** Consolidation is documented in code as the margin
lever rather than a customer perk, and a test asserts a three-stop plan never
costs more than a one-stop plan. A customer-facing bonus inverts economics that
were chosen deliberately. It needs that decision reopened first, not a UI.

**Stored value, top-up, and withdrawal.** See D1. If revisited it needs a
customer-keyed ledger, a top-up rail, refund-to-balance semantics, and a BSP
e-money answer. It is a product, and would need its own spec and ADR.

**Push notifications for intermediate milestones.** See D4. The two-event design
is deliberate; this feature satisfies the milestone requirement in the screen,
which is where that decision says such progress belongs.

**Tile-based maps.** See D2. Deferred behind `MapSurfaceProps`, additive when a
token is provisioned and the build-secret work is justified.

---

## Risks and open items

- **First real omnideliv → field-ops read.** Checkout already calls field-ops
  outbound, so the token plumbing is proven, but a new failure mode is introduced
  on a customer-facing hot path. Mitigated by `NoopCourierTelemetry` and by the
  rule that telemetry failure never fails `/track`.
- **Poll amplification.** `delivering` polls at 5s per open screen, and each poll
  may now make an outbound call to field-ops. Watch this once more than a handful
  of orders are in flight; a short-TTL cache on the position read is the obvious
  relief and is deliberately not built yet.
- **ETA constants are uncalibrated.** `ROAD_FACTOR`, the speed clamp band and
  `DWELL_PER_STOP` are reasoned starting values, not measured ones. They are
  named constants in one module so calibration against real deliveries is a
  one-file change.
- **The map cannot be gated locally.** `expo export` fails on this dev machine
  regardless of the code — the Windows `hermesc` rejects `#private` fields in a
  dependency. Local gates are `tsc --noEmit`, `npx expo-doctor`, jest, and a Metro
  module-count comparison against a stashed baseline. The APK workflow on Linux is
  the real gate.
- **Pre-0013 orders have no destination** and therefore no plot. Accepted: they
  degrade to milestones and money rather than being shown a guessed point.
