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

### D1 — promo codes, done 2026-09-18 (nothing discounted until a tenant creates an offer)
New service `services/promotions` (port 8022, DB `svc_promotions`, gateway prefix
`/v1/promotions`, since `/v1/offers` is dispatch's gig board). `8711b17f`, `d1398ebc`, `d4bdf479`.
- **Rules (pure, 30 tests):** window = weekdays 11th–24th on the tenant's local day
  (`PROMOTIONS__UTC_OFFSET_MINUTES`, 480 — no tenant time zone exists); one windowed code per
  account per calendar month; once-per-account codes; the full stack (code vs corporate, larger
  wins, tie to corporate; tier within its cap; credit last with rollover; ceiling
  `min(20% gross, 50 units)` in the move's currency, and it says when it binds).
- **Ledger:** `promotions.redemptions` with partial unique indexes (one windowed per month, one
  once-per-account) among unreleased rows; one per booking (`shipment_id` UNIQUE) so a retried
  create is not a second spend. `shipment.cancelled` releases (decision: a cancelled booking gets
  its code back — the design is silent).
- **order-intake:** `promo_code` on the quote → promotions `price` (mesh-internal); discount off
  billed only (`accessorial_paid_cents` untouched); token signs net amount + code + account. At
  create, the code is spent **before** the payment intent; failure after that releases it; a code
  spent since the quote → 409 `PROMO_ALREADY_USED`; a token priced for another account → 422.
  Only `code` lines are booked; tier/credit need their own redemption (D2).
- **App:** Offers screen (month grid, code check, offer cards), code field + discount lines on the
  plan; total = rows − discounts, shown only when it equals what the server charges.
- **Admin:** API only — `POST /v1/promotions/admin/offers` (`campaigns:create`), no portal UI yet.
**Deploy:** create `svc_promotions` on the live Postgres (init.sql only runs on a fresh volume),
add the `promotions` service to the Dokploy compose, set `SERVICES__PROMOTIONS_URL` on
api-gateway and order-intake. No new Kafka topic (consumes `order.shipment.cancelled`).
**D2 next:** loyalty tiers (needs a completed-moves projection — consume `shipment.created` +
`delivery.completed`, not a call back into order-intake, which would be circular at quote time),
credit ledger + referral, corporate rate, campaign inbox (engagement).

### D2 — tiers, credit, referrals, corporate rate, campaign inbox, done 2026-09-18
`2efd9be1` promotions, `f122e67a` order-intake, `0bfc7408` + `28bb64cd` engagement,
`b571adfe` app. **Every discount line promotions prices is now charged**, not only the code.
- **promotions (migration 0002):** tier ladder by completed moves in 365 days (accessorial bps,
  cap, referral multiplier; admin replaces it whole); append-only credit ledger (applied under an
  advisory lock with a balance check, returned once on cancel); one referral code per account,
  claimable only by an account with no booking yet, paying the referrer reward × tier multiplier
  on the invitee's first completed move; corporate accounts linked by code or email domain.
  One consumer (`promotions-events`, earliest) follows `shipment.created`, `delivery.completed`,
  `shipment.cancelled`, retrying in place. Redeem commits every line in one transaction,
  idempotent per shipment.
- **order-intake:** every quote is priced by promotions (code or not, `loyalty_program` from
  the plan); all lines signed into the token (`discounts[]`; old tokens read as one code line)
  and redeemed together before the payment intent; credit spent since the quote → 409
  `CREDIT_CHANGED`; the quote says when a corporate rate beat a valid code.
- **engagement (migration 0010):** each campaign send keeps its rendered title/body, deep link
  and normalised address; `GET /v1/engagement/inbox`, `POST …/:id/read`, `POST …/read-all`.
  **Matching:** user id, or the email/phone on the token — campaign recipients are CDP profiles,
  whose ids are never app user ids.
- **App:** Offers gains member tier + ladder, company rate, referrals (Share, claim); Payments
  gains the credit panel; new Inbox screen with a home dot; plan names every discount kind.
**Config (all default off):** `PROMOTIONS__REFERRAL_REWARD_CENTS` (0),
`PROMOTIONS__CREDIT_CURRENCY` (PHP); tiers exist only once a tenant PUTs a ladder; loyalty needs
the tenant's plan to include `loyalty_program`.
**Deploy:** redeploy promotions (0002), order-intake, engagement (0010). No new Kafka topic.
The promotions consumer starts from earliest, so retained bookings/completions seed tier counts.
**Follow-up done 2026-09-19** (`06e29c21`, `2d161ebb`, `376a4662`): admin portal **Promotions** page
(codes, ladder, company rates on/off, credit grants by customer email/phone via new identity
`GET /v1/users/lookup`); **tier-upgrade push** (`logisticos.promotions.tier.changed`, new topic —
**pre-create before deploy**; upgrades only; completions >24h old announce nothing, so the
earliest-offset replay does not spam). A ladder is refused where the plan lacks `loyalty_program`.
**Found and fixed on the way:** identity serialized `password_hash` on every user endpoint
(`/v1/users/me`, `/v1/users/:id`, `/v1/users`); and no non-campaign push carried its deep link,
so invoice-receipt and support-answer taps never navigated.
**Still open:** campaign pushes address CDP profile ids, not app user ids; the admin page was
type-checked and unit-tested but not rendered.

### B — intent routing, done 2026-09-19 (`32e7c569`, `f2e48607`, `65d14ecf`)
- ai-layer `POST /v1/agents/classify`: stateless (no session, no tools), one structured answer
  `{intent: book|home_move|support, confidence, extracted}` including the property fields, from
  `ANTHROPIC__CLASSIFY_MODEL` (default Haiku 4.5). Held to rules: closed intents, clamped numbers,
  support only with a running job. Plan-gated like chat (`can_use_ai`); the prompt is never logged.
- App: Home/Voice → Thinking, whose first step waits on it (5 s); unsure, refused or offline falls
  back to the regex — and offline, the handoff's home rules run first. Support → Support screen.
- order-intake 0014 `shipment_intake`: the parse is stored with the booking, best-effort.

### A — whole-home moves, done 2026-09-19 (`b3657524`, `a5e5b5fc`, `a02dcb32`)
- order-intake: `domain/home_move` (property, survey rule, the arithmetic in integers, the calendar
  with the two-day survey gap); 0015 `home_catalogue` (55 design presets as platform defaults,
  tenant overrides per group); 0016 `home_moves`. `GET /v1/shipments/home/{catalogue,slots}`,
  `POST /v1/shipments/home/quote` (server catalogue + server-geocoded distance; the token carries the
  whole job, `kind`-discriminated), `POST /v1/shipments/home` (verifies, checks the schedule, mints
  a parcel-shaped token for the shared create path), `GET /v1/shipments/:id/home`.
  `ServiceType::HomeMove` is refused on the parcel endpoint; its schedule is `#[serde(skip)]`
  (it decides the cancellation tier); no 70 kg cap; auto-dispatch off.
- App: six screens (A1–A6), the Home tile, and the read-back panel from the prompt.
**Config:** `HOME_MOVE__TRIP_CENTS` (off at 0), `HOME_MOVE__PER_KM_CENTS`,
`HOME_MOVE__HELPER_HOUR_CENTS`, `HOME_MOVE__ASSEMBLY_CENTS`, `HOME_MOVE__PACKING_CENTS`,
`HOME_MOVE__SURVEY_CENTS`, truck size and payload, `HOME_MOVE__UTC_OFFSET_MINUTES` (480).
**Not built (a decision or a service is missing):** crew identity (ops assign; no crew model), the
survey correcting the inventory, promotions on home moves, scan/photo/video capture (no detection
service), slot capacity (every window is offered), the survey fee's refund under Part C, and road
distance (straight line, as the rate card).

## Whole-home follow-up — the architect's decisions (2026-09-19)

Decided by the user; **(default)** marks where a range or an unstated case was filled in here and
is config, not code.

1. **Crew.** The contractor who accepts the job brings the crew the app states. Lead + helpers by
   size: Studio 1+1, 1BR 1+2, 2BR 1+3, 3BR 1+4, 4BR 1+5. **(default)** 5BR+ 1+6, villa 6BR+ 1+7;
   offices: up to 10 desks 1+3, 10–30 1+5, 30–75 1+7, whole floor 1+9. Per truck at least one
   driver and two helpers: crew = max(size table, trucks × 3), plus one helper each for a walk-up,
   a heavy item and a long carry (the design's access rules). Priced as helpers × hours.
2. **Multi-truck.** Over **75 m³** the move is a Large Estate: single-truck options (the
   "fewer trucks, more trips" plan) are locked out. **Model A — Enterprise Team Lead** is built:
   such a job is offered only to leads tagged multi-truck capable with enough trucks and helpers;
   they accept one job and bring both trucks. **Model B (paired co-leads) is not built** — it needs
   multi-driver assignment in dispatch; it is the fallback if the enterprise pool is too thin.
3. **Customer view.** One line: "Team {lead} · N trucks & M-person crew", once a lead accepts.
4. **Provider filtering.** Drivers carry service lines (`freight_move`, `home_move`) and coverage
   (`local`, `international`). Existing drivers default to freight/local; home moves go only to
   home-move leads, international ones only to international-capable leads.
5. **Survey addendum.** The surveyor (the team lead) lists additional items, resources and packing
   materials — only ever adding; the agreed price can never go down. The customer approves or
   declines in the app; approval charges the difference.
6. **Promotions on home moves:** unchanged (none).
7. **Survey fee.** Charged at booking as a deposit credited toward the move. Cancelling more than
   **(default) 12 h** before the survey (range given 4–12 h) refunds it in full; once the survey is
   done, or the lead has arrived, it is retained and paid to the lead. A reschedule rolls it over.
8. **Slot capacity.** Leads set working days and off days and a max jobs per day **(default 1)**.
   A window is open while available home-move leads for that date exceed moves already booked in
   it (and multi-truck leads exceed Large Estate moves); full windows grey out, and the response
   names the next open one. **Waitlist and surge: not built.**
