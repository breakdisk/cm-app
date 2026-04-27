"use client";
/**
 * Dispatch Console — Admin Portal
 * Real-time view of the dispatch queue, available drivers, and active routes.
 */
import { useMemo, useEffect, useState, useCallback, useRef, Suspense } from "react";
import { useSearchParams } from "next/navigation";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { LiveDispatchMap } from "@/components/maps/live-dispatch-map";
import { variants } from "@/lib/design-system/tokens";
import { authFetch } from "@/lib/auth/auth-fetch";
import { readBus, subscribeToBus, type BusBooking } from "@/lib/api/marketplace-bus";

// Route through the api-gateway (same base as every other admin-portal caller).
// Gateway proxies /v1/queue + /v1/drivers to dispatch+driver-ops — no service-specific URL needed.
const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

interface QueueItem {
  id:             string;
  shipment_id:    string;
  customer_name:  string;
  dest_city:      string;
  dest_address_line1: string;
  service_type:   string;
  status:         string;
  cod_amount_cents?: number | null;
  tracking_number?: string | null;   // present on rows from dispatch_queue.tracking_number (migration 0005)
  origin?:        "dispatch" | "marketplace"; // synthetic rows from accepted marketplace bookings carry origin="marketplace"
  partner_display?: string;          // marketplace-origin: which partner accepted the job
  // Auto-dispatch attempt tracking (migration 0007). Non-zero attempts means
  // the customer-booked auto-assign failed (no available driver, etc.) and
  // the row is parked here awaiting ops action. Rendered as an amber warning
  // badge so the silent-failure mode is visible at a glance.
  auto_dispatch_attempts?: number | null;
  last_dispatch_error?:    string | null;
  last_attempt_at?:        string | null;
  dispatched_at?:          string | null;
  // Set by migration 0008 — the driver who was assigned when dispatched.
  assigned_driver_id?:     string | null;
}

type QueueFilter = "pending" | "dispatched" | "all";

// Project an accepted marketplace booking into a dispatch QueueItem.
// Per ADR-0013 §Booking flow, accept → "shipment enters dispatch flow".
// Until the real /v1/marketplace/bookings endpoint + order-intake mint the
// shipment, we surface the accepted booking here so ops can see the pipeline.
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
  id:           string;
  first_name:   string;
  last_name:    string;
  email:        string;
  phone:        string;
  tenant_id:    string;
  vehicle_type: string;
  status?:      string;        // "available" | "offline" | "en_route" | "delivering" | "returning" | "on_break"
  is_online?:   boolean;
  lat?:         number | null;
  lng?:         number | null;
  active_route_id?:   string | null;
  last_location_at?:  string | null;
}

const KPI_METRICS = [
  { label: "Pending Queue",    value: 0,  trend: 0,    color: "cyan"   as const, format: "number"  as const, key: "queue"    },
  { label: "Active Drivers",   value: 0,  trend: 0,    color: "green"  as const, format: "number"  as const, key: "drivers"  },
  { label: "Success Rate",     value: 94.7, trend: +1.2, color: "green" as const, format: "percent" as const, key: "rate"   },
  { label: "Avg Delivery Time",value: 47,  trend: -5.1, color: "purple" as const, format: "duration" as const, key: "time"  },
];

