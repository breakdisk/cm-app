"use client";
/**
 * Partner Portal — SLA Dashboard
 * Real-time SLA compliance tracking per zone, shipment type, and time window.
 */
import { useState, useEffect, useCallback, useRef, Suspense } from "react";
import { useSearchParams } from "next/navigation";
import { useRosterEvents } from "@/hooks/useRosterEvents";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  BarChart, Bar, LineChart, Line,
  XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, ReferenceLine,
} from "recharts";
import { Star, AlertTriangle, CheckCircle2, Clock, GitBranch, Download, ChevronLeft, ChevronRight } from "lucide-react";
import { authFetch } from "@/lib/auth/auth-fetch";
import { carriersApi, carrierIdOf, type ZoneSlaRow, type SlaRecord } from "@/lib/api/carriers";
import { dailyOnTimeRate, totalsFromZones, type TrendPoint } from "@/lib/sla-trend";

// ── API helpers ────────────────────────────────────────────────────────────────
//
// Every figure on this page is the signed-in partner's OWN carrier data
// (`/v1/carriers/:id/sla-summary` and `/sla-history`). It used to read the
// tenant-wide analytics endpoints, which count every carrier's and merchant's
// shipments, and which a partner token is no longer allowed to call.

// Gateway routes /v1/carriers to the carrier service; NEXT_PUBLIC_CARRIER_URL was never a build arg.
const CARRIER_SVC_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

function todayStr()     { return new Date().toISOString().slice(0, 10); }
function daysAgoStr(n: number) {
  const d = new Date();
  d.setDate(d.getDate() - n);
  return d.toISOString().slice(0, 10);
}

/** How many recent delivery records to fold into the trend chart. */
const TREND_HISTORY_LIMIT = 500;

/** `2026-11-02` → `Nov 2`, in UTC so the label matches the day it was bucketed under. */
function dayLabel(isoDate: string) {
  return new Date(`${isoDate}T00:00:00Z`).toLocaleDateString("en-US", { month: "short", day: "numeric", timeZone: "UTC" });
}

// ── Constants / fallbacks ──────────────────────────────────────────────────────

/** SLA target per zone — used when the carrier's global SLA pct isn't zone-specific. */
const DEFAULT_ZONE_TARGET = 90;

const BREACH_REASONS_FALLBACK = [
  { reason: "Traffic / Road closure", count: 0 },
  { reason: "Customer unavailable",   count: 0 },
  { reason: "Wrong address",          count: 0 },
  { reason: "Vehicle breakdown",      count: 0 },
  { reason: "Weather",                count: 0 },
];

async function fetchBreachReasons(
  carrierId: string,
  from: string,
  to: string,
): Promise<Array<{ reason: string; count: number }>> {
  try {
    const res = await authFetch(
      `${CARRIER_SVC_URL}/v1/carriers/${carrierId}/breach-reasons?from=${from}T00:00:00Z&to=${to}T23:59:59Z`,
    );
    if (!res.ok) return BREACH_REASONS_FALLBACK;
    const json = await res.json();
    const items = json.data ?? json.reasons ?? [];
    if (!Array.isArray(items) || items.length === 0) return BREACH_REASONS_FALLBACK;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return items.map((r: any) => ({ reason: r.reason ?? r.label ?? "Unknown", count: Number(r.count ?? 0) }));
  } catch {
    return BREACH_REASONS_FALLBACK;
  }
}

/**
 * Trigger a client-side CSV download of the current SLA trend. No server
 * round-trip — the data is already loaded. Used by the Export button in
 * the header.
 */
