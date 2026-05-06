"use client";
/**
 * Dispatch Console — Admin Portal
 * Real-time view of the dispatch queue, available drivers, and active routes.
 */
import { useMemo, useEffect, useState, useCallback, useRef, Suspense } from "react";
import { useSearchParams } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { LiveDispatchMap } from "@/components/maps/live-dispatch-map";
import type { DriverPin } from "@/components/maps/live-dispatch-map";
import { variants } from "@/lib/design-system/tokens";
import { authFetch } from "@/lib/auth/auth-fetch";
import { readBus, subscribeToBus, type BusBooking } from "@/lib/api/marketplace-bus";

// Route through the api-gateway (same base as every other admin-portal caller).
// Gateway proxies /v1/queue + /v1/drivers to dispatch+driver-ops — no service-specific URL needed.
const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

// Manila city center — fallback when driver hasn't shared GPS yet
const MANILA_LAT = 14.5995;
const MANILA_LNG = 120.9842;

interface QueueItem {
  id:             string;
  shipment_id:    string;
  customer_name:  string;
  dest_city:      string;
  dest_address_line1: string;
  service_type:   string;
  status:         string;
  cod_amount_cents?: number | null;
  tracking_number?: string | null;
  origin?:        "dispatch" | "marketplace";
  partner_display?: string;
  auto_dispatch_attempts?: number | null;
  last_dispatch_error?:    string | null;
  last_attempt_at?:        string | null;
  dispatched_at?:          string | null;
}

type QueueFilter = "pending" | "dispatched" | "all";

function busToQueueItem(b: BusBooking): QueueItem {
  return {
    id:                 `mp-${b.id}`,
    shipment_id:        b.shipment_id,
    customer_name:      b.merchant_display,
    dest_address_line1: b.dropoff_label,
    dest_city:          b.dropoff_label.split(",").pop()?.trim() ?? "—",
    service_type:       b.size_class,
    status:             "pending",
    cod_amount_cents:   null,
    tracking_number:    b.awb,
    origin:             "marketplace",
    partner_display:    b.partner_display_name,
  };
}

interface DriverProfile {
  id:               string;
  user_id:          string;
  first_name:       string;
  last_name:        string;
  email?:           string;
  phone?:           string;
  status?:          string;
  is_online?:       boolean;
  driver_type?:     string;
  vehicle_type?:    string | null;
  zone?:            string | null;
  lat?:             number | null;
  lng?:             number | null;
  last_location_at?: string | null;
  active_route_id?: string | null;
  per_delivery_rate_cents?: number;
}

interface DriverSummary {
  online: number;
  idle: number;
  on_break: number;
  offline: number;
  total_tasks_assigned: number;
  total_tasks_completed: number;
  total_tasks_failed: number;
  total_cod_collected: number;
}

/** Map driver-ops status → LiveDispatchMap pin status */
function toMapStatus(s?: string): DriverPin["status"] {
  switch (s) {
    case "en_route":   return "en_route";
    case "delivering": return "delivering";
    case "returning":  return "returning";
    case "on_break":   return "on_break";
    case "offline":    return "offline";
    default:           return "available";
  }
}

// ── Driver Detail Drawer ───────────────────────────────────────────────────────

interface DriverDrawerProps {
  driver:   DriverProfile;
  onClose:  () => void;
  onCancelTasks: () => void;
  cancellingTasks: boolean;
}

