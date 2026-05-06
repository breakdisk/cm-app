# Admin Portal ↔ Driver App Production Launch Gaps — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the Admin Portal live dispatch map and driver roster to real WebSocket data via a shared context, and document FCM env-var setup for VPS deployment.

**Architecture:** A `DriverRosterProvider` mounts once in the authenticated dashboard layout, opens a single WebSocket to `/ws/locations` via the existing `useRosterEvents` hook, and exposes `useDriverRoster()`. The `/drivers` page removes its direct `useRosterEvents()` call and reads real-time patches from context. The `/dispatch` page sources its map `DriverPin[]` from context. `LiveDispatchMap` drops the ~150-line `SimulationMap` and gains full backend status support.

**Tech Stack:** Next.js 14 App Router, TypeScript, React `useContext` + `useReducer`, existing `useRosterEvents` hook, `authFetch`, Mapbox GL (`react-map-gl`)

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/hooks/useRosterEvents.ts` | Modify | Add optional `onConnect`/`onDisconnect` callbacks so context can track WS state |
| `src/context/driver-roster-context.tsx` | Create | Provider + reducer + `useDriverRoster()` hook |
| `src/components/maps/live-dispatch-map.tsx` | Modify | Expand `DriverPin` status type, remove `SimulationMap`, dark placeholder fallback |
| `src/app/(dashboard)/layout.tsx` | Modify | Wrap children in `<DriverRosterProvider>` |
| `src/app/(dashboard)/drivers/page.tsx` | Modify | Remove direct `useRosterEvents`, add context patch effect |
| `src/app/(dashboard)/dispatch/page.tsx` | Modify | Source `mapDrivers` from context, remove `toMapStatus` |
| `docs/runbooks/fcm-vps-setup.md` | Create | FCM env var setup guide for VPS/Dokploy |

---

## Task 1 — Extend `useRosterEvents` with lifecycle callbacks

**Files:**
- Modify: `apps/admin-portal/src/hooks/useRosterEvents.ts`

The hook currently only calls `onEvent` on messages. The context needs to know when the socket opens and closes to expose `connected: boolean`. Add an optional second argument with lifecycle callbacks — non-breaking change.

- [ ] **Step 1.1 — Add `RosterEventsOpts` interface and update signature**

Replace the file content with:

```typescript
"use client";
import { useEffect, useRef } from "react";
import { getAccessToken } from "@/lib/auth/auth-fetch";

export type RosterEvent =
  | {
      type: "location_updated";
      driver_id: string;
      tenant_id: string;
      lat: number;
      lng: number;
      heading?: number | null;
      speed_kmh?: number | null;
    }
  | {
      type: "status_changed";
      driver_id: string;
      tenant_id: string;
      status: "offline" | "available" | "en_route" | "delivering" | "returning" | "on_break";
      is_online: boolean;
      active_route_id?: string | null;
    };

export interface RosterEventsOpts {
  onConnect?: () => void;
  onDisconnect?: () => void;
}

// WebSocket endpoint on driver-ops service (not the API gateway)
const DRIVER_OPS_URL = process.env.NEXT_PUBLIC_DRIVER_OPS_URL ?? "http://localhost:8006";

function wsUrl(token: string): string {
  const base = DRIVER_OPS_URL.replace(/^http/, "ws");
  return `${base}/ws/locations?token=${encodeURIComponent(token)}`;
}

/**
 * Subscribe to the driver-ops RosterEvent WebSocket stream.
 * The server filters by tenant using the JWT — no client-side tenant check needed.
 * Reconnects with exponential backoff (capped at 30s). The latest `onEvent`
 * is always called — callers don't need to memoize it.
 */