function DispatchPageInner() {
  const today = useMemo(
    () => new Date().toLocaleDateString("en-PH", { month: "long", day: "numeric", year: "numeric" }),
    [],
  );

  const [queue,         setQueue]         = useState<QueueItem[]>([]);
  const [marketplaceQueue, setMarketplaceQueue] = useState<QueueItem[]>([]);
  const [drivers,       setDrivers]       = useState<DriverProfile[]>([]);
  const [dispatching,   setDispatching]   = useState<string | null>(null);
  const [selectedDriver,setSelectedDriver]= useState<string>("");
  const [loading,       setLoading]       = useState(false);
  const [error,         setError]         = useState<string | null>(null);
  const [queueFilter,   setQueueFilter]   = useState<QueueFilter>("pending");
  const [togglingDriver,setTogglingDriver]= useState<string | null>(null);

  // Deep-link from partner-portal: /admin/dispatch?order=<shipment_id> highlights
  // and scrolls to the matching queue card once data lands.
  const searchParams  = useSearchParams();
  const focusOrderId  = searchParams.get("order");
  const focusCardRef  = useRef<HTMLDivElement | null>(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    const queueUrl  = `${API_BASE}/v1/queue?status=${queueFilter}`;
    const driverUrl = `${API_BASE}/v1/drivers`;
    try {
      const [qRes, dRes] = await Promise.all([
        authFetch(queueUrl),
        authFetch(driverUrl),
      ]);
      if (qRes.ok) { const j = await qRes.json(); setQueue(j.data ?? []); }
      else {
        // eslint-disable-next-line no-console
        console.error("[dispatch] /v1/queue HTTP", qRes.status, qRes.statusText);
        setError(`Queue fetch failed: ${qRes.status} ${qRes.statusText} (${queueUrl})`);
      }
      if (dRes.ok) { const j = await dRes.json(); setDrivers(j.data ?? []); }
      else {
        // eslint-disable-next-line no-console
        console.error("[dispatch] /v1/drivers HTTP", dRes.status, dRes.statusText);
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : "Failed to load dispatch data";
      // Surface the attempted URL so ops can spot env-var misconfigurations
      // (e.g. NEXT_PUBLIC_API_URL not baked into the build → fallback to
      // http://localhost:8000 → Mixed Content block under HTTPS).
      // eslint-disable-next-line no-console
      console.error("[dispatch] fetchData threw:", msg, { queueUrl, driverUrl, API_BASE });
      setError(`${msg} — tried ${queueUrl}`);
    } finally {
      setLoading(false);
    }
  }, [queueFilter]);

  useEffect(() => { fetchData(); }, [fetchData]);

  // Marketplace-origin queue: accepted bookings from the bus become synthetic
  // queue rows (dedup'd by shipment_id against the real dispatch queue).
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

  // Scroll the focused queue card into view after the queue loads.
  useEffect(() => {
    if (focusOrderId && focusCardRef.current) {
      focusCardRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [focusOrderId, queue]);

  // Admin override — flip a driver's status. Backed by FLEET_MANAGE on
  // PUT /v1/drivers/:id/status (driver-ops). Used to pull idle drivers
  // out of the auto-dispatch pool for testing or to mark a no-show offline.
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

  // Dedup by shipment_id: real dispatch rows override synthetic marketplace
  // rows. Marketplace bookings only join the merged list under the Pending
  // tab — they're synthetic "awaiting dispatch" rows and have no meaning
  // under Dispatched/All views, which read straight from the queue table.
  const mergedQueue = useMemo(() => {
    const byId = new Map<string, QueueItem>();
    if (queueFilter === "pending") {
      marketplaceQueue.forEach((q) => byId.set(q.shipment_id, q));
    }
    queue.forEach((q) => byId.set(q.shipment_id, q));
    return Array.from(byId.values());
  }, [queue, marketplaceQueue, queueFilter]);

  // Map backend status → DriverPin status accepted by LiveDispatchMap.
  function toMapStatus(s: string | undefined): "idle" | "en_route" | "delivering" | "returning" {
    switch (s) {
      case "en_route":   return "en_route";
      case "delivering": return "delivering";
      case "returning":  return "returning";
      default:           return "idle";
    }
  }

  const mapDrivers = drivers
    .filter((d) => d.lat != null && d.lng != null)
    .map((d) => ({
      driver_id:            d.id,
      driver_name:          [d.first_name, d.last_name].filter(Boolean).join(" ") || d.email,
      lat:                  d.lat as number,
      lng:                  d.lng as number,
      status:               toMapStatus(d.status),
      deliveries_remaining: 0,
    }));

  // Build a id→name lookup so dispatched queue cards can show the driver name.
  const driverNameById = useMemo(() => {
    const m = new Map<string, string>();
    drivers.forEach((d) => {
      const name = [d.first_name, d.last_name].filter(Boolean).join(" ") || d.email;
      m.set(d.id, name);
    });
    return m;
  }, [drivers]);

  const kpiValues = [
    { ...KPI_METRICS[0], value: mergedQueue.length },
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

          {/* Driver Roster — admin override (FLEET_MANAGE). Status dot
              + force-online/offline button per driver. Used to pull idle
              drivers out of the auto-dispatch pool, e.g. for a focused
              test loop on a single device. Disabled while a driver is
              en_route — they need their route cancelled first. */}
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
                      className="flex items-center justify-between gap-2 rounded-md border border-white/5 bg-white/[0.02] px-2 py-1.5"
                    >
                      <div className="flex items-center gap-2 min-w-0">
                        <span className={`h-2 w-2 rounded-full ${dotColor}`} />
                        <span className="text-xs text-white/80 truncate">{name}</span>
                      </div>
                      <button
                        onClick={() => handleToggleDriver(d)}
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

          {/* Queue tabs — Pending = unassigned, Dispatched = already on a
              driver, All = both (read-only). Auto-dispatched merchant
              shipments go straight to Dispatched, so without this tab they
              vanish from the console even though they're alive in the system. */}
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
                          <NeonBadge variant="amber">
                            ⚠ AUTO-DISPATCH FAILED
                          </NeonBadge>
                        </div>
                      )}
                    </div>
                  </div>
                  {item.dispatched_at && (
                    <div className="flex flex-col gap-0.5">
                      <p className="text-[10px] font-mono text-white/35">
                        Dispatched {new Date(item.dispatched_at).toLocaleString("en-PH", {
                          month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
                        })}
                      </p>
                      {item.assigned_driver_id && (
                        <p className="text-[10px] font-mono text-cyan-neon/70 truncate">
                          👤 {driverNameById.get(item.assigned_driver_id) ?? item.assigned_driver_id}
                        </p>
                      )}
                    </div>
                  )}
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => handleDispatch(item.shipment_id)}
                      disabled={dispatching === item.shipment_id || item.status === "dispatched"}
                      className="flex-1 rounded-lg border border-purple-500/30 bg-purple-500/10 px-3 py-1.5 text-xs font-mono text-purple-300 hover:bg-purple-500/20 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                    >
                      {item.status === "dispatched"
                        ? (item.assigned_driver_id && driverNameById.get(item.assigned_driver_id)
                            ? `✓ ${driverNameById.get(item.assigned_driver_id)}`
                            : "✓ Dispatched")
                        : dispatching === item.shipment_id
                          ? "Dispatching…"
                          : "⚡ Dispatch"}
                    </button>
                    {item.tracking_number && (
                      // Cross-portal jump to merchant's own view — preserves /merchant basePath.
                      // Useful when ops wants to see what the merchant is seeing for this shipment.
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
