# OmniDeliv — Driver Application, Slice One

**Date:** 2026-08-17
**Status:** Approved section-by-section, ready for an implementation plan
**Scope:** `apps/omnideliv-driver-android` (new), `services/field-ops`, `services/omnideliv`

---

## What triggered this

A full Driver Application Specification was raised for OmniDeliv: an
agent-synchronised operational interface receiving dynamically recalculated
multi-stop manifests across Food, Grocery, Pharmacy, Flower and E-commerce, with
vertical-aware proof of delivery, persistent-socket telemetry, instant cashout
and voice control.

Investigation found no OmniDeliv driver app exists. `apps/driver-app-android` is
a different product entirely — LogisticOS logistics, talking to driver-ops,
dispatch and pod. `apps/omnideliv-app` is the OmniDeliv *customer* app. So this
is greenfield, landing on a backend already about half-built for it.

It also found the specification, taken literally, collides with the codebase in
three places and spans at least five independent subsystems. It was decomposed;
this spec covers **slice one — the walking skeleton**.

---

## Findings that shaped the design

Each verified against code, not assumed.

| Finding | Evidence |
|---|---|
| field-ops is deliberately product-opaque: *"`product` and `external_ref` are deliberately opaque: field-ops must not interpret a product's job id, or the tier stops being product-agnostic."* It structurally cannot know a stop is a pharmacy. | `services/field-ops/src/domain/entities/assignment.rs:1` |
| `GET /assignments/mine` returns assignment id, product, external_ref, cents and a timestamp. **No address, no stop, no vertical.** | `services/field-ops/src/api/http/couriers.rs:174` |
| Multi-**vendor** consolidation already exists and is sequenced by readiness, with temperature classes. | `services/omnideliv/src/domain/entities/consolidation.rs` |
| Multi-**customer** batching is blocked: one live claim per courier, enforced by a partial unique index. | `services/field-ops/migrations/0002_create_assignments.sql:53` |
| `mark_collected` already takes `vendor_id` + `device_timestamp` and passes both through to Kafka uninterpreted — multi-vendor pickup works end to end today. | `services/field-ops/src/application/services/dispatch_service.rs:387` |
| `CourierEvent::Assigned` carries `courier_id` **and** `assignment_id`. | `services/field-ops/src/infrastructure/messaging/mod.rs:17` |
| omnideliv persists the assignment as `Order.courier_task_id` but **discards `courier_id`**. | `services/omnideliv/src/infrastructure/messaging/courier_consumer.rs:98` |
| `GET /assignments/:id/position` is capability-based — any valid tenant JWT plus the UUID reads a live courier position. Recorded as safe only because the id never reaches a client. | `services/field-ops/src/api/http/couriers.rs:505` |
| `collected` and `delivered` pass only `claims.tenant_id` — neither verifies the caller holds the assignment. `claim` was hardened; these were not. | `couriers.rs:533`, `couriers.rs:557` |
| `mark_delivered` completes the assignment, credits the courier ledger and debits COD. | `dispatch_service.rs:412` |
| The credit guard scans only the current period's ledger, and `current_period()` is the ISO week read from `Utc::now()` with no clock seam. | `dispatch_service.rs:465`, `dispatch_service.rs:560` |
| omnideliv's `Collected` consumer branch is explicitly idempotent; `Delivered` is not — it propagates a `TransitionError` on a duplicate. | `courier_consumer.rs:112` vs `courier_consumer.rs:135` |
| `LegStatus::Settled` already means *the vendor has been paid*. | `services/omnideliv/src/domain/entities/order.rs:37` |
| omnideliv's storage layer already sniffs and accepts `image/webp` — but on `RIFF` alone, which is also the WAV and AVI container. | `services/omnideliv/src/infrastructure/storage.rs:24` |
| Object storage is cluster-internal: minio *"publishes no port and has no Traefik route"*, so presigned URLs are unreachable from a client. Photos upload as multipart through the service. | `services/omnideliv/src/api/http/catalog.rs:709` |
| No barcode or QR concept exists anywhere in `services/omnideliv`, and `Order` has no `order_number`, `reference` or `short_code`. Scanning belongs to the LogisticOS product. | repo-wide grep |
| `OutboundSyncWorker` uses `enqueueUniqueWork` — a single drain worker, not one request per item. | `apps/driver-app-android/.../worker/OutboundSyncWorker.kt:637` |
| Existing Android floor is `minSdk = 26`. `Bitmap.CompressFormat.WEBP_LOSSY` requires API 30. | `apps/driver-app-android/app/build.gradle.kts:24` |
| The payout run refuses any courier holding outstanding cash — mutation-verified, two tests. | `project-omnideliv-deployed` memo, verified live 2026-08-09 |

