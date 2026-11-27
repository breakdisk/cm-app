/**
 * OmniDeliv vendor order queue — merchant-portal client.
 *
 * The queue endpoint is the record. Every alert the console raises is a hint
 * that something is on it; a missed alert costs a poll interval and never an
 * order. That is why the console polls unconditionally rather than refreshing
 * only when something tells it to.
 */
import { authFetch } from "@/lib/auth/auth-fetch";
import { API_BASE } from "@/lib/api/endpoints";

/** Mirrors `LegStatus` in services/omnideliv. Only the live ones reach here. */
export type LegStatus = "pending" | "accepted" | "preparing" | "ready";

export interface VendorLegRow {
  leg_id: string;
  order_id: string;
  status: LegStatus;
  goods_subtotal_cents: number;
  ready_in_minutes: number | null;
  accepted_at: string | null;
  created_at: string;
}

export interface TransitionResponse {
  leg_id: string;
  status: string;
  /** False when the leg was already in that state — a retry, or a colleague. */
  changed: boolean;
}

/** Raised for a 409 so the console can explain it rather than dump an error. */
export class LegConflictError extends Error {
  constructor() {
    super("Someone else already updated this order.");
    this.name = "LegConflictError";
  }
}

async function post(path: string, body?: unknown): Promise<TransitionResponse> {
  const res = await authFetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      // One key per attempt. A retry of THIS submission replays the stored
      // answer instead of acting twice; a fresh tap is a new action and gets a
      // new key. Matters most on accept, which is the transition that will
      // trigger a payment capture once that work lands.
      "X-Idempotency-Key": crypto.randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (res.status === 409) throw new LegConflictError();
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    throw new Error(detail || "That did not go through. Try again.");
  }
  return res.json();
}

export const vendorOrdersApi = {
  async queue(): Promise<VendorLegRow[]> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/vendors/me/orders`);
    // Signed in, but this login runs no store. An empty queue is the honest
    // answer; the nav gate is what stops a parcel merchant getting here.
    if (res.status === 404) return [];
    if (!res.ok) throw new Error("Could not load the order queue");
    return res.json();
  },

  accept: (legId: string, readyInMinutes: number) =>
    post(`/v1/omnideliv/vendors/me/legs/${legId}/accept`, {
      ready_in_minutes: readyInMinutes,
    }),

  reject: (legId: string, reason: string) =>
    post(`/v1/omnideliv/vendors/me/legs/${legId}/reject`, { reason }),

  ready: (legId: string) => post(`/v1/omnideliv/vendors/me/legs/${legId}/ready`),

  served: (legId: string) => post(`/v1/omnideliv/vendors/me/legs/${legId}/served`),
};
