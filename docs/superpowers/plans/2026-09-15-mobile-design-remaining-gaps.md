# Mobile Design — Remaining Gaps Roadmap

> **For agentic workers:** execute workstream by workstream, in order. Each workstream
> ends with a green build and a downloadable APK. Detailed steps for a workstream are
> written into this file when it starts (see "Execution notes").

**Goal:** Close every gap left after the "Move" builds (commits `234e8e8c`, `d450bc56` on
`claude/delivery-pin-required`): the unstyled screens, and the seven features the design
shows with no backend.

**Architecture:** UI first (no backend, testable at once), then small backend slices, then
the three large ones. Every money figure ships configurable and at 0 / off, the precedent
set for cancellation fees on 2026-09-14. Anything the design shows that the platform cannot
yet do stays hidden rather than dead.

**Tech Stack:** Rust/Axum/SQLx services; React Native + Expo (customer-app, `APP_VARIANT=move`);
Kotlin + Compose (driver-app-android, flavor `move`).

---

## Order and why

| # | Workstream | Size | Backend | Why here |
|---|---|---|---|---|
| R | Restyle: driver route, stack, compliance, wallet, POD, arrival, sign-in; customer sign-in; sun mode everywhere | L (UI) | none | Visible in the next APK; unblocks nothing else but is what testers see first |
| H | Hours-of-service clock | S | driver-ops | Duty card already reserves the line; one table and one read |
| S | Item scan on the plan screen | S | none | The customer app already ships `modules/ar-measurement` (`measureBox()`) |
| V | Voice input (customer prompt) | S | none | On-device speech recognition; the prompt parser already exists |
| C | Driver ↔ customer chat | M | engagement | Needed again by whole-home's crew thread (part-2 plan 3.7) — build once |
| K | Masked calling | M | engagement | Twilio Calls API bridge; native dial stays the fallback |
| P | Mover drop penalties + waiting grace clock | M | dispatch, driver-ops, payments | Part-2 plan Phase 1 |
| D | Promotions, loyalty, referral, credit, campaign inbox | L | new `promotions` service, engagement | Part-2 plan Phase 4 |
| A | Whole-home moves + server-side intent routing | XL | order-intake, ai-layer, payments | Part-2 plan Phases 2–3; largest, most open decisions |

## Defaults taken (flag, do not block)

1. **Every rate is config and 0 / off**: drop penalty, acceptance deltas, waiting fee, survey
   fee, promo values, tier caps, corporate rate. Same as `CANCELLATION_POLICY__*`.
2. **HOS**: display and record only, `HOS__MAX_ON_DUTY_MINUTES=660` (11 h) in a rolling
   14 h window. No enforcement until decided.
3. **Chat** lives in **engagement** (CLAUDE.md: the engagement engine owns communications).
   Task-scoped thread keyed by shipment; ownership checked through order-intake (customer)
   and driver-ops (driver). New topic `job.message.sent`, pre-created on the live broker.
4. **Masked calling**: Twilio Calls API click-to-call bridge (the platform calls the caller,
   then dials the other party with the platform number as caller ID). Off without
   `TWILIO_VOICE_NUMBER`; the apps fall back to native dial.
5. **Voice**: customer prompt only, on-device (`expo-speech-recognition`). The driver's
   "say a counter" needs a counter-offer policy that does not exist — **decision needed**;
   not built until then.
6. **Suspension** stays on the existing 20-decline rule. No-shows are recorded, not
   auto-suspending — **decision needed** (part-2 plan open question 5).
7. **Crew model**: lead-only task with `crew_size` (part-2 plan open question 7 recommends
   deciding; this is the smaller change).
8. **Promo window**: 11th–24th (README and design code agree; Wiring Map says 11–19).

## Workstream R — Restyle (detailed below when started)

