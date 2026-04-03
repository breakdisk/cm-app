"use client";
/**
 * Partner Portal — Settings
 * Carrier profile, coverage zones, service capabilities, contact info.
 */
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { variants } from "@/lib/design-system/tokens";

const COVERAGE_ZONES = [
  { zone: "NCR",          regions: ["Manila", "Quezon City", "Makati", "Pasig", "Taguig"],    d1: true,  d2: true,  cod: true  },
  { zone: "Region III",   regions: ["Bulacan", "Pampanga", "Bataan", "Tarlac"],               d1: false, d2: true,  cod: true  },
  { zone: "Region IV-A",  regions: ["Cavite", "Laguna", "Batangas", "Rizal", "Quezon"],       d1: false, d2: true,  cod: true  },
  { zone: "Region VII",   regions: ["Cebu", "Bohol", "Negros Oriental", "Siquijor"],          d1: false, d2: true,  cod: false },
  { zone: "Region XI",    regions: ["Davao del Norte", "Davao del Sur", "Davao City"],        d1: false, d2: true,  cod: false },
  { zone: "Region X",     regions: ["Cagayan de Oro", "Misamis Oriental", "Bukidnon"],        d1: false, d2: false, cod: false },
];

export default function PartnerSettingsPage() {
  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="p-6 space-y-6"
    >
      <motion.div variants={variants.fadeInUp}>
        <h1 className="text-2xl font-bold text-white font-space-grotesk">Settings</h1>
        <p className="text-white/40 text-sm mt-1">Carrier profile, coverage zones, and service capabilities</p>
      </motion.div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Carrier Profile */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard title="Carrier Profile">
            <div className="space-y-4">
              {[
                { label: "Carrier Name",   value: "FastFreight Express"             },
                { label: "Carrier ID",     value: "carr-f1e2d3c4", mono: true       },
                { label: "Contact",        value: "Maria Santos"                     },
                { label: "Email",          value: "ops@fastfreight.ph"               },
                { label: "Phone",          value: "+63 2 8123 4567"                  },
                { label: "Integration",    value: "API",         badge: "cyan"       },
                { label: "Grade",          value: "A",           badge: "green"      },
                { label: "Status",         value: "Active",      badge: "green"      },
              ].map((row) => (
                <div key={row.label} className="flex justify-between items-center py-2 border-b border-white/[0.06]">
                  <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{row.label}</span>
                  {row.badge ? (
                    <NeonBadge variant={row.badge as any}>{row.value}</NeonBadge>
                  ) : (
                    <span className={`text-sm font-medium ${row.mono ? "font-mono text-[#00E5FF]" : "text-white"}`}>{row.value}</span>
                  )}
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>

        {/* SLA Commitments */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard title="SLA Commitments">
            <div className="space-y-3">
              {[
                { zone: "NCR Metro",       commitment: "D+1 (next-day)",  target: "97%", current: "98.2%", ok: true  },
                { zone: "Luzon Provincial",commitment: "D+2",              target: "95%", current: "94.1%", ok: false },
                { zone: "Visayas",         commitment: "D+2",              target: "95%", current: "96.7%", ok: true  },
                { zone: "Mindanao",        commitment: "D+3",              target: "90%", current: "91.3%", ok: true  },
              ].map((s) => (
                <div key={s.zone} className="flex items-center justify-between p-3 bg-white/[0.03] border border-white/[0.06] rounded-lg">
                  <div>
                    <p className="text-sm text-white font-medium">{s.zone}</p>
                    <p className="text-xs text-white/40">{s.commitment} — Target: {s.target}</p>
                  </div>
                  <NeonBadge variant={s.ok ? "green" : "amber"}>{s.current}</NeonBadge>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>

        {/* Coverage Zones */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-2">
          <GlassCard title="Coverage Zones & Capabilities" padding="none">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-white/[0.08]">
                  {["Zone", "Covered Provinces / Cities", "D+1", "D+2", "COD"].map((h) => (
                    <th key={h} className="text-left px-4 py-3 text-xs text-white/30 uppercase tracking-widest font-mono">{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {COVERAGE_ZONES.map((z) => (
                  <tr key={z.zone} className="border-b border-white/[0.04] hover:bg-white/[0.02]">
                    <td className="px-4 py-3 font-mono text-[#00E5FF] text-xs font-semibold">{z.zone}</td>
                    <td className="px-4 py-3 text-white/50 text-xs">{z.regions.join(", ")}</td>
                    <td className="px-4 py-3 text-center">
                      <span className={`text-sm ${z.d1 ? "text-[#00FF88]" : "text-white/20"}`}>{z.d1 ? "✓" : "—"}</span>
                    </td>
                    <td className="px-4 py-3 text-center">
                      <span className={`text-sm ${z.d2 ? "text-[#00FF88]" : "text-white/20"}`}>{z.d2 ? "✓" : "—"}</span>
                    </td>
                    <td className="px-4 py-3 text-center">
                      <span className={`text-sm ${z.cod ? "text-[#FFAB00]" : "text-white/20"}`}>{z.cod ? "✓" : "—"}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </GlassCard>
        </motion.div>
      </div>
    </motion.div>
  );
}
