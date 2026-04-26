"use client";

import { useRouter } from "next/navigation";
import Link from "next/link";
import { motion } from "framer-motion";
import {
  Package, TrendingUp, Users, Wallet, Plus, Upload, BarChart3,
  Megaphone, CheckCircle2, Clock, AlertCircle, Truck, Store,
  ArrowRight, MapPin, Clock3, TrendingDown, ArrowUpRight, ArrowDownRight,
  Activity, ChevronRight, Sparkles,
} from "lucide-react";
import {
  AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer,
} from "recharts";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { cn } from "@/lib/design-system/cn";
import { variants } from "@/lib/design-system/tokens";

// ─── Data ─────────────────────────────────────────────────────────────────────

const WEEKLY = [
  { day: "Mon", d: 38, f: 3 },
  { day: "Tue", d: 52, f: 5 },
  { day: "Wed", d: 47, f: 2 },
  { day: "Thu", d: 61, f: 4 },
  { day: "Fri", d: 55, f: 6 },
  { day: "Sat", d: 44, f: 3 },
  { day: "Sun", d: 47, f: 3 },
];

const SPARK = {
  shipments: [28, 35, 31, 44, 41, 39, 47],
  rate:      [91.2, 92.8, 93.1, 94.0, 93.8, 94.5, 94.2],
  drivers:   [9, 11, 10, 13, 12, 14, 12],
  cod:       [12200, 14800, 11000, 16400, 15200, 19800, 18450],
};

const ACTIVITY = [
  { id: "CM-PH1-S0000847A", customer: "Maria Santos",  location: "Quezon City", time: "2m ago",  status: "delivered"        as const },
  { id: "CM-PH1-E0000846B", customer: "Jose Reyes",    location: "Makati CBD",  time: "18m ago", status: "out_for_delivery" as const },
  { id: "CM-PH1-S0000845C", customer: "Ana Garcia",    location: "Taguig BGC",  time: "35m ago", status: "failed"           as const },
  { id: "CM-PH1-S0000844D", customer: "Carlos Lim",    location: "Pasig Hub",   time: "1h ago",  status: "in_transit"       as const },
  { id: "CM-PH1-D0000843E", customer: "Rosa Cruz",     location: "Mandaluyong", time: "2h ago",  status: "picked_up"        as const },
];

const STATUS_CFG = {
  delivered:        { color: "#00FF88", bg: "rgba(0,255,136,0.10)",  Icon: CheckCircle2, label: "Delivered"        },
  out_for_delivery: { color: "#00E5FF", bg: "rgba(0,229,255,0.10)",  Icon: Truck,        label: "Out for Delivery" },
  failed:           { color: "#FF3B5C", bg: "rgba(255,59,92,0.10)",  Icon: AlertCircle,  label: "Failed"           },
  in_transit:       { color: "#A855F7", bg: "rgba(168,85,247,0.10)", Icon: Package,      label: "In Transit"       },
  picked_up:        { color: "#00E5FF", bg: "rgba(0,229,255,0.10)",  Icon: Package,      label: "Picked Up"        },
};

const AI_INSIGHTS = [
  {
    icon:    MapPin,
    accent:  "#00E5FF",
    urgency: "info"  as const,
    title:   "Peak Zone Today",
    body:    "Quezon City demand is 3× normal. Pre-stage 4 drivers by 10:00 AM to avoid SLA breach.",
    action:  "Assign Drivers",
  },
  {
    icon:    Clock3,
    accent:  "#A855F7",
    urgency: "tip"   as const,
    title:   "Optimal Dispatch Window",
    body:    "Dispatch bulk shipments 8:30–9:15 AM. Historical data shows +8.4% SLA improvement.",
    action:  "Schedule Dispatch",
  },
  {
    icon:    TrendingDown,
    accent:  "#FF3B5C",
    urgency: "alert" as const,
    title:   "Churn Risk Alert",
    body:    "2 high-value merchants (₱48K/mo combined) haven't booked in 14 days.",
    action:  "Send Campaign",
  },
];

// ─── Inline sparkline SVG ─────────────────────────────────────────────────────

