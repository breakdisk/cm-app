# Single-Domain Multi-Portal with Firebase Auth

**Date:** 2026-04-08
**Status:** Approved
**Scope:** Landing page routing, Firebase Auth integration, portal path-based proxying

---

## Overview

All LogisticOS web surfaces are served from a single domain `os.cargomarket.net`. The landing app acts as the public entry point and reverse proxy to four internal portal apps. Authentication is centralized via Firebase Auth with role-based custom claims. Shipment tracking is publicly accessible without login.

---

## URL Structure

| Path | Served By | Auth Required |
|------|-----------|---------------|
| `os.cargomarket.net/` | Landing app | No |
| `os.cargomarket.net/track` | Landing app (`/track` route) | No |
| `os.cargomarket.net/login` | Landing app (`/login` route) | No |
| `os.cargomarket.net/merchant/*` | Merchant Portal (proxied) | Yes → `/login?role=merchant` |
| `os.cargomarket.net/admin/*` | Admin Portal (proxied) | Yes → `/login?role=admin` |
| `os.cargomarket.net/partner/*` | Partner Portal (proxied) | Yes → `/login?role=partner` |
| `os.cargomarket.net/customer/*` | Customer Portal (proxied) | Yes → `/login?role=customer` |

---

## Architecture

### Deployment (Dokploy)

Only the landing app has a public Dokploy domain. All other portals are internal-only, reachable by Docker container name on the shared network.

| App | Container Name | Port | Public Domain |
|-----|---------------|------|---------------|
| landing | `logisticos-landing` | 3004 | `os.cargomarket.net` |
| merchant-portal | `logisticos-merchant` | 3000 | internal only |
| admin-portal | `logisticos-admin` | 3001 | internal only |
| partner-portal | `logisticos-partner` | 3003 | internal only |
| customer-portal | `logisticos-customer` | 3002 | internal only |

### Proxying (Next.js Rewrites)

The landing app's `next.config.js` proxies portal paths to internal containers:

```js
async rewrites() {
  return [
    { source: '/merchant/:path*', destination: 'http://logisticos-merchant:3000/merchant/:path*' },
    { source: '/admin/:path*',    destination: 'http://logisticos-admin:3001/admin/:path*' },
    { source: '/partner/:path*',  destination: 'http://logisticos-partner:3003/partner/:path*' },
    { source: '/customer/:path*', destination: 'http://logisticos-customer:3002/customer/:path*' },
  ]
}
```

Each portal uses `basePath` in its own `next.config.js` so Next.js internal routing (asset paths, `<Link>`) works correctly under the prefix:

```js
// merchant-portal/next.config.js
basePath: '/merchant'
```

---

## Firebase Auth

### Setup

- One Firebase project for all of LogisticOS
- Providers enabled: Google, Facebook, Email magic link
- Firebase Admin SDK used server-side in landing middleware and each portal middleware
- Environment variable `FIREBASE_SERVICE_ACCOUNT_JSON` holds the Admin SDK credentials (base64-encoded JSON)
- Client-side config via `NEXT_PUBLIC_FIREBASE_*` env vars

### Custom Claims

When a user is created or invited, Firebase Admin SDK sets a custom claim:

```json
{ "role": "merchant" }
```

Valid roles: `merchant`, `admin`, `partner`, `customer`

### Auth Flow

1. User visits `os.cargomarket.net` → clicks "Sign In" in Navbar
2. Redirected to `/login` — shows 4 role cards (Merchant, Admin, Partner, Customer)
3. User selects role → `?role=merchant` appended to URL
4. Chosen role card expands to show: Google, Facebook, Magic Link buttons
5. Firebase client SDK handles the OAuth / magic link popup
6. On success: client calls `POST /api/auth/session` with the Firebase ID token
7. Server verifies token with Admin SDK, checks `role` custom claim matches selected role
8. Sets `__session` httpOnly cookie (7-day expiry) with the ID token
9. Redirects user to `os.cargomarket.net/<role>` (e.g., `/merchant`)

### Session Verification

Landing `middleware.ts` intercepts requests to `/merchant/*`, `/admin/*`, `/partner/*`, `/customer/*`:
- Reads `__session` cookie
- Verifies with Firebase Admin SDK
- Checks custom claim role matches the path prefix
- On failure: redirects to `/login?role=<role>`

Each portal also has its own `middleware.ts` doing the same verification as a second layer of defense (in case portal is ever accessed directly on internal network).

### Sign Out

