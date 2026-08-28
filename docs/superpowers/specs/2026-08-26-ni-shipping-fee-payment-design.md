# Network International — Online Shipping Fee Payment (AE-region)

**Date:** 2026-08-26
**Status:** Approved section-by-section, ready for an implementation plan
**Scope:** `libs/types`, `libs/auth`, `services/order-intake`, `services/payments`, `apps/customer-app`

---

## What triggered this

The user asked to add Network International as a payment gateway across four
surfaces at once: LogisticOS SaaS subscription billing, OmniDeliv storefront
checkout, LogisticOS customer booking (truck & recovery), and LogisticOS
shipping fee (parcel/courier/balikbayan). Investigation found these are four
independent subsystems with almost nothing shared in code today, so the work
was decomposed into separate specs. This document covers the first and
narrowest: **shipping fee payment**, for AE-region tenants only, from the
Customer App.

---

## Findings that shaped the design

| Finding | Evidence |
|---|---|
| No payment gateway is wired anywhere in the platform. The adapter module is a two-line comment. | `services/payments/src/infrastructure/external/mod.rs` |
| There is no "sender pays the shipping fee" rail at all today, online or offline. The only money-collection mechanism is COD cash collected by the driver **from the receiver**. The shipping fee itself is collected as cash by the driver at pickup, per the POP directive's `cash_collected_amount_cents`. | `CLAUDE.md` POP directive; `services/payments/src/domain/strategies/` |
| The fee shown to the customer is computed **entirely client-side** and never validated server-side. It is sent only as `declared_value_cents`, not as an amount the server prices or charges. | `apps/customer-app/src/screens/booking/BookingScreen.tsx:414` (`calcTotal`), `:507` |
| The customer-app's quote engine explicitly has "no network dependency — all computation is local," duplicated manually between the app and the landing page. | `apps/customer-app/src/lib/quote-engine.ts:8` |
| order-intake **does** have a server-side, authoritative fee function already — but it is hardcoded to PHP and Philippines-domestic tariffs. | `services/order-intake/src/domain/entities/shipment.rs:79-118` |
| `Currency` has no `AED` variant. Network International cannot process PHP — it is a UAE/MENA acquirer. | `libs/types/src/lib.rs:105-111` |
| `Claims` carries no tenant currency/region, only `tenant_id`. Every service that needs to price or charge in the tenant's currency would otherwise need a cross-service call to identity per request. | `libs/auth/src/claims.rs:8` |
| The established internal-service-auth pattern already exists and needs no new mechanism: routes under `/v1/internal/*` are gated by Istio mTLS, no JWT. | `services/payments/src/api/http/mod.rs:39,90` |
| OmniDeliv's checkout already anticipates this exact shape of change for its own COD field: *"`cod_amount_cents` ... becomes 0 for prepaid orders when that rail exists — an amount rather than a payment-method flag, so that change is a value change and not a new branch through dispatch."* | `services/omnideliv/src/application/services/checkout_service.rs:114-118` |
| The codebase already has a signed-token pattern usable for a stateless, short-TTL quote (HMAC), and an event-driven status-map pattern usable for the payment-captured transition (Kafka event → status). | `services/connectors/src/infrastructure/hmac.rs`; memory: Kafka event → `shipment.status` map, `status_consumer.rs` |

---

## Decisions

### D1 — Scope to AE-region/AED tenants only; PH-region is untouched

**Rejected:** generalizing to a pluggable multi-gateway abstraction across all
tenant currencies now.

Network International only processes AED (and other GCC currencies) — it
cannot be the gateway for a PHP-denominated booking. Building a
gateway-selection abstraction today would have exactly one real implementation
behind it. AE-region tenants get the new AED tariff, quote endpoint, and
"Pay Online" option; PH-region tenants keep today's cash-on-pickup flow
unchanged.

### D2 — The adapter lives in `services/payments`, not a new service or embedded in order-intake

**Rejected:** a dedicated `services/payment-gateway` microservice; embedding
NI calls directly inside `services/order-intake`.

`services/payments` already owns money movement (`Invoice`, `DriverLedger`,
`Wallet`) and has a literal placeholder reserved for this
(`"PayMongo, GCash, Maya (future)"`). A new microservice for one gateway with
one consumer is speculative. Embedding it in order-intake would fragment
payment logic right before the subscription-billing and truck/recovery specs
need the same adapter.

