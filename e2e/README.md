# LogisticOS E2E Tests

Playwright suite for the Firebase → LogisticOS JWT bridge and portal auth flows.

## What's covered

| Spec | Scope |
|------|-------|
| `portal-token.spec.ts` | `/api/token` route on all four portals — 401 without cookie, 200 with seeded cookie |
| `refresh.spec.ts`      | Landing `/api/auth/refresh` + portal proxies — rotation, unauthenticated rejection |
| `setup.spec.ts`        | Draft-tenant `/setup` page + `/api/tenants/finalize` route |
| `csrf.spec.ts`         | `X-LogisticOS-Client` header handling |

The interactive Firebase sign-in flow (popup → `/api/auth/session` → cookies) is exercised by `setup.spec.ts` indirectly: we seed a draft LoS session directly rather than automating the Google popup. Full Firebase UI coverage belongs in a dedicated suite with a real Firebase Auth emulator — tracked as follow-up.

## Prerequisites

1. **Running stack** — either local dev servers or a staging deployment reachable from the test host.
2. **Seed tokens** — scripts must have exported:
   - `TEST_LOS_ACCESS_TOKEN` / `TEST_LOS_REFRESH_TOKEN` (active tenant)
   - `TEST_DRAFT_ACCESS_TOKEN` / `TEST_DRAFT_REFRESH_TOKEN` (draft tenant for setup spec)
3. **URL env vars** (optional; defaults target localhost):
   - `LANDING_URL`, `MERCHANT_URL`, `ADMIN_URL`, `PARTNER_URL`, `CUSTOMER_URL`

## Running locally

```bash
cd e2e
npm install
npx playwright install chromium

# Seed tokens against a running identity service:
#   POST /v1/internal/auth/exchange-firebase with a test fixture firebase_uid
#   plus a second call for a fresh draft tenant.
# (See scripts/e2e-seed.sh — to be added in Task 13.)
export TEST_LOS_ACCESS_TOKEN=...
export TEST_LOS_REFRESH_TOKEN=...
export TEST_DRAFT_ACCESS_TOKEN=...
export TEST_DRAFT_REFRESH_TOKEN=...

npm test
```

## Running in CI

GitHub Actions job example:

```yaml
- name: Seed auth fixtures
  run: ./scripts/e2e-seed.sh   # exports TEST_* vars to $GITHUB_ENV

- name: Playwright
  working-directory: e2e
  run: |
    npm ci
    npx playwright install --with-deps chromium
    npm test
  env:
    LANDING_URL:  ${{ vars.STAGING_LANDING_URL }}
    MERCHANT_URL: ${{ vars.STAGING_MERCHANT_URL }}
    ADMIN_URL:    ${{ vars.STAGING_ADMIN_URL }}
    PARTNER_URL:  ${{ vars.STAGING_PARTNER_URL }}
    CUSTOMER_URL: ${{ vars.STAGING_CUSTOMER_URL }}
```

## Design notes

- We **do not** automate the Firebase Auth popup. The identity service exposes `/v1/internal/auth/exchange-firebase` guarded by `X-Internal-Secret` — the seed script uses this to mint LoS tokens directly for a deterministic test user. This keeps E2E hermetic and avoids brittle popup scraping.
- Cookies are set via `context.addCookies()` rather than real Next middleware to let each spec target a single code path without the whole sign-in dance.
- `X-LogisticOS-Client: web` is injected globally via `playwright.config.ts` — matches what real browsers send from our portals.
