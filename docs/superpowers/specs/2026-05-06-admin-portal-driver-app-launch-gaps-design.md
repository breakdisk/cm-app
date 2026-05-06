# Design: Admin Portal ↔ Driver App — Production Launch Gaps

**Date:** 2026-05-06  
**Author:** Principal Software Architect  
**Status:** Approved  
**Scope:** Admin Portal (Next.js), Driver App (Android), FCM deployment

---

## 1. Problem Statement

Three gaps block production launch between the Admin Portal and the driver app:

1. **Live dispatch map** — `LiveDispatchMap` uses a deterministic Manila simulation; no WebSocket connection to real driver GPS data exists in the Admin Portal.
2. **Driver roster page** — calls `useRosterEvents()` directly per-page, creating an isolated WebSocket connection that cannot be shared; the dispatch page has no real-time driver data at all.
3. **FCM env vars missing on VPS** — `FCM_PROJECT_ID` and `FCM_SERVICE_ACCOUNT_JSON` are not set in the Dokploy compose env file, so drivers only receive task assignment notifications when the app is foregrounded.

**Deferred (not in scope):**
- `requires_photo / requires_signature / requires_otp` — current heuristic (`photo=always`, `signature=delivery`, `otp=delivery+COD`) is correct for launch. Per-shipment policy propagation deferred post-launch.

---

## 2. Confirmed Working (No Changes Needed)

| Area | Status |
|------|--------|
| POD viewer in `ShipmentDetailPanel` | Wired; calls `GET /v1/pods?shipment_id=` |
| Driver assignment UI in driver app | FCM + StateFlow bus + accept/reject screen |
| Admin dispatch actions (force status, send instruction, cancel tasks) | Wired |
| `useRosterEvents.ts` WebSocket hook | Exists; auth, reconnect, ping all implemented |
| Driver app offline sync | SQLite + OutboundSyncWorker |

---

## 3. Approach: Shared Context + Single WebSocket (Approach A)

Lift the existing `useRosterEvents()` hook from page-level into a shared `DriverRosterProvider` mounted once in the authenticated dashboard layout. Both the `/drivers` and `/dispatch` pages consume from context.

### Before

```
(dashboard)/drivers/page.tsx  →  useRosterEvents()  →  /ws/locations  [WebSocket #1]
(dashboard)/dispatch/page.tsx →  LiveDispatchMap  →  SimulationMap   [no real data]
```

### After

```
(dashboard)/layout.tsx
  └── DriverRosterProvider
        └── useRosterEvents() internally           [single WebSocket]
              │
              ├── /drivers/page.tsx  → useDriverRoster()
              └── /dispatch/page.tsx → useDriverRoster() → LiveDispatchMap (Mapbox)
```

---

## 4. Data Model

### `DriverPin` (shared type)

```typescript
type DriverPin = {
  driver_id: string
  name: string
  vehicle_type: string
  plate: string
  status: 'available' | 'en_route' | 'delivering' | 'returning' | 'on_break' | 'offline'
  lat: number | null
  lng: number | null
  heading: number | null
  task_count: number
  cod_collected_cents: number
}
```

Mirrors the `RosterEvent` payloads from `driver-ops /ws/locations`.

### `RosterState`

```typescript
type RosterState = {
  drivers: Record<string, DriverPin>  // keyed by driver_id
  connected: boolean
}
```

### Reducer Actions

| Action | Source | Effect |
|--------|--------|--------|
| `ROSTER_INIT` | `GET /v1/drivers` initial fetch | Populates full driver map |
| `LOCATION_UPDATED` | WS `location_updated` event | Patches `lat`, `lng`, `heading` |
| `STATUS_CHANGED` | WS `status_changed` event | Patches `status`, `task_count` |
| `CONNECTED` | WS open | Sets `connected: true` |
| `DISCONNECTED` | WS close/error | Sets `connected: false` |

### `useDriverRoster()` API

```typescript
function useDriverRoster(): {
  drivers: DriverPin[]                   // sorted: online-first, then by name
  driverMap: Record<string, DriverPin>   // O(1) lookup by driver_id
  connected: boolean
}
```

---

## 5. File-by-File Changes

### 5.1 `src/context/driver-roster-context.tsx` (new)

- `'use client'` component
- Defines `RosterState`, `RosterAction`, `reducer`
- `DriverRosterProvider`:
  1. On mount: `GET /v1/drivers` via `authFetch` → dispatch `ROSTER_INIT`
  2. Opens WebSocket via `useRosterEvents(callback)` → dispatches `LOCATION_UPDATED` / `STATUS_CHANGED`
  3. Exposes context via `DriverRosterContext`
