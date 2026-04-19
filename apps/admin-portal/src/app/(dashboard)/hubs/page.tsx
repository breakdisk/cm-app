"use client";
/**
 * Admin Portal — Hub Operations Page
 * Live capacity, induction queue, sorting progress, dock scheduling.
 */
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from "recharts";
import { Building2, Package, Truck, Clock, AlertTriangle, FileText } from "lucide-react";

// ── Mock data ──────────────────────────────────────────────────────────────────────

const KPI = [
  { label: "Parcels In Hub",    value: 4284,  trend: +12.4, color: "cyan"   as const, format: "number"  as const },
  { label: "Sorted Today",      value: 8420,  trend: +8.2,  color: "green"  as const, format: "number"  as const },
  { label: "Pending Induction", value: 312,   trend: -18.4, color: "amber"  as const, format: "number"  as const },
  { label: "Dock Utilization",  value: 78,    trend: +4.1,  color: "purple" as const, format: "percent" as const },
];

const HUBS = [
  { id: "H01", name: "Caloocan Main Hub",   capacity: 5000, current: 3840, zones: 8,  active_docks: 4, total_docks: 6, status: "normal"  as const },
  { id: "H02", name: "Makati CBD Hub",       capacity: 2000, current: 1820, zones: 4,  active_docks: 3, total_docks: 4, status: "high"    as const },
  { id: "H03", name: "Pasig East Hub",       capacity: 3000, current: 1240, zones: 6,  active_docks: 2, total_docks: 4, status: "normal"  as const },
  { id: "H04", name: "Las Piñas South Hub",  capacity: 2500, current: 980,  zones: 5,  active_docks: 2, total_docks: 3, status: "normal"  as const },
  { id: "H05", name: "Quezon City North Hub",capacity: 4000, current: 3960, zones: 8,  active_docks: 5, total_docks: 6, status: "critical" as const },
];

type HubStatus = "normal" | "high" | "critical";

const STATUS_CONFIG: Record<HubStatus, { label: string; variant: "green" | "amber" | "red"; color: string }> = {
  normal:   { label: "Normal",   variant: "green", color: "#00FF88" },
  high:     { label: "High",     variant: "amber", color: "#FFAB00" },
  critical: { label: "Critical", variant: "red",   color: "#FF3B5C" },
};

const HOURLY_THROUGHPUT = [
  { hour: "6AM",  sorted: 280, inducted: 320 },
  { hour: "7AM",  sorted: 420, inducted: 480 },
  { hour: "8AM",  sorted: 680, inducted: 720 },
  { hour: "9AM",  sorted: 840, inducted: 880 },
  { hour: "10AM", sorted: 920, inducted: 960 },
  { hour: "11AM", sorted: 780, inducted: 820 },
  { hour: "12PM", sorted: 540, inducted: 580 },
  { hour: "1PM",  sorted: 640, inducted: 680 },
  { hour: "2PM",  sorted: 720, inducted: 740 },
];

const DOCK_SCHEDULE = [
  { dock: "Dock 1", vehicle: "VAN-XYZ-5678", driver: "Maria Santos",      eta: "2:15 PM", type: "Outbound", parcels: 24 },
  { dock: "Dock 2", vehicle: "TRK-JKL-7890", driver: "Carlo Reyes",       eta: "2:30 PM", type: "Inbound",  parcels: 87 },
  { dock: "Dock 3", vehicle: "VAN-STU-9012", driver: "Rowena Ramos",      eta: "2:45 PM", type: "Outbound", parcels: 26 },
  { dock: "Dock 4", vehicle: "TRK-PQR-5678", driver: "3PL — SpeedEx",     eta: "3:00 PM", type: "Inbound",  parcels: 142 },
  { dock: "Dock 5", vehicle: "VAN-ABC-1234", driver: "Juan Dela Cruz",    eta: "3:15 PM", type: "Outbound", parcels: 18 },
  { dock: "Dock 6", vehicle: "—",            driver: "Available",          eta: "—",       type: "—",        parcels: 0  },
];

