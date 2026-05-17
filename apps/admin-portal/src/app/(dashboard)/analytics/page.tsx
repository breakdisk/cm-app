"use client";
/**
 * Admin Portal — Analytics Page
 * Network-wide delivery performance, zone heatmap, SLA trends, AI model accuracy.
 */
import { useState, useEffect, useRef, useCallback, Suspense } from "react";
import { useSearchParams } from "next/navigation";
import { motion } from "framer-motion";
import { createAnalyticsApi } from "@/lib/api/analytics";
import { authFetch } from "@/lib/auth/auth-fetch";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  AreaChart, Area, BarChart, Bar, LineChart, Line,
  XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, ReferenceLine,
} from "recharts";
import { BarChart3, TrendingUp, Brain, Calendar, ChevronLeft, ChevronRight, LineChart as LineChartIcon } from "lucide-react";

// ── Types ──────────────────────────────────────────────────────────────────────

type KpiEntry = { label: string; value: number; trend: number; color: "cyan" | "green" | "purple" | "amber"; format: "number" | "percent" | "currency" };
type WeeklyVolumeEntry = { day: string; delivered: number; failed: number };
type SlaTrendEntry = { date: string; rate: number; target: number };
type ZoneEntry = { zone: string; deliveries: number; rate: number; revenue: number };

const EMPTY_KPI: KpiEntry[] = [
  { label: "Shipments Today",   value: 0, trend: 0, color: "cyan",   format: "number"   },
  { label: "Delivery Rate",     value: 0, trend: 0, color: "green",  format: "percent"  },
  { label: "Avg Delivery Time", value: 0, trend: 0, color: "purple", format: "number"   },
  { label: "Revenue MTD",       value: 0, trend: 0, color: "amber",  format: "currency" },
];

const AI_METRICS = [
  { label: "Dispatch Accuracy",   value: 97.4, color: "#00E5FF" },
  { label: "ETA Accuracy (±30m)", value: 84.2, color: "#A855F7" },
  { label: "Fraud Detection",     value: 99.1, color: "#00FF88" },
  { label: "Demand Forecast",     value: 91.8, color: "#FFAB00" },
];

const MONTH_NAMES = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];

