# ADR-0017: Vendor Order Notification and QR Table Ordering

**Status:** Proposed
**Date:** 2026-08-30
**Deciders:** Principal Architect, PM — Logistics Operations, PM — Customer Experience, EM — Engagement, EM — Frontend, CISO

> **Load-bearing invariant.** The vendor's order queue is the record; every notification is a hint. A channel that fails must never be the reason an order is lost — the store sees the order on its next read of `GET /v1/omnideliv/vendors/me/orders` regardless of what was pushed, rung, or messaged.

---

## Context

Two problems, one root cause.

### 1. No vendor is ever told they have an order

Traced against `services/omnideliv` on 2026-08-30:

- `omnideliv.order.placed` carries `customer_id`, `order_id`, `grand_total_cents`, `stops` — and **no `vendor_id`** (`src/infrastructure/messaging/order_events.rs`).
- Engagement maps that topic to the `omnideliv_order_placed` template, resolves the recipient from `customer_id`, push-only (`services/engagement/src/application/services/event_consumer.rs:51`).
- The vendor console (`apps/merchant-portal/src/app/(dashboard)/storefront`) is a catalog-*confirmation* screen. There is no order queue and no orders call in `src/lib/api/storefront.ts`.
- `OrderStatus` is `Placed → AwaitingCourier → Collecting → Delivering → Delivered` (`src/domain/entities/order.rs:13`). No vendor acknowledgement exists anywhere in it.

The store finds out it has an order when a courier walks through the door. `prep_time_minutes` (default 15) is used to plan the job; nothing ever tells the kitchen to start cooking.

The root cause is an asymmetry: **the vendor is already a first-class settlement party and not a first-class operational one.** `VendorLeg::settle`, `commission_bps`, `vendor_ledger` and the payout run all treat the vendor as a party to the transaction. Nothing treats it as a party to the work.

### 2. Dine-in has no home in the model

QR table ordering — a printed code on a table, scanned by a walk-in diner — is being added in two shapes: a **single restaurant**, and a **mall foodcourt** where one table is shared by many stalls. Nothing in the codebase has a table, a dine-in order, or an unauthenticated customer; a repo-wide grep for `table_number` / `dine_in` / `foodcourt` returns nothing.

These are one ADR rather than two because a dine-in order **is an order with N vendor legs and zero courier legs**. It cannot work at all until vendors are operational parties, and once they are, it is mostly a new entry path rather than a new domain.

---

## Decision

### 1. Acceptance lives on the leg, not the order

`LegStatus` (`Pending → PickedUp | Failed | Settled`) gains `Accepted`, `Preparing`, `Ready`, `Served`. Order status is **derived** from its legs and never set directly by a vendor action.

A basket already spans vendors (`Order.legs`, `card_stops` in `checkout_service.rs`). One stall accepting must not move a three-stall order. A per-order vendor state would be a lie on every foodcourt basket — which is the majority case for the new surface.

### 2. A vendor-scoped queue is the record

New routes, following the `/me`-resolves-from-claims rule already established in `src/api/http/vendors.rs` — a vendor id in the path would let any signed-in vendor read another store's orders.

| Method | Path | Purpose |
|---|---|---|
| GET | `/v1/omnideliv/vendors/me/orders` | The queue. SSE stream + poll fallback. |
| POST | `/v1/omnideliv/vendors/me/legs/:leg_id/accept` | Carries `ready_in_minutes`. |
| POST | `/v1/omnideliv/vendors/me/legs/:leg_id/reject` | Carries a reason — the substitution path needs it. |
| POST | `/v1/omnideliv/vendors/me/legs/:leg_id/ready` | Cooked / picked / wrapped. |
| POST | `/v1/omnideliv/vendors/me/legs/:leg_id/served` | Dine-in only. |

Same discipline as dispatch's app-restart recovery path (`services/dispatch/src/api/http/offers.rs:19`): the notification is a wake-up, the endpoint is the truth.

### 3. A new topic, keyed on the vendor

`omnideliv.vendor.leg.received`, keyed on `vendor_id`, carrying **only that vendor's leg**.

`order.placed` is deliberately not widened. Its consumer is customer notification; adding a vendor recipient makes one event with two audiences, two authorization rules and two failure modes — and a foodcourt order would need it fanned out N ways regardless. Vendor-keyed events also give per-vendor ordering, which is what a stall's queue actually needs.

