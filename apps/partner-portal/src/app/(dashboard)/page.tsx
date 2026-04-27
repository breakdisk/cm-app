"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { motion } from "framer-motion";
import Link from "next/link";
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
import { carriersApi, fmtPhp, carrierIdOf } from "@/lib/api/carriers";

const DISPATCH_URL    = process.env.NEXT_PUBLIC_DISPATCH_URL    ?? "http://localhost:8005";
const PAYMENTS_URL    = process.env.NEXT_PUBLIC_PAYMENTS_URL    ?? "http://localhost:8012";
const DRIVER_OPS_URL  = process.env.NEXT_PUBLIC_DRIVER_OPS_URL  ?? "http://localhost:8006";
const ANALYTICS_URL   = process.env.NEXT_PUBLIC_ANALYTICS_URL   ?? "http://localhost:8013";
const MARKETPLACE_URL = process.env.NEXT_PUBLIC_MARKETPLACE_URL ?? "http://localhost:8016";

const CURRENT_MONTH = new Intl.DateTimeFormat("en-US", {
  month: "long",
  year:  "numeric",
}).format(new Date());

function todayStr() { return new Date().toISOString().slice(0, 10); }
function daysAgoStr(n: number) {
  const d = new Date();
  d.setDate(d.getDate() - n);
  return d.toISOString();
}

// ── Types ─────────────────────────────────────────────────────────────────────

interface PartnerKpis {
  slaPct:            number | null;
  slaTarget:         number | null;
  activeRoutes:      number;
  todayDeliveries:   number;
  pendingRemittance: number;
  grossEarnings:     number;
  platformFee:       number;
  netEarnings:       number;
  alreadyPaid:       number;
}

interface SlaPoint { day: string; sla: number }

interface MarketplaceStats { idleCount: number; revenueToday: number }

// ── Custom Tooltip ────────────────────────────────────────────────────────────

