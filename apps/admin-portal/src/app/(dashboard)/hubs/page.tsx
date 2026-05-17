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
import { motion, AnimatePresence } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from "recharts";
import { Building2, Package, Truck, AlertTriangle, FileText, RefreshCw, X, MapPin, Layers, ExternalLink } from "lucide-react";
import { createHubsApi, hubIdOf, hubUtilization, hubStatusTier, type Hub, type HubStatusTier, type HubThroughputBucket } from "@/lib/api/hubs";

const STATUS_CONFIG: Record<HubStatusTier, { label: string; variant: "green" | "amber" | "red"; color: string }> = {
  normal:   { label: "Normal",   variant: "green", color: "#00FF88" },
  high:     { label: "High",     variant: "amber", color: "#FFAB00" },
  critical: { label: "Critical", variant: "red",   color: "#FF3B5C" },
};

const THROUGHPUT_FALLBACK: HubThroughputBucket[] = [
  { hour: "6AM",  sorted: 0, inducted: 0 },
  { hour: "8AM",  sorted: 0, inducted: 0 },
  { hour: "10AM", sorted: 0, inducted: 0 },
  { hour: "12PM", sorted: 0, inducted: 0 },
  { hour: "2PM",  sorted: 0, inducted: 0 },
];

// ── Hub Detail Drawer ─────────────────────────────────────────────────────────
function HubDetailDrawer({ hub, onClose }: { hub: Hub; onClose: () => void }) {
  const id = hubIdOf(hub);
  const tier = hubStatusTier(hub);
  const { label, variant, color } = STATUS_CONFIG[tier];
  const capacityPct = Math.round(hubUtilization(hub));

  return (
    <motion.div
      key="hub-drawer-backdrop"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex justify-end"
      onClick={onClose}
    >
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" />

      {/* Panel */}
      <motion.div
        key="hub-drawer-panel"
        initial={{ x: "100%" }}
        animate={{ x: 0 }}
        exit={{ x: "100%" }}
        transition={{ type: "spring", damping: 28, stiffness: 260 }}
        className="relative w-full max-w-md bg-[#0d1422] border-l border-glass-border flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-start justify-between p-5 border-b border-glass-border">
          <div className="min-w-0">
            <div className="flex items-center gap-2 mb-1">
              <Building2 size={16} className="text-purple-plasma shrink-0" />
              <h2 className="font-heading text-base font-bold text-white truncate">{hub.name}</h2>
            </div>
            <p className="text-2xs font-mono text-white/30 truncate">{id}</p>
          </div>
          <div className="flex items-center gap-2 ml-3 shrink-0">
            <NeonBadge variant={variant} dot={tier !== "normal"}>{label}</NeonBadge>
            <button
              onClick={onClose}
              className="p-1.5 rounded-lg hover:bg-glass-200 text-white/50 hover:text-white transition-colors"
            >
              <X size={14} />
            </button>
          </div>
        </div>

        {/* Scrollable body */}
        <div className="flex-1 overflow-y-auto p-5 space-y-4">
          {/* Address */}
          <div className="flex items-start gap-2 text-sm text-white/60">
            <MapPin size={13} className="text-white/30 mt-0.5 shrink-0" />
            <span className="font-mono text-xs leading-relaxed">{hub.address}</span>
          </div>

          {/* Capacity */}
          <GlassCard size="sm">
            <p className="text-2xs font-mono text-white/40 uppercase tracking-widest mb-2">Capacity</p>
            <div className="flex items-end justify-between mb-2">
              <span className="text-xl font-bold font-mono text-white">
                {hub.current_load.toLocaleString()}
                <span className="text-sm text-white/30 ml-1">/ {hub.capacity.toLocaleString()}</span>
              </span>
              <span className="text-sm font-mono font-bold" style={{ color }}>{capacityPct}%</span>
            </div>
            <div className="h-2.5 rounded-full bg-glass-300 overflow-hidden">
              <motion.div
                className="h-full rounded-full"
                initial={{ width: 0 }}
                animate={{ width: `${Math.min(capacityPct, 100)}%` }}
                transition={{ duration: 0.6, ease: "easeOut" }}
                style={{ background: color }}
              />
            </div>
            {tier === "critical" && (
              <p className="text-2xs font-mono text-red-signal mt-2 flex items-center gap-1">
                <AlertTriangle size={10} /> Near capacity — consider redistributing parcels
              </p>
            )}
          </GlassCard>

          {/* Status & Activity */}
          <GlassCard size="sm">
            <p className="text-2xs font-mono text-white/40 uppercase tracking-widest mb-3">Status</p>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <p className="text-2xs font-mono text-white/30 mb-0.5">Operational</p>
                <p className={`text-xs font-mono font-semibold ${hub.is_active ? "text-green-signal" : "text-white/40"}`}>
                  {hub.is_active ? "Active" : "Inactive"}
                </p>
              </div>
              <div>
                <p className="text-2xs font-mono text-white/30 mb-0.5">Utilization</p>
                <p className="text-xs font-mono font-semibold" style={{ color }}>{capacityPct}%</p>
              </div>
              <div>
                <p className="text-2xs font-mono text-white/30 mb-0.5">Parcels In Hub</p>
                <p className="text-xs font-mono font-semibold text-white">{hub.current_load.toLocaleString()}</p>
              </div>
              <div>
                <p className="text-2xs font-mono text-white/30 mb-0.5">Total Capacity</p>
                <p className="text-xs font-mono font-semibold text-white">{hub.capacity.toLocaleString()}</p>
              </div>
            </div>
          </GlassCard>

          {/* Serving Zones */}
          {hub.serving_zones.length > 0 && (
            <GlassCard size="sm">
              <div className="flex items-center gap-2 mb-3">
                <Layers size={12} className="text-white/30" />
                <p className="text-2xs font-mono text-white/40 uppercase tracking-widest">
                  Serving Zones ({hub.serving_zones.length})
                </p>
              </div>
              <div className="flex flex-wrap gap-1.5">
                {hub.serving_zones.map((zone) => (
                  <span
                    key={zone}
                    className="px-2 py-0.5 rounded-md bg-glass-200 border border-glass-border text-2xs font-mono text-white/60"
                  >
                    {zone}
                  </span>
                ))}
              </div>
            </GlassCard>
          )}

          {/* Quick links */}
          <GlassCard size="sm">
            <p className="text-2xs font-mono text-white/40 uppercase tracking-widest mb-3">Quick Links</p>
            <div className="space-y-2">
              <a
                href={`/partner/manifests?hub=${encodeURIComponent(id)}`}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center justify-between w-full px-3 py-2 rounded-lg border border-glass-border bg-glass-100 text-xs font-mono text-white/60 hover:border-cyan-neon/40 hover:text-cyan-neon transition-colors"
              >
                <span className="flex items-center gap-2"><FileText size={11} /> Hub Manifests</span>
                <ExternalLink size={10} />
              </a>
              <a
                href={`/analytics?hub=${encodeURIComponent(id)}`}
                className="flex items-center justify-between w-full px-3 py-2 rounded-lg border border-glass-border bg-glass-100 text-xs font-mono text-white/60 hover:border-purple-plasma/40 hover:text-purple-plasma transition-colors"
              >
                <span className="flex items-center gap-2"><Truck size={11} /> Hub Analytics</span>
                <ExternalLink size={10} />
              </a>
            </div>
          </GlassCard>
        </div>
      </motion.div>
    </motion.div>
  );
}