- `useDriverRoster()` hook: reads context, throws if used outside provider, returns `{ drivers, driverMap, connected }`
- `drivers` array: sorted `online/available/en_route/delivering/returning` first, `on_break`/`offline` last, then alphabetical by name

**Error handling:**
- Initial fetch failure: `drivers` empty, `connected: false`; pages render their existing empty states
- WebSocket reconnect: handled by `useRosterEvents` (exponential backoff, capped at 30s, already implemented)
- Malformed WS frames: `useRosterEvents` already silently drops them

### 5.2 `src/app/(dashboard)/layout.tsx`

Single change: wrap `{children}` in `<DriverRosterProvider>`.

```tsx
// before
return <div className={styles.shell}>{children}</div>

// after
import { DriverRosterProvider } from '@/context/driver-roster-context'
return (
  <div className={styles.shell}>
    <DriverRosterProvider>{children}</DriverRosterProvider>
  </div>
)
```

Layout is already `'use client'` (Framer Motion sidebar). No other changes.

### 5.3 `src/app/(dashboard)/drivers/page.tsx`

- Remove: `useRosterEvents()` import and call
- Remove: local `drivers` useState + the `useCallback` event handler (~30 lines)
- Add: `const { drivers, connected } = useDriverRoster()`
- Wire `connected` to the existing System Status indicator (currently hardcoded or derived elsewhere)
- All filters, search, KPI strip, driver cards, onboard modal: unchanged

### 5.4 `src/app/(dashboard)/dispatch/page.tsx`

- Add: `const { drivers } = useDriverRoster()`
- Pass `drivers` directly as the `drivers` prop to `<LiveDispatchMap>` (types align — `DriverPin[]` on both sides)
- Existing dispatch queue, driver detail drawer, action buttons: unchanged

### 5.5 `src/components/maps/live-dispatch-map.tsx`

- **Delete** `SimulationMap` component (~150 lines) and the `NEXT_PUBLIC_MAPBOX_TOKEN` branch logic
- **Keep** `MapboxMap` exactly as-is
- **When token absent:** render a dark `div` with centered glassmorphism card:

  ```tsx
  if (!process.env.NEXT_PUBLIC_MAPBOX_TOKEN) {
    return (
      <div className="flex items-center justify-center h-full bg-[#050810]">
        <div className="glass-card p-6 text-center text-muted-foreground text-sm">
          Mapbox token not configured
        </div>
      </div>
    )
  }
  ```

- Manila fallback coordinate for drivers with `lat: null` (`14.5995, 120.9842`) stays in `MapboxMap` — legitimate UX (show driver at hub until first GPS fix)
- **Net:** 430 → ~280 lines

### 5.6 `docs/runbooks/fcm-vps-setup.md` (new)

Covers:
1. Where to obtain `FCM_PROJECT_ID` — Firebase Console → Project Settings → General → Project ID
2. Where to obtain `FCM_SERVICE_ACCOUNT_JSON` — Firebase Console → Project Settings → Service Accounts → Generate new private key → paste entire JSON as single-line env var (escape inner quotes)
3. Which container needs it — `logisticos-driver-ops` only
4. Compose env file path: `/etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code/.env`
5. Verification: `docker logs logisticos-driver-ops | grep FCM` → expect `FCM client initialized`
6. Degraded mode without FCM: assignments delivered via WebSocket only; drivers must have app foregrounded to receive

---

## 6. What Does NOT Change

- `src/hooks/useRosterEvents.ts` — called once inside provider, no modifications
- All backend services — no API changes required
- Driver app (Android) — no changes
- `ShipmentDetailPanel` POD viewer — already wired
- Dispatch action endpoints — already wired

---

## 7. Testing Checklist

- [ ] `/drivers` page shows live driver cards populated from API (not mocked)
- [ ] Status badge updates in real time when driver goes online/offline via driver app
- [ ] GPS coordinates on driver card update as driver moves
- [ ] `/dispatch` page live map renders Mapbox dark style with neon driver markers
- [ ] Driver markers update position on the map within ~2s of location push from driver app
- [ ] Navigating between `/drivers` and `/dispatch` maintains a single WebSocket (verify in Network tab: one `ws://` connection)
- [ ] Driver goes offline → marker color updates on map + roster badge changes
- [ ] FCM: driver app backgrounded → assign shipment → driver receives push notification → opens assignment screen
- [ ] Without `NEXT_PUBLIC_MAPBOX_TOKEN`: map renders dark placeholder card, no simulation, no crash
- [ ] WebSocket disconnect (kill driver-ops container briefly): reconnects within 30s, roster repopulates
