"use client";
/**
 * Admin Portal — Fleet Page
 * Vehicle roster, telemetry status, maintenance schedule.
 */
import { useState, useEffect, useRef } from "react";
import { useSearchParams } from "next/navigation";
import { motion } from "framer-motion";
import { createFleetApi, Vehicle as ApiVehicle } from "@/lib/api/fleet";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { Truck, Fuel, Wrench, MapPin, AlertTriangle, Briefcase } from "lucide-react";

// ── Mock data ──────────────────────────────────────────────────────────────────

const KPI = [
  { label: "Active Vehicles", value: 38,  trend: 0,    color: "green"  as const, format: "number" as const },
  { label: "In Maintenance",  value: 4,   trend: -1,   color: "amber"  as const, format: "number" as const },
  { label: "Avg Fuel Level",  value: 68,  trend: -4.2, color: "cyan"   as const, format: "percent" as const },
  { label: "KM Today",        value: 8420, trend: +6.8, color: "purple" as const, format: "number" as const },
];

type VehicleStatus = "active" | "idle" | "maintenance" | "offline";

interface Vehicle {
  id: string;
  plate: string;
  type: "Motorcycle" | "Van" | "Truck";
  driver?: string;
  driver_id?: string;
  status: VehicleStatus;
  fuel_pct: number;
  km_today: number;
  location: string;
  next_service_km: number;
  alerts: string[];
}

const VEHICLES: Vehicle[] = [
  { id: "V01", plate: "ABC-1234", type: "Motorcycle", driver: "Juan Dela Cruz",    status: "active",      fuel_pct: 72, km_today: 184, location: "Makati CBD",       next_service_km: 2400, alerts: []                          },
  { id: "V02", plate: "XYZ-5678", type: "Van",        driver: "Maria Santos",      status: "active",      fuel_pct: 48, km_today: 248, location: "BGC Taguig",       next_service_km: 800,  alerts: ["Low fuel warning"]        },
  { id: "V03", plate: "DEF-9012", type: "Motorcycle", driver: "Pedro Gonzales",    status: "idle",        fuel_pct: 91, km_today: 142, location: "Pasig City",       next_service_km: 4200, alerts: []                          },
  { id: "V04", plate: "GHI-3456", type: "Van",        driver: undefined,           status: "maintenance", fuel_pct: 100, km_today: 0,  location: "Caloocan Depot",   next_service_km: 0,    alerts: ["Scheduled PMS"]           },
  { id: "V05", plate: "JKL-7890", type: "Truck",      driver: "Carlo Reyes",       status: "active",      fuel_pct: 61, km_today: 312, location: "NLEX North",       next_service_km: 1100, alerts: []                          },
  { id: "V06", plate: "MNO-1234", type: "Motorcycle", driver: "Ana Cruz",          status: "active",      fuel_pct: 84, km_today: 167, location: "Quezon City",      next_service_km: 5800, alerts: []                          },
  { id: "V07", plate: "PQR-5678", type: "Van",        driver: undefined,           status: "maintenance", fuel_pct: 100, km_today: 0,  location: "Makati Depot",     next_service_km: 0,    alerts: ["Brake system check"]      },
  { id: "V08", plate: "STU-9012", type: "Motorcycle", driver: "Dennis Villanueva", status: "active",      fuel_pct: 55, km_today: 198, location: "Caloocan City",    next_service_km: 3200, alerts: []                          },
];

const STATUS_CONFIG: Record<VehicleStatus, { label: string; variant: "green" | "cyan" | "amber" | "red" }> = {
  active:      { label: "Active",      variant: "green" },
  idle:        { label: "Idle",        variant: "cyan"  },
  maintenance: { label: "Maintenance", variant: "amber" },
  offline:     { label: "Offline",     variant: "red"   },
};

const TYPE_ICON: Record<Vehicle["type"], React.ReactNode> = {
  Motorcycle: <Truck size={14} className="text-cyan-neon" />,
  Van:        <Truck size={14} className="text-purple-plasma" />,
  Truck:      <Truck size={14} className="text-amber-signal" />,
};

