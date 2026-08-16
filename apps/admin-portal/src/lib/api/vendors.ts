/**
 * OmniDeliv vendor review queue.
 *
 * Applying and being approved are deliberately two actions: letting a store
 * list itself would mean anyone with a login can put food in front of
 * customers. Until 2026-08-14 the approve route checked no permission at all,
 * so an applicant could approve their own application — and nothing listed
 * pending vendors, so no operator could have found one to review anyway.
 */
import { authFetch } from "@/lib/auth/auth-fetch";

const BASE = process.env.NEXT_PUBLIC_API_BASE ?? "";

export interface AdminVendor {
  id:         string;
  name:       string;
  address:    string;
  vertical:   string;
  /** `onboarding` until an operator approves; then `active`. */
  status:     string;
  /**
   * False means no login owns this store. `/vendors/me` resolves by user_id,
   * so nobody can manage its catalog — it can be approved and still be a
   * shop no human can edit. Seeded rows arrive this way.
   */
  has_owner:  boolean;
  created_at: string;
}

async function okJson(r: Response) {
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j?.error?.message ?? j?.message ?? `HTTP ${r.status}`);
  return j;
}

export async function fetchVendors(): Promise<AdminVendor[]> {
  const j = await okJson(await authFetch(`${BASE}/v1/omnideliv/admin/vendors`));
  return Array.isArray(j) ? j : (j.data ?? []);
}

export async function approveVendor(id: string): Promise<void> {
  const r = await authFetch(`${BASE}/v1/omnideliv/admin/vendors/${id}/approve`, {
    method: "POST",
  });
  if (!r.ok && r.status !== 204) {
    await okJson(r);
  }
}
