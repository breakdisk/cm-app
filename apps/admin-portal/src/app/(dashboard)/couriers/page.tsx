"use client";
/**
 * Admin Portal — OmniDeliv couriers.
 *
 * The missing management surface. Couriers live in `field-ops` and nothing in
 * any portal touched that service: no list, no suspend, no way to see why
 * somebody was not being offered work. The only lever was SQL.
 *
 * Deliberately a separate page from `/drivers`, not a tab on it. Those are
 * driver-ops drivers — employed, carrier-linked, running routes. A courier is
 * the platform tier's gig worker (ADR-0015). Merging the two screens would
 * imply an identity relationship that does not exist in either schema.
 *
 * The column that earns its place is **Dispatchable**, because it is the
 * question ops actually asks — "why is this person not getting jobs?" — and it
 * has two independent answers that look identical from the outside.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { Bike, RefreshCw, ShieldOff, ShieldCheck, AlertTriangle } from "lucide-react";
import { toast } from "sonner";

import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { fetchCouriers, setCourierActive, type AdminCourier } from "@/lib/api/couriers";

/** Minutes since the last GPS fix, or null when there has never been one. */
function fixAgeMinutes(iso: string | null): number | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  return Math.floor((Date.now() - t) / 60_000);
}

function DutyPill({ status }: { status: string }) {
  const base = "rounded-full px-2 py-0.5 text-[11px] font-medium";
  if (status === "available") return <span className={`${base} bg-emerald-400/10 text-emerald-300`}>on duty</span>;
  if (status === "assigned") return <span className={`${base} bg-cyan-400/10 text-cyan-300`}>on a job</span>;
  if (status === "on_break") return <span className={`${base} bg-amber-400/10 text-amber-300`}>on break</span>;
  return <span className={`${base} bg-white/5 text-white/50`}>off duty</span>;
}

/**
 * Why this courier is or is not being offered work.
 *
 * Never colour alone, and never just "no": the whole point of the column is to
 * say *which* of the three independent reasons applies, because ops cannot tell
 * them apart from the courier's complaint.
 */
function DispatchCell({ c }: { c: AdminCourier }) {
  const age = fixAgeMinutes(c.last_seen_at);

  if (!c.is_active) {
    return <span className="text-[12px] text-rose-300">suspended by ops</span>;
  }
  if (c.status !== "available") {
    return <span className="text-[12px] text-white/40">not on duty</span>;
  }
  // The third reason, and the one nobody guesses: the proximity search only
  // considers a fix from the last ten minutes, so a courier who is on duty and
  // active is still invisible if their phone stopped reporting.
  if (age === null) {
    return <span className="text-[12px] text-amber-300">on duty · never sent a position</span>;
  }
  if (age > 10) {
    return <span className="text-[12px] text-amber-300">on duty · last fix {age}m ago (stale)</span>;
  }
  return <span className="text-[12px] text-emerald-300">receiving offers</span>;
}

export default function CouriersPage() {
  const [couriers, setCouriers] = useState<AdminCourier[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setCouriers(await fetchCouriers());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load couriers");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const counts = useMemo(() => ({
    total: couriers.length,
    dispatchable: couriers.filter((c) => c.dispatchable).length,
    suspended: couriers.filter((c) => !c.is_active).length,
  }), [couriers]);

  async function toggle(c: AdminCourier) {
    const next = !c.is_active;
    setBusy(c.id);
    const tid = toast.loading(next ? "Reinstating courier…" : "Suspending courier…");
    try {
      await setCourierActive(c.id, next);
      // Refetch rather than patching in place: `dispatchable` is computed
      // server-side from two flags, and recomputing it here is how the two
      // definitions drift.
      await load();
      toast.success(next ? "Courier reinstated" : "Courier suspended", { id: tid });
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "That did not work", { id: tid });
    } finally {
      setBusy(null);
    }
  }

  return (
    <motion.div {...variants.fadeUp} className="space-y-5">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h1 className="flex items-center gap-2 text-xl font-semibold text-white">
            <Bike className="h-5 w-5 text-cyan-300" /> OmniDeliv Couriers
          </h1>
          <p className="mt-1 text-[13px] text-white/50">
            Field-ops couriers — gig workers paid per job. Separate from Drivers, which are
            employed driver-ops drivers on routes.
          </p>
        </div>
        <button
          onClick={() => void load()}
          className="flex items-center gap-2 rounded-lg bg-white/5 px-3 py-2 text-[13px] text-white/70 hover:bg-white/10"
        >
          <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} /> Refresh
        </button>
      </header>

      <div className="grid grid-cols-3 gap-3">
        {[
          ["Couriers", counts.total, "text-white"],
          ["Receiving offers", counts.dispatchable, "text-emerald-300"],
          ["Suspended", counts.suspended, "text-rose-300"],
        ].map(([label, value, tone]) => (
          <GlassCard key={String(label)} className="p-4">
            <div className="text-[11px] uppercase tracking-wide text-white/40">{label}</div>
            <div className={`mt-1 text-2xl font-semibold ${tone}`}>{value}</div>
          </GlassCard>
        ))}
      </div>

      {error && (
        <GlassCard className="flex items-center gap-2 p-4 text-[13px] text-amber-300">
          <AlertTriangle className="h-4 w-4" /> {error}
        </GlassCard>
      )}

      <GlassCard className="overflow-hidden p-0">
        <div className="overflow-x-auto">
          <table className="w-full min-w-[860px] text-left text-[13px]">
            <thead className="bg-white/[0.03] text-[11px] uppercase tracking-wide text-white/40">
              <tr>
                <th className="px-4 py-3">Courier</th>
                <th className="px-4 py-3">Phone</th>
                <th className="px-4 py-3">Duty</th>
                <th className="px-4 py-3">Dispatchable</th>
                <th className="px-4 py-3">Zone</th>
                <th className="px-4 py-3 text-right">Ops</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-white/5">
              {loading && couriers.length === 0 && (
                <tr><td colSpan={6} className="px-4 py-8 text-center text-white/40">Loading…</td></tr>
              )}
              {!loading && couriers.length === 0 && !error && (
                <tr>
                  <td colSpan={6} className="px-4 py-8 text-center text-white/40">
                    No couriers yet. They appear here after signing in to the OmniDeliv
                    courier app, which registers them.
                  </td>
                </tr>
              )}
              {couriers.map((c) => (
                <tr key={c.id} className={c.is_active ? "" : "opacity-60"}>
                  <td className="px-4 py-3 text-white">
                    {c.first_name} {c.last_name}
                    <div className="font-mono text-[10px] text-white/30">{c.id.slice(0, 8)}</div>
                  </td>
                  <td className="px-4 py-3 font-mono text-white/60">{c.phone}</td>
                  <td className="px-4 py-3"><DutyPill status={c.status} /></td>
                  <td className="px-4 py-3"><DispatchCell c={c} /></td>
                  <td className="px-4 py-3 text-white/50">{c.zone ?? "—"}</td>
                  <td className="px-4 py-3 text-right">
                    <button
                      disabled={busy === c.id}
                      onClick={() => void toggle(c)}
                      className={`inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[12px] disabled:opacity-40 ${
                        c.is_active
                          ? "bg-rose-500/10 text-rose-300 hover:bg-rose-500/20"
                          : "bg-emerald-500/10 text-emerald-300 hover:bg-emerald-500/20"
                      }`}
                    >
                      {c.is_active
                        ? <><ShieldOff className="h-3.5 w-3.5" /> Suspend</>
                        : <><ShieldCheck className="h-3.5 w-3.5" /> Reinstate</>}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </GlassCard>
    </motion.div>
  );
}
