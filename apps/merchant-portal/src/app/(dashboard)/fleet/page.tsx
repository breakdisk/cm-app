"use client";
/**
 * Merchant Portal — Fleet Page
 * Merchant's own vehicle/rider roster for first-mile pickups (if self-fleet enabled).
 * Fetches from driver-ops service; falls back to mock seed when API is unavailable.
 */
import { useState, useEffect, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { Truck, Plus, MapPin, X } from "lucide-react";
import { authFetch } from "@/lib/auth/auth-fetch";

// ── API ────────────────────────────────────────────────────────────────────────

const DRIVER_OPS_URL = process.env.NEXT_PUBLIC_DRIVER_OPS_URL ?? "http://localhost:8006";

type RiderStatus = "active" | "idle" | "offline";

interface Rider {
  id:           string;
  name:         string;
  type:         string;
  status:       RiderStatus;
  pickups_today: number;
  location:     string;
}

const RIDERS_SEED: Rider[] = [
  { id: "R01", name: "Ben Aquino",   type: "Motorcycle", status: "active",  pickups_today: 8,  location: "QC Hub" },
  { id: "R02", name: "Tess Lim",     type: "Motorcycle", status: "active",  pickups_today: 12, location: "Makati" },
  { id: "R03", name: "Ricky Santos", type: "Van",        status: "active",  pickups_today: 24, location: "Pasig"  },
  { id: "R04", name: "Donna Cruz",   type: "Motorcycle", status: "idle",    pickups_today: 6,  location: "Depot"  },
  { id: "R05", name: "Felix Torres", type: "Motorcycle", status: "offline", pickups_today: 0,  location: "—"      },
  { id: "R06", name: "Nena Ramos",   type: "Motorcycle", status: "offline", pickups_today: 0,  location: "—"      },
];

async function fetchRiders(): Promise<Rider[]> {
  try {
    const res = await authFetch(`${DRIVER_OPS_URL}/v1/drivers`);
    if (!res.ok) return RIDERS_SEED;
    const json = await res.json();
    const list = json.data ?? json.drivers ?? json ?? [];
    if (!Array.isArray(list) || list.length === 0) return RIDERS_SEED;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return list.map((d: any): Rider => ({
      id:            d.id,
      name:          d.name ?? d.full_name ?? "Unknown",
      type:          d.vehicle_type ?? d.vehicle ?? "Motorcycle",
      status:        (d.status === "on_route" ? "active" : d.status) ?? "offline",
      pickups_today: d.tasks_completed_today ?? d.pickups_today ?? 0,
      location:      d.current_location ?? d.location ?? "—",
    }));
  } catch {
    return RIDERS_SEED;
  }
}

const STATUS_CONFIG: Record<RiderStatus, { label: string; variant: "green" | "cyan" | "red" }> = {
  active:  { label: "Active",  variant: "green" },
  idle:    { label: "Idle",    variant: "cyan"  },
  offline: { label: "Offline", variant: "red"   },
};

// ── Add Rider Modal ────────────────────────────────────────────────────────────

function AddRiderModal({ onClose, onAdded }: { onClose: () => void; onAdded: (r: Rider) => void }) {
  const [name, setName]   = useState("");
  const [type, setType]   = useState("Motorcycle");
  const [saving, setSaving] = useState(false);
  const [err, setErr]     = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim()) { setErr("Name is required"); return; }
    setSaving(true);
    setErr(null);
    try {
      const res = await authFetch(`${DRIVER_OPS_URL}/v1/drivers`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: name.trim(), vehicle_type: type }),
      });
      if (res.ok) {
        const json = await res.json();
        const d = json.data ?? json;
        onAdded({
          id: d.id ?? `R-${Date.now()}`,
          name: d.name ?? name.trim(),
          type,
          status: "idle",
          pickups_today: 0,
          location: "Depot",
        });
      } else {
        // API not available yet — optimistic local add
        onAdded({ id: `R-${Date.now()}`, name: name.trim(), type, status: "idle", pickups_today: 0, location: "Depot" });
      }
      onClose();
    } catch {
      onAdded({ id: `R-${Date.now()}`, name: name.trim(), type, status: "idle", pickups_today: 0, location: "Depot" });
      onClose();
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
        className="absolute inset-0 bg-black/70 backdrop-blur-sm" onClick={onClose} />
      <motion.div initial={{ opacity: 0, scale: 0.95, y: 20 }} animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.95, y: 20 }} transition={{ duration: 0.22 }}
        className="relative z-10 w-full max-w-sm rounded-2xl border border-glass-border bg-canvas-100 p-6 shadow-glass"
      >
        <div className="mb-4 flex items-center justify-between">
          <p className="font-heading text-sm font-semibold text-white">Add Rider</p>
          <button onClick={onClose} className="text-white/40 hover:text-white transition-colors"><X size={16} /></button>
        </div>
        <form onSubmit={handleSubmit} className="space-y-3">
          <div>
            <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">Full Name</label>
            <input value={name} onChange={e => setName(e.target.value)} placeholder="e.g. Juan Dela Cruz"
              className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2.5 text-sm text-white placeholder-white/20 focus:outline-none focus:border-cyan-neon/40 transition-all" />
          </div>
          <div>
            <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">Vehicle Type</label>
            <select value={type} onChange={e => setType(e.target.value)}
              className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2.5 text-sm text-white focus:outline-none focus:border-cyan-neon/40 transition-all">
              <option value="Motorcycle">Motorcycle</option>
              <option value="Van">Van</option>
              <option value="Sedan">Sedan</option>
            </select>
          </div>
          {err && <p className="text-xs text-red-signal font-mono">{err}</p>}
          <button type="submit" disabled={saving}
            className="w-full rounded-lg bg-gradient-to-r from-cyan-neon to-purple-plasma py-2.5 text-sm font-semibold text-canvas disabled:opacity-50 transition-opacity hover:opacity-90">
            {saving ? "Adding…" : "Add Rider"}
          </button>
        </form>
      </motion.div>
    </div>
  );
}

// ── Page ───────────────────────────────────────────────────────────────────────

export default function FleetPage() {
  const [riders, setRiders] = useState<Rider[]>(RIDERS_SEED);
  const [showAdd, setShowAdd] = useState(false);

  const load = useCallback(async () => {
    const data = await fetchRiders();
    setRiders(data);
  }, []);

  useEffect(() => { load(); }, [load]);

  const activeCount   = riders.filter(r => r.status === "active").length;
  const pickupsMtd    = riders.reduce((s, r) => s + r.pickups_today, 0);

  const KPI = [
    { label: "Own Riders",   value: riders.length, trend: 0,     color: "green"  as const, format: "number"   as const },
    { label: "Active Today", value: activeCount,   trend: 0,     color: "cyan"   as const, format: "number"   as const },
    { label: "Pickups Today",value: pickupsMtd,    trend: +12.4, color: "purple" as const, format: "number"   as const },
    { label: "Pickup Cost",  value: pickupsMtd * 30, trend: +8.2, color: "amber" as const, format: "currency" as const },
  ];

  return (
    <>
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
          <button
            onClick={() => setShowAdd(true)}
            className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-cyan-neon to-purple-plasma px-4 py-2 text-xs font-semibold text-canvas"
          >
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
          {riders.map((r) => {
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

      <AnimatePresence>
        {showAdd && (
          <AddRiderModal
            onClose={() => setShowAdd(false)}
            onAdded={(r) => setRiders(prev => [...prev, r])}
          />
        )}
      </AnimatePresence>
    </>
  );
}
