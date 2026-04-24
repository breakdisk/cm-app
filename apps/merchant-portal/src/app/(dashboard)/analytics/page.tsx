"use client";
/**
 * Merchant Portal — Analytics Page
 * Delivery performance + COD breakdown backed by the analytics service:
 *   GET /v1/analytics/dashboard     → live metrics + weekly/zone/SLA breakdowns
 *   GET /v1/analytics/kpis?from&to  → totals for the 30-day window
 *   GET /v1/analytics/timeseries    → daily buckets for the trend chart
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  AreaChart, Area, BarChart, Bar,
  XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import { BarChart3, TrendingUp, TrendingDown, Calendar, RefreshCw } from "lucide-react";
import {
  analyticsApi, daysAgo, today,
  type DashboardData, type DeliveryKpis, type DailyBucket,
} from "@/lib/api/analytics";

// Analytics service doesn't expose a per-reason failure breakdown yet — the
// shipment_events aggregate stores the boolean `on_time` but not a structured
// failure reason. Shown as a placeholder until we extend the aggregate.
const FAIL_REASONS_TODO = [
  { reason: "Breakdown not yet available", pct: 100, color: "rgba(255,255,255,0.2)" },
];

function fmtPhp(cents: number): string {
  if (cents >= 1_000_000_00) return `₱${(cents / 100_000_000).toFixed(2)}M`;
  if (cents >= 1_000_00)     return `₱${(cents / 100_000).toFixed(1)}K`;
  return `₱${Math.round(cents / 100).toLocaleString()}`;
}

function fmtWindowLabel(from: string): string {
  const d = new Date(from);
  return d.toLocaleString(undefined, { month: "short", year: "numeric" });
}

export default function AnalyticsPage() {
  const [dashboard, setDashboard] = useState<DashboardData | null>(null);
  const [kpis, setKpis]           = useState<DeliveryKpis | null>(null);
  const [timeseries, setTimeseries] = useState<DailyBucket[]>([]);
  const [loading, setLoading]     = useState(true);
  const [error, setError]         = useState<string | null>(null);

  // 30-day window — could be made user-selectable in a follow-up.
  const windowFrom = useMemo(() => daysAgo(30), []);
  const windowTo   = useMemo(() => today(), []);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [dash, k, ts] = await Promise.all([
        analyticsApi.dashboard(),
        analyticsApi.kpis(windowFrom, windowTo),
        analyticsApi.timeseries(windowFrom, windowTo),
      ]);
      setDashboard(dash);
      setKpis(k);
      setTimeseries(ts);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load analytics");
    } finally {
      setLoading(false);
    }
  }, [windowFrom, windowTo]);

  useEffect(() => { load(); }, [load]);

  // Prefer the dashboard's 'today' metrics for top-line KPIs; fall back to
  // the 30-day window totals when the dashboard endpoint is unavailable.
  const kpiCards = useMemo(() => {
    if (dashboard) {
      const m = dashboard.metrics;
      return [
        { label: "Delivery Rate",   value: m.delivery_rate,       trend: m.delivery_rate_trend,       color: "green"  as const, format: "percent"  as const },
        { label: "Avg Days",        value: m.avg_delivery_days,   trend: m.avg_delivery_days_trend,   color: "cyan"   as const, format: "number"   as const },
        { label: "Revenue MTD",     value: m.revenue_mtd / 100,   trend: m.revenue_mtd_trend,         color: "amber"  as const, format: "currency" as const },
        { label: "Shipments Today", value: m.shipments_today,     trend: m.shipments_today_trend,     color: "purple" as const, format: "number"   as const },
      ];
    }
    if (kpis) {
      return [
        { label: "Delivery Rate",    value: kpis.delivery_success_rate, trend: 0, color: "green"  as const, format: "percent"  as const },
        { label: "On-Time Rate",     value: kpis.on_time_rate,           trend: 0, color: "cyan"   as const, format: "percent"  as const },
        { label: "COD Collected",    value: kpis.cod_collected_cents / 100, trend: 0, color: "amber" as const, format: "currency" as const },
        { label: "Failed Attempts",  value: kpis.total_shipments === 0 ? 0 : (kpis.failed / kpis.total_shipments) * 100, trend: 0, color: "red" as const, format: "percent" as const },
      ];
    }
    return [];
  }, [dashboard, kpis]);

  const zones = dashboard?.zone_performance ?? [];
  const sla   = dashboard?.sla_breakdown ?? [];
  const weekly = dashboard?.weekly_volume ?? [];

  // COD timeseries — group the daily buckets by ISO week for the bar chart.
  const weeklyCod = useMemo(() => {
    if (timeseries.length === 0) return [];
    const weeks = new Map<string, { week: string; collected: number; pending: number }>();
    for (const b of timeseries) {
      const d = new Date(b.date);
      const weekNum = Math.ceil((((d.getTime() - new Date(d.getFullYear(), 0, 1).getTime()) / 86400000) + 1) / 7);
      const key = `W${weekNum}`;
      const bucket = weeks.get(key) ?? { week: key, collected: 0, pending: 0 };
      bucket.collected += b.cod_collected_cents;
      weeks.set(key, bucket);
    }
    return Array.from(weeks.values()).slice(-4);
  }, [timeseries]);

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
            <BarChart3 size={22} className="text-cyan-neon" />
            Analytics
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {fmtWindowLabel(windowFrom)} · Real-time delivery intelligence
          </p>
        </div>
        <div className="flex items-center gap-2">
          <span className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60">
            <Calendar size={12} /> Last 30 days
          </span>
          <button
            onClick={load}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            title="Refresh"
          >
            <RefreshCw size={13} />
          </button>
        </div>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="sm">
            <p className="text-xs text-red-signal font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {loading && kpiCards.length === 0 ? (
          Array.from({ length: 4 }).map((_, i) => (
            <GlassCard key={i} size="sm">
              <div className="h-14 animate-pulse rounded bg-glass-200" />
            </GlassCard>
          ))
        ) : kpiCards.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* 30-day delivery trend */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="green">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">30-Day Delivery Trend</h2>
              <p className="text-2xs font-mono text-white/30">Delivered vs Failed per day</p>
            </div>
            <TrendingUp size={15} className="text-green-signal" />
          </div>
          <ResponsiveContainer width="100%" height={180}>
            <AreaChart data={timeseries} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
              <defs>
                <linearGradient id="grad-del" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#00FF88" stopOpacity={0.3} />
                  <stop offset="95%" stopColor="#00FF88" stopOpacity={0}   />
                </linearGradient>
                <linearGradient id="grad-fail" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#FF3B5C" stopOpacity={0.25} />
                  <stop offset="95%" stopColor="#FF3B5C" stopOpacity={0}    />
                </linearGradient>
              </defs>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="date" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} tickFormatter={(v) => v.slice(5)} />
              <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                labelStyle={{ color: "rgba(255,255,255,0.4)" }}
              />
              <Area type="monotone" dataKey="delivered" stroke="#00FF88" fill="url(#grad-del)"  strokeWidth={2} />
              <Area type="monotone" dataKey="failed"    stroke="#FF3B5C" fill="url(#grad-fail)" strokeWidth={2} />
            </AreaChart>
          </ResponsiveContainer>
          {timeseries.length === 0 && !loading && (
            <p className="mt-3 text-center text-2xs text-white/30 font-mono">No shipment activity in the last 30 days.</p>
          )}
        </GlassCard>
      </motion.div>

      {/* Zone performance + failure reasons */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        {/* Zone breakdown */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="h-full">
            <h2 className="font-heading text-sm font-semibold text-white mb-4">Delivery Rate by Zone</h2>
            <div className="flex flex-col gap-3">
              {zones.length === 0 ? (
                <p className="text-xs text-white/30 font-mono">No zone data yet.</p>
              ) : zones.map((z) => (
                <div key={z.zone} className="flex flex-col gap-1.5">
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-white/70">{z.zone}</span>
                    <div className="flex items-center gap-2">
                      <span className="text-2xs font-mono text-white/40">{z.deliveries.toLocaleString()} delivered</span>
                      <span className={`text-xs font-bold font-mono ${z.success_rate > 95 ? "text-green-signal" : z.success_rate > 92 ? "text-cyan-neon" : "text-amber-signal"}`}>
                        {z.success_rate.toFixed(1)}%
                      </span>
                    </div>
                  </div>
                  <div className="h-1.5 rounded-full bg-glass-300 overflow-hidden">
                    <div
                      className="h-full rounded-full transition-all"
                      style={{
                        width: `${Math.min(z.success_rate, 100)}%`,
                        background: z.success_rate > 95 ? "#00FF88" : z.success_rate > 92 ? "#00E5FF" : "#FFAB00",
                      }}
                    />
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>

        {/* SLA breakdown — uses dashboard.sla_breakdown */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="h-full">
            <div className="flex items-center justify-between mb-4">
              <h2 className="font-heading text-sm font-semibold text-white">SLA Breakdown</h2>
              <TrendingDown size={14} className="text-red-signal" />
            </div>
            <div className="flex flex-col gap-2.5">
              {sla.length === 0 ? (
                FAIL_REASONS_TODO.map((r) => (
                  <div key={r.reason} className="flex items-center gap-3">
                    <div className="flex-1">
                      <div className="flex items-center justify-between mb-1">
                        <span className="text-xs text-white/40 italic">{r.reason}</span>
                        <span className="text-xs font-mono font-semibold" style={{ color: r.color }}>—</span>
                      </div>
                    </div>
                  </div>
                ))
              ) : sla.map((r) => (
                <div key={r.name} className="flex items-center gap-3">
                  <div className="flex-1">
                    <div className="flex items-center justify-between mb-1">
                      <span className="text-xs text-white/70">{r.name}</span>
                      <span className="text-xs font-mono font-semibold" style={{ color: r.fill }}>{r.value.toFixed(1)}%</span>
                    </div>
                    <div className="h-1 rounded-full bg-glass-300 overflow-hidden">
                      <div className="h-full rounded-full" style={{ width: `${Math.min(r.value, 100)}%`, background: r.fill }} />
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>
      </div>

      {/* COD collection weekly */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="amber">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">COD Collection — Last 4 Weeks</h2>
              <p className="text-2xs font-mono text-white/30">Collected from daily timeseries</p>
            </div>
            <NeonBadge variant="amber">
              {kpis ? fmtPhp(kpis.cod_collected_cents) : "—"}
            </NeonBadge>
          </div>
          <ResponsiveContainer width="100%" height={160}>
            <BarChart data={weeklyCod} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="week" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                formatter={(v) => [fmtPhp(Number(v)), ""]}
              />
              <Bar dataKey="collected" fill="#FFAB00" radius={[4,4,0,0]} fillOpacity={0.85} />
              <Bar dataKey="pending"   fill="rgba(255,171,0,0.25)" radius={[4,4,0,0]} />
            </BarChart>
          </ResponsiveContainer>
          {weeklyCod.length === 0 && !loading && (
            <p className="mt-3 text-center text-2xs text-white/30 font-mono">No COD collection in the last 30 days.</p>
          )}
          {weekly.length > 0 && (
            <p className="mt-3 text-2xs text-white/30 font-mono">
              Weekly volume from dashboard: {weekly.map(w => `${w.day}:${w.delivered}`).join(' · ')}
            </p>
          )}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
