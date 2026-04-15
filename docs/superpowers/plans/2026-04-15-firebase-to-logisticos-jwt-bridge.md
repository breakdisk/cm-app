# Firebase → LogisticOS JWT Bridge Implementation Plan

> **Spec:** [docs/superpowers/specs/2026-04-15-firebase-to-logisticos-jwt-bridge-design.md](../specs/2026-04-15-firebase-to-logisticos-jwt-bridge-design.md)
>
> **For agentic workers:** Use superpowers:executing-plans to work through these tasks in order. Steps use checkbox (`- [ ]`) syntax. Do not skip tasks — later work depends on earlier artifacts.

**Goal:** Bridge Firebase Auth (web sign-in) to the LogisticOS JWT required by backend RLS per [ADR-0008](../../adr/0008-multi-tenancy-rls-strategy.md). Portal API calls stop returning "Not authenticated" because every authenticated browser request now carries a valid, tenant-scoped LogisticOS access token.

**Non-goals:** Replacing Firebase with custom OIDC, mobile auth changes, Edge middleware fix (tracked in `project_firebase_auth_plan.md`).

---

## File Map

### `services/identity` (Rust)

| File | Action | Purpose |
|------|--------|---------|
| `migrations/0008_auth_identities.sql` | Create | `auth_identities` join table + RLS policy |
| `migrations/0009_tenant_status_draft.sql` | Create | Add `tenants.status` enum (`draft`/`active`/`suspended`) |
| `src/domain/entities/auth_identity.rs` | Create | Domain entity |
| `src/domain/repositories/auth_identity_repository.rs` | Create | Trait |
| `src/infrastructure/repositories/auth_identity_pg.rs` | Create | Postgres impl |
| `src/application/commands.rs` | Modify | Add `ExchangeFirebaseCommand`, `FinalizeTenantCommand` |
| `src/application/services/auth_service.rs` | Modify | Add `exchange_firebase()` + `finalize_tenant()` |
| `src/api/http/auth.rs` | Modify | Add `exchange_firebase` handler |
| `src/api/http/tenants.rs` | Modify | Add `finalize_self` handler |
| `src/api/http/mod.rs` | Modify | Wire new routes + `require_internal_secret` guard |
| `src/api/http/middleware/internal_secret.rs` | Create | Header-based auth for internal endpoints |
| `Cargo.toml` | Modify | No new deps needed (uses existing `sqlx`, `axum`, `serde`) |

### `libs/auth` (Rust)

| File | Action | Purpose |
|------|--------|---------|
| `src/middleware.rs` | Modify | Add `require_client_header` middleware (CSRF defense) |
| `src/claims.rs` | Modify | Add `onboarding_required: bool` flag to `Claims` |

### `apps/landing` (Next.js)

| File | Action | Purpose |
|------|--------|---------|
| `src/lib/identity/client.ts` | Create | Fetch wrapper for identity exchange endpoint |
| `src/app/api/auth/session/route.ts` | Modify | Call exchange → set 3 cookies (`__session`, `los_at`, `los_rt`) |
| `src/app/api/auth/refresh/route.ts` | Create | Rotate `los_at` from `los_rt` |
| `src/app/api/auth/signout/route.ts` | Modify | Clear all 3 cookies + revoke refresh JTI |
| `.env.example` | Modify | Add `IDENTITY_URL`, `IDENTITY_INTERNAL_SECRET` |

### Each Portal (`apps/{merchant,admin,partner,customer}-portal`)

| File | Action | Purpose |
|------|--------|---------|
| `src/app/api/token/route.ts` | Create | Expose `los_at` cookie to client JS (with cache headers) |
| `src/lib/auth-fetch.ts` | Create | Single wrapper for authenticated API calls |
| `src/app/(auth)/login/page.tsx` | Delete | Dormant — centralized on landing |
| `src/app/(dashboard)/layout.tsx` | Modify | Replace localStorage redirect guard with `/api/token` call |
| `src/app/**/*.tsx` (call sites) | Modify | Replace `localStorage.getItem("access_token")` with `authFetch` |

