"use client";
/**
 * Admin Portal — Analytics Page
 * Network-wide delivery performance, zone heatmap, SLA trends, AI model accuracy.
 */
import { useState, useEffect } from "react";
import { motion } from "framer-motion";
import { createAnalyticsApi } from "@/lib/api/analytics";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  AreaChart, Area, BarChart, Bar, LineChart, Line,
  XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, ReferenceLine,
} from "recharts";
import { BarChart3, TrendingUp, Brain, Calendar } from "lucide-react";

// ── Mock data ──────────────────────────────────────────────────────────────────

const KPI = [
  { label: "Shipments Today",  value: 1284,   trend: +8.2,  color: "cyan"   as const, format: "number"  as const },
  { label: "Delivery Rate",    value: 94.2,   trend: +1.8,  color: "green"  as const, format: "percent" as const },
  { label: "Avg Delivery Time",value: 1.8,    trend: -0.2,  color: "purple" as const, format: "number"  as const },
  { label: "Revenue MTD",      value: 2840000, trend: +14.2, color: "amber" as const, format: "currency" as const },
];

const WEEKLY_VOLUME = [
  { day: "Mon", delivered: 1180, failed: 48 },
  { day: "Tue", delivered: 1340, failed: 42 },
  { day: "Wed", delivered: 1280, failed: 61 },
  { day: "Thu", delivered: 1420, failed: 38 },
  { day: "Fri", delivered: 1640, failed: 52 },
  { day: "Sat", delivered: 1280, failed: 28 },
  { day: "Sun", delivered: 840,  failed: 18 },
];

const SLA_TREND = [
  { date: "W1",  rate: 92.8, target: 95 }, { date: "W2", rate: 93.4, target: 95 },
  { date: "W3",  rate: 94.1, target: 95 }, { date: "W4", rate: 95.2, target: 95 },
  { date: "W5",  rate: 94.8, target: 95 }, { date: "W6", rate: 96.1, target: 95 },
  { date: "W7",  rate: 95.4, target: 95 }, { date: "W8", rate: 94.2, target: 95 },
  { date: "W9",  rate: 94.8, target: 95 },
];

const ZONE_PERFORMANCE = [
  { zone: "Metro Manila",    deliveries: 8420, rate: 96.5, revenue: 1842000 },
  { zone: "Luzon Provinces", deliveries: 2840, rate: 93.2, revenue: 682000  },
  { zone: "Visayas",         deliveries: 980,  rate: 89.8, revenue: 284000  },
  { zone: "Mindanao",        deliveries: 520,  rate: 86.4, revenue: 148000  },
];

const AI_METRICS = [
  { label: "Dispatch Accuracy",   value: 97.4, color: "#00E5FF" },
  { label: "ETA Accuracy (±30m)", value: 84.2, color: "#A855F7" },
  { label: "Fraud Detection",     value: 99.1, color: "#00FF88" },
  { label: "Demand Forecast",     value: 91.8, color: "#FFAB00" },
];

