"use client";
/**
 * Partner Portal — Daily Manifests
 * Aggregated view of a carrier's drivers' work on a selected date.
 *
 * Data:
 *   GET /v1/tasks/manifest?date=YYYY-MM-DD&carrier_id=<uuid>
 * The driver-ops service returns one row per (driver, task_type) with
 * counts by status. Partner drivers are linked via drivers.carrier_id
 * (migration 0007). When a partner hasn't onboarded drivers as theirs,
 * carrier_id is NULL and the row won't appear — operators can still
 * see the whole tenant view via the admin/hubs deep-link.
 */
import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { FileText, Search, Building2, RefreshCw, Calendar, CheckCircle2, Clock, X } from "lucide-react";
import { carriersApi, type ManifestEntry } from "@/lib/api/carriers";
import { getCurrentPartnerId } from "@/lib/api/partner-identity";

type DerivedStatus = "pending" | "in_progress" | "completed" | "failed";

const STATUS_CONFIG: Record<DerivedStatus, { label: string; variant: "green" | "amber" | "red" | "cyan"; icon: React.ReactNode }> = {
  pending:     { label: "Pending",     variant: "amber", icon: <Clock size={10} />       },
  in_progress: { label: "In Progress", variant: "cyan",  icon: <Clock size={10} />       },
  completed:   { label: "Completed",   variant: "green", icon: <CheckCircle2 size={10} /> },
  failed:      { label: "Failed",      variant: "red",   icon: <X size={10} />           },
};

function isoToday(): string {
  return new Date().toISOString().slice(0, 10);
}

function deriveStatus(e: ManifestEntry): DerivedStatus {
  if (e.total === 0)                                   return "pending";
  if (e.completed === e.total)                         return "completed";
  if (e.in_progress > 0 || e.pending > 0)              return "in_progress";
  if (e.failed > 0 && e.completed + e.failed === e.total) return "failed";
  return "in_progress";
}

