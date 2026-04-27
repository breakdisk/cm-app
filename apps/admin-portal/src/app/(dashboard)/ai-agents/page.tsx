"use client";
/**
 * Admin Portal — AI Agents Page
 *
 * Fully live. Three round-trips populate the page:
 *   1. GET /v1/agents/aggregate            → KPI counters + 24h chart buckets
 *   2. GET /v1/agents/sessions?limit=200   → derived per-agent metrics
 *      (last action, success rate, avg latency)
 *   3. GET /v1/agents/sessions/escalated   → human-review queue
 *
 * The static AGENT_CATALOG in this file mirrors AgentType from
 * services/ai-layer/src/domain/entities/mod.rs — name, description, icon,
 * default model, inspect deep-link. Everything else is computed from
 * the session/aggregate APIs above.
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
import { Bot, Zap, Brain, ShieldCheck, Headphones, Route, RefreshCw, ArrowUpRight, AlertCircle, CheckCircle2, Sparkles } from "lucide-react";
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

/** Session summary as returned by GET /v1/agents/sessions. */
interface SessionSummary {
  id:                string;
  agent_type:        string;
  status:            string;   // "running" | "completed" | "human_escalated" | "failed"
  outcome?:          string | null;
  escalation_reason?: string | null;
  confidence_score:  number;
  actions_taken:     number;
  started_at:        string;
  completed_at?:     string | null;
}

// ── Types & data ───────────────────────────────────────────────────────────────

type AgentStatus = "running" | "idle" | "error";

/** Static catalog — descriptions, icons, and inspect links for each backend
 *  AgentType (see services/ai-layer/src/domain/entities/mod.rs). All metrics
 *  (invocations, latency, success, last action) are derived live from sessions. */
interface AgentCatalogEntry {
  /** Snake-case agent_type from the backend (matches AgentType enum serde). */
  type:        string;
  name:        string;
  description: string;
  /** Default model — ai-layer doesn't expose this per session yet, so it's
   *  shown as the configured default until the field flows through. */
  model:       string;
  icon:        React.ReactNode;
  inspect:     { href: string; label: string; crossPortal: boolean };
}

const AGENT_CATALOG: AgentCatalogEntry[] = [
  {
    type: "dispatch",
    name: "Dispatch Agent",
    description: "Watches shipment.created → assigns optimal driver, optimizes routes, reroutes on delays",
    model: "claude-opus-4-6 + ONNX VRP",
    icon: <Route size={18} className="text-cyan-neon" />,
    inspect: { href: "/dispatch", label: "Dispatch queue", crossPortal: false },
  },
  {
    type: "recovery",
    name: "Recovery Agent",
    description: "Watches delivery.failed → reschedules, notifies, applies SLA penalties",
    model: "claude-sonnet-4-6",
    icon: <Headphones size={18} className="text-purple-plasma" />,
    inspect: { href: "/alerts", label: "Failed deliveries", crossPortal: false },
  },
  {
    type: "reconciliation",
    name: "Reconciliation Agent",
    description: "Detects COD collections un-credited > 24h, triggers wallet credit",
    model: "claude-sonnet-4-6",
    icon: <Sparkles size={18} className="text-green-signal" />,
    inspect: { href: "/finance", label: "COD reconciliation", crossPortal: false },
  },
  {
    type: "anomaly",
    name: "Anomaly Detection Agent",
    description: "Streams analytics → flags unusual patterns, pages ops team",
    model: "ONNX GradientBoost + claude-haiku-4-5",
    icon: <ShieldCheck size={18} className="text-red-signal" />,
    inspect: { href: "/alerts", label: "Flagged alerts", crossPortal: false },
  },
  {
    type: "merchant_support",
    name: "Merchant Support Agent",
    description: "Answers merchant queries about shipments, billing, performance",
    model: "claude-sonnet-4-6",
    icon: <Brain size={18} className="text-amber-signal" />,
    inspect: { href: "/analytics", label: "Analytics", crossPortal: false },
  },
  {
    type: "on_demand",
    name: "On-Demand Agent",
    description: "Free-form agent triggered by humans or API callers",
    model: "claude-opus-4-6",
    icon: <Zap size={18} className="text-purple-plasma" />,
    inspect: { href: "/ai-agents", label: "Sessions", crossPortal: false },
  },
];

interface DerivedAgentStats {
  invocations_today: number;
  avg_latency_ms:    number;   // -1 if no completed sessions yet
  success_rate:      number;   // -1 if no terminal sessions yet
  last_action:       string;
  last_action_time:  string;   // human-friendly "12s ago"
  status:            AgentStatus;
}

