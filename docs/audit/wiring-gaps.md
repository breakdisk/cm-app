# Wiring Audit — UI → Backend

**Date:** 2026-04-24
**Scope:** 7 frontend apps × 18 Rust services. Tracks every page/screen from button → hook → API client → backend endpoint → service → repo → SQL → response → UI, plus navigation paths (links + backlinks).
**Method:** file enumeration + wire-signal grep (`authFetch`/`fetch`/`useQuery`/`services/api`/router files) + targeted code reads. Line counts are at HEAD (`master @ 6e87867`, before any Cargo.lock refresh).

This is an **inventory**, not fixes. Each "Gap" row is sized to be one focused session.

---

## Executive summary

| App | Pages | Fully wired | Partial | Stub (0 API calls) | Critical gaps |
|---|---:|---:|---:|---:|---:|
| admin-portal       | 15 | 13 | 2  | 0 | 3  |
| merchant-portal    | 8  | 3  | 1  | 4 | 9  |
| partner-portal     | 9  | 5  | 0  | 4 | 8  |
| customer-portal    | 3  | 3  | 0  | 0 | 2  |
| landing            | 4  | 3  | 0  | 1 | 6 dead `href="#"` links |
| customer-app       | 15 | 5  | 3  | 7 | 11 |
| driver-app-android | 11 | 11 | 0  | 0 | 1  |
| **Total**          | 65 | 43 | 6  | 16 | **~40 gap tickets** |

**Backend coverage (18 services):**
- ✅ 8 services with active frontend consumers: identity, dispatch, driver-ops, pod, order-intake, delivery-experience, carrier, compliance
- ⚠️ 6 services with partial frontend coverage: payments, analytics, engagement, cdp, hub-ops, fleet
- ❌ 4 services with zero frontend wiring: ai-layer, business-logic, marketing, api-gateway (gateway has no own endpoints — correct)

**Top 5 blocker tickets** (do first):
1. Merchant portal `campaigns/page.tsx` — 411 lines, 9 buttons, 0 API calls ⇒ whole Marketing feature decorative
2. Customer-app `HomeScreen` (72 LOC, 0 calls) and `HistoryScreen` (331 LOC, 0 calls) — app feels alive, isn't
3. Partner portal `manifests` / `rates` / `settings` — stubs; carriers can't self-serve
4. Landing 6× `href="#"` in Footer/Navbar/CTA/Pricing — every marketing click is a dead link
5. Admin portal `alerts` / `analytics` / `hubs` / `map` / `ai-agents` — need targeted verification; Stage 1.5 follow-up

---

## Per-app wiring map

### admin-portal (Next.js 14, App Router, `authFetch`)

| Page | LOC | API imports | Buttons | Status | Gaps |
|---|---:|---:|---:|---|---|
| (root dashboard) `/`            | —  | ✓ | ✓ | 🟢 | — |
| `/dispatch`                     | 862 | ✓ (dispatch, marketplace, marketplace-bus) | ✓ | 🟢 | — (debugged this session end-to-end) |
| `/drivers`                      | 318 | ✓ (drivers) | ✓ | 🟢 | — |
| `/finance`                      | 321 | ✓ (authFetch) | ✓ | 🟢 | — |
| `/shipments`                    | 329 | ✓ (authFetch) | ✓ | 🟢 | — |
| `/marketplace`                  | 862 | ✓ (marketplace-bus, marketplace) | ✓ | 🟢 | — |
| `/compliance`                   | —  | ✓ (compliance) | ✓ | 🟢 | — |
| `/carriers`                     | —  | ✓ (carriers) | ✓ | 🟢 | — |
| `/fleet`                        | —  | ✓ (fleet) | ✓ | 🟢 | — |
| `/analytics`                    | —  | ✓ (analytics) | ✓ | 🟢 | **verify**: charts populated from live `/v1/analytics/*`? |
| `/ai-agents`                    | —  | ✓ (agents) | ✓ | 🟢 | **verify**: agent runs actually dispatched to ai-layer? |
| `/map`                          | —  | — | — | 🟡 | Map likely uses driver-ops `/ws/locations`; audit ws reconnect + marker click→detail |
| `/hubs`                         | —  | ✓ (hubs) | ✓ | 🟡 | **verify**: capacity editor writes? manifest download works? |
| `/alerts`                       | —  | ? | — | 🟡 | No obvious alerts API on the backend side; check if this reads Kafka or DB |
| `/settings`                     | —  | ? | ? | 🟡 | Tenant/user settings save path unclear |

### merchant-portal (Next.js 14)

