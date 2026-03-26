"use client";
/**
 * Partner Portal — Manifests Page
 * Shipment manifests: daily pickup/delivery lists, POD status, bulk download.
 */
import { useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { FileText, Download, Search, ChevronDown, CheckCircle2, Clock, X } from "lucide-react";

// ── Types & data ───────────────────────────────────────────────────────────────

type ManifestStatus = "completed" | "in_progress" | "pending" | "failed";

interface ManifestRow {
  id: string;
  date: string;
  type: "pickup" | "delivery";
  driver: string;
  zone: string;
  total: number;
  delivered: number;
  failed: number;
  status: ManifestStatus;
}

const MANIFESTS: ManifestRow[] = [
  { id: "MAN-20260317-001", date: "Mar 17, 2026", type: "delivery", driver: "Juan Dela Cruz",    zone: "Makati City",       total: 18, delivered: 11, failed: 0, status: "in_progress" },
  { id: "MAN-20260317-002", date: "Mar 17, 2026", type: "delivery", driver: "Maria Santos",      zone: "BGC Taguig",        total: 24, delivered: 16, failed: 1, status: "in_progress" },
  { id: "MAN-20260317-003", date: "Mar 17, 2026", type: "pickup",   driver: "Ana Cruz",          zone: "Quezon City",       total: 12, delivered: 12, failed: 0, status: "completed"   },
  { id: "MAN-20260317-004", date: "Mar 17, 2026", type: "delivery", driver: "Carlo Reyes",       zone: "Mandaluyong",       total: 22, delivered: 8,  failed: 2, status: "in_progress" },
  { id: "MAN-20260316-001", date: "Mar 16, 2026", type: "delivery", driver: "Gloria Mendoza",    zone: "Valenzuela",        total: 16, delivered: 16, failed: 0, status: "completed"   },
  { id: "MAN-20260316-002", date: "Mar 16, 2026", type: "delivery", driver: "Dennis Villanueva", zone: "Caloocan City",     total: 19, delivered: 17, failed: 2, status: "completed"   },
  { id: "MAN-20260316-003", date: "Mar 16, 2026", type: "pickup",   driver: "Pedro Gonzales",    zone: "Pasig City",        total: 14, delivered: 14, failed: 0, status: "completed"   },
  { id: "MAN-20260315-001", date: "Mar 15, 2026", type: "delivery", driver: "Rowena Ramos",      zone: "Parañaque City",    total: 26, delivered: 24, failed: 2, status: "completed"   },
];

const STATUS_CONFIG: Record<ManifestStatus, { label: string; variant: "green" | "cyan" | "amber" | "red"; icon: React.ReactNode }> = {
  completed:   { label: "Complete",    variant: "green", icon: <CheckCircle2 size={10} /> },
  in_progress: { label: "In Progress", variant: "cyan",  icon: <Clock size={10} />        },
  pending:     { label: "Pending",     variant: "amber", icon: <Clock size={10} />        },
  failed:      { label: "Failed",      variant: "red",   icon: <X size={10} />            },
};

export default function ManifestsPage() {
  const [search, setSearch] = useState("");
  const [typeFilter, setTypeFilter] = useState<"all" | "pickup" | "delivery">("all");

  const filtered = MANIFESTS.filter((m) => {
    const matchType   = typeFilter === "all" || m.type === typeFilter;
    const matchSearch = !search || m.id.toLowerCase().includes(search.toLowerCase()) || m.driver.toLowerCase().includes(search.toLowerCase()) || m.zone.toLowerCase().includes(search.toLowerCase());
    return matchType && matchSearch;
  });

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
            <FileText size={20} className="text-cyan-neon" />
            Manifests
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">{MANIFESTS.length} manifests · March 2026</p>
        </div>
        <button className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors">
          <Download size={12} /> Export All
        </button>
      </motion.div>

      {/* Filters */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-1.5">
              {(["all", "pickup", "delivery"] as const).map((t) => (
                <button
                  key={t}
                  onClick={() => setTypeFilter(t)}
                  className={`rounded-full px-3 py-1 text-xs font-medium capitalize transition-all ${
                    typeFilter === t
                      ? "bg-cyan-surface text-cyan-neon border border-cyan-neon/30"
                      : "text-white/40 border border-glass-border hover:text-white"
                  }`}
                >
                  {t === "all" ? "All Types" : t}
                </button>
              ))}
            </div>
            <div className="ml-auto flex items-center gap-2 rounded-lg border border-glass-border bg-glass-100 px-3 py-2">
              <Search size={13} className="text-white/30" />
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Manifest ID, driver, zone…"
                className="bg-transparent text-xs text-white placeholder:text-white/25 outline-none font-mono w-48"
              />
            </div>
          </div>
        </GlassCard>
      </motion.div>

      {/* Manifests table */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="grid grid-cols-[2fr_80px_1fr_1fr_80px_80px_80px_100px_60px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Manifest ID", "Type", "Driver", "Zone", "Total", "Done", "Failed", "Status", ""].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {filtered.map((m) => {
            const { label, variant, icon } = STATUS_CONFIG[m.status];
            const pct = Math.round((m.delivered / m.total) * 100);
            return (
              <div key={m.id} className="grid grid-cols-[2fr_80px_1fr_1fr_80px_80px_80px_100px_60px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors">
                <div>
                  <p className="text-xs font-mono text-cyan-neon">{m.id}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">{m.date}</p>
                </div>
                <NeonBadge variant={m.type === "pickup" ? "purple" : "cyan"}>
                  {m.type}
                </NeonBadge>
                <span className="text-xs text-white truncate">{m.driver}</span>
                <span className="text-xs text-white/60 truncate">{m.zone}</span>
                <span className="text-xs font-mono text-white/60">{m.total}</span>
                <span className="text-xs font-mono text-green-signal font-bold">{m.delivered}</span>
                <span className={`text-xs font-mono font-bold ${m.failed > 0 ? "text-red-signal" : "text-white/30"}`}>{m.failed}</span>
                <NeonBadge variant={variant}>
                  <span className="flex items-center gap-1">{icon}{label}</span>
                </NeonBadge>
                <button className="text-white/30 hover:text-cyan-neon transition-colors">
                  <Download size={13} />
                </button>
              </div>
            );
          })}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
