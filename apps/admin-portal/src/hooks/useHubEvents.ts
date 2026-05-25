'use client';

import { useEffect, useRef, useCallback } from 'react';

export type HubEventType = 'plan_computed' | 'placements_updated' | 'box_scanned';

export interface HubEvent {
  type:           HubEventType;
  plan_id?:       string;
  hub_id?:        string;
  volume_pct?:    number;
  weight_kg?:     number;
  piece_count?:   number;
  unplaced_count?: number;
}

interface UseHubEventsOptions {
  hubId:      string;
  token:      string | null;
  onEvent:    (event: HubEvent) => void;
  onConnect?:  () => void;
  onDisconnect?: () => void;
}

const HUB_OPS_WS_URL =
  process.env.NEXT_PUBLIC_HUB_OPS_WS_URL ??
  process.env.NEXT_PUBLIC_API_URL?.replace(/^http/, 'ws') ??
  'ws://localhost:3009';

const MAX_BACKOFF_MS = 30_000;

/**
 * Subscribes to the hub-ops WebSocket event bus for a given hub.
 *
 * Reconnects automatically with exponential back-off (capped at 30 s).
 * The `?token=` query param carries the JWT — same pattern as `useRosterEvents`.
 */
export function useHubEvents({
  hubId,
  token,
  onEvent,
  onConnect,
  onDisconnect,
}: UseHubEventsOptions) {
  const onEventRef      = useRef(onEvent);
  const onConnectRef    = useRef(onConnect);
  const onDisconnectRef = useRef(onDisconnect);

  useEffect(() => { onEventRef.current      = onEvent;      }, [onEvent]);
  useEffect(() => { onConnectRef.current    = onConnect;    }, [onConnect]);
  useEffect(() => { onDisconnectRef.current = onDisconnect; }, [onDisconnect]);

  useEffect(() => {
    if (!hubId || !token) return;

    let ws: WebSocket | null = null;
    let cancelled   = false;
    let backoff     = 1_000;
    let pingTimer:  ReturnType<typeof setTimeout> | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    function clearTimers() {
      if (pingTimer)  clearTimeout(pingTimer);
      if (retryTimer) clearTimeout(retryTimer);
    }

    function connect() {
      if (cancelled) return;

      const url = `${HUB_OPS_WS_URL}/v1/hubs/${hubId}/ws?token=${encodeURIComponent(token!)}`;
      ws = new WebSocket(url);

      ws.onopen = () => {
        if (cancelled) { ws?.close(); return; }
        backoff = 1_000;
        onConnectRef.current?.();

        // 25 s keepalive ping — server ignores unknown frames but keeps connection open.
        pingTimer = setInterval(() => {
          if (ws?.readyState === WebSocket.OPEN) ws.send('ping');
        }, 25_000) as unknown as ReturnType<typeof setTimeout>;
      };

      ws.onmessage = (e) => {
        try {
          const event: HubEvent = JSON.parse(e.data as string);
          onEventRef.current(event);
        } catch {
          // non-JSON frame (e.g. pong echo) — ignore
        }
      };

      ws.onclose = () => {
        clearTimers();
        onDisconnectRef.current?.();
        if (!cancelled) {
          retryTimer = setTimeout(() => {
            backoff = Math.min(backoff * 2, MAX_BACKOFF_MS);
            connect();
          }, backoff);
        }
      };

      ws.onerror = () => {
        ws?.close();
      };
    }

    connect();

    return () => {
      cancelled = true;
      clearTimers();
      ws?.close();
    };
  }, [hubId, token]);
}
