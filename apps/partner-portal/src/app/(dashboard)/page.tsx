"use client";

import { motion } from "framer-motion";
import {
  Target,
  Truck,
  DollarSign,
  TrendingUp,
  ArrowRight,
  CheckCircle2,
  AlertTriangle,
} from "lucide-react";
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  Tooltip,
  ReferenceLine,
  ResponsiveContainer,
} from "recharts";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { variants } from "@/lib/design-system/tokens";

// ─── Static data ──────────────────────────────────────────────────────────────

// 30-day SLA trend (last 30 days, daily %)
const SLA_TREND_DATA = Array.from({ length: 30 }, (_, i) => ({
  day: i + 1,
  sla: parseFloat((94 + Math.sin(i * 0.4) * 2.5 + Math.random() * 1.5).toFixed(1)),
}));

const TOP_ZONES = [
  { rank: 1, zone: "Quezon City",   delivered: 89, total: 91, rate: 97.8 },
  { rank: 2, zone: "Makati CBD",    delivered: 74, total: 76, rate: 97.4 },
  { rank: 3, zone: "Taguig",        delivered: 67, total: 70, rate: 95.7 },
  { rank: 4, zone: "Pasig",         delivered: 45, total: 48, rate: 93.8 },
  { rank: 5, zone: "Mandaluyong",   delivered: 37, total: 40, rate: 92.5 },
];

const CURRENT_MONTH = new Intl.DateTimeFormat("en-US", {
  month: "long",
  year: "numeric",
}).format(new Date());

// ─── Custom Tooltip ───────────────────────────────────────────────────────────

function SlaTooltip({
  active,
  payload,
  label,
}: {
  active?: boolean;
  payload?: Array<{ value: number }>;
  label?: number;
}) {
  if (!active || !payload?.length) return null;
  const sla = payload[0].value;
  return (
    <div
      className="rounded-lg border border-glass-border px-3 py-2 text-xs"
      style={{
        background: "rgba(13, 20, 34, 0.95)",
        backdropFilter: "blur(8px)",
      }}
    >
      <p className="font-mono text-white/40 mb-1">Day {label}</p>
      <p
        className="font-bold"
        style={{ color: sla >= 95 ? "#00FF88" : sla >= 90 ? "#FFAB00" : "#FF3B5C" }}
      >
        {sla}%
      </p>
    </div>
  );
}

// ─── Page ─────────────────────────────────────────────────────────────────────

