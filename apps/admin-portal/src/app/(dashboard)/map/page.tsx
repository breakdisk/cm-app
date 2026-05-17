"use client";
/**
 * Admin Portal — Live Map Page
 * Real-time driver map with Mapbox dark theme, driver markers, delivery zones.
 *
 * Initial roster comes from `/v1/drivers`; live updates stream from the
 * driver-ops RosterEvent WebSocket (tenant-filtered server-side).
 */
import { useEffect, useMemo, useRef, useState, Suspense, useCallback } from "react";
import { useSearchParams } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { LiveDispatchMap, type DriverPin } from "@/components/maps/live-dispatch-map";
import { createDriversApi } from "@/lib/api/drivers";
import { useRosterEvents } from "@/hooks/useRosterEvents";
import { MapPin, Navigation, Filter, ExternalLink, Check, Eye, EyeOff } from "lucide-react";

// Backend taxonomy — what the API and WS emit.
type BackendStatus =
  | "offline" | "available" | "en_route" | "delivering" | "returning" | "on_break";

interface LiveDriver {
  driver_id: string;
  driver_name: string;
  lat: number;
  lng: number;
  status: BackendStatus;
  deliveries_remaining: number;
  location_label: string;
}

// Map the backend status into the narrower `DriverPin` taxonomy the map uses.
function toPinStatus(s: BackendStatus): DriverPin["status"] {
  switch (s) {
    case "en_route":   return "en_route";
    case "delivering": return "delivering";
    case "returning":  return "returning";
    case "available":  return "available";
    case "on_break":   return "on_break";
    default:           return "available";
  }
}

type StatusFilter = "all" | "delivering" | "available" | "en_route" | "on_break" | "returning";

const STATUS_FILTER_OPTIONS: { value: StatusFilter; label: string }[] = [
  { value: "all",        label: "All drivers"   },
  { value: "delivering", label: "Delivering"    },
  { value: "en_route",   label: "En route"      },
  { value: "available",  label: "Available"     },
  { value: "on_break",   label: "On break"      },
  { value: "returning",  label: "Returning"     },
];

// Layers that can be toggled on the map.
interface LayerState {
  showNames:     boolean;
  showZones:     boolean;
  showOffline:   boolean;
}