function SlaTooltip({
  active,
  payload,
  label,
}: {
  active?:  boolean;
  payload?: Array<{ value: number }>;
  label?:   string;
}) {
  if (!active || !payload?.length) return null;
  const sla = payload[0].value;
  return (
    <div
      className="rounded-lg border border-glass-border px-3 py-2 text-xs"
      style={{ background: "rgba(13, 20, 34, 0.95)", backdropFilter: "blur(8px)" }}
    >
      <p className="font-mono text-white/40 mb-1">{label}</p>
      <p className="font-bold" style={{ color: sla >= 95 ? "#00FF88" : sla >= 90 ? "#FFAB00" : "#FF3B5C" }}>
        {sla.toFixed(1)}%
      </p>
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function PartnerOverviewPage() {
  const router = useRouter();

  const [kpis, setKpis] = useState<PartnerKpis>({
    slaPct: null, slaTarget: null,
    activeRoutes: 0, todayDeliveries: 0,
    pendingRemittance: 0, grossEarnings: 0,
    platformFee: 0, netEarnings: 0, alreadyPaid: 0,
  });
  const [loading,        setLoading]        = useState(true);
  const [topZones,       setTopZones]       = useState<Array<{ rank: number; zone: string; delivered: number; total: number; rate: number }>>([]);
  const [slaChartData,   setSlaChartData]   = useState<SlaPoint[]>([]);
  const [marketStats,    setMarketStats]    = useState<MarketplaceStats>({ idleCount: 0, revenueToday: 0 });

  const load = useCallback(async () => {
    setLoading(true);
    const today = todayStr();

    // ── 1. Resolve the authenticated carrier from JWT ──────────────────────
    let carrier = null;
    let carrierId = "";
    try {
      carrier   = await carriersApi.me();
      carrierId = carrierIdOf(carrier);
    } catch {
      // carrier not found for this user — continue with null so the page
      // still renders with zeroes rather than a hard error.
    }

    const totalShipments = carrier?.total_shipments ?? 0;
    const onTime         = carrier?.on_time_count   ?? 0;
    const slaPct         = totalShipments > 0 ? (onTime / totalShipments) * 100 : null;

    // ── 2. Parallel fetch of all live data ─────────────────────────────────
    const [queueRes, driversRes, manifestRes, invRes, walletRes, timeseriesRes, marketplaceRes] =
      await Promise.allSettled([
        authFetch(`${DISPATCH_URL}/v1/queue?status=all`),
        authFetch(`${DRIVER_OPS_URL}/v1/drivers`),
        carrierId ? carriersApi.manifest(today, carrierId) : Promise.resolve({ data: [], date: today, carrier_id: null }),
        authFetch(`${PAYMENTS_URL}/v1/invoices`),
        authFetch(`${PAYMENTS_URL}/v1/wallet`),
        authFetch(`${ANALYTICS_URL}/v1/analytics/timeseries?from=${daysAgoStr(30)}&to=${new Date().toISOString()}`),
        authFetch(`${MARKETPLACE_URL}/v1/marketplace/listings?limit=100`),
      ]);

    // ── Active routes ──────────────────────────────────────────────────────
    let activeRoutes = 0;
    if (driversRes.status === "fulfilled" && driversRes.value.ok) {
      const j = await driversRes.value.json();
      const list: Array<{ active_route_id?: string | null }> = j.data ?? [];
      activeRoutes = list.filter((d) => d.active_route_id).length;
    }

    // ── Today's deliveries + zone breakdown ───────────────────────────────
    let todayDeliveries = 0;
    const zoneCounts = new Map<string, { delivered: number; total: number }>();

    if (manifestRes.status === "fulfilled") {
      const m = (manifestRes.value as { data: Array<{ task_type: string; completed: number }> }).data ?? [];
      for (const row of m) {
        if (row.task_type === "delivery") todayDeliveries += row.completed;
      }
    }

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

    // ── 30-day SLA trend (from analytics timeseries) ───────────────────────
    if (timeseriesRes.status === "fulfilled" && timeseriesRes.value.ok) {
      const j = await timeseriesRes.value.json();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const buckets: Array<any> = j.data?.buckets ?? j.data ?? [];
      if (buckets.length > 0) {
        const trend = buckets.map((b) => {
          const del   = Number(b.delivered ?? b.completed ?? 0);
          const fail  = Number(b.failed ?? 0);
          const total = del + fail;
          return {
            day: new Date(b.date).toLocaleDateString("en-US", { month: "short", day: "numeric" }),
            sla: total > 0 ? parseFloat(((del / total) * 100).toFixed(1)) : 0,
          };
        });
        setSlaChartData(trend);
      }
    }
    // If analytics returned nothing, leave chart empty — no fake data.

    // ── Financials ─────────────────────────────────────────────────────────
    let billedMtd = 0, paid = 0, outstanding = 0;
    if (invRes.status === "fulfilled" && invRes.value.ok) {
      const j = await invRes.value.json();
      const list: Array<{ status?: string; total_cents?: number; billing_period?: string }> = j.data ?? [];
      const now      = new Date();
      const monthKey = `${now.getUTCFullYear()}-${String(now.getUTCMonth() + 1).padStart(2, "0")}`;
      for (const inv of list) {
        const cents = inv.total_cents ?? 0;
        if (inv.billing_period === monthKey) billedMtd += cents;
        if (inv.status === "paid")    paid        += cents;
        if (inv.status === "issued" || inv.status === "overdue") outstanding += cents;
      }
    }
    let walletBalance = 0;
    if (walletRes.status === "fulfilled" && walletRes.value.ok) {
      const j = await walletRes.value.json();
      walletBalance = j.data?.balance_cents ?? 0;
    }
    // Platform fee: prefer invoice line item `fee_cents` when available;
    // fall back to 10% estimate until the payments service exposes it.
    const platformFee  = Math.round(billedMtd * 0.10);
    const netEarnings  = billedMtd - platformFee;

    setKpis({
      slaPct,
      slaTarget:         carrier?.sla.on_time_target_pct ?? null,
      activeRoutes,
      todayDeliveries,
      pendingRemittance: Math.max(outstanding, walletBalance),
      grossEarnings:     billedMtd,
      platformFee,
      netEarnings,
      alreadyPaid:       paid,
    });

    // ── Marketplace stats ──────────────────────────────────────────────────
    if (marketplaceRes.status === "fulfilled" && marketplaceRes.value.ok) {
      const j = await marketplaceRes.value.json();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const listings: Array<any> = j.data ?? j.listings ?? [];
      const todayPrefix = todayStr();
      let revenueToday = 0;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const idleNow = listings.filter((l: any) => l.status === "active").length;
      // Tally revenue from bookings accepted/delivered today (if bookings embedded).
      for (const l of listings) {
        for (const b of (l.bookings ?? [])) {
          const acceptedAt: string = b.accepted_at ?? b.created_at ?? "";
          if ((b.status === "accepted" || b.status === "in_transit" || b.status === "delivered") &&
              acceptedAt.startsWith(todayPrefix)) {
            revenueToday += Number(b.total_cents ?? b.amount_cents ?? 0);
          }
        }
      }
      setMarketStats({ idleCount: idleNow, revenueToday });
    }

    setLoading(false);
  }, []);

  useEffect(() => { load(); }, [load]);

  // Refresh on a 60s interval — SLA rate and route counts change frequently.
  useEffect(() => {
    const id = setInterval(load, 60_000);
    return () => clearInterval(id);
  }, [load]);

  const slaBadge = useMemo(() => {
    if (kpis.slaPct == null || kpis.slaTarget == null) return { label: "No data", variant: "muted" as const };
    return kpis.slaPct >= kpis.slaTarget
      ? { label: "Above target", variant: "green"  as const }
      : { label: "Below target", variant: "amber"  as const };
  }, [kpis.slaPct, kpis.slaTarget]);

  // The target line shown on the chart — use the carrier's contractual SLA
  // if loaded, otherwise default to 95.
  const slaTargetLine = kpis.slaTarget ?? 95;

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

        {/* Active Routes */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="cyan" accent className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-white/50">Active Routes</span>
              <Truck className="h-4 w-4 text-cyan-neon/60" />
            </div>
            <div className="flex items-end justify-between">
              <span
                className="font-heading text-3xl font-bold tabular-nums text-white"
                style={{ textShadow: "0 0 16px rgba(0,229,255,0.3)" }}
              >
                {kpis.activeRoutes}
              </span>
              <NeonBadge variant="cyan">{loading ? "syncing…" : "live"}</NeonBadge>
            </div>
          </GlassCard>
        </motion.div>

        {/* Today's Deliveries */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="purple" accent className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-white/50">Today&apos;s Deliveries</span>
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

        {/* Pending Remittance */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="amber" accent className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-white/50">Pending Remittance</span>
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
                <p className="text-sm font-semibold text-white">Marketplace Discovery</p>
                <NeonBadge variant="green" dot pulse>Live</NeonBadge>
              </div>
              <p className="mt-0.5 text-xs text-white/50">
                Monetize idle fleet before windows close. Consumer on-demand bookings flow into order-intake like any shipment.
              </p>
            </div>

            <div className="flex items-center gap-5 border-l border-glass-border pl-5">
              <div className="text-right">
                <p className="font-mono text-2xs uppercase tracking-wider text-white/40">Idle &lt; 6h</p>
                <p
                  className="mt-0.5 font-mono text-lg font-bold text-amber-signal"
                  style={{ textShadow: "0 0 8px rgba(255,171,0,0.35)" }}
                >
                  {marketStats.idleCount}
                </p>
              </div>
              <div className="text-right">
                <p className="font-mono text-2xs uppercase tracking-wider text-white/40">Revenue Today</p>
                <p
                  className="mt-0.5 font-mono text-lg font-bold text-green-signal"
                  style={{ textShadow: "0 0 8px rgba(0,255,136,0.35)" }}
                >
                  {fmtPhp(marketStats.revenueToday)}
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
        {/* 30-day SLA trend */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-3">
          <GlassCard padding="none" className="p-5">
            <div className="mb-4 flex items-center justify-between">
              <div>
                <p className="text-sm font-semibold text-white">30-Day SLA Trend</p>
                <p className="text-xs text-white/40">Daily SLA rate · {slaTargetLine}% target threshold</p>
              </div>
              <NeonBadge variant={slaChartData.length > 0 ? "green" : "muted"}>
                {slaChartData.length > 0 ? "Live data" : "No data yet"}
              </NeonBadge>
            </div>
            <div className="h-52">
              {slaChartData.length === 0 ? (
                <div className="flex h-full items-center justify-center">
                  <p className="text-2xs font-mono text-white/25">
                    SLA trend populates as shipments complete
                  </p>
                </div>
              ) : (
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={slaChartData} margin={{ top: 4, right: 4, left: -24, bottom: 0 }}>
                    <XAxis
                      dataKey="day"
                      tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10 }}
                      axisLine={false}
                      tickLine={false}
                      interval={Math.max(0, Math.floor(slaChartData.length / 6) - 1)}
                    />
                    <YAxis
                      domain={[Math.max(0, Math.min(...slaChartData.map(d => d.sla)) - 5), 100]}
                      tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10 }}
                      axisLine={false}
                      tickLine={false}
                      tickFormatter={(v) => `${v}%`}
                    />
                    <Tooltip content={<SlaTooltip />} />
                    <ReferenceLine
                      y={slaTargetLine}
                      stroke="rgba(255,171,0,0.4)"
                      strokeDasharray="4 4"
                      label={{
                        value: `${slaTargetLine}% Target`,
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
              )}
            </div>
          </GlassCard>
        </motion.div>

        {/* Top 5 Zones */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-2">
          <GlassCard padding="none" className="p-5 h-full">
            <div className="mb-4 flex items-center justify-between">
              <p className="text-sm font-semibold text-white">Top Zones</p>
              <Link
                href="/sla"
                className="flex items-center gap-1 text-xs text-green-signal/70 transition-colors hover:text-green-signal"
              >
                All zones <ArrowRight className="h-3 w-3" />
              </Link>
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
                    <p className="truncate text-xs font-medium text-white/80">{zone.zone}</p>
                    <p className="text-2xs text-white/30 font-mono">
                      {zone.delivered}/{zone.total} dispatched
                    </p>
                  </div>
                  <span
                    className="flex-shrink-0 font-mono text-xs font-bold tabular-nums"
                    style={{ color: zone.rate >= 97 ? "#00FF88" : zone.rate >= 95 ? "#00E5FF" : "#FFAB00" }}
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
              <p className="text-xs text-white/40">Earnings breakdown for the current billing cycle</p>
            </div>
            <button
              onClick={() => router.push("/payouts")}
              className="flex items-center gap-1.5 rounded-lg border border-green-signal/30 bg-green-surface px-3 py-1.5 text-xs font-medium text-green-signal transition-all hover:border-green-signal/60 hover:shadow-[0_0_10px_rgba(0,255,136,0.2)]"
            >
              <DollarSign className="h-3.5 w-3.5" />
              Request Payout
            </button>
          </div>

          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            {[
              { label: "Gross Earnings", value: fmtPhp(kpis.grossEarnings), color: "#00FF88", icon: TrendingUp    },
              { label: "Platform Fee",   value: fmtPhp(kpis.platformFee),   color: "#FF3B5C", icon: AlertTriangle },
              { label: "Net Earnings",   value: fmtPhp(kpis.netEarnings),   color: "#00E5FF", icon: DollarSign    },
              { label: "Already Paid",   value: fmtPhp(kpis.alreadyPaid),   color: "#A855F7", icon: CheckCircle2  },
            ].map(({ label, value, color, icon: Icon }) => (
              <div
                key={label}
                className="rounded-lg border p-4"
                style={{ borderColor: `${color}20`, background: `${color}06` }}
              >
                <div className="mb-2 flex items-center gap-1.5">
                  <Icon className="h-3.5 w-3.5" style={{ color }} />
                  <span className="text-xs text-white/40">{label}</span>
                </div>
                <p className="font-heading text-xl font-bold tabular-nums" style={{ color }}>
                  {value}
                </p>
              </div>
            ))}
          </div>

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
