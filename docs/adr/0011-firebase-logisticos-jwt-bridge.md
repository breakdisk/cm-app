# ADR-0011: Firebase Auth → LogisticOS JWT Bridge

**Status:** Accepted
**Date:** 2026-04-15
**Deciders:** Principal Architect, Senior Rust Engineer — Identity & Auth, Staff Frontend Engineer, CISO

---

## Context

The portals (`merchant`, `admin`, `partner`, `customer`) and the landing app use Firebase Auth for the interactive sign-in experience (Google popup, email/password, magic links). The backend services, however, cannot consume Firebase ID tokens directly — they enforce tenant isolation via ADR-0008 (RLS), which requires a LogisticOS JWT carrying `tenant_id`, `role`, and service-scoped claims. Firebase tokens have none of that.

Earlier portal code attempted to work around this by mixing Firebase tokens with a `X-Tenant: <slug>` localStorage header. This produced the production "Not authenticated" symptom and, worse, bypassed the RLS guarantee — any user with a Firebase account could impersonate any tenant by editing localStorage.

### Requirements

1. Portals authenticate users via Firebase (unchanged UX).
2. All backend calls carry a LogisticOS JWT with trusted `tenant_id`, `role`, and permission claims — no client-editable headers.
3. Brand-new users without a tenant can still sign in and complete onboarding (lazy tenant creation).
4. Tokens must be stored in a way that is not readable by page JavaScript (no XSS token theft).
5. Refresh must be transparent to call sites — a 401 from a downstream service should auto-refresh once and retry.
6. The bridge must be testable end-to-end in CI without automating the Firebase popup.

## Decision

Introduce a **server-side bridge** in the identity service that exchanges Firebase ID tokens for LogisticOS JWT pairs. Portals never see the LogisticOS tokens directly — they are stored as `httpOnly` cookies scoped to each portal's domain.

### Cookie layout

| Cookie | Purpose | Lifetime | Flags |
|---|---|---|---|
| `__session` | Firebase ID token (short-lived) | ~1h | `httpOnly`, `secure`, `SameSite=Lax` |
| `__los_at` | LogisticOS access token | ~15m | `httpOnly`, `secure`, `SameSite=Lax` |
| `__los_rt` | LogisticOS refresh token | ~30d | `httpOnly`, `secure`, `SameSite=Lax`, `Path=/api/auth/refresh` |

### Exchange flow (interactive sign-in)

```
Browser  ──Firebase popup──►  Firebase Auth
Browser  ──POST /api/auth/session {idToken}──►  Landing (or portal)
Landing  ──POST /v1/internal/auth/exchange-firebase──►  Identity
          X-Internal-Secret: <HMAC shared secret>
Identity ──verifies Firebase ID token, upserts user, loads tenant(s)──►
Identity ──returns {access_token, refresh_token, user, tenant?}──►  Landing
Landing  ──Set-Cookie: __los_at, __los_rt──►  Browser
```

The `/v1/internal/auth/exchange-firebase` endpoint is **never exposed publicly** — it is gated by a shared HMAC secret (`LOGISTICOS_INTERNAL_SECRET`) that only landing/portal servers hold. The CSRF header `X-LogisticOS-Client: web|mobile|service` is enforced at the identity boundary to prevent cross-origin POSTs.

### Refresh flow

```
authFetch() ──request──► backend          → 401
authFetch() ──POST /api/auth/refresh──►   portal proxy
portal      ──forwards __los_rt cookie──► identity /v1/auth/refresh
identity    ──rotates access + refresh──► portal
portal      ──Set-Cookie: new __los_at, __los_rt──► browser
authFetch() ──retry original request──►   backend   → 200
```

Refresh rotates both tokens; the old refresh token is invalidated server-side.

### Lazy onboarding

Users signing in for the first time have no tenant. The exchange issues a **draft JWT** with a single permission, `tenants:update-self`. The landing `/setup` page calls `POST /api/tenants/finalize`, which calls identity's `/v1/tenants/me/finalize` and then immediately refreshes the token so the browser receives a full-permission active-tenant JWT without a second sign-in.

### Frontend integration

- Each portal owns an `/api/token` route (reads `__los_at`, returns bearer token for same-origin JS that cannot read the cookie directly).
- `lib/auth/auth-fetch.ts` is a tiny wrapper around `fetch` that:
  1. calls `/api/token` (cached),
  2. attaches `Authorization: Bearer`,
  3. on 401 calls `/api/auth/refresh` and retries once,
  4. sends `X-LogisticOS-Client: web`.
- Legacy Axios call sites are migrated to `authFetch` or wired via an async request interceptor.
- Portal middlewares are pass-throughs — auth is enforced at call time and at the service boundary (ADR-0008), never in middleware.

## Consequences

### Positive

- Tenant isolation is cryptographically enforced — no header the client can forge.
- Tokens are not accessible to JavaScript → XSS cannot exfiltrate sessions.
- Brand-new users can sign in and finish onboarding without a second round-trip to support.
- The bridge is hermetic in tests: `scripts/e2e-seed.sh` mints tokens via the internal endpoint for Playwright.

### Negative / Trade-offs

- The `LOGISTICOS_INTERNAL_SECRET` is a secret with real blast radius. Rotation is documented in `docs/runbooks/auth-bridge-rotation.md`.
- Portals must proxy `/api/auth/refresh` — each portal has a near-identical route handler. Acceptable duplication for domain isolation.
- Firebase tokens are still short-lived; the `__session` cookie must be refreshed by the Firebase SDK. If the Firebase session expires but the LoS refresh is valid, the user stays signed in for LoS APIs but loses Firebase-gated features (IAM, Cloud Messaging). Acceptable — Firebase is only the identity provider.

## Test coverage

- Unit: `services/identity/src/auth/firebase_exchange.rs` — happy path, bad signature, unknown UID, missing email verification.
- Integration: landing `/api/auth/refresh`, `/api/tenants/finalize`; portal `/api/token` on all four portals.
- End-to-end: `e2e/specs/{portal-token,refresh,setup,csrf}.spec.ts` — seeded cookies, no Firebase popup.
- Smoke (post-deploy): `scripts/auth-bridge-smoke.sh`.

## Related

- ADR-0008 — Multi-Tenancy RLS Strategy (consumer of the JWT claims)
- ADR-0009 — Multi-Product Platform Gateway Topology (shared bridge serves all products)
- Runbook: `docs/runbooks/auth-bridge-rotation.md`
