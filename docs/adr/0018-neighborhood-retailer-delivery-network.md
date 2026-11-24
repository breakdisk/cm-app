# ADR-0018: Neighborhood Retailer Delivery Network

**Status:** Proposed
**Date:** 2026-11-23
**Deciders:** Principal Architect, PM — Logistics Operations, PM — Partner & Carrier, PM — Payments & Billing, EM — Logistics Domain, CISO, Compliance & Legal Counsel
**Design:** [Neighborhood Delivery Network](../superpowers/specs/2026-11-23-neighborhood-delivery-network-design.md)

> **Load-bearing invariant.** Every parcel has exactly one accountable party at every moment, and that party changes only on a recorded two-sided scan. Money, liability, customer tracking and dispute handling are all derived from that record.

---

## Context

We want OmniDeliv vendors to earn additional revenue by delivering LogisticOS parcels in their own neighborhoods. The hub sorts parcels into labelled bags, the platform moves those bags to stores, and each store's riders complete the last mile.

Reading `origin/master` at `bb8d9519` shows the operating pieces mostly exist but cannot be combined safely:

- A `partner` token can read tenant-wide shipments and wallet data, and the driver manifest has no permission check.
- JWTs carry no partner identity.
- Dispatch cannot give one rider a multi-stop batch. `quick_dispatch` is single-stop, and route assignment emits no `TASK_ASSIGNED`.
- Nothing closes an accepted driver assignment, and a unique index allows only one active assignment per driver.
- Four automatic assigners can take any pending shipment.
- Shipment status becomes `out_for_delivery` when dispatch is *requested*.
- Coordinates carry no quality marker.
- There is no territory, tote, per-store cash liability, or partner ledger.

---

## Decision

### D1. A vendor participates through a linked carrier profile

An enrolled vendor gets a `Carrier` plus a `store_partner_profiles` row, linked by opaque `origin_product` and `origin_ref`. Carrier does not know what an OmniDeliv vendor is. A future retailer from another product is onboarded with data, not DDL.

### D2. Hub-ops owns territories, waves, totes and custody

The tote trip is a new aggregate modelled on `Pallet`, separate from the permanent `tote_assets` record and from single-use serialized `security_seals`. Custody is an append-only event log with dual timestamps and a `client_ref` for offline replay. Territories are versioned PostGIS `geometry` polygons with effective dates, and may not overlap in area within a hub. They are edited with Terra Draw on the existing Mapbox GL stack.

### D3. Partner identity is an explicit membership and a `pid` claim

`identity.partner_memberships` is the source of truth. `Claims.pid` is issued from it, and repositories filter on it at the application layer. This adopts ADR-0013's `pid` and replaces its RLS mechanics with ADR-0016's model. A partner-role token with no `pid` fails closed.

### D4. Store-bound parcels live in a separate dispatch table

`dispatch.partner_delivery_jobs` holds store work, and the queue row moves to `partner_fulfilment`. Automatic assigners only read `pending`, so exclusion is structural. Any assigner added later inherits it without a guard.

### D5. Reservation is synchronous and everything downstream is event-driven

A sorter needs an immediate answer before bagging a parcel, so hub-ops reserves through an internal dispatch endpoint. If dispatch is down, the parcel is held. It is never bagged on a guess. All later state changes flow over Kafka and are idempotent.

### D6. Batch assignment is a new path

One route, N stops, one assignment, and one delivery-only `TASK_ASSIGNED` per stop, with an explicit `payout_cents = 0`. The existing `quick_dispatch` and offer paths are not widened.

A store's non-final failed attempt publishes `DELIVERY_ATTEMPTED`, because `DELIVERY_FAILED` makes a shipment terminal. Only the final attempt publishes `DELIVERY_FAILED`. Behavior for non-partner tasks does not change.

### D7. Assignments gain a `completed` state

This fixes a platform-wide defect and is a prerequisite here. Closing is derived on the server from task outcomes, and `cancel_route` closes its assignment too. Readers ship before writers because `parse_status` maps unknown values to `Pending`. `drivers.active_route_id` has no production writer and stays unwired, because writing it would make `go_offline` and `set_status` start refusing riders.

### D8. Only a physical release makes a store parcel `out_for_delivery`

`logisticos.hub.tote.piece_released` drives the flip. No `ShipmentStatus` is added.

### D9. Partner money is a separate append-only ledger

