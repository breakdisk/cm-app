"use client";
/**
 * Partner Portal — Settings
 * Read-only carrier profile + SLA commitment sourced from GET /v1/carriers/:id.
 *
 * Coverage zones, service capabilities, and contact edits are deferred until
 * the backend exposes PUT /v1/carriers/:id (currently ops-managed). The zone
 * table below is rendered from rate_cards.coverage_zones when available so
 * partners can at least see what's configured for them.
 */
import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { RefreshCw } from "lucide-react";
import { variants } from "@/lib/design-system/tokens";
import { carriersApi, fmtPhp, type Carrier } from "@/lib/api/carriers";
import { getCurrentPartnerId } from "@/lib/api/partner-identity";

function gradeBadge(grade: string): "green" | "cyan" | "amber" | "red" {
  switch (grade) {
    case "excellent": return "green";
    case "good":      return "cyan";
    case "fair":      return "amber";
    default:          return "red";
  }
}

function statusBadge(status: string): "green" | "amber" | "red" | "muted" {
  switch (status) {
    case "active":               return "green";
    case "pending_verification": return "amber";
    case "suspended":            return "red";
    default:                     return "muted";
  }
}

export default function PartnerSettingsPage() {
  const [carrier, setCarrier] = useState<Carrier | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const c = await carriersApi.get(getCurrentPartnerId());
      setCarrier(c);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load carrier profile");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  // Flatten all coverage_zones across rate cards; dedupe.
  const coverageZones: string[] = carrier
    ? Array.from(new Set(carrier.rate_cards.flatMap((r) => r.coverage_zones)))
    : [];

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="p-6 space-y-6"
    >
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-white font-space-grotesk">Settings</h1>
          <p className="text-white/40 text-sm mt-1">Carrier profile and SLA commitment (read-only — contact ops to update)</p>
        </div>
        <button
          onClick={load}
          className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
          title="Refresh"
        >
          <RefreshCw size={12} />
        </button>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="sm">
            <p className="text-xs text-red-signal font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {loading && !carrier ? (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="sm">
            <p className="text-xs text-white/40 font-mono text-center py-6">loading carrier profile…</p>
          </GlassCard>
        </motion.div>
      ) : carrier ? (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Carrier Profile */}
          <motion.div variants={variants.fadeInUp}>
            <GlassCard title="Carrier Profile">
              <div className="space-y-4">
                {[
                  { label: "Carrier Name", value: carrier.name                                           },
                  { label: "Code",         value: carrier.code,                              mono: true  },
                  { label: "Email",        value: carrier.contact_email                                  },
                  { label: "Phone",        value: carrier.contact_phone ?? "—"                           },
                  { label: "API Endpoint", value: carrier.api_endpoint ?? "Not integrated", mono: true  },
                  { label: "Grade",        value: carrier.performance_grade, badge: gradeBadge(carrier.performance_grade) },
                  { label: "Status",       value: carrier.status,            badge: statusBadge(carrier.status) },
                ].map((row) => (
                  <div key={row.label} className="flex justify-between items-center py-2 border-b border-white/[0.06]">
                    <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{row.label}</span>
                    {row.badge ? (
                      <NeonBadge variant={row.badge}>{row.value}</NeonBadge>
                    ) : (
                      <span className={`text-sm text-white ${row.mono ? "font-mono text-white/70" : ""} truncate max-w-[220px]`} title={row.value}>
                        {row.value}
                      </span>
                    )}
                  </div>
                ))}
              </div>
            </GlassCard>
          </motion.div>

          {/* SLA Commitment */}
          <motion.div variants={variants.fadeInUp}>
            <GlassCard title="SLA Commitment">
              <div className="space-y-4">
                {[
                  { label: "On-Time Target",     value: `${carrier.sla.on_time_target_pct.toFixed(1)}%` },
                  { label: "Max Delivery Days",  value: `${carrier.sla.max_delivery_days} day${carrier.sla.max_delivery_days === 1 ? "" : "s"}` },
                  { label: "Breach Penalty",     value: carrier.sla.penalty_per_breach > 0 ? fmtPhp(carrier.sla.penalty_per_breach) : "None" },
                  { label: "Total Shipments",    value: carrier.total_shipments.toLocaleString() },
                  { label: "On-Time Completed",  value: carrier.on_time_count.toLocaleString() },
                  { label: "Failed",             value: carrier.failed_count.toLocaleString() },
                ].map((row) => (
                  <div key={row.label} className="flex justify-between items-center py-2 border-b border-white/[0.06]">
                    <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{row.label}</span>
                    <span className="text-sm text-white font-mono">{row.value}</span>
                  </div>
                ))}
                <p className="text-2xs text-white/30 font-mono pt-2">
                  Onboarded {new Date(carrier.onboarded_at).toLocaleDateString()} · updated {new Date(carrier.updated_at).toLocaleDateString()}
                </p>
              </div>
            </GlassCard>
          </motion.div>

          {/* Coverage Zones */}
          <motion.div variants={variants.fadeInUp} className="lg:col-span-2">
            <GlassCard title="Coverage Zones">
              {coverageZones.length === 0 ? (
                <p className="text-xs text-white/40 font-mono py-2">
                  No zones configured across your rate cards. Contact ops to add coverage.
                </p>
              ) : (
                <div className="flex flex-wrap gap-2">
                  {coverageZones.map((z) => (
                    <NeonBadge key={z} variant="cyan">{z}</NeonBadge>
                  ))}
                </div>
              )}
            </GlassCard>
          </motion.div>
        </div>
      ) : null}
    </motion.div>
  );
}
