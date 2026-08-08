/**
 * Consumes the mesh's SSE stream.
 *
 * React Native has no EventSource, so the stream is parsed by hand over
 * expo/fetch's streaming response. That also makes the reconnect loop ours —
 * hence the pure backoff function: a fake-timer test against a self-scheduling
 * loop shares one virtual clock and hangs rather than fails, which has already
 * cost this repo two six-hour CI runs on the Android app.
 */
import { useCallback, useRef, useState } from "react";
import { fetch as expoFetch } from "expo/fetch";

import { API_BASE, authHeaders } from "../api/client";
import type { MeshEvent } from "../api/mesh";
import { parseSseChunk } from "../api/sse";

// Re-exported so callers have one import site for the stream contract.
export { parseSseChunk, reconnectDelayMs } from "../api/sse";

export interface MeshRunState {
  events: MeshEvent[];
  running: boolean;
  error: string | null;
}

export function useMeshRun() {
  const [state, setState] = useState<MeshRunState>({ events: [], running: false, error: null });
  const abort = useRef<AbortController | null>(null);

  /**
   * `where` is the customer's delivery point. Every vendor search in the run is
   * centred on it, so it is a required argument rather than something the hook
   * defaults — a wrong point returns plausible shops in the wrong place, which
   * looks like the agent reasoning badly instead of a missing field.
   */
  const run = useCallback(async (utterance: string, where: { lat: number; lng: number }) => {
    abort.current?.abort();
    const controller = new AbortController();
    abort.current = controller;

    setState({ events: [], running: true, error: null });

    try {
      const res = await expoFetch(`${API_BASE}/v1/omnideliv/mesh/run`, {
        method: "POST",
        headers: await authHeaders(),
        body: JSON.stringify({
          utterance,
          delivery_lat: where.lat,
          delivery_lng: where.lng,
        }),
        signal: controller.signal,
      });

      if (!res.ok || !res.body) {
        throw new Error(`mesh run failed: ${res.status}`);
      }

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const { events, rest } = parseSseChunk(buffer);
        buffer = rest;

        if (events.length > 0) {
          setState((s) => ({ ...s, events: [...s.events, ...events] }));
        }
      }

      setState((s) => ({ ...s, running: false }));
    } catch (e) {
      if (controller.signal.aborted) return; // the caller navigated away
      setState((s) => ({
        ...s,
        running: false,
        // The run may still be completing server-side; the basket persists
        // either way, so this is recoverable rather than lost work.
        error: e instanceof Error ? e.message : "Lost connection",
      }));
    }
  }, []);

  const cancel = useCallback(() => abort.current?.abort(), []);

  return { ...state, run, cancel };
}