function LiveMapPageInner() {
  const [drivers, setDrivers] = useState<Record<string, LiveDriver>>({});
  const searchParams = useSearchParams();
  const focusId     = searchParams.get("driver");
  const rowRefs     = useRef<Record<string, HTMLDivElement | null>>({});

  // Filter + layers panel state
  const [statusFilter, setStatusFilter]  = useState<StatusFilter>("all");
  const [showFilterPanel, setShowFilterPanel] = useState(false);
  const [showLayersPanel, setShowLayersPanel] = useState(false);
  const [layers, setLayers] = useState<LayerState>({
    showNames:   true,
    showZones:   false,
    showOffline: false,
  });

  // Ref to close panels on outside click
  const filterRef = useRef<HTMLDivElement>(null);
  const layersRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function onOutside(e: MouseEvent) {
      if (filterRef.current && !filterRef.current.contains(e.target as Node)) setShowFilterPanel(false);
      if (layersRef.current && !layersRef.current.contains(e.target as Node)) setShowLayersPanel(false);
    }
    document.addEventListener("mousedown", onOutside);
    return () => document.removeEventListener("mousedown", onOutside);
  }, []);

  // Initial fetch — populates the roster + gives us a baseline to patch.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const api = createDriversApi();
        const res = await api.listDrivers({ per_page: 200 });
        if (cancelled) return;
        const seed: Record<string, LiveDriver> = {};
        for (const d of res.data) {
          seed[d.id] = {
            driver_id:            d.id,
            driver_name:          d.name,
            lat:                  d.lat ?? 14.5995,
            lng:                  d.lng ?? 120.9842,
            status:               (d.status as BackendStatus) ?? "offline",
            deliveries_remaining: Math.max(0, d.tasks_total - d.tasks_done),
            location_label:       d.last_location ?? "—",
          };
        }
        setDrivers(seed);
      } catch {
        // Leave empty — the WS may still populate as drivers come online.
      }
    })();
    return () => { cancelled = true; };
  }, []);

  useRosterEvents((event) => {
    setDrivers((prev) => {
      const existing = prev[event.driver_id];
      const base: LiveDriver = existing ?? {
        driver_id:            event.driver_id,
        driver_name:          "Driver",
        lat:                  0,
        lng:                  0,
        status:               "offline",
        deliveries_remaining: 0,
        location_label:       "—",
      };
      if (event.type === "location_updated") {
        return {
          ...prev,
          [event.driver_id]: {
            ...base,
            lat: event.lat,
            lng: event.lng,
            location_label: `${event.lat.toFixed(4)}, ${event.lng.toFixed(4)}`,
          },
        };
      }
      return { ...prev, [event.driver_id]: { ...base, status: event.status } };
    });
  });

  const driverList = useMemo(() => Object.values(drivers), [drivers]);
  const active = useMemo(
    () => driverList.filter((d) =>
      layers.showOffline ? true : d.status !== "offline"
    ),
    [driverList, layers.showOffline],
  );

  // Apply status filter to sidebar list and map pins.
  const filtered = useMemo(
    () => statusFilter === "all" ? active : active.filter((d) => d.status === statusFilter),
    [active, statusFilter],
  );

  const pins = useMemo<DriverPin[]>(
    () => filtered.map((d) => ({
      driver_id:            d.driver_id,
      driver_name:          d.driver_name,
      lat:                  d.lat,
      lng:                  d.lng,
      status:               toPinStatus(d.status),
      deliveries_remaining: d.deliveries_remaining,
    })),
    [filtered],
  );

  // When the focused driver row renders, scroll it into view once.
  useEffect(() => {
    if (focusId && rowRefs.current[focusId]) {
      rowRefs.current[focusId]?.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [focusId, drivers]);

  // Map marker click → scroll sidebar to matching row and highlight it.
  const [selectedDriverId, setSelectedDriverId] = useState<string | null>(focusId);

  const handleDriverClick = useCallback((pin: DriverPin) => {
    setSelectedDriverId(pin.driver_id);
    const el = rowRefs.current[pin.driver_id];
    if (el) el.scrollIntoView({ behavior: "smooth", block: "center" });
  }, []);

  const onlineCount     = active.filter((d) => d.status === "available" || d.status === "on_break").length;
  const deliveringCount = active.filter((d) => d.status === "delivering" || d.status === "en_route").length;
  const idleCount       = active.filter((d) => d.status === "available").length;
  const activeStops     = active.reduce((acc, d) => acc + d.deliveries_remaining, 0);

  const kpi = [
    { label: "Online Drivers", value: active.length,    trend: 0, color: "green"  as const, format: "number" as const },
    { label: "Delivering",     value: deliveringCount,  trend: 0, color: "cyan"   as const, format: "number" as const },
    { label: "Idle",           value: idleCount,        trend: 0, color: "amber"  as const, format: "number" as const },
    { label: "Active Stops",   value: activeStops,      trend: 0, color: "purple" as const, format: "number" as const },
  ];

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-4 p-6"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
            <MapPin size={22} className="text-cyan-neon" />
            Live Map
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">Real-time driver positions · Metro Manila</p>
        </div>
        <div className="flex items-center gap-2">

          {/* Layers panel */}
          <div ref={layersRef} className="relative">
            <button
              onClick={() => { setShowLayersPanel((v) => !v); setShowFilterPanel(false); }}
              className={`flex items-center gap-1.5 rounded-lg border px-3 py-2 text-xs transition-colors ${
                showLayersPanel
                  ? "border-purple-plasma/40 bg-purple-plasma/10 text-purple-plasma"
                  : "border-glass-border bg-glass-100 text-white/60 hover:text-white"
              }`}
            >
              {showLayersPanel ? <EyeOff size={12} /> : <Eye size={12} />}
              Layers
            </button>
            <AnimatePresence>
              {showLayersPanel && (
                <motion.div
                  initial={{ opacity: 0, y: -4, scale: 0.96 }}
                  animate={{ opacity: 1, y: 0, scale: 1 }}
                  exit={{ opacity: 0, y: -4, scale: 0.96 }}
                  transition={{ duration: 0.12 }}
                  className="absolute right-0 top-full mt-2 z-30 w-52 rounded-xl border border-glass-border bg-canvas-100 shadow-xl p-1"
                >
                  {([
                    { key: "showNames",   label: "Driver names on map" },
                    { key: "showZones",   label: "Delivery zones" },
                    { key: "showOffline", label: "Show offline drivers" },
                  ] as { key: keyof LayerState; label: string }[]).map(({ key, label }) => (
                    <button
                      key={key}
                      onClick={() => setLayers((l) => ({ ...l, [key]: !l[key] }))}
                      className="flex w-full items-center justify-between rounded-lg px-3 py-2 text-xs text-white/70 hover:bg-glass-100 transition-colors"
                    >
                      {label}
                      {layers[key] && <Check size={11} className="text-cyan-neon" />}
                    </button>
                  ))}
                </motion.div>
              )}
            </AnimatePresence>
          </div>

          {/* Filter panel */}
          <div ref={filterRef} className="relative">
            <button
              onClick={() => { setShowFilterPanel((v) => !v); setShowLayersPanel(false); }}
              className={`flex items-center gap-1.5 rounded-lg border px-3 py-2 text-xs transition-colors ${
                statusFilter !== "all"
                  ? "border-cyan-neon/40 bg-cyan-neon/10 text-cyan-neon"
                  : showFilterPanel
                    ? "border-cyan-neon/40 bg-cyan-neon/10 text-cyan-neon"
                    : "border-glass-border bg-glass-100 text-white/60 hover:text-white"
              }`}
            >
              <Filter size={12} />
              {statusFilter === "all" ? "Filter" : STATUS_FILTER_OPTIONS.find((o) => o.value === statusFilter)?.label ?? "Filter"}
            </button>
            <AnimatePresence>
              {showFilterPanel && (
                <motion.div
                  initial={{ opacity: 0, y: -4, scale: 0.96 }}
                  animate={{ opacity: 1, y: 0, scale: 1 }}
                  exit={{ opacity: 0, y: -4, scale: 0.96 }}
                  transition={{ duration: 0.12 }}
                  className="absolute right-0 top-full mt-2 z-30 w-44 rounded-xl border border-glass-border bg-canvas-100 shadow-xl p-1"
                >
                  {STATUS_FILTER_OPTIONS.map((opt) => (
                    <button
                      key={opt.value}
                      onClick={() => { setStatusFilter(opt.value); setShowFilterPanel(false); }}
                      className="flex w-full items-center justify-between rounded-lg px-3 py-2 text-xs text-white/70 hover:bg-glass-100 transition-colors"
                    >
                      {opt.label}
                      {statusFilter === opt.value && <Check size={11} className="text-cyan-neon" />}
                    </button>
                  ))}
                </motion.div>
              )}
            </AnimatePresence>
          </div>

          <NeonBadge variant="green" dot>Live</NeonBadge>
        </div>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-4 gap-3">
        {kpi.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Active filter banner */}
      {statusFilter !== "all" && (
        <motion.div
          variants={variants.fadeInUp}
          className="flex items-center gap-2 rounded-lg border border-cyan-neon/20 bg-cyan-neon/5 px-3 py-2 text-xs"
        >
          <Filter size={11} className="text-cyan-neon" />
          <span className="text-white/60">Filtering: <span className="text-cyan-neon font-mono">{STATUS_FILTER_OPTIONS.find((o) => o.value === statusFilter)?.label}</span></span>
          <span className="text-white/40 font-mono ml-1">· {filtered.length} driver{filtered.length !== 1 ? "s" : ""} shown</span>
          <button
            onClick={() => setStatusFilter("all")}
            className="ml-auto text-white/30 hover:text-white/70 transition-colors font-mono"
          >
            Clear ×
          </button>
        </motion.div>
      )}

      {/* Map + sidebar */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[1fr_280px]">
        {/* Map */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="none" className="overflow-hidden">
            <div className="h-[520px] w-full">
              <LiveDispatchMap
                drivers={pins}
                onDriverClick={handleDriverClick}
              />
            </div>
          </GlassCard>
        </motion.div>

        {/* Driver roster sidebar */}
        <motion.div variants={variants.fadeInUp} className="flex flex-col gap-2">
          <GlassCard size="sm">
            <p className="text-2xs font-mono text-white/40 uppercase tracking-wider mb-1">Active Drivers</p>
            <p className="text-lg font-bold font-heading text-white">{active.length} shown · {onlineCount} online</p>
          </GlassCard>

          <GlassCard padding="none" className="flex-1 overflow-y-auto max-h-[460px]">
            {filtered.length === 0 ? (
              <p className="p-4 text-xs text-white/40 font-mono">No drivers match filter.</p>
            ) : filtered.map((d) => {
              const isDelivering = d.status === "delivering" || d.status === "en_route";
              const isFocused    = focusId === d.driver_id || selectedDriverId === d.driver_id;
              return (
                <div
                  key={d.driver_id}
                  ref={(el) => { rowRefs.current[d.driver_id] = el; }}
                  onClick={() => setSelectedDriverId(isFocused ? null : d.driver_id)}
                  className={`flex flex-col gap-2 px-4 py-3 border-b border-glass-border/50 transition-colors cursor-pointer ${
                    isFocused ? "bg-cyan-neon/10 ring-1 ring-cyan-neon/40" : "hover:bg-glass-100"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <p className="text-xs font-semibold text-white truncate">{d.driver_name}</p>
                    <NeonBadge variant={isDelivering ? "green" : "cyan"} dot={isDelivering}>
                      {d.status.replace("_", " ")}
                    </NeonBadge>
                  </div>
                  <div className="flex items-center gap-1 text-2xs font-mono text-white/40">
                    <Navigation size={9} className="text-cyan-neon" />
                    {d.location_label}
                  </div>
                  <div className="flex items-center justify-between">
                    {d.deliveries_remaining > 0 ? (
                      <span className="text-2xs font-mono text-white/30">{d.deliveries_remaining} stops remaining</span>
                    ) : <span />}
                    <a
                      href={`/partner/drivers?focus=${encodeURIComponent(d.driver_id)}`}
                      title="Manage in Partner Portal"
                      onClick={(e) => e.stopPropagation()}
                      className="inline-flex items-center gap-1 text-2xs font-mono text-white/40 hover:text-purple-plasma transition-colors"
                    >
                      <ExternalLink size={9} /> Manage
                    </a>
                  </div>
                </div>
              );
            })}
          </GlassCard>
        </motion.div>
      </div>
    </motion.div>
  );
}

export default function LiveMapPage() {
  return (
    <Suspense fallback={null}>
      <LiveMapPageInner />
    </Suspense>
  );
}