### `apps/landing/src/app/setup` (merchant onboarding)

| File | Action | Purpose |
|------|--------|---------|
| `src/app/setup/page.tsx` | Create | Draft-tenant completion form (business name, currency, region) |
| `src/app/api/setup/finalize/route.ts` | Create | Proxy to `POST /v1/tenants/me/finalize` |

---

## Task 1: Identity schema — `auth_identities` + `tenants.status`

**Files:**
- Create: `services/identity/migrations/0008_auth_identities.sql`
- Create: `services/identity/migrations/0009_tenant_status_draft.sql`

- [ ] **Step 1: Write `0008_auth_identities.sql`**

  ```sql
  CREATE TABLE auth_identities (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id           UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider          TEXT NOT NULL CHECK (provider IN ('firebase', 'saml', 'google_workspace')),
    provider_subject  TEXT NOT NULL,
    email_at_link     TEXT NOT NULL,
    linked_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_subject)
  );
  CREATE INDEX idx_auth_identities_user ON auth_identities (user_id);

  ALTER TABLE auth_identities ENABLE ROW LEVEL SECURITY;
  CREATE POLICY tenant_isolation ON auth_identities
    USING (user_id IN (SELECT id FROM users WHERE tenant_id = current_setting('app.tenant_id', true)::uuid));
  ```

- [ ] **Step 2: Write `0009_tenant_status_draft.sql`**

  ```sql
  DO $$ BEGIN
    CREATE TYPE tenant_status AS ENUM ('draft', 'active', 'suspended');
  EXCEPTION WHEN duplicate_object THEN null; END $$;

  ALTER TABLE tenants
    ADD COLUMN IF NOT EXISTS status tenant_status NOT NULL DEFAULT 'active';

  -- Existing tenants stay active; only new draft-tenants start as 'draft'
  ```

- [ ] **Step 3: Run migrations locally**

  ```bash
  cd services/identity && sqlx migrate run --database-url $DATABASE_URL
  ```

  Verify with `psql $DATABASE_URL -c "\d auth_identities"`.

---

## Task 2: `auth_identities` domain + repository

**Files:**
- Create: `services/identity/src/domain/entities/auth_identity.rs`
- Create: `services/identity/src/domain/repositories/auth_identity_repository.rs`
- Create: `services/identity/src/infrastructure/repositories/auth_identity_pg.rs`
- Modify: `services/identity/src/domain/entities/mod.rs`, `domain/repositories/mod.rs`, `infrastructure/repositories/mod.rs`

- [ ] **Step 1: Entity**

  ```rust
  #[derive(Debug, Clone)]
  pub struct AuthIdentity {
      pub id:               Uuid,
      pub user_id:          Uuid,
      pub provider:         String,
      pub provider_subject: String,
      pub email_at_link:    String,
      pub linked_at:        DateTime<Utc>,
  }
  ```

- [ ] **Step 2: Repository trait**

  ```rust
  #[async_trait]
  pub trait AuthIdentityRepository: Send + Sync {
      async fn find_by_provider_subject(&self, provider: &str, subject: &str) -> Result<Option<AuthIdentity>, AppError>;
      async fn insert(&self, identity: AuthIdentity) -> Result<(), AppError>;
  }
  ```

- [ ] **Step 3: Postgres implementation** — SQLx queries mirroring `users_pg.rs` patterns (use `query_as!` with compile-time checks).

- [ ] **Step 4: Wire into `AppState`** in `services/identity/src/api/http/mod.rs`.

---

## Task 3: `exchange_firebase` command + service method

**Files:**
- Modify: `services/identity/src/application/commands.rs`
- Modify: `services/identity/src/application/services/auth_service.rs`