---

## Scope

### In

A courier signs in, goes online, receives an offer, accepts it, works a
consolidated multi-vendor job stop by stop, delivers with a photo proof,
collects COD, and sees the money land. That path exercises the manifest
contract, the milestone chain, the Kafka round trip, both ledgers and the
settlement identity, on real hardware.

### Out, with reasons

| Excluded | Why |
|---|---|
| Multi-**customer** batching (spec §3.1 "Drop-off Customer 1 → Customer 2") | One live claim per courier is enforced by a unique index, and `trip_cents` is declared per order. Two customers on one trip has no pay-split model and breaks the per-order settlement identity that currently balances. Own backend project. |
| Instant cashout (§3.4) | The payout run refuses a courier holding outstanding cash. Mid-shift a COD courier is normally in debt to the platform. Inverting that guard is a money-rail decision, not a screen. |
| Vertical-specific PoD — ID match, signature, recipient photo (§3.2) | Needs a proof-requirement model in omnideliv and on-device ID processing. Photo only here; the capture module carries a pluggable proof-step seam so richer verticals drop in without restructuring. |
| Exception → agent → new instructions (§3.3) | Requires a mesh round trip and a customer-contact channel. The button ships and files an exception; the agent conversation is its own spec. |
| Voice (§4) | YAGNI for slice one. |
| QR / barcode scanning | Nothing to scan. No code concept exists in omnideliv and no order carries a scannable identifier. Shipping a scanner first requires minting an order short-code, rendering it vendor-side, and vendor adoption — a feature chain, not a UI affordance. |
| iOS | The specification itself defers it. |

### Assumed, not asked

Phone OTP via the platform's existing `/v1/auth/otp/send` → `/verify` (role
`driver`, that endpoint's default), then `POST /v1/field-ops/couriers/register`.
No second bespoke auth — ADR-0009 rule 4. Earnings is a read-only screen over
the existing `GET /couriers/me/earnings`.

---

## Decisions

| # | Decision | Rationale |
|---|---|---|
| 1 | **Manifest splits at the claim** | Two different questions. Before claim: what is this job worth. After claim: where do I go and what do I do. |
| 2 | **Native Kotlin, brand-new app** | Best runtime behaviour for a foreground location service and camera, and no contamination of either existing app. |
| 3 | **Focus + rail home layout** | A mid-route re-sequence animates in the rail while the focus card and its primary action never move — the agent can rewrite the route while a driver is mid-tap. |
| 4 | **Graduated advance control; geofence advises, never blocks** | Gesture cost matches consequence: tap the 7 reversible steps, slide the 1 irreversible one. |
| 5 | **HTTP batched ingest up, adaptive poll down** | field-ops is stateless and rolling updates are a platform non-negotiable. |
| 6 | **Offline queue with pending shown honestly** | The driver is never shown money as earned that the server has not agreed to. |

### 1 — Manifest splits at the claim

`offer_to_nearest` fans out to the N nearest couriers, so anything on the offer
is disclosed to everyone merely *considered* for the job — not just whoever takes
it. The privacy line therefore falls exactly where the responsibility line
already is.

### 4 — Geofence advises, never blocks