function statusFromSessions(invocations: number, anyRunning: boolean, anyFailed: boolean): AgentStatus {
  if (anyRunning) return "running";
  if (anyFailed && invocations === 0) return "error";
  if (invocations === 0) return "idle";
  return "running";
}

/** "12s ago" / "5m ago" / "2h ago" / "3d ago" — kept short for table cells. */
function relTimeLabel(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime();
  if (ms < 0) return "now";
  const s = Math.round(ms / 1000);
  if (s < 60)   return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60)   return `${m}m ago`;
  const h = Math.round(m / 60);
  if (h < 24)   return `${h}h ago`;
  const d = Math.round(h / 24);
  return `${d}d ago`;
}

const STATUS_CONFIG: Record<AgentStatus, { label: string; variant: "green" | "cyan" | "amber" | "red" | "purple" }> = {
  running:  { label: "Running",  variant: "green"  },
  idle:     { label: "Idle",     variant: "cyan"   },
  error:    { label: "Error",    variant: "red"    },
};

/** Empty-KPI fallback shown until the first /v1/agents/aggregate response lands.
 *  Values are placeholders rendered as "—" if 0; trend is 0 so no arrow paints. */
const EMPTY_KPI = [
  { label: "Active Agents",     value: 0, trend: 0, color: "green"  as const, format: "number"  as const },
  { label: "Invocations Today", value: 0, trend: 0, color: "cyan"   as const, format: "number"  as const },
  { label: "Escalated Today",   value: 0, trend: 0, color: "purple" as const, format: "number"  as const },
  { label: "Success Rate",      value: 0, trend: 0, color: "amber"  as const, format: "percent" as const },
];