export default function AnalyticsPage() {
  const [kpi, setKpi] = useState(KPI);
  const [weeklyVolume, setWeeklyVolume] = useState(WEEKLY_VOLUME);
  const [zonePerformance, setZonePerformance] = useState(ZONE_PERFORMANCE);

  useEffect(() => {
    const api = createAnalyticsApi();
    api.getDashboard().then((res) => {
      const m = res.data.metrics;
      setKpi([
        { label: "Shipments Today",   value: m.shipments_today,   trend: m.shipments_today_trend,   color: "cyan"   as const, format: "number"   as const },
        { label: "Delivery Rate",     value: m.delivery_rate,     trend: m.delivery_rate_trend,     color: "green"  as const, format: "percent"  as const },
        { label: "Avg Delivery Time", value: m.avg_delivery_days, trend: m.avg_delivery_days_trend, color: "purple" as const, format: "number"   as const },
        { label: "Revenue MTD",       value: m.revenue_mtd,       trend: m.revenue_mtd_trend,       color: "amber"  as const, format: "currency" as const },
      ]);
      if (res.data.weekly_volume?.length) setWeeklyVolume(res.data.weekly_volume);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      if (res.data.zone_performance?.length) setZonePerformance(res.data.zone_performance as any);
    }).catch(() => { /* retain mock on error */ });
  }, []);

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
          <p className="text-sm text-white/40 font-mono mt-0.5">Network-wide · March 2026 · All tenants</p>
        </div>
        <button className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors">
          <Calendar size={12} /> Mar 2026
        </button>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {kpi.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Weekly volume + SLA trend */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="purple" className="h-full">
            <div className="flex items-center justify-between mb-5">
              <div>
                <h2 className="font-heading text-sm font-semibold text-white">Weekly Delivery Volume</h2>
                <p className="text-2xs font-mono text-white/30">Delivered vs Failed</p>
              </div>
              <TrendingUp size={15} className="text-purple-plasma" />
            </div>
            <ResponsiveContainer width="100%" height={180}>
              <BarChart data={weeklyVolume} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
                <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
                <XAxis dataKey="day" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
                <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
                <Tooltip
                  contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                  labelStyle={{ color: "rgba(255,255,255,0.4)" }}
                />
                <Bar dataKey="delivered" fill="#A855F7" radius={[4,4,0,0]} fillOpacity={0.85} />
                <Bar dataKey="failed"    fill="#FF3B5C" radius={[4,4,0,0]} fillOpacity={0.7}  />
              </BarChart>
            </ResponsiveContainer>
          </GlassCard>
        </motion.div>

        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="green" className="h-full">
            <div className="flex items-center justify-between mb-5">
              <div>
                <h2 className="font-heading text-sm font-semibold text-white">SLA Compliance — Weekly</h2>
                <p className="text-2xs font-mono text-white/30">Target: 95%</p>
              </div>
            </div>
            <ResponsiveContainer width="100%" height={180}>
              <LineChart data={SLA_TREND} margin={{ top: 10, right: 10, bottom: 0, left: -24 }}>
                <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
                <XAxis dataKey="date" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
                <YAxis domain={[88, 100]} tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
                <Tooltip
                  contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                  formatter={(v) => [`${v}%`, "SLA Rate"]}
                />
                <ReferenceLine y={95} stroke="rgba(255,171,0,0.4)" strokeDasharray="4 4" />
                <Line type="monotone" dataKey="rate" stroke="#00FF88" strokeWidth={2} dot={{ fill: "#00FF88", r: 3 }} />
              </LineChart>
            </ResponsiveContainer>
          </GlassCard>
        </motion.div>
      </div>

      {/* Zone performance table */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white">Performance by Zone — MTD</h2>
          </div>
          <div className="grid grid-cols-4 gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Zone", "Deliveries", "Success Rate", "Revenue"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>
          {zonePerformance.map((z) => (
            <div key={z.zone} className="grid grid-cols-4 gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors">
              <span className="text-sm font-medium text-white">{z.zone}</span>
              <span className="text-xs font-mono text-white/60">{z.deliveries.toLocaleString()}</span>
              <div>
                <div className="flex items-center gap-2 mb-1">
                  <span className={`text-xs font-bold font-mono ${z.rate >= 95 ? "text-green-signal" : z.rate >= 90 ? "text-cyan-neon" : "text-amber-signal"}`}>
                    {z.rate}%
                  </span>
                </div>
                <div className="h-1 rounded-full bg-glass-300 overflow-hidden">
                  <div className="h-full rounded-full" style={{ width: `${z.rate}%`, background: z.rate >= 95 ? "#00FF88" : z.rate >= 90 ? "#00E5FF" : "#FFAB00" }} />
                </div>
              </div>
              <span className="text-xs font-mono text-amber-signal">₱{z.revenue.toLocaleString()}</span>
            </div>
          ))}
        </GlassCard>
      </motion.div>

      {/* AI model metrics */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="purple">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
                <Brain size={15} className="text-purple-plasma" />
                AI Model Performance
              </h2>
              <p className="text-2xs font-mono text-white/30">Live model accuracy metrics</p>
            </div>
            <NeonBadge variant="purple">claude-opus-4-6</NeonBadge>
          </div>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            {AI_METRICS.map((m) => (
              <div key={m.label} className="rounded-xl bg-glass-100 border border-glass-border p-4">
                <p className="text-2xs font-mono text-white/30 uppercase tracking-wider mb-2">{m.label}</p>
                <p className="font-heading text-2xl font-bold" style={{ color: m.color }}>{m.value}%</p>
                <div className="h-1 rounded-full bg-glass-300 overflow-hidden mt-2">
                  <div className="h-full rounded-full" style={{ width: `${m.value}%`, background: m.color }} />
                </div>
              </div>
            ))}
          </div>
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