- [ ] **Step 1: Add command**

  ```rust
  #[derive(Debug, Deserialize)]
  pub struct ExchangeFirebaseCommand {
      pub firebase_uid:   String,
      pub email:          String,
      pub email_verified: bool,
      pub role:           String,      // "merchant" | "admin" | "partner" | "customer"
      pub display_name:   Option<String>,
      pub partner_slug:   Option<String>,  // for customer white-label auto-link
      pub partner_sig:    Option<String>,  // HMAC over (partner_slug + firebase_uid)
  }
  ```

- [ ] **Step 2: Service method skeleton**

  ```rust
  pub async fn exchange_firebase(&self, cmd: ExchangeFirebaseCommand) -> Result<TokenResponse, AppError> {
      // 1. Lookup auth_identities by (firebase, uid)
      if let Some(identity) = self.auth_identity_repo.find_by_provider_subject("firebase", &cmd.firebase_uid).await? {
          return self.mint_for_existing_user(identity.user_id).await;
      }
      // 2. Not found — lazy onboarding branch
      match cmd.role.as_str() {
          "merchant" => self.provision_draft_merchant(&cmd).await,
          "customer" => self.provision_partner_customer(&cmd).await,
          "admin" | "partner" => Err(AppError::forbidden("tenant_not_provisioned")),
          _ => Err(AppError::bad_request("invalid_role")),
      }
  }
  ```

- [ ] **Step 3: `provision_draft_merchant`**
  - Create tenant with `status = 'draft'`, `slug = format!("draft-{}", &firebase_uid[..8])`, `subscription_tier = 'starter'`
  - Create user with `OWNER` role
  - Insert `auth_identities` row
  - Mint `Claims` with `permissions = ["tenants:update-self", "billing:setup"]` and `onboarding_required = true`
  - Publish Kafka `user.provisioned` with `{ status: "draft" }`

- [ ] **Step 4: `provision_partner_customer`**
  - Verify `partner_sig` = HMAC-SHA256(`partner_slug + firebase_uid`, `PARTNER_LINK_SECRET`). Reject if missing or invalid.
  - Lookup tenant by `partner_slug` — reject if not active.
  - Create user in that tenant with `CUSTOMER` role.
  - Insert `auth_identities` row.
  - Mint standard customer claims.

- [ ] **Step 5: `mint_for_existing_user`** — load user + tenant, if `tenant.status = 'draft'` and user is owner, mint draft-scoped claims; else mint full claims.

- [ ] **Step 6: Unit tests** for each branch: existing user / draft merchant / valid partner / invalid partner sig / admin-role rejected.

---

## Task 4: Internal-secret middleware + `POST /v1/auth/exchange-firebase`

**Files:**
- Create: `services/identity/src/api/http/middleware/internal_secret.rs`
- Modify: `services/identity/src/api/http/auth.rs`
- Modify: `services/identity/src/api/http/mod.rs`

- [ ] **Step 1: Middleware**

  ```rust
  pub async fn require_internal_secret(req: Request, next: Next) -> Result<Response, AppError> {
      let header = req.headers().get("x-internal-secret").and_then(|v| v.to_str().ok());
      let expected = std::env::var("IDENTITY_INTERNAL_SECRET").map_err(|_| AppError::internal("missing secret config"))?;
      if header != Some(expected.as_str()) {
          return Err(AppError::forbidden("internal_only"));
      }
      Ok(next.run(req).await)
  }
  ```

  Constant-time compare via `subtle::ConstantTimeEq` to avoid timing side-channel.

- [ ] **Step 2: Handler**

  ```rust
  pub async fn exchange_firebase(
      State(state): State<Arc<AppState>>,
      Json(cmd): Json<ExchangeFirebaseCommand>,
  ) -> Result<Json<serde_json::Value>, AppError> {
      let result = state.auth_service.exchange_firebase(cmd).await?;
      Ok(Json(serde_json::json!({ "data": result })))
  }
  ```

