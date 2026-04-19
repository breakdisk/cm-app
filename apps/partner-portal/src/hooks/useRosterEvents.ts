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

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

function wsUrl(token: string): string {
  const base = API_BASE.replace(/^http/, "ws");
  return `${base}/ws/locations?token=${encodeURIComponent(token)}`;
}

/**
 * Subscribe to the driver-ops RosterEvent WebSocket stream.
 * The server filters by tenant using the JWT — no client-side tenant check needed.
 * Reconnects with exponential backoff (capped at 30s). The latest `onEvent`
 * is always called — callers don't need to memoize it.
 */
export function useRosterEvents(onEvent: (event: RosterEvent) => void): void {
  const cbRef = useRef(onEvent);
  cbRef.current = onEvent;

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
        if (cancelled) return;
        const delay = Math.min(30_000, 1_000 * 2 ** attempt);
        attempt += 1;
        retryTimer = setTimeout(connect, delay);
      };

      ws.onerror = () => { ws.close(); };
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
