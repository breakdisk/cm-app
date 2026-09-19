"use client";
/**
 * Onboarding filter: which jobs a driver is offered. Freight moves and
 * whole-home moves are separate service lines; a home move goes only to a lead
 * onboarded for it, with the coverage, trucks and helpers it needs. A
 * multi-truck (Enterprise) lead brings more than one truck and is the only
 * kind offered a Large Estate.
 *
 * Operations set this; the lead keeps their own working days in the driver
 * app, shown here read-only alongside the onboarding.
 */
import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Loader2, Save, X } from "lucide-react";
import { toast } from "sonner";

import { GlassCard } from "@/components/ui/glass-card";
import { createApiClient } from "@/lib/api/client";

export interface ProviderProfile {
  service_lines: string[];
  coverage: string[];
  multi_truck_capable: boolean;
  fleet_trucks: number;
  registered_helpers: number;
  max_daily_jobs: number;
  /** 0 = Monday … 6 = Sunday. */
  working_days: number[];
}

const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** What would stop the profile saving, as the server checks it. */
export function profileProblem(p: ProviderProfile): string | null {
  if (p.service_lines.length === 0) return "Pick at least one job type.";
  if (p.coverage.length === 0) return "Pick local, international, or both.";
  if (!Number.isInteger(p.fleet_trucks) || p.fleet_trucks < 1 || p.fleet_trucks > 20) return "Trucks run 1 to 20.";
  if (!Number.isInteger(p.registered_helpers) || p.registered_helpers < 0 || p.registered_helpers > 60) return "Registered helpers run 0 to 60.";
  if (!Number.isInteger(p.max_daily_jobs) || p.max_daily_jobs < 1 || p.max_daily_jobs > 4) return "Jobs a day run 1 to 4.";
  if (p.multi_truck_capable && p.fleet_trucks < 2) return "A multi-truck lead has at least two trucks.";
  return null;
}

function toggle(list: string[], v: string): string[] {
  return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
}

const box = "flex items-center gap-2 rounded-lg border border-white/10 px-3 py-2 text-sm text-white/80";
const num = "w-full rounded-lg border border-white/10 bg-white/[0.03] px-3 py-2 text-sm text-white focus:border-cyan-400/50 focus:outline-none";