/** Empty 24-bucket placeholder so the chart paints axes while data loads. */
const EMPTY_INVOCATION_TREND = Array.from({ length: 24 }, (_, i) => ({
  hour: `${i}:00`, dispatch: 0, support: 0, fraud: 0,
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
  // Falls back to empty placeholders on failure so the page degrades
  // gracefully if ai-layer is unreachable.
  const [agg, setAgg] = useState<AggregateStats | null>(null);

  // Recent sessions — used to derive per-agent live metrics (last action,
  // success rate, average latency). Capped at 200; backfills on every
  // aggregate refresh. Set to empty on failure so all agent cards still
  // render their static descriptions.
  const [sessions, setSessions] = useState<SessionSummary[]>([]);

  const refreshAggregate = useCallback(async () => {
    try {
      const [aggRes, sessRes] = await Promise.all([
        authFetch(`${API_BASE}/v1/agents/aggregate`),
        authFetch(`${API_BASE}/v1/agents/sessions?limit=200`),
      ]);
      if (aggRes.ok) {
        const j = await aggRes.json();
        if (j?.data) setAgg(j.data);
      }
      if (sessRes.ok) {
        const j = await sessRes.json() as { sessions?: SessionSummary[] };
        setSessions(j.sessions ?? []);
      }
    } catch {
      /* swallow — empty fallback already set */
    }
  }, []);

  useEffect(() => { refreshAggregate(); }, [refreshAggregate]);
  useEffect(() => {
    const id = setInterval(refreshAggregate, 30_000);
    return () => clearInterval(id);
  }, [refreshAggregate]);

  const liveKpi = useMemo(() => {
    if (!agg) return EMPTY_KPI;
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
    if (!agg) return EMPTY_INVOCATION_TREND;
    return agg.hourly_24h.map((b) => ({
      hour:     `${b.hour}:00`,
      dispatch: b.by_type["dispatch"] ?? 0,
      support:  (b.by_type["merchant_support"] ?? 0) + (b.by_type["recovery"] ?? 0),
      fraud:    b.by_type["anomaly"] ?? 0,
    }));
  }, [agg]);

  /** Per-agent metrics derived from `sessions` + `agg.by_type_today`. The
   *  sessions list is the source of truth for the "last action" and average
   *  latency; the aggregate gives today's invocation counts (sessions may
   *  be capped at 200, so prefer the aggregate for raw counts). */
  const agentStats: Record<string, DerivedAgentStats> = useMemo(() => {
    const out: Record<string, DerivedAgentStats> = {};
    for (const cat of AGENT_CATALOG) {
      const mine = sessions.filter((s) => s.agent_type === cat.type);
      const completed = mine.filter((s) => s.status === "completed" && s.completed_at);
      const terminal  = mine.filter((s) => s.status === "completed" || s.status === "failed");
      const failed    = mine.filter((s) => s.status === "failed");
      const running   = mine.filter((s) => s.status === "running");

      const totalLatencyMs = completed.reduce((acc, s) => {
        const dur = new Date(s.completed_at!).getTime() - new Date(s.started_at).getTime();
        return acc + Math.max(0, dur);
      }, 0);
      const avgLatency = completed.length > 0 ? Math.round(totalLatencyMs / completed.length) : -1;
      const successRate = terminal.length > 0
        ? Math.round((terminal.length - failed.length) / terminal.length * 1000) / 10
        : -1;

      const last = mine[0]; // backend orders by started_at DESC
      const invocations = agg?.by_type_today[cat.type] ?? mine.length;

      out[cat.type] = {
        invocations_today: invocations,
        avg_latency_ms:    avgLatency,
        success_rate:      successRate,
        last_action:       last?.outcome
          ?? last?.escalation_reason
          ?? (last ? `Session ${last.id.slice(0, 8)} · ${last.status}` : "No activity recorded yet"),
        last_action_time:  last ? relTimeLabel(last.started_at) : "—",
        status:            statusFromSessions(invocations, running.length > 0, failed.length > 0),
      };
    }
    return out;
  }, [sessions, agg]);

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
          onClick={() => { void loadEscalated(); void refreshAggregate(); }}
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
        {AGENT_CATALOG.map((agent) => {
          const stats = agentStats[agent.type] ?? {
            invocations_today: 0, avg_latency_ms: -1, success_rate: -1,
            last_action: "No activity recorded yet", last_action_time: "—",
            status: "idle" as AgentStatus,
          };
          const { label, variant } = STATUS_CONFIG[stats.status];
          const isSelected = selectedAgent === agent.type;
          return (
            <GlassCard
              key={agent.type}
              className={`cursor-pointer transition-all ${isSelected ? "border-purple-plasma/40" : "hover:border-glass-border-bright"}`}
              glow={isSelected ? "purple" : undefined}
              onClick={() => setSelectedAgent(isSelected ? null : agent.type)}
            >
              <div className="flex items-start gap-3 mb-3">
                <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-xl bg-glass-200 border border-glass-border">
                  {agent.icon}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-sm font-semibold text-white truncate">{agent.name}</p>
                    <NeonBadge variant={variant} dot={stats.status === "running"}>{label}</NeonBadge>
                  </div>
                  <p className="text-2xs font-mono text-white/40 mt-0.5 truncate">{agent.model}</p>
                </div>
              </div>

              <p className="text-xs text-white/50 mb-3">{agent.description}</p>

              {/* Stats row — derived live from /v1/agents/sessions */}
              <div className="grid grid-cols-3 gap-2 mb-3">
                {[
                  { label: "Invocations", value: stats.invocations_today > 0 ? stats.invocations_today.toLocaleString() : "—" },
                  { label: "Avg Latency", value: stats.avg_latency_ms >= 0 ? `${stats.avg_latency_ms}ms` : "—" },
                  { label: "Success",     value: stats.success_rate >= 0 ? `${stats.success_rate}%` : "—" },
                ].map((s) => (
                  <div key={s.label} className="rounded-lg bg-glass-100 px-2.5 py-2">
                    <p className="text-2xs font-mono text-white/30 uppercase">{s.label}</p>
                    <p className="text-sm font-bold text-white mt-0.5">{s.value}</p>
                  </div>
                ))}
              </div>

              {/* Last action */}
              <div className="rounded-lg bg-glass-100 border border-glass-border px-3 py-2">
                <p className="text-2xs font-mono text-white/30 mb-0.5">Last action · {stats.last_action_time}</p>
                <p className="text-xs text-white/60 line-clamp-2 mb-2">{stats.last_action}</p>
                {agent.inspect && (
                  <a
                    href={agent.inspect.href}
                    onClick={(e) => e.stopPropagation()}
                    className={`inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-2xs font-mono transition-colors ${
                      agent.inspect.crossPortal
                        ? "border-purple-plasma/30 bg-purple-plasma/10 text-purple-plasma hover:bg-purple-plasma/20"
                        : "border-cyan-neon/30 bg-cyan-neon/10 text-cyan-neon hover:bg-cyan-neon/20"
                    }`}
                  >
                    <ArrowUpRight size={10} />
                    {agent.inspect.label}
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
