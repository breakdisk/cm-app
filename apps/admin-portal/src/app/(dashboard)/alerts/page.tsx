"use client";
/**
 * Admin Portal — Alerts Page
 *
 * Client-side aggregator: merges three live sources into one alert stream.
 * No dedicated alerts service exists yet, so this page owns the merge logic.
 *
 *   1. AI escalations       GET /v1/agents/sessions/escalated (ai-layer)
 *   2. Compliance pending   GET /api/v1/compliance/admin/queue
 *   3. Stuck shipments      GET /v1/shipments?status=delivery_attempted (order-intake)
 *
 * Severity heuristic:
 *   Critical — AI escalation; shipments in delivery_attempted > 24h
 *   Warning  — compliance docs awaiting review; shipments in delivery_attempted <= 24h
 *
 * Dismissal is client-side (localStorage). When a real `alerts` table ships
 * in business-logic or a new alerts service, swap the Set<string> for a
 * server-backed dismiss+reopen flow. See ADR slot reserved for this.
 *
 * Future-work hook: business-logic rule executions (/v1/rules/:id/executions)
 * aren't pulled here because they require one fetch per rule (N+1). When
 * business-logic gains a cross-rule `GET /v1/executions?failed=true`
 * endpoint, extend `loadAlerts` below to pull from it too.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import {
  AlertTriangle, AlertCircle, Info, CheckCircle2, X, Bell, RefreshCw,
} from "lucide-react";
import { authFetch } from "@/lib/auth/auth-fetch";

// ── Types ──────────────────────────────────────────────────────────────────────

type AlertSeverity = "critical" | "warning" | "info";
type AlertCategory = "ai" | "compliance" | "shipment";

interface Alert {
  id: string;
  title: string;
  description: string;
  severity: AlertSeverity;
  category: AlertCategory;
  /** ISO timestamp; used for age display + sorting */
  occurredAt: string;
  /** Deep-link when the user clicks "View" */
  actionHref?: string;
  actionLabel?: string;
}

// ── Config ─────────────────────────────────────────────────────────────────────

const SEVERITY_CONFIG: Record<AlertSeverity, { icon: React.ReactNode; variant: "red" | "amber" | "cyan"; label: string; borderColor: string }> = {
  critical: { icon: <AlertCircle size={14} className="text-red-signal"    />, variant: "red",   label: "Critical", borderColor: "border-red-signal/20"   },
  warning:  { icon: <AlertTriangle size={14} className="text-amber-signal" />, variant: "amber", label: "Warning",  borderColor: "border-amber-signal/20" },
  info:     { icon: <Info size={14} className="text-cyan-neon"             />, variant: "cyan",  label: "Info",     borderColor: "border-cyan-neon/20"    },
};

const CATEGORY_LABEL: Record<AlertCategory, string> = {
  ai:         "AI",
  compliance: "Compliance",
  shipment:   "Shipment",
};

const DISMISSED_KEY = "cm:admin:alerts:dismissed:v1";
const POLL_MS       = 30_000;
const STUCK_HOURS   = 24;

// ── Source fetchers ────────────────────────────────────────────────────────────

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

/** Safe fetch that tolerates 4xx/5xx — caller gets `null` so one dead
 *  source doesn't blank the whole page. */
async function safeFetch<T>(path: string): Promise<T | null> {
  try {
    const res = await authFetch(`${API_BASE}${path}`);
    if (!res.ok) return null;
    return (await res.json()) as T;
  } catch {
    return null;
  }
}

function hoursSince(iso: string): number {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return 0;
  return (Date.now() - t) / 36e5;
}

