/**
 * OmniDeliv couriers — the field-ops roster.
 *
 * These are **not** the drivers on `/drivers`. That page talks to
 * `driver-ops`, LogisticOS's product tier, where a driver is employed
 * (`FullTime | PartTime`), belongs to a carrier and runs routes. A courier
 * lives in `field-ops`, the platform tier shared across products (ADR-0015):
 * no employment type, no carrier, one live claim at a time, paid per job from a
 * weekly ledger.
 *
 * The same human can be both. They are two rows in two services, and nothing
 * links them.
 *
 * Until this file existed there was no management surface for couriers at all —
 * not here, not in the partner portal. The only way to see or suspend one was
 * SQL against `field_ops.couriers`.
 */
import { authFetch } from "@/lib/auth/auth-fetch";

const BASE = process.env.NEXT_PUBLIC_API_BASE ?? "";

export interface AdminCourier {
  id:            string;
  user_id:       string;
  first_name:    string;
  last_name:     string;
  phone:         string;
  /** `offline | available | assigned | on_break` — the courier's own duty flag. */
  status:        string;
  /** Ops' flag. Suspension lives here, not in `status`. */
  is_active:     boolean;
  vehicle_type:  string | null;
  zone:          string | null;
  last_lat:      number | null;
  last_lng:      number | null;
  last_seen_at:  string | null;
  /**
   * `is_active && status === "available"`, computed server-side.
   *
   * Both halves matter and neither implies the other: a suspended courier can
   * go on duty all day and still never be offered a job, and reinstating one
   * does not clock them on. Sent from the server so the two places that decide
   * it cannot drift.
   */
  dispatchable:  boolean;
}

async function okJson(r: Response) {
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j?.error?.message ?? j?.message ?? `HTTP ${r.status}`);
  return j;
}

export async function fetchCouriers(): Promise<AdminCourier[]> {
  const j = await okJson(await authFetch(`${BASE}/v1/field-ops/admin/couriers?limit=200`));
  return j.couriers ?? [];
}

/**
 * Suspend or reinstate.
 *
 * 404 means the courier is not in this tenant. field-ops has no row-level
 * security — the tenant bound into each query is the whole of it — so a foreign
 * id reads as absent rather than forbidden, and the message says so plainly
 * rather than implying the row exists somewhere.
 */
export async function setCourierActive(id: string, active: boolean): Promise<void> {
  const r = await authFetch(`${BASE}/v1/field-ops/admin/couriers/${id}/active`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ active }),
  });
  if (r.ok || r.status === 204) return;
  if (r.status === 404) throw new Error("No such courier in this tenant.");
  if (r.status === 403) throw new Error("You need the drivers:manage permission.");
  throw new Error(`HTTP ${r.status}`);
}