- [ ] **Step 3: Route wiring** — add under an `/internal` router scope gated by `require_internal_secret`:

  ```rust
  let internal = Router::new()
      .route("/auth/exchange-firebase", post(auth::exchange_firebase))
      .layer(middleware::from_fn(require_internal_secret));
  app = app.nest("/v1/internal", internal);
  ```

  Path becomes `POST /v1/internal/auth/exchange-firebase`. **Not** routed through the public API gateway — restrict at gateway config too.

- [ ] **Step 4: Integration test** — happy path + missing-header + wrong-secret.

---

## Task 5: `finalize_tenant` endpoint (onboarding exit)

**Files:**
- Modify: `services/identity/src/application/commands.rs`
- Modify: `services/identity/src/application/services/tenant_service.rs`
- Modify: `services/identity/src/api/http/tenants.rs`

- [ ] **Step 1: Command** — `FinalizeTenantCommand { business_name, currency, country, timezone }`.

- [ ] **Step 2: Service method** — only allowed when caller's `permissions` include `tenants:update-self` AND `tenant.status = 'draft'`. Updates columns, flips `status → 'active'`, assigns full merchant permission set to the user.

- [ ] **Step 3: Handler** — `POST /v1/tenants/me/finalize` (public, requires standard JWT auth).

- [ ] **Step 4: Next refresh picks up full claims** — finalize returns updated access token in response body for immediate use.

---

## Task 6: `libs/auth` — CSRF header + onboarding flag

**Files:**
- Modify: `libs/auth/src/middleware.rs`
- Modify: `libs/auth/src/claims.rs`

- [ ] **Step 1: Add `onboarding_required: bool` to `Claims`** (default `false`, skip if missing on deserialize for backward-compat with existing tokens in flight).

- [ ] **Step 2: Add `require_client_header` middleware**

  ```rust
  pub async fn require_client_header(req: Request, next: Next) -> Result<Response, AppError> {
      if matches!(req.method(), &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE) {
          let header = req.headers().get("x-logisticos-client").and_then(|v| v.to_str().ok());
          if !matches!(header, Some("web") | Some("mobile") | Some("service")) {
              return Err(AppError::forbidden("missing_client_header"));
          }
      }
      Ok(next.run(req).await)
  }
  ```

- [ ] **Step 3: Every service applies the layer** — add `.layer(middleware::from_fn(require_client_header))` in each service's `main.rs` right after `require_auth`.

- [ ] **Step 4: Unit tests** for header presence / absence / GET-bypass.

---

## Task 7: Landing — call exchange + set 3 cookies

**Files:**
- Create: `apps/landing/src/lib/identity/client.ts`
- Modify: `apps/landing/src/app/api/auth/session/route.ts`
- Create: `apps/landing/src/app/api/auth/refresh/route.ts`
- Modify: `apps/landing/src/app/api/auth/signout/route.ts`
- Modify: `apps/landing/.env.example`

- [ ] **Step 1: `identity/client.ts`**

  ```ts
  export async function exchangeFirebase(input: {
    firebase_uid: string; email: string; email_verified: boolean;
    role: string; display_name?: string; partner_slug?: string; partner_sig?: string;
  }) {
    const res = await fetch(`${process.env.IDENTITY_URL}/v1/internal/auth/exchange-firebase`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Internal-Secret": process.env.IDENTITY_INTERNAL_SECRET!,
        "X-LogisticOS-Client": "service",
      },
      body: JSON.stringify(input),
      cache: "no-store",
    });
    if (!res.ok) throw new Error(`exchange failed: ${res.status}`);
    return (await res.json()).data as TokenResponse;
  }
  ```

- [ ] **Step 2: Update `/api/auth/session/route.ts`**

  - After `verifySession(idToken)` succeeds → call `exchangeFirebase(...)` with `firebase_uid = session.uid`.
  - Set three cookies (shared config: `httpOnly: true, secure: prod, sameSite: "lax", domain: ".cargomarket.net", path: "/"`):
    - `__session` = Firebase ID token, 7d
    - `los_at` = access token, `expires_in` seconds
    - `los_rt` = refresh token, 30d
  - Response JSON includes `{ onboarding_required: boolean, redirect: "/setup" | "/merchant" | ... }`.

