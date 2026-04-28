"use client";
/**
 * Admin Portal — Hub Operations Page
 *
 * LIVE:  Hub cards — GET /v1/hubs (hub-ops service). Capacity tier is
 *        derived client-side from current_load / capacity.
 * LIVE:  KPI totals — computed from the hub list (not a backend call).
 * STATIC: Hourly throughput chart + dock schedule — hub-ops doesn't
 *        yet expose time-bucketed throughput or dock scheduling. Left
 *        as placeholders with 0-valued charts when no data; replace
 *        with real endpoints when those ship.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from "recharts";
import { Building2, Package, Truck, AlertTriangle, FileText, RefreshCw } from "lucide-react";
import { createHubsApi, hubIdOf, hubUtilization, hubStatusTier, type Hub, type HubStatusTier } from "@/lib/api/hubs";

const STATUS_CONFIG: Record<HubStatusTier, { label: string; variant: "green" | "amber" | "red"; color: string }> = {
  normal:   { label: "Normal",   variant: "green", color: "#00FF88" },
  high:     { label: "High",     variant: "amber", color: "#FFAB00" },
  critical: { label: "Critical", variant: "red",   color: "#FF3B5C" },
};

// Hourly throughput chart — no backend endpoint exists for this yet.
// Renders as flat zeros with a "no data" note below when the field is unavailable.
const HOURLY_THROUGHPUT_STATIC = [
  { hour: "6AM", sorted: 0, inducted: 0 },
  { hour: "8AM", sorted: 0, inducted: 0 },
  { hour: "10AM", sorted: 0, inducted: 0 },
  { hour: "12PM", sorted: 0, inducted: 0 },
  { hour: "2PM", sorted: 0, inducted: 0 },
];

export default function HubOpsPage() {
  const api = useMemo(() => createHubsApi(), []);

  const [hubs, setHubs]       = useState<Hub[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const list = await api.list();
      setHubs(list);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load hubs");
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => { load(); }, [load]);

  const kpis = useMemo(() => {
    const inHub     = hubs.reduce((n, h) => n + h.current_load, 0);
    const totalCap  = hubs.reduce((n, h) => n + h.capacity, 0);
    const avgUtil   = hubs.length === 0
      ? 0
      : hubs.reduce((n, h) => n + hubUtilization(h), 0) / hubs.length;
    const critical  = hubs.filter((h) => hubStatusTier(h) === "critical").length;
    return [
      { label: "Parcels In Hub",    value: inHub,    trend: 0, color: "cyan"   as const, format: "number"  as const },
      { label: "Total Capacity",    value: totalCap, trend: 0, color: "green"  as const, format: "number"  as const },
      { label: "Avg Utilization",   value: avgUtil,  trend: 0, color: "purple" as const, format: "percent" as const },
      { label: "Critical Hubs",     value: critical, trend: 0, color: "red"    as const, format: "number"  as const },
    ];
  }, [hubs]);

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
            <Building2 size={22} className="text-purple-plasma" />
            Hub Operations
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {loading ? "loading…" : `${hubs.length} hub${hubs.length === 1 ? "" : "s"}`} · live capacity
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={load}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            title="Refresh"
          >
            <RefreshCw size={12} />
          </button>
          <NeonBadge variant="green" dot>Live</NeonBadge>
        </div>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <p className="text-xs text-red-signal font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {kpis.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Hub capacity grid */}
      {loading && hubs.length === 0 ? (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="text-center py-10">
            <p className="text-xs text-white/40 font-mono">loading hubs…</p>
          </GlassCard>
        </motion.div>
      ) : hubs.length === 0 ? (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="text-center py-10">
            <p className="text-sm text-white/60 font-mono">No hubs configured yet.</p>
            <p className="text-xs text-white/30 font-mono mt-1">Create a hub via POST /v1/hubs or contact ops.</p>
          </GlassCard>
        </motion.div>
      ) : (
        <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 lg:grid-cols-2 xl:grid-cols-3">
          {hubs.map((hub) => {
            const tier = hubStatusTier(hub);
            const { label, variant, color } = STATUS_CONFIG[tier];
            const capacityPct = Math.round(hubUtilization(hub));
            const id = hubIdOf(hub);
            const shortId = id.slice(0, 8);
            return (
              <GlassCard
                key={id}
                glow={tier === "critical" ? "red" : tier === "high" ? "amber" : undefined}
                className="cursor-pointer hover:border-glass-border-bright transition-all"
              >
                <div className="flex items-start justify-between mb-3">
                  <div className="min-w-0">
                    <p className="text-sm font-semibold text-white truncate">{hub.name}</p>
                    <p className="text-2xs font-mono text-white/30 mt-0.5 truncate" title={hub.address}>
                      {shortId} · {hub.serving_zones.length} zones · {hub.address}
                    </p>
                  </div>
                  <NeonBadge variant={variant} dot={tier !== "normal"}>{label}</NeonBadge>
                </div>

                {/* Capacity bar */}
                <div className="mb-3">
                  <div className="flex items-center justify-between mb-1.5">
                    <div className="flex items-center gap-1 text-2xs font-mono text-white/40">
                      <Package size={10} /> {hub.current_load.toLocaleString()} / {hub.capacity.toLocaleString()} parcels
                    </div>
                    <span className="text-2xs font-mono font-bold" style={{ color }}>{capacityPct}%</span>
                  </div>
                  <div className="h-2 rounded-full bg-glass-300 overflow-hidden">
                    <div className="h-full rounded-full transition-all" style={{ width: `${Math.min(capacityPct, 100)}%`, background: color }} />
                  </div>
                </div>

                {/* Status row */}
                <div className="flex items-center gap-2 flex-wrap">
                  <Truck size={11} className="text-white/40" />
                  <span className="text-2xs font-mono text-white/40">
                    {hub.is_active ? "Active" : "Inactive"}
                  </span>
                  {tier === "critical" && (
                    <div className="flex items-center gap-1 text-2xs font-mono text-red-signal">
                      <AlertTriangle size={10} /> Near capacity
                    </div>
                  )}
                  {/* Cross-portal: manifests (daily pickup/delivery lists) live in partner-portal */}
                  <a
                    href={`/partner/manifests?hub=${encodeURIComponent(id)}`}
                    title="Open hub manifests in Partner Portal"
                    onClick={(e) => e.stopPropagation()}
                    className="ml-auto inline-flex items-center gap-1 rounded-md border border-glass-border bg-glass-100 px-1.5 py-0.5 text-2xs font-mono text-white/50 hover:border-cyan-neon/40 hover:text-cyan-neon transition-colors"
                  >
                    <FileText size={9} /> Manifests
                  </a>
                </div>
              </GlassCard>
            );
          })}
        </motion.div>
      )}

      {/* Hourly throughput — placeholder, no backend endpoint yet */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="purple">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Sorting Throughput — Today</h2>
              <p className="text-2xs font-mono text-white/30">
                Per-hour induction + sort counts (hub-ops endpoint pending)
              </p>
            </div>
            <NeonBadge variant="muted">Not yet wired</NeonBadge>
          </div>
          <ResponsiveContainer width="100%" height={160}>
            <BarChart data={HOURLY_THROUGHPUT_STATIC} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="hour" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }} />
              <Bar dataKey="inducted" fill="#A855F7" radius={[4, 4, 0, 0]} fillOpacity={0.5} />
              <Bar dataKey="sorted"   fill="#00E5FF" radius={[4, 4, 0, 0]} fillOpacity={0.5} />
            </BarChart>
          </ResponsiveContainer>
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
