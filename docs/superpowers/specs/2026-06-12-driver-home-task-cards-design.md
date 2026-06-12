# Driver App Home Screen — Rich Task Cards, Gig Accept/Decline, Performance Rules

**Date:** 2026-06-12
**Status:** Approved for implementation (autonomous session — assumptions documented inline)
**Scope:** Driver app home screen revision + the backend data plumbing it requires.

## Problem

The driver app home screen shows only shift stats and banners. Task offers arrive as a
full-screen `AssignmentScreen` triggered by FCM — but the backend FCM push
(`FcmClient::send_fcm`) sends a generic `dispatch_message` with **no assignment data**, so
the rich offer flow is dormant. Tasks carry no merchant name, no shipment category, no
weight, and only one end of the route (the leg's own address). Drivers cannot see payout;
there is no distinction between full-time and gig (part-time) presentation. No decline
tracking, no rating surface, and auto-dispatch ignores vehicle capacity.

## Goals

1. Home screen task cards showing: merchant (sender) name, delivery-type icon
   (food / parcel / grocery / medicine / heavy-weight / big-shipment), distance to
   destination, time to reach, and payment — **payout visible only to part-time (gig)
   drivers**.
2. Card background renders the full pickup→delivery route sketch.
3. Gig workers Accept / Decline directly from the home screen offer card.
4. Decline accountability: 20 declines ⇒ automatic ban (deactivation).
5. Auto vehicle matching: shipment weight/category filters eligible drivers by vehicle type.
6. Groundwork for customer ratings + lateness penalties (columns + display; ingestion is
   phase 2).

## Approaches considered

- **A. Map SDK background (Mapbox) on each card** — heavy, online-dependent, costly per
  render. Rejected for the card; Mapbox stays on the Route screen.
- **B. Stylized Canvas route sketch** (pickup pin → curved neon path → destination pin,
  positions normalized from real coordinates) — offline-safe, zero dependency, matches the
  dark-glass design language. **Chosen.**
- **C. Backend-computed distance/ETA vs. client-computed** — the phone has the freshest GPS
  fix; backend would need a per-driver query per task. **Client-side haversine + 22 km/h
  urban average speed. Chosen.**

## Data flow (all additive, `#[serde(default)]` — no breaking changes)

```
order-intake (CreateShipment: + merchant_name?, delivery_category?)
   └─ ShipmentCreated (+ merchant_name, delivery_category; weight_grams existed)
        └─ dispatch.dispatch_queue (+ merchant_name, delivery_category, weight_grams)
             └─ quick_dispatch:
                  • vehicle gate: weight/category → allowed vehicle classes
                  • TaskAssigned (+ merchant_name, delivery_category, weight_grams,
                                   pickup_lat/lng, delivery_lat/lng on BOTH legs)
                       └─ driver_ops.tasks (+ same columns)
                            ├─ FCM data push type="task_assigned" (rich payload — fixes
                            │   the dormant AssignmentScreen path)
                            └─ GET /v1/tasks (+ fields, + payout_cents for part_time only)
```

### Delivery category

`delivery_category ∈ {food, parcel, grocery, medicine, heavy, large}`. Optional at booking;
derived when absent: `balikbayan service → large`, `weight ≥ 20 kg → heavy`, else `parcel`.
Carried as an opaque string end-to-end; icon mapping lives in the app.

### Merchant name

`ShipmentCreated.merchant_name` is an event-time passthrough from the booking request
(merchant portal / API send their display name; empty default otherwise). Not persisted in
order-intake (no re-emission path exists); durably persisted in `dispatch_queue` and
`driver_ops.tasks`. Fallback display in app: customer name.

### Payout (gig only)

`driver_ops.drivers` already has `driver_type (full_time|part_time)` and
`per_delivery_rate_cents`. `list_my_tasks` joins the driver row: `payout_cents =
per_delivery_rate_cents` when `driver_type = part_time`, else `null`. The app renders the
payment chip only when non-null — full-time drivers never see a price, per requirement.

### Accept / Decline + decline ban

Existing `PUT /v1/assignments/:id/accept|reject` (dispatch) stays the contract. Changes:
- dispatch `reject_assignment` now publishes `ASSIGNMENT_REJECTED`
  (`logisticos.dispatch.assignment.rejected`) with `{assignment_id, driver_id, tenant_id, reason}`.
- driver-ops consumes it: `drivers.decline_count += 1`; at `decline_count ≥ 20` the driver
  is banned: `is_active = false, status = 'offline'`. Banned drivers drop out of
  `find_available_near` automatically (it filters `is_active`). Threshold const
  `DECLINE_BAN_THRESHOLD = 20`.
- Home screen shows a performance strip: `Declines: n/20` (amber ≥ 15) and rating stars.

### Vehicle matching

In `quick_dispatch` auto-selection, candidates are filtered by
`vehicle_can_carry(vehicle_type, weight_grams, category)`:
- `motorcycle/bike`: ≤ 20 kg and category ∉ {heavy, large}
- `sedan/car/mpv/suv`: ≤ 200 kg, category ≠ large
- `van/pickup`: ≤ 1,000 kg
- `truck`: unlimited
- unknown/NULL vehicle_type: treated as sedan-class (conservative middle).
A dispatcher's explicit `preferred_driver_id` bypasses the gate (manual override).

### Phase 2 (documented, not in this change)

- **Customer rating ingestion:** delivery-experience `save_feedback` already stores a 1–5
  rating by tracking number; phase 2 publishes a feedback event and driver-ops resolves
  tracking → task → driver to maintain `rating_avg/rating_count` (columns added now,
  surfaced in profile API + app immediately; values stay null/0 until ingestion lands).
- **Lateness penalty:** requires per-task SLA deadline propagation (none exists on tasks
  today) — needs `promised_at` on TaskAssigned + a completion-time comparison job.
- **Re-dispatch on rejection:** `ASSIGNMENT_REJECTED` is the hook; a dispatch consumer can
  re-queue the shipment (today ops re-dispatches manually, unchanged).

## Android UI

`HomeScreen` gains, above "Today's Shift":

1. **Incoming offer card** (when `PendingAssignmentBus` has a payload): route-sketch
   Canvas background, category icon + label, merchant name, AWB, distance-to-pickup +
   distance pickup→destination, ETA, COD badge, payout chip (gig only), full-width
   **Accept** (green) / **Decline** (red, reason sheet) buttons. Decline reasons reuse the
   assignment screen's list. Accept/reject call `DriverOpsApiService` directly from
   `HomeViewModel`.
2. **Task list section**: one card per pending/in-progress task from `GET /v1/tasks` with
   the same visual language (smaller); tap → Route tab.
3. **Performance strip**: rating stars (placeholder until phase 2 ingestion) +
   `Declines n/20` counter for part-time drivers.

The full-screen `AssignmentScreen` continues to work (FCM now actually feeds it); the home
card covers the case where the driver dismissed the screen or the app restarted.

## Error handling

- All new event fields default — old producers/consumers interop cleanly.
- FCM enrichment is fire-and-forget (existing pattern).
- Decline-ban consumer is idempotent per assignment (decline_count increments only on
  status transition rows; duplicate Kafka delivery tolerated by checking assignment id is
  not strictly required — increments are monotonic and a rare double-count is acceptable
  vs. the complexity of a dedup table; documented trade-off).
- Distance/ETA show "—" when no GPS fix or task coords missing.

## Testing

- Rust: unit tests for category derivation, `vehicle_can_carry`, decline-ban threshold;
  existing test payload literals updated.
- Android: validated by GitHub Actions CI (no local Gradle per project convention).
