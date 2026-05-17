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
import { motion, AnimatePresence } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import {
  Bot, Zap, Brain, ShieldCheck, Headphones, Route,
  RefreshCw, ArrowUpRight, AlertCircle, CheckCircle2, Sparkles,
  Play, X, ChevronDown, ChevronUp, Clock, BarChart2,
} from "lucide-react";
import { authFetch } from "@/lib/auth/auth-fetch";
import { usePermissions } from "@/hooks/usePermissions";

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
  // Backend may return "escalated" or "human_escalated" depending on version.
  status:            "running" | "completed" | "human_escalated" | "escalated" | "failed";
  outcome?:          string | null;
  escalation_reason?: string | null;
  confidence_score:  number;
  actions_taken:     number;
  started_at:        string;
  completed_at?:     string | null;
}

// ── Types & data ───────────────────────────────────────────────────────────────

type AgentStatus = "running" | "idle" | "error";

interface AgentCatalogEntry {
  type:        string;
  name:        string;
  description: string;
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
  avg_latency_ms:    number;
  success_rate:      number;
  last_action:       string;
  last_action_time:  string;
  status:            AgentStatus;
  recent_sessions:   SessionSummary[];
}

function isEscalated(s: SessionSummary): boolean {
  return s.status === "human_escalated" || s.status === "escalated";
}

function statusFromSessions(invocations: number, anyRunning: boolean, anyFailed: boolean): AgentStatus {
  if (anyRunning) return "running";
  if (anyFailed && invocations === 0) return "error";
  if (invocations === 0) return "idle";
  // Has invocations but nothing currently running → idle (all sessions completed)
  return "idle";
}

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
  running: { label: "Running", variant: "green"  },
  idle:    { label: "Idle",    variant: "cyan"   },
  error:   { label: "Error",   variant: "red"    },
};

const EMPTY_KPI = [
  { label: "Active Agents",     value: 0, trend: 0, color: "green"  as const, format: "number"  as const },
  { label: "Invocations Today", value: 0, trend: 0, color: "cyan"   as const, format: "number"  as const },
  { label: "Escalated Today",   value: 0, trend: 0, color: "purple" as const, format: "number"  as const },
  { label: "Success Rate",      value: 0, trend: 0, color: "amber"  as const, format: "percent" as const },
];