export default function PartnerOverviewPage() {
  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="space-y-6"
    >
      {/* ── KPI Cards ─────────────────────────────────────────────────── */}
      <motion.div
        variants={variants.staggerContainer}
        className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4"
      >
        {/* SLA Rate */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="green" accent className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-white/50">SLA Rate</span>
              <Target className="h-4 w-4 text-green-signal/60" />
            </div>
            <div className="flex items-end justify-between">
              <span
                className="font-heading text-3xl font-bold tabular-nums text-white"
                style={{ textShadow: "0 0 16px rgba(0,255,136,0.3)" }}
              >
                96.8%
              </span>
              <NeonBadge variant="green" dot pulse>
                Above target
              </NeonBadge>
            </div>
            {/* Mini progress bar */}
            <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-glass-200">
              <div
                className="h-full rounded-full"
                style={{
                  width: "96.8%",
                  background: "linear-gradient(90deg, #00CC6A, #00FF88)",
                  boxShadow: "0 0 6px rgba(0,255,136,0.4)",
                }}
              />
            </div>
            <p className="text-2xs text-white/30 font-mono">Target: 95.0%</p>
          </GlassCard>
        </motion.div>

        {/* Active Routes */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="cyan" accent className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-white/50">
                Active Routes
              </span>
              <Truck className="h-4 w-4 text-cyan-neon/60" />
            </div>
            <div className="flex items-end justify-between">
              <span
                className="font-heading text-3xl font-bold tabular-nums text-white"
                style={{ textShadow: "0 0 16px rgba(0,229,255,0.3)" }}
              >
                23
              </span>
              <NeonBadge variant="cyan">
                5 completing
              </NeonBadge>
            </div>
          </GlassCard>
        </motion.div>

        {/* Today's Deliveries */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="purple" accent className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-white/50">
                Today&apos;s Deliveries
              </span>
              <TrendingUp className="h-4 w-4 text-purple-plasma/60" />
            </div>
            <div className="flex items-end justify-between">
              <span
                className="font-heading text-3xl font-bold tabular-nums text-white"
                style={{ textShadow: "0 0 16px rgba(168,85,247,0.3)" }}
              >
                312
              </span>
              <NeonBadge variant="purple">
                +8% today
              </NeonBadge>
            </div>
          </GlassCard>
        </motion.div>

        {/* Pending Remittance */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="amber" accent className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-white/50">
                Pending Remittance
              </span>
              <DollarSign className="h-4 w-4 text-amber-signal/60" />
            </div>
            <div className="flex items-end justify-between">
              <span
                className="font-heading text-3xl font-bold tabular-nums text-white"
                style={{ textShadow: "0 0 16px rgba(255,171,0,0.3)" }}
              >
                ₱54,200
              </span>
              <NeonBadge variant="amber">
                Due Fri
              </NeonBadge>
            </div>
          </GlassCard>
        </motion.div>
      </motion.div>

      {/* ── SLA Trend + Zone table ────────────────────────────────────── */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-5">
        {/* 30-day SLA trend chart */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-3">
          <GlassCard padding="none" className="p-5">
            <div className="mb-4 flex items-center justify-between">
              <div>
                <p className="text-sm font-semibold text-white">
                  30-Day SLA Trend
                </p>
                <p className="text-xs text-white/40">
                  Daily SLA rate · 95% target threshold
                </p>
              </div>
              <NeonBadge variant="green">Last 30 days</NeonBadge>
            </div>
            <div className="h-52">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart
                  data={SLA_TREND_DATA}
                  margin={{ top: 4, right: 4, left: -24, bottom: 0 }}
                >
                  <XAxis
                    dataKey="day"
                    tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10 }}
                    axisLine={false}
                    tickLine={false}
                    interval={4}
                  />
                  <YAxis
                    domain={[88, 100]}
                    tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10 }}
                    axisLine={false}
                    tickLine={false}
                    tickFormatter={(v) => `${v}%`}
                  />
                  <Tooltip content={<SlaTooltip />} />
                  {/* 95% reference line */}
                  <ReferenceLine
                    y={95}
                    stroke="rgba(255,171,0,0.4)"
                    strokeDasharray="4 4"
                    label={{
                      value: "95% Target",
                      fill: "rgba(255,171,0,0.6)",
                      fontSize: 10,
                      position: "right",
                    }}
                  />
                  <Line
                    type="monotone"
                    dataKey="sla"
                    stroke="#00FF88"
                    strokeWidth={2}
                    dot={false}
                    activeDot={{ r: 4, fill: "#00FF88", stroke: "transparent" }}
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          </GlassCard>
        </motion.div>

        {/* Top 5 Zones */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-2">
          <GlassCard padding="none" className="p-5 h-full">
            <div className="mb-4 flex items-center justify-between">
              <p className="text-sm font-semibold text-white">Top Zones</p>
              <button className="flex items-center gap-1 text-xs text-green-signal/70 transition-colors hover:text-green-signal">
                All zones <ArrowRight className="h-3 w-3" />
              </button>
            </div>
            <div className="space-y-2">
              {TOP_ZONES.map((zone) => (
                <div
                  key={zone.zone}
                  className="flex items-center gap-3 rounded-lg px-3 py-2 transition-colors hover:bg-glass-100"
                >
                  <span className="font-mono text-xs text-white/30 w-4 text-right flex-shrink-0">
                    {zone.rank}
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-xs font-medium text-white/80">
                      {zone.zone}
                    </p>
                    <p className="text-2xs text-white/30 font-mono">
                      {zone.delivered}/{zone.total} delivered
                    </p>
                  </div>
                  <span
                    className="flex-shrink-0 font-mono text-xs font-bold tabular-nums"
                    style={{
                      color: zone.rate >= 97 ? "#00FF88" : zone.rate >= 95 ? "#00E5FF" : "#FFAB00",
                    }}
                  >
                    {zone.rate}%
                  </span>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>
      </div>

      {/* ── Payout Summary ────────────────────────────────────────────── */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none" className="p-5">
          <div className="mb-4 flex items-center justify-between">
            <div>
              <p className="text-sm font-semibold text-white">
                Payout Summary — {CURRENT_MONTH}
              </p>
              <p className="text-xs text-white/40">
                Earnings breakdown for the current billing cycle
              </p>
            </div>
            <button
              className="flex items-center gap-1.5 rounded-lg border border-green-signal/30 bg-green-surface px-3 py-1.5 text-xs font-medium text-green-signal transition-all hover:border-green-signal/60"
            >
              <DollarSign className="h-3.5 w-3.5" />
              Request Payout
            </button>
          </div>

          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            {[
              { label: "Gross Earnings",   value: "₱128,400", color: "#00FF88",  icon: TrendingUp },
              { label: "Platform Fee",     value: "₱12,840",  color: "#FF3B5C",  icon: AlertTriangle },
              { label: "Net Earnings",     value: "₱115,560", color: "#00E5FF",  icon: DollarSign },
              { label: "Already Paid",     value: "₱61,360",  color: "#A855F7",  icon: CheckCircle2 },
            ].map(({ label, value, color, icon: Icon }) => (
              <div
                key={label}
                className="rounded-lg border p-4"
                style={{
                  borderColor: `${color}20`,
                  background: `${color}06`,
                }}
              >
                <div className="mb-2 flex items-center gap-1.5">
                  <Icon className="h-3.5 w-3.5" style={{ color }} />
                  <span className="text-xs text-white/40">{label}</span>
                </div>
                <p
                  className="font-heading text-xl font-bold tabular-nums"
                  style={{ color }}
                >
                  {value}
                </p>
              </div>
            ))}
          </div>

          {/* Progress bar to next payout */}
          <div className="mt-4 space-y-1.5">
            <div className="flex items-center justify-between text-xs">
              <span className="text-white/40">Payout progress</span>
              <span className="font-mono text-white/60">
                ₱61,360 / ₱115,560
              </span>
            </div>
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-glass-200">
              <div
                className="h-full rounded-full"
                style={{
                  width: "53%",
                  background: "linear-gradient(90deg, #00CC6A, #00FF88)",
                  boxShadow: "0 0 6px rgba(0,255,136,0.4)",
                }}
              />
            </div>
            <p className="text-2xs text-white/30 font-mono">
              ₱54,200 remaining — disbursement scheduled Friday
            </p>
          </div>
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
