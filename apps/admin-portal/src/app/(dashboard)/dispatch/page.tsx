"use client";
/**
 * Dispatch Console — Admin Portal
 * Real-time view of the dispatch queue, available drivers, and active routes.
 */
import { useMemo, useEffect, useState, useCallback } from "react";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { LiveDispatchMap } from "@/components/maps/live-dispatch-map";
import { variants } from "@/lib/design-system/tokens";

const DISPATCH_URL   = process.env.NEXT_PUBLIC_DISPATCH_URL   ?? "http://localhost:8005";
const DRIVER_OPS_URL = process.env.NEXT_PUBLIC_DRIVER_OPS_URL ?? "http://localhost:8006";

function getToken(): string {
  return typeof window !== "undefined" ? localStorage.getItem("access_token") ?? "" : "";
}

interface QueueItem {
  id:             string;
  shipment_id:    string;
  customer_name:  string;
  dest_city:      string;
  dest_address_line1: string;
  service_type:   string;
  status:         string;
  cod_amount_cents?: number | null;
}

interface DriverProfile {
  id:         string;
  first_name: string;
  last_name:  string;
  email:      string;
  tenant_id:  string;
}

interface ActiveTask {
  task_id:          string;
  shipment_id:      string;
  driver_id:        string;
  sequence:         number;
  status:           string;
  task_type:        string;
  customer_name:    string;
  address:          string;
  cod_amount_cents?: number | null;
}

const KPI_METRICS = [
  { label: "Pending Queue",    value: 0,  trend: 0,    color: "cyan"   as const, format: "number"  as const, key: "queue"    },
  { label: "Active Drivers",   value: 0,  trend: 0,    color: "green"  as const, format: "number"  as const, key: "drivers"  },
  { label: "Success Rate",     value: 94.7, trend: +1.2, color: "green" as const, format: "percent" as const, key: "rate"   },
  { label: "Avg Delivery Time",value: 47,  trend: -5.1, color: "purple" as const, format: "duration" as const, key: "time"  },
];

