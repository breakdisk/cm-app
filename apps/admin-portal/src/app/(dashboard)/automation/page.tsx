"use client";
/**
 * Admin Portal — Automation Rules
 * Surfaces the business-logic service rules engine:
 *   GET   /v1/rules                → list
 *   PATCH /v1/rules/:id/toggle     → enable/disable
 *   POST  /v1/rules/reload         → hot-reload the in-memory engine
 *   GET   /v1/rules/:id/executions → (future) drill-down
 *
 * The rule-builder UI (create/edit a new rule) is deferred — it needs a
 * dedicated wizard for the ECA (event-condition-action) composition. For
 * now operators can view, toggle, and inspect what's live.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { Workflow, RefreshCw, Zap, Power, ChevronDown, ChevronUp } from "lucide-react";
import { rulesApi, enumLabel, type AutomationRule } from "@/lib/api/rules";

export default function AutomationPage() {
  const [rules, setRules]     = useState<AutomationRule[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);
  const [busyId, setBusyId]   = useState<string | null>(null);
  const [reloading, setReloading] = useState(false);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const resp = await rulesApi.list({ perPage: 100 });
      // Sort by priority (lower = higher priority), then name
      const sorted = [...(resp.data ?? [])].sort(
        (a, b) => a.priority - b.priority || a.name.localeCompare(b.name),
      );
      setRules(sorted);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load rules");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  async function handleToggle(id: string) {
    setBusyId(id);
    try {
      await rulesApi.toggle(id);
      await load();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Toggle failed");
    } finally {
      setBusyId(null);
    }
  }

  async function handleReload() {
    setReloading(true);
    try {
      const { rules_loaded } = await rulesApi.reload();
      setError(`Engine reloaded — ${rules_loaded} rule${rules_loaded === 1 ? "" : "s"} active.`);
      await load();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Reload failed");
    } finally {
      setReloading(false);
    }
  }

  const kpis = useMemo(() => {
    const active = rules.filter(r => r.is_active).length;
    const totalActions = rules.reduce((n, r) => n + r.actions.length, 0);
    const withConds = rules.filter(r => r.conditions.length > 0).length;
    return [
      { label: "Active Rules",   value: active,       trend: 0, color: "green"  as const, format: "number" as const },
      { label: "Total Rules",    value: rules.length, trend: 0, color: "cyan"   as const, format: "number" as const },
      { label: "Total Actions",  value: totalActions, trend: 0, color: "purple" as const, format: "number" as const },
      { label: "Conditional",    value: withConds,    trend: 0, color: "amber"  as const, format: "number" as const },
    ];
  }, [rules]);

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
            <Workflow size={22} className="text-purple-plasma" />
            Automation
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            Business rules · {kpis[0].value} active of {rules.length}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleReload}
            disabled={reloading}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-40"
            title="Hot-reload rules engine from DB"
          >
            <Zap size={13} /> {reloading ? "Reloading…" : "Reload engine"}
          </button>
          <button
            onClick={load}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            title="Refresh"
          >
            <RefreshCw size={13} />
          </button>
        </div>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <p className="text-xs text-white/60 font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* KPIs */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {kpis.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Rules list */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white">Rules</h2>
            <span className="text-2xs font-mono text-white/30">
              {loading ? "loading…" : `sorted by priority (lower = higher)`}
            </span>
          </div>

          {loading ? (
            <div className="px-5 py-10 text-center text-xs text-white/40 font-mono">loading…</div>
          ) : rules.length === 0 ? (
            <div className="px-5 py-10 text-center">
              <p className="text-xs text-white/40 font-mono">
                No automation rules defined yet. The rule builder UI is coming in a future release.
                Rules can be seeded via migration or the POST /v1/rules endpoint directly.
              </p>
            </div>
          ) : (
            rules.map((r) => {
              const expanded = expandedId === r.id;
              const busy = busyId === r.id;
              return (
                <div key={r.id} className="border-b border-glass-border/50">
                  <div className="grid grid-cols-[60px_1fr_120px_120px_80px_100px] gap-3 items-center px-5 py-3 hover:bg-glass-100 transition-colors">
                    <span className="text-2xs font-mono text-white/30">P{r.priority}</span>
                    <div className="min-w-0">
                      <p className="text-xs font-medium text-white truncate">{r.name}</p>
                      {r.description && (
                        <p className="text-2xs text-white/40 mt-0.5 truncate">{r.description}</p>
                      )}
                    </div>
                    <span className="text-xs text-cyan-neon font-mono truncate" title={enumLabel(r.trigger)}>
                      {enumLabel(r.trigger)}
                    </span>
                    <span className="text-2xs text-white/50 font-mono">
                      {r.actions.length} action{r.actions.length === 1 ? "" : "s"}
                      {r.conditions.length > 0 && ` · ${r.conditions.length} cond`}
                    </span>
                    <NeonBadge variant={r.is_active ? "green" : "muted"} dot>
                      {r.is_active ? "active" : "disabled"}
                    </NeonBadge>
                    <div className="flex items-center gap-1 justify-end">
                      <button
                        onClick={() => handleToggle(r.id)}
                        disabled={busy}
                        className="rounded p-1.5 text-white/30 hover:text-cyan-neon hover:bg-glass-200 transition-colors disabled:opacity-40"
                        title={r.is_active ? "Disable" : "Enable"}
                      >
                        {busy
                          ? <span className="block h-3 w-3 animate-spin rounded-full border-2 border-white/30 border-t-white" />
                          : <Power size={12} />}
                      </button>
                      <button
                        onClick={() => setExpandedId(expanded ? null : r.id)}
                        className="rounded p-1.5 text-white/30 hover:text-white hover:bg-glass-200 transition-colors"
                        title={expanded ? "Collapse" : "Expand"}
                      >
                        {expanded ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
                      </button>
                    </div>
                  </div>

                  {expanded && (
                    <div className="bg-glass-50 px-5 py-4 grid gap-4 md:grid-cols-2 border-t border-glass-border/30">
                      <div>
                        <p className="text-2xs font-mono text-white/40 uppercase tracking-wider mb-2">Conditions</p>
                        {r.conditions.length === 0 ? (
                          <p className="text-xs text-white/30">No conditions — fires on every trigger</p>
                        ) : (
                          <ul className="flex flex-col gap-1">
                            {r.conditions.map((c, i) => (
                              <li key={i} className="text-xs text-white/70 font-mono">
                                • {enumLabel(c)}
                              </li>
                            ))}
                          </ul>
                        )}
                      </div>
                      <div>
                        <p className="text-2xs font-mono text-white/40 uppercase tracking-wider mb-2">Actions</p>
                        <ul className="flex flex-col gap-1">
                          {r.actions.map((a, i) => (
                            <li key={i} className="text-xs text-green-signal font-mono">
                              → {enumLabel(a)}
                            </li>
                          ))}
                        </ul>
                      </div>
                    </div>
                  )}
                </div>
              );
            })
          )}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