const EMPTY_INVOCATION_TREND = Array.from({ length: 24 }, (_, i) => ({
  hour: `${i}:00`, dispatch: 0, support: 0, anomaly: 0,
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

// ── Run Agent Modal ────────────────────────────────────────────────────────────

interface RunAgentResult {
  session_id:       string;
  status:           string;
  outcome:          string;
  confidence:       number;
  actions_taken:    number;
  escalated:        boolean;
  escalation_reason?: string;
}

function RunAgentModal({
  open,
  onClose,
  onSuccess,
}: {
  open: boolean;
  onClose: () => void;
  onSuccess: (result: RunAgentResult) => void;
}) {
  const [agentType, setAgentType] = useState("on_demand");
  const [prompt, setPrompt]       = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError]         = useState<string | null>(null);
  const [result, setResult]       = useState<RunAgentResult | null>(null);

  function reset() {
    setAgentType("on_demand");
    setPrompt("");
    setError(null);
    setResult(null);
    setSubmitting(false);
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!prompt.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/agents/run`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ prompt: prompt.trim(), context: { agent_type: agentType } }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const json = await res.json() as RunAgentResult;
      setResult(json);
      onSuccess(json);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Agent invocation failed");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4"
          onClick={(e) => { if (e.target === e.currentTarget) { reset(); onClose(); } }}
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: 10 }}
            transition={{ type: "spring", stiffness: 360, damping: 30 }}
            className="w-full max-w-lg rounded-2xl border border-glass-border bg-canvas-100 p-6 shadow-2xl"
          >
            <div className="flex items-center justify-between mb-5">
              <div className="flex items-center gap-2">
                <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-purple-surface border border-purple-plasma/30">
                  <Play size={14} className="text-purple-plasma" />
                </div>
                <h2 className="font-heading text-base font-semibold text-white">Run On-Demand Agent</h2>
              </div>
              <button onClick={() => { reset(); onClose(); }} className="text-white/30 hover:text-white/70 transition-colors">
                <X size={16} />
              </button>
            </div>

            {result ? (
              <div className="space-y-4">
                <div className={`flex items-center gap-2 rounded-xl border p-3 ${
                  result.escalated ? "border-amber-signal/30 bg-amber-surface" : "border-green-signal/30 bg-green-surface"
                }`}>
                  {result.escalated
                    ? <AlertCircle size={14} className="text-amber-signal flex-shrink-0" />
                    : <CheckCircle2 size={14} className="text-green-signal flex-shrink-0" />
                  }
                  <div>
                    <p className="text-xs font-semibold text-white">
                      {result.escalated ? "Escalated for human review" : "Session completed"}
                    </p>
                    <p className="text-2xs font-mono text-white/50 mt-0.5">
                      Session <span className="text-white/70">{result.session_id.slice(0, 12)}…</span>
                      {" · "}{result.actions_taken} action{result.actions_taken !== 1 ? "s" : ""}
                      {" · "}conf {Math.round(result.confidence * 100)}%
                    </p>
                  </div>
                </div>
                {result.outcome && (
                  <div className="rounded-lg bg-glass-100 border border-glass-border px-3 py-2">
                    <p className="text-2xs font-mono text-white/30 mb-1">Outcome</p>
                    <p className="text-xs text-white/70">{result.outcome}</p>
                  </div>
                )}
                {result.escalation_reason && (
                  <div className="rounded-lg bg-amber-surface border border-amber-signal/20 px-3 py-2">
                    <p className="text-2xs font-mono text-amber-signal/60 mb-1">Escalation reason</p>
                    <p className="text-xs text-white/70">{result.escalation_reason}</p>
                  </div>
                )}
                <button
                  onClick={() => { reset(); onClose(); }}
                  className="w-full rounded-xl border border-glass-border bg-glass-100 py-2.5 text-xs text-white/70 hover:text-white transition-colors"
                >
                  Close
                </button>
              </div>
            ) : (
              <form onSubmit={handleSubmit} className="space-y-4">
                <div>
                  <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">
                    Agent type
                  </label>
                  <select
                    value={agentType}
                    onChange={(e) => setAgentType(e.target.value)}
                    className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/80 outline-none focus:border-purple-plasma/40"
                  >
                    {AGENT_CATALOG.map((a) => (
                      <option key={a.type} value={a.type} className="bg-canvas-100">{a.name}</option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">
                    Prompt / Task
                  </label>
                  <textarea
                    value={prompt}
                    onChange={(e) => setPrompt(e.target.value)}
                    rows={4}
                    placeholder="Describe the task for the agent…"
                    required
                    className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/80 outline-none focus:border-purple-plasma/40 resize-none placeholder:text-white/20 font-mono"
                  />
                </div>
                {error && (
                  <p className="text-xs text-red-signal font-mono">{error}</p>
                )}
                <div className="flex gap-2 pt-1">
                  <button
                    type="button"
                    onClick={() => { reset(); onClose(); }}
                    className="flex-1 rounded-xl border border-glass-border bg-glass-100 py-2.5 text-xs text-white/60 hover:text-white transition-colors"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    disabled={submitting || !prompt.trim()}
                    className="flex-1 flex items-center justify-center gap-2 rounded-xl bg-purple-plasma py-2.5 text-xs font-semibold text-white hover:brightness-110 disabled:opacity-40 transition-all"
                  >
                    {submitting ? (
                      <><RefreshCw size={11} className="animate-spin" /> Running…</>
                    ) : (
                      <><Play size={11} /> Run Agent</>
                    )}
                  </button>
                </div>
              </form>
            )}
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

// ── Main page ──────────────────────────────────────────────────────────────────

export default function AIAgentsPage() {
  const { hasPermission } = usePermissions();
  const canResolve = hasPermission("dispatch:assign");

  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);
  const [showRunModal, setShowRunModal]   = useState(false);

  const [escalated, setEscalated]     = useState<EscalatedSession[]>([]);
  const [loadingEsc, setLoadingEsc]   = useState(true);
  const [escError, setEscError]       = useState<string | null>(null);
  const [resolvingId, setResolvingId] = useState<string | null>(null);

  const [agg, setAgg]         = useState<AggregateStats | null>(null);
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
      anomaly:  b.by_type["anomaly"] ?? 0,
    }));
  }, [agg]);

  const agentStats: Record<string, DerivedAgentStats> = useMemo(() => {
    const out: Record<string, DerivedAgentStats> = {};
    for (const cat of AGENT_CATALOG) {
      const mine     = sessions.filter((s) => s.agent_type === cat.type);
      const completed = mine.filter((s) => s.status === "completed" && s.completed_at);
      const terminal  = mine.filter((s) => s.status === "completed" || s.status === "failed");
      const failed    = mine.filter((s) => s.status === "failed");
      const running   = mine.filter((s) => s.status === "running");

      const totalLatencyMs = completed.reduce((acc, s) => {
        const dur = new Date(s.completed_at!).getTime() - new Date(s.started_at).getTime();
        return acc + Math.max(0, dur);
      }, 0);
      const avgLatency  = completed.length > 0 ? Math.round(totalLatencyMs / completed.length) : -1;
      const successRate = terminal.length > 0
        ? Math.round((terminal.length - failed.length) / terminal.length * 1000) / 10
        : -1;

      const last = mine[0];
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
        recent_sessions:   mine.slice(0, 5),
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
    <>
      <RunAgentModal
        open={showRunModal}
        onClose={() => setShowRunModal(false)}
        onSuccess={() => { void refreshAggregate(); void loadEscalated(); }}
      />

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
          <div className="flex items-center gap-2">
            <button
              onClick={() => setShowRunModal(true)}
              className="flex items-center gap-1.5 rounded-lg border border-purple-plasma/30 bg-purple-surface px-3 py-2 text-xs text-purple-plasma hover:bg-purple-plasma/20 transition-colors"
            >
              <Play size={12} /> Run Agent
            </button>
            <button
              onClick={() => { void loadEscalated(); void refreshAggregate(); }}
              className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            >
              <RefreshCw size={12} /> Refresh
            </button>
          </div>
        </motion.div>

        {/* Escalated sessions */}
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
                  {canResolve ? (
                    <button
                      onClick={() => handleResolve(s.id)}
                      disabled={resolvingId === s.id}
                      className="rounded-md border border-green-signal/30 bg-green-signal/10 px-2.5 py-1 text-2xs font-mono text-green-signal hover:bg-green-signal/20 disabled:opacity-40 transition-colors"
                    >
                      {resolvingId === s.id ? "…" : "Resolve"}
                    </button>
                  ) : (
                    <span className="text-2xs font-mono text-white/30">Pending review</span>
                  )}
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
                <p className="text-2xs font-mono text-white/30">Dispatch · Support · Anomaly</p>
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
                  <linearGradient id="grad-anomaly" x1="0" y1="0" x2="0" y2="1">
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
                <Area type="monotone" dataKey="anomaly"  stroke="#FF3B5C" fill="url(#grad-anomaly)" strokeWidth={1.5} />
                <Area type="monotone" dataKey="dispatch" stroke="#00E5FF" fill="url(#grad-disp)"    strokeWidth={1.5} />
                <Area type="monotone" dataKey="support"  stroke="#A855F7" fill="url(#grad-sup)"     strokeWidth={1.5} />
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
              status: "idle" as AgentStatus, recent_sessions: [],
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
                      <div className="flex items-center gap-2 flex-shrink-0">
                        <NeonBadge variant={variant} dot={stats.status === "running"}>{label}</NeonBadge>
                        {isSelected ? <ChevronUp size={12} className="text-white/30" /> : <ChevronDown size={12} className="text-white/30" />}
                      </div>
                    </div>
                    <p className="text-2xs font-mono text-white/40 mt-0.5 truncate">{agent.model}</p>
                  </div>
                </div>

                <p className="text-xs text-white/50 mb-3">{agent.description}</p>

                {/* Stats row */}
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

                {/* Expanded session detail — visible when card is selected */}
                <AnimatePresence>
                  {isSelected && stats.recent_sessions.length > 0 && (
                    <motion.div
                      initial={{ opacity: 0, height: 0 }}
                      animate={{ opacity: 1, height: "auto" }}
                      exit={{ opacity: 0, height: 0 }}
                      transition={{ duration: 0.2 }}
                      className="overflow-hidden"
                    >
                      <div className="mt-3 rounded-lg border border-purple-plasma/20 bg-purple-surface/30 p-3">
                        <div className="flex items-center gap-1.5 mb-2">
                          <BarChart2 size={11} className="text-purple-plasma" />
                          <p className="text-2xs font-mono text-purple-plasma uppercase tracking-wider">
                            Recent Sessions ({stats.recent_sessions.length})
                          </p>
                        </div>
                        <div className="space-y-1.5">
                          {stats.recent_sessions.map((s) => {
                            const esc = isEscalated(s);
                            return (
                              <div
                                key={s.id}
                                className="flex items-center justify-between gap-2 rounded-md bg-glass-100 px-2.5 py-1.5"
                              >
                                <div className="flex items-center gap-1.5 min-w-0">
                                  <Clock size={9} className="text-white/30 flex-shrink-0" />
                                  <span className="text-2xs font-mono text-white/40 truncate">
                                    {relTimeLabel(s.started_at)}
                                  </span>
                                  {s.actions_taken > 0 && (
                                    <span className="text-2xs font-mono text-white/25">
                                      · {s.actions_taken} action{s.actions_taken !== 1 ? "s" : ""}
                                    </span>
                                  )}
                                </div>
                                <NeonBadge
                                  variant={
                                    s.status === "completed" ? "green"
                                    : s.status === "running"  ? "cyan"
                                    : s.status === "failed"   ? "red"
                                    : esc                     ? "amber"
                                    : "muted"
                                  }
                                >
                                  {esc ? "escalated" : s.status}
                                </NeonBadge>
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    </motion.div>
                  )}
                  {isSelected && stats.recent_sessions.length === 0 && (
                    <motion.div
                      initial={{ opacity: 0, height: 0 }}
                      animate={{ opacity: 1, height: "auto" }}
                      exit={{ opacity: 0, height: 0 }}
                      transition={{ duration: 0.2 }}
                      className="overflow-hidden"
                    >
                      <div className="mt-3 rounded-lg border border-glass-border bg-glass-100 px-3 py-3 text-center">
                        <p className="text-2xs font-mono text-white/30">No sessions recorded today</p>
                        <button
                          onClick={(e) => { e.stopPropagation(); setShowRunModal(true); }}
                          className="mt-2 inline-flex items-center gap-1.5 rounded-md border border-purple-plasma/30 bg-purple-surface px-2.5 py-1 text-2xs font-mono text-purple-plasma hover:bg-purple-plasma/20 transition-colors"
                        >
                          <Play size={9} /> Run now
                        </button>
                      </div>
                    </motion.div>
                  )}
                </AnimatePresence>
              </GlassCard>
            );
          })}
        </motion.div>
      </motion.div>
    </>
  );
}
