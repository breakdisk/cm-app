"use client";
/**
 * Admin Portal — Drivers Page
 * Live driver roster: online status, task load, GPS last-seen, performance grade.
 */
import { useState, useEffect, useCallback } from "react";
import { createDriversApi, Driver as ApiDriver, DriverSummary } from "@/lib/api/drivers";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { Search, MapPin, Package, Star, RefreshCw } from "lucide-react";

// ── Types & mock data ─────────────────────────────────────────────────────────

type DriverStatus = "online" | "idle" | "offline" | "on_break";

interface Driver {
  id: string;
  name: string;
  vehicle: string;
  plate: string;
  status: DriverStatus;
  tasks_total: number;
  tasks_done: number;
  last_location: string;
  last_seen: string;
  grade: "A" | "B" | "C" | "D";
  cod_collected: number;
}

const DRIVERS: Driver[] = [
  { id: "1",  name: "Juan Dela Cruz",     vehicle: "Motorcycle", plate: "ABC-1234", status: "online",   tasks_total: 18, tasks_done: 11, last_location: "Makati CBD",       last_seen: "Just now",  grade: "A", cod_collected: 8400  },
  { id: "2",  name: "Maria Santos",       vehicle: "Van",        plate: "XYZ-5678", status: "online",   tasks_total: 24, tasks_done: 16, last_location: "Taguig BGC",       last_seen: "1m ago",    grade: "A", cod_collected: 14200 },
  { id: "3",  name: "Pedro Gonzales",     vehicle: "Motorcycle", plate: "DEF-9012", status: "on_break", tasks_total: 15, tasks_done: 9,  last_location: "Pasig City",       last_seen: "8m ago",    grade: "B", cod_collected: 5100  },
  { id: "4",  name: "Ana Cruz",           vehicle: "Motorcycle", plate: "GHI-3456", status: "online",   tasks_total: 20, tasks_done: 14, last_location: "Quezon City",      last_seen: "Just now",  grade: "A", cod_collected: 9800  },
  { id: "5",  name: "Carlo Reyes",        vehicle: "Van",        plate: "JKL-7890", status: "online",   tasks_total: 22, tasks_done: 8,  last_location: "Mandaluyong",      last_seen: "2m ago",    grade: "B", cod_collected: 11600 },
  { id: "6",  name: "Luz Bautista",       vehicle: "Motorcycle", plate: "MNO-1234", status: "idle",     tasks_total: 12, tasks_done: 12, last_location: "Las Piñas",        last_seen: "5m ago",    grade: "A", cod_collected: 4200  },
  { id: "7",  name: "Dennis Villanueva",  vehicle: "Motorcycle", plate: "PQR-5678", status: "online",   tasks_total: 19, tasks_done: 13, last_location: "Caloocan City",    last_seen: "Just now",  grade: "B", cod_collected: 6300  },
  { id: "8",  name: "Rowena Ramos",       vehicle: "Van",        plate: "STU-9012", status: "online",   tasks_total: 26, tasks_done: 19, last_location: "Parañaque City",   last_seen: "3m ago",    grade: "A", cod_collected: 16800 },
  { id: "9",  name: "Eduardo Torres",     vehicle: "Motorcycle", plate: "VWX-3456", status: "offline",  tasks_total: 0,  tasks_done: 0,  last_location: "Depot — Caloocan", last_seen: "2h ago",    grade: "C", cod_collected: 0     },
  { id: "10", name: "Gloria Mendoza",     vehicle: "Motorcycle", plate: "YZA-7890", status: "online",   tasks_total: 16, tasks_done: 11, last_location: "Valenzuela",       last_seen: "Just now",  grade: "B", cod_collected: 7200  },
];

const STATUS_CONFIG: Record<DriverStatus, { label: string; variant: "green" | "cyan" | "amber" | "red"; dot: boolean }> = {
  online:   { label: "Online",    variant: "green", dot: true  },
  idle:     { label: "Idle",      variant: "cyan",  dot: false },
  on_break: { label: "On Break",  variant: "amber", dot: false },
  offline:  { label: "Offline",   variant: "red",   dot: false },
};

const GRADE_COLOR: Record<Driver["grade"], string> = {
  A: "text-green-signal",
  B: "text-cyan-neon",
  C: "text-amber-signal",
  D: "text-red-signal",
};

const KPI = [
  { label: "Online Drivers",  value: 7,   trend: 0,    color: "green"  as const, format: "number"  as const },
  { label: "Tasks Assigned",  value: 172, trend: +8.2, color: "cyan"   as const, format: "number"  as const },
  { label: "Tasks Complete",  value: 113, trend: +6.4, color: "purple" as const, format: "number"  as const },
  { label: "COD Collected",   value: 83600, trend: +11.2, color: "amber" as const, format: "currency" as const },
];