function ManifestsPageInner() {
  const searchParams = useSearchParams();
  const hubParam = searchParams.get("hub");

  const [search, setSearch]       = useState(
    searchParams.get("zone") ?? searchParams.get("driver") ?? "",
  );
  const [typeFilter, setTypeFilter] = useState<"all" | "pickup" | "delivery">("all");
  const [date, setDate]           = useState(isoToday());
  const [entries, setEntries]     = useState<ManifestEntry[]>([]);
  const [loading, setLoading]     = useState(true);
  const [error, setError]         = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    setLoading(true);
    try {
      const partnerId = getCurrentPartnerId();
      const resp = await carriersApi.manifest(date, partnerId);
      setEntries(resp.data ?? []);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load manifest");
    } finally {
      setLoading(false);
    }
  }, [date]);

  useEffect(() => { load(); }, [load]);

  const filtered = useMemo(() => {
    return entries.filter((e) => {
      const matchType   = typeFilter === "all" || e.task_type === typeFilter;
      const matchSearch = !search || e.driver_name.toLowerCase().includes(search.toLowerCase());
      return matchType && matchSearch;
    });
  }, [entries, typeFilter, search]);

  const kpis = useMemo(() => {
    const total      = entries.reduce((n, e) => n + e.total, 0);
    const completed  = entries.reduce((n, e) => n + e.completed, 0);
    const failed     = entries.reduce((n, e) => n + e.failed, 0);
    const inProgress = entries.reduce((n, e) => n + e.in_progress + e.pending, 0);
    const rate       = total > 0 ? (completed / total) * 100 : 0;
    return { total, completed, failed, inProgress, rate };
  }, [entries]);

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
            <FileText size={20} className="text-cyan-neon" />
            Manifests
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {loading ? "loading…" : `${filtered.length} row${filtered.length === 1 ? "" : "s"}`} · {date}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-2 rounded-lg border border-glass-border bg-glass-100 px-3 py-2">
            <Calendar size={13} className="text-white/40" />
            <input
              type="date"
              value={date}
              onChange={(e) => setDate(e.target.value)}
              className="bg-transparent text-xs text-white outline-none font-mono"
            />
          </div>
          <button
            onClick={load}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            title="Refresh"
          >
            <RefreshCw size={12} />
          </button>
        </div>
      </motion.div>

      {/* Hub deep-link context banner (preserved from prior design) */}
      {hubParam && (
        <motion.div variants={variants.fadeInUp}>
          <div className="flex items-center gap-2 rounded-lg border border-purple-plasma/25 bg-purple-plasma/5 px-3 py-2">
            <Building2 size={13} className="text-purple-plasma" />
            <span className="text-xs font-mono text-white/70">
              Scoped to hub <span className="text-purple-plasma font-bold">{hubParam}</span>
            </span>
            <span className="text-2xs font-mono text-white/30">· via admin/hubs deep-link</span>
          </div>
        </motion.div>
      )}

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="sm">
            <p className="text-xs text-red-signal font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* KPI strip */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        {[
          { label: "Total Tasks",   value: kpis.total,      color: "text-cyan-neon"     },
          { label: "Completed",     value: kpis.completed,  color: "text-green-signal"  },
          { label: "In Progress",   value: kpis.inProgress, color: "text-amber-signal"  },
          { label: "Failed",        value: kpis.failed,     color: "text-red-signal"    },
          { label: "Completion %",  value: `${kpis.rate.toFixed(0)}%`, color: "text-purple-plasma" },
        ].map((m) => (
          <GlassCard key={m.label} size="sm">
            <p className="text-2xs font-mono text-white/30 uppercase tracking-wider">{m.label}</p>
            <p className={`text-lg font-bold font-mono mt-1 ${m.color}`}>{m.value}</p>
          </GlassCard>
        ))}
      </motion.div>

      {/* Filters */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-1.5">
              {(["all", "pickup", "delivery"] as const).map((t) => (
                <button
                  key={t}
                  onClick={() => setTypeFilter(t)}
                  className={`rounded-full px-3 py-1 text-xs font-medium capitalize transition-all ${
                    typeFilter === t
                      ? "bg-cyan-surface text-cyan-neon border border-cyan-neon/30"
                      : "text-white/40 border border-glass-border hover:text-white"
                  }`}
                >
                  {t === "all" ? "All Types" : t}
                </button>
              ))}
            </div>
            <div className="ml-auto flex items-center gap-2 rounded-lg border border-glass-border bg-glass-100 px-3 py-2">
              <Search size={13} className="text-white/30" />
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Driver name…"
                className="bg-transparent text-xs text-white placeholder:text-white/25 outline-none font-mono w-48"
              />
            </div>
          </div>
        </GlassCard>
      </motion.div>

      {/* Manifests table */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="grid grid-cols-[2fr_90px_80px_80px_80px_80px_100px_100px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Driver", "Type", "Total", "Done", "In-Flight", "Failed", "%", "Status"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {loading ? (
            <div className="px-5 py-10 text-center text-xs text-white/40 font-mono">loading…</div>
          ) : filtered.length === 0 ? (
            <div className="px-5 py-10 text-center">
              <p className="text-xs text-white/40 font-mono">
                No tasks for drivers linked to this partner on {date}. Ensure drivers have
                <span className="mx-1 text-cyan-neon">drivers.carrier_id</span>
                set (ADR-0013); otherwise contact ops.
              </p>
            </div>
          ) : (
            filtered.map((e) => {
              const status = deriveStatus(e);
              const cfg = STATUS_CONFIG[status];
              const pct = e.total === 0 ? 0 : Math.round((e.completed / e.total) * 100);
              const inFlight = e.in_progress + e.pending;
              return (
                <div
                  key={`${e.driver_id}-${e.task_type}`}
                  className="grid grid-cols-[2fr_90px_80px_80px_80px_80px_100px_100px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors"
                >
                  <div>
                    <p className="text-xs font-medium text-white">{e.driver_name}</p>
                    <p className="text-2xs font-mono text-white/30 mt-0.5 truncate" title={e.driver_id}>
                      {e.driver_id.slice(0, 8)}…
                    </p>
                  </div>
                  <NeonBadge variant={e.task_type === "pickup" ? "cyan" : "purple"}>
                    {e.task_type}
                  </NeonBadge>
                  <span className="text-sm font-mono text-white">{e.total}</span>
                  <span className="text-sm font-mono text-green-signal">{e.completed}</span>
                  <span className="text-sm font-mono text-amber-signal">{inFlight}</span>
                  <span className={`text-sm font-mono ${e.failed > 0 ? "text-red-signal" : "text-white/30"}`}>{e.failed}</span>
                  <span className={`text-sm font-mono font-semibold ${pct >= 90 ? "text-green-signal" : pct >= 60 ? "text-cyan-neon" : "text-white/40"}`}>
                    {pct}%
                  </span>
                  <div className="flex items-center gap-1.5">
                    <NeonBadge variant={cfg.variant} dot>
                      {cfg.label}
                    </NeonBadge>
                  </div>
                </div>
              );
            })
          )}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}

export default function ManifestsPage() {
  return (
    <Suspense>
      <ManifestsPageInner />
    </Suspense>
  );
}