### 4. Escalating transport, chosen by vertical

A restaurant has a counter tablet. A florist has a phone in an apron. One channel cannot serve both.

| Tier | Channel | Status |
|---|---|---|
| 0 | Console SSE + repeating audible alarm until acknowledged | To build. The tier that actually works in a kitchen. |
| 1 | Push (FCM) | `FcmClient` exists in `driver-ops`; omnideliv has none. VPS `.env` still lacks `FCM_PROJECT_ID` / `FCM_SERVICE_ACCOUNT_JSON`. |
| 2 | WhatsApp / SMS with accept-reject quick replies | Inbound landing pad exists and validates Meta signatures (`services/engagement/src/api/http/webhook.rs`). |
| 3 | Voice call / ops escalation | Manual. |

**Prerequisite for tiers 2–3:** `Vendor` has no phone and no email — only `payout_account` and an optional `user_id` (`src/domain/entities/vendor.rs`). A **verified `contact_phone`** column is required before any non-app channel can address a store at all.

**Enforcement for stores that already exist.** A vendor without a phone is not unreachable — tiers 0 and 1 need no phone number. Missing contact is therefore a *degraded-tier* condition, not an outage, and the rollout reflects that:

- **New vendors:** `contact_phone`, verified, becomes a precondition of the existing `onboarding → active` approval gate in `vendors.rs`. A store cannot be approved onto the platform without one.
- **Existing active vendors:** a persistent banner on the storefront console, escalating to a blocking modal on that console only, with a dated deadline. **They are not deactivated.** Taking a live store offline to collect a phone number is a self-inflicted outage that costs more than the missing channel.
- **Until then:** the recovery ladder skips tiers 2–3 for that vendor and escalates from tier 1 straight to ops, so a phone-less store still fails loudly rather than silently.

> **Verification is theatre while the dev OTP bypass is open.** Phone verification reuses identity's existing OTP path, which currently accepts `123456` for any number behind `AUTH__ALLOW_DEV_OTP`. A `phone_verified` flag set through that path asserts nothing. Either close the bypass before this ships, or record the verification method on the vendor so a bypassed verification is distinguishable from a real one.

### 5. Non-acknowledgement is a modelled failure, reusing the existing ladder

`recovery_service.rs` already implements `Wait / Retry / Escalate` for a courier who never arrives. An unattended tablet is the same failure shape and reuses that ladder: re-alert, escalate a tier, then ops or auto-cancel-with-refund.

Without it, a dead tablet silently swallows orders while the customer waits on a courier who is waiting on a kitchen that never looked.

### 6. QR table ordering

**The QR encodes an identifier, never a session.** The printed code resolves to `/t/{table_token}`, where `table_token` is an opaque, rotatable, per-table random value. It is printed on adhesive vinyl in a public room and is photographable from three metres by anyone walking past — it must be worth nothing on its own.

New entities in the `omnideliv` schema:

- **`venues`** — a single restaurant or a foodcourt. `kind: standalone | foodcourt`. A standalone venue has exactly one vendor; a foodcourt has many.
- **`tables`** — `venue_id`, `label` ("A-14"), `table_token`, `status`, `printed_at`. The system generates the codes; a print sheet renders them for lamination.
- **`table_sessions`** — an open party at a table, with a TTL.

**Scan → session.** `POST /v1/omnideliv/tables/{table_token}/session` returns a short-lived JWT carrying a synthetic `user_id`, an empty email, `table_session: true`, and a permission set narrow enough to browse that venue's catalog and check out — nothing else. This reuses the `onboarding: bool` precedent in `libs/auth/src/claims.rs`: a narrow-scope principal minted through a dedicated path, rather than a second auth model. `Order.customer_id` stays required and non-null, so tracking, legs and the ledger are untouched.

**Catalog scoping is by venue, not radius.** `vendors_near` is a delivery concept; at a mall table it would return the other floor. A table scan filters to the venue.

**One cross-stall basket.** A foodcourt scan yields one basket spanning every stall in the venue — one payment, one bill — and N vendor legs, each independently accepted, prepared and settled. This is the existing multi-vendor model with the delivery leg removed.

