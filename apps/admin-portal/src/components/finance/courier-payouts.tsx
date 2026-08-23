"use client";
/**
 * OmniDeliv courier payouts, on the Finance page.
 *
 * Finance covered merchant invoices and nothing else, so the money owed to
 * couriers — and the cash they were holding on the platform's behalf — was
 * invisible to ops. `POST /admin/payouts/run` had no caller anywhere.
 *
 * Preview first, then run. Ops was otherwise firing an irreversible money batch
 * with no idea what it would do, and the most common outcome is the least
 * obvious one: a courier who is owed money but still holding COD cash is
 * skipped, every run, until they remit.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { Banknote, RefreshCw, AlertTriangle, Wallet } from "lucide-react";
import { toast } from "sonner";

import { GlassCard } from "@/components/ui/glass-card";
import {
  fetchPayoutPreview,
  runPayout,
  type Disposition,
  type PayoutPreview,
} from "@/lib/api/courier-payouts";

function peso(cents: number): string {
  return `₱${(cents / 100).toFixed(2)}`;
}

/**
 * Never colour alone — the reason is the point.
 *
 * "Holding cash" is the one an operator has to act on: it is not a failure, it
 * is a courier who owes the platform money, and the fix is a remittance rather
 * than a retry.
 */
function DispositionCell({ d, held }: { d: Disposition; held: number }) {
  if (d === "pay") return <span className="text-[12px] text-emerald-300">will be paid</span>;
  if (d === "holding_cash") {
    return (
      <span className="text-[12px] text-amber-300">
        holding {peso(held)} — pays after they remit
      </span>
    );
  }
  return <span className="text-[12px] text-white/40">nothing owed</span>;
}

export function CourierPayouts() {
  const [preview, setPreview] = useState<PayoutPreview | null>(null);
  const [loading, setLoading] = useState(true);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setPreview(await fetchPayoutPreview());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load the payout preview");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const counts = useMemo(() => {
    const rows = preview?.rows ?? [];
    return {
      pay: rows.filter((r) => r.disposition === "pay").length,
      holding: rows.filter((r) => r.disposition === "holding_cash").length,
      heldCents: rows.reduce((n, r) => n + r.cash_held_cents, 0),
    };
  }, [preview]);

  async function run() {
    if (!preview) return;
    setRunning(true);
    const tid = toast.loading("Running the payout batch…");
    try {
      // The period the operator was looking at, not whatever the server would
      // default to — the two clicks must not straddle a week boundary.
      const result = await runPayout(preview.period);
      const failed = result.failed.length;
      toast[failed ? "warning" : "success"](
        `Paid ${peso(result.paid_cents)} to ${result.paid.length} courier(s)` +
          (failed ? ` · ${failed} ledger write(s) failed and must be retried` : ""),
        { id: tid },
      );
      await load();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "The batch did not run", { id: tid });
    } finally {
      setRunning(false);
    }
  }

  return (
    <GlassCard className="p-0">
      <div className="flex items-start justify-between gap-4 border-b border-white/5 p-4">
        <div>
          <h2 className="flex items-center gap-2 text-[15px] font-semibold text-white">
            <Banknote className="h-4 w-4 text-cyan-300" /> OmniDeliv courier payouts
          </h2>
          <p className="mt-1 text-[12px] text-white/50">
            Period {preview?.period ?? "—"} · gig couriers paid per job from a weekly ledger.
            Separate from merchant invoices above.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => void load()}
            className="flex items-center gap-2 rounded-lg bg-white/5 px-3 py-2 text-[12px] text-white/70 hover:bg-white/10"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} /> Refresh
          </button>
          <button
            onClick={() => void run()}
            disabled={running || loading || !preview || preview.payable_cents <= 0}
            className="rounded-lg bg-emerald-500/15 px-3 py-2 text-[12px] font-medium text-emerald-300 hover:bg-emerald-500/25 disabled:opacity-40"
          >
            {running ? "Running…" : `Pay ${peso(preview?.payable_cents ?? 0)}`}
          </button>
        </div>
      </div>

      <div className="grid grid-cols-3 gap-3 p-4">
        <div>
          <div className="text-[11px] uppercase tracking-wide text-white/40">To be paid</div>
          <div className="mt-1 text-xl font-semibold text-emerald-300">
            {peso(preview?.payable_cents ?? 0)}
          </div>
          <div className="text-[11px] text-white/40">{counts.pay} courier(s)</div>
        </div>
        <div>
          <div className="text-[11px] uppercase tracking-wide text-white/40">Cash held by couriers</div>
          <div className="mt-1 text-xl font-semibold text-amber-300">{peso(counts.heldCents)}</div>
          <div className="text-[11px] text-white/40">{counts.holding} blocked until remitted</div>
        </div>
        <div>
          <div className="text-[11px] uppercase tracking-wide text-white/40">Open ledgers</div>
          <div className="mt-1 text-xl font-semibold text-white">{preview?.rows.length ?? 0}</div>
        </div>
      </div>

      {error && (
        <div className="flex items-center gap-2 border-t border-white/5 p-4 text-[12px] text-amber-300">
          <AlertTriangle className="h-4 w-4" /> {error}
        </div>
      )}

      <div className="overflow-x-auto border-t border-white/5">
        <table className="w-full min-w-[620px] text-left text-[13px]">
          <thead className="bg-white/[0.03] text-[11px] uppercase tracking-wide text-white/40">
            <tr>
              <th className="px-4 py-3">Courier</th>
              <th className="px-4 py-3 text-right">Balance</th>
              <th className="px-4 py-3 text-right">Cash held</th>
              <th className="px-4 py-3">Next run</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-white/5">
            {loading && !preview && (
              <tr><td colSpan={4} className="px-4 py-8 text-center text-white/40">Loading…</td></tr>
            )}
            {preview?.rows.length === 0 && !loading && (
              <tr>
                <td colSpan={4} className="px-4 py-8 text-center text-white/40">
                  <Wallet className="mx-auto mb-2 h-5 w-5 opacity-40" />
                  No open courier ledgers this period.
                </td>
              </tr>
            )}
            {preview?.rows.map((r) => (
              <tr key={r.courier_id}>
                <td className="px-4 py-3 font-mono text-[11px] text-white/60">
                  {r.courier_id.slice(0, 8)}
                </td>
                <td className="px-4 py-3 text-right font-mono text-white">
                  {peso(r.balance_cents)}
                </td>
                <td className="px-4 py-3 text-right font-mono text-amber-300">
                  {r.cash_held_cents > 0 ? peso(r.cash_held_cents) : "—"}
                </td>
                <td className="px-4 py-3">
                  <DispositionCell d={r.disposition} held={r.cash_held_cents} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </GlassCard>
  );
}
