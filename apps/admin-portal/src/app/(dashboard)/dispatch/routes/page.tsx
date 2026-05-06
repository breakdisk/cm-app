"use client";
/**
 * Routes View — Dispatch Console
 *
 * Lists all dispatch routes for the tenant with their status, stop count,
 * and estimated duration. Ops can cancel a planned or in-progress route
 * from here — this frees the driver back into the auto-dispatch pool.
 */
import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { variants } from "@/lib/design-system/tokens";
import { authFetch } from "@/lib/auth/auth-fetch";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

interface RouteView {
  route_id:                   string;
  driver_id:                  string;
  vehicle_id:                 string;
  status:                     string;
  stop_count:                 number;
  total_distance_km:          number;
  estimated_duration_minutes: number;
  created_at:                 string;
}

const STATUS_BADGE: Record<string, { variant: "cyan" | "green" | "amber" | "purple"; label: string }> = {
  Planned:    { variant: "cyan",   label: "Planned"     },
  InProgress: { variant: "green",  label: "In Progress" },
  Completed:  { variant: "purple", label: "Completed"   },
  Cancelled:  { variant: "amber",  label: "Cancelled"   },
};

function statusBadge(status: string) {
  const cfg = STATUS_BADGE[status] ?? { variant: "amber" as const, label: status };
  return <NeonBadge variant={cfg.variant}>{cfg.label}</NeonBadge>;
}

export default function RoutesPage() {
  const [routes,          setRoutes]          = useState<RouteView[]>([]);
  const [loading,         setLoading]         = useState(false);
  const [error,           setError]           = useState<string | null>(null);
  const [cancellingRoute, setCancellingRoute] = useState<string | null>(null);

  const fetchRoutes = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/routes`);
      if (!res.ok) {
        setError(`Routes fetch failed: ${res.status} ${res.statusText}`);
        return;
      }
      const j = await res.json();
      setRoutes(j.data ?? []);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to load routes");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { fetchRoutes(); }, [fetchRoutes]);

  async function handleCancelRoute(routeId: string) {
    if (!confirm("Cancel this route? The driver will be returned to the available pool.")) return;
    setCancellingRoute(routeId);
    try {
      const res = await authFetch(`${API_BASE}/v1/routes/${routeId}/cancel`, { method: "PUT" });
      if (!res.ok) {
        const j = await res.json().catch(() => ({}));
        throw new Error(j.error?.message ?? `Cancel failed (${res.status})`);
      }
      await fetchRoutes();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Cancel route failed");
    } finally {
      setCancellingRoute(null);
    }
  }

  const active    = routes.filter((r) => r.status === "InProgress");
  const planned   = routes.filter((r) => r.status === "Planned");
  const completed = routes.filter((r) => r.status === "Completed");
  const cancelled = routes.filter((r) => r.status === "Cancelled");

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-4"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Link
            href="/dispatch"
            className="text-xs font-mono text-white/40 hover:text-white/70 transition-colors"
          >
            ← Dispatch Console
          </Link>
          <span className="text-white/20">/</span>
          <h1 className="font-heading text-xl font-bold text-white">Routes</h1>
        </div>
        <div className="flex items-center gap-2">
          {loading && <span className="text-xs font-mono text-white/30 animate-pulse">Loading…</span>}
          <button
            onClick={fetchRoutes}
            className="text-xs font-mono text-cyan-400/70 hover:text-cyan-400 transition-colors"
          >
            ↻ Refresh
          </button>
        </div>
      </motion.div>

      {error && (
        <div className="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-2 text-sm font-mono text-red-400">
          {error}
        </div>
      )}

      {/* KPI strip */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {[
          { label: "In Progress", value: active.length,    color: "bg-green-400" },
          { label: "Planned",     value: planned.length,   color: "bg-cyan-400"  },
          { label: "Completed",   value: completed.length, color: "bg-purple-400"},
          { label: "Cancelled",   value: cancelled.length, color: "bg-amber-400" },
        ].map((m) => (
          <GlassCard key={m.label} size="sm" accent>
            <div className="flex items-center gap-2">
              <span className={`h-2 w-2 rounded-full ${m.color}`} />
              <div>
                <p className="text-xl font-bold font-mono text-white">{m.value}</p>
                <p className="text-[10px] font-mono uppercase tracking-wider text-white/40">{m.label}</p>
              </div>
            </div>
          </GlassCard>
        ))}
      </motion.div>

      {/* Route list */}
      <motion.div variants={variants.fadeInUp} className="flex flex-col gap-2">
        {routes.length === 0 && !loading && (
          <p className="text-sm font-mono text-white/25 text-center py-8">No routes found</p>
        )}
        {routes.map((route) => {
          const canCancel = route.status === "Planned" || route.status === "InProgress";
          const durationH = Math.floor(route.estimated_duration_minutes / 60);
          const durationM = route.estimated_duration_minutes % 60;
          return (
            <GlassCard key={route.route_id} size="sm" glow="cyan">
              <div className="flex items-center gap-4">
                <div className="flex-1 min-w-0 grid grid-cols-2 gap-x-4 gap-y-1 sm:grid-cols-4">
                  <div>
                    <p className="text-[10px] font-mono text-white/30 uppercase">Route</p>
                    <p className="text-sm font-mono text-white/80 truncate">{route.route_id.slice(0, 8)}…</p>
                  </div>
                  <div>
                    <p className="text-[10px] font-mono text-white/30 uppercase">Status</p>
                    <div className="mt-0.5">{statusBadge(route.status)}</div>
                  </div>
                  <div>
                    <p className="text-[10px] font-mono text-white/30 uppercase">Stops</p>
                    <p className="text-sm font-mono text-white/80">{route.stop_count}</p>
                  </div>
                  <div>
                    <p className="text-[10px] font-mono text-white/30 uppercase">Est. Duration</p>
                    <p className="text-sm font-mono text-white/80">
                      {durationH > 0 ? `${durationH}h ` : ""}{durationM}m
                      {route.total_distance_km > 0 && (
                        <span className="text-white/40"> · {route.total_distance_km.toFixed(1)}km</span>
                      )}
                    </p>
                  </div>
                </div>
                <div className="flex items-center gap-2 flex-shrink-0">
                  <p className="hidden sm:block text-[10px] font-mono text-white/25">
                    {new Date(route.created_at).toLocaleString("en-PH", {
                      month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
                    })}
                  </p>
                  {canCancel && (
                    <button
                      onClick={() => handleCancelRoute(route.route_id)}
                      disabled={cancellingRoute === route.route_id}
                      className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-1.5 text-xs font-mono text-red-400 hover:bg-red-500/20 transition-colors disabled:opacity-40"
                    >
                      {cancellingRoute === route.route_id ? "…" : "Cancel"}
                    </button>
                  )}
                </div>
              </div>
            </GlassCard>
          );
        })}
      </motion.div>
    </motion.div>
  );
}