**Fulfilment is by stall staff.** Dine-in never invokes dispatch. `courier_task_id`, `courier_user_id` and `delivery_lat/lng` are already `Option` and stay `None`; `delivery_fee_cents` is zero.

**Payment is prepaid at checkout,** reusing the existing `authorize → capture | void` rails (`src/application/services/order_payments.rs`). **The capture trigger changes for dine-in:** the delivery flow captures on courier claim, and dine-in has no courier.

Capture fires at the **acceptance barrier** — the moment every leg has resolved, or the acceptance timeout expires, whichever comes first — and captures **only the accepted subtotal**. The remainder is voided.

Capturing on *first* acceptance is wrong and was the rule in this ADR's first draft: on a three-stall basket, stall A accepting takes the full grand total, and stall B rejecting thirty seconds later leaves money captured for food nobody is making. Unwinding that is a refund, not a void — a slower instrument with fees, and NI voids are same-day only, so the reversal window is not symmetric with the capture window.

Requiring *all* legs to accept before capturing is also wrong, in the other direction: it lets one unattended stall kill an otherwise good four-stall table. The barrier resolves a non-answer as a rejection and charges for the rest.

> **Blocking dependency.** `OrderPayments::capture(&self, intent_id)` takes no amount — a partial capture is not expressible through the port as it stands. This is a change in `services/payments` and its mesh-internal intent API, not a change local to omnideliv, and the gateway's own partial-capture support must be confirmed before this design is committed to. If partial capture proves unavailable, the fallback is per-leg authorization at checkout (N holds, each captured or voided independently), which costs N gateway calls and shows N pending lines on the diner's statement.

### 7. Vendor acceptance is an MCP tool

`accept_vendor_leg` / `reject_vendor_leg` are registered as MCP tools per ADR-0004, so a grocery with freshly confirmed stock can auto-accept without a human touching a tablet, and the agent mesh can reason about acceptance latency. Consistent with ADR-0010: the agent acts, the vendor can always override.

### 8. Vendor leg transitions are guarded first and idempotent second

A kitchen tablet is on the venue's worst Wi-Fi, behind a fryer. Duplicate submissions are the normal case, not the edge case — and the transitions now carry money, because acceptance is an input to capture.

Two distinct failures need two distinct controls, and only one of them is an idempotency key:

1. **The same request arriving twice** (a retry after a timeout the client never saw resolve). Handled by an `X-Idempotency-Key` header on every vendor-scoped `POST`, following the pattern already established in `services/order-intake` — `find_by_idempotency_key` checked before any other work (`application/services/shipment_service.rs:205`). Reuse that shape rather than inventing a second one.
2. **Two different requests racing** — two staff on two tablets both tapping *Accept* on the same leg. Both are first attempts and carry different keys, so idempotency keys do nothing here. Handled by making the transition itself conditional: `UPDATE ... WHERE leg_id = $1 AND status = $expected`, and on zero rows affected, return the current state with `200` rather than an error. The tablet that lost the race sees the leg accepted, which is the truth.

**The guarded transition is the primary control**; the idempotency key is additive and matters most on `accept`, the only one of the four that can trigger a capture. No network I/O happens inside the transition — same rule as dispatch's claim transaction (`task_offer_repo.rs`).

### 9. The table-session endpoint is the platform's first unauthenticated write

`POST /v1/omnideliv/tables/{table_token}/session` takes no credential by design, and the QR that feeds it is public property — printed on vinyl and photographable from three metres by anyone walking past.

**The existing rate limiter cannot cover it.** `check_rate_limit` keys on `ratelimit:tenant:{tenant_id}:{window}` and sizes the window from the caller's subscription tier (`services/api-gateway/src/ratelimit/mod.rs`). An unauthenticated request has no tenant and no tier, so this endpoint falls outside the platform's rate-limiting model entirely rather than merely being under-configured. It needs its own limiter, keyed on `table_token` and on client IP independently.

Rate limiting alone is the weakest of the controls, because it bounds request volume and not the thing that actually matters — whether a stranger can put food on someone else's table. The controls that bound the real threat:

- **A table has an open/closed state, gated on venue hours.** Ordering to table A-14 at 03:00 when the foodcourt is shut must be impossible regardless of how valid the token is. This is the single highest-value control here and it is not a security mechanism, it is an operational one.
- **A cap on concurrent live sessions per table.** A four-top does not need fifty.
- **Token rotation is an operator action, not a migration.** "Reprint this table's code" is a button on the venue console; a leaked token is then a five-minute fix rather than an incident.
- **Prepaid checkout is the economic backstop.** An abusive order costs the abuser money before it costs a stall an ingredient.

