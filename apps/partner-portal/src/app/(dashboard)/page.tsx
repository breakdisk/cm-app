"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import Link from "next/link";
import { useRouter } from "next/navigation";
import {
  Target,
  Truck,
  DollarSign,
  TrendingUp,
  ArrowRight,
  CheckCircle2,
  AlertTriangle,
  Store,
  Clock,
  Plus,
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
import { authFetch } from "@/lib/auth/auth-fetch";
import { carriersApi, fmtPhp } from "@/lib/api/carriers";
import { getCurrentPartnerId } from "@/lib/api/partner-identity";

const DISPATCH_URL    = process.env.NEXT_PUBLIC_DISPATCH_URL    ?? "http://localhost:8005";
const PAYMENTS_URL    = process.env.NEXT_PUBLIC_PAYMENTS_URL    ?? "http://localhost:8012";
const DRIVER_OPS_URL  = process.env.NEXT_PUBLIC_DRIVER_OPS_URL  ?? "http://localhost:8006";

// 30-day SLA trend chart isn't backed by an endpoint yet — keep the visual
// scaffolding so the page lays out the same once carriers exposes a
// /v1/carriers/:id/sla-trend route (TODO). Static placeholder values keep
// the line readable until then.
const SLA_TREND_PLACEHOLDER = Array.from({ length: 30 }, (_, i) => ({
  day: i + 1,
  sla: parseFloat((94 + Math.sin(i * 0.4) * 2.5).toFixed(1)),
}));

const CURRENT_MONTH = new Intl.DateTimeFormat("en-US", {
  month: "long",
  year: "numeric",
}).format(new Date());

interface PartnerKpis {
  slaPct:           number | null;
  slaTarget:        number | null;
  activeRoutes:     number;          // unique routes today (drivers with active_route_id)
  todayDeliveries:  number;          // tasks completed today (manifest aggregation)
  pendingRemittance:number;          // outstanding invoices in cents
  grossEarnings:    number;          // billed MTD
  platformFee:      number;          // estimated 10% — refine when invoice line items expose fee_cents
  netEarnings:      number;
  alreadyPaid:      number;
}

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
  const router = useRouter();
  const [kpis, setKpis] = useState<PartnerKpis>({
    slaPct: null, slaTarget: null,
    activeRoutes: 0, todayDeliveries: 0,
    pendingRemittance: 0, grossEarnings: 0,
    platformFee: 0, netEarnings: 0, alreadyPaid: 0,
  });
  const [loading, setLoading] = useState(true);
  const [topZones, setTopZones] = useState<Array<{ rank: number; zone: string; delivered: number; total: number; rate: number }>>([]);

  // Pull every metric from sources we already expose. SLA % is derived from
  // the carrier's lifetime on_time_count / total_shipments — a true 30-day
  // window needs a new endpoint. Same caveat for top zones: derived from a
  // best-effort manifest aggregation grouped by city of the assigned tasks
  // (defaults to whole-tenant when the partner is logged in as admin).
  const load = useCallback(async () => {
    setLoading(true);
    const carrierId = getCurrentPartnerId();
    const today = new Date().toISOString().slice(0, 10);

    const [carrierRes, queueRes, driversRes, manifestRes, invRes, walletRes] =
      await Promise.allSettled([
        carriersApi.get(carrierId),
        authFetch(`${DISPATCH_URL}/v1/queue?status=all`),
        authFetch(`${DRIVER_OPS_URL}/v1/drivers`),
        carriersApi.manifest(today, carrierId),
        authFetch(`${PAYMENTS_URL}/v1/invoices`),
        authFetch(`${PAYMENTS_URL}/v1/wallet`),
      ]);

    const carrier = carrierRes.status === "fulfilled" ? carrierRes.value : null;
    const totalShipments = carrier?.total_shipments ?? 0;
    const onTime = carrier?.on_time_count ?? 0;
    const slaPct = totalShipments > 0 ? (onTime / totalShipments) * 100 : null;

    let activeRoutes = 0;
    if (driversRes.status === "fulfilled" && driversRes.value.ok) {
      const j = await driversRes.value.json();
      const list: Array<{ active_route_id?: string | null }> = j.data ?? [];
      activeRoutes = list.filter((d) => d.active_route_id).length;
    }

    let todayDeliveries = 0;
    const zoneCounts = new Map<string, { delivered: number; total: number }>();
    if (manifestRes.status === "fulfilled") {
      const m = manifestRes.value.data ?? [];
      for (const row of m) {
        if (row.task_type === "delivery") todayDeliveries += row.completed;
      }
    }
    // Best-effort: query the dispatch queue for delivered/total per dest_city.
    // The queue carries dest_city + status; partner sees their own tenant rows.
    if (queueRes.status === "fulfilled" && queueRes.value.ok) {
      const j = await queueRes.value.json();
      const items: Array<{ dest_city?: string; status?: string }> = j.data ?? [];
      for (const it of items) {
        const city = (it.dest_city ?? "").trim() || "Unknown";
        const e = zoneCounts.get(city) ?? { delivered: 0, total: 0 };
        e.total += 1;
        if (it.status === "dispatched") e.delivered += 1;
        zoneCounts.set(city, e);
      }
    }
    const zones = Array.from(zoneCounts.entries())
      .map(([city, c]) => ({
        zone:      city,
        delivered: c.delivered,
        total:     c.total,
        rate:      c.total > 0 ? (c.delivered / c.total) * 100 : 0,
      }))
      .sort((a, b) => b.total - a.total)
      .slice(0, 5)
      .map((z, i) => ({ rank: i + 1, ...z }));
    setTopZones(zones);

    let billedMtd = 0, paid = 0, outstanding = 0;
    if (invRes.status === "fulfilled" && invRes.value.ok) {
      const j = await invRes.value.json();
      const list: Array<{ status?: string; total_cents?: number; billing_period?: string }> = j.data ?? [];
      const now = new Date();
      const monthKey = `${now.getUTCFullYear()}-${String(now.getUTCMonth() + 1).padStart(2, "0")}`;
      for (const inv of list) {
        const cents = inv.total_cents ?? 0;
        if (inv.billing_period === monthKey) billedMtd += cents;
        if (inv.status === "paid")    paid        += cents;
        if (inv.status === "issued"
         || inv.status === "overdue") outstanding += cents;
      }
    }
    let walletBalance = 0;
    if (walletRes.status === "fulfilled" && walletRes.value.ok) {
      const j = await walletRes.value.json();
      walletBalance = j.data?.balance_cents ?? 0;
    }
    const platformFee = Math.round(billedMtd * 0.10);
    const netEarnings = billedMtd - platformFee;

    setKpis({
      slaPct,
      slaTarget:        carrier?.sla.on_time_target_pct ?? null,
      activeRoutes,
      todayDeliveries,
      pendingRemittance: Math.max(outstanding, walletBalance),
      grossEarnings:    billedMtd,
      platformFee,
      netEarnings,
      alreadyPaid:      paid,
    });
    setLoading(false);
  }, []);

  useEffect(() => { load(); }, [load]);

  const slaBadge = useMemo(() => {
    if (kpis.slaPct == null || kpis.slaTarget == null) return { label: "No data", variant: "muted" as const };
    return kpis.slaPct >= kpis.slaTarget
      ? { label: "Above target", variant: "green"  as const }
      : { label: "Below target", variant: "amber"  as const };
  }, [kpis.slaPct, kpis.slaTarget]);

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
        {/* SLA Rate — derived from carrier.on_time_count / total_shipments
             (lifetime). True 30-day window pending /v1/carriers/:id/sla-trend. */}
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
                {kpis.slaPct == null ? "—" : `${kpis.slaPct.toFixed(1)}%`}
              </span>
              <NeonBadge variant={slaBadge.variant} dot pulse={slaBadge.variant === "green"}>
                {slaBadge.label}
              </NeonBadge>
            </div>
            <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-glass-200">
              <div
                className="h-full rounded-full"
                style={{
                  width: `${kpis.slaPct ?? 0}%`,
                  background: "linear-gradient(90deg, #00CC6A, #00FF88)",
                  boxShadow: "0 0 6px rgba(0,255,136,0.4)",
                }}
              />
            </div>
            <p className="text-2xs text-white/30 font-mono">
              Target: {kpis.slaTarget == null ? "—" : `${kpis.slaTarget.toFixed(1)}%`}
            </p>
          </GlassCard>
        </motion.div>

        {/* Active Routes — count of drivers with an active_route_id. */}
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
                {kpis.activeRoutes}
              </span>
              <NeonBadge variant="cyan">
                {loading ? "syncing…" : "live"}
              </NeonBadge>
            </div>
          </GlassCard>
        </motion.div>

        {/* Today's Deliveries — sum of completed delivery tasks from manifest. */}
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
                {kpis.todayDeliveries}
              </span>
              <NeonBadge variant="purple">today</NeonBadge>
            </div>
          </GlassCard>
        </motion.div>

        {/* Pending Remittance — outstanding invoices (issued + overdue). */}
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
                {fmtPhp(kpis.pendingRemittance)}
              </span>
              <NeonBadge variant="amber">due</NeonBadge>
            </div>
          </GlassCard>
        </motion.div>
      </motion.div>

      {/* ── Marketplace strip ─────────────────────────────────────────── */}
      {/* Surfaces idle-capacity monetization at-a-glance. Deep-links into
          /marketplace; the ?new=1 shortcut auto-opens the listing drawer. */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="green" accent padding="none" className="p-5">
          <div className="flex flex-wrap items-center gap-4">
            <div
              className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg border border-green-signal/30 bg-green-surface"
              style={{ boxShadow: "0 0 14px rgba(0,255,136,0.25)" }}
            >
              <Store className="h-4 w-4 text-green-signal" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <p className="text-sm font-semibold text-white">
                  Marketplace Discovery
                </p>
                <NeonBadge variant="green" dot pulse>
                  Live
                </NeonBadge>
              </div>
              <p className="mt-0.5 text-xs text-white/50">
                Monetize idle fleet before windows close. Consumer on-demand
                bookings flow into order-intake like any shipment.
              </p>
            </div>

            <div className="flex items-center gap-5 border-l border-glass-border pl-5">
              <div className="text-right">
                <p className="font-mono text-2xs uppercase tracking-wider text-white/40">
                  Idle &lt; 6h
                </p>
                <p
                  className="mt-0.5 font-mono text-lg font-bold text-amber-signal"
                  style={{ textShadow: "0 0 8px rgba(255,171,0,0.35)" }}
                >
                  3
                </p>
              </div>
              <div className="text-right">
                <p className="font-mono text-2xs uppercase tracking-wider text-white/40">
                  Revenue Today
                </p>
                <p
                  className="mt-0.5 font-mono text-lg font-bold text-green-signal"
                  style={{ textShadow: "0 0 8px rgba(0,255,136,0.35)" }}
                >
                  ₱14,260
                </p>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <Link
                href="/marketplace?new=1"
                className="flex items-center gap-1.5 rounded-lg border border-green-signal/40 bg-green-surface px-3 py-2 text-xs font-medium text-green-signal transition-all hover:shadow-[0_0_12px_rgba(0,255,136,0.35)]"
              >
                <Plus className="h-3 w-3" />
                List Vehicle
              </Link>
              <Link
                href="/marketplace"
                className="flex items-center gap-1 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs font-medium text-white/70 transition-colors hover:bg-glass-200 hover:text-white"
                title="Open Marketplace"
              >
                <Clock className="h-3 w-3" />
                View
                <ArrowRight className="h-3 w-3" />
              </Link>
            </div>
          </div>
        </GlassCard>
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
                  data={SLA_TREND_PLACEHOLDER}
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
              <button onClick={() => router.push("/sla")} className="flex items-center gap-1 text-xs text-green-signal/70 transition-colors hover:text-green-signal">
                All zones <ArrowRight className="h-3 w-3" />
              </button>
            </div>
            <div className="space-y-2">
              {topZones.length === 0 && !loading && (
                <p className="text-2xs font-mono text-white/25 py-3 text-center">No zone activity yet</p>
              )}
              {topZones.map((zone) => (
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
                      {zone.delivered}/{zone.total} dispatched
                    </p>
                  </div>
                  <span
                    className="flex-shrink-0 font-mono text-xs font-bold tabular-nums"
                    style={{
                      color: zone.rate >= 97 ? "#00FF88" : zone.rate >= 95 ? "#00E5FF" : "#FFAB00",
                    }}
                  >
                    {zone.rate.toFixed(0)}%
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
              onClick={() => router.push("/payouts")}
              className="flex items-center gap-1.5 rounded-lg border border-green-signal/30 bg-green-surface px-3 py-1.5 text-xs font-medium text-green-signal transition-all hover:border-green-signal/60"
            >
              <DollarSign className="h-3.5 w-3.5" />
              Request Payout
            </button>
          </div>

          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            {[
              { label: "Gross Earnings", value: fmtPhp(kpis.grossEarnings), color: "#00FF88", icon: TrendingUp     },
              { label: "Platform Fee",   value: fmtPhp(kpis.platformFee),   color: "#FF3B5C", icon: AlertTriangle  },
              { label: "Net Earnings",   value: fmtPhp(kpis.netEarnings),   color: "#00E5FF", icon: DollarSign     },
              { label: "Already Paid",   value: fmtPhp(kpis.alreadyPaid),   color: "#A855F7", icon: CheckCircle2   },
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
                {fmtPhp(kpis.alreadyPaid)} / {fmtPhp(kpis.netEarnings)}
              </span>
            </div>
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-glass-200">
              <div
                className="h-full rounded-full"
                style={{
                  width: kpis.netEarnings > 0
                    ? `${Math.min(100, Math.round((kpis.alreadyPaid / kpis.netEarnings) * 100))}%`
                    : "0%",
                  background: "linear-gradient(90deg, #00CC6A, #00FF88)",
                  boxShadow: "0 0 6px rgba(0,255,136,0.4)",
                }}
              />
            </div>
            <p className="text-2xs text-white/30 font-mono">
              {fmtPhp(Math.max(0, kpis.netEarnings - kpis.alreadyPaid))} remaining
            </p>
          </div>
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