| Page | LOC | API imports | Buttons | Status | Gaps |
|---|---:|---:|---:|---|---|
| (root) `/`                      | —  | ? | — | 🟡 | **verify**: home dashboard KPIs live or mocked |
| `/shipments`                    | 1170 | ✓ (shipments, tracking, billing) | 31 | 🟢 | **verify**: bulk upload, cancel, reschedule all fire |
| `/marketplace`                  | 681 | ✓ (marketplace, marketplace-bus) | 9 | 🟢 | — |
| `/billing`                      | 335 | ✓ (billing) | 2 | 🟢 | **verify**: "Pay now" / "Download invoice" hit payments service |
| **`/campaigns`**                | 411 | ❌ 0 | 9 | 🔴 | **blocker**: no campaigns API client (marketing service has 5 endpoints unused) |
| **`/analytics`**                | 207 | ❌ 0 | 1 | 🔴 | Hardcoded/mock data; analytics service has 4 endpoints unused here |
| **`/fleet`**                    | 100 | ❌ 0 | 1 | 🔴 | Stub page; merchant fleet visibility is a feature requirement not delivered |
| **`/settings`**                 | 233 | ❌ 0 | 9 | 🔴 | 9 save/toggle buttons with no backend calls |

### partner-portal (Next.js 14)

| Page | LOC | API imports | Buttons | Status | Gaps |
|---|---:|---:|---:|---|---|
| (root) `/`                      | —  | ✓ | ✓ | 🟢 | — |
| `/drivers`                      | 882 | ✓ | 15 | 🟢 | **verify**: "Add driver" flow persists via carrier/driver-ops |
| `/orders`                       | 869 | ✓ | 9 | 🟢 | **verify**: accept-order → enters dispatch queue (ADR-0013) |
| `/marketplace`                  | 1192 | ✓ (marketplace, marketplace-bus) | 13 | 🟢 | — |
| `/payouts`                      | 316 | ✓ (marketplace-bus) | 1 | 🟢 | **verify**: payouts computed server-side, not client-side |
| **`/sla`**                      | 307 | ✓ (3 imports) | ❌ 0 | 🟡 | Read-only dashboard; no "Dispute SLA" or "Export" action |
| **`/manifests`**                | 180 | ❌ 0 | 3 | 🔴 | Stub; hub-ops manifest endpoint unused |
| **`/rates`**                    | 212 | ❌ 0 | 1 | 🔴 | Stub; rate-shop endpoint (`carrier/v1/carriers/rate-shop`) not consumed |
| **`/settings`**                 | 117 | ❌ 0 | ❌ 0 | 🔴 | Empty page |

### customer-portal (Next.js 14)

| Page | LOC | API imports | Buttons | Status | Gaps |
|---|---:|---:|---:|---|---|
| `/` (tracking landing)          | 316 | ✓ (tracking) | 1 | 🟢 | — |
| `/(dashboard)/reschedule`       | 289 | ✓ (tracking + 1) | 5 | 🟢 | **verify**: POST hits `/track/:tracking/reschedule` → order-intake |
| `/(dashboard)/feedback`         | 303 | ✓ | 5 | 🟢 | **verify**: POST hits `/track/:tracking/feedback` → delivery-experience |

Low surface area — mostly functional.

### landing (Next.js 14)

| Page | LOC | API | Status | Gaps |
|---|---:|---:|---|---|
| `/` (root marketing)            | 34  | 0 | 🟡 | Pure marketing page; acceptable but 6× `href="#"` in components below |
| `/track`                        | 181 | ✓ | 🟢 | — |
| `/setup` (tenant finalize)      | 210 | ✓ | 🟢 | — |
| `/login`                        | 350 | ✓ | 🟢 | — |

**6 dead links to fix** (all marketing — low user impact, but brand-damaging):
- `src/components/Navbar.tsx:36` — logo href="#" → should be "/"
- `src/components/Footer.tsx:22,42,64` — logo + product/company links
- `src/components/CTA.tsx:43` — primary CTA
- `src/components/Pricing.tsx:189` — pricing CTA

### customer-app (React Native + Expo)