`POST /api/auth/signout` — clears `__session` cookie, redirects to `/`

---

## Landing App Changes

### New Files

| File | Purpose |
|------|---------|
| `app/login/page.tsx` | Role selector + Firebase auth UI |
| `app/track/page.tsx` | Public AWB search + shipment timeline |
| `app/api/auth/session/route.ts` | Sets `__session` httpOnly cookie after Firebase auth |
| `app/api/auth/signout/route.ts` | Clears cookie, redirects to `/` |
| `lib/firebase/client.ts` | Firebase client SDK initialization |
| `lib/firebase/admin.ts` | Firebase Admin SDK initialization |
| `middleware.ts` | Protects portal paths, redirects unauthenticated users |

### Modified Files

| File | Change |
|------|--------|
| `next.config.js` | Add rewrites for 4 portal paths; mark `firebase-admin` as external server package |
| `components/Navbar.tsx` | "Sign in" → `/login`; add "Track" link → `/track` |

### Login Page UI

- Dark glassmorphism card, matches existing landing design language
- 4 role cards in a 2×2 grid: Merchant (cyan), Admin (purple), Partner (amber), Customer (green)
- Selecting a role expands it to show auth provider buttons
- `?role=merchant` pre-selects the card on load (for redirect-back flow)
- "Track a shipment" link at bottom for public tracking

### Track Page UI

- Single AWB input with search button
- Calls `GET /api/delivery-experience/track/:awb` (proxied to delivery-experience service)
- Shows shipment timeline: status steps, estimated delivery, last location
- No login required
- "Create an account" CTA at bottom

---

## Portal Changes

### All Portals (merchant, admin, partner, customer)

| Change | Details |
|--------|---------|
| Remove `app/(auth)/login/` | Auth is now centralized on landing |
| Remove `app/(auth)/register/` | Registration handled via Firebase on landing |
| Add `middleware.ts` | Verifies `__session` cookie, redirects to `os.cargomarket.net/login?role=<role>` |
| Add `lib/firebase/admin.ts` | Token verification utility |
| Update `next.config.js` | Add `basePath: '/<role>'` |
| Add Firebase env vars | `FIREBASE_SERVICE_ACCOUNT_JSON`, `NEXT_PUBLIC_FIREBASE_*` |

### basePath Values

| Portal | basePath |
|--------|----------|
| merchant-portal | `/merchant` |
| admin-portal | `/admin` |
| partner-portal | `/partner` |
| customer-portal | `/customer` |

---

## Environment Variables

### Landing App

```env
# Firebase Client (public)
NEXT_PUBLIC_FIREBASE_API_KEY=
NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN=
NEXT_PUBLIC_FIREBASE_PROJECT_ID=
NEXT_PUBLIC_FIREBASE_APP_ID=

# Firebase Admin (server only)
FIREBASE_SERVICE_ACCOUNT_JSON=   # base64-encoded service account JSON

# Portal internal URLs (server only, used by rewrites)
MERCHANT_PORTAL_URL=http://logisticos-merchant:3000
ADMIN_PORTAL_URL=http://logisticos-admin:3001
PARTNER_PORTAL_URL=http://logisticos-partner:3003
CUSTOMER_PORTAL_URL=http://logisticos-customer:3002
```

### Each Portal

```env
NEXT_PUBLIC_FIREBASE_API_KEY=
NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN=
NEXT_PUBLIC_FIREBASE_PROJECT_ID=
NEXT_PUBLIC_FIREBASE_APP_ID=
FIREBASE_SERVICE_ACCOUNT_JSON=
```

---

## New Dependencies

### Landing App

```
firebase          # client SDK
firebase-admin    # server Admin SDK
```

### Each Portal

```
firebase-admin    # server Admin SDK (already has firebase client if needed)
```

---

## Error Handling

| Scenario | Behavior |
|----------|---------|
| Token expired | Middleware redirects to `/login?role=<role>&expired=1` — login page shows "Session expired" message |
| Wrong role | Middleware redirects to `/login?role=<role>&error=unauthorized` |
| Firebase unavailable | Middleware fails open (allows through) to avoid locking out users during outage — logged as error |
| Magic link clicked on different device | Firebase handles natively — shows "confirm email" prompt |

---

## Out of Scope

- Role assignment UI (admin manually sets custom claims via Firebase Console for now)
- Invite flow (future feature)
- MFA (future feature)
- LogisticOS identity service (`services/identity/`) — this design is a stopgap; custom OIDC server replaces Firebase when ready
