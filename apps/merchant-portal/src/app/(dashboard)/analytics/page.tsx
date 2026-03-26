"use client";
/**
 * Merchant Portal — Analytics Page
 * Delivery performance, COD reconciliation, zone heatmap summary.
 */
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  AreaChart, Area, BarChart, Bar,
  XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import { BarChart3, TrendingUp, TrendingDown, Calendar } from "lucide-react";

// ── Mock data ──────────────────────────────────────────────────────────────────

const KPI_METRICS = [
  { label: "Delivery Rate",    value: 94.2, trend: +1.8,  color: "green"  as const, format: "percent" as const },
  { label: "Avg Days",         value: 1.9,  trend: -0.2,  color: "cyan"   as const, format: "number"  as const },
  { label: "COD Collected",    value: 2840000, trend: +14.2, color: "amber" as const, format: "currency" as const },
  { label: "Failed Attempts",  value: 4.8,  trend: -0.6,  color: "red"    as const, format: "percent" as const },
];

const DELIVERY_TREND = Array.from({ length: 30 }, (_, i) => ({
  day: i + 1,
  delivered: Math.floor(380 + Math.random() * 120),
  failed:    Math.floor(15  + Math.random() * 30),
}));

const ZONE_DATA = [
  { zone: "Metro Manila",    delivered: 5842, failed: 210, rate: 96.5 },
  { zone: "Luzon Provinces", delivered: 2104, failed: 118, rate: 94.7 },
  { zone: "Visayas",         delivered: 784,  failed: 62,  rate: 92.7 },
  { zone: "Mindanao",        delivered: 412,  failed: 48,  rate: 89.6 },
];

const WEEKLY_COD = [
  { week: "W9",  collected: 580000, pending: 42000 },
  { week: "W10", collected: 620000, pending: 38000 },
  { week: "W11", collected: 710000, pending: 55000 },
  { week: "W12", collected: 840000, pending: 61000 },
];

const FAIL_REASONS = [
  { reason: "Customer absent",    pct: 42, color: "#FF3B5C" },
  { reason: "Wrong address",      pct: 23, color: "#FFAB00" },
  { reason: "Refused delivery",   pct: 16, color: "#A855F7" },
  { reason: "Area not covered",   pct: 11, color: "#00E5FF" },
  { reason: "Other",              pct: 8,  color: "rgba(255,255,255,0.2)" },
];

export default function AnalyticsPage() {
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
          <p className="text-sm text-white/40 font-mono mt-0.5">March 2026 · Real-time delivery intelligence</p>
        </div>
        <button className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors">
          <Calendar size={12} /> Mar 2026
        </button>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {KPI_METRICS.map((m) => (
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
            <AreaChart data={DELIVERY_TREND} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
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
              <XAxis dataKey="day" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                labelStyle={{ color: "rgba(255,255,255,0.4)" }}
              />
              <Area type="monotone" dataKey="delivered" stroke="#00FF88" fill="url(#grad-del)"  strokeWidth={2} />
              <Area type="monotone" dataKey="failed"    stroke="#FF3B5C" fill="url(#grad-fail)" strokeWidth={2} />
            </AreaChart>
          </ResponsiveContainer>
        </GlassCard>
      </motion.div>

      {/* Zone performance + failure reasons */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        {/* Zone breakdown */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="h-full">
            <h2 className="font-heading text-sm font-semibold text-white mb-4">Delivery Rate by Zone</h2>
            <div className="flex flex-col gap-3">
              {ZONE_DATA.map((z) => (
                <div key={z.zone} className="flex flex-col gap-1.5">
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-white/70">{z.zone}</span>
                    <div className="flex items-center gap-2">
                      <span className="text-2xs font-mono text-white/40">{z.delivered.toLocaleString()} delivered</span>
                      <span className={`text-xs font-bold font-mono ${z.rate > 95 ? "text-green-signal" : z.rate > 92 ? "text-cyan-neon" : "text-amber-signal"}`}>
                        {z.rate}%
                      </span>
                    </div>
                  </div>
                  <div className="h-1.5 rounded-full bg-glass-300 overflow-hidden">
                    <div
                      className="h-full rounded-full transition-all"
                      style={{
                        width: `${z.rate}%`,
                        background: z.rate > 95 ? "#00FF88" : z.rate > 92 ? "#00E5FF" : "#FFAB00",
                      }}
                    />
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>

        {/* Failure reasons */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="h-full">
            <div className="flex items-center justify-between mb-4">
              <h2 className="font-heading text-sm font-semibold text-white">Failed Delivery Reasons</h2>
              <TrendingDown size={14} className="text-red-signal" />
            </div>
            <div className="flex flex-col gap-2.5">
              {FAIL_REASONS.map((r) => (
                <div key={r.reason} className="flex items-center gap-3">
                  <div className="flex-1">
                    <div className="flex items-center justify-between mb-1">
                      <span className="text-xs text-white/70">{r.reason}</span>
                      <span className="text-xs font-mono font-semibold" style={{ color: r.color }}>{r.pct}%</span>
                    </div>
                    <div className="h-1 rounded-full bg-glass-300 overflow-hidden">
                      <div className="h-full rounded-full" style={{ width: `${r.pct}%`, background: r.color }} />
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
              <p className="text-2xs font-mono text-white/30">Collected vs Pending remittance</p>
            </div>
            <NeonBadge variant="amber">₱2.84M MTD</NeonBadge>
          </div>
          <ResponsiveContainer width="100%" height={160}>
            <BarChart data={WEEKLY_COD} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="week" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                formatter={(v) => [`₱${Number(v).toLocaleString()}`, ""]}
              />
              <Bar dataKey="collected" fill="#FFAB00" radius={[4,4,0,0]} fillOpacity={0.85} />
              <Bar dataKey="pending"   fill="rgba(255,171,0,0.25)" radius={[4,4,0,0]} />
            </BarChart>
          </ResponsiveContainer>
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
