/**
 * OmniDeliv courier payouts — the money side of field-ops.
 *
 * The Finance page showed merchant invoices from the payments service and
 * nothing else, so a courier's earnings, the cash they were holding, and the
 * payout batch itself were all invisible to ops. `POST /admin/payouts/run` had
 * existed with no caller anywhere: the only way to pay a courier was curl.
 *
 * The preview and the run share one rule server-side (`payout_disposition`), so
 * this client never re-derives who gets paid — a screen that disagrees with the
 * money rail about that is worse than a screen showing nothing.
 */
import { authFetch } from "@/lib/auth/auth-fetch";

const BASE = process.env.NEXT_PUBLIC_API_BASE ?? "";

/** Why a courier is or is not in the next batch. Decided server-side. */
export type Disposition = "pay" | "holding_cash" | "nothing_owed";

export interface PayoutPreviewRow {
  courier_id:      string;
  balance_cents:   number;
  cash_held_cents: number;
  disposition:     Disposition;
  payable_cents:   number;
}

export interface PayoutPreview {
  period:        string;
  rows:          PayoutPreviewRow[];
  payable_cents: number;
}

export interface PayoutRunResult {
  period:                string;
  batch:                 string;
  paid_cents:            number;
  paid:                  [string, number][];
  skipped_holding_cash:  [string, number][];
  skipped_nothing_owed:  string[];
  /** The ledger write failed. Unpaid, and must be retried. */
  failed:                string[];
}

async function okJson(r: Response) {
  const j = await r.json().catch(() => ({}));
  if (!r.ok) {
    if (r.status === 403) throw new Error("You need the payments:read permission.");
    throw new Error(j?.error?.message ?? j?.message ?? `HTTP ${r.status}`);
  }
  return j;
}

export async function fetchPayoutPreview(period?: string): Promise<PayoutPreview> {
  const q = period ? `?period=${encodeURIComponent(period)}` : "";
  return okJson(await authFetch(`${BASE}/v1/field-ops/admin/payouts/preview${q}`));
}

/**
 * Run the batch. Irreversible: it writes payout entries to courier ledgers.
 *
 * `period` is sent explicitly rather than letting the server default, so the
 * batch that runs is the one the operator was looking at — the preview and the
 * run must not straddle a week boundary between the two clicks.
 */
export async function runPayout(period: string): Promise<PayoutRunResult> {
  return okJson(
    await authFetch(`${BASE}/v1/field-ops/admin/payouts/run`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ period }),
    }),
  );
}