function DriverDrawer({ driver, onClose, onCancelTasks, cancellingTasks }: DriverDrawerProps) {
  const name = [driver.first_name, driver.last_name].filter(Boolean).join(" ") || driver.email || driver.id;
  const isOnline = driver.is_online ?? (driver.status && driver.status !== "offline");
  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        className="fixed inset-0 z-50 flex items-end justify-end bg-black/40 backdrop-blur-sm"
        onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
      >
        <motion.div
          initial={{ x: "100%" }}
          animate={{ x: 0 }}
          exit={{ x: "100%" }}
          transition={{ type: "spring", stiffness: 320, damping: 30 }}
          className="h-full w-full max-w-sm border-l border-white/10 bg-[#050810]/95 backdrop-blur-xl flex flex-col overflow-y-auto"
        >
          {/* Header */}
          <div className="flex items-center justify-between border-b border-white/10 px-5 py-4">
            <div>
              <h2 className="font-heading text-base font-semibold text-white">{name}</h2>
              <p className="text-xs font-mono text-white/40">{driver.phone ?? driver.id}</p>
            </div>
            <button onClick={onClose} className="text-white/40 hover:text-white transition-colors text-xl leading-none">×</button>
          </div>

          {/* Status row */}
          <div className="border-b border-white/10 px-5 py-3 flex items-center gap-3">
            <span className={`h-2.5 w-2.5 rounded-full ${isOnline ? "bg-green-400" : "bg-white/20"}`} />
            <span className="text-sm font-mono text-white/70 capitalize">{driver.status ?? "unknown"}</span>
            {driver.vehicle_type && (
              <span className="ml-auto text-xs font-mono text-white/40">{driver.vehicle_type}</span>
            )}
          </div>

          {/* Profile */}
          <div className="px-5 py-4 flex flex-col gap-3">
            <h3 className="text-xs font-mono uppercase tracking-widest text-white/30">Profile</h3>
            <div className="grid grid-cols-2 gap-3">
              <div className="rounded-lg border border-white/5 bg-white/[0.02] px-3 py-2">
                <p className="text-[10px] font-mono text-white/30 uppercase mb-0.5">Zone</p>
                <p className="text-sm text-white/80">{driver.zone ?? "—"}</p>
              </div>
              <div className="rounded-lg border border-white/5 bg-white/[0.02] px-3 py-2">
                <p className="text-[10px] font-mono text-white/30 uppercase mb-0.5">Type</p>
                <p className="text-sm text-white/80 capitalize">{(driver.driver_type ?? "—").replace("_", " ")}</p>
              </div>
              <div className="rounded-lg border border-white/5 bg-white/[0.02] px-3 py-2">
                <p className="text-[10px] font-mono text-white/30 uppercase mb-0.5">Rate / delivery</p>
                <p className="text-sm text-white/80">
                  {driver.per_delivery_rate_cents != null
                    ? `₱${(driver.per_delivery_rate_cents / 100).toFixed(0)}`
                    : "—"}
                </p>
              </div>
              <div className="rounded-lg border border-white/5 bg-white/[0.02] px-3 py-2">
                <p className="text-[10px] font-mono text-white/30 uppercase mb-0.5">Route</p>
                <p className="text-sm text-white/80 font-mono truncate">
                  {driver.active_route_id
                    ? driver.active_route_id.slice(0, 8) + "…"
                    : "None"}
                </p>
              </div>
            </div>

            {/* GPS */}
            {driver.lat != null && driver.lng != null ? (
              <div className="rounded-lg border border-white/5 bg-white/[0.02] px-3 py-2">
                <p className="text-[10px] font-mono text-white/30 uppercase mb-0.5">Last Location</p>
                <p className="text-sm font-mono text-cyan-400/80">
                  {driver.lat.toFixed(5)}, {driver.lng.toFixed(5)}
                </p>
                {driver.last_location_at && (
                  <p className="text-[10px] font-mono text-white/25 mt-0.5">
                    {new Date(driver.last_location_at).toLocaleTimeString("en-PH")}
                  </p>
                )}
              </div>
            ) : (
              <p className="text-xs font-mono text-white/25 italic">No GPS signal yet</p>
            )}
          </div>

          {/* Actions */}
          <div className="mt-auto border-t border-white/10 px-5 py-4 flex flex-col gap-2">
            <button
              onClick={onCancelTasks}
              disabled={cancellingTasks}
              className="w-full rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-2 text-sm font-mono text-red-400 hover:bg-red-500/20 transition-colors disabled:opacity-40"
            >
              {cancellingTasks ? "Cancelling…" : "Cancel Active Tasks"}
            </button>
          </div>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}

