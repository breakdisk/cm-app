"use client";
/**
 * Admin Portal — OmniDeliv vendor review.
 *
 * A store applies from the merchant portal and lands in `onboarding`. Nothing
 * customer-facing shows it until an operator approves it here: `find_near`,
 * which is what puts a shop in front of a customer, returns active stores only.
 *
 * This page is the missing half of that flow. The approve endpoint existed and
 * the apply endpoint existed; neither had a caller, so every vendor on the
 * platform had been inserted by hand in SQL.
 */
import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { Store, RefreshCw, CheckCircle2, AlertTriangle } from "lucide-react";
import { toast } from "sonner";

import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { fetchVendors, approveVendor, type AdminVendor } from "@/lib/api/vendors";

function statusPill(status: string) {
  const base = "rounded-full px-2 py-0.5 text-[11px] font-medium";
  if (status === "active") {
    return <span className={`${base} bg-emerald-400/10 text-emerald-300`}>active</span>;
  }
  if (status === "onboarding") {
    return <span className={`${base} bg-amber-400/10 text-amber-300`}>awaiting review</span>;
  }
  return <span className={`${base} bg-white/5 text-white/50`}>{status}</span>;
}

export default function VendorsPage() {
  const [vendors, setVendors] = useState<AdminVendor[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setVendors(await fetchVendors());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "could not load vendors");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function approve(v: AdminVendor) {
    setBusy(v.id);
    try {
      await approveVendor(v.id);
      toast.success(`${v.name} is now live to customers`);
      await load();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "approval failed");
    } finally {
      setBusy(null);
    }
  }

  const pending = vendors.filter((v) => v.status === "onboarding");

  return (
    <motion.div
      variants={variants.fadeIn}
      initial="hidden"
      animate="visible"
      className="space-y-5 p-4 sm:p-6"
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0">
          <h1 className="font-heading text-lg font-semibold text-white">OmniDeliv Vendors</h1>
          <p className="mt-1 text-sm text-white/50">
            {pending.length === 0
              ? "Nothing is waiting for review."
              : `${pending.length} store${pending.length === 1 ? "" : "s"} waiting for review.`}
          </p>
        </div>
        <button
          type="button"
          onClick={() => void load()}
          className="inline-flex items-center gap-2 self-start rounded-lg border border-white/10 px-3 py-1.5 text-xs text-white/70 hover:bg-white/5"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          Refresh
        </button>
      </div>

      {error && (
        <GlassCard className="p-4">
          <p className="text-sm text-rose-300">{error}</p>
        </GlassCard>
      )}

      {loading ? (
        <GlassCard className="p-8 text-center">
          <p className="text-sm text-white/40">Loading…</p>
        </GlassCard>
      ) : vendors.length === 0 ? (
        <GlassCard className="p-8 text-center">
          <Store className="mx-auto mb-3 h-8 w-8 text-white/30" />
          <p className="text-white/60">No OmniDeliv stores in this tenant yet.</p>
          <p className="mt-2 text-xs text-white/40">
            Merchants apply from their own portal — the &ldquo;Sell on OmniDeliv&rdquo; entry.
          </p>
        </GlassCard>
      ) : (
        <GlassCard padding="none">
          <div className="divide-y divide-white/5">
            {vendors.map((v) => (
              <div key={v.id} className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="truncate font-medium text-white">{v.name}</span>
                    {statusPill(v.status)}
                    <span className="text-[11px] uppercase tracking-wide text-white/30">{v.vertical}</span>
                  </div>
                  <p className="mt-1 truncate text-xs text-white/45">{v.address}</p>
                  {!v.has_owner && (
                    <p className="mt-1.5 inline-flex items-center gap-1.5 text-xs text-amber-300/90">
                      <AlertTriangle className="h-3.5 w-3.5" />
                      No login owns this store — nobody can edit its catalog.
                    </p>
                  )}
                </div>

                {v.status === "onboarding" ? (
                  <button
                    type="button"
                    onClick={() => void approve(v)}
                    disabled={busy === v.id}
                    className="inline-flex shrink-0 items-center gap-2 self-start rounded-lg bg-emerald-500/90 px-3 py-1.5 text-xs font-medium text-[#04140d] hover:bg-emerald-400 disabled:opacity-40 sm:self-auto"
                  >
                    <CheckCircle2 className="h-3.5 w-3.5" />
                    {busy === v.id ? "Approving…" : "Approve"}
                  </button>
                ) : (
                  <span className="shrink-0 text-xs text-white/30">
                    live since {new Date(v.created_at).toLocaleDateString()}
                  </span>
                )}
              </div>
            ))}
          </div>
        </GlassCard>
      )}
    </motion.div>
  );
}
