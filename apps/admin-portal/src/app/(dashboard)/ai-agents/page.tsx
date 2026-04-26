"use client";
/**
 * Admin Portal — AI Agents Page
 *
 * LIVE:  Escalated sessions section (GET /v1/agents/sessions/escalated)
 *        — these require human review; ops resolves via the Resolve button.
 * STATIC: Agent-type KPI cards + invocation trend chart below — ai-layer
 *        doesn't yet expose per-agent-type counters or hourly buckets, so
 *        those values stay mock until backend ships the aggregation.
 *        When it does, replace AGENTS / INVOCATION_TREND / KPI with live
 *        fetches analogous to the escalated section's pattern.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import { Bot, Zap, Brain, ShieldCheck, Megaphone, Headphones, Route, RefreshCw, ArrowUpRight, AlertCircle, CheckCircle2 } from "lucide-react";
import { authFetch } from "@/lib/auth/auth-fetch";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

interface EscalatedSession {
  id: string;
  agent_type: string;
  status: string;
  outcome?: string;
  escalation_reason?: string;
  confidence_score: number;
  actions_taken: number;
  started_at: string;
}

// ── Types & data ───────────────────────────────────────────────────────────────

type AgentStatus = "running" | "idle" | "error" | "training";

interface AIAgent {
  id: string;
  name: string;
  description: string;
  status: AgentStatus;
  model: string;
  invocations_today: number;
  avg_latency_ms: number;
  success_rate: number;
  last_action: string;
  last_action_time: string;
  icon: React.ReactNode;
}

const AGENTS: AIAgent[] = [
  {
    id: "dispatch",
    name: "Dispatch Agent",
    description: "Smart driver assignment, VRP optimization, re-routing on delays",
    status: "running",
    model: "claude-opus-4-6 + ONNX VRP",
    invocations_today: 2840,
    avg_latency_ms: 320,
    success_rate: 99.1,
    last_action: "Assigned driver JDC to route R-2847 (18 stops, Makati)",
    last_action_time: "12s ago",
    icon: <Route size={18} className="text-cyan-neon" />,
  },
  {
    id: "support",
    name: "Support Agent",
    description: "WhatsApp & chat support, reschedule requests, complaint triage",
    status: "running",
    model: "claude-sonnet-4-6",
    invocations_today: 1240,
    avg_latency_ms: 890,
    success_rate: 96.4,
    last_action: "Rescheduled delivery for LS-A1B2C3 to March 18 (customer request)",
    last_action_time: "2m ago",
    icon: <Headphones size={18} className="text-purple-plasma" />,
  },
  {
    id: "marketing",
    name: "Marketing Agent",
    description: "Campaign copy generation, send-time optimization, A/B test selection",
    status: "running",
    model: "claude-sonnet-4-6",
    invocations_today: 84,
    avg_latency_ms: 1240,
    success_rate: 100,
    last_action: "Generated 3 WhatsApp variants for 'Post-Delivery Upsell' campaign",
    last_action_time: "15m ago",
    icon: <Megaphone size={18} className="text-amber-signal" />,
  },
  {
    id: "fraud",
    name: "Fraud Detection Agent",
    description: "Payment fraud scoring, COD refusal patterns, shipment anomaly detection",
    status: "running",
    model: "ONNX GradientBoost + claude-haiku-4-5",
    invocations_today: 8420,
    avg_latency_ms: 45,
    success_rate: 99.8,
    last_action: "Flagged shipment SH-992 for COD fraud review (score: 0.94)",
    last_action_time: "34s ago",
    icon: <ShieldCheck size={18} className="text-red-signal" />,
  },
  {
    id: "logistics-planner",
    name: "Logistics Planner",
    description: "Daily route planning, hub capacity optimization, demand forecasting",
    status: "idle",
    model: "claude-opus-4-6 + ONNX Forecast",
    invocations_today: 12,
    avg_latency_ms: 2800,
    success_rate: 100,
    last_action: "Generated tomorrow's route plan: 47 drivers, 1,320 stops across Metro Manila",
    last_action_time: "2h ago",
    icon: <Brain size={18} className="text-green-signal" />,
  },
  {
    id: "customer-intelligence",
    name: "Customer Intelligence",
    description: "CLV scoring, churn prediction, delivery preference modeling",
    status: "training",
    model: "ONNX XGBoost (retraining)",
    invocations_today: 0,
    avg_latency_ms: 0,
    success_rate: 0,
    last_action: "Model retraining on 90-day behavioral dataset (ETA: 23min)",
    last_action_time: "now",
    icon: <Zap size={18} className="text-purple-plasma" />,
  },
];

// Each agent gets one "inspect" deep link to the operational surface its last
// action most likely affected. Some cross into partner-portal (driver-centric
// actions), others stay on admin (dispatch, fraud alerts, analytics).
const INSPECT_LINKS: Record<string, { href: string; label: string; crossPortal: boolean }> = {
  "dispatch":               { href: "/admin/dispatch",            label: "Dispatch queue",       crossPortal: false },
  "support":                { href: "/admin/alerts",              label: "Alerts",               crossPortal: false },
  "marketing":              { href: "/admin/alerts",              label: "Alerts",               crossPortal: false },
  "fraud":                  { href: "/admin/alerts",              label: "Flagged alerts",       crossPortal: false },
  "logistics-planner":      { href: "/partner/manifests",         label: "Manifests (partner)",  crossPortal: true  },
  "customer-intelligence":  { href: "/admin/analytics",           label: "Analytics",            crossPortal: false },
};

const STATUS_CONFIG: Record<AgentStatus, { label: string; variant: "green" | "cyan" | "amber" | "red" | "purple" }> = {
  running:  { label: "Running",  variant: "green"  },
  idle:     { label: "Idle",     variant: "cyan"   },
  error:    { label: "Error",    variant: "red"    },
  training: { label: "Training", variant: "purple" },
};

const KPI = [
  { label: "Active Agents",      value: 4,     trend: 0,    color: "green"  as const, format: "number"  as const },
  { label: "Invocations Today",  value: 12600, trend: +18.4, color: "cyan"   as const, format: "number"  as const },
  { label: "Avg Latency P99",    value: 890,   trend: -12.0, color: "purple" as const, format: "number"  as const },
  { label: "Avg Success Rate",   value: 98.8,  trend: +0.4,  color: "amber"  as const, format: "percent" as const },
];

const INVOCATION_TREND = Array.from({ length: 24 }, (_, i) => ({
  hour: `${i}:00`,
  dispatch: Math.floor(80 + Math.sin(i / 4) * 40 + Math.random() * 20),
  support:  Math.floor(30 + Math.sin(i / 3) * 20 + Math.random() * 10),
  fraud:    Math.floor(200 + Math.sin(i / 5) * 80 + Math.random() * 30),
}));

interface AggregateStats {
  total_today:      number;
  completed_today:  number;
  escalated_today:  number;
  failed_today:     number;
  success_rate_pct: number;
  by_type_today:    Record<string, number>;
  hourly_24h: Array<{
    hour:    number;
    total:   number;
    by_type: Record<string, number>;
  }>;
}

export default function AIAgentsPage() {
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);

  const [escalated, setEscalated] = useState<EscalatedSession[]>([]);
  const [loadingEsc, setLoadingEsc] = useState(true);
  const [escError, setEscError]   = useState<string | null>(null);
  const [resolvingId, setResolvingId] = useState<string | null>(null);

  // Aggregate KPIs + 24h invocation breakdown — single round-trip.
  // Falls back to the static KPI/INVOCATION_TREND constants on failure
  // so the page degrades gracefully if ai-layer is unreachable.
  const [agg, setAgg] = useState<AggregateStats | null>(null);
  useEffect(() => {
    authFetch(`${API_BASE}/v1/agents/aggregate`)
      .then((res) => res.ok ? res.json() : null)
      .then((json) => { if (json?.data) setAgg(json.data); })
      .catch(() => { /* keep mock */ });
  }, []);

  const liveKpi = useMemo(() => {
    if (!agg) return KPI;
    return [
      // Active Agents = count of distinct agent_types seen today (proxy
      // for "what's actually doing work right now"). Falls back to 0 if
      // the tenant hasn't run anything today.
      { label: "Active Agents",     value: Object.keys(agg.by_type_today).length, trend: 0, color: "green"  as const, format: "number"  as const },
      { label: "Invocations Today", value: agg.total_today,                       trend: 0, color: "cyan"   as const, format: "number"  as const },
      { label: "Escalated Today",   value: agg.escalated_today,                   trend: 0, color: "purple" as const, format: "number"  as const },
      { label: "Success Rate",      value: agg.success_rate_pct,                  trend: 0, color: "amber"  as const, format: "percent" as const },
    ];
  }, [agg]);

  const liveInvocationTrend = useMemo(() => {
    if (!agg) return INVOCATION_TREND;
    return agg.hourly_24h.map((b) => ({
      hour:     `${b.hour}:00`,
      dispatch: b.by_type["dispatch"] ?? 0,
      support:  b.by_type["on_demand"] ?? 0,  // chatbot lives under on_demand for now
      fraud:    b.by_type["anomaly"] ?? 0,
    }));
  }, [agg]);

  const loadEscalated = useCallback(async () => {
    setEscError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/agents/sessions/escalated`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const json = await res.json() as { escalated?: EscalatedSession[]; count?: number };
      setEscalated(json.escalated ?? []);
    } catch (e) {
      const err = e as { message?: string };
      setEscError(err?.message ?? "Failed to load escalated sessions");
    } finally {
      setLoadingEsc(false);
    }
  }, []);

  useEffect(() => { loadEscalated(); }, [loadEscalated]);

  // Poll every 30s — escalations are human-review-critical, stale data is
  // worse than a mild API load increase.
  useEffect(() => {
    const id = setInterval(loadEscalated, 30_000);
    return () => clearInterval(id);
  }, [loadEscalated]);

  async function handleResolve(sessionId: string) {
    setResolvingId(sessionId);
    try {
      await authFetch(`${API_BASE}/v1/agents/sessions/${sessionId}/resolve`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ resolution_notes: "Resolved via admin portal" }),
      });
      await loadEscalated();
    } catch (e) {
      const err = e as { message?: string };
      setEscError(err?.message ?? "Failed to resolve session");
    } finally {
      setResolvingId(null);
    }
  }

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
            <Bot size={22} className="text-purple-plasma" />
            AI Agents
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">Agentic runtime · MCP tool dispatch · claude-opus-4-6</p>
        </div>
        <button
          onClick={loadEscalated}
          className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
        >
          <RefreshCw size={12} /> Refresh
        </button>
      </motion.div>

      {/* LIVE — Escalated sessions needing human review */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <div className="flex items-center gap-2">
              <AlertCircle size={14} className="text-red-signal" />
              <h2 className="font-heading text-sm font-semibold text-white">Escalated Sessions — Need Review</h2>
            </div>
            <NeonBadge variant={escalated.length > 0 ? "red" : "green"} dot>
              {loadingEsc ? "loading" : `${escalated.length} pending`}
            </NeonBadge>
          </div>

          {escError && (
            <p className="px-5 py-3 text-xs text-red-signal font-mono">{escError}</p>
          )}

          {!loadingEsc && escalated.length === 0 && !escError ? (
            <div className="px-5 py-8 text-center">
              <CheckCircle2 size={24} className="text-green-signal mx-auto mb-2" />
              <p className="text-xs text-white/50 font-mono">All agent sessions resolved automatically.</p>
            </div>
          ) : (
            escalated.map((s) => (
              <div key={s.id} className="grid grid-cols-[2fr_140px_80px_100px] gap-3 items-center px-5 py-3 border-b border-glass-border/50">
                <div>
                  <p className="text-xs font-medium text-white">
                    {s.agent_type} · <span className="text-white/50">{s.id.slice(0, 8)}</span>
                  </p>
                  <p className="text-2xs font-mono text-white/40 mt-0.5 line-clamp-1" title={s.escalation_reason ?? ""}>
                    {s.escalation_reason ?? "No reason provided"}
                  </p>
                </div>
                <span className="text-2xs font-mono text-white/50">
                  {new Date(s.started_at).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}
                </span>
                <span className="text-2xs font-mono text-white/60">
                  conf {Math.round(s.confidence_score * 100)}%
                </span>
                <button
                  onClick={() => handleResolve(s.id)}
                  disabled={resolvingId === s.id}
                  className="rounded-md border border-green-signal/30 bg-green-signal/10 px-2.5 py-1 text-2xs font-mono text-green-signal hover:bg-green-signal/20 disabled:opacity-40 transition-colors"
                >
                  {resolvingId === s.id ? "…" : "Resolve"}
                </button>
              </div>
            ))
          )}
        </GlassCard>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {liveKpi.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Invocation trend */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="purple">
          <div className="flex items-center justify-between mb-4">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Agent Invocations — Today (24h)</h2>
              <p className="text-2xs font-mono text-white/30">Dispatch · Support · Fraud</p>
            </div>
          </div>
          <ResponsiveContainer width="100%" height={160}>
            <AreaChart data={liveInvocationTrend} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
              <defs>
                <linearGradient id="grad-disp" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#00E5FF" stopOpacity={0.25} />
                  <stop offset="95%" stopColor="#00E5FF" stopOpacity={0}    />
                </linearGradient>
                <linearGradient id="grad-sup" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#A855F7" stopOpacity={0.25} />
                  <stop offset="95%" stopColor="#A855F7" stopOpacity={0}    />
                </linearGradient>
                <linearGradient id="grad-fraud" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#FF3B5C" stopOpacity={0.2}  />
                  <stop offset="95%" stopColor="#FF3B5C" stopOpacity={0}    />
                </linearGradient>
              </defs>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="hour" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 9, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} interval={3} />
              <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                labelStyle={{ color: "rgba(255,255,255,0.4)" }}
              />
              <Area type="monotone" dataKey="fraud"    stroke="#FF3B5C" fill="url(#grad-fraud)" strokeWidth={1.5} />
              <Area type="monotone" dataKey="dispatch" stroke="#00E5FF" fill="url(#grad-disp)"  strokeWidth={1.5} />
              <Area type="monotone" dataKey="support"  stroke="#A855F7" fill="url(#grad-sup)"   strokeWidth={1.5} />
            </AreaChart>
          </ResponsiveContainer>
        </GlassCard>
      </motion.div>

      {/* Agent cards */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        {AGENTS.map((agent) => {
          const { label, variant } = STATUS_CONFIG[agent.status];
          const isSelected = selectedAgent === agent.id;
          return (
            <GlassCard
              key={agent.id}
              className={`cursor-pointer transition-all ${isSelected ? "border-purple-plasma/40" : "hover:border-glass-border-bright"}`}
              glow={isSelected ? "purple" : undefined}
              onClick={() => setSelectedAgent(isSelected ? null : agent.id)}
            >
              <div className="flex items-start gap-3 mb-3">
                <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-xl bg-glass-200 border border-glass-border">
                  {agent.icon}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-sm font-semibold text-white truncate">{agent.name}</p>
                    <NeonBadge variant={variant} dot={agent.status === "running"}>{label}</NeonBadge>
                  </div>
                  <p className="text-2xs font-mono text-white/40 mt-0.5 truncate">{agent.model}</p>
                </div>
              </div>

              <p className="text-xs text-white/50 mb-3">{agent.description}</p>

              {/* Stats row */}
              <div className="grid grid-cols-3 gap-2 mb-3">
                {[
                  { label: "Invocations", value: agent.invocations_today > 0 ? agent.invocations_today.toLocaleString() : "—" },
                  { label: "Avg Latency", value: agent.avg_latency_ms > 0 ? `${agent.avg_latency_ms}ms` : "—" },
                  { label: "Success",     value: agent.success_rate > 0 ? `${agent.success_rate}%` : "—" },
                ].map((s) => (
                  <div key={s.label} className="rounded-lg bg-glass-100 px-2.5 py-2">
                    <p className="text-2xs font-mono text-white/30 uppercase">{s.label}</p>
                    <p className="text-sm font-bold text-white mt-0.5">{s.value}</p>
                  </div>
                ))}
              </div>

              {/* Last action */}
              <div className="rounded-lg bg-glass-100 border border-glass-border px-3 py-2">
                <p className="text-2xs font-mono text-white/30 mb-0.5">Last action · {agent.last_action_time}</p>
                <p className="text-xs text-white/60 line-clamp-2 mb-2">{agent.last_action}</p>
                {INSPECT_LINKS[agent.id] && (
                  <a
                    href={INSPECT_LINKS[agent.id].href}
                    onClick={(e) => e.stopPropagation()}
                    className={`inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-2xs font-mono transition-colors ${
                      INSPECT_LINKS[agent.id].crossPortal
                        ? "border-purple-plasma/30 bg-purple-plasma/10 text-purple-plasma hover:bg-purple-plasma/20"
                        : "border-cyan-neon/30 bg-cyan-neon/10 text-cyan-neon hover:bg-cyan-neon/20"
                    }`}
                  >
                    <ArrowUpRight size={10} />
                    {INSPECT_LINKS[agent.id].label}
                  </a>
                )}
              </div>
            </GlassCard>
          );
        })}
      </motion.div>
    </motion.div>
  );
}