function exportTrendCsv(rows: Array<{ date: string; rate: number }>) {
  if (rows.length === 0) return;
  const header = "date,sla_rate_pct";
  const lines = rows.map((r) => `${r.date},${r.rate.toFixed(2)}`);
  const csv = [header, ...lines].join("\n");
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `sla-trend-${new Date().toISOString().slice(0, 10)}.csv`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

type SlaGrade = "Excellent" | "Good" | "Fair" | "At Risk";

function getSlaGrade(rate: number, target: number): SlaGrade {
  const diff = rate - target;
  if (diff >= 2) return "Excellent";
  if (diff >= 0) return "Good";
  if (diff >= -2) return "Fair";
  return "At Risk";
}

function gradeVariant(grade: SlaGrade): "green" | "cyan" | "amber" | "red" {
  if (grade === "Excellent") return "green";
  if (grade === "Good")      return "cyan";
  if (grade === "Fair")      return "amber";
  return "red";
}

function SLADashboardPageInner() {
  const searchParams    = useSearchParams();
  const focusZone       = searchParams.get("zone");
  const focusRowRef     = useRef<HTMLDivElement | null>(null);

  // Start empty, not with sample numbers: a figure on this page is either the
  // partner's real one or absent.
  const [overallSla, setOverallSla]       = useState<number | null>(null);
  const [onTimeCount, setOnTimeCount]     = useState<number>(0);
  const [breachCount, setBreachCount]     = useState<number>(0);
  const [trackedCount, setTrackedCount]   = useState<number>(0);
  const [trendData, setTrendData]         = useState<Array<{ date: string; rate: number }>>([]);
  const [loaded, setLoaded]               = useState<boolean>(false);
  const [zoneSla, setZoneSla]             = useState<ZoneSlaRow[]>([]);
  const [slaTarget, setSlaTarget]         = useState<number>(DEFAULT_ZONE_TARGET);
  const [carrierName, setCarrierName]     = useState<string | null>(null);
  const [carrierId,   setCarrierId]       = useState<string | null>(null);

  const [breachReasons, setBreachReasons] = useState<Array<{ reason: string; count: number }>>(BREACH_REASONS_FALLBACK);

  // Delivery history pagination
  const [history,      setHistory]        = useState<SlaRecord[]>([]);
  const [historyTotal, setHistoryTotal]   = useState<number>(0);
  const [historyPage,  setHistoryPage]    = useState<number>(0);
  const HISTORY_PAGE_SIZE = 15;

  useEffect(() => {
    if (focusZone && focusRowRef.current) {
      focusRowRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [focusZone]);

  const loadData = useCallback(async () => {
    // Resolve the partner's own carrier, then read its SLA summary (30 days),
    // its recent delivery history (the trend) and its breach reasons.
    try {
      const carrier = await carriersApi.me();
      const cid = carrierIdOf(carrier);
      setSlaTarget(carrier.sla.on_time_target_pct);
      setCarrierName(carrier.name);
      setCarrierId(cid);
      if (cid) {
        const from = new Date();
        from.setDate(from.getDate() - 30);
        const since = from.toISOString().slice(0, 10);
        const [zones, breachData, trendHistory] = await Promise.all([
          carriersApi.slaSummary(cid, from.toISOString(), new Date().toISOString()),
          fetchBreachReasons(cid, daysAgoStr(30), todayStr()),
          carriersApi.slaHistory(cid, TREND_HISTORY_LIMIT, 0),
        ]);
        setZoneSla(zones);
        setBreachReasons(breachData);

        const totals = totalsFromZones(zones);
        setOverallSla(totals.ratePct);
        setOnTimeCount(totals.onTime);
        setBreachCount(totals.failed);
        setTrackedCount(totals.total);

        // Only the last 30 days, to match the summary the figures above use.
        const recent = trendHistory.records.filter((r) => (r.delivered_at ?? r.created_at).slice(0, 10) >= since);
        setTrendData(dailyOnTimeRate(recent).map((p: TrendPoint) => ({ date: dayLabel(p.date), rate: p.rate })));
      }
    } catch (e) {
      // Non-fatal — the page keeps its empty state rather than invented numbers.
      console.warn("Failed to load SLA data:", e);
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => { loadData(); }, [loadData]);

  const loadHistory = useCallback(async () => {
    if (!carrierId) return;
    try {
      const resp = await carriersApi.slaHistory(carrierId, HISTORY_PAGE_SIZE, historyPage * HISTORY_PAGE_SIZE);
      setHistory(resp.records);
      setHistoryTotal(resp.count);
    } catch {
      // Non-fatal — table stays empty
    }
  }, [carrierId, historyPage]);

  useEffect(() => { loadHistory(); }, [loadHistory]);

  // SLA rate moves on every delivery completion / failure, which correlates with
  // driver status transitions (en_route → returning/available). Refetch opportunistically
  // on roster events, with a 60s poll backstop.
  useRosterEvents((event) => {
    if (event.type === "status_changed") loadData();
  });
  useEffect(() => {
    const id = setInterval(loadData, 60_000);
    return () => clearInterval(id);
  }, [loadData]);

  // No trend arrows: there is no prior-period figure to compare against, and a
  // made-up percentage change is worse than none.
  const KPI = [
    // null (nothing decided yet) shows a dash, never a 0% that reads as a result.
    { label: "Overall SLA",         value: overallSla,      color: "green"  as const, format: "percent" as const },
    { label: "On-Time Deliveries",  value: onTimeCount,     color: "cyan"   as const, format: "number"  as const },
    { label: "SLA Breaches (30d)",  value: breachCount,     color: "red"    as const, format: "number"  as const },
    { label: "Shipments Tracked",   value: trackedCount,    color: "purple" as const, format: "number"  as const },
  ];
  const nothingTracked = loaded && trackedCount === 0;

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
            <Star size={20} className="text-purple-plasma" />
            SLA Dashboard
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {carrierName ?? "—"} · {new Date().toLocaleString("en-PH", { month: "long", year: "numeric" })} · Contract SLA: {slaTarget}% on-time
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => exportTrendCsv(trendData)}
            disabled={trendData.length === 0}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-40"
            title="Download 30-day SLA trend as CSV"
          >
            <Download size={12} /> Export CSV
          </button>
          <NeonBadge variant="green" dot>Live</NeonBadge>
        </div>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {KPI.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            {m.value === null ? (
              <div className="flex flex-col gap-1">
                <span className="text-2xs font-mono uppercase tracking-wider text-white/40">{m.label}</span>
                <span className="font-heading text-3xl font-bold text-white/30" aria-label="no data yet">—</span>
              </div>
            ) : (
              <LiveMetric label={m.label} value={m.value} color={m.color} format={m.format} />
            )}
          </GlassCard>
        ))}
      </motion.div>
      {nothingTracked && (
        <p className="-mt-2 text-xs font-mono text-white/40" role="status">
          No shipments tracked for your carrier in the last 30 days yet. Figures appear here as deliveries complete.
        </p>
      )}

      {/* SLA trend */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="green">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">
                SLA Compliance Trend — {new Date().toLocaleString("en-PH", { month: "long", year: "numeric" })}
              </h2>
              <p className="text-2xs font-mono text-white/30">Contract target: {slaTarget}%</p>
            </div>
            <CheckCircle2 size={15} className="text-green-signal" />
          </div>
          {trendData.length === 0 ? (
            <div className="flex h-[180px] items-center justify-center">
              <p className="text-xs font-mono text-white/30">
                {loaded ? "No completed deliveries in the last 30 days." : "loading…"}
              </p>
            </div>
          ) : (
          <ResponsiveContainer width="100%" height={180}>
            <LineChart data={trendData} margin={{ top: 10, right: 10, bottom: 0, left: -24 }}>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="date" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <YAxis domain={[Math.max(0, Math.min(slaTarget, ...trendData.map((d) => d.rate)) - 5), 100]} tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                formatter={(v) => [`${v}%`, "SLA Rate"]}
              />
              <ReferenceLine y={slaTarget} stroke="rgba(255,171,0,0.4)" strokeDasharray="4 4" label={{ value: `Target ${slaTarget}%`, fill: "rgba(255,171,0,0.6)", fontSize: 10 }} />
              <Line type="monotone" dataKey="rate" stroke="#00FF88" strokeWidth={2} dot={{ fill: "#00FF88", r: 3 }} activeDot={{ r: 5 }} />
            </LineChart>
          </ResponsiveContainer>
          )}
        </GlassCard>
      </motion.div>

      {/* Zone breakdown */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          <div className="flex items-center justify-between mb-5">
            <h2 className="font-heading text-sm font-semibold text-white">SLA by Zone & Day Window</h2>
            <AlertTriangle size={14} className="text-amber-signal" />
          </div>

          {/* Table header */}
          <div className="grid grid-cols-[1fr_72px_72px_72px_88px_80px] gap-3 mb-2 px-1">
            {["Zone", "Total", "On-Time", "Failed", "Rate", "Grade"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {zoneSla.length === 0 ? (
            <p className="text-xs font-mono text-white/30 py-6 text-center">
              No SLA data yet — records populate as shipments complete.
            </p>
          ) : (
            <div className="flex flex-col gap-2">
              {zoneSla.map((z) => {
                const grade     = getSlaGrade(z.on_time_rate, slaTarget);
                const v         = gradeVariant(grade);
                const isFocused = focusZone && z.zone.toLowerCase().includes(focusZone.toLowerCase());
                return (
                  <div
                    key={z.zone}
                    ref={isFocused ? focusRowRef : undefined}
                    className={`grid grid-cols-[1fr_72px_72px_72px_88px_80px] gap-3 items-center rounded-lg bg-glass-100 px-3 py-3 transition-all ${
                      isFocused ? "ring-1 ring-cyan-neon/50 bg-cyan-neon/5" : ""
                    }`}
                  >
                    <div className="flex items-start gap-2">
                      <div>
                        <p className="text-xs font-medium text-white">{z.zone}</p>
                        <p className="text-2xs font-mono text-white/30">Target: {slaTarget}%</p>
                      </div>
                      <a
                        href={`/admin/carriers?coverage=${encodeURIComponent(z.zone)}`}
                        title="View carriers serving this zone"
                        className="inline-flex h-5 w-5 items-center justify-center rounded-md border border-glass-border text-white/40 hover:text-purple-plasma hover:border-purple-plasma/30 transition-colors"
                      >
                        <GitBranch size={10} />
                      </a>
                    </div>
                    <span className="text-xs font-mono text-white/60">{z.total}</span>
                    <span className="text-xs font-mono text-green-signal">{z.on_time}</span>
                    <span className="text-xs font-mono text-red-signal">{z.failed}</span>
                    <span
                      className={`text-xs font-mono font-bold ${
                        z.on_time_rate >= slaTarget       ? "text-green-signal" :
                        z.on_time_rate >= slaTarget - 3   ? "text-amber-signal" : "text-red-signal"
                      }`}
                    >
                      {z.on_time_rate.toFixed(1)}%
                    </span>
                    <NeonBadge variant={v}>{grade}</NeonBadge>
                  </div>
                );
              })}
            </div>
          )}
        </GlassCard>
      </motion.div>

      {/* Breach reasons */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="red">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">SLA Breach Root Causes</h2>
              <p className="text-2xs font-mono text-white/30">{breachCount} breaches MTD · last 30 days</p>
            </div>
            <Clock size={14} className="text-red-signal" />
          </div>
          <ResponsiveContainer width="100%" height={160}>
            <BarChart data={breachReasons} layout="vertical" margin={{ top: 0, right: 20, bottom: 0, left: 0 }}>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" horizontal={false} />
              <XAxis type="number" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <YAxis type="category" dataKey="reason" tick={{ fill: "rgba(255,255,255,0.5)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} width={140} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
              />
              <Bar dataKey="count" fill="#FF3B5C" radius={[0,4,4,0]} fillOpacity={0.8} />
            </BarChart>
          </ResponsiveContainer>
        </GlassCard>
      </motion.div>

      {/* Delivery History */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Delivery History</h2>
              <p className="text-2xs font-mono text-white/30">
                {historyTotal > 0 ? `${historyTotal} records · page ${historyPage + 1} of ${Math.ceil(historyTotal / HISTORY_PAGE_SIZE)}` : "No records yet"}
              </p>
            </div>
            <div className="flex items-center gap-1.5">
              <button
                disabled={historyPage === 0}
                onClick={() => setHistoryPage((p) => Math.max(0, p - 1))}
                className="inline-flex h-7 w-7 items-center justify-center rounded-md border border-glass-border text-white/40 hover:text-white disabled:opacity-30 transition-colors"
              >
                <ChevronLeft size={13} />
              </button>
              <button
                disabled={(historyPage + 1) * HISTORY_PAGE_SIZE >= historyTotal}
                onClick={() => setHistoryPage((p) => p + 1)}
                className="inline-flex h-7 w-7 items-center justify-center rounded-md border border-glass-border text-white/40 hover:text-white disabled:opacity-30 transition-colors"
              >
                <ChevronRight size={13} />
              </button>
            </div>
          </div>

          {/* Table header */}
          <div className="grid grid-cols-[1fr_90px_90px_90px_80px_90px] gap-3 px-5 py-2 border-b border-glass-border">
            {["Shipment", "Zone", "Service", "Promised By", "Delivered", "Status"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {history.length === 0 ? (
            <p className="text-xs font-mono text-white/30 py-8 text-center">
              No delivery records yet — they appear here as shipments complete.
            </p>
          ) : (
            history.map((r) => {
              const statusColor = r.status === "delivered"
                ? r.on_time ? "text-green-signal" : "text-amber-signal"
                : r.status === "failed" ? "text-red-signal" : "text-white/40";
              const statusLabel = r.status === "delivered"
                ? r.on_time ? "On Time" : "Late"
                : r.status === "failed" ? "Failed" : "In Transit";
              return (
                <div
                  key={r.id}
                  className="grid grid-cols-[1fr_90px_90px_90px_80px_90px] gap-3 items-center px-5 py-3 border-b border-glass-border/40 hover:bg-glass-100 transition-colors"
                >
                  <span className="text-2xs font-mono text-white/50 truncate" title={r.shipment_id}>
                    {r.shipment_id.slice(0, 8)}…
                  </span>
                  <span className="text-xs font-mono text-white/60 truncate">{r.zone}</span>
                  <NeonBadge variant={r.service_level === "same_day" ? "cyan" : r.service_level === "next_day" ? "purple" : "amber"}>
                    {r.service_level.replace("_", " ")}
                  </NeonBadge>
                  <span className="text-2xs font-mono text-white/40">
                    {new Date(r.promised_by).toLocaleDateString("en-PH", { month: "short", day: "numeric" })}
                  </span>
                  <span className="text-2xs font-mono text-white/40">
                    {r.delivered_at
                      ? new Date(r.delivered_at).toLocaleDateString("en-PH", { month: "short", day: "numeric" })
                      : "—"}
                  </span>
                  <span className={`text-xs font-mono font-semibold ${statusColor}`}>{statusLabel}</span>
                </div>
              );
            })
          )}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}

export default function SLADashboardPage() {
  return (
    <Suspense fallback={null}>
      <SLADashboardPageInner />
    </Suspense>
  );
}