`payments.partner_ledger_entries` separates `earnings` from `cod_held`, with a unique idempotency key on every entry. It is independent of `vendor_ledger`, `DriverLedger` and tenant wallets. Payments learns which shipments are partner jobs from `logisticos.partner.job.changed`, and posts collected cash to the store in the `reconcile_cod` transaction. Riders are paid by their store.

### D10. A claim never moves money, and silence never moves parcels

Discrepancies open cases, and charges post only on ops resolution. A store that has not confirmed a wave receives nothing. A store that goes quiet after receiving is escalated to a human and physically recalled.

### D11. Stores see the minimum

Stores never see merchant identity, parcel contents, or declared value. Recipient contact details are limited to active assigned work and are audit-logged.

### D12. Contracts come first, on the existing conventions

New endpoints go into the existing OpenAPI 3.1 files under `/v1`, reusing `PaginatedResponse` and the `AppError` body. Tenant and partner identity always come from the JWT. Commands use `Idempotency-Key` and `expected_version`. Offline physical scans use `client_ref` and are not version-checked, because they record something that already happened.

### D13. Stop order sits behind a replaceable engine

A `StopSequencer` starts with road-distance nearest-neighbor from the store, capped at 24 stops by the Mapbox Matrix limit, and degrades to straight-line distance if routing fails. Manual reorder and a locked next stop are supported, and every sequence is kept.

### D14. Timers are configuration, and recovery is human

Each owning service stores its rules and an append-only escalation log. Clocks anchor to wave deadlines. Alerts group by tote or batch. A timer never reassigns a parcel a store is holding.

### D15. Location quality gates only the store network

Geocoder results normalize to a quality level through a provider adapter, and unknown values fail closed. Existing dispatch is not gated, because every existing shipment starts as `UNKNOWN`.

---

## Alternatives considered

**New delivery domain inside OmniDeliv.** Rejected. It would rebuild dispatch, POD, COD, SLA and custody for a second time, and would tie the network to one product.

**Vendors as field-ops couriers.** Rejected. Field-ops allows one live claim per courier, has no owner above the courier, and has no batch or store-level liability.

**Flags on `dispatch_queue`.** Rejected. It needs a guard in four assigners today and silently fails when a fifth is added.

**Reusing `quick_dispatch` for hub-out.** Rejected. It requires a `pending` row, adds a pickup leg at the merchant's address, and applies gig pay.

**Tote-level receipt on seal scan.** Rejected. An intact seal cannot prove what was packed, so liability would rest on no evidence.

**MapLibre for the territory editor.** Not adopted. Every map-using portal runs Mapbox GL, so this would add a second map stack for one screen. Terra Draw supports both if the platform later moves.

**A central SLA engine in `business-logic`.** Not adopted for now. Its trigger entity is a stub and its scheduler ticks hourly.

**A new portal and app.** Rejected. Partner Portal and the Android app already cover these roles. A separate app adds distribution and offline work with no capability gain.

---

## Consequences

**Positive**

- Phase 0 closes live partner data exposure and frees riders after completed jobs, regardless of whether the network ships.
- The network extends to non-OmniDeliv retailers through data.
- Every parcel and peso can be traced to a scan.

**Negative**

- The work touches identity, carrier, hub-ops, dispatch, driver-ops, order-intake, payments, engagement, three portals and the Android app.
- Stores become a new operational dependency, so fallback paths must stay healthy.
- Sharing recipient data with independent stores creates PDPA obligations that need legal review.
- Dispatch still reads `driver_ops` across schemas to validate riders. This follows the existing pattern and remains accepted debt.

**Rollout traps**

- Enforcing `pid` before backfill and one full token lifetime locks out every existing partner.
- Removing partner `payments:read` before moving the Payouts page breaks that page.
- New Kafka topics must exist on the broker before consumers depend on them.
- PostGIS has not been used in the hub-ops database and must be verified before its first migration.
- `parse_status` reads unknown assignment statuses as `Pending`, so the `completed` reader must deploy before the writer.
- `COD_COLLECTED` carries the `CodReconciled` shape, not the shared `CodCollected` type.

---

## Prerequisites outside this codebase

- `cargomarket.net` must be released from registrar `clientHold`.
- Twilio must be configured for PIN delivery.
- The Mapbox account must be confirmed for permanent geocoding, because results are stored but `permanent=true` is not sent today.
- Compliance and Legal Counsel must review the store agreement, rider status and permits, and data sharing for the UAE pilot.