- New module `:core:designsystem` (the part-1 plan's Phase 1.3 path): `MoveColors`,
  `NightColors`/`SunColors`, `LocalMoveColors`, `Condensed`, and the shared pieces now
  private to `LoadsBoardScreen` (panel, big button, header, square button, pill, stat tile,
  section heading). `:app` and every feature module depend on it.
- `ShiftScaffold` provides `LocalMoveColors` for the whole NavHost, so sun mode reaches every
  screen, not just the board.
- Restyle, keeping each ViewModel untouched: `RouteScreen` (design "circuit"),
  `HubScreen` + `HubScanScreen` ("hub"), `ComplianceScreen`, `EarningsScreen` ("wallet"),
  `PodScreen` ("pin": PIN pad + three photo frames), `ArrivalScreen`, `PhoneScreen`/`OtpScreen`.
- Customer: `PhoneScreen`, `OnboardingProfileScreen`, `KYCScreen` take the Move tokens when
  `IS_MOVE_APP`; logic untouched.
- Verification: driver CI (unit tests + `assembleMoveDebug`; the app module cannot compile
  locally without a Mapbox download token), customer `tsc` + jest.

## Workstreams H, S, V, C, K, P, D, A

Scope for P, D and A is already specified task-by-task in
`docs/superpowers/plans/2026-09-14-mobile-design-part2-promotions-penalty-home.md`
(Phases 1–4). Detailed steps for each are appended here as the workstream starts.

---

## Execution notes

### R — done 2026-09-15
`e7e56b9b` driver screens on `:core:designsystem`; `95e86b33` customer sign-in on the Move
tokens. Sun mode is held in `AppNavGraph` and offered in the Move build only. The restyle is
not flavor-gated: the staging build shows the same screens at night.

### H — done 2026-09-15
driver-ops migration `0015_create_duty_sessions.sql`. Go-online opens a session; go-offline and
an admin forcing a driver offline close it (one open per driver, partial unique index).
`GET /v1/drivers/me/hos` returns the rolling-window clock with `enforced: false`. Config
`HOS__MAX_ON_DUTY_MINUTES` (660), `HOS__WINDOW_HOURS` (14). Driver app: compliance panel and a
duty-card line, both hidden when driver-ops has no clock. Deploy: the driver-ops image only —
no topic, no gateway change (`/v1/drivers` already routes to driver-ops).

### S — done 2026-09-15
The Move plan screen scans items with `ArMeasurementModule.measureBox()` wherever
`isAvailable()`; the load's L × W × H holds the scanned volume (`scan.ts`). Weight stays typed:
the module measures size, not mass.

### V — done 2026-09-16
`expo-speech-recognition` 3.1.x (the release built against SDK 54), plugin declared for the
Move build only. `src/screens/move/voice.ts` guards every call, so the mic is offered only
where the module is linked and the phone can listen.

### C — done 2026-09-16 (no push yet)
Stacked on #162: engagement asks order-intake ("this is my shipment") and driver-ops ("I am
the driver on it") with the caller's own token, so no second rule set lives in engagement.
Migration `0009_create_job_messages.sql`; routes under `/v1/engagement/jobs/:shipment_id/
messages` (list, send, read, unread), which the gateway already routes. Both apps poll every
4 s while the thread is open.
**Not done:** push on either side. A driver push needs FCM credentials in engagement (the
driver app is FCM, engagement only has Expo push), and the `job.message.sent` topic would
have to be pre-created on the live broker. Until then a closed app learns nothing until it
is opened. K (masked calling) is next and still needs the Twilio voice number.

### K — done 2026-09-16 (off until a voice number is set)
`POST /v1/engagement/jobs/:shipment_id/call` rings the caller's own phone, then dials the other
party with `TWILIO_VOICE_NUMBER` as the caller id (Twilio Calls API, inline TwiML — no public
webhook needed). Same participant check as the thread. Each side's number is fetched from the
service that holds it with the caller's own token: order-intake (`customer_phone`),
delivery-experience (`driver_phone`), driver-ops (`/v1/drivers/me`, and the task's
`customer_phone`). Neither number is ever returned to an app.
Off without `TWILIO_ACCOUNT_SID` / `TWILIO_AUTH_TOKEN` / `TWILIO_VOICE_NUMBER`: the response
says `not_configured`, the driver app falls back to a normal dial (it has the number), and the
customer app says to send a message (it never had one).
**Found while wiring:** delivery-experience `GET /v1/tracking/:shipment_id` checks the tenant
but not the owner, so any tenant user with `shipments:read` can read another customer's
tracking record — driver phone included. Same class as the #162 order-intake bug. Not fixed
here; flagged.
**Fixed 2026-09-18** (`6ca4a34a`): tracking records carry `owner_id` from `shipment.created`;
merchants and customers read only their own, by id and in the list. Records projected before
the column have no owner and are refused to owner-scoped readers (fail closed); operators and
the public tracking-number page are unaffected. The scope rule moved to `logisticos_auth`
(`shipments_tenant_wide`) and order-intake re-exports it.
**K was half-working until 2026-09-18:** the customer side read the driver's line from the
tracking record, which is stamped empty at assignment, so it always answered `no_number`.
engagement now asks driver-ops over the mesh (`GET /v1/internal/shipments/:id/driver-contact`,
refused by the gateway from outside) for the driver holding the job, after order-intake has
confirmed the caller owns it. `SERVICES__DELIVERY_EXPERIENCE_URL` is no longer read.

### P — done 2026-09-18 (money off by default)
Leaving an accepted job. The server decides, from its own deadline:
- **Arrival** `POST /v1/tasks/:id/arrive` starts the grace clock once, only when the driver's
  last fix (< 5 min) is inside the stop's 200 m geofence. A clock started by a tap anywhere
  would let a driver start it on the way and release for free on arrival. No coordinates or
  no fix, no clock. Starting a task at the stop also arrives; nothing moves a running clock.
- **Quote** `GET /v1/tasks/:id/leave`: drop / release / refused (`GOODS_ABOARD`: load aboard,
  customer not late — that is a failed delivery; `TASK_CLOSED`), with `as_of` so the app
  counts down on the server's clock.
- **Drop** `POST …/leave` before grace: the shipment's open tasks → `cancelled` and a
  `job_drops` row, one transaction; `driver.job.dropped` → dispatch records the drop,
  cancels the assignment/route when nothing else is on it, requeues and re-broadcasts, and
  never offers it back to that driver; delivery-experience clears the driver off tracking.
  Replay-safe on both sides.
- **Release** after grace: the stop fails `CUSTOMER_ABSENT` through `delivery.failed`
  (carrying `waiting_fee_cents`); a released pickup cancels its delivery leg.
- Earnings stay a query: daily totals net waiting pay and drop fees; listed as adjustments.
- `acceptance_pct` on `/v1/drivers/me`, net of drops of claimed offers.
- Config (driver-ops): `PENALTY__GRACE_MINUTES` 45, `PENALTY__DROP_FEE_PCT` 0,
  `PENALTY__WAITING_FEE_CENTS_PER_HOUR` 0, `PENALTY__DROP_COUNTS_AS_DECLINES` 1.
- Found in passing, fixed: `cancelled` was not an allowed task status, so admin
  `POST /v1/drivers/:id/cancel-tasks` failed for any driver with an open task.
**Not built:** no-show detection (needs a pickup deadline — Finding 3 — and the suspension
decision); the rating penalty (nothing writes `rating_avg` — Finding 4); "replacement labour
if higher"; the 85% dispatch-priority line (dispatch does not rank by acceptance, so the
sheet does not claim it). **Nothing charges the customer the waiting fee**: a nonzero rate is
paid to the driver at the tenant's cost until payments bills it.
**Deploy:** driver-ops (migration 0016), dispatch (0014), delivery-experience (0009),
engagement; pre-create `logisticos.driver.job.dropped` on the live broker before those
consumers start, and verify with `--members --verbose`.