export function ProviderProfileModal({ driverId, driverName, onClose }: { driverId: string; driverName: string; onClose: () => void }) {
  const [profile, setProfile] = useState<ProviderProfile | null>(null);
  const [offDays, setOffDays] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    createApiClient()
      .get<{ data: { profile: ProviderProfile; off_days: string[] } }>(`/v1/drivers/${driverId}/provider`)
      .then((r) => { setProfile(r.data.data.profile); setOffDays(r.data.data.off_days); })
      .catch((e: { message?: string }) => setError(e?.message ?? "Could not load the profile"));
  }, [driverId]);

  const problem = profile ? profileProblem(profile) : null;
  const home = !!profile?.service_lines.includes("home_move");

  async function save() {
    if (!profile || problem) return;
    setSaving(true);
    try {
      await createApiClient().put(`/v1/drivers/${driverId}/provider`, profile);
      toast.success(`${driverName} saved`);
      onClose();
    } catch (e) {
      toast.error((e as { message?: string })?.message ?? "Save failed");
    } finally {
      setSaving(false);
    }
  }

  return (
    <AnimatePresence>
      <motion.div
        className="fixed inset-0 z-50 flex items-end justify-center bg-black/60 p-0 sm:items-center sm:p-4"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        onClick={onClose}
      >
        <motion.div
          className="max-h-[90vh] w-full max-w-lg overflow-y-auto"
          initial={{ y: 24 }}
          animate={{ y: 0 }}
          onClick={(e) => e.stopPropagation()}
        >
          <GlassCard className="space-y-4 p-5">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <h2 className="font-heading text-base font-semibold text-white">Job types</h2>
                <p className="truncate text-xs text-white/45">{driverName}</p>
              </div>
              <button type="button" onClick={onClose} aria-label="Close" className="rounded-lg p-1 text-white/50 hover:bg-white/5">
                <X className="h-4 w-4" />
              </button>
            </div>

            {error && <p className="text-sm text-rose-300">{error}</p>}
            {!profile && !error && <p className="text-sm text-white/40">Loading…</p>}

            {profile && (
              <>
                <section className="space-y-2">
                  <p className="text-[11px] uppercase tracking-wide text-white/40">Offered</p>
                  <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                    <label className={box}>
                      <input type="checkbox" checked={profile.service_lines.includes("freight_move")} onChange={() => setProfile({ ...profile, service_lines: toggle(profile.service_lines, "freight_move") })} />
                      Freight moves
                    </label>
                    <label className={box}>
                      <input type="checkbox" checked={home} onChange={() => setProfile({ ...profile, service_lines: toggle(profile.service_lines, "home_move") })} />
                      Whole-home moves
                    </label>
                  </div>
                </section>

                <section className="space-y-2">
                  <p className="text-[11px] uppercase tracking-wide text-white/40">Coverage</p>
                  <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                    <label className={box}>
                      <input type="checkbox" checked={profile.coverage.includes("local")} onChange={() => setProfile({ ...profile, coverage: toggle(profile.coverage, "local") })} />
                      Local
                    </label>
                    <label className={box}>
                      <input type="checkbox" checked={profile.coverage.includes("international")} onChange={() => setProfile({ ...profile, coverage: toggle(profile.coverage, "international") })} />
                      International
                    </label>
                  </div>
                </section>

                {home && (
                  <section className="space-y-3 rounded-xl border border-white/5 p-3">
                    <p className="text-[11px] uppercase tracking-wide text-white/40">Home-move team</p>
                    <label className={box}>
                      <input type="checkbox" checked={profile.multi_truck_capable} onChange={() => setProfile({ ...profile, multi_truck_capable: !profile.multi_truck_capable })} />
                      Multi-truck (Enterprise) lead — offered Large Estates
                    </label>
                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                      <label className="block text-xs text-white/50">
                        Trucks
                        <input className={num} type="number" min={1} max={20} value={profile.fleet_trucks} onChange={(e) => setProfile({ ...profile, fleet_trucks: Number(e.target.value) })} />
                      </label>
                      <label className="block text-xs text-white/50">
                        Registered helpers
                        <input className={num} type="number" min={0} max={60} value={profile.registered_helpers} onChange={(e) => setProfile({ ...profile, registered_helpers: Number(e.target.value) })} />
                      </label>
                      <label className="block text-xs text-white/50">
                        Jobs a day
                        <input className={num} type="number" min={1} max={4} value={profile.max_daily_jobs} onChange={(e) => setProfile({ ...profile, max_daily_jobs: Number(e.target.value) })} />
                      </label>
                    </div>
                    <p className="text-[11px] text-white/40">
                      A move is offered only to a lead with at least as many helpers as its crew needs, and enough trucks for a multi-truck job.
                    </p>
                  </section>
                )}

                <section className="space-y-1">
                  <p className="text-[11px] uppercase tracking-wide text-white/40">Works (set by the lead)</p>
                  <p className="text-sm text-white/70">{profile.working_days.map((d) => DAYS[d] ?? d).join(", ") || "No days set"}</p>
                  {offDays.length > 0 && <p className="text-xs text-white/40">Off: {offDays.slice(0, 8).join(", ")}{offDays.length > 8 ? "…" : ""}</p>}
                </section>

                {problem && <p className="text-xs text-amber-300">{problem}</p>}
                <div className="flex justify-end gap-2">
                  <button type="button" onClick={onClose} className="rounded-lg border border-white/10 px-3 py-2 text-xs text-white/70 hover:bg-white/5">Cancel</button>
                  <button
                    type="button"
                    disabled={saving || !!problem}
                    onClick={() => void save()}
                    className="inline-flex items-center gap-2 rounded-lg bg-cyan-400/90 px-3 py-2 text-xs font-medium text-[#041a1f] hover:bg-cyan-300 disabled:opacity-40"
                  >
                    {saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Save className="h-3.5 w-3.5" />}
                    Save
                  </button>
                </div>
              </>
            )}
          </GlassCard>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}