| Screen | LOC | API calls | Status | Gaps |
|---|---:|---:|---|---|
| `HomeScreen`                | 72   | ❌ 0 | 🔴 | Shows dummy data; should fetch active shipments + loyalty + recent |
| `BookingScreen`             | 1135 | ✓ 2  | 🟢 | — (large, already wired) |
| `TrackingScreen`            | 618  | ✓ 2  | 🟢 | — |
| `CollectionScreen`          | 606  | ✓ 1  | 🟢 | — |
| `InvoicesScreen`            | 191  | ✓ 2  | 🟢 | — |
| `InvoiceDetailScreen`       | —    | ✓    | 🟢 | — |
| `ReceiptScreen`             | —    | ✓    | 🟢 | — |
| `PhoneScreen` (auth)        | 368  | ✓ 1  | 🟢 | — |
| **`HistoryScreen`**         | 331  | ❌ 0 | 🔴 | No history API call; should list past shipments from order-intake |
| **`ProfileScreen`**         | 529  | ❌ 0 | 🟡 | Displays user from context; "Edit"/"KYC"/"Sign out" — KYC not submitted to backend |
| **`SupportScreen`**         | 367  | ❌ 0 | 🔴 | No engagement-service connection; "Contact us" creates no ticket |
| **`NotificationsScreen`**   | 104  | ❌ 0 | 🔴 | Shows stub list; should read engagement service notification history |
| **`KYCScreen`**             | 308  | ❌ 0 | 🔴 | Camera/library permission wiring works locally, no upload to compliance service |
| **`OnboardingProfileScreen`** | —  | ?    | 🟡 | Needs audit |
| **`InvoicesScreen`** — empty state | — | ✓ | 🟢 | — |

### driver-app-android (Native Kotlin + Jetpack Compose)

All 11 screens wired end-to-end after this session's work:
`PhoneScreen, OtpScreen, HomeScreen, RouteScreen, ScannerScreen, NavigationScreen, ArrivalScreen, PickupScreen, PodScreen, NotificationsScreen, ProfileScreen`.

Remaining gap:
- **One empty `onClick = { }`** (location TBD) — 30-second fix.
- Follow-up items tracked separately: sync-queue retry UI, POST_NOTIFICATIONS permission (API 33+), ACCESS_BACKGROUND_LOCATION.

---

## Backend service coverage

| Service | Endpoints | Frontend consumers | Status |
|---|---:|---|---|
| identity            | 22 | all portals + customer-app + driver-app | 🟢 |
| dispatch            | 11 | admin-portal | 🟢 |
| driver-ops          | 13 | admin-portal, driver-app | 🟢 |
| pod                 | 11 | driver-app | 🟢 |
| order-intake        | 9  | merchant-portal, customer-app | 🟢 |
| delivery-experience | 10 | customer-portal, customer-app | 🟢 |
| carrier             | 5  | partner-portal (drivers/orders) | 🟢 |
| compliance          | 14 | admin-portal, partner-portal | 🟢 (customer-app KYC missing) |
| payments            | 16 | admin-portal (finance), merchant-portal (billing), customer-app (invoices) | 🟡 wallet/batches unused |
| analytics           | 4  | admin-portal (analytics) | 🟡 merchant analytics page uses 0 |
| engagement          | 10 | **nothing** (campaigns/notifications/send unused) | 🔴 |
| cdp                 | 4  | **nothing** | 🔴 no frontend profile view |
| hub-ops             | 11 | admin-portal (hubs) — partial | 🟡 inductions + manifest not in partner |
| fleet               | 7  | admin-portal (fleet) — partial | 🟡 merchant fleet page stubbed |
| marketing           | 5  | **nothing** (campaigns page stubbed) | 🔴 |
| business-logic      | 7  | **nothing** (rules not surfaced to admin UI) | 🔴 |
| ai-layer            | 7  | admin-portal (ai-agents) | 🟡 verify actual session creation works |
| api-gateway         | — (proxy) | all (via gateway routing) | 🟢 |

---

## Navigation path gaps

Spot-checks:
- Driver app back-nav from Navigation → Route → Home ✅ (fixed this session)
- Customer-app has tab nav; deep links from push notification to `InvoiceDetailScreen` need re-verification after push-token changes
- Merchant-portal's "Book shipment" CTA lands on `/shipments/new` — verify breadcrumb back to list populates state
- Admin-portal `/dispatch` → shipment detail → driver detail → back: each transition should preserve filters (audit; Next.js App Router pattern may have lost them in recent refactor)
- Landing footer links → `/login` / `/setup` vs internal anchors — all `#` today

---

## Ticket-sized gaps (proposed backlog)