export function useRosterEvents(
  onEvent: (event: RosterEvent) => void,
  opts?: RosterEventsOpts,
): void {
  const cbRef      = useRef(onEvent);
  const optsRef    = useRef(opts);
  cbRef.current    = onEvent;
  optsRef.current  = opts;

  useEffect(() => {
    let cancelled = false;
    let socket: WebSocket | null = null;
    let pingTimer: ReturnType<typeof setInterval> | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;
    let attempt = 0;

    async function connect() {
      if (cancelled) return;
      const token = await getAccessToken();
      if (!token || cancelled) return;

      const ws = new WebSocket(wsUrl(token));
      socket = ws;

      ws.onopen = () => {
        attempt = 0;
        optsRef.current?.onConnect?.();
        pingTimer = setInterval(() => {
          if (ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ type: "ping" }));
          }
        }, 25_000);
      };

      ws.onmessage = (ev) => {
        try {
          const msg = JSON.parse(ev.data);
          if (msg && (msg.type === "location_updated" || msg.type === "status_changed")) {
            cbRef.current(msg as RosterEvent);
          }
        } catch {
          // ignore malformed frames
        }
      };

      ws.onclose = () => {
        if (pingTimer) { clearInterval(pingTimer); pingTimer = null; }
        optsRef.current?.onDisconnect?.();
        if (cancelled) return;
        const delay = Math.min(30_000, 1_000 * 2 ** attempt);
        attempt += 1;
        retryTimer = setTimeout(connect, delay);
      };

      ws.onerror = () => {
        ws.close();
      };
    }

    connect();

    return () => {
      cancelled = true;
      if (pingTimer) clearInterval(pingTimer);
      if (retryTimer) clearTimeout(retryTimer);
      socket?.close();
    };
  }, []);
}
```

- [ ] **Step 1.2 — Type-check**

```bash
cd apps/admin-portal && npx tsc --noEmit 2>&1 | head -40
```

Expected: no errors on `useRosterEvents.ts` (existing callers pass only one argument, which is still valid).

- [ ] **Step 1.3 — Commit**

```bash
git add apps/admin-portal/src/hooks/useRosterEvents.ts
git commit -m "feat(admin-portal): add onConnect/onDisconnect lifecycle callbacks to useRosterEvents"
```

---

## Task 2 — Create `DriverRosterContext`

**Files:**
- Create: `apps/admin-portal/src/context/driver-roster-context.tsx`

The provider opens a single WebSocket (via `useRosterEvents`), fetches the initial driver list, and exposes `{ drivers, driverMap, connected, refresh }` to any page in the dashboard.

- [ ] **Step 2.1 — Create the file**

```typescript
"use client";
import {
  createContext,
  useContext,
  useReducer,
  useEffect,
  useCallback,
  type ReactNode,
} from "react";
import { authFetch } from "@/lib/auth/auth-fetch";
import { useRosterEvents, type RosterEvent } from "@/hooks/useRosterEvents";

// ── Types ─────────────────────────────────────────────────────────────────────

export type DriverStatus =
  | "offline"
  | "available"
  | "en_route"
  | "delivering"
  | "returning"
  | "on_break";

export interface DriverPin {
  driver_id: string;
  name: string;
  vehicle_type: string;
  plate: string;
  status: DriverStatus;
  lat: number | null;
  lng: number | null;
  heading: number | null;
  tasks_total: number;
  cod_collected_cents: number;
}

// ── State & reducer ───────────────────────────────────────────────────────────

interface RosterState {
  drivers: Record<string, DriverPin>;
  connected: boolean;
}

type RosterAction =
  | { type: "ROSTER_INIT"; payload: DriverPin[] }
  | { type: "LOCATION_UPDATED"; driver_id: string; lat: number; lng: number; heading: number | null }
  | { type: "STATUS_CHANGED"; driver_id: string; status: DriverStatus }
  | { type: "CONNECTED" }
  | { type: "DISCONNECTED" };

function normalizeStatus(s: string): DriverStatus {
  switch (s) {
    case "offline":
    case "available":
    case "en_route":
    case "delivering":
    case "returning":
    case "on_break":
      return s as DriverStatus;
    default:
      return "offline";
  }
}

function reducer(state: RosterState, action: RosterAction): RosterState {
  switch (action.type) {
    case "ROSTER_INIT": {
      const drivers: Record<string, DriverPin> = {};
      for (const d of action.payload) drivers[d.driver_id] = d;
      return { ...state, drivers };
    }
    case "LOCATION_UPDATED": {
      const existing = state.drivers[action.driver_id];
      if (!existing) return state;
      return {
        ...state,
        drivers: {
          ...state.drivers,
          [action.driver_id]: {
            ...existing,
            lat: action.lat,
            lng: action.lng,
            heading: action.heading,
          },
        },
      };
    }
    case "STATUS_CHANGED": {
      const existing = state.drivers[action.driver_id];
      if (!existing) return state;
      return {
        ...state,
        drivers: {
          ...state.drivers,
          [action.driver_id]: { ...existing, status: action.status },
        },
      };
    }
    case "CONNECTED":
      return { ...state, connected: true };
    case "DISCONNECTED":
      return { ...state, connected: false };
    default:
      return state;
  }
}