### D3 — order-intake prices, payments charges; neither trusts the other's domain

order-intake owns "what does this cost" (tariffs, quotes) because it already
owns `Shipment::compute_base_fee_with_pieces`. `services/payments` owns
"move this exact amount of money" and knows nothing about shipping tariffs.
`POST /v1/payments/intents` is internal-mesh-only (`/v1/internal`, Istio mTLS)
specifically so no tenant can self-serve a payment intent priced by them —
the amount always originates from order-intake's server-side quote
re-verification, never from the client.

### D4 — Hosted checkout page; LogisticOS never touches card data

**Rejected:** collecting card details directly (NI Direct API / raw card
fields) in the app.

Routing the customer to NI's own hosted payment page (opened in an in-app
WebView) keeps `services/payments` at PCI SAQ-A instead of pulling it into
full PCI scope, consistent with the platform's existing PCI-DSS
scope-minimization standard.

### D5 — Quotes are stateless signed tokens, not a database table

A quote nobody completes shouldn't cost a row. `POST /v1/shipments/quote`
returns an HMAC-signed, short-TTL token embedding the priced inputs and
amount (reusing the signing pattern already in
`services/connectors/src/infrastructure/hmac.rs`). It is re-verified — not
re-trusted — when the shipment is actually created.

### D6 — Payment state change is event-driven, not a synchronous chain

**Rejected:** order-intake calling out to payments and blocking on the
result inline during shipment creation.

The shipment is created immediately in a new `PendingPayment` status (so the
AWB/tracking number exists right away), and a Kafka consumer — the same
event→status-map shape as the existing `status_consumer.rs` — reacts to
`payment.intent_captured`/`payment.intent_failed` published by payments'
webhook handler. This matches ADR-0002 (event-first) and means a captured
payment is never lost to a downed consumer; Kafka retains it until the
consumer is back.

---

## Data model & API contracts

**`libs/types::Currency`** — add `AED`.

**`libs/auth::Claims`** — add `currency: Option<String>`, populated from
`Tenant.currency` at JWT mint time (`mint_for_existing_user` and `finalize`
paths in `auth_service.rs`).

**order-intake:**
- New AE tariff table (AED-denominated, same shape as the existing PHP one),
  selected when `claims.currency == "AED"`.
- `POST /v1/shipments/quote` — authenticated. Input: the same booking fields
  already collected (service_type, weight/pieces, COD flag), minus payment
  details. Output: `{ amount_cents, currency, quote_token, expires_at }`.
- `POST /v1/shipments` — extended to accept an optional `quote_token` +
  client-generated idempotency key. When present, order-intake re-verifies
  the token server-side, creates the shipment in `PendingPayment`, and calls
  payments' internal intents endpoint with the verified amount.
- New `ShipmentStatus::PendingPayment`.
- New Kafka consumer for `payment.intent_captured` (→ `Placed`, normal
  dispatch-offer flow proceeds, `cash_collected_amount_cents` = 0 at pickup)
  and `payment.intent_failed`/expiry (→ `Cancelled`).
- Timeout sweep for shipments left in `PendingPayment` past `expires_at`
  (e.g. 30 min), reusing the shape of the existing `AwaitingCourier`
  recovery sweep rather than a new mechanism.
- `cancel_shipment` on a `PendingPayment`-originated, already-captured
  shipment triggers `payments::refund`.

**payments:**
- New table `payments.payment_intents`: `id, tenant_id, purpose, reference_type,
  reference_id, amount_cents, currency, status (Created|Pending|Captured|
  Failed|Refunded|Expired), gateway, gateway_order_ref, gateway_payment_ref,
  created_at, updated_at, expires_at`. `purpose` is an open enum with
  `shipping_fee` as the only value used today — the later subscription/
  storefront/booking specs add more values to the same table rather than
  inventing parallel ones.
- `PaymentGateway` trait: `create_session`, `verify_webhook`, `refund`.
- `infrastructure/external/network_international.rs` implements it against
  NI's Hosted Payment Page (order/session creation, webhook signature
  verification, refund). Exact request/response shapes and signature scheme
  are confirmed against NI's live API/sandbox docs during implementation —
  this spec fixes the contract our own services expose, not NI's wire
  format.