function Sparkline({ data, color }: { data: number[]; color: string }) {
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;
  const W = 72, H = 28;
  const step = W / (data.length - 1);
  const pts = data.map((v, i) => [i * step, H - ((v - min) / range) * (H - 4) - 2]);
  const path = pts.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
  const area = `${path} L${W},${H} L0,${H} Z`;
  const id = `sg${color.replace("#", "")}`;
  return (
    <svg width={W} height={H} className="shrink-0" aria-hidden>
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%"   stopColor={color} stopOpacity={0.28} />
          <stop offset="100%" stopColor={color} stopOpacity={0}    />
        </linearGradient>
      </defs>
      <path d={area} fill={`url(#${id})`} />
      <path d={path} fill="none" stroke={color} strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

// ─── Custom Recharts tooltip ──────────────────────────────────────────────────

function ChartTip({ active, payload, label }: { active?: boolean; payload?: Array<{ value: number; name: string }>; label?: string }) {
  if (!active || !payload?.length) return null;
  return (
    <div className="rounded-xl border border-glass-border px-3 py-2.5 text-xs shadow-glass"
      style={{ background: "rgba(8,12,24,0.97)", backdropFilter: "blur(12px)" }}>
      <p className="mb-2 font-mono text-white/40 tracking-wider">{label}</p>
      {payload.map((p) => (
        <div key={p.name} className="flex items-center gap-2">
          <span className="h-1.5 w-1.5 rounded-full" style={{ background: p.name === "d" ? "#00E5FF" : "#FF3B5C" }} />
          <span className="text-white/60">{p.name === "d" ? "Delivered" : "Failed"}</span>
          <span className="ml-2 font-bold tabular-nums" style={{ color: p.name === "d" ? "#00E5FF" : "#FF3B5C" }}>{p.value}</span>
        </div>
      ))}
    </div>
  );
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function getGreeting() {
  const h = new Date().getHours();
  return h < 12 ? "Good morning" : h < 17 ? "Good afternoon" : "Good evening";
}

function getSubtitle(h: number) {
  if (h < 10) return "Pre-dispatch window — review today's route schedule.";
  if (h < 14) return "Your fleet is on the road. Monitor failed attempts closely.";
  if (h < 18) return "Afternoon rush in progress — peak delivery hour.";
  return "Day winding down — review tomorrow's queue.";
}

// ─── Page ─────────────────────────────────────────────────────────────────────

export default function DashboardPage() {
  const hour = new Date().getHours();
  const router = useRouter();

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="mx-auto max-w-[1400px] space-y-5"
    >

      {/* ── 1. HERO BANNER ────────────────────────────────────────────── */}
      <motion.div variants={variants.fadeInUp}>
        <div
          className="relative overflow-hidden rounded-2xl border border-glass-border p-6"
          style={{
            background: "linear-gradient(135deg, rgba(0,229,255,0.05) 0%, rgba(168,85,247,0.05) 50%, rgba(0,255,136,0.03) 100%)",
          }}
        >
          {/* Ambient glow */}
          <div className="pointer-events-none absolute -right-20 -top-20 h-56 w-56 rounded-full opacity-[0.18]"
            style={{ background: "radial-gradient(circle, #00E5FF 0%, transparent 70%)", filter: "blur(40px)" }} />

          <div className="relative flex flex-col gap-5 md:flex-row md:items-center md:justify-between">
            <div>
              <div className="flex items-center gap-2 mb-1.5">
                <span className="inline-flex h-1.5 w-1.5 rounded-full bg-green-signal" style={{ boxShadow: "0 0 6px #00FF88" }} />
                <span className="text-2xs font-mono text-white/30 uppercase tracking-[0.15em]">Live Operations</span>
              </div>
              <h1 className="font-heading text-3xl font-bold text-white leading-tight">
                {getGreeting()},{" "}
                <span style={{
                  background: "linear-gradient(90deg, #00E5FF 0%, #A855F7 100%)",
                  WebkitBackgroundClip: "text",
                  WebkitTextFillColor: "transparent",
                }}>
                  Juan
                </span>
              </h1>
              <p className="mt-1.5 text-sm text-white/40 max-w-md leading-relaxed">{getSubtitle(hour)}</p>
            </div>

            {/* Network health widget */}
            <div
              className="flex items-center gap-4 rounded-xl border px-4 py-3 md:px-5 md:py-4 shrink-0 self-start md:self-auto"
              style={{ borderColor: "rgba(0,255,136,0.15)", background: "rgba(0,255,136,0.03)" }}
            >
              <div className="text-center min-w-[64px]">
                <p className="text-2xs font-mono text-white/25 uppercase tracking-widest mb-1">Network Health</p>
                <p className="font-heading text-2xl font-bold tabular-nums"
                  style={{ color: "#00FF88", textShadow: "0 0 16px rgba(0,255,136,0.45)" }}>97.4</p>
                <p className="text-2xs text-white/25 font-mono">/ 100</p>
              </div>
              <div className="h-12 w-px bg-glass-border" />
              <div className="flex flex-col gap-1.5">
                {[
                  { label: "Dispatch",  val: "99ms", ok: true  },
                  { label: "Tracking",  val: "< 2s",  ok: true  },
                  { label: "WhatsApp",  val: "4.1s",  ok: true  },
                ].map(({ label, val, ok }) => (
                  <div key={label} className="flex items-center gap-2 text-2xs font-mono">
                    <span className={cn("h-1 w-1 rounded-full shrink-0", ok ? "bg-green-signal" : "bg-red-signal")} />
                    <span className="text-white/30 w-14">{label}</span>
                    <span className="font-semibold" style={{ color: ok ? "#00FF88" : "#FF3B5C" }}>{val}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* Stats strip */}
          <div className="relative mt-5 flex flex-wrap gap-x-6 gap-y-2 border-t border-glass-border pt-4">
            {[
              { label: "Shipments this week", value: "344",      color: "#00E5FF" },
              { label: "On-time rate (7d)",   value: "94.2%",    color: "#00FF88" },
              { label: "COD collected today", value: "₱18,450", color: "#FFAB00" },
              { label: "Active routes",       value: "12",       color: "#A855F7" },
              { label: "Avg ETA accuracy",    value: "±28 min",  color: "#00E5FF" },
            ].map(({ label, value, color }) => (
              <div key={label} className="flex items-baseline gap-2">
                <span className="font-mono text-sm font-bold tabular-nums" style={{ color }}>{value}</span>
                <span className="text-2xs text-white/25">{label}</span>
              </div>
            ))}
          </div>
        </div>
      </motion.div>

      {/* ── 2. KPI CARDS ─────────────────────────────────────────────── */}
      <motion.div variants={variants.staggerContainer} className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {[
          {
            label: "Shipments Today",
            value: "47",
            sub:   "344 this week",
            trend: +12.4,
            color: "#00E5FF",
            glow:  "cyan"   as const,
            icon:  Package,
            spark: SPARK.shipments,
          },
          {
            label: "Delivery Rate",
            value: "94.2%",
            sub:   "Target: 95%",
            trend: +1.8,
            color: "#00FF88",
            glow:  "green"  as const,
            icon:  TrendingUp,
            spark: SPARK.rate,
          },
          {
            label: "Active Drivers",
            value: "12",
            sub:   "3 idle · 2 on break",
            trend: 0,
            color: "#A855F7",
            glow:  "purple" as const,
            icon:  Users,
            spark: SPARK.drivers,
          },
          {
            label: "COD Pending",
            value: "₱18,450",
            sub:   "8 orders · due today",
            trend: -6.2,
            color: "#FFAB00",
            glow:  "amber"  as const,
            icon:  Wallet,
            spark: SPARK.cod,
          },
        ].map(({ label, value, sub, trend, color, glow, icon: Icon, spark }) => (
          <motion.div key={label} variants={variants.fadeInUp}>
            <GlassCard glow={glow} accent className="flex flex-col gap-0 p-5 cursor-pointer group">
              <div className="flex items-center justify-between mb-3">
                <span className="text-xs text-white/40 font-medium">{label}</span>
                <div className="flex h-7 w-7 items-center justify-center rounded-lg transition-transform group-hover:scale-110"
                  style={{ background: `${color}12` }}>
                  <Icon className="h-3.5 w-3.5" style={{ color }} />
                </div>
              </div>

              <span className="font-heading text-[1.75rem] font-bold leading-none tabular-nums text-white"
                style={{ textShadow: `0 0 24px ${color}25` }}>
                {value}
              </span>

              <span className="mt-1.5 text-2xs text-white/30 font-mono">{sub}</span>

              <div className="my-3 h-px bg-glass-border" />

              <div className="flex items-end justify-between">
                <Sparkline data={spark} color={color} />
                <div className={cn(
                  "flex items-center gap-1 text-xs font-semibold",
                  trend > 0 ? "text-green-signal" : trend < 0 ? "text-red-signal" : "text-white/25",
                )}>
                  {trend > 0 && <ArrowUpRight className="h-3.5 w-3.5" />}
                  {trend < 0 && <ArrowDownRight className="h-3.5 w-3.5" />}
                  {trend !== 0 ? `${Math.abs(trend)}%` : "—"}
                </div>
              </div>
            </GlassCard>
          </motion.div>
        ))}
      </motion.div>

      {/* ── 2b. MARKETPLACE STRIP ────────────────────────────────────── */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard className="flex flex-col md:flex-row md:items-center gap-4 md:gap-6 p-4">
          <div className="flex items-center gap-3 min-w-0 flex-1">
            <div
              className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-xl"
              style={{
                background: "linear-gradient(135deg, rgba(168,85,247,0.15), rgba(0,229,255,0.12))",
                boxShadow: "0 0 20px rgba(168,85,247,0.2)",
              }}
            >
              <Store className="h-5 w-5" style={{ color: "#A855F7" }} />
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2 mb-0.5">
                <p className="font-heading text-sm font-semibold text-white">Marketplace Capacity</p>
                <NeonBadge variant="purple" dot pulse>Live</NeonBadge>
              </div>
              <p className="text-xs text-white/40 font-mono truncate">
                Idle vehicles across alliance + marketplace partners
              </p>
            </div>
          </div>

          {/* Quick stats */}
          <div className="flex items-center gap-5 md:gap-7 flex-shrink-0">
            <div className="flex flex-col">
              <span className="font-heading text-xl font-bold text-purple-plasma tabular-nums"
                style={{ textShadow: "0 0 14px rgba(168,85,247,0.35)" }}>
                4
              </span>
              <span className="text-2xs text-white/30 font-mono">available now</span>
            </div>
            <div className="flex flex-col">
              <span className="font-heading text-xl font-bold text-amber-signal tabular-nums flex items-center gap-1">
                <Clock size={13} /> 3h
              </span>
              <span className="text-2xs text-white/30 font-mono">fastest idle</span>
            </div>
            <div className="flex flex-col">
              <span className="font-heading text-xl font-bold text-cyan-neon tabular-nums">3</span>
              <span className="text-2xs text-white/30 font-mono">partners</span>
            </div>
          </div>

          {/* Actions */}
          <div className="flex items-center gap-2 flex-shrink-0">
            <Link
              href="/marketplace"
              className="flex items-center gap-1.5 rounded-lg border border-purple-plasma/40 bg-purple-surface px-3 py-2 text-xs font-medium text-purple-plasma transition-all hover:bg-purple-plasma/15 hover:border-purple-plasma"
            >
              Browse Marketplace
              <ArrowRight size={12} />
            </Link>
          </div>
        </GlassCard>
      </motion.div>

      {/* ── 3. PRIMARY CTA + QUICK ACTIONS ───────────────────────────── */}
      <motion.div variants={variants.fadeInUp}>
        <div className="flex flex-wrap gap-3 items-center">
          <button
            onClick={() => router.push("/shipments?new=1")}
            className="group relative flex items-center gap-2.5 overflow-hidden rounded-xl px-5 py-3 text-sm font-bold text-[#050810] transition-all hover:scale-[1.02] active:scale-[0.98]"
            style={{
              background: "linear-gradient(135deg, #00E5FF 0%, #A855F7 100%)",
              boxShadow: "0 0 24px rgba(0,229,255,0.2), 0 4px 16px rgba(0,0,0,0.4)",
            }}
          >
            <Plus className="h-4 w-4" />
            <span>Create Shipment</span>
          </button>

          {[
            { label: "Bulk Upload CSV", icon: Upload,    color: "#A855F7", href: "/shipments?bulk=1" },
            { label: "Book Marketplace", icon: Store,    color: "#A855F7", href: "/marketplace"      },
            { label: "Analytics",       icon: BarChart3, color: "#00FF88", href: "/analytics"        },
            { label: "New Campaign",    icon: Megaphone, color: "#FFAB00", href: "/campaigns?new=1"  },
          ].map(({ label, icon: Icon, color, href }) => (
            <button
              key={label}
              onClick={() => router.push(href)}
              className="flex items-center gap-2 rounded-xl border px-4 py-3 text-sm font-medium transition-all hover:scale-[1.02] active:scale-[0.98]"
              style={{ borderColor: `${color}25`, background: `${color}07`, color }}
            >
              <Icon className="h-4 w-4" />
              {label}
            </button>
          ))}

          <div className="ml-auto hidden sm:flex items-center gap-2">
            <Activity className="h-3.5 w-3.5 text-white/15" />
            <span className="text-2xs font-mono text-white/20">Updated 12s ago</span>
          </div>
        </div>
      </motion.div>

      {/* ── 4. CHART + ACTIVITY FEED ─────────────────────────────────── */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-5">
        {/* Chart */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-3">
          <GlassCard padding="none" className="p-5 h-full flex flex-col">
            <div className="mb-4 flex items-start justify-between">
              <div>
                <p className="font-heading text-sm font-semibold text-white">Weekly Delivery Volume</p>
                <p className="text-xs text-white/35 mt-0.5">Delivered vs. failed — last 7 days</p>
              </div>
              <div className="flex items-center gap-4 text-2xs font-mono text-white/35">
                <span className="flex items-center gap-1.5">
                  <span className="inline-block h-px w-4 bg-cyan-neon" />Delivered
                </span>
                <span className="flex items-center gap-1.5">
                  <span className="inline-block h-px w-4 bg-red-signal" />Failed
                </span>
              </div>
            </div>

            {/* Summary */}
            <div className="mb-4 flex gap-5 border-b border-glass-border pb-4">
              {[
                { label: "Total delivered", value: "344",    color: "#00E5FF" },
                { label: "Failed attempts", value: "26",     color: "#FF3B5C" },
                { label: "Peak day",        value: "Thu 61", color: "#A855F7" },
              ].map(({ label, value, color }) => (
                <div key={label}>
                  <span className="font-mono text-lg font-bold tabular-nums" style={{ color }}>{value}</span>
                  <p className="text-2xs text-white/25 mt-0.5">{label}</p>
                </div>
              ))}
            </div>

            <div className="flex-1 min-h-[220px]">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={WEEKLY} margin={{ top: 4, right: 4, left: -20, bottom: 0 }}>
                  <defs>
                    <linearGradient id="gD" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="5%"  stopColor="#00E5FF" stopOpacity={0.2} />
                      <stop offset="95%" stopColor="#00E5FF" stopOpacity={0}   />
                    </linearGradient>
                    <linearGradient id="gF" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="5%"  stopColor="#FF3B5C" stopOpacity={0.18} />
                      <stop offset="95%" stopColor="#FF3B5C" stopOpacity={0}    />
                    </linearGradient>
                  </defs>
                  <XAxis dataKey="day" tick={{ fill: "rgba(255,255,255,0.25)", fontSize: 11 }} axisLine={false} tickLine={false} />
                  <YAxis tick={{ fill: "rgba(255,255,255,0.25)", fontSize: 11 }} axisLine={false} tickLine={false} />
                  <Tooltip content={<ChartTip />} />
                  <Area type="monotone" dataKey="d" stroke="#00E5FF" strokeWidth={2}   fill="url(#gD)" dot={false} activeDot={{ r: 4, fill: "#00E5FF",  strokeWidth: 0 }} />
                  <Area type="monotone" dataKey="f" stroke="#FF3B5C" strokeWidth={1.5} fill="url(#gF)" dot={false} activeDot={{ r: 3, fill: "#FF3B5C", strokeWidth: 0 }} />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </GlassCard>
        </motion.div>

        {/* Activity feed */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-2">
          <GlassCard padding="none" className="p-5 h-full flex flex-col">
            <div className="mb-3 flex items-center justify-between">
              <div>
                <p className="font-heading text-sm font-semibold text-white">Live Activity</p>
                <p className="text-2xs text-white/30 mt-0.5 font-mono">Updating every 30s</p>
              </div>
              <button onClick={() => router.push("/shipments")} className="flex items-center gap-1 rounded-lg border border-glass-border px-2.5 py-1.5 text-2xs text-white/40 transition-colors hover:text-cyan-neon hover:border-cyan-neon/30">
                All <ChevronRight className="h-3 w-3" />
              </button>
            </div>

            {/* Summary pills */}
            <div className="mb-3 flex flex-wrap gap-1.5">
              {[
                { label: "3 Delivered", color: "#00FF88" },
                { label: "1 Failed",    color: "#FF3B5C" },
                { label: "1 Transit",   color: "#A855F7" },
              ].map(({ label, color }) => (
                <span key={label} className="rounded-full px-2.5 py-0.5 text-2xs font-mono font-medium"
                  style={{ background: `${color}12`, color, border: `1px solid ${color}25` }}>
                  {label}
                </span>
              ))}
            </div>

            {/* Feed */}
            <div className="flex-1 space-y-1 overflow-y-auto">
              {ACTIVITY.map((item, i) => {
                const cfg = STATUS_CFG[item.status];
                return (
                  <motion.div
                    key={item.id}
                    initial={{ opacity: 0, x: 8 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ delay: i * 0.06, ease: [0.16, 1, 0.3, 1], duration: 0.3 }}
                    className="flex items-center gap-3 rounded-xl px-3 py-2.5 transition-colors cursor-pointer hover:bg-glass-200 group"
                  >
                    <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg"
                      style={{ background: cfg.bg }}>
                      <cfg.Icon className="h-3.5 w-3.5" style={{ color: cfg.color }} />
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center justify-between gap-1">
                        <span className="truncate text-xs font-semibold text-white/80">{item.customer}</span>
                        <span className="shrink-0 text-2xs font-semibold font-mono" style={{ color: cfg.color }}>{cfg.label}</span>
                      </div>
                      <div className="flex items-center gap-1.5 mt-0.5">
                        <span className="font-mono text-2xs text-white/20 truncate">{item.id}</span>
                        <span className="text-white/10">·</span>
                        <span className="text-2xs text-white/25 shrink-0 flex items-center gap-1">
                          <Clock className="h-2.5 w-2.5" />{item.time}
                        </span>
                      </div>
                    </div>
                    <ChevronRight className="h-3 w-3 text-white/10 group-hover:text-white/30 shrink-0 transition-colors" />
                  </motion.div>
                );
              })}
            </div>
          </GlassCard>
        </motion.div>
      </div>

      {/* ── 5. AI INSIGHTS ────────────────────────────────────────────── */}
      <motion.div variants={variants.fadeInUp}>
        <div
          className="relative overflow-hidden rounded-2xl border p-5"
          style={{ borderColor: "rgba(168,85,247,0.18)", background: "linear-gradient(135deg, rgba(168,85,247,0.05) 0%, rgba(0,229,255,0.03) 100%)" }}
        >
          {/* BG glow */}
          <div className="pointer-events-none absolute -left-16 -top-16 h-48 w-48 rounded-full opacity-15"
            style={{ background: "radial-gradient(circle, #A855F7 0%, transparent 70%)", filter: "blur(32px)" }} />

          {/* Header */}
          <div className="relative mb-4 flex items-center gap-3">
            <div className="flex h-9 w-9 items-center justify-center rounded-xl"
              style={{ background: "linear-gradient(135deg, rgba(168,85,247,0.18), rgba(0,229,255,0.12))", border: "1px solid rgba(168,85,247,0.28)" }}>
              <Sparkles className="h-4 w-4 text-purple-plasma" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <p className="font-heading text-sm font-bold text-white">AI Insights</p>
                <NeonBadge variant="purple">Claude · 3 active</NeonBadge>
              </div>
              <p className="text-2xs text-white/30 mt-0.5">Personalized recommendations for your fleet</p>
            </div>
          </div>

          {/* Cards */}
          <div className="relative grid grid-cols-1 gap-3 sm:grid-cols-3">
            {AI_INSIGHTS.map(({ icon: Icon, accent, urgency, title, body, action }) => (
              <div
                key={title}
                className="relative flex flex-col gap-3 overflow-hidden rounded-xl border p-4 transition-all cursor-pointer hover:scale-[1.01]"
                style={{ borderColor: `${accent}18`, background: `${accent}05` }}
              >
                {/* Left accent bar */}
                <div className="absolute inset-y-0 left-0 w-[3px] rounded-r-full"
                  style={{ background: `linear-gradient(180deg, ${accent} 0%, ${accent}40 100%)`, boxShadow: `0 0 10px ${accent}60` }} />

                <div className="flex items-center gap-2.5 pl-1">
                  <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg"
                    style={{ background: `${accent}14` }}>
                    <Icon className="h-3.5 w-3.5" style={{ color: accent }} />
                  </div>
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="text-xs font-bold truncate" style={{ color: accent }}>{title}</span>
                    {urgency === "alert" && (
                      <span className="shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-mono font-bold"
                        style={{ background: "rgba(255,59,92,0.14)", color: "#FF3B5C" }}>ALERT</span>
                    )}
                  </div>
                </div>

                <p className="pl-1 text-xs leading-relaxed text-white/50">{body}</p>

                <button
                  className="pl-1 mt-auto flex items-center gap-1.5 text-xs font-semibold transition-all group hover:gap-2.5"
                  style={{ color: accent }}
                >
                  {action}
                  <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
                </button>
              </div>
            ))}
          </div>
        </div>
      </motion.div>

    </motion.div>
  );
}