// ── Main Page ─────────────────────────────────────────────────────────────────

function DispatchPageInner() {
  const today = useMemo(
    () => new Date().toLocaleDateString("en-PH", { month: "long", day: "numeric", year: "numeric" }),
    [],
  );

  const [queue,             setQueue]             = useState<QueueItem[]>([]);
  const [marketplaceQueue,  setMarketplaceQueue]  = useState<QueueItem[]>([]);
  const [drivers,           setDrivers]           = useState<DriverProfile[]>([]);
  const [driverSummary,     setDriverSummary]     = useState<DriverSummary | null>(null);
  const [dispatching,       setDispatching]       = useState<string | null>(null);
  const [cancellingDispatch,setCancellingDispatch]= useState<string | null>(null);
  const [selectedDriver,    setSelectedDriver]    = useState<string>("");
  const [loading,           setLoading]           = useState(false);
  const [error,             setError]             = useState<string | null>(null);
  const [queueFilter,       setQueueFilter]       = useState<QueueFilter>("pending");
  const [togglingDriver,    setTogglingDriver]    = useState<string | null>(null);
  const [drawerDriver,      setDrawerDriver]      = useState<DriverProfile | null>(null);
  const [cancellingTasks,   setCancellingTasks]   = useState(false);

  const searchParams  = useSearchParams();
  const focusOrderId  = searchParams.get("order");
  const focusCardRef  = useRef<HTMLDivElement | null>(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    const queueUrl   = `${API_BASE}/v1/queue?status=${queueFilter}`;
    const driverUrl  = `${API_BASE}/v1/drivers`;
    const summaryUrl = `${API_BASE}/v1/drivers/summary`;
    try {
      const [qRes, dRes, sRes] = await Promise.all([
        authFetch(queueUrl),
        authFetch(driverUrl),
        authFetch(summaryUrl),
      ]);
      if (qRes.ok) { const j = await qRes.json(); setQueue(j.data ?? []); }
      else {
        console.error("[dispatch] /v1/queue HTTP", qRes.status, qRes.statusText);
        setError(`Queue fetch failed: ${qRes.status} ${qRes.statusText}`);
      }
      if (dRes.ok) { const j = await dRes.json(); setDrivers(j.data ?? []); }
      else {
        console.error("[dispatch] /v1/drivers HTTP", dRes.status, dRes.statusText);
      }
      if (sRes.ok) { const j = await sRes.json(); setDriverSummary(j.data ?? null); }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : "Failed to load dispatch data";
      console.error("[dispatch] fetchData threw:", msg, { queueUrl, driverUrl, API_BASE });
      setError(`${msg} — tried ${queueUrl}`);
    } finally {
      setLoading(false);
    }
  }, [queueFilter]);

  useEffect(() => { fetchData(); }, [fetchData]);

  // Marketplace-origin queue: accepted bookings from the bus become synthetic queue rows
  const refreshMarketplaceQueue = useCallback(() => {
    const accepted = readBus()
      .filter((b) => b.status === "accepted")
      .map(busToQueueItem);
    setMarketplaceQueue(accepted);
  }, []);

  useEffect(() => {
    refreshMarketplaceQueue();
    const unsubscribe = subscribeToBus(refreshMarketplaceQueue);
    return unsubscribe;
  }, [refreshMarketplaceQueue]);

  useEffect(() => {
    if (focusOrderId && focusCardRef.current) {
      focusCardRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [focusOrderId, queue]);

  async function handleToggleDriver(driver: DriverProfile) {
    const isAvailable = driver.status === "available";
    const next = isAvailable ? "offline" : "available";
    setTogglingDriver(driver.id);
    try {
      const res = await authFetch(`${API_BASE}/v1/drivers/${driver.id}/status`, {
        method: "PUT",
        body: JSON.stringify({ status: next }),
      });
      if (!res.ok) {
        const j = await res.json().catch(() => ({}));
        throw new Error(j.error?.message ?? `Status flip failed (${res.status})`);
      }
      await fetchData();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Driver status flip failed");
    } finally {
      setTogglingDriver(null);
    }
  }

  async function handleDispatch(shipmentId: string) {
    setDispatching(shipmentId);
    try {
      const body = selectedDriver ? { preferred_driver_id: selectedDriver } : {};
      const res = await authFetch(`${API_BASE}/v1/queue/${shipmentId}/dispatch`, {
        method: "POST",
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

  async function handleCancelDispatch(shipmentId: string) {
    setCancellingDispatch(shipmentId);
    try {
      const res = await authFetch(`${API_BASE}/v1/queue/${shipmentId}/cancel-dispatch`, {
        method: "POST",
      });
      if (!res.ok) {
        const j = await res.json().catch(() => ({}));
        throw new Error(j.error?.message ?? `Cancel dispatch failed (${res.status})`);
      }
      await fetchData();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Cancel dispatch error");
    } finally {
      setCancellingDispatch(null);
    }
  }

  async function handleCancelTasks(driver: DriverProfile) {
    setCancellingTasks(true);
    try {
      const res = await authFetch(`${API_BASE}/v1/drivers/${driver.id}/cancel-tasks`, {
        method: "POST",
      });
      if (!res.ok) {
        const j = await res.json().catch(() => ({}));
        throw new Error(j.error?.message ?? `Cancel tasks failed (${res.status})`);
      }
      setDrawerDriver(null);
      await fetchData();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Cancel tasks error");
    } finally {
      setCancellingTasks(false);
    }
  }

  // Dedup by shipment_id: real dispatch rows override synthetic marketplace rows.
  const mergedQueue = useMemo(() => {
    const byId = new Map<string, QueueItem>();
    if (queueFilter === "pending") {
      marketplaceQueue.forEach((q) => byId.set(q.shipment_id, q));
    }
    queue.forEach((q) => byId.set(q.shipment_id, q));
    return Array.from(byId.values());
  }, [queue, marketplaceQueue, queueFilter]);

  // Build driver pins for the live map using real GPS coordinates.
  // Falls back to Manila city centre only when a driver has never shared location.
  const mapDrivers: DriverPin[] = drivers
    .filter((d) => d.status !== "offline" || d.lat != null)
    .map((d) => ({
      driver_id:            d.id,
      driver_name:          [d.first_name, d.last_name].filter(Boolean).join(" ") || d.email || d.id,
      lat:                  d.lat ?? MANILA_LAT,
      lng:                  d.lng ?? MANILA_LNG,
      status:               toMapStatus(d.status),
      deliveries_remaining: 0,
    }));

  // KPIs: queue length and live driver count are always exact.
  // Success rate and avg delivery time come from the driver-ops summary endpoint.
  const successRate = driverSummary && (driverSummary.total_tasks_completed + driverSummary.total_tasks_failed) > 0
    ? (driverSummary.total_tasks_completed / (driverSummary.total_tasks_completed + driverSummary.total_tasks_failed)) * 100
    : null;

  const onlineDriverCount = driverSummary?.online ?? drivers.filter((d) => d.is_online).length;

  const kpiValues = [
    { label: "Pending Queue",     value: mergedQueue.length,              trend: 0,    color: "cyan"   as const, format: "number"  as const, live: true  },
    { label: "Online Drivers",    value: onlineDriverCount,               trend: 0,    color: "green"  as const, format: "number"  as const, live: false },
    { label: "Success Rate",      value: successRate ?? 0,                trend: 0,    color: "green"  as const, format: "percent" as const, live: false },
    { label: "Tasks Completed",   value: driverSummary?.total_tasks_completed ?? 0, trend: 0, color: "purple" as const, format: "number" as const, live: false },
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
              live={m.live}
            />
          </GlassCard>
        ))}
      </motion.div>

      {/* Map + panels */}
      <div className="flex flex-col flex-1 gap-4 min-h-0 lg:flex-row">
        <motion.div variants={variants.fadeInUp} className="flex-1">
          <LiveDispatchMap
            drivers={mapDrivers}
            onDriverClick={(pin) => {
              const d = drivers.find((dr) => dr.id === pin.driver_id);
              if (d) setDrawerDriver(d);
            }}
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
                    {d.status === "available" ? " ✓" : ""}
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* Driver Roster */}
          {drivers.length > 0 && (
            <div>
              <span className="text-xs font-mono uppercase tracking-widest text-white/30 mb-1 block">
                Driver Roster · {drivers.length}
              </span>
              <div className="flex flex-col gap-1.5 overflow-y-auto max-h-[180px] pr-1">
                {drivers.map((d) => {
                  const name = [d.first_name, d.last_name].filter(Boolean).join(" ") || d.email;
                  const isAvailable = d.status === "available";
                  const isOffline   = d.status === "offline";
                  const isOnRoute   = !!d.active_route_id;
                  const dotColor =
                    isAvailable ? "bg-green-400" :
                    isOnRoute   ? "bg-cyan-400"  :
                    isOffline   ? "bg-white/20"  :
                                  "bg-amber-400";
                  return (
                    <div
                      key={d.id}
                      className="flex items-center justify-between gap-2 rounded-md border border-white/5 bg-white/[0.02] px-2 py-1.5 cursor-pointer hover:border-white/15 transition-colors"
                      onClick={() => setDrawerDriver(d)}
                    >
                      <div className="flex items-center gap-2 min-w-0">
                        <span className={`h-2 w-2 rounded-full flex-shrink-0 ${dotColor}`} />
                        <span className="text-xs text-white/80 truncate">{name}</span>
                        {d.lat != null && (
                          <span className="text-[9px] font-mono text-cyan-400/50 flex-shrink-0" title="GPS active">●</span>
                        )}
                      </div>
                      <button
                        onClick={(e) => { e.stopPropagation(); handleToggleDriver(d); }}
                        disabled={togglingDriver === d.id || isOnRoute}
                        title={isOnRoute ? "Cancel route before changing status" : ""}
                        className={`text-[10px] font-mono uppercase tracking-wider px-2 py-0.5 rounded transition-colors disabled:opacity-30 disabled:cursor-not-allowed ${
                          isAvailable
                            ? "text-amber-300 hover:bg-amber-500/15 border border-amber-500/30"
                            : "text-green-300 hover:bg-green-500/15 border border-green-500/30"
                        }`}
                      >
                        {togglingDriver === d.id
                          ? "…"
                          : isAvailable
                            ? "→ Offline"
                            : "→ Online"}
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Queue tabs */}
          <div className="flex items-center gap-1 rounded-lg border border-white/10 bg-white/[0.02] p-1">
            {(["pending", "dispatched", "all"] as const).map((t) => (
              <button
                key={t}
                onClick={() => setQueueFilter(t)}
                className={`flex-1 rounded-md px-2 py-1.5 text-[11px] font-mono uppercase tracking-wider transition-colors ${
                  queueFilter === t
                    ? "bg-cyan-500/15 text-cyan-300 ring-1 ring-cyan-500/40"
                    : "text-white/40 hover:text-white/70"
                }`}
              >
                {t}
              </button>
            ))}
          </div>

          <span className="text-xs font-mono uppercase tracking-widest text-white/30">
            {queueFilter === "pending" ? "Pending Queue" :
             queueFilter === "dispatched" ? "Dispatched" :
             "All Shipments"} · {mergedQueue.length}
          </span>

          {mergedQueue.length === 0 && !loading && (
            <p className="text-xs font-mono text-white/25 text-center py-4">
              {queueFilter === "pending"    ? "No pending shipments"    :
               queueFilter === "dispatched" ? "No dispatched shipments" :
                                              "No shipments"}
            </p>
          )}

          <div className="flex flex-col gap-2 overflow-y-auto max-h-[400px] pr-1">
            {mergedQueue.map((item) => {
              const isFocused = focusOrderId === item.shipment_id || focusOrderId === item.id;
              return (
              <div key={item.id} ref={isFocused ? focusCardRef : undefined}>
              <GlassCard size="sm" glow="cyan" className={isFocused ? "ring-1 ring-cyan-neon/60" : undefined}>
                <div className="flex flex-col gap-2">
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <p className="text-sm font-medium text-white truncate">{item.customer_name}</p>
                      <p className="text-xs font-mono text-white/40 truncate">{item.dest_address_line1}</p>
                      <p className="text-xs font-mono text-white/25">{item.dest_city} · {item.service_type}</p>
                      {item.origin === "marketplace" && item.partner_display && (
                        <p className="text-[10px] font-mono text-cyan-neon/70 mt-1 truncate">
                          via {item.partner_display}
                        </p>
                      )}
                    </div>
                    <div className="flex flex-col items-end gap-1">
                      {item.origin === "marketplace" && (
                        <NeonBadge variant="cyan">Marketplace</NeonBadge>
                      )}
                      {item.status === "dispatched" && (
                        <NeonBadge variant="green">Dispatched</NeonBadge>
                      )}
                      {item.cod_amount_cents && (
                        <NeonBadge variant="amber">COD</NeonBadge>
                      )}
                      {(item.auto_dispatch_attempts ?? 0) > 0 && (
                        <div
                          title={item.last_dispatch_error ?? "Auto-dispatch failed — no available driver"}
                          className="cursor-help"
                        >
                          <NeonBadge variant="amber">⚠ AUTO-DISPATCH FAILED</NeonBadge>
                        </div>
                      )}
                    </div>
                  </div>
                  {item.dispatched_at && (
                    <p className="text-[10px] font-mono text-white/35">
                      Dispatched {new Date(item.dispatched_at).toLocaleString("en-PH", {
                        month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
                      })}
                    </p>
                  )}
                  <div className="flex items-center gap-2">
                    {item.status !== "dispatched" ? (
                      <button
                        onClick={() => handleDispatch(item.shipment_id)}
                        disabled={dispatching === item.shipment_id}
                        className="flex-1 rounded-lg border border-purple-500/30 bg-purple-500/10 px-3 py-1.5 text-xs font-mono text-purple-300 hover:bg-purple-500/20 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                      >
                        {dispatching === item.shipment_id ? "Dispatching…" : "⚡ Dispatch"}
                      </button>
                    ) : (
                      <button
                        onClick={() => handleCancelDispatch(item.shipment_id)}
                        disabled={cancellingDispatch === item.shipment_id || item.origin === "marketplace"}
                        title={item.origin === "marketplace" ? "Marketplace bookings cannot be cancelled here" : "Return to pending queue"}
                        className="flex-1 rounded-lg border border-red-500/20 bg-red-500/5 px-3 py-1.5 text-xs font-mono text-red-400/80 hover:bg-red-500/15 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                      >
                        {cancellingDispatch === item.shipment_id ? "Cancelling…" : "✕ Cancel Dispatch"}
                      </button>
                    )}
                    {item.tracking_number && (
                      <a
                        href={`/merchant/shipments?awb=${encodeURIComponent(item.tracking_number)}`}
                        title="Open in Merchant Portal"
                        className="flex-shrink-0 rounded-lg border border-glass-border bg-glass-100 px-2 py-1.5 text-xs font-mono text-white/50 hover:text-cyan-neon hover:border-cyan-neon/30 transition-colors"
                      >
                        ↗ Merchant
                      </a>
                    )}
                  </div>
                </div>
              </GlassCard>
              </div>
              );
            })}
          </div>
        </motion.div>
      </div>

      {/* Driver Detail Drawer */}
      {drawerDriver && (
        <DriverDrawer
          driver={drawerDriver}
          onClose={() => setDrawerDriver(null)}
          onCancelTasks={() => handleCancelTasks(drawerDriver)}
          cancellingTasks={cancellingTasks}
        />
      )}
    </motion.div>
  );
}

export default function DispatchPage() {
  return (
    <Suspense fallback={null}>
      <DispatchPageInner />
    </Suspense>
  );
}