- `POST /v1/internal/payments/intents` — mesh-internal only, Istio mTLS,
  same pattern as the existing `/v1/internal/*` routes. Body:
  `{ tenant_id, purpose, reference_type, reference_id, amount_cents,
  currency }`. Returns `{ intent_id, checkout_url }`.
- `POST /v1/payments/webhooks/network-international` — public, no JWT,
  authenticated by NI's webhook signature. Idempotent on
  `gateway_payment_ref`. Publishes `payment.intent_captured` /
  `payment.intent_failed` keyed by `reference_id` (the shipment id).
- Credentials (API key, webhook signing secret) in Vault.

---

## Sequence

1. Customer fills the existing booking form (Steps 1–3 in `BookingScreen.tsx`
   unchanged). On reaching Review, the app calls `POST /v1/shipments/quote`
   instead of trusting `calcTotal()`, which becomes a display estimate only.
2. For AE-region/AED tenants, a new **Pay Online** option appears alongside
   the existing COD toggle. Tapping "Book" sends the booking payload plus
   `quote_token` and an idempotency key to `POST /v1/shipments`.
3. order-intake re-verifies `quote_token`, creates the `Shipment` in
   `PendingPayment`, calls payments' internal intents endpoint, and returns
   `{ shipment_id, awb, checkout_url }` — nothing is dispatched yet.
4. The app opens `checkout_url` in an in-app WebView; the customer pays on
   NI's hosted page.
5. NI redirects back to a deep-link (`return_url`) **and independently**
   calls the webhook. The redirect is a UX signal only — a customer closing
   the WebView before it fires must not be treated as failure or success.
6. The webhook handler verifies NI's signature, marks the intent `Captured`
   idempotently, and publishes `payment.intent_captured`.
7. order-intake's consumer transitions `PendingPayment → Placed`; the
   existing dispatch-offer flow proceeds unchanged.
8. A shipment left in `PendingPayment` past `expires_at` is swept to
   `Cancelled`.
9. Cancelling an already-captured `PendingPayment`-originated shipment
   triggers a refund.

---

## Error handling

- **Double-booking:** idempotency key on `POST /v1/shipments` — a retry with
  the same key returns the existing shipment/intent rather than creating a
  second charge.
- **Declined/failed payment:** webhook marks the intent `Failed` → shipment
  `Cancelled` with a reason. The app polls shipment status after the WebView
  closes and offers "Try again," which requests a fresh quote + intent — a
  failed intent is never resurrected.
- **Webhook races/replays:** capture is idempotent on `gateway_payment_ref`;
  a replayed or out-of-order webhook is a no-op.
- **Consumer downtime:** Kafka retains the event until the consumer is back
  — no payment is silently lost.
- **Refund failure on cancellation:** the cancellation itself is not blocked
  on the refund call succeeding, but a failed refund is logged/audited and
  retried, not dropped.

## Security

- Card data never reaches LogisticOS — NI's hosted page only (PCI SAQ-A).
- NI credentials and webhook signing secret in Vault, never env/code.
- Webhook payloads are untrusted until signature-verified; no state change
  happens on an unverified webhook.
- `POST /v1/internal/payments/intents` is mesh-internal only — unreachable
  by any tenant-facing credential.

## Testing

- Unit tests: AE tariff calculation; quote-token sign/verify including
  expiry and tamper rejection.
- Integration tests: intent state machine (`Created→Captured`,
  `Created→Failed`, replay-safety, expiry sweep) against a mocked
  `PaymentGateway`.
- Contract test against NI's sandbox for the real adapter.
- End-to-end test: book → pay in NI sandbox → confirm shipment reaches
  `Placed` and is offered to dispatch.

---

## Out of scope (separate future specs)

- LogisticOS SaaS subscription billing (recurring charge, card-on-file,
  dunning).
- OmniDeliv storefront checkout (prepaid online payment for vendor orders —
  requires reworking the courier-payout/driver-ledger COD assumptions).
- LogisticOS customer booking for truck & recovery (payment for
  `services/carrier` marketplace bookings — the booking product's
  merchant-facing flow is itself still incomplete).
- PH-region or any non-AED tenant shipping-fee payment (would need a
  different gateway).
