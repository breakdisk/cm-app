"use client";
/**
 * Partner Portal — Rate Cards
 * Active rate schedule, zone-based pricing, weight brackets, fuel surcharge.
 */
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { GitBranch, Calendar, Download, ChevronDown, BarChart3 } from "lucide-react";

// ── Mock data ──────────────────────────────────────────────────────────────────

const RATE_CARD = {
  version: "v4.2",
  effective: "January 1, 2026",
  fuel_surcharge: 2.50,
  cod_pct: 1.5,
};

const ZONE_RATES = [
  {
    zone: "Metro Manila",
    description: "NCR + immediate surrounding areas",
    sla: "D+1",
    base: 15.00,
    per_kg_over_1: 4.00,
    max_kg: 30,
    cod_eligible: true,
  },
  {
    zone: "Luzon A",
    description: "Bulacan, Cavite, Laguna, Rizal, Pampanga",
    sla: "D+2",
    base: 20.00,
    per_kg_over_1: 5.00,
    max_kg: 30,
    cod_eligible: true,
  },
  {
    zone: "Luzon B",
    description: "All other Luzon provinces",
    sla: "D+3",
    base: 26.00,
    per_kg_over_1: 6.00,
    max_kg: 20,
    cod_eligible: true,
  },
  {
    zone: "Visayas",
    description: "Cebu, Iloilo, Bacolod, Eastern Visayas",
    sla: "D+3",
    base: 38.00,
    per_kg_over_1: 8.00,
    max_kg: 15,
    cod_eligible: false,
  },
  {
    zone: "Mindanao",
    description: "Davao, Cagayan de Oro, General Santos + surrounding",
    sla: "D+4",
    base: 45.00,
    per_kg_over_1: 10.00,
    max_kg: 15,
    cod_eligible: false,
  },
];

const WEIGHT_BRACKETS = [
  { label: "0 – 1 kg",   note: "Base rate applies" },
  { label: "1 – 3 kg",   note: "+₱4–10/kg depending on zone" },
  { label: "3 – 5 kg",   note: "+₱4–10/kg"          },
  { label: "5 – 10 kg",  note: "+₱4–10/kg + volumetric check" },
  { label: "10 – 20 kg", note: "Heavy item surcharge: ₱50" },
  { label: "20 – 30 kg", note: "Heavy item surcharge: ₱120" },
  { label: "> 30 kg",    note: "Freight pricing — contact ops" },
];

const SPECIAL_SERVICES = [
  { service: "Same-Day Delivery",       price: "+₱80",    availability: "Metro Manila only" },
  { service: "Saturday Delivery",       price: "+₱25",    availability: "Luzon zones" },
  { service: "Sunday / Holiday",        price: "+₱50",    availability: "Metro Manila only" },
  { service: "Fragile Handling",        price: "+₱30",    availability: "All zones" },
  { service: "Temperature-Controlled",  price: "+₱120",   availability: "Metro Manila only" },
  { service: "POD — Digital Signature", price: "Included", availability: "All zones" },
  { service: "POD — Photo Capture",     price: "Included", availability: "All zones" },
];

export default function RateCardsPage() {
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
            <GitBranch size={20} className="text-cyan-neon" />
            Rate Cards
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            Rate Card {RATE_CARD.version} · Effective {RATE_CARD.effective}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <NeonBadge variant="green" dot>Active</NeonBadge>
          <button className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors">
            <Download size={12} /> Download PDF
          </button>
        </div>
      </motion.div>

      {/* Summary chips */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {[
          { label: "Rate Version",    value: RATE_CARD.version,          color: "text-cyan-neon"    },
          { label: "Effective Date",  value: RATE_CARD.effective,        color: "text-white"        },
          { label: "Fuel Surcharge",  value: `₱${RATE_CARD.fuel_surcharge}/shipment`, color: "text-amber-signal" },
          { label: "COD Fee",         value: `${RATE_CARD.cod_pct}% of COD`,           color: "text-purple-plasma" },
        ].map((s) => (
          <GlassCard key={s.label} size="sm">
            <p className="text-2xs font-mono text-white/30 uppercase tracking-wider">{s.label}</p>
            <p className={`text-sm font-bold font-mono mt-1 ${s.color}`}>{s.value}</p>
          </GlassCard>
        ))}
      </motion.div>

      {/* Zone rate table */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white">Zone-Based Pricing</h2>
            <p className="text-2xs font-mono text-white/30 mt-0.5">Base rate includes 1 kg. Additional weight per kg extra.</p>
          </div>

          <div className="grid grid-cols-[2fr_60px_80px_100px_80px_80px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Zone", "SLA", "Base Rate", "Add'l /kg", "Max KG", "COD"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {ZONE_RATES.map((z) => (
            <div key={z.zone} className="grid grid-cols-[2fr_60px_80px_100px_80px_80px] gap-3 items-center px-5 py-4 border-b border-glass-border/50 hover:bg-glass-100 transition-colors">
              <div className="flex items-start gap-2">
                <div>
                  <p className="text-sm font-semibold text-white">{z.zone}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">{z.description}</p>
                </div>
                {/* Cross-portal — zone delivery performance lives in admin/analytics.
                    Plain <a> keeps the /admin basePath after the jump. */}
                <a
                  href={`/admin/analytics?zone=${encodeURIComponent(z.zone)}`}
                  title="View zone performance in Admin Analytics"
                  className="inline-flex h-6 w-6 items-center justify-center rounded-md border border-glass-border text-white/40 hover:text-cyan-neon hover:border-cyan-neon/30 transition-colors"
                >
                  <BarChart3 size={11} />
                </a>
              </div>
              <NeonBadge variant={z.sla === "D+1" ? "green" : z.sla === "D+2" ? "cyan" : "amber"}>{z.sla}</NeonBadge>
              <span className="text-sm font-bold font-mono text-green-signal">₱{z.base.toFixed(2)}</span>
              <span className="text-xs font-mono text-white/60">₱{z.per_kg_over_1.toFixed(2)} / kg</span>
              <span className="text-xs font-mono text-white/60">{z.max_kg} kg</span>
              {z.cod_eligible
                ? <NeonBadge variant="green">Yes</NeonBadge>
                : <NeonBadge variant="red">No</NeonBadge>
              }
            </div>
          ))}
        </GlassCard>
      </motion.div>

      {/* Weight brackets + special services */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="h-full">
            <h2 className="font-heading text-sm font-semibold text-white mb-4">Weight Brackets</h2>
            <div className="flex flex-col gap-2">
              {WEIGHT_BRACKETS.map((w) => (
                <div key={w.label} className="flex items-center justify-between rounded-lg bg-glass-100 px-3 py-2.5">
                  <span className="text-xs font-mono text-white">{w.label}</span>
                  <span className="text-2xs font-mono text-white/40">{w.note}</span>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>

        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="h-full">
            <h2 className="font-heading text-sm font-semibold text-white mb-4">Special Services</h2>
            <div className="flex flex-col gap-2">
              {SPECIAL_SERVICES.map((s) => (
                <div key={s.service} className="flex items-center justify-between rounded-lg bg-glass-100 px-3 py-2.5">
                  <div>
                    <p className="text-xs text-white">{s.service}</p>
                    <p className="text-2xs font-mono text-white/30 mt-0.5">{s.availability}</p>
                  </div>
                  <span className={`text-xs font-bold font-mono ${s.price === "Included" ? "text-green-signal" : "text-amber-signal"}`}>
                    {s.price}
                  </span>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>
      </div>
    </motion.div>
  );
}
