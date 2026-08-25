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
 * has four independent answers that look identical from the outside: suspended
 * by ops, off duty, a GPS fix older than the ten minutes the proximity search
 * will consider, and now compliance.
 *
 * Compliance ships in observe-only mode, so `dispatchable` and
 * `compliance_assignable` can disagree — a courier compliance has refused is
 * still being offered work. That disagreement is not a bug to paper over; it is
 * the rollout, and this screen is where anyone can see what enforcing it would
 * cost before the flag is turned on.
 *
 * A further answer turned out to be "the server did not say". The deployed
 * field-ops predated the compliance gate and sent none of the three compliance
 * fields, so this page threw on every row, and before it threw it reported an
 * offline courier as an on-duty one with a stale GPS fix. The decisions now live
 * in `lib/couriers/compliance-view.ts`, tested against that exact payload —
 * portal and service are separate deploy units and the skew is structural, not
 * a one-off missed `docker compose pull`.
 *
 * The Compliance cell links into the compliance console rather than growing a
 * review UI here. It was a dead end otherwise: this roster holds `user_id` and
 * has never held a `compliance_profile_id`, so it could state that a courier was
 * outstanding and offer no way to reach the documents — and the console could
 * not show them either, because its queue lists documents awaiting a decision
 * and a courier who has submitted nothing has none. One implementation of
 * approve/reject, two ways in.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { motion } from "framer-motion";
import { Bike, RefreshCw, ShieldOff, ShieldCheck, AlertTriangle, FileText } from "lucide-react";
import { toast } from "sonner";

import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { fetchCouriers, setCourierActive, type AdminCourier } from "@/lib/api/couriers";
import { complianceView, dispatchView, courierCounts } from "@/lib/couriers/compliance-view";

function DutyPill({ status }: { status: string }) {
  const base = "rounded-full px-2 py-0.5 text-[11px] font-medium";
  if (status === "available") return <span className={`${base} bg-emerald-400/10 text-emerald-300`}>on duty</span>;
  if (status === "assigned") return <span className={`${base} bg-cyan-400/10 text-cyan-300`}>on a job</span>;
  if (status === "on_break") return <span className={`${base} bg-amber-400/10 text-amber-300`}>on break</span>;
  return <span className={`${base} bg-white/5 text-white/50`}>off duty</span>;
}

/**
 * The Compliance column and the Dispatchable column, both rendered from
 * `lib/couriers/compliance-view.ts`.
 *
 * The decisions moved out of this file after they were found to be wrong
 * against a live payload. They are consequential enough — "why is this person
 * not getting jobs?" — to need tests, and a decision written inline in JSX
 * cannot have any.
 */
function CompliancePill({ c }: { c: AdminCourier }) {
  const v = complianceView(c);
  const pill = (
    <span className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${v.tone}`} title={v.title}>
      {v.label}
    </span>
  );

  // Nothing to open when this deployment reports no compliance at all — a link
  // would promise documents that cannot exist to be found.
  if (v.kind === "unsupported") return pill;

  // `user_id`, not `id`. `entity_id` on a compliance profile is the identity
  // user on both creation paths — `claims.user_id` on the lazy `/me` route, and
  // the id field-ops puts in `driver.registered`. ADR-0015 makes the two equal
  // for anyone registered since, and depending on that here would break
  // silently for every courier who predates it.
  return (
    <Link
      href={`/compliance?entity=${c.user_id}`}
      className="inline-flex items-center gap-1 rounded-full transition-opacity hover:opacity-80 focus:outline-none focus:ring-1 focus:ring-cyan-300/40"
      title={
        v.kind === "not-onboarded"
          ? "Open in the compliance console. No profile exists yet — the console says so and does not treat it as an error."
          : "Open this courier's documents in the compliance console"
      }
    >
      {pill}
      <FileText className="h-3 w-3 text-white/30" />
    </Link>
  );
}

function DispatchCell({ c, nowMs }: { c: AdminCourier; nowMs: number }) {
  const v = dispatchView(c, nowMs);
  return <span className={`text-[12px] ${v.tone}`} title={v.title}>{v.label}</span>;
}

export default function CouriersPage() {
  const [couriers, setCouriers] = useState<AdminCourier[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  /**
   * The clock the Dispatchable column reasons against, stamped when the roster
   * arrives rather than read during render. `dispatchView` takes it as an
   * argument so the GPS-age branch is testable at all; re-reading `Date.now()`
   * per render would also make rows disagree with the data they came from.
   */
  const [nowMs, setNowMs] = useState<number>(() => Date.now());

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setCouriers(await fetchCouriers());
      setNowMs(Date.now());
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

  const counts = useMemo(() => courierCounts(couriers), [couriers]);

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
    <motion.div
      variants={variants.fadeInUp}
      initial="hidden"
      animate="visible"
      className="space-y-5 p-4 sm:p-6"
    >
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

      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
        {/*
          A `null` count renders as an em dash, never as `0`. Zero is a claim,
          and the claim it makes — nobody is blocked, nobody needs onboarding —
          is one a field-ops without the compliance fields cannot support. The
          dash also makes a lagging backend visible on the screen instead of
          silently confident.
        */}
        {([
          ["Couriers", counts.total, "text-white", undefined],
          ["Receiving offers", counts.dispatchable, "text-emerald-300", undefined],
          ["Suspended", counts.suspended, "text-rose-300", undefined],
          ["Compliance blocked", counts.complianceBlocked, "text-amber-300",
            "This deployment's field-ops reports nothing about compliance, so this cannot be counted."],
          ["Not onboarded", counts.notOnboarded, "text-white/60",
            "This deployment's field-ops reports nothing about compliance, so this cannot be counted."],
        ] as [string, number | null, string, string | undefined][]).map(([label, value, tone, title]) => (
          <GlassCard key={label} className="p-4">
            <div className="text-[11px] uppercase tracking-wide text-white/40">{label}</div>
            <div
              className={`mt-1 text-2xl font-semibold ${value === null ? "text-white/25" : tone}`}
              title={value === null ? title : undefined}
            >
              {value === null ? "—" : value}
            </div>
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
          <table className="w-full min-w-[980px] text-left text-[13px]">
            <thead className="bg-white/[0.03] text-[11px] uppercase tracking-wide text-white/40">
              <tr>
                <th className="px-4 py-3">Courier</th>
                <th className="px-4 py-3">Phone</th>
                <th className="px-4 py-3">Duty</th>
                <th className="px-4 py-3">Dispatchable</th>
                <th className="px-4 py-3">Compliance</th>
                <th className="px-4 py-3">Zone</th>
                <th className="px-4 py-3 text-right">Ops</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-white/5">
              {loading && couriers.length === 0 && (
                <tr><td colSpan={7} className="px-4 py-8 text-center text-white/40">Loading…</td></tr>
              )}
              {!loading && couriers.length === 0 && !error && (
                <tr>
                  <td colSpan={7} className="px-4 py-8 text-center text-white/40">
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
                  <td className="px-4 py-3"><DispatchCell c={c} nowMs={nowMs} /></td>
                  <td className="px-4 py-3"><CompliancePill c={c} /></td>
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