A hard gate strands a courier standing at the door in an urban canyon, a lift
lobby or a basement. With COD the cash is already in their hand when the door
closes, so blocking the button does not un-collect it — it only prevents the
system recording what already happened, which is strictly worse than recording
it with a flag. Follows the platform's existing `OUT_OF_BOUNDS_HANDOVER`
soft-flag precedent rather than the POD service's hard 200 m gate.

*(Raised for reversal during review as "geofence-gated action buttons, disabled
until native location validation passes"; re-affirmed as advisory.)*

### 5 — No WebSocket

The specification asks for a persistent WebSocket or gRPC connection. Declined
for slice one:

- A socket server makes field-ops **stateful**, against the explicit
  "zero downtime deployments — all services must support rolling updates"
  non-negotiable. Every Istio rolling restart would drop every courier mid-shift.
- On Android a backgrounded socket dies to Doze regardless, so the full HTTP
  path gets built anyway — and then two paths exist where one carries all the
  real traffic.
- The value is in ingest *cadence*, not transport. Batched HTTP delivers it.

Revisit when sub-5-second customer ETA is a measured product requirement rather
than an assumed one.

---

## Architecture

### Contract 1 — the offer card (field-ops, opaque)

One nullable `offer_card JSONB` column on `courier_assignments`. omnideliv
supplies it at offer time; field-ops stores it and returns it verbatim in
`GET /assignments/mine`, **never reading a key of it**.

Carries: stop count, pickup count, total distance, vendor names, vertical and
temperature tags, a deadline hint, and a `v` schema version.

Carries **nothing about the customer** — no name, no address, no coordinates —
and **no street addresses at all**, including the vendors'. Accepting a job needs
distance, not navigation; addresses arrive with the manifest.

A blob rather than columns because columns named `vertical` or
`temperature_class` would be field-ops naming product concepts in its own
schema. That *is* interpretation, and it forecloses a third product with
different concepts. An opaque blob is the same category as `external_ref`.

**Tripwire:** a CI grep failing the build on any `offer_card->>` appearing in a
query, following the `scripts/check-runtime-boundary.sh` precedent. Without it,
the opacity is one convenient `WHERE` clause from being gone.

### Contract 2 — the live manifest (omnideliv)

`GET /v1/omnideliv/courier/jobs/{order_id}` — addresses, per-vendor line items,
handling notes, customer detail, gate codes, COD amount.

Re-read on every screen open and on the adaptive poll. **Never cached as
truth**, so a mid-route change simply appears rather than having to be
reconciled against stale local state.

### Authorization, with no new synchronous call

`CourierEvent::Assigned` already carries `courier_id`; omnideliv persists the
assignment id and throws the courier away.

- field-ops adds `courier_user_id` to that event — additive, `Option<Uuid>` with
  a serde default so in-flight messages still parse.
- omnideliv persists it on the order.
- The manifest read authorizes `claims.user_id` against it locally.

No verify-holder round trip on a polled path, and no capability-based access
anywhere in the new surface.

### One new milestone

`POST /v1/field-ops/assignments/:id/arrived`, carrying an opaque `stop_ref` and
`device_timestamp`, passed straight through to Kafka — the identical shape
`mark_collected(vendor_id, …)` already uses, so field-ops still interprets
nothing.

`stop_ref` is a `Uuid` that field-ops never resolves. omnideliv sets it to the
**vendor id** for a pickup stop and the **order id** for the dropoff, and is the
only party that knows the difference.

**"En Route" gets no milestone.** It is derivable from *claimed, and not yet
collected at the next stop*. "Arrived" is not derivable — a geofence cannot
distinguish "parked outside" from "at the door" — and it is the event a customer
most wants pushed.

---

## Hardening — prerequisites, not improvements

Each is a latent defect today that becomes an exploitable one the moment this
app exists.

### H1 — Close the position capability leak

`GET /assignments/:id/position` accepts any valid tenant JWT plus the UUID.
Once `assignments/mine` has a real consumer, assignment ids are in the field,
and any courier who learns one can follow another courier around the city.

Accept the call only from the assignment's holder, or from omnideliv's minted
service token carrying one narrow permission for this read. That token today
deliberately grants no roles and no permissions, with a test pinning it — update
that test to pin **exactly this one permission**, so the widening stays on the
record instead of silently loosening.

### H2 — Verify assignment ownership on `collected` and `delivered`

Thread `claims.user_id` through both, refuse an assignment the caller does not
hold, return **404** — matching the repo convention that a foreign id reads as
nonexistent rather than forbidden, so ids cannot be probed.

**`claim` keeps returning `200 {won:false}`.** It deliberately gives the same
answer for "not yours" and "you lost the race"; converting it to 404 would
destroy that property.

### H3 — Make a retried delivery a true no-op

Two independent bugs, both reachable only once something retries — which is
exactly what an offline queue does.

- **field-ops:** two fixes, because one is not enough. The application guard
  must ask the *store* whether this job was ever credited, rather than scanning
  the single period's ledger it happens to hold; and a partial unique index
  backstops every other crediting path.

  The index cannot be written against the table as it stands.
  `courier_ledger_entries` keys on `ledger_id`, and a ledger is per
  `(tenant, courier, period)` — so an index on `ledger_id` is scoped *inside*
  the very boundary the bug crosses. `tenant_id` and `courier_id` are therefore
  denormalised onto the entry row and backfilled from the owning ledger, after
  which the index is
  `(tenant_id, courier_id, kind, external_ref) WHERE external_ref IS NOT NULL`.

  The index creation **fails** if production already holds duplicates. That is
  correct: they are real money, and which entry survives is a human decision,
  not something a migration should quietly pick. Check before deploying.
- **omnideliv:** make the `Delivered` consumer branch early-return on an
  already-delivered order, symmetric with the `Collected` branch that already
  gets this right.

### H4 — Tighten the WebP magic-byte sniff

`("image/webp", b"RIFF")` matches WAV and AVI too. A real check needs `RIFF` at
bytes 0–4 **and** `WEBP` at 8–12. Pre-existing and minor; routing every proof
photo through it is what promotes it to worth fixing.

---

## App structure

```
apps/omnideliv-driver-android/          package net.cargomarket.omnideliv.courier
├── app/            nav host, DI, theme
├── core/
│   ├── network/    Retrofit + JWT interceptor/refresh, gateway base URL
│   ├── database/   Room — manifest cache + outbound queue
│   ├── location/   foreground service, batched ingest
│   └── design/     tokens + components
└── feature/
    ├── auth/       phone OTP → couriers/register
    ├── shift/      online/offline, offer inbox, claim
    ├── manifest/   focus + rail, stop detail, advance control
    ├── proof/      camera, encode, enqueue (pluggable proof steps)
    └── earnings/   read-only ledger, confirmed vs pending
```

Patterns copied from `apps/driver-app-android` — its `LocationForegroundService`
and `OutboundSyncWorker` are the right shape — but **no shared Gradle modules**,
so neither app's refactor can break the other's build. `minSdk = 26`: that floor
is the right one for a courier app in PH, where the demographic is
disproportionately on older hardware, and raising it to 30 for a cleaner API
constant would trade real couriers for tidier code.

### Two stores with different authority

This distinction is the whole offline design.

- **Manifest cache** — a *render cache*. Replaced wholesale on each fetch, never
  a source of truth, so a mid-route rewrite cannot be argued with by stale local
  state.
- **Outbound queue** — the only place a local write is authoritative-pending.
  Every row is a claim the server has not agreed to yet.

The earnings screen reads both and **never adds them together**.

Queue row: milestone kind, assignment id, opaque `stop_ref`, `device_timestamp`,
payload, proof path, attempt count.

### Sync worker

A **single drain worker** via `enqueueUniqueWork`, reading the queue
`ORDER BY device_timestamp` — copying the existing pattern. Independent
`OneTimeWorkRequest`s give no ordering guarantee, so one-request-per-item would
silently lose chronology.

**Dead-letter after 5 attempts, or immediately on a 4xx.** Under strict ordering
a single permanently failing row — a stale assignment returning 404 — blocks
every later milestone forever, freezing a courier's whole shift behind a
spinner. A 4xx will never succeed on retry, so parking it at once is both
correct and faster; 5xx and transport failures exhaust the backoff first. A
parked row lets the queue proceed and surfaces for support.

### Disciplines applied from `CLAUDE.md`

- `System.currentTimeMillis()` captured **at the physical event** — tap, shutter —
  and serialised into the queue row immediately. Never re-sampled at worker
  execution.
- The capture screen advances when the payload is **enqueued**, not uploaded.
- Proofs upload as **multipart through omnideliv**, never presigned — minio
  publishes no port and has no Traefik route, so a presigned URL is unreachable
  from a phone.

### Amendment to the `CLAUDE.md` POP directive

That directive specifies `Bitmap.compress(JPEG, 75)` at ≤ 800 KB by name. This
app encodes **WebP lossy, quality 80, target ≤ 300 KB, hard ceiling 400 KB**,
with a quality step-down retry if the first pass overshoots.

WebP lossy runs 25–34 % smaller than JPEG at equivalent visual quality, which
takes an 800 KB proof to roughly 250–300 KB with no loss of readability for
human or ops review. The server already accepts `image/webp`.

`Bitmap.CompressFormat.WEBP_LOSSY` is API 30+, so the encoder branches to the
deprecated-but-functional `WEBP` constant on 26–29. Same libwebp underneath.
Encoding runs off the main thread — libwebp is roughly 2–3× slower than libjpeg,
negligible on modern hardware but a few hundred milliseconds on a low-end API 26
device, and the capture screen must not stall before enqueueing.

This is recorded as a deliberate amendment rather than a silent contradiction of
a written directive.

### Cadence

| Condition | Behaviour |
|---|---|
| Location, job claimed | sample 10 s, batch POST every ~20 s, flush immediately on any milestone |
| Manifest, foreground near a stop | poll 10 s |
| Manifest, foreground otherwise | poll 30 s |
| Manifest, background | no poll |

---

## UI/UX

### Tokens

Platform palette and typography retained — `#050810` base, Space Grotesk
headings, JetBrains Mono for money and codes.

**Surfaces are opaque.** `backdrop-blur` and translucency collapse contrast in
direct sunlight, which the specification names as the app's primary condition.
Recorded as a deliberate, documented divergence from the design system in
`CLAUDE.md`, not an omission.

*(`#0A0A0A` was proposed during review. `#050810` retained: both are effectively
black, neither is true OLED pixel-off, and `#050810` keeps family resemblance
with every other CargoMarket surface. If OLED power draw becomes the goal the
answer is `#000000`.)*

### Ergonomics

- **56 dp minimum** on any control that advances state, with **expanded touch
  slop** — the default slop turns a tap during a bump into a swallowed drag,
  which is the normal case on a bike mount.
- The primary action is pinned in the bottom third and **never moves** when the
  rail re-sequences.
- Money and distances in tabular figures so digits do not jitter on refresh.
- Every status carries a shape or label, **never colour alone** — WCAG 2.1 AA is
  a platform non-negotiable, and a courier squinting in sunlight is the case it
  exists for.
- **Zero-typing:** the entire skeleton has exactly one text input, the OTP, and
  Android autofills that from SMS.

### Screens

Sign-in → shift (online toggle, offer inbox) → manifest (focus + rail) → stop
detail → proof capture → delivered → earnings. Seven, no more.

### Offline and sync states

- **Render-cache indicator.** Operating from cache shifts the manifest bar to the
  platform's amber warning token, stating plainly that data is local.
- **Pending is a first-class visual state, not a toast.** A queued milestone
  renders as `Delivered · Syncing` with a persistent badge and a count.
  - Labelled **Synced**, never *settled* — `LegStatus::Settled` already means
    *the vendor has been paid* in omnideliv, and reusing it here would collide
    with live domain vocabulary.
  - **Static badge, not a perpetual spinner.** Nothing is in progress when there
    is no network; the item is waiting, not working, and a spinner that never
    resolves reads as a hung app. Spinner only during an actual in-flight
    attempt.
- **Earnings shows confirmed and pending as two figures that are never summed.**
  The payout run works off the server balance, and an app that quietly disagrees
  with it is how a courier stops trusting the number.
- **Geofence renders as a distance chip, never a disabled button.** A commit
  beyond 50 m writes a telemetry-exception flag for ops and proceeds.

---

## Verification

Shaped by what this repo has already learned about tests that pass without
running.

### V1 — Period-boundary idempotency (Rust)

Write the first ledger entry under an explicit period (`2026-W33`), attempt the
credit under `2026-W34`, assert the unique index rejects it. Assert courier
balance, COD debit and order state are each unchanged on the second attempt.

**Why not "seed Sunday 23:59:59, retry Monday 00:00:01":** `current_period()`
reads `Utc::now()` directly with no clock seam, so a wall-clock test would either
need a real week to elapse or would silently never cross the boundary and pass
for the wrong reason. Testing at the index makes it writable today. Testing the
application-level guard instead would require injecting a clock first.

### V2 — Sync durability and ordering (Kotlin)

Enqueue five proof uploads and milestones with connectivity mocked
`DISCONNECTED`. Assert the rows persist. Re-initialise WorkManager — that *is*
the restart, since the queue is Room-backed and durable. Meet constraints,
assert execution in `device_timestamp` order with zero duplication. Assert a
permanently failing row parks and does not block the rest.

**Note:** `WorkManagerTestInitHelper` cannot force-kill a process — it supplies a
`TestDriver` for driving constraints synchronously. Re-initialisation against the
persisted queue is the valid and equivalent proxy.

### V3 — Authorization (integration)

Issue a JWT for Courier A. Against an assignment held by Courier B, assert
`GET /assignments/:id/position` returns **404** and `collected` / `delivered`
return **404** — preventing enumeration and matching platform convention.
Separately assert `claim` still returns `200 {won:false}`.

### V4 — The composition test

Geofence-advisory + offline-pending + idempotent-retry exercised **together**: a
delivery committed outside the fence, queued offline, retried across a period
boundary. Each rule is trivial alone. Two rules each tested in isolation that
were never composed is the exact shape of the last defect shipped on this
surface.

### V5 — Mutation verification

Each hardening fix is mutation-verified: remove the guard, watch a test go red.

### V6 — End to end

`scripts/omnideliv-smoke.sh` extended so the courier legs are driven by the
app's own API calls rather than curl. Re-seed within 10 minutes of testing
dispatch — `find_available_near` only considers fixes from the last 10, and a
stale courier fails looking exactly like a dispatch bug.

### CI

The app cannot be built locally — no Gradle on the dev machine — so its own CI
workflow is the gate **from the first commit**, not retrofitted. Both
`testDebugUnitTest` and `testStagingDebugUnitTest`.

---

## Build order

This spec expects **two implementation plans, in this order**:

1. **Backend** — H1–H4, the `offer_card` column, the `courier_user_id` event
   field, `POST /assignments/:id/arrived`, and
   `GET /v1/omnideliv/courier/jobs/{order_id}`. Independently verifiable against
   the existing smoke script with no app in existence.
2. **App** — the Kotlin client, against endpoints already hardened.

The order is not cosmetic. Building the app first would mean writing a client
against `collected` and `delivered` that do not yet check ownership, and against
a delivery path that double-credits on retry — so the offline queue, the single
feature most likely to trigger both, would be exercised against the versions
that break.

---

## Deferred, with the gate for each

| Deferred | Unblocked by |
|---|---|
| Multi-customer batching | A trip entity and a pay-split model; relaxing the single-live-claim index |
| Instant cashout | A decision on paying a courier who holds platform cash |
| Vertical PoD | A proof-requirement model in omnideliv |
| Exception → agent loop | A mesh round trip and a customer-contact channel |
| WebSocket telemetry | A measured sub-5-second ETA requirement |
| QR / barcode | An order short-code minted and rendered vendor-side |
| Voice | Nothing; YAGNI |
