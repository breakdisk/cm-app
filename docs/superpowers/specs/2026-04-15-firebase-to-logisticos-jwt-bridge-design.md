# Firebase → LogisticOS JWT Bridge

**Date:** 2026-04-15
**Status:** Proposed
**Scope:** Web portal → backend API authentication bridge. No changes to mobile auth, no changes to backend RLS.

---

## Problem

Web portals (merchant, admin, partner, customer) authenticate users via **Firebase Auth** on the landing app (per [2026-04-08-single-domain-auth-design](2026-04-08-single-domain-auth-design.md)). The resulting `__session` httpOnly cookie holds a Firebase ID token.

Backend services (identity, order-intake, dispatch, etc.) require a **LogisticOS JWT** that carries `tenant_id`, `tenant_slug`, `subscription_tier`, `roles`, and `permissions` ([libs/auth/src/claims.rs](libs/auth/src/claims.rs#L7-L25)). [ADR-0008](docs/adr/0008-multi-tenancy-rls-strategy.md#L39) mandates that `tenant_id` must come from a trusted server-signed JWT, never user input. Firebase tokens don't carry `tenant_id`.

Today, portal pages (e.g. [shipments/page.tsx:511](apps/merchant-portal/src/app/(dashboard)/shipments/page.tsx#L511)) read `localStorage.getItem("access_token")` and get `null` — users who signed in with Firebase have no LogisticOS JWT anywhere. Result: "Not authenticated" on any protected API call.

---

## Design

**Exchange the Firebase ID token for a LogisticOS JWT server-side, during the existing `/api/auth/session` flow.** Portal pages continue to send the LogisticOS JWT on every API call, unchanged from the original intent. Firebase remains the identity provider; LogisticOS identity service remains the backend auth authority.

```
┌─────────────┐  1. Firebase sign-in (Google/magic-link)
│  Browser    │──────────────────────────────────┐
└─────────────┘                                   ▼
      │                                    ┌──────────────┐
      │ 2. POST /api/auth/session          │  Firebase    │
      │    { idToken, role }               │  (Google)    │
      ▼                                    └──────────────┘
┌─────────────┐  3. verifyIdToken (Firebase Admin)
│  Landing    │──────────────────────────────────┐
│  /api/auth/ │                                   ▼
│  session    │                            ┌──────────────┐
└─────────────┘                            │  Firebase    │
      │                                    │  Admin SDK   │
      │ 4. POST /v1/auth/exchange-firebase └──────────────┘
      │    { firebase_uid, email, role }
      ▼
┌──────────────────┐
│  identity        │  5. Lookup or provision user+tenant by
│  service         │     firebase_uid → mint LogisticOS JWT
└──────────────────┘
      │
      │ 6. { access_token, refresh_token, user }
      ▼
┌─────────────┐
│  Landing    │  7. Set TWO cookies on os.cargomarket.net:
│  /api/auth/ │       __session   → Firebase ID token (existing)
│  session    │       los_at      → LogisticOS access token
│             │       los_rt      → LogisticOS refresh token
└─────────────┘
      │
      ▼
Browser, on subsequent portal API calls:
   fetch(url, { credentials: "include" })  ─► Cookie: los_at=<jwt>
   Portal client code reads los_at from cookie (or proxies)
   and sends Authorization: Bearer <jwt>
```

---

## Contract: new identity endpoint

### `POST /v1/auth/exchange-firebase` (internal only)

**Authentication:** mTLS or shared secret header (`X-Internal-Secret`), enforced by identity service config. This endpoint must NOT be exposed through the public API gateway.

**Request:**
```json
{
  "firebase_uid":  "string (Firebase UID)",
  "email":         "string",
  "email_verified": true,
  "role":          "merchant | admin | partner | customer",
  "display_name":  "string (optional)"
}
```

**Response (200):**
```json
{
  "data": {
    "access_token":  "<LogisticOS JWT, 1h>",
    "refresh_token": "<LogisticOS refresh, 30d>",
    "token_type":    "Bearer",
    "expires_in":    3600,
    "user": {
      "id":          "uuid",
      "tenant_id":   "uuid",
      "tenant_slug": "string",
      "email":       "string",
      "roles":       ["merchant"]
    }
  }
}
```

**Response (403):** `{ "error": { "code": "tenant_not_provisioned", "message": "..." } }` — Firebase user exists but no tenant assignment. Landing redirects to onboarding.

### Behavior (identity service)

1. Look up `auth_identities` row by `(provider = "firebase", provider_subject = firebase_uid)`.
2. If found → load `users.tenant_id`, `tenants.slug`, `tenants.subscription_tier`, role/permission set → mint `Claims` → return.
3. If not found → **lazy onboarding** (policy-driven):
   - **merchant** role: create a *draft* tenant (`tenants.status = 'draft'`, slug = `draft-<uid-prefix>`, `subscription_tier = 'starter'`) + user with `OWNER` role, attach Firebase identity. Mint a JWT whose permissions only allow the onboarding endpoints (`tenants:update-self`, `billing:setup`). Emit `user.provisioned` Kafka event with `{ status: "draft" }`. Response includes `{ "user": { "onboarding_required": true } }` so landing redirects to `/setup` instead of `/merchant`.
   - **customer** role: only auto-link when the Firebase sign-in carries a signed partner context (`?partner=<tenant_slug>&sig=<hmac>` set by a white-label link, verified server-side). Create user in that tenant with `CUSTOMER` role. If no partner context → 403 `tenant_required`.
   - **admin / partner** role: return 403 `tenant_not_provisioned`. These roles require explicit invite.
4. Log every exchange to `audit_events` with `actor = firebase:<uid>`, `action = "auth.exchange"`.

### Onboarding state machine

Draft tenants have a restricted claim set. The `/v1/tenants/me/finalize` endpoint promotes `status: draft → active` once the user submits business name, currency, and region. `finalize` swaps in the full permission set on next refresh. Until then, every non-onboarding API call returns 403 `onboarding_required` so no stray requests hit RLS-protected resources with a half-built tenant.

### New schema (identity DB)

```sql
-- Already exists: users (id, tenant_id, email, ...)
-- New join table for external identity providers
CREATE TABLE auth_identities (
  id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id           UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider          TEXT NOT NULL,              -- "firebase" | "saml" | ...
  provider_subject  TEXT NOT NULL,              -- Firebase UID
  linked_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (provider, provider_subject)
);
CREATE INDEX ON auth_identities (user_id);
```

RLS policy: `USING (user_id IN (SELECT id FROM users WHERE tenant_id = current_tenant()))`. Provisioning path bypasses RLS via the service role (same pattern as existing tenant creation).

---

## Cookie strategy

Landing `POST /api/auth/session` sets **three** cookies, all `HttpOnly; Secure; SameSite=Lax; Domain=os.cargomarket.net; Path=/`:

| Cookie | Contents | TTL | Purpose |
|--------|----------|-----|---------|
| `__session` | Firebase ID token | 7 days | Portal middleware (Edge, via `jose` + JWKS per [project_firebase_auth_plan.md](memory/project_firebase_auth_plan.md)) |
| `los_at` | LogisticOS access JWT | 1 hour | Sent on every API call (via `credentials: "include"`) |
| `los_rt` | LogisticOS refresh JWT | 30 days | Used by `/api/auth/refresh` to mint new `los_at` |

All cookies share the `os.cargomarket.net` domain, so the browser forwards them to the landing app's rewrite proxy, which forwards to portal containers, which then proxy backend API calls.

### `POST /api/auth/refresh` (new landing route)

- Reads `los_rt` cookie.
- Calls identity `POST /v1/auth/refresh`.
- Rotates `los_at` (and optionally `los_rt`).
- Returns 204.

Portal client-side wraps `fetch` with a 401 interceptor that calls `/api/auth/refresh` once, then retries. On second 401 → redirect to `/login`.

### `POST /api/auth/signout`

- Clears all three cookies.
- Calls identity `POST /v1/auth/revoke` to revoke the refresh JTI.
- Redirects to `/`.

---

## Portal integration

### Reading `los_at` from portal pages

Two options:

**A. Expose via a portal route handler (recommended).**
Each portal adds `app/api/token/route.ts` (Node runtime):
```ts
export async function GET(req: NextRequest) {
  const token = req.cookies.get("los_at")?.value;
  const res = NextResponse.json({ token: token ?? null });
  // Browser caches 60s; CDN/edge never caches (prevents cross-user leaks)
  res.headers.set("Cache-Control", "private, s-maxage=0, max-age=60, must-revalidate");
  res.headers.set("Vary", "Cookie");
  return res;
}
```
Portal client fetches `/<role>/api/token` on mount, stores in memory (NOT localStorage), attaches to `Authorization` header. The 60s private cache absorbs dashboard refresh bursts; `s-maxage=0` + `Vary: Cookie` ensures no Vercel/Traefik/CDN layer ever serves one user's token to another.

**B. Server-side only (stricter).**
Portal pages become Server Components; API calls happen via server route handlers that read the cookie directly. Better security but requires refactoring existing client-heavy pages. Defer to a later pass.

**Pick A for the unblock**, with a note to migrate hot paths to B.

### Remove localStorage writes

Delete `localStorage.setItem("access_token", ...)` from:
- [apps/merchant-portal/src/app/(auth)/login/page.tsx](apps/merchant-portal/src/app/(auth)/login/page.tsx)
- All other `(auth)/login` pages in each portal

These routes remain dead until removed per the original spec (which calls for centralized landing login). Keeping them dormant is fine.

### Replace `localStorage.getItem("access_token")`

All call sites become:
```ts
async function authFetch(url: string, init: RequestInit = {}) {
  const { token } = await fetch("/<role>/api/token").then(r => r.json());
  return fetch(url, {
    ...init,
    headers: { ...init.headers, Authorization: `Bearer ${token}` },
  });
}
```

Add `apps/<portal>/src/lib/auth-fetch.ts` as the single pattern. Grep for `localStorage.getItem("access_token")` and migrate.

---

## Security

| Concern | Mitigation |
|---------|-----------|
| Exchange endpoint abused to mint arbitrary tokens | Locked to internal network + `X-Internal-Secret` header; never exposed via public gateway |
| Firebase token stolen from browser | `__session` is `HttpOnly` — JS cannot read it. `los_at` same |
| XSS steals `los_at` via token endpoint | `los_at` lives in cookie, not JS memory long-term. Token endpoint requires `__session` cookie = same origin |
| Tenant escalation via crafted `role` | Landing re-verifies Firebase custom claim `role` matches requested role BEFORE calling exchange. Identity ignores role from request on lookup path (derives from DB) |
| Stale JWT after role change in Firebase | `los_at` 1h TTL bounds exposure. Refresh endpoint re-reads DB. Admin role changes require a hard sign-out (document in runbook) |
| CSRF | `SameSite=Lax` blocks cross-site cookie-bearing POSTs + **required custom header** `X-LogisticOS-Client: web` on every state-changing request (`POST`/`PUT`/`PATCH`/`DELETE`). Enforced in `libs/auth` middleware: requests without the header → 403. SOP prevents third-party sites from adding custom headers without a CORS preflight, which our gateway denies for unknown origins. Mobile apps send `X-LogisticOS-Client: mobile` |
| Cross-user cache leak on `/api/token` | `Cache-Control: private, s-maxage=0` + `Vary: Cookie` stops every CDN / reverse proxy from sharing responses. Browser-only cache, 60s max |
| Draft tenant used to hit protected endpoints | Draft-tenant JWT carries only onboarding permissions. `require_permission` middleware returns 403 `onboarding_required` on all other routes |

---

## Out of scope

- Replacing Firebase with LogisticOS OIDC server (tracked separately; see [single-domain-auth-design:244](docs/superpowers/specs/2026-04-08-single-domain-auth-design.md#L244))
- Mobile app auth (driver-app, customer-app) — they use native identity service endpoints directly and don't touch Firebase
- Edge-runtime middleware fix with `jose` + JWKS — pre-existing work from [project_firebase_auth_plan.md](memory/project_firebase_auth_plan.md), orthogonal to this bridge
- Multi-tenant users (one Firebase account mapped to N tenants with tenant picker) — deferred; v1 is 1:1

---

## Migration

1. Ship identity migration + exchange endpoint (backwards compatible — no existing behavior changes).
2. Ship landing `/api/auth/session` update to call exchange + set `los_at`/`los_rt`.
3. Ship portal `/api/token` route + `auth-fetch.ts` helper.
4. Migrate call sites from `localStorage` → `authFetch` one portal at a time. Merchant portal first (active incident).
5. Close PR #5 (the "restore JWT login page" approach — superseded by this design).
6. Delete the dormant `(auth)/login/*` routes in all portals once all call sites migrated.

No data migration needed. Existing non-Firebase users (seed data like `merchant@demo.com`) keep working via the original `POST /v1/auth/login` path; only the web UI stops using it.

---

## Resolved decisions (2026-04-15)

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **Lazy onboarding.** Merchant → draft tenant + `/setup` flow. Customer → signed white-label partner context required. Admin/partner → invite only. | Low friction for self-service roles; no unscoped data for draft tenants; gated escalation for privileged roles |
| 2 | **`Cache-Control: private, s-maxage=0, max-age=60, must-revalidate` + `Vary: Cookie`** on `/<role>/api/token` | 60s browser cache absorbs refresh bursts; explicit `s-maxage=0` prevents any CDN/proxy from sharing tokens across users |
| 3 | **`SameSite=Lax` + required `X-LogisticOS-Client` custom header** on every state-changing request | Cheaper and more composable than double-submit tokens; relies on SOP/CORS guarantees already in the browser; mobile apps opt into `mobile` value for telemetry + client-specific policy later |
