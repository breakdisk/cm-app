"use client";
/**
 * Admin Portal — Live Map Page
 * Real-time driver map with Mapbox dark theme, driver markers, delivery zones.
 *
 * Initial roster comes from `/v1/drivers`; live updates stream from the
 * driver-ops RosterEvent WebSocket (tenant-filtered server-side).
 */
import { useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { LiveDispatchMap, type DriverPin } from "@/components/maps/live-dispatch-map";
import { createDriversApi } from "@/lib/api/drivers";
import { useRosterEvents } from "@/hooks/useRosterEvents";
import { MapPin, Navigation, Filter, Layers } from "lucide-react";

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
// Offline drivers are filtered out *before* this — we never call it for them.
function toPinStatus(s: BackendStatus): DriverPin["status"] {
  switch (s) {
    case "en_route":   return "en_route";
    case "delivering": return "delivering";
    case "returning":  return "returning";
    case "available":
    case "on_break":
    default:           return "idle";
  }
}

export default function LiveMapPage() {
  const [drivers, setDrivers] = useState<Record<string, LiveDriver>>({});

  // Initial fetch — populates the roster + gives us a baseline to patch.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const api = createDriversApi();
        const res = await api.listDrivers({ per_page: 100 });
        if (cancelled) return;
        const seed: Record<string, LiveDriver> = {};
        for (const d of res.data) {
          seed[d.id] = {
            driver_id:           d.id,
            driver_name:         d.name,
            lat:                 d.lat ?? 0,
            lng:                 d.lng ?? 0,
            status:              (d.status as BackendStatus) ?? "offline",
            deliveries_remaining: Math.max(0, d.tasks_total - d.tasks_done),
            location_label:      d.last_location ?? "—",
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
      // Unknown driver from WS — create a stub so we don't drop the event.
      // The roster refetch will fill in the name/deliveries next time.
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
      // status_changed
      return {
        ...prev,
        [event.driver_id]: { ...base, status: event.status },
      };
    });
  });

  const driverList = useMemo(() => Object.values(drivers), [drivers]);
  const active     = useMemo(() => driverList.filter((d) => d.status !== "offline"), [driverList]);
  const pins       = useMemo<DriverPin[]>(
    () => active.map((d) => ({
      driver_id:           d.driver_id,
      driver_name:         d.driver_name,
      lat:                 d.lat,
      lng:                 d.lng,
      status:              toPinStatus(d.status),
      deliveries_remaining: d.deliveries_remaining,
    })),
    [active],
  );

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
          <button className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors">
            <Layers size={12} /> Layers
          </button>
          <button className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors">
            <Filter size={12} /> Filter
          </button>
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

      {/* Map + sidebar */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[1fr_280px]">
        {/* Map */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="none" className="overflow-hidden">
            <div className="h-[520px] w-full">
              <LiveDispatchMap drivers={pins} />
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
            {active.length === 0 ? (
              <p className="p-4 text-xs text-white/40 font-mono">No drivers online.</p>
            ) : active.map((d) => {
              const isDelivering = d.status === "delivering" || d.status === "en_route";
              return (
                <div key={d.driver_id} className="flex flex-col gap-2 px-4 py-3 border-b border-glass-border/50 hover:bg-glass-100 transition-colors cursor-pointer">
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
                  {d.deliveries_remaining > 0 && (
                    <span className="text-2xs font-mono text-white/30">{d.deliveries_remaining} stops remaining</span>
                  )}
                </div>
              );
            })}
          </GlassCard>
        </motion.div>
      </div>
    </motion.div>
  );
}