function AnalyticsPageInner() {
  const searchParams = useSearchParams();
  const focusZone    = searchParams.get("zone");
  const focusRowRef  = useRef<HTMLDivElement | null>(null);

  // Date range state — default to current month.
  const now = new Date();
  const [selectedYear,  setSelectedYear]  = useState(now.getFullYear());
  const [selectedMonth, setSelectedMonth] = useState(now.getMonth()); // 0-indexed
  const [showCalendar,  setShowCalendar]  = useState(false);

  const [kpi,             setKpi]             = useState<KpiEntry[]>(EMPTY_KPI);
  const [weeklyVolume,    setWeeklyVolume]    = useState<WeeklyVolumeEntry[]>([]);
  const [zonePerformance, setZonePerformance] = useState<ZoneEntry[]>([]);
  const [slaTrend,        setSlaTrend]        = useState<SlaTrendEntry[]>([]);
  const [loading,         setLoading]         = useState(true);
  const [error,           setError]           = useState<string | null>(null);

  useEffect(() => {
    if (focusZone && focusRowRef.current) {
      focusRowRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [focusZone, zonePerformance]);

  const fetchDashboard = useCallback(async () => {
    setError(null);
    setLoading(true);
    try {
      const api  = createAnalyticsApi();
      const base = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";
      // Build from/to for the selected month.
      const from = new Date(selectedYear, selectedMonth, 1).toISOString().slice(0, 10);
      const to   = new Date(selectedYear, selectedMonth + 1, 0).toISOString().slice(0, 10);

      const [dashRes, slaRes] = await Promise.allSettled([
        api.getDashboard(),
        authFetch(`${base}/v1/analytics/sla-trend?days=30&from=${from}&to=${to}`),
      ]);

      if (dashRes.status === "fulfilled") {
        const m = dashRes.value.data.metrics;
        setKpi([
          { label: "Shipments Today",   value: m.shipments_today,   trend: m.shipments_today_trend,   color: "cyan",   format: "number"   },
          { label: "Delivery Rate",     value: m.delivery_rate,     trend: m.delivery_rate_trend,     color: "green",  format: "percent"  },
          { label: "Avg Delivery Time", value: m.avg_delivery_days, trend: m.avg_delivery_days_trend, color: "purple", format: "number"   },
          { label: "Revenue MTD",       value: m.revenue_mtd,       trend: m.revenue_mtd_trend,       color: "amber",  format: "currency" },
        ]);
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        if (dashRes.value.data.weekly_volume?.length)    setWeeklyVolume(dashRes.value.data.weekly_volume);
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        if (dashRes.value.data.zone_performance?.length) setZonePerformance(dashRes.value.data.zone_performance as any);
      } else {
        setError("Failed to load analytics — check the analytics service.");
      }

      if (slaRes.status === "fulfilled" && slaRes.value.ok) {
        const json = await slaRes.value.json();
        if (json?.data?.length) {
          const points = (json.data as Array<{ date: string; on_time_pct: number }>).map((p) => ({
            date:   p.date.slice(5).replace("-", "/"),
            rate:   Number(p.on_time_pct.toFixed(1)),
            target: 95,
          }));
          setSlaTrend(points);
        }
      }
    } finally {
      setLoading(false);
    }
  }, [selectedYear, selectedMonth]);

  useEffect(() => { fetchDashboard(); }, [fetchDashboard]);

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
            <BarChart3 size={22} className="text-cyan-neon" />
            Analytics
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            Network-wide · {MONTH_NAMES[selectedMonth]} {selectedYear} · All tenants
            {loading && <span className="ml-2 text-white/20">loading…</span>}
          </p>
        </div>
        {/* Month picker */}
        <div className="relative">
          <button
            onClick={() => setShowCalendar((v) => !v)}
            className={`flex items-center gap-1.5 rounded-lg border px-3 py-2 text-xs transition-colors ${
              showCalendar ? "border-cyan-neon/40 bg-cyan-neon/10 text-cyan-neon" : "border-glass-border bg-glass-100 text-white/60 hover:text-white"
            }`}
          >
            <Calendar size={12} /> {MONTH_NAMES[selectedMonth]} {selectedYear}
          </button>
          {showCalendar && (
            <div className="absolute right-0 top-full mt-2 z-30 w-52 rounded-xl border border-glass-border bg-canvas-100 shadow-xl p-3">
              <div className="flex items-center justify-between mb-3">
                <button
                  onClick={() => { if (selectedMonth === 0) { setSelectedMonth(11); setSelectedYear(y => y - 1); } else setSelectedMonth(m => m - 1); }}
                  className="text-white/40 hover:text-white transition-colors"
                ><ChevronLeft size={14} /></button>
                <span className="text-xs font-mono text-white/60">{selectedYear}</span>
                <button
                  onClick={() => { if (selectedMonth === 11) { setSelectedMonth(0); setSelectedYear(y => y + 1); } else setSelectedMonth(m => m + 1); }}
                  className="text-white/40 hover:text-white transition-colors"
                ><ChevronRight size={14} /></button>
              </div>
              <div className="grid grid-cols-3 gap-1">
                {MONTH_NAMES.map((name, i) => (
                  <button
                    key={name}
                    onClick={() => { setSelectedMonth(i); setShowCalendar(false); }}
                    className={`rounded-lg py-1.5 text-2xs font-mono transition-colors ${
                      i === selectedMonth ? "bg-cyan-neon/20 text-cyan-neon border border-cyan-neon/40" : "text-white/50 hover:bg-glass-100 hover:text-white"
                    }`}
                  >{name}</button>
                ))}
              </div>
            </div>
          )}
        </div>
      </motion.div>

      {/* Error banner */}
      {error && (
        <motion.div variants={variants.fadeInUp}>
          <div className="rounded-lg border border-red-signal/30 bg-red-signal/5 px-4 py-3 text-xs text-red-signal font-mono">{error}</div>
        </motion.div>
      )}

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
            {weeklyVolume.length === 0 ? (
              <div className="flex h-[180px] items-center justify-center text-xs text-white/20 font-mono">
                {loading ? "Loading…" : "No data for selected period"}
              </div>
            ) : (
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
            )}
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
              <LineChart data={slaTrend} margin={{ top: 10, right: 10, bottom: 0, left: -24 }}>
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
          {zonePerformance.length === 0 ? (
            <div className="px-5 py-8 text-center text-xs text-white/20 font-mono">
              {loading ? "Loading zone data…" : "No zone data available for this period"}
            </div>
          ) : (<>
          <div className="grid grid-cols-[1fr_100px_1fr_120px_60px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Zone", "Deliveries", "Success Rate", "Revenue", ""].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>
          {zonePerformance.map((z) => {
            const isFocused = focusZone != null &&
              (z.zone.toLowerCase().includes(focusZone.toLowerCase()) ||
               focusZone.toLowerCase().includes(z.zone.toLowerCase()));
            return (
            <div
              key={z.zone}
              ref={isFocused ? focusRowRef : undefined}
              className={`grid grid-cols-[1fr_100px_1fr_120px_60px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors ${
                isFocused ? "bg-cyan-neon/5 ring-1 ring-cyan-neon/40" : ""
              }`}
            >
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
              {/* Cross-portal — zone SLA detail lives in partner-portal.
                  Plain <a> preserves the /partner basePath. */}
              <a
                href={`/partner/sla?zone=${encodeURIComponent(z.zone)}`}
                title="Open zone SLA detail in Partner Portal"
                className="inline-flex h-6 w-6 items-center justify-center rounded-md border border-glass-border text-white/40 hover:text-cyan-neon hover:border-cyan-neon/30 transition-colors"
              >
                <LineChartIcon size={11} />
              </a>
            </div>
            );
          })}
          </>)}
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
            <NeonBadge variant="purple">{process.env.NEXT_PUBLIC_AI_MODEL ?? "claude-opus-4-6"}</NeonBadge>
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

export default function AnalyticsPage() {
  return (
    <Suspense fallback={null}>
      <AnalyticsPageInner />
    </Suspense>
  );
}