- [ ] **Step 3: Create `/api/auth/refresh/route.ts`**

  - Read `los_rt` cookie → call identity `POST /v1/auth/refresh` (already exists) → rotate `los_at`. 204 on success, 401 if refresh invalid.

- [ ] **Step 4: Update `/api/auth/signout/route.ts`**

  - Clear all 3 cookies (`res.cookies.delete(...)` × 3).
  - Call identity revoke if we have a refresh JTI.

- [ ] **Step 5: Env docs**

  ```env
  IDENTITY_URL=http://logisticos-identity:8001
  IDENTITY_INTERNAL_SECRET=<generated>
  ```

---

## Task 8: Portal `/api/token` + `auth-fetch`

**Files (per portal, all four):**
- Create: `src/app/api/token/route.ts`
- Create: `src/lib/auth-fetch.ts`

- [ ] **Step 1: `/api/token/route.ts`**

  ```ts
  import { NextRequest, NextResponse } from "next/server";
  export const runtime = "nodejs";
  export async function GET(req: NextRequest) {
    const token = req.cookies.get("los_at")?.value ?? null;
    const res = NextResponse.json({ token });
    res.headers.set("Cache-Control", "private, s-maxage=0, max-age=60, must-revalidate");
    res.headers.set("Vary", "Cookie");
    return res;
  }
  ```

- [ ] **Step 2: `lib/auth-fetch.ts`**

  ```ts
  let cached: { token: string; expiresAt: number } | null = null;

  async function getToken(): Promise<string | null> {
    if (cached && cached.expiresAt > Date.now()) return cached.token;
    const r = await fetch("/<role>/api/token", { credentials: "include" });
    const { token } = await r.json();
    if (token) cached = { token, expiresAt: Date.now() + 55_000 };
    return token;
  }

  export async function authFetch(url: string, init: RequestInit = {}): Promise<Response> {
    let token = await getToken();
    const doFetch = () => fetch(url, {
      ...init,
      credentials: "include",
      headers: {
        ...init.headers,
        "X-LogisticOS-Client": "web",
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
    });
    let res = await doFetch();
    if (res.status === 401) {
      cached = null;
      const refresh = await fetch("/api/auth/refresh", { method: "POST", credentials: "include" });
      if (refresh.ok) {
        token = await getToken();
        res = await doFetch();
      }
    }
    return res;
  }
  ```

  Replace `<role>` per portal (`/merchant`, `/admin`, `/partner`, `/customer`).

- [ ] **Step 3: Unit test** — mock a 401 cycle and assert one retry.

---

## Task 9: Migrate portal call sites

**Per portal:**

- [ ] **Step 1: Grep** — `grep -rn "localStorage.getItem(\"access_token\")" apps/<portal>/src`.
- [ ] **Step 2: Replace each with `authFetch`** from `@/lib/auth-fetch`.
- [ ] **Step 3: Remove `localStorage.setItem("access_token", ...)` lines** — these were writing tokens that no one reads anymore.
- [ ] **Step 4: Update `(dashboard)/layout.tsx` redirect guard** — instead of checking localStorage, call `/api/token` once; if `null`, redirect to `<LANDING_URL>/login?role=<role>`.

**Checklist of known call sites (verify nothing new has appeared):**
- [ ] `apps/merchant-portal/src/app/(dashboard)/shipments/page.tsx`
- [ ] `apps/merchant-portal/src/app/(dashboard)/layout.tsx`
- [ ] `apps/merchant-portal/src/app/(auth)/login/page.tsx` → DELETE
- [ ] `apps/admin-portal/src/app/(dashboard)/**/*.tsx`
- [ ] `apps/partner-portal/src/app/(dashboard)/**/*.tsx`
- [ ] `apps/customer-portal/src/app/(dashboard)/**/*.tsx`

---

