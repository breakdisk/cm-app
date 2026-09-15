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