export default function HubOpsPage() {
  const api = useMemo(() => createHubsApi(), []);

  const [hubs, setHubs]               = useState<Hub[]>([]);
  const [loading, setLoading]         = useState(true);
  const [error, setError]             = useState<string | null>(null);
  const [throughput, setThroughput]   = useState<HubThroughputBucket[]>(THROUGHPUT_FALLBACK);
  const [throughputLive, setThroughputLive] = useState(false);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [list] = await Promise.all([
        api.list(),
        api.throughputToday()
          .then((buckets) => { setThroughput(buckets.length > 0 ? buckets : THROUGHPUT_FALLBACK); setThroughputLive(true); })
          .catch(() => { /* endpoint not yet deployed — keep fallback zeros */ }),
      ]);
      setHubs(list);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load hubs");
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => { load(); }, [load]);

  // Poll every 60s — hub current_load changes as parcels are inducted and sorted.
  useEffect(() => {
    const id = setInterval(load, 60_000);
    return () => clearInterval(id);
  }, [load]);

  const [drawerHub, setDrawerHub] = useState<Hub | null>(null);

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
                onClick={() => setDrawerHub(hub)}
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

      {/* Hub Detail Drawer */}
      <AnimatePresence>
        {drawerHub && (
          <HubDetailDrawer hub={drawerHub} onClose={() => setDrawerHub(null)} />
        )}
      </AnimatePresence>

      {/* Hourly throughput — live from /v1/hubs/throughput/today; falls back to zeros if endpoint not yet deployed */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="purple">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Sorting Throughput — Today</h2>
              <p className="text-2xs font-mono text-white/30">
                Per-hour induction + sort counts · {throughputLive ? "live" : "endpoint pending"}
              </p>
            </div>
            <NeonBadge variant={throughputLive ? "green" : "muted"} dot={throughputLive}>
              {throughputLive ? "Live" : "Pending"}
            </NeonBadge>
          </div>
          <ResponsiveContainer width="100%" height={160}>
            <BarChart data={throughput} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
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
