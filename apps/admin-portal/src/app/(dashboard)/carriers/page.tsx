"use client";
/**
 * Admin Portal — Carrier Ops Page
 * Third-party carrier management: performance, allocation, SLA contract status.
 */
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { GitBranch, Star, TrendingUp, ExternalLink, Plus } from "lucide-react";

// ── Mock data ──────────────────────────────────────────────────────────────────

const KPI = [
  { label: "Active Carriers",   value: 8,    trend: +1,    color: "cyan"   as const, format: "number"  as const },
  { label: "Shipments via 3PL", value: 2840, trend: +18.4, color: "purple" as const, format: "number"  as const },
  { label: "Avg 3PL SLA",       value: 91.2, trend: +2.1,  color: "green"  as const, format: "percent" as const },
  { label: "3PL Cost MTD",      value: 284000, trend: +12.4, color: "amber" as const, format: "currency" as const },
];

type CarrierStatus = "active" | "probation" | "suspended" | "pending";

interface Carrier {
  id: string;
  name: string;
  coverage: string[];
  status: CarrierStatus;
  sla_rate: number;
  sla_target: number;
  shipments_mtd: number;
  cost_per_shipment: number;
  integration: "API" | "Manual" | "EDI";
  grade: "A" | "B" | "C" | "D";
  ai_allocated: boolean;
}

const CARRIERS: Carrier[] = [
  { id: "C01", name: "FastLine Couriers",  coverage: ["Metro Manila", "Luzon A"],   status: "active",    sla_rate: 94.8, sla_target: 93, shipments_mtd: 8420, cost_per_shipment: 50,  integration: "API",    grade: "A", ai_allocated: true  },
  { id: "C02", name: "SpeedEx PH",         coverage: ["Luzon B", "Visayas"],        status: "active",    sla_rate: 89.2, sla_target: 87, shipments_mtd: 1840, cost_per_shipment: 68,  integration: "API",    grade: "B", ai_allocated: true  },
  { id: "C03", name: "IslandLink Express", coverage: ["Visayas", "Mindanao"],       status: "active",    sla_rate: 86.4, sla_target: 85, shipments_mtd: 920,  cost_per_shipment: 95,  integration: "Manual", grade: "B", ai_allocated: false },
  { id: "C04", name: "NorthlinkLogistics", coverage: ["Luzon B (North)"],           status: "active",    sla_rate: 91.8, sla_target: 88, shipments_mtd: 640,  cost_per_shipment: 72,  integration: "EDI",    grade: "A", ai_allocated: true  },
  { id: "C05", name: "SouthStar Delivery", coverage: ["Mindanao"],                  status: "probation", sla_rate: 78.4, sla_target: 85, shipments_mtd: 280,  cost_per_shipment: 82,  integration: "Manual", grade: "D", ai_allocated: false },
  { id: "C06", name: "MegaMover PH",       coverage: ["Metro Manila"],              status: "active",    sla_rate: 96.1, sla_target: 95, shipments_mtd: 1240, cost_per_shipment: 45,  integration: "API",    grade: "A", ai_allocated: true  },
  { id: "C07", name: "VisMinLog",          coverage: ["Visayas", "Mindanao"],       status: "active",    sla_rate: 88.2, sla_target: 85, shipments_mtd: 480,  cost_per_shipment: 88,  integration: "API",    grade: "B", ai_allocated: true  },
  { id: "C08", name: "QuickShip Cebu",     coverage: ["Cebu Metro"],               status: "pending",   sla_rate: 0,    sla_target: 90, shipments_mtd: 0,    cost_per_shipment: 62,  integration: "API",    grade: "A", ai_allocated: false },
];

const STATUS_CONFIG: Record<CarrierStatus, { label: string; variant: "green" | "cyan" | "amber" | "red" }> = {
  active:    { label: "Active",    variant: "green" },
  probation: { label: "Probation", variant: "red"   },
  suspended: { label: "Suspended", variant: "red"   },
  pending:   { label: "Pending",   variant: "amber" },
};

const GRADE_COLOR: Record<Carrier["grade"], string> = {
  A: "text-green-signal", B: "text-cyan-neon", C: "text-amber-signal", D: "text-red-signal",
};

export default function CarriersPage() {
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
            <GitBranch size={22} className="text-cyan-neon" />
            Carrier Ops
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">8 carriers · AI auto-allocation active</p>
        </div>
        <button className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-cyan-neon to-purple-plasma px-4 py-2 text-xs font-semibold text-canvas">
          <Plus size={12} /> Onboard Carrier
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

      {/* Carrier table */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="grid grid-cols-[2fr_1fr_60px_80px_80px_100px_60px_80px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Carrier", "Coverage", "Grade", "SLA", "Shipments", "Cost/Ship", "Int.", "Status"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {CARRIERS.map((c) => {
            const { label, variant } = STATUS_CONFIG[c.status];
            const slaOk = c.sla_rate >= c.sla_target;
            return (
              <div key={c.id} className="grid grid-cols-[2fr_1fr_60px_80px_80px_100px_60px_80px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors cursor-pointer">
                <div className="flex items-center gap-2">
                  <div>
                    <p className="text-xs font-semibold text-white">{c.name}</p>
                    <div className="flex items-center gap-1 mt-0.5">
                      {c.ai_allocated && (
                        <span className="text-2xs font-mono text-purple-plasma">AI</span>
                      )}
                      <span className="text-2xs font-mono text-white/30">{c.id}</span>
                    </div>
                  </div>
                </div>
                <div className="flex flex-wrap gap-0.5">
                  {c.coverage.slice(0,2).map((z) => (
                    <span key={z} className="text-2xs font-mono text-white/40 bg-glass-200 rounded px-1">{z}</span>
                  ))}
                  {c.coverage.length > 2 && <span className="text-2xs font-mono text-white/30">+{c.coverage.length - 2}</span>}
                </div>
                <span className={`text-lg font-bold font-heading ${GRADE_COLOR[c.grade]}`}>{c.grade}</span>
                <span className={`text-xs font-bold font-mono ${slaOk ? "text-green-signal" : "text-red-signal"}`}>
                  {c.sla_rate > 0 ? `${c.sla_rate}%` : "—"}
                </span>
                <span className="text-xs font-mono text-white/60">{c.shipments_mtd.toLocaleString()}</span>
                <span className="text-xs font-mono text-white/60">₱{c.cost_per_shipment}</span>
                <NeonBadge variant={c.integration === "API" ? "green" : c.integration === "EDI" ? "cyan" : "amber"}>
                  {c.integration}
                </NeonBadge>
                <NeonBadge variant={variant} dot={c.status === "active"}>{label}</NeonBadge>
              </div>
            );
          })}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