export default function DispatchPage() {
  const today = useMemo(
    () => new Date().toLocaleDateString("en-PH", { month: "long", day: "numeric", year: "numeric" }),
    [],
  );

  const [queue,         setQueue]         = useState<QueueItem[]>([]);
  const [drivers,       setDrivers]       = useState<DriverProfile[]>([]);
  const [tasks,         setTasks]         = useState<ActiveTask[]>([]);
  const [dispatching,   setDispatching]   = useState<string | null>(null);
  const [taskActing,    setTaskActing]    = useState<string | null>(null);
  const [selectedDriver,setSelectedDriver]= useState<string>("");
  const [loading,       setLoading]       = useState(false);
  const [error,         setError]         = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    const token = getToken();
    if (!token) return;
    setLoading(true);
    setError(null);
    try {
      const [qRes, dRes, tRes] = await Promise.all([
        fetch(`${DISPATCH_URL}/v1/queue`,        { headers: { Authorization: `Bearer ${token}` } }),
        fetch(`${DISPATCH_URL}/v1/drivers`,      { headers: { Authorization: `Bearer ${token}` } }),
        fetch(`${DRIVER_OPS_URL}/v1/admin/tasks`,{ headers: { Authorization: `Bearer ${token}` } }),
      ]);
      if (qRes.ok) { const j = await qRes.json(); setQueue(j.data ?? []); }
      if (dRes.ok) { const j = await dRes.json(); setDrivers(j.data ?? []); }
      if (tRes.ok) { const j = await tRes.json(); setTasks(j.data ?? []); }
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to load dispatch data");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    const id = setInterval(fetchData, 10_000);
    return () => clearInterval(id);
  }, [fetchData]);

  async function handleTaskAction(taskId: string, action: "start" | "complete") {
    const token = getToken();
    if (!token) return;
    setTaskActing(taskId);
    try {
      const res = await fetch(`${DRIVER_OPS_URL}/v1/admin/tasks/${taskId}/${action}`, {
        method:  "PUT",
        headers: {
          "Content-Type": "application/json",
          Authorization:  `Bearer ${token}`,
        },
        body: action === "complete" ? JSON.stringify({}) : undefined,
      });
      if (!res.ok) {
        const j = await res.json().catch(() => ({}));
        throw new Error(j.error?.message ?? `Task ${action} failed`);
      }
      await fetchData();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : `Task ${action} error`);
    } finally {
      setTaskActing(null);
    }
  }

  async function handleDispatch(shipmentId: string) {
    const token = getToken();
    if (!token) return;
    setDispatching(shipmentId);
    try {
      const body = selectedDriver ? { preferred_driver_id: selectedDriver } : {};
      const res = await fetch(`${DISPATCH_URL}/v1/queue/${shipmentId}/dispatch`, {
        method: "POST",
        headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
        body: JSON.stringify(body),
      });
      if (!res.ok) {
        const j = await res.json();
        throw new Error(j.error?.message ?? "Dispatch failed");
      }
      await fetchData();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Dispatch error");
    } finally {
      setDispatching(null);
    }
  }

  const mapDrivers = drivers.map((d) => ({
    driver_id:             d.id,
    driver_name:           [d.first_name, d.last_name].filter(Boolean).join(" ") || d.email,
    lat:                   14.5995,
    lng:                   120.9842,
    status:                "idle" as const,
    deliveries_remaining:  0,
  }));

  const kpiValues = [
    { ...KPI_METRICS[0], value: queue.length },
    { ...KPI_METRICS[1], value: drivers.length },
    KPI_METRICS[2],
    KPI_METRICS[3],
  ];

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex h-full flex-col gap-4"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="font-heading text-xl font-bold text-white sm:text-2xl">Dispatch Console</h1>
          <p className="text-sm text-white/40 font-mono">Live Operations · {today}</p>
        </div>
        <div className="flex items-center gap-2">
          {loading && <span className="text-xs font-mono text-white/30 animate-pulse">Syncing…</span>}
          <NeonBadge variant="green" dot pulse>Live</NeonBadge>
          <button
            onClick={fetchData}
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

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {kpiValues.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric
              label={m.label}
              value={m.value}
              trend={m.trend}
              color={m.color}
              format={m.format}
              live={m.key === "queue"}
            />
          </GlassCard>
        ))}
      </motion.div>

      {/* Map + panels */}
      <div className="flex flex-col flex-1 gap-4 min-h-0 lg:flex-row">
        <motion.div variants={variants.fadeInUp} className="flex-1">
          <LiveDispatchMap
            drivers={mapDrivers}
            className="h-full min-h-[320px] sm:min-h-[420px] lg:min-h-[500px]"
          />
        </motion.div>

        <motion.div variants={variants.fadeInUp} className="flex flex-col gap-3 lg:w-80">

          {/* Driver selector */}
          {drivers.length > 0 && (
            <div>
              <span className="text-xs font-mono uppercase tracking-widest text-white/30 mb-1 block">
                Select Driver
              </span>
              <select
                value={selectedDriver}
                onChange={(e) => setSelectedDriver(e.target.value)}
                className="w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm font-mono text-white/80 focus:border-cyan-500/50 focus:outline-none"
              >
                <option value="">Auto-select nearest</option>
                {drivers.map((d) => (
                  <option key={d.id} value={d.id}>
                    {[d.first_name, d.last_name].filter(Boolean).join(" ") || d.email}
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* Queue */}
          <span className="text-xs font-mono uppercase tracking-widest text-white/30">
            Pending Queue · {queue.length}
          </span>

          {queue.length === 0 && !loading && (
            <p className="text-xs font-mono text-white/25 text-center py-4">No pending shipments</p>
          )}

          <div className="flex flex-col gap-2 overflow-y-auto max-h-[260px] pr-1">
            {queue.map((item) => (
              <GlassCard key={item.id} size="sm" glow="cyan">
                <div className="flex flex-col gap-2">
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <p className="text-sm font-medium text-white truncate">{item.customer_name}</p>
                      <p className="text-xs font-mono text-white/40 truncate">{item.dest_address_line1}</p>
                      <p className="text-xs font-mono text-white/25">{item.dest_city} · {item.service_type}</p>
                    </div>
                    {item.cod_amount_cents && (
                      <NeonBadge variant="amber">COD</NeonBadge>
                    )}
                  </div>
                  <button
                    onClick={() => handleDispatch(item.shipment_id)}
                    disabled={dispatching === item.shipment_id}
                    className="w-full rounded-lg border border-purple-500/30 bg-purple-500/10 px-3 py-1.5 text-xs font-mono text-purple-300 hover:bg-purple-500/20 transition-colors disabled:opacity-40"
                  >
                    {dispatching === item.shipment_id ? "Dispatching…" : "⚡ Dispatch"}
                  </button>
                </div>
              </GlassCard>
            ))}
          </div>

          {/* Active Tasks — admin override controls for picked-up / delivered */}
          <span className="text-xs font-mono uppercase tracking-widest text-white/30 mt-2">
            Active Tasks · {tasks.length}
          </span>

          {tasks.length === 0 && !loading && (
            <p className="text-xs font-mono text-white/25 text-center py-4">No active tasks</p>
          )}

          <div className="flex flex-col gap-2 overflow-y-auto max-h-[300px] pr-1">
            {tasks.map((t) => {
              const isPending    = t.status === "pending";
              const isInProgress = t.status === "inprogress" || t.status === "in_progress";
              const glow         = isInProgress ? "green" : "cyan";
              const badgeVariant = isInProgress ? "green" : "cyan";
              return (
                <GlassCard key={t.task_id} size="sm" glow={glow}>
                  <div className="flex flex-col gap-2">
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0">
                        <p className="text-sm font-medium text-white truncate">{t.customer_name}</p>
                        <p className="text-xs font-mono text-white/40 truncate">{t.address}</p>
                        <p className="text-xs font-mono text-white/25">
                          #{t.sequence} · {t.task_type}
                        </p>
                      </div>
                      <NeonBadge variant={badgeVariant}>{t.status}</NeonBadge>
                    </div>
                    {isPending && (
                      <button
                        onClick={() => handleTaskAction(t.task_id, "start")}
                        disabled={taskActing === t.task_id}
                        className="w-full rounded-lg border border-cyan-500/30 bg-cyan-500/10 px-3 py-1.5 text-xs font-mono text-cyan-300 hover:bg-cyan-500/20 transition-colors disabled:opacity-40"
                      >
                        {taskActing === t.task_id ? "Working…" : "📦 Mark Picked Up"}
                      </button>
                    )}
                    {isInProgress && (
                      <button
                        onClick={() => handleTaskAction(t.task_id, "complete")}
                        disabled={taskActing === t.task_id}
                        className="w-full rounded-lg border border-green-500/30 bg-green-500/10 px-3 py-1.5 text-xs font-mono text-green-300 hover:bg-green-500/20 transition-colors disabled:opacity-40"
                      >
                        {taskActing === t.task_id ? "Working…" : "✓ Mark Delivered"}
                      </button>
                    )}
                  </div>
                </GlassCard>
              );
            })}
          </div>
        </motion.div>
      </div>
    </motion.div>
  );
}