export default function DriversPage() {
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<DriverStatus | "all">("all");
  const [drivers, setDrivers] = useState<Driver[]>(DRIVERS);
  const [kpi, setKpi] = useState(KPI);
  const [loading, setLoading] = useState(false);

  const fetchDrivers = useCallback(async () => {
    setLoading(true);
    try {
      const api = createDriversApi();
      const [listRes, summaryRes] = await Promise.all([
        api.listDrivers({ per_page: 100 }),
        api.getSummary(),
      ]);
      // Map API shape to page Driver shape
      setDrivers(listRes.data.map((d: ApiDriver) => ({
        id:            d.id,
        name:          d.name,
        vehicle:       d.vehicle_type,
        plate:         d.vehicle_plate,
        status:        d.status as DriverStatus,
        tasks_total:   d.tasks_total,
        tasks_done:    d.tasks_done,
        last_location: d.last_location ?? "Unknown",
        last_seen:     d.last_seen_at ? new Date(d.last_seen_at).toLocaleTimeString() : "—",
        grade:         d.performance_grade,
        cod_collected: d.cod_collected,
      })));
      const s = summaryRes.data;
      setKpi([
        { label: "Online Drivers",  value: s.online,                  trend: 0,    color: "green"  as const, format: "number"   as const },
        { label: "Tasks Assigned",  value: s.total_tasks_assigned,    trend: 0,    color: "cyan"   as const, format: "number"   as const },
        { label: "Tasks Complete",  value: s.total_tasks_completed,   trend: 0,    color: "purple" as const, format: "number"   as const },
        { label: "COD Collected",   value: s.total_cod_collected,     trend: 0,    color: "amber"  as const, format: "currency" as const },
      ]);
    } catch {
      // retain mock data on error
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { fetchDrivers(); }, [fetchDrivers]);

  const filtered = drivers.filter((d) => {
    const matchStatus = statusFilter === "all" || d.status === statusFilter;
    const matchSearch = !search || d.name.toLowerCase().includes(search.toLowerCase()) || d.plate.toLowerCase().includes(search.toLowerCase());
    return matchStatus && matchSearch;
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
          <h1 className="font-heading text-2xl font-bold text-white">Drivers</h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">{drivers.filter(d => d.status === "online").length} online · {drivers.length} total roster</p>
        </div>
        <button
          onClick={fetchDrivers}
          disabled={loading}
          className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-50"
        >
          <RefreshCw size={12} className={loading ? "animate-spin" : ""} /> Refresh
        </button>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {kpi.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Filters */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-1.5">
              {(["all", "online", "idle", "on_break", "offline"] as const).map((s) => (
                <button
                  key={s}
                  onClick={() => setStatusFilter(s)}
                  className={`rounded-full px-3 py-1 text-xs font-medium capitalize transition-all ${
                    statusFilter === s
                      ? "bg-cyan-surface text-cyan-neon border border-cyan-neon/30"
                      : "text-white/40 border border-glass-border hover:text-white"
                  }`}
                >
                  {s === "all" ? "All" : s.replace("_", " ")}
                </button>
              ))}
            </div>
            <div className="ml-auto flex items-center gap-2 rounded-lg border border-glass-border bg-glass-100 px-3 py-2">
              <Search size={13} className="text-white/30" />
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Name or plate…"
                className="bg-transparent text-xs text-white placeholder:text-white/25 outline-none font-mono w-40"
              />
            </div>
          </div>
        </GlassCard>
      </motion.div>

      {/* Driver grid */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        {filtered.map((driver) => {
          const { label, variant, dot } = STATUS_CONFIG[driver.status];
          const progress = driver.tasks_total > 0 ? (driver.tasks_done / driver.tasks_total) * 100 : 0;
          return (
            <GlassCard key={driver.id} className="hover:border-glass-border-bright transition-colors cursor-pointer">
              <div className="flex items-start justify-between mb-3">
                <div className="flex items-center gap-3">
                  <div className="relative">
                    <div className="h-9 w-9 rounded-full bg-gradient-to-br from-cyan-neon/20 to-purple-plasma/20 flex items-center justify-center border border-glass-border">
                      <span className="text-sm font-bold text-white">{driver.name.split(" ").map(n => n[0]).join("").slice(0,2)}</span>
                    </div>
                    {driver.status === "online" && (
                      <span className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full bg-green-signal border-2 border-canvas" />
                    )}
                  </div>
                  <div>
                    <p className="text-sm font-semibold text-white">{driver.name}</p>
                    <p className="text-2xs font-mono text-white/40">{driver.vehicle} · {driver.plate}</p>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <span className={`text-lg font-bold font-heading ${GRADE_COLOR[driver.grade]}`}>{driver.grade}</span>
                  <NeonBadge variant={variant} dot={dot}>{label}</NeonBadge>
                </div>
              </div>

              {/* Task progress */}
              <div className="mb-3">
                <div className="flex items-center justify-between mb-1.5">
                  <div className="flex items-center gap-1 text-2xs font-mono text-white/40">
                    <Package size={10} /> {driver.tasks_done}/{driver.tasks_total} tasks
                  </div>
                  <span className="text-2xs font-mono text-white/40">{Math.round(progress)}%</span>
                </div>
                <div className="h-1.5 rounded-full bg-glass-300 overflow-hidden">
                  <div
                    className="h-full rounded-full transition-all"
                    style={{
                      width: `${progress}%`,
                      background: progress === 100 ? "#00FF88" : progress > 60 ? "#00E5FF" : "#A855F7",
                    }}
                  />
                </div>
              </div>

              {/* Location + COD */}
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-1 text-2xs font-mono text-white/40">
                  <MapPin size={10} className="text-cyan-neon" />
                  {driver.last_location} · {driver.last_seen}
                </div>
                {driver.cod_collected > 0 && (
                  <span className="text-xs font-mono text-amber-signal font-semibold">
                    ₱{driver.cod_collected.toLocaleString()}
                  </span>
                )}
              </div>
            </GlassCard>
          );
        })}
      </motion.div>
    </motion.div>
  );
}