## Task 10: `/setup` onboarding page

**Files:**
- Create: `apps/landing/src/app/setup/page.tsx`
- Create: `apps/landing/src/app/api/setup/finalize/route.ts`

- [ ] **Step 1: Form page** — dark glass card per design system, fields: business name, country (PH/AE/SG dropdown), currency (derived), timezone (derived). Submit → POST to `/api/setup/finalize`.

- [ ] **Step 2: Proxy route** — reads `los_at` cookie, forwards to identity `POST /v1/tenants/me/finalize` with `X-LogisticOS-Client: web`. On success, updates `los_at` cookie from response.

- [ ] **Step 3: Redirect** — after finalize, 302 to `/merchant`.

- [ ] **Step 4: Access control** — middleware must allow `/setup` for draft-tenant users but block everything else.

---

## Task 11: Remove dormant login routes

- [ ] `apps/merchant-portal/src/app/(auth)/login/page.tsx` — delete
- [ ] `apps/admin-portal/src/app/(auth)/login/page.tsx` — delete
- [ ] `apps/partner-portal/src/app/(auth)/login/page.tsx` — delete
- [ ] `apps/customer-portal/src/app/(auth)/login/page.tsx` — delete
- [ ] Remove any `(auth)` layout files left empty

**Verify** — `grep -rn "\"access_token\"" apps/` returns zero hits in portal code.

---

## Task 12: Integration test — end-to-end

Write a Playwright test in `apps/landing/tests/auth-bridge.spec.ts`:

- [ ] **Step 1: Mock Firebase** — use Firebase Auth emulator OR stub `verifySession()` in test env.
- [ ] **Step 2: Test: new merchant flow**
  - Sign in with Firebase (emulator) → expect redirect to `/setup`
  - Fill form → expect redirect to `/merchant`
  - Navigate to `/merchant/shipments` → create shipment → expect 201 (no "Not authenticated")
- [ ] **Step 3: Test: returning merchant** — skip `/setup`, land directly on dashboard.
- [ ] **Step 4: Test: admin without invite** — expect 403 page with "contact your administrator" copy.
- [ ] **Step 5: Test: refresh rotation** — expire `los_at`, issue request, expect retry with new token.

---

## Task 13: Deployment

- [ ] **Step 1: Generate `IDENTITY_INTERNAL_SECRET`** — 64-byte random, store in Dokploy env for both landing and identity services.
- [ ] **Step 2: Ensure identity's `/v1/internal/*` path is NOT proxied by Traefik/gateway** — add deny rule.
- [ ] **Step 3: Rebuild + redeploy** in order: `identity` → `landing` → `merchant-portal` → `admin-portal` → `partner-portal` → `customer-portal`.
- [ ] **Step 4: Smoke test** on VPS — sign in with Firebase Google on prod, create a test shipment, confirm 201.
- [ ] **Step 5: Rollback plan** — previous images stay in GHCR; revert the 6 Dokploy services if smoke test fails.

---

## Task 14: Docs + memory

- [ ] Update `memory/project_firebase_auth_plan.md` — note that the backend bridge is now in place; middleware Edge fix is still pending.
- [ ] Create `memory/project_firebase_logisticos_bridge.md` — record the deployed internal endpoint, secret rotation policy, onboarding state machine.
- [ ] Update `MEMORY.md` index with the new entry.
- [ ] Write runbook `docs/runbooks/auth-bridge-rotation.md` — how to rotate `IDENTITY_INTERNAL_SECRET` without downtime (identity accepts both old+new for a grace window).

---

## Rollout order summary

1. Tasks 1–6 (backend) — merge & deploy identity first. Public API unchanged; only new internal routes added.
2. Tasks 7–8 (landing + portal helpers) — deploy landing + portals.
3. Tasks 9–11 (call-site migration + cleanup) — landed after helpers so nothing breaks mid-migration.
4. Tasks 12–14 (test + deploy + docs).

Each task self-contained — can be shipped as a separate PR if desired.