export default function HubOpsPage() {
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
            <Building2 size={22} className="text-purple-plasma" />
            Hub Operations
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">5 active hubs · Live induction & sorting status</p>
        </div>
        <NeonBadge variant="green" dot>Live</NeonBadge>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {KPI.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Hub capacity grid */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 lg:grid-cols-2 xl:grid-cols-3">
        {HUBS.map((hub) => {
          const { label, variant, color } = STATUS_CONFIG[hub.status];
          const capacityPct = Math.round((hub.current / hub.capacity) * 100);
          return (
            <GlassCard key={hub.id} glow={hub.status === "critical" ? "red" : hub.status === "high" ? "amber" : undefined} className="cursor-pointer hover:border-glass-border-bright transition-all">
              <div className="flex items-start justify-between mb-3">
                <div>
                  <p className="text-sm font-semibold text-white">{hub.name}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">{hub.id} · {hub.zones} sort zones</p>
                </div>
                <NeonBadge variant={variant} dot={hub.status !== "normal"}>{label}</NeonBadge>
              </div>

              {/* Capacity bar */}
              <div className="mb-3">
                <div className="flex items-center justify-between mb-1.5">
                  <div className="flex items-center gap-1 text-2xs font-mono text-white/40">
                    <Package size={10} /> {hub.current.toLocaleString()} / {hub.capacity.toLocaleString()} parcels
                  </div>
                  <span className="text-2xs font-mono font-bold" style={{ color }}>{capacityPct}%</span>
                </div>
                <div className="h-2 rounded-full bg-glass-300 overflow-hidden">
                  <div className="h-full rounded-full transition-all" style={{ width: `${capacityPct}%`, background: color }} />
                </div>
              </div>

              {/* Docks */}
              <div className="flex items-center gap-2">
                <Truck size={11} className="text-white/40" />
                <span className="text-2xs font-mono text-white/40">{hub.active_docks}/{hub.total_docks} docks active</span>
                {hub.status === "critical" && (
                  <div className="flex items-center gap-1 text-2xs font-mono text-red-signal">
                    <AlertTriangle size={10} /> Near capacity
                  </div>
                )}
                {/* Cross-portal — manifests (daily pickup/delivery lists) live in partner-portal.
                    Plain <a> preserves the /partner basePath. */}
                <a
                  href={`/partner/manifests?hub=${encodeURIComponent(hub.id)}`}
                  title="Open hub manifests in Partner Portal"
                  onClick={(e) => e.stopPropagation()}
                  className="ml-auto inline-flex items-center gap-1 rounded-md border border-glass-border bg-glass-100 px-1.5 py-0.5 text-2xs font-mono text-white/50 hover:border-cyan-neon/40 hover:text-cyan-neon transition-colors"
                >
                  <FileText size={9} /> Manifests
                </a>
              </div>
            </GlassCard>
          );
        })}
      </motion.div>

      {/* Hourly throughput chart */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="purple">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Sorting Throughput — Today</h2>
              <p className="text-2xs font-mono text-white/30">Inducted vs Sorted per hour (Caloocan Main Hub)</p>
            </div>
          </div>
          <ResponsiveContainer width="100%" height={180}>
            <BarChart data={HOURLY_THROUGHPUT} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="hour" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                labelStyle={{ color: "rgba(255,255,255,0.4)" }}
              />
              <Bar dataKey="inducted" fill="#A855F7" radius={[4,4,0,0]} fillOpacity={0.7} />
              <Bar dataKey="sorted"   fill="#00E5FF" radius={[4,4,0,0]} fillOpacity={0.8} />
            </BarChart>
          </ResponsiveContainer>
        </GlassCard>
      </motion.div>

      {/* Dock schedule */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white">Dock Schedule — Caloocan Main Hub</h2>
            <NeonBadge variant="cyan">Next 2h</NeonBadge>
          </div>

          <div className="grid grid-cols-[80px_1fr_1fr_80px_80px_80px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Dock", "Vehicle", "Driver/Carrier", "ETA", "Type", "Parcels"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {DOCK_SCHEDULE.map((d, i) => (
            <div key={i} className={`grid grid-cols-[80px_1fr_1fr_80px_80px_80px] gap-3 items-center px-5 py-3 border-b border-glass-border/50 ${d.parcels === 0 ? "opacity-40" : "hover:bg-glass-100"} transition-colors`}>
              <span className="text-xs font-mono font-bold text-white">{d.dock}</span>
              <span className="text-xs font-mono text-cyan-neon truncate">{d.vehicle}</span>
              <span className="text-xs text-white/60 truncate">{d.driver}</span>
              <div className="flex items-center gap-1 text-xs font-mono text-white/60">
                <Clock size={10} className="text-amber-signal" />{d.eta}
              </div>
              {d.type !== "—"
                ? <NeonBadge variant={d.type === "Inbound" ? "purple" : "cyan"}>{d.type}</NeonBadge>
                : <span className="text-white/20 text-xs font-mono">—</span>
              }
              <span className={`text-xs font-mono font-bold ${d.parcels > 0 ? "text-white" : "text-white/20"}`}>
                {d.parcels > 0 ? d.parcels : "—"}
              </span>
            </div>
          ))}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