export default function FleetPage() {
  const searchParams = useSearchParams();
  // Deep-link from partner/drivers: /admin/fleet?driver=<user_id>. The fleet API
  // returns driver_id = identity user_id, so matching is symmetric with the partner side.
  const focusDriverId  = searchParams.get("driver");
  const focusCardRef   = useRef<HTMLDivElement | null>(null);

  const [vehicles, setVehicles] = useState<Vehicle[]>(VEHICLES);
  const [kpi, setKpi] = useState(KPI);

  useEffect(() => {
    if (focusDriverId && focusCardRef.current) {
      focusCardRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [focusDriverId, vehicles]);

  useEffect(() => {
    const api = createFleetApi();
    Promise.all([api.listVehicles({ per_page: 100 }), api.getSummary()])
      .then(([listRes, summaryRes]) => {
        setVehicles(listRes.data.map((v: ApiVehicle) => ({
          id:               v.id,
          plate:            v.plate,
          type:             v.type,
          driver:           v.driver_name,
          driver_id:        v.driver_id,
          status:           v.status as VehicleStatus,
          fuel_pct:         v.fuel_pct,
          km_today:         v.km_today,
          location:         v.location ?? "Unknown",
          next_service_km:  v.next_service_km,
          alerts:           v.alerts,
        })));
        const s = summaryRes.data;
        setKpi([
          { label: "Active Vehicles", value: s.active,         trend: 0,    color: "green"  as const, format: "number"  as const },
          { label: "In Maintenance",  value: s.maintenance,    trend: 0,    color: "amber"  as const, format: "number"  as const },
          { label: "Avg Fuel Level",  value: s.avg_fuel_pct,   trend: 0,    color: "cyan"   as const, format: "percent" as const },
          { label: "KM Today",        value: s.total_km_today, trend: 0,    color: "purple" as const, format: "number"  as const },
        ]);
      })
      .catch(() => { /* retain mock on error */ });
  }, []);

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
            Fleet Management
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">42 vehicles · 38 active · 4 in maintenance</p>
        </div>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {kpi.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Vehicle grid */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        {vehicles.map((v) => {
          const { label, variant } = STATUS_CONFIG[v.status];
          const fuelColor = v.fuel_pct < 25 ? "#FF3B5C" : v.fuel_pct < 50 ? "#FFAB00" : "#00FF88";
          const isFocused = focusDriverId != null && v.driver_id === focusDriverId;
          return (
            <div key={v.id} ref={isFocused ? focusCardRef : undefined}>
            <GlassCard className={`hover:border-glass-border-bright transition-colors cursor-pointer ${isFocused ? "ring-1 ring-cyan-neon/50" : ""}`}>
              <div className="flex items-start justify-between mb-3">
                <div className="flex items-center gap-3">
                  <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-glass-200 border border-glass-border">
                    {TYPE_ICON[v.type]}
                  </div>
                  <div>
                    <p className="text-sm font-bold font-mono text-white">{v.plate}</p>
                    <p className="text-2xs font-mono text-white/40">{v.type} · {v.id}</p>
                  </div>
                </div>
                <NeonBadge variant={variant} dot={v.status === "active"}>{label}</NeonBadge>
              </div>

              {v.driver && (
                <div className="flex items-center gap-2 mb-2">
                  <p className="text-xs text-white/60">Driver: <span className="text-white">{v.driver}</span></p>
                  {/* Cross-portal — driver profile (commission, zone, SLA) lives in partner-portal.
                      Plain <a> preserves the /partner basePath. */}
                  {v.driver_id && (
                    <a
                      href={`/partner/drivers?focus=${encodeURIComponent(v.driver_id)}`}
                      title="Open driver in Partner Portal"
                      onClick={(e) => e.stopPropagation()}
                      className="inline-flex items-center gap-1 rounded-md border border-glass-border bg-glass-100 px-1.5 py-0.5 text-2xs font-mono text-white/50 hover:border-purple-plasma/40 hover:text-purple-plasma transition-colors"
                    >
                      <Briefcase size={9} /> Partner
                    </a>
                  )}
                </div>
              )}

              {/* Fuel bar */}
              <div className="mb-3">
                <div className="flex items-center justify-between mb-1">
                  <div className="flex items-center gap-1 text-2xs font-mono text-white/40">
                    <Fuel size={10} /> Fuel
                  </div>
                  <span className="text-2xs font-mono" style={{ color: fuelColor }}>{v.fuel_pct}%</span>
                </div>
                <div className="h-1 rounded-full bg-glass-300 overflow-hidden">
                  <div className="h-full rounded-full" style={{ width: `${v.fuel_pct}%`, background: fuelColor }} />
                </div>
              </div>

              {/* Stats */}
              <div className="grid grid-cols-3 gap-2 mb-2">
                <div className="rounded bg-glass-100 px-2 py-1.5">
                  <p className="text-2xs font-mono text-white/30">KM Today</p>
                  <p className="text-xs font-bold text-white">{v.km_today}</p>
                </div>
                <div className="rounded bg-glass-100 px-2 py-1.5">
                  <p className="text-2xs font-mono text-white/30">Next PMS</p>
                  <p className={`text-xs font-bold ${v.next_service_km < 1000 ? "text-amber-signal" : "text-white"}`}>
                    {v.next_service_km > 0 ? `${v.next_service_km}km` : "Now"}
                  </p>
                </div>
                <div className="rounded bg-glass-100 px-2 py-1.5">
                  <p className="text-2xs font-mono text-white/30 flex items-center gap-1"><MapPin size={8} /> Location</p>
                  <p className="text-2xs font-mono text-cyan-neon truncate">{v.location}</p>
                </div>
              </div>

              {v.alerts.length > 0 && (
                <div className="flex items-center gap-1.5 rounded bg-amber-signal/10 border border-amber-signal/20 px-2 py-1.5">
                  <AlertTriangle size={11} className="text-amber-signal flex-shrink-0" />
                  <span className="text-2xs font-mono text-amber-signal">{v.alerts[0]}</span>
                </div>
              )}
            </GlassCard>
            </div>
          );
        })}
      </motion.div>
    </motion.div>
  );
}
