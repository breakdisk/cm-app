"use client";
/**
 * Partner Portal — SLA Dashboard
 * Real-time SLA compliance tracking per zone, shipment type, and time window.
 */
import { useState, useEffect, useCallback, useRef } from "react";
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
import { Star, AlertTriangle, CheckCircle2, Clock } from "lucide-react";
import { authFetch } from "@/lib/auth/auth-fetch";

// ── API helpers ────────────────────────────────────────────────────────────────

const ANALYTICS_URL = process.env.NEXT_PUBLIC_ANALYTICS_URL ?? "http://localhost:8013";

function todayStr()     { return new Date().toISOString().slice(0, 10); }
function daysAgoStr(n: number) {
  const d = new Date();
  d.setDate(d.getDate() - n);
  return d.toISOString().slice(0, 10);
}

async function fetchKpis() {
  try {
    const res = await authFetch(
      `${ANALYTICS_URL}/v1/analytics/kpis?from=${daysAgoStr(30)}&to=${todayStr()}`,
    );
    if (!res.ok) return null;
    const json = await res.json();
    return json.data ?? json;
  } catch {
    return null;
  }
}

async function fetchTimeseries() {
  try {
    const res = await authFetch(
      `${ANALYTICS_URL}/v1/analytics/timeseries?from=${daysAgoStr(30)}&to=${todayStr()}`,
    );
    if (!res.ok) return null;
    const json = await res.json();
    return json.data?.buckets ?? json.data ?? null;
  } catch {
    return null;
  }
}

// ── Mock data ──────────────────────────────────────────────────────────────────

const ZONE_SLA = [
  { zone: "Metro Manila",    d1: 82.4, d2: 96.2, d3: 99.1, breach: 18,  target: 95 },
  { zone: "Luzon Provinces", d1: 54.1, d2: 87.4, d3: 94.1, breach: 42,  target: 90 },
  { zone: "Visayas",         d1: 38.2, d2: 76.8, d3: 91.8, breach: 64,  target: 88 },
  { zone: "Mindanao",        d1: 21.4, d2: 61.3, d3: 88.4, breach: 112, target: 85 },
];

const BREACH_REASONS = [
  { reason: "Traffic / Road closure", count: 184 },
  { reason: "Customer unavailable",   count: 142 },
  { reason: "Wrong address",          count: 76  },
  { reason: "Vehicle breakdown",      count: 38  },
  { reason: "Weather",                count: 22  },
];

const DAILY_SLA_TREND_DEFAULT = [
  { date: "Mar 1",  rate: 93.2 }, { date: "Mar 3",  rate: 94.1 },
  { date: "Mar 5",  rate: 92.8 }, { date: "Mar 7",  rate: 95.4 },
  { date: "Mar 9",  rate: 93.7 }, { date: "Mar 11", rate: 94.8 },
  { date: "Mar 13", rate: 96.1 }, { date: "Mar 15", rate: 95.2 },
  { date: "Mar 17", rate: 94.8 },
];

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