function relativeTime(iso: string): string {
  const hours = hoursSince(iso);
  if (hours < 1) {
    const m = Math.max(1, Math.floor(hours * 60));
    return `${m}m ago`;
  }
  if (hours < 24) return `${Math.floor(hours)}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

async function loadAlerts(): Promise<Alert[]> {
  const [ai, compliance, shipments] = await Promise.all([
    safeFetch<{ escalated: Array<{ id: string; agent_type: string; escalation_reason?: string; started_at: string }>; count: number }>(
      "/v1/agents/sessions/escalated",
    ),
    safeFetch<{ data: Array<{ id: string; document_number: string; submitted_at: string; document_type_id: string }> }>(
      "/api/v1/compliance/admin/queue?limit=50",
    ),
    safeFetch<{ shipments: Array<{ id: string; tracking_number?: string | null; customer_name: string; status: string; updated_at: string; dest_city?: string }>; total: number }>(
      "/v1/shipments?status=delivery_attempted&per_page=50",
    ),
  ]);

  const out: Alert[] = [];

  // 1. AI escalations — always critical.
  for (const s of ai?.escalated ?? []) {
    out.push({
      id:          `ai:${s.id}`,
      title:       `${s.agent_type} agent escalated`,
      description: s.escalation_reason ?? "Agent session requires human review.",
      severity:    "critical",
      category:    "ai",
      occurredAt:  s.started_at,
      actionHref:  `/ai-agents?session=${encodeURIComponent(s.id)}`,
      actionLabel: "Review session",
    });
  }

  // 2. Compliance review queue — warning severity; escalates to critical
  //    if the doc has been sitting > 48h.
  for (const d of compliance?.data ?? []) {
    const stale = hoursSince(d.submitted_at) > 48;
    out.push({
      id:          `compliance:${d.id}`,
      title:       `Driver doc awaiting review${stale ? " — >48h" : ""}`,
      description: `Document ${d.document_number} pending admin approval.`,
      severity:    stale ? "critical" : "warning",
      category:    "compliance",
      occurredAt:  d.submitted_at,
      actionHref:  `/compliance?doc=${encodeURIComponent(d.id)}`,
      actionLabel: "Open review",
    });
  }

  // 3. Stuck shipments — critical if > STUCK_HOURS in delivery_attempted.
  for (const s of shipments?.shipments ?? []) {
    const hours = hoursSince(s.updated_at);
    const critical = hours > STUCK_HOURS;
    const tracking = s.tracking_number ?? s.id.slice(0, 8);
    out.push({
      id:          `shipment:${s.id}`,
      title:       critical
        ? `Shipment ${tracking} stuck ${Math.floor(hours)}h`
        : `Shipment ${tracking} — delivery attempted`,
      description: `Customer ${s.customer_name}${s.dest_city ? ` · ${s.dest_city}` : ""}`,
      severity:    critical ? "critical" : "warning",
      category:    "shipment",
      occurredAt:  s.updated_at,
      actionHref:  `/shipments?q=${encodeURIComponent(tracking)}`,
      actionLabel: "Open shipment",
    });
  }

  // Newest first.
  out.sort((a, b) => new Date(b.occurredAt).getTime() - new Date(a.occurredAt).getTime());
  return out;
}

// ── Page ──────────────────────────────────────────────────────────────────────

function loadDismissed(): Set<string> {
  if (typeof window === "undefined") return new Set();
  try {
    const raw = window.localStorage.getItem(DISMISSED_KEY);
    if (!raw) return new Set();
    const arr = JSON.parse(raw) as string[];
    return new Set(Array.isArray(arr) ? arr : []);
  } catch {
    return new Set();
  }
}

function saveDismissed(ids: Set<string>) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(DISMISSED_KEY, JSON.stringify(Array.from(ids)));
  } catch { /* quota exceeded — acceptable */ }
}

export default function AlertsPage() {
  const [alerts, setAlerts]         = useState<Alert[]>([]);
  const [loading, setLoading]       = useState(true);
  const [error, setError]           = useState<string | null>(null);
  const [dismissed, setDismissed]   = useState<Set<string>>(() => loadDismissed());
  const [showResolved, setShowResolved] = useState(false);
  const [severityFilter, setSeverityFilter] = useState<AlertSeverity | "all">("all");

  const refresh = useCallback(async () => {
    setError(null);
    try {
      const next = await loadAlerts();
      setAlerts(next);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load alerts");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  useEffect(() => {
    const id = setInterval(refresh, POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  function resolve(id: string) {
    setDismissed((prev) => {
      const next = new Set(prev);
      next.add(id);
      saveDismissed(next);
      return next;
    });
  }

  function unresolve(id: string) {
    setDismissed((prev) => {
      const next = new Set(prev);
      next.delete(id);
      saveDismissed(next);
      return next;
    });
  }

  const summary = useMemo(() => {
    const active = alerts.filter((a) => !dismissed.has(a.id));
    return {
      critical: active.filter((a) => a.severity === "critical").length,
      warning:  active.filter((a) => a.severity === "warning").length,
      info:     active.filter((a) => a.severity === "info").length,
    };
  }, [alerts, dismissed]);

  const visible = useMemo(() => {
    return alerts.filter((a) => {
      const isResolved = dismissed.has(a.id);
      if (!showResolved && isResolved) return false;
      if (severityFilter !== "all" && a.severity !== severityFilter) return false;
      return true;
    });
  }, [alerts, dismissed, showResolved, severityFilter]);

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
            <Bell size={22} className="text-red-signal" />
            Alerts
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {summary.critical} critical · {summary.warning} warnings · {summary.info} info
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={refresh}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            title="Refresh"
          >
            <RefreshCw size={12} />
          </button>
          <button
            onClick={() => setShowResolved((v) => !v)}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
          >
            <CheckCircle2 size={12} />
            {showResolved ? "Hide Resolved" : "Show Resolved"}
          </button>
        </div>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <p className="text-xs text-red-signal font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* Summary */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-3 gap-3">
        {[
          { label: "Critical", count: summary.critical, color: "text-red-signal",   bg: "bg-red-signal/10 border-red-signal/20"     },
          { label: "Warnings", count: summary.warning,  color: "text-amber-signal", bg: "bg-amber-signal/10 border-amber-signal/20" },
          { label: "Info",     count: summary.info,     color: "text-cyan-neon",    bg: "bg-cyan-surface border-cyan-neon/20"       },
        ].map((s) => (
          <div key={s.label} className={`rounded-xl border px-4 py-3 ${s.bg}`}>
            <p className={`font-heading text-3xl font-bold ${s.color}`}>{s.count}</p>
            <p className="text-xs text-white/40 font-mono mt-0.5">{s.label} active</p>
          </div>
        ))}
      </motion.div>

      {/* Filter */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          <div className="flex items-center gap-1.5">
            {(["all", "critical", "warning", "info"] as const).map((s) => (
              <button
                key={s}
                onClick={() => setSeverityFilter(s)}
                className={`rounded-full px-3 py-1 text-xs font-medium capitalize transition-all ${
                  severityFilter === s
                    ? "bg-canvas border border-glass-border-bright text-white"
                    : "text-white/40 border border-glass-border hover:text-white"
                }`}
              >
                {s}
              </button>
            ))}
          </div>
        </GlassCard>
      </motion.div>

      {/* Alert list */}
      <motion.div variants={variants.fadeInUp} className="flex flex-col gap-2">
        {loading && alerts.length === 0 ? (
          <GlassCard className="text-center py-10">
            <p className="text-xs text-white/40 font-mono">loading…</p>
          </GlassCard>
        ) : visible.length === 0 ? (
          <GlassCard className="text-center py-10">
            <CheckCircle2 size={28} className="text-green-signal mx-auto mb-2" />
            <p className="text-sm font-semibold text-white">All clear</p>
            <p className="text-xs text-white/40 font-mono mt-1">
              {alerts.length === 0
                ? "No active alerts across AI, compliance, or shipment sources"
                : "No alerts match your filter"}
            </p>
          </GlassCard>
        ) : (
          visible.map((alert) => {
            const cfg = SEVERITY_CONFIG[alert.severity];
            const isResolved = dismissed.has(alert.id);
            return (
              <div
                key={alert.id}
                className={`rounded-xl border bg-glass-100 px-4 py-4 transition-all ${
                  isResolved ? "opacity-50 border-glass-border" : cfg.borderColor
                }`}
              >
                <div className="flex items-start gap-3">
                  <div className="mt-0.5 flex-shrink-0">{cfg.icon}</div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1 flex-wrap">
                      <p className={`text-sm font-semibold ${isResolved ? "text-white/40 line-through" : "text-white"}`}>
                        {alert.title}
                      </p>
                      <NeonBadge variant={cfg.variant}>{cfg.label}</NeonBadge>
                      <NeonBadge variant="cyan">{CATEGORY_LABEL[alert.category]}</NeonBadge>
                      {isResolved && (
                        <NeonBadge variant="green">
                          <CheckCircle2 size={10} className="mr-1 inline" />Resolved
                        </NeonBadge>
                      )}
                    </div>
                    <p className="text-xs text-white/50 mb-2">{alert.description}</p>
                    <div className="flex items-center gap-3">
                      <span className="text-2xs font-mono text-white/30">{relativeTime(alert.occurredAt)}</span>
                      {alert.actionHref && !isResolved && (
                        <a href={alert.actionHref} className="text-2xs font-mono text-cyan-neon hover:underline">
                          {alert.actionLabel ?? "Open"} ↗
                        </a>
                      )}
                      {!isResolved ? (
                        <button
                          onClick={() => resolve(alert.id)}
                          className="ml-auto text-2xs font-mono text-white/30 hover:text-green-signal transition-colors flex items-center gap-1"
                        >
                          <CheckCircle2 size={11} /> Mark Resolved
                        </button>
                      ) : (
                        <button
                          onClick={() => unresolve(alert.id)}
                          className="ml-auto text-2xs font-mono text-white/30 hover:text-cyan-neon transition-colors"
                        >
                          Undo
                        </button>
                      )}
                    </div>
                  </div>
                  {!isResolved && (
                    <button
                      onClick={() => resolve(alert.id)}
                      className="flex-shrink-0 rounded p-1 text-white/20 hover:text-white/60 transition-colors"
                    >
                      <X size={14} />
                    </button>
                  )}
                </div>
              </div>
            );
          })
        )}
      </motion.div>
    </motion.div>
  );
}
