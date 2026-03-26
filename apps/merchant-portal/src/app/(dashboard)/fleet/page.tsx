"use client";
/**
 * Merchant Portal — Fleet Page
 * Merchant's own vehicle/rider roster for first-mile pickups (if self-fleet enabled).
 */
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { Truck, Plus, MapPin } from "lucide-react";

// ── Mock data ──────────────────────────────────────────────────────────────────

const KPI = [
  { label: "Own Riders",    value: 6,    trend: 0,    color: "green"  as const, format: "number"  as const },
  { label: "Active Today",  value: 4,    trend: 0,    color: "cyan"   as const, format: "number"  as const },
  { label: "Pickups MTD",   value: 284,  trend: +12.4, color: "purple" as const, format: "number"  as const },
  { label: "Pickup Cost",   value: 8520, trend: +8.2,  color: "amber"  as const, format: "currency" as const },
];

const RIDERS = [
  { id: "R01", name: "Ben Aquino",    type: "Motorcycle", status: "active"  as const, pickups_today: 8,  location: "QC Hub" },
  { id: "R02", name: "Tess Lim",      type: "Motorcycle", status: "active"  as const, pickups_today: 12, location: "Makati" },
  { id: "R03", name: "Ricky Santos",  type: "Van",        status: "active"  as const, pickups_today: 24, location: "Pasig"  },
  { id: "R04", name: "Donna Cruz",    type: "Motorcycle", status: "idle"    as const, pickups_today: 6,  location: "Depot"  },
  { id: "R05", name: "Felix Torres",  type: "Motorcycle", status: "offline" as const, pickups_today: 0,  location: "—"      },
  { id: "R06", name: "Nena Ramos",    type: "Motorcycle", status: "offline" as const, pickups_today: 0,  location: "—"      },
];

const STATUS_CONFIG = {
  active:  { label: "Active",  variant: "green" as const },
  idle:    { label: "Idle",    variant: "cyan"  as const },
  offline: { label: "Offline", variant: "red"   as const },
};

export default function FleetPage() {
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
            <Truck size={22} className="text-cyan-neon" />
            My Fleet
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">Self-fleet riders for first-mile pickup</p>
        </div>
        <button className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-cyan-neon to-purple-plasma px-4 py-2 text-xs font-semibold text-canvas">
          <Plus size={12} /> Add Rider
        </button>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {KPI.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Rider grid */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        {RIDERS.map((r) => {
          const { label, variant } = STATUS_CONFIG[r.status];
          return (
            <GlassCard key={r.id} className="hover:border-glass-border-bright transition-colors">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div className="h-9 w-9 rounded-full bg-gradient-to-br from-cyan-neon/20 to-purple-plasma/20 flex items-center justify-center border border-glass-border">
                    <span className="text-sm font-bold text-white">{r.name.split(" ").map(n => n[0]).join("")}</span>
                  </div>
                  <div>
                    <p className="text-sm font-semibold text-white">{r.name}</p>
                    <p className="text-2xs font-mono text-white/40">{r.type} · {r.id}</p>
                  </div>
                </div>
                <NeonBadge variant={variant} dot={r.status === "active"}>{label}</NeonBadge>
              </div>
              {r.status !== "offline" && (
                <div className="mt-3 flex items-center justify-between">
                  <div className="flex items-center gap-1 text-2xs font-mono text-white/40">
                    <MapPin size={10} className="text-cyan-neon" />{r.location}
                  </div>
                  <span className="text-xs font-mono text-white/60">{r.pickups_today} pickups today</span>
                </div>
              )}
            </GlassCard>
          );
        })}
      </motion.div>
    </motion.div>
  );
}