// ── Sorted drivers list ───────────────────────────────────────────────────────

const ONLINE_ORDER: Record<DriverStatus, number> = {
  en_route:   0,
  delivering: 1,
  returning:  2,
  available:  3,
  on_break:   4,
  offline:    5,
};

function sortedDrivers(map: Record<string, DriverPin>): DriverPin[] {
  return Object.values(map).sort((a, b) => {
    const diff = ONLINE_ORDER[a.status] - ONLINE_ORDER[b.status];
    return diff !== 0 ? diff : a.name.localeCompare(b.name);
  });
}

// ── Context ───────────────────────────────────────────────────────────────────

interface DriverRosterContextValue {
  drivers: DriverPin[];
  driverMap: Record<string, DriverPin>;
  connected: boolean;
  refresh: () => void;
}

const DriverRosterContext = createContext<DriverRosterContextValue | null>(null);

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

// ── Provider ──────────────────────────────────────────────────────────────────

export function DriverRosterProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, { drivers: {}, connected: false });

  const fetchRoster = useCallback(() => {
    authFetch(`${API_BASE}/v1/drivers?per_page=200`)
      .then((res) => res.json())
      .then((json) => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const pins: DriverPin[] = (json.data ?? []).map((d: any) => ({
          driver_id:          d.id,
          name:               d.name ?? `${d.first_name ?? ""} ${d.last_name ?? ""}`.trim(),
          vehicle_type:       d.vehicle_type ?? "",
          plate:              d.vehicle_plate ?? "",
          status:             normalizeStatus(d.status ?? "offline"),
          lat:                d.lat ?? null,
          lng:                d.lng ?? null,
          heading:            d.heading ?? null,
          tasks_total:        d.tasks_total ?? 0,
          cod_collected_cents: d.cod_collected_cents ?? d.cod_collected ?? 0,
        }));
        dispatch({ type: "ROSTER_INIT", payload: pins });
      })
      .catch(() => {
        // Leave roster empty — pages render their existing empty states
      });
  }, []);

  useEffect(() => { fetchRoster(); }, [fetchRoster]);

  const handleEvent = useCallback((event: RosterEvent) => {
    if (event.type === "location_updated") {
      dispatch({
        type:      "LOCATION_UPDATED",
        driver_id: event.driver_id,
        lat:       event.lat,
        lng:       event.lng,
        heading:   event.heading ?? null,
      });
    } else {
      dispatch({
        type:      "STATUS_CHANGED",
        driver_id: event.driver_id,
        status:    normalizeStatus(event.status),
      });
    }
  }, []);

  useRosterEvents(handleEvent, {
    onConnect:    () => dispatch({ type: "CONNECTED" }),
    onDisconnect: () => dispatch({ type: "DISCONNECTED" }),
  });

  const value: DriverRosterContextValue = {
    drivers:   sortedDrivers(state.drivers),
    driverMap: state.drivers,
    connected: state.connected,
    refresh:   fetchRoster,
  };

  return (
    <DriverRosterContext.Provider value={value}>
      {children}
    </DriverRosterContext.Provider>
  );
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useDriverRoster(): DriverRosterContextValue {
  const ctx = useContext(DriverRosterContext);
  if (!ctx) throw new Error("useDriverRoster must be used within DriverRosterProvider");
  return ctx;
}
```

- [ ] **Step 2.2 — Type-check**

```bash
cd apps/admin-portal && npx tsc --noEmit 2>&1 | head -40
```

Expected: no errors on the new file.

- [ ] **Step 2.3 — Commit**

```bash
git add apps/admin-portal/src/context/driver-roster-context.tsx
git commit -m "feat(admin-portal): add DriverRosterContext with shared WebSocket + reducer"
```

---

## Task 3 — Update `live-dispatch-map.tsx`

**Files:**
- Modify: `apps/admin-portal/src/components/maps/live-dispatch-map.tsx`

Three changes:
1. Expand `DriverPin["status"]` to include all backend statuses (`available`, `offline`, `on_break`)
2. Remove the 150-line `SimulationMap` component and its helpers (`MANILA_LAYOUT`, `getDriverPosition`)
3. Replace `SimulationMap` fallback with a dark placeholder `div`

- [ ] **Step 3.1 — Replace the file**

Write the entire new file (SimulationMap block removed, types and statusColor/statusLabel updated):

```typescript
"use client";
import { useRef, useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { cn } from "@/lib/design-system/cn";
import { colors } from "@/lib/design-system/tokens";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface DriverPin {
  driver_id: string;
  driver_name: string;
  lat: number;
  lng: number;
  status: "available" | "en_route" | "delivering" | "returning" | "on_break" | "offline";
  deliveries_remaining: number;
}

export interface RouteGeoJson {
  driver_id: string;
  geojson: GeoJSON.FeatureCollection;
  color: string;
}

interface LiveDispatchMapProps {
  drivers?: DriverPin[];
  routes?: RouteGeoJson[];
  onDriverClick?: (driver: DriverPin) => void;
  className?: string;
}

const statusColor: Record<DriverPin["status"], string> = {
  available:  colors.amber.signal,
  en_route:   colors.cyan.neon,
  delivering: colors.green.signal,
  returning:  colors.purple.plasma,
  on_break:   colors.amber.signal,
  offline:    "rgba(255,255,255,0.18)",
};

const statusLabel: Record<DriverPin["status"], string> = {
  available:  "Available",
  en_route:   "En Route",
  delivering: "Delivering",
  returning:  "Returning",
  on_break:   "On Break",
  offline:    "Offline",
};

// ── Mapbox map ────────────────────────────────────────────────────────────────

function MapboxMap({
  drivers,
  routes = [],
  onDriverClick,
  className,
}: LiveDispatchMapProps & { drivers: DriverPin[] }) {
  const [Map, setMap]   = useState<any>(null);
  const [libs, setLibs] = useState<any>(null);
  const mapRef          = useRef<any>(null);
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      import("react-map-gl"),
      import("mapbox-gl/dist/mapbox-gl.css" as any),
    ]).then(([mapgl]) => {
      setMap(() => mapgl.default);
      setLibs(mapgl);
    });
  }, []);

  const visibleDrivers = drivers.filter(
    (d) => d.status !== "offline" || (d.lat !== 0 && d.lng !== 0),
  );

  useEffect(() => {
    if (!visibleDrivers.length || !mapRef.current) return;
    const lngs = visibleDrivers.map((d) => d.lng);
    const lats  = visibleDrivers.map((d) => d.lat);
    mapRef.current.fitBounds(
      [[Math.min(...lngs) - 0.02, Math.min(...lats) - 0.02],
       [Math.max(...lngs) + 0.02, Math.max(...lats) + 0.02]],
      { padding: 60, duration: 1200 },
    );
  }, [visibleDrivers]);

  if (!Map || !libs) return null;
  const { Marker, Source, Layer } = libs;
  const MAPBOX_TOKEN = process.env.NEXT_PUBLIC_MAPBOX_TOKEN ?? "";

  return (
    <div className={cn("relative rounded-2xl overflow-hidden border border-glass-border", className)}>
      <Map
        ref={mapRef}
        mapboxAccessToken={MAPBOX_TOKEN}
        mapStyle="mapbox://styles/mapbox/dark-v11"
        style={{ width: "100%", height: "100%" }}
        initialViewState={{ longitude: 121.774, latitude: 12.879, zoom: 6 }}
        attributionControl={false}
      >
        {routes.map((route) => (
          <Source key={route.driver_id} type="geojson" data={route.geojson}>
            <Layer
              id={`route-${route.driver_id}`}
              type="line"
              paint={{
                "line-color":     route.color,
                "line-width":     2,
                "line-opacity":   0.7,
                "line-dasharray": [2, 1],
              }}
            />
          </Source>
        ))}
        {visibleDrivers.map((driver) => (
          <Marker
            key={driver.driver_id}
            longitude={driver.lng}
            latitude={driver.lat}
            anchor="center"
            onClick={() => {
              setSelected(driver.driver_id);
              onDriverClick?.(driver);
            }}
          >
            <motion.div
              initial={{ scale: 0 }}
              animate={{ scale: 1 }}
              whileHover={{ scale: 1.2 }}
              className="relative cursor-pointer"
            >
              {(driver.status === "en_route" || driver.status === "delivering") && (
                <span
                  className="absolute inset-0 rounded-full animate-beacon"
                  style={{ background: statusColor[driver.status] }}
                />
              )}
              <span
                className="relative flex h-4 w-4 rounded-full border-2 border-canvas"
                style={{ background: statusColor[driver.status] }}
              />
              {driver.deliveries_remaining > 0 && (
                <span className="absolute -top-2.5 -right-2.5 flex h-4 w-4 items-center justify-center rounded-full bg-canvas border border-glass-border text-2xs font-mono text-white/70">
                  {driver.deliveries_remaining}
                </span>
              )}
            </motion.div>
          </Marker>
        ))}
      </Map>

      {/* Live badge */}
      <div className="absolute top-4 right-4 z-20">
        <span className="inline-flex items-center gap-1.5 glass-sm px-2.5 py-1 rounded-full">
          <span className="relative flex h-1.5 w-1.5">
            <span className="absolute inline-flex h-full w-full rounded-full bg-green-signal opacity-75 animate-beacon" />
            <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-green-signal" />
          </span>
          <span className="text-2xs font-mono text-green-signal uppercase tracking-widest">Live</span>
        </span>
      </div>

      {/* Status legend */}
      <div className="absolute bottom-4 left-4 glass-sm rounded-xl p-3 flex flex-col gap-1.5 z-20">
        {(["available", "en_route", "delivering", "returning", "on_break"] as const).map((s) => (
          <div key={s} className="flex items-center gap-2">
            <span
              className="h-2 w-2 rounded-full flex-shrink-0"
              style={{ background: statusColor[s], boxShadow: `0 0 5px ${statusColor[s]}` }}
            />
            <span className="text-2xs font-mono text-white/50">{statusLabel[s]}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ── Exported component ────────────────────────────────────────────────────────

export function LiveDispatchMap({
  drivers = [],
  routes  = [],
  onDriverClick,
  className,
}: LiveDispatchMapProps) {
  const MAPBOX_TOKEN = process.env.NEXT_PUBLIC_MAPBOX_TOKEN ?? "";

  if (!MAPBOX_TOKEN) {
    return (
      <div
        className={cn(
          "flex items-center justify-center rounded-2xl border border-glass-border bg-[#050810]",
          className,
        )}
      >
        <div className="glass-card p-6 text-center text-muted-foreground text-sm font-mono">
          Mapbox token not configured
        </div>
      </div>
    );
  }

  return (
    <MapboxMap
      drivers={drivers}
      routes={routes}
      onDriverClick={onDriverClick}
      className={className}
    />
  );
}
```

- [ ] **Step 3.2 — Type-check**

```bash
cd apps/admin-portal && npx tsc --noEmit 2>&1 | head -40
```

Expected: errors only on `dispatch/page.tsx` where `DriverPin["status"]` now includes new values — those will be fixed in Task 6. Ignore those. No errors on the map file itself.

- [ ] **Step 3.3 — Commit**

```bash
git add apps/admin-portal/src/components/maps/live-dispatch-map.tsx
git commit -m "feat(admin-portal): expand LiveDispatchMap status taxonomy, remove SimulationMap"
```

---

## Task 4 — Mount `DriverRosterProvider` in dashboard layout

**Files:**
- Modify: `apps/admin-portal/src/app/(dashboard)/layout.tsx`

The layout already renders `{children}` at line 486 inside `<main>`. Wrap it in the provider.

- [ ] **Step 4.1 — Add import**

At the top of `layout.tsx`, after the existing imports, add:

```typescript
import { DriverRosterProvider } from "@/context/driver-roster-context";
```

- [ ] **Step 4.2 — Wrap children**

Find this block (around line 479):

```tsx
        {/* ── Page content ──────────────────────────────────────────────── */}
        <main className="flex-1 overflow-auto bg-canvas p-4 md:p-6">
          <motion.div
            key={pathname}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
          >
            {children}
          </motion.div>
        </main>
```

Replace with:

```tsx
        {/* ── Page content ──────────────────────────────────────────────── */}
        <main className="flex-1 overflow-auto bg-canvas p-4 md:p-6">
          <DriverRosterProvider>
            <motion.div
              key={pathname}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
            >
              {children}
            </motion.div>
          </DriverRosterProvider>
        </main>
```

- [ ] **Step 4.3 — Type-check**

```bash
cd apps/admin-portal && npx tsc --noEmit 2>&1 | head -40
```

Expected: no new errors from this file.

- [ ] **Step 4.4 — Commit**

```bash
git add apps/admin-portal/src/app/\(dashboard\)/layout.tsx
git commit -m "feat(admin-portal): mount DriverRosterProvider in dashboard layout"
```

---

## Task 5 — Refactor `/drivers/page.tsx` to consume context

**Files:**
- Modify: `apps/admin-portal/src/app/(dashboard)/drivers/page.tsx`

Three changes:
1. Remove the direct `useRosterEvents` import and call (lines 11, 144–166)
2. Add `useDriverRoster()` and a `useEffect` that patches local driver state whenever `driverMap` updates
3. Wire `connected` to a small inline indicator; replace the Refresh button handler to use `refresh()`

- [ ] **Step 5.1 — Update imports**

Find:

```typescript
import { useRosterEvents, type RosterEvent } from "@/hooks/useRosterEvents";
```

Replace with:

```typescript
import { useDriverRoster } from "@/context/driver-roster-context";
```

- [ ] **Step 5.2 — Update state wiring in `DriversPage`**

Find this block (around line 94):

```typescript
export default function DriversPage() {
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<DriverStatus | "all" | "online">("all");
  const [drivers, setDrivers] = useState<Driver[]>(DRIVERS);
  const [kpi, setKpi] = useState(KPI);
  const [loading, setLoading] = useState(false);
  const [onboardOpen, setOnboardOpen] = useState(false);
```

Replace with:

```typescript
export default function DriversPage() {
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<DriverStatus | "all" | "online">("all");
  const [drivers, setDrivers] = useState<Driver[]>(DRIVERS);
  const [kpi, setKpi] = useState(KPI);
  const [loading, setLoading] = useState(false);
  const [onboardOpen, setOnboardOpen] = useState(false);

  const { driverMap, connected, refresh: refreshRoster } = useDriverRoster();
```

- [ ] **Step 5.3 — Remove the `handleRosterEvent` callback and `useRosterEvents` call**

Find and delete this entire block (lines 141–166):

```typescript
  // ── Live roster WS ──────────────────────────────────────────────────────────
  // Patch driver state in-place as events arrive — no refetch, no flicker.
  // Unknown driver_ids are ignored (roster refetch will pick up new drivers).
  const handleRosterEvent = useCallback((event: RosterEvent) => {
    setDrivers((prev) => {
      const idx = prev.findIndex((d) => d.id === event.driver_id);
      if (idx === -1) return prev;
      const next = [...prev];
      if (event.type === "status_changed") {
        next[idx] = {
          ...next[idx],
          status: normalizeStatus(event.status),
          last_seen: "Just now",
        };
      } else {
        next[idx] = {
          ...next[idx],
          last_location: `${event.lat.toFixed(4)}, ${event.lng.toFixed(4)}`,
          last_seen: "Just now",
        };
      }
      return next;
    });
  }, []);

  useRosterEvents(handleRosterEvent);
```

Replace with:

```typescript
  // ── Live roster patches from shared context ──────────────────────────────────
  // The context WebSocket updates driverMap on every location/status event.
  // Merge into local drivers (which carry grade, tasks_done from API fetch).
  useEffect(() => {
    if (Object.keys(driverMap).length === 0) return;
    setDrivers((prev) =>
      prev.map((d) => {
        const live = driverMap[d.id];
        if (!live) return d;
        return {
          ...d,
          status:        normalizeStatus(live.status),
          last_location: live.lat !== null
            ? `${live.lat.toFixed(4)}, ${live.lng.toFixed(4)}`
            : d.last_location,
          last_seen:     "Just now",
        };
      }),
    );
  }, [driverMap]);
```

- [ ] **Step 5.4 — Remove the unused `useCallback` import if it becomes unused**

Check the imports line:

```typescript
import { useState, useEffect, useCallback, useMemo } from "react";
```

`useCallback` is now unused (was only used by `handleRosterEvent` and `fetchDrivers`). `fetchDrivers` still uses `useCallback`, so keep it. No change needed.

- [ ] **Step 5.5 — Add disconnected indicator and wire Refresh to context refresh**

Find the Refresh button (around line 196):

```tsx
          <button
            onClick={fetchDrivers}
            disabled={loading}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-50"
          >
            <RefreshCw size={12} className={loading ? "animate-spin" : ""} /> Refresh
          </button>
```

Replace with:

```tsx
          {!connected && (
            <span className="flex items-center gap-1.5 rounded-lg border border-red-signal/30 bg-red-signal/10 px-3 py-1.5 text-xs font-mono text-red-signal">
              WS disconnected
            </span>
          )}
          <button
            onClick={() => { fetchDrivers(); refreshRoster(); }}
            disabled={loading}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-50"
          >
            <RefreshCw size={12} className={loading ? "animate-spin" : ""} /> Refresh
          </button>
```

- [ ] **Step 5.6 — Type-check**

```bash
cd apps/admin-portal && npx tsc --noEmit 2>&1 | head -40
```

Expected: no errors on this file.

- [ ] **Step 5.7 — Commit**

```bash
git add apps/admin-portal/src/app/\(dashboard\)/drivers/page.tsx
git commit -m "feat(admin-portal): drivers page consumes DriverRosterContext for real-time patches"
```

---

## Task 6 — Wire `/dispatch/page.tsx` to context

**Files:**
- Modify: `apps/admin-portal/src/app/(dashboard)/dispatch/page.tsx`

Three changes:
1. Add `useDriverRoster` import
2. Delete `toMapStatus()` helper (lines 92–100) — no longer needed
3. Replace the `mapDrivers` derivation (lines 379–390) to use context `DriverPin[]`

- [ ] **Step 6.1 — Add import**

Find the import block at the top. After:

```typescript
import { authFetch } from "@/lib/auth/auth-fetch";
```

Add:

```typescript
import { useDriverRoster } from "@/context/driver-roster-context";
```

- [ ] **Step 6.2 — Delete `toMapStatus`**

Find and delete this function (lines 92–100):

```typescript
/** Map driver-ops status → LiveDispatchMap pin status */
function toMapStatus(s?: string): DriverPin["status"] {
  switch (s) {
    case "en_route":   return "en_route";
    case "delivering": return "delivering";
    case "returning":  return "returning";
    default:           return "idle";
  }
}
```

Delete it entirely. `DriverPin["status"]` now includes `available` and `offline` so no mapping is needed.

- [ ] **Step 6.3 — Add `useDriverRoster` call in the component**

Find the start of the `DispatchConsole` (or unnamed default export) function body, near where `useState` declarations are. Add immediately after the existing `useState` declarations:

```typescript
  const { drivers: rosterDrivers } = useDriverRoster();
```

- [ ] **Step 6.4 — Replace `mapDrivers` computation**

Find (lines 379–390):

```typescript
  // Build driver pins for the live map using real GPS coordinates.
  // Falls back to Manila city centre only when a driver has never shared location.
  const mapDrivers: DriverPin[] = drivers
    .filter((d) => d.status !== "offline" || d.lat != null)
    .map((d) => ({
      driver_id:            d.id,
      driver_name:          [d.first_name, d.last_name].filter(Boolean).join(" ") || d.email || d.id,
      lat:                  d.lat ?? MANILA_LAT,
      lng:                  d.lng ?? MANILA_LNG,
      status:               toMapStatus(d.status),
      deliveries_remaining: 0,
    }));
```

Replace with:

```typescript
  // Build driver pins for the live map from the shared WebSocket-backed context.
  // Drivers without a GPS fix are excluded from the map until first location event.
  const mapDrivers: DriverPin[] = rosterDrivers
    .filter((d) => d.lat !== null && d.lng !== null)
    .map((d) => ({
      driver_id:            d.driver_id,
      driver_name:          d.name,
      lat:                  d.lat as number,
      lng:                  d.lng as number,
      status:               d.status,
      deliveries_remaining: 0,
    }));
```

- [ ] **Step 6.5 — Delete unused `MANILA_LAT` / `MANILA_LNG` constants**

Find and delete lines 22–24 in `dispatch/page.tsx`:

```typescript
// Manila city center — fallback when driver hasn't shared GPS yet
const MANILA_LAT = 14.5995;
const MANILA_LNG = 120.9842;
```

These constants are no longer referenced after the `mapDrivers` replacement.

- [ ] **Step 6.6 — Type-check (final clean)**

```bash
cd apps/admin-portal && npx tsc --noEmit 2>&1 | head -40
```

Expected: zero errors across all modified files.

- [ ] **Step 6.7 — Commit**

```bash
git add apps/admin-portal/src/app/\(dashboard\)/dispatch/page.tsx
git commit -m "feat(admin-portal): dispatch page sources live map pins from DriverRosterContext"
```

---

## Task 7 — FCM VPS setup runbook

**Files:**
- Create: `docs/runbooks/fcm-vps-setup.md`

- [ ] **Step 7.1 — Create the file**

```markdown
# FCM VPS Setup — driver-ops Push Notifications

Firebase Cloud Messaging lets the driver app receive task assignment notifications
when the app is **backgrounded**. Without it, assignments still arrive via WebSocket
when the app is foregrounded — but drivers won't see the push badge.

## 1 — Get `FCM_PROJECT_ID`

1. Open [Firebase Console](https://console.firebase.google.com) → select the LogisticOS project
2. Click the gear icon → **Project settings** → **General** tab
3. Copy the **Project ID** (e.g. `logisticos-prod`)

## 2 — Get `FCM_SERVICE_ACCOUNT_JSON`

1. **Project settings** → **Service accounts** tab
2. Click **Generate new private key** → **Generate key**
3. A JSON file downloads. Open it and copy the entire contents.
4. Minify it to a single line (remove all newlines):
   ```bash
   cat firebase-service-account.json | python3 -m json.tool --compact
   ```
5. The resulting single-line JSON string is the value of `FCM_SERVICE_ACCOUNT_JSON`.

> **Security:** Do not commit this JSON. Store it only in the Dokploy env file.

## 3 — Set env vars on VPS

SSH into the VPS and edit the compose env file:

```bash
nano /etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code/.env
```

Add (or update) these two lines:

```
FCM_PROJECT_ID=your-project-id-here
FCM_SERVICE_ACCOUNT_JSON={"type":"service_account","project_id":"...","private_key":"-----BEGIN RSA PRIVATE KEY-----\n..."}
```

Only the `logisticos-driver-ops` container reads these. No other service needs them.

## 4 — Redeploy driver-ops

```bash
cd /etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code
docker compose up -d --no-deps logisticos-driver-ops
```

## 5 — Verify

```bash
docker logs logisticos-driver-ops 2>&1 | grep FCM
```

Expected output:
```
FCM client initialized for project logisticos-prod
```

If you see `FCM disabled — FCM_SERVICE_ACCOUNT_JSON not set`, the env var was not loaded.
Try `docker compose down logisticos-driver-ops && docker compose up -d logisticos-driver-ops`.

## 6 — Degraded mode without FCM

If FCM is not configured:
- Drivers receive assignments via **WebSocket only**
- Assignment notification appears only when the driver app is **foregrounded**
- Accept/reject flow works normally once the notification appears
- No data is lost — missed push = driver just doesn't get the badge
```

- [ ] **Step 7.2 — Commit**

```bash
git add docs/runbooks/fcm-vps-setup.md
git commit -m "docs: add FCM VPS setup runbook for driver-ops push notifications"
```

---

## Verification Checklist

Run through these manually after all tasks are committed and deployed:

- [ ] Network tab shows **one** `ws://` connection when navigating between `/drivers` and `/dispatch`
- [ ] `/drivers` page: driver status badge updates within ~2s of driver going online in the app
- [ ] `/drivers` page: GPS coordinates update as driver moves; "WS disconnected" badge appears if driver-ops container is stopped
- [ ] `/dispatch` page: `LiveDispatchMap` renders Mapbox dark style with neon markers (requires `NEXT_PUBLIC_MAPBOX_TOKEN` set)
- [ ] Driver marker positions update on the map within ~2s of location push from driver app
- [ ] Marker color matches status (amber=available, cyan=en_route, green=delivering, purple=returning)
- [ ] Map renders dark placeholder card (no crash) when `NEXT_PUBLIC_MAPBOX_TOKEN` is unset
- [ ] FCM: background driver app → dispatch a shipment → driver receives push notification → opens AssignmentScreen
- [ ] WebSocket reconnect: stop driver-ops container for 5s, restart — roster repopulates within 30s