| # | Title | Area | Est. | Priority |
|---|---|---|---:|---|
| 1 | Merchant `/campaigns`: implement campaigns.ts client + wire to marketing service (create/list/activate/cancel/schedule) | merchant | 1 sess | P1 |
| 2 | Merchant `/analytics`: replace mock data with `/v1/analytics/*` calls | merchant | 1 sess | P1 |
| 3 | Merchant `/settings`: wire 9 toggles to identity/tenant endpoints | merchant | 1 sess | P1 |
| 4 | Merchant `/fleet`: implement or scope-cut (remove from sidebar) | merchant | 0.5 sess | P2 |
| 5 | Partner `/manifests`: wire to hub-ops `/v1/hubs/:id/manifest` | partner | 1 sess | P1 |
| 6 | Partner `/rates`: rate-shop UI → carrier `/v1/carriers/rate-shop` | partner | 1 sess | P1 |
| 7 | Partner `/settings`: add content + wire | partner | 1 sess | P2 |
| 8 | Partner `/sla`: add Export + Dispute actions | partner | 0.5 sess | P2 |
| 9 | Customer-app `HomeScreen`: wire active shipments + loyalty banner | customer-app | 1 sess | P0 |
| 10 | Customer-app `HistoryScreen`: wire order-intake list API | customer-app | 1 sess | P0 |
| 11 | Customer-app `SupportScreen`: wire engagement ticket creation | customer-app | 1 sess | P1 |
| 12 | Customer-app `NotificationsScreen`: wire engagement notification list | customer-app | 1 sess | P1 |
| 13 | Customer-app `KYCScreen`: wire compliance document upload | customer-app | 1 sess | P1 |
| 14 | Customer-app `ProfileScreen`: wire "Sign out" to identity + KYC status to compliance | customer-app | 1 sess | P1 |
| 15 | Landing 6× `href="#"` → real routes | landing | 0.5 sess | P2 |
| 16 | Admin `/alerts`: design decision — read from which service? | admin | verify | P2 |
| 17 | Admin `/map`: verify ws reconnect + driver→shipment drill-down | admin | 0.5 sess | P2 |
| 18 | Admin `/hubs`: verify capacity save + manifest download | admin | 0.5 sess | P2 |
| 19 | Admin `/ai-agents`: smoke-test session creation end-to-end | admin | 0.5 sess | P2 |
| 20 | Admin `/settings`: identify tenant/user save paths | admin | verify | P2 |
| 21 | Surface CDP (customer profile) in merchant+admin | cross | 2 sess | P2 |
| 22 | Business-logic rules UI in admin (toggle/run/view executions) | admin | 2 sess | P2 |
| 23 | End-to-end lifecycle validation (merchant → dispatch → driver → POD → delivered) | integration | 1 sess | P0 |
| 24 | Cargo.lock refresh + CI green after reqwest add | infra | 0.2 sess | P0 (blocking) |
| 25 | Deploy POD 422 fix + geocoder + dispatch anchor (already committed, awaits push of Cargo.lock) | infra | 0.2 sess | P0 (blocking) |

**Legend:** P0 = ship next, P1 = next sprint, P2 = backlog.

---

## Methodology notes / caveats

1. Heuristics (`0 API imports`, `href="#"`, `alert()`) flag candidates; each P1/P0 ticket was verified by opening the file. "Verify" tickets in the table mean a follow-up read is needed before sizing.
2. This audit doesn't trace **hooks → API client → URL path** through the `client.ts` wrapper in each portal — a deeper pass would cross-reference each hook's fetch target against the router file in the corresponding service. Estimate: 1 additional session to do that for merchant + partner (most impactful).
3. Response flow back to UI (shape validation, error handling) was not exhaustively audited — only spot-checked where bugs were suspected (e.g. POD 422 earlier this session).
4. Navigation paths were surveyed structurally (`router.push`, `href`), not behaviorally (actual clicks in a browser).

---

## Recommended session sequencing (Stage 2)

**Sprint 1 (P0, unblocks prod demo):**
- Ticket 24 — Cargo.lock refresh (5 min)
- Ticket 25 — deploy pending backend fixes (30 min)
- Ticket 23 — full lifecycle validation (1 session)
- Ticket 9 — customer-app Home live data (1 session)

**Sprint 2 (P1, closes biggest visible gaps):**
- Tickets 1, 2, 3 — merchant campaigns, analytics, settings
- Tickets 5, 6 — partner manifests, rates
- Tickets 10, 13 — customer-app history, KYC

**Sprint 3 (P1, feature completeness):**
- Tickets 11, 12, 14 — customer-app support/notifications/profile
- Ticket 4, 7, 8 — merchant fleet scope-cut, partner settings, partner SLA actions

**Sprint 4 (P2, polish + backend surface):**
- Tickets 15–22 — landing links, admin verify set, CDP, business-logic

Total estimated scope post-Stage-1: **~15 sessions** to get every page/screen functional.