export default function SLADashboardPage() {
  const searchParams    = useSearchParams();
  const focusZone       = searchParams.get("zone");
  const focusRowRef     = useRef<HTMLDivElement | null>(null);

  const [overallSla, setOverallSla]       = useState<number>(94.8);
  const [onTimeCount, setOnTimeCount]     = useState<number>(8412);
  const [breachCount, setBreachCount]     = useState<number>(462);
  const [avgDays, setAvgDays]             = useState<number>(1.8);
  const [trendData, setTrendData]         = useState(DAILY_SLA_TREND_DEFAULT);

  useEffect(() => {
    if (focusZone && focusRowRef.current) {
      focusRowRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [focusZone]);

  const loadData = useCallback(async () => {
    const [kpis, timeseries] = await Promise.all([fetchKpis(), fetchTimeseries()]);

    if (kpis) {
      if (kpis.delivery_success_rate != null)  setOverallSla(Number(kpis.delivery_success_rate));
      if (kpis.delivered != null)              setOnTimeCount(Number(kpis.delivered));
      if (kpis.failed != null)                 setBreachCount(Number(kpis.failed));
      if (kpis.avg_delivery_hours != null)     setAvgDays(Number(kpis.avg_delivery_hours) / 24);
    }

    if (timeseries && Array.isArray(timeseries) && timeseries.length > 0) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const trend = timeseries.map((b: any) => ({
        date: b.date,
        rate: b.delivered > 0 ? Math.round((b.delivered / (b.delivered + b.failed)) * 100) : 100,
      }));
      setTrendData(trend);
    }
  }, []);

  useEffect(() => { loadData(); }, [loadData]);

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

  const KPI = [
    { label: "Overall SLA",        value: overallSla,  trend: +1.2,  color: "green"  as const, format: "percent" as const },
    { label: "On-Time Deliveries", value: onTimeCount, trend: +8.4,  color: "cyan"   as const, format: "number"  as const },
    { label: "SLA Breaches MTD",   value: breachCount, trend: -18.2, color: "red"    as const, format: "number"  as const },
    { label: "Avg Days to Deliver",value: avgDays,     trend: -0.2,  color: "purple" as const, format: "number"  as const },
  ];

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
          <p className="text-sm text-white/40 font-mono mt-0.5">FastLine Couriers · March 2026 · Contract SLA: 95% on-time</p>
        </div>
        <NeonBadge variant="green" dot>Live</NeonBadge>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {KPI.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* SLA trend */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="green">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">SLA Compliance Trend — March 2026</h2>
              <p className="text-2xs font-mono text-white/30">Contract target: 95%</p>
            </div>
            <CheckCircle2 size={15} className="text-green-signal" />
          </div>
          <ResponsiveContainer width="100%" height={180}>
            <LineChart data={trendData} margin={{ top: 10, right: 10, bottom: 0, left: -24 }}>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="date" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <YAxis domain={[85, 100]} tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                formatter={(v) => [`${v}%`, "SLA Rate"]}
              />
              <ReferenceLine y={95} stroke="rgba(255,171,0,0.4)" strokeDasharray="4 4" label={{ value: "Target 95%", fill: "rgba(255,171,0,0.6)", fontSize: 10 }} />
              <Line type="monotone" dataKey="rate" stroke="#00FF88" strokeWidth={2} dot={{ fill: "#00FF88", r: 3 }} activeDot={{ r: 5 }} />
            </LineChart>
          </ResponsiveContainer>
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
          <div className="grid grid-cols-[1fr_80px_80px_80px_80px_80px] gap-3 mb-2 px-1">
            {["Zone", "D+1", "D+2", "D+3", "Breaches", "Grade"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          <div className="flex flex-col gap-2">
            {ZONE_SLA.map((z) => {
              const grade     = getSlaGrade(z.d3, z.target);
              const v         = gradeVariant(grade);
              const isFocused = focusZone && z.zone.toLowerCase().includes(focusZone.toLowerCase());
              return (
                <div
                  key={z.zone}
                  ref={isFocused ? focusRowRef : undefined}
                  className={`grid grid-cols-[1fr_80px_80px_80px_80px_80px] gap-3 items-center rounded-lg bg-glass-100 px-3 py-3 transition-all ${
                    isFocused ? "ring-1 ring-cyan-neon/50 bg-cyan-neon/5" : ""
                  }`}
                >
                  <div>
                    <p className="text-xs font-medium text-white">{z.zone}</p>
                    <p className="text-2xs font-mono text-white/30">Target: {z.target}%</p>
                  </div>
                  {[z.d1, z.d2, z.d3].map((rate, i) => (
                    <span
                      key={i}
                      className={`text-xs font-mono font-bold ${
                        rate >= z.target     ? "text-green-signal" :
                        rate >= z.target - 3 ? "text-amber-signal" : "text-red-signal"
                      }`}
                    >
                      {rate}%
                    </span>
                  ))}
                  <span className="text-xs font-mono text-red-signal">{z.breach}</span>
                  <NeonBadge variant={v}>{grade}</NeonBadge>
                </div>
              );
            })}
          </div>
        </GlassCard>
      </motion.div>

      {/* Breach reasons */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="red">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">SLA Breach Root Causes</h2>
              <p className="text-2xs font-mono text-white/30">{breachCount} breaches MTD</p>
            </div>
            <Clock size={14} className="text-red-signal" />
          </div>
          <ResponsiveContainer width="100%" height={160}>
            <BarChart data={BREACH_REASONS} layout="vertical" margin={{ top: 0, right: 20, bottom: 0, left: 0 }}>
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
    </motion.div>
  );
}
