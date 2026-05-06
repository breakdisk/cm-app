"use client";
import {
  createContext,
  useContext,
  useReducer,
  useEffect,
  useCallback,
  useMemo,
  useRef,
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
        const total = json.meta?.total ?? json.total_count ?? null;
        if (total !== null && total > pins.length) {
          console.warn(
            `DriverRosterContext: fetched ${pins.length} of ${total} drivers — increase per_page or add pagination`,
          );
        }
        dispatch({ type: "ROSTER_INIT", payload: pins });
      })
      .catch((err) => {
        console.error("DriverRosterContext: fetchRoster failed", err);
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

  const disconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useRosterEvents(handleEvent, {
    onConnect: () => {
      if (disconnectTimer.current) {
        clearTimeout(disconnectTimer.current);
        disconnectTimer.current = null;
      }
      dispatch({ type: "CONNECTED" });
    },
    onDisconnect: () => {
      disconnectTimer.current = setTimeout(() => {
        dispatch({ type: "DISCONNECTED" });
        disconnectTimer.current = null;
      }, 3_000);
    },
  });

  const value = useMemo<DriverRosterContextValue>(
    () => ({
      drivers:   sortedDrivers(state.drivers),
      driverMap: state.drivers,
      connected: state.connected,
      refresh:   fetchRoster,
    }),
    [state.drivers, state.connected, fetchRoster],
  );

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
