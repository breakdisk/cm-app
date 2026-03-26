"use client";
/**
 * Dispatch Console — Admin Portal
 * Real-time view of all active drivers, routes, and delivery KPIs.
 */
import { useMemo } from "react";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { LiveDispatchMap } from "@/components/maps/live-dispatch-map";
import { variants } from "@/lib/design-system/tokens";

// In production: fetched via TanStack Query + SSE for real-time updates
const MOCK_DRIVERS = [
  { driver_id: "d1", driver_name: "Juan Dela Cruz", lat: 14.5995, lng: 120.9842, status: "en_route"   as const, deliveries_remaining: 5 },
  { driver_id: "d2", driver_name: "Maria Santos",   lat: 14.6760, lng: 121.0437, status: "delivering" as const, deliveries_remaining: 2 },
  { driver_id: "d3", driver_name: "Pedro Reyes",    lat: 14.5547, lng: 121.0244, status: "idle"       as const, deliveries_remaining: 0 },
];

const KPI_METRICS = [
  { label: "Active Drivers",        value: 47,   trend: +12.0, color: "cyan"   as const, format: "number"  as const },
  { label: "Deliveries Today",      value: 1284, trend: +8.3,  color: "green"  as const, format: "number"  as const },
  { label: "Success Rate",          value: 94.7, trend: +1.2,  color: "green"  as const, format: "percent" as const },
  { label: "Avg Delivery Time",     value: 47,   trend: -5.1,  color: "purple" as const, format: "duration" as const },
  { label: "Failed Deliveries",     value: 23,   trend: -18.0, color: "red"    as const, format: "number"  as const },
  { label: "COD Collected",         value: 245800, trend: +22.4, color: "amber" as const, format: "currency" as const },
];

export default function DispatchPage() {
  const today = useMemo(
    () =>
      new Date().toLocaleDateString("en-PH", {
        month: "long",
        day: "numeric",
        year: "numeric",
      }),
    [],
  );

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
          <p className="text-sm text-white/40 font-mono">Metro Manila · {today}</p>
        </div>
        <NeonBadge variant="green" dot pulse>Live Operations</NeonBadge>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-6">
        {KPI_METRICS.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric
              label={m.label}
              value={m.value}
              trend={m.trend}
              color={m.color}
              format={m.format}
              live={m.label === "Active Drivers"}
            />
          </GlassCard>
        ))}
      </motion.div>

      {/* Map + driver list */}
      <div className="flex flex-col flex-1 gap-4 min-h-0 lg:flex-row">
        <motion.div variants={variants.fadeInUp} className="flex-1">
          <LiveDispatchMap
            drivers={MOCK_DRIVERS}
            className="h-full min-h-[320px] sm:min-h-[420px] lg:min-h-[500px]"
          />
        </motion.div>

        {/* Driver roster */}
        <motion.div variants={variants.fadeInUp} className="flex flex-col gap-2 lg:w-72">
          <span className="text-xs font-mono uppercase tracking-widest text-white/30">
            Drivers · {MOCK_DRIVERS.length} active
          </span>
          {MOCK_DRIVERS.map((driver) => (
            <GlassCard key={driver.driver_id} size="sm" className="cursor-pointer" glow="cyan">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm font-medium text-white">{driver.driver_name}</p>
                  <p className="text-2xs font-mono text-white/40">
                    {driver.deliveries_remaining} stops remaining
                  </p>
                </div>
                <NeonBadge
                  variant={
                    driver.status === "delivering" ? "green"
                    : driver.status === "en_route"  ? "cyan"
                    : driver.status === "idle"       ? "amber"
                    : "muted"
                  }
                  dot
                >
                  {driver.status.replace("_", " ")}
                </NeonBadge>
              </div>
            </GlassCard>
          ))}
        </motion.div>
      </div>
    </motion.div>
  );
}
