"use client";
/**
 * Admin Portal — Live Map Page
 * Real-time driver map with Mapbox dark theme, driver markers, delivery zones.
 */
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { LiveDispatchMap } from "@/components/maps/live-dispatch-map";
import { MapPin, Navigation, Filter, Layers } from "lucide-react";

// ── Mock data ──────────────────────────────────────────────────────────────────

const KPI = [
  { label: "Online Drivers", value: 47,    trend: +3,    color: "green"  as const, format: "number" as const },
  { label: "Delivering",     value: 38,    trend: 0,     color: "cyan"   as const, format: "number" as const },
  { label: "Idle",           value: 9,     trend: -2,    color: "amber"  as const, format: "number" as const },
  { label: "Active Stops",   value: 1284,  trend: +84,   color: "purple" as const, format: "number" as const },
];

const ACTIVE_DRIVERS = [
  { id: "D01", name: "Juan Dela Cruz",    tasks: 18, done: 11, location: "Makati CBD",    status: "delivering" as const },
  { id: "D02", name: "Maria Santos",      tasks: 24, done: 16, location: "BGC Taguig",    status: "delivering" as const },
  { id: "D03", name: "Pedro Gonzales",    tasks: 15, done: 9,  location: "Pasig City",    status: "idle"       as const },
  { id: "D04", name: "Ana Cruz",          tasks: 20, done: 14, location: "Quezon City",   status: "delivering" as const },
  { id: "D05", name: "Carlo Reyes",       tasks: 22, done: 8,  location: "Mandaluyong",   status: "delivering" as const },
  { id: "D06", name: "Luz Bautista",      tasks: 12, done: 12, location: "Las Piñas",     status: "idle"       as const },
  { id: "D07", name: "Dennis Villanueva", tasks: 19, done: 13, location: "Caloocan City", status: "delivering" as const },
];

export default function LiveMapPage() {
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
        {KPI.map((m) => (
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
              <LiveDispatchMap />
            </div>
          </GlassCard>
        </motion.div>

        {/* Driver roster sidebar */}
        <motion.div variants={variants.fadeInUp} className="flex flex-col gap-2">
          <GlassCard size="sm">
            <p className="text-2xs font-mono text-white/40 uppercase tracking-wider mb-1">Active Drivers</p>
            <p className="text-lg font-bold font-heading text-white">{ACTIVE_DRIVERS.length} shown</p>
          </GlassCard>

          <GlassCard padding="none" className="flex-1 overflow-y-auto max-h-[460px]">
            {ACTIVE_DRIVERS.map((d) => {
              const pct = Math.round((d.done / d.tasks) * 100);
              return (
                <div key={d.id} className="flex flex-col gap-2 px-4 py-3 border-b border-glass-border/50 hover:bg-glass-100 transition-colors cursor-pointer">
                  <div className="flex items-center justify-between">
                    <p className="text-xs font-semibold text-white truncate">{d.name}</p>
                    <NeonBadge variant={d.status === "delivering" ? "green" : "cyan"} dot={d.status === "delivering"}>
                      {d.status === "delivering" ? "Delivering" : "Idle"}
                    </NeonBadge>
                  </div>
                  <div className="flex items-center gap-1 text-2xs font-mono text-white/40">
                    <Navigation size={9} className="text-cyan-neon" />
                    {d.location}
                  </div>
                  <div>
                    <div className="flex items-center justify-between mb-1">
                      <span className="text-2xs font-mono text-white/30">{d.done}/{d.tasks} stops</span>
                      <span className="text-2xs font-mono text-white/30">{pct}%</span>
                    </div>
                    <div className="h-1 rounded-full bg-glass-300 overflow-hidden">
                      <div className="h-full rounded-full" style={{ width: `${pct}%`, background: pct === 100 ? "#00FF88" : "#00E5FF" }} />
                    </div>
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