**Browser fingerprinting is explicitly rejected as a control here.** It is behavioural tracking of an unconsented, anonymous diner, which contradicts the platform's stated GDPR/PDPA position that consent precedes behavioural tracking — and it is weak against the actual threat, since the abuse that matters is committed by someone holding a genuine phone at a genuine table. Anomaly detection on session-creation rate per venue is in scope as an ops signal; identifying the device is not.

---

## Consequences

### Positive
- The vendor becomes an operational party, closing the asymmetry with its existing settlement role.
- Dine-in is an entry path over the existing multi-vendor leg model, not a second order domain.
- `ready_at` becomes a real observation (`accepted_at + ready_in_minutes`) instead of the static `prep_time_minutes` guess — for delivery as well as dine-in.
- One acceptance state machine serves a restaurant, a grocery, a pharmacy, a florist and a foodcourt stall.

### Negative — stated plainly
- **A vendor who does not answer is a new way for an order to fail.** Today a store cannot reject an order because it is never asked. Introducing acceptance introduces refusal, and the recovery ladder is the only thing standing between that and a customer holding a paid order nobody is cooking. It ships *with* the acceptance path, not after it.
- **The anonymous table principal is a new class of token.** Every service today assumes an authenticated `user_id` with a real identity behind it. Blast radius is contained by the narrow permission set and short TTL, but this is a genuine widening of the auth surface and warrants its own security review before rollout.
- **Order status becomes derived.** Anything that writes `OrderStatus` directly must be audited against the leg-derivation rule, or two writers will disagree about the same order.
- **Capture-on-acceptance cannot be rolled out untested.** A dine-in order that authorizes and is never accepted holds a customer's funds until the void fires. The void path must be exercised against a live NI sandbox before this is enabled — not assumed from the delivery flow, which triggers capture somewhere else entirely.
- **This ADR now depends on a payments-service change it does not own.** Partial capture requires an amount on `OrderPayments::capture` and support for it in the gateway beneath. Until both are confirmed, the foodcourt case has no correct money path, and the standalone-restaurant case (one vendor, one leg) is the only part of table ordering that can ship.
- **The unauthenticated session endpoint is a new class of surface for this platform.** Every existing public route is either rate-limited by tenant or behind auth. This one is neither, and it needs its own limiter and its own review rather than inheriting the gateway's.

### Neutral
- No existing delivery behaviour changes on day one. Legs default to the current path, and a vendor who never accepts a *delivery* order behaves exactly as today until enforcement is turned on.

---

## Alternatives Considered

### Alternative 1: Notify the vendor by widening `order.placed`
**Rejected.** Cheapest available — one payload field and a second Engagement branch. But it puts two recipients with different authorization rules on one event, and a foodcourt order would still need fanning out per stall. Vendor-keyed events also give per-vendor ordering, which a stall queue needs and an order-keyed event cannot provide.

### Alternative 2: Courier-delivered table service
**Rejected.** Reuses dispatch unchanged and needs no new state machine. But paying courier economics for a twenty-metre walk is indefensible, and it makes a foodcourt's throughput depend on courier supply inside the mall.

### Alternative 3: Require login before ordering at a table
**Rejected on conversion, not on merit.** It costs nothing to build and yields a CDP profile per diner. But a hungry walk-in will not create an account to order lunch, and this surface exists precisely to remove the friction the counter queue already imposes. The optional post-order upgrade captures the profile for diners who want it.

### Alternative 4: One order per stall in a foodcourt
**Rejected.** Simpler per-vendor logic and no cross-stall coordination. But the customer pays N times and receives N receipts, and the platform loses the consolidated table bill — which is the whole reason to scan one code instead of queueing at four counters.

---

## References

- ADR-0004 — MCP for AI interoperability (`accept_vendor_leg` tool registration)
- ADR-0009 — Multi-product platform gateway topology (OmniDeliv route namespace)
- ADR-0010 — Operator-agent collaboration model (agent acts, vendor overrides)
- ADR-0016 — Application-layer tenancy (every new repository method takes `tenant_id`)
