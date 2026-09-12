"use client";
/**
 * Admin Portal — Drivers Page
 * Live driver roster: online status, task load, GPS last-seen, performance grade.
 *
 * Live updates arrive via the driver-ops RosterEvent WebSocket — status toggles
 * and GPS fixes patch the roster in place without a refetch.
 */
import { useState, useEffect, useCallback } from "react";
import { createDriversApi, Driver as ApiDriver } from "@/lib/api/drivers";
import { fetchProfiles, type ComplianceProfile } from "@/lib/api/compliance";
import { useDriverRoster } from "@/context/driver-roster-context";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { OnboardDriverModal } from "@/components/drivers/OnboardDriverModal";
import { Search, MapPin, Package, RefreshCw, Briefcase, UserPlus, Trash2, ShieldCheck, ShieldAlert, ShieldX, AlertCircle, Zap } from "lucide-react";
import { usePermissions } from "@/hooks/usePermissions";

// ── Types & mock data ─────────────────────────────────────────────────────────

// Matches driver-ops backend status taxonomy.
type DriverStatus =
  | "offline"
  | "available"
  | "en_route"
  | "delivering"
  | "returning"
  | "on_break";

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
  driver_type: string;
  per_delivery_rate_cents: number;
}

// Empty initial state. Previously this file shipped a 10-row hardcoded
// roster of fake Filipino driver names that flashed on every page load
// and stuck around if the /v1/drivers fetch failed — making real onboarded
// drivers (e.g. a single registered driver) impossible to spot through
// the noise. We now show only what the API returns; an empty roster
// renders the empty-state card below the grid.
const DRIVERS: Driver[] = [];

const STATUS_CONFIG: Record<DriverStatus, { label: string; variant: "green" | "cyan" | "amber" | "red" | "purple"; dot: boolean; isActive: boolean }> = {
  offline:    { label: "Offline",    variant: "red",    dot: false, isActive: false },
  available:  { label: "Online",     variant: "green",  dot: true,  isActive: true  },
  en_route:   { label: "En Route",   variant: "cyan",   dot: true,  isActive: true  },
  delivering: { label: "Delivering", variant: "green",  dot: true,  isActive: true  },
  returning:  { label: "Returning",  variant: "purple", dot: false, isActive: true  },
  on_break:   { label: "On Break",   variant: "amber",  dot: false, isActive: false },
};

const GRADE_COLOR: Record<Driver["grade"], string> = {
  A: "text-green-signal",
  B: "text-cyan-neon",
  C: "text-amber-signal",
  D: "text-red-signal",
};

// part_time + non-zero per-delivery rate = gig worker (dispatch broadcasts offers to this pool).
function isGigWorker(d: Driver): boolean {
  return d.driver_type === "part_time" && d.per_delivery_rate_cents > 0;
}

// Zeroed initial KPI strip. Real values come from /v1/drivers/summary —
// rendering 7 / 172 / 113 / 83600 before the API responds was lying to ops.
const KPI = [
  { label: "Online Drivers",  value: 0, trend: 0, color: "green"  as const, format: "number"   as const },
  { label: "Tasks Assigned",  value: 0, trend: 0, color: "cyan"   as const, format: "number"   as const },
  { label: "Tasks Complete",  value: 0, trend: 0, color: "purple" as const, format: "number"   as const },
  { label: "COD Collected",   value: 0, trend: 0, color: "amber"  as const, format: "currency" as const },
];

// Coarse backend-string → UI DriverStatus mapping for fresh API payloads.
// Unknown values fall through to 'offline' so stale clients don't crash.
function normalizeStatus(s: string): DriverStatus {
  switch (s) {
    case "offline":
    case "available":
    case "en_route":
    case "delivering":
    case "returning":
    case "on_break":
      return s;
    default:
      return "offline";
  }
}

export default function DriversPage() {
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<DriverStatus | "all" | "online">("all");
  const [typeFilter, setTypeFilter]     = useState<"all" | "gig">("all");
  const [drivers, setDrivers] = useState<Driver[]>(DRIVERS);
  const [kpi, setKpi] = useState(KPI);
  const [loading, setLoading] = useState(false);
  const [onboardOpen, setOnboardOpen] = useState(false);
  // entity_id (driver_id) → overall_status from the compliance service
  const [complianceMap, setComplianceMap] = useState<Map<string, string>>(new Map());

  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  const { driverMap, connected, refresh } = useDriverRoster();
  const { hasPermission } = usePermissions();
  const canCreateDriver = hasPermission("drivers:create");
  const canManageDrivers = hasPermission("drivers:manage");

  const handleDeleteDriver = async (driverId: string) => {
    setDeletingId(driverId);
    try {
      const api = createDriversApi();
      await api.deleteDriver(driverId);
      setDrivers((prev) => prev.filter((d) => d.id !== driverId));
    } catch (err: unknown) {
      const e = err as { message?: string };
      alert(e.message ?? "Failed to delete driver");
    } finally {
      setDeletingId(null);
      setConfirmDeleteId(null);
    }
  };

  const fetchDrivers = useCallback(async () => {
    setLoading(true);
    try {
      const api = createDriversApi();
      const [listRes, summaryRes, profiles] = await Promise.all([
        api.listDrivers({ per_page: 100 }),
        api.getSummary(),
        fetchProfiles().catch(() => [] as ComplianceProfile[]),
      ]);

      // Build entity_id → status map for O(1) badge lookups.
      setComplianceMap(new Map(profiles.map((p) => [p.entity_id, p.overall_status])));
      setDrivers(listRes.data.map((d: ApiDriver) => ({
        id:            d.id,
        name:          d.name || `${d.first_name} ${d.last_name}`.trim(),
        vehicle:       d.vehicle_type,
        plate:         d.vehicle_plate,
        status:        normalizeStatus(d.status as string),
        tasks_total:   d.tasks_total,
        tasks_done:    d.tasks_done,
        last_location: d.last_location ?? "Unknown",
        last_seen:     d.last_seen_at ? new Date(d.last_seen_at).toLocaleTimeString() : "—",
        grade:         d.performance_grade,
        cod_collected: d.cod_collected,
        driver_type:              d.driver_type ?? "full_time",
        per_delivery_rate_cents:  d.per_delivery_rate_cents ?? 0,
      })));
      const s = summaryRes.data;
      setKpi([
        { label: "Online Drivers",  value: s.online,                             trend: 0, color: "green"  as const, format: "number"   as const },
        { label: "Tasks Assigned",  value: s.total_tasks_assigned,               trend: 0, color: "cyan"   as const, format: "number"   as const },
        { label: "Tasks Complete",  value: s.total_tasks_completed,              trend: 0, color: "purple" as const, format: "number"   as const },
        { label: "COD Collected",   value: Math.round(s.total_cod_collected / 100), trend: 0, color: "amber" as const, format: "currency" as const },
      ]);
    } catch {
      // Leave the roster empty on fetch failure — silently falling back
      // to seeded mock data made real outages invisible.
      setDrivers([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { fetchDrivers(); }, [fetchDrivers]);

  // ── Live roster patch from shared context ───────────────────────────────────
  // Context carries status + GPS; page's own fetch carries full profile data.
  useEffect(() => {
    if (Object.keys(driverMap).length === 0) return;
    setDrivers((prev) =>
      prev.map((d) => {
        const pin = driverMap[d.id];
        if (!pin) return d;
        return {
          ...d,
          status: pin.status as DriverStatus,
          last_location:
            pin.lat != null
              ? `${pin.lat.toFixed(4)}, ${pin.lng.toFixed(4)}`
              : d.last_location,
          last_seen: "Live",
        };
      })
    );
  }, [driverMap]);

  const filtered = drivers.filter((d) => {
    const cfg = STATUS_CONFIG[d.status];
    const matchStatus =
      statusFilter === "all" ||
      (statusFilter === "online" && cfg.isActive) ||
      d.status === statusFilter;
    const matchType   = typeFilter === "all" || (typeFilter === "gig" && isGigWorker(d));
    const matchSearch = !search || d.name.toLowerCase().includes(search.toLowerCase()) || d.plate.toLowerCase().includes(search.toLowerCase());
    return matchStatus && matchType && matchSearch;
  });

  const onlineCount      = drivers.filter((d) => STATUS_CONFIG[d.status].isActive).length;
  const gigCount         = drivers.filter(isGigWorker).length;
  const pendingGigCount  = drivers.filter((d) => {
    if (!isGigWorker(d)) return false;
    const cs = complianceMap.get(d.id);
    // under_review = docs submitted but not yet approved — still blocks offer claiming.
    return !cs || cs === "pending_submission" || cs === "under_review";
  }).length;

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
          <div className="flex flex-wrap items-center gap-2 mt-0.5">
            <p className="text-sm text-white/40 font-mono">{onlineCount} online · {drivers.length} total roster</p>
            {pendingGigCount > 0 && (
              <button
                onClick={() => setTypeFilter("gig")}
                className="inline-flex items-center gap-1 rounded-full border border-amber-signal/30 bg-amber-signal/10 px-2 py-0.5 text-2xs font-mono text-amber-signal hover:bg-amber-signal/20 transition-colors"
              >
                <AlertCircle size={10} />
                {pendingGigCount} gig pending
              </button>
            )}
          </div>
        </div>
        <div className="flex items-center gap-2">
          {!connected && (
            <span className="inline-flex items-center gap-1.5 rounded-full border border-red-signal/30 bg-red-signal/10 px-2.5 py-1 text-2xs font-mono text-red-signal">
              WS disconnected
            </span>
          )}
          <button
            onClick={() => { fetchDrivers(); refresh(); }}
            disabled={loading}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-50"
          >
            <RefreshCw size={12} className={loading ? "animate-spin" : ""} /> Refresh
          </button>
          {canCreateDriver && (
            <button
              onClick={() => setOnboardOpen(true)}
              className="flex items-center gap-1.5 rounded-lg border border-cyan-neon/30 bg-cyan-neon/10 px-3 py-2 text-xs font-semibold text-cyan-neon hover:bg-cyan-neon/20 transition-all"
            >
              <UserPlus size={12} /> Onboard Driver
            </button>
          )}
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

      {/* Pending gig workers banner */}
      {pendingGigCount > 0 && (
        <motion.div variants={variants.fadeInUp}>
          <button
            onClick={() => setTypeFilter("gig")}
            className="w-full flex items-center gap-3 rounded-lg border border-amber-signal/25 bg-amber-signal/5 px-4 py-3 text-left transition-colors hover:bg-amber-signal/10"
          >
            <AlertCircle size={14} className="flex-shrink-0 text-amber-signal" />
            <p className="text-xs text-white/70">
              <span className="font-semibold text-amber-signal">
                {pendingGigCount} gig worker{pendingGigCount !== 1 ? "s" : ""}
              </span>{" "}
              pending compliance approval — cannot claim broadcast offers until documents are reviewed.{" "}
              <span className="font-medium text-amber-signal underline-offset-2 hover:underline">
                View gig workers →
              </span>
            </p>
          </button>
        </motion.div>
      )}

      {/* Filters */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          <div className="flex flex-wrap items-center gap-4">
            <div className="flex flex-wrap items-center gap-1.5">
              {(["all", "online", "available", "en_route", "delivering", "on_break", "offline"] as const).map((s) => (
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
            {/* Driver type filter — gig workers are part_time with a non-zero per-delivery rate */}
            {gigCount > 0 && (
              <div className="flex items-center gap-1.5 border-l border-glass-border/50 pl-4">
                <button
                  onClick={() => setTypeFilter("all")}
                  className={`rounded-full px-3 py-1 text-xs font-medium transition-all ${
                    typeFilter === "all"
                      ? "bg-glass-200 text-white border border-glass-border-bright"
                      : "text-white/40 border border-transparent hover:text-white"
                  }`}
                >
                  All types
                </button>
                <button
                  onClick={() => setTypeFilter("gig")}
                  className={`relative inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium transition-all ${
                    typeFilter === "gig"
                      ? "bg-purple-surface text-purple-plasma border border-purple-plasma/30"
                      : "text-white/40 border border-glass-border hover:text-white"
                  }`}
                >
                  <Zap size={10} />
                  Gig Workers ({gigCount})
                  {pendingGigCount > 0 && (
                    <span className="ml-0.5 inline-flex h-4 min-w-[1rem] items-center justify-center rounded-full bg-amber-signal px-1 font-mono text-2xs text-canvas leading-none">
                      {pendingGigCount}
                    </span>
                  )}
                </button>
              </div>
            )}
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

      {/* Onboard modal */}
      <OnboardDriverModal
        open={onboardOpen}
        onClose={() => setOnboardOpen(false)}
        onSuccess={fetchDrivers}
      />

      {/* Empty state — shown when the API returns no drivers (or returned
          an error and we cleared the roster). The Onboard button above is
          the only path forward, so highlight it visually. */}
      {!loading && drivers.length === 0 && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="flex flex-col items-center justify-center gap-3 py-12 text-center">
            <div className="flex h-12 w-12 items-center justify-center rounded-full border border-glass-border bg-glass-100">
              <UserPlus size={20} className="text-white/30" />
            </div>
            <div>
              <p className="text-sm font-semibold text-white">No drivers onboarded yet</p>
              <p className="mt-1 text-2xs font-mono text-white/40">
                Use <span className="text-cyan-neon">Onboard Driver</span> above to register your first courier
              </p>
            </div>
          </GlassCard>
        </motion.div>
      )}

      {/* Filtered-but-empty state — drivers exist but none match the current filter/search */}
      {!loading && drivers.length > 0 && filtered.length === 0 && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="flex flex-col items-center justify-center gap-2 py-8 text-center">
            <p className="text-xs text-white/40">
              No drivers match the current filter
              {search && <> · search “<span className="font-mono text-white/60">{search}</span>”</>}
            </p>
          </GlassCard>
        </motion.div>
      )}

      {/* Driver grid */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        {filtered.map((driver) => {
          const cfg = STATUS_CONFIG[driver.status];
          const progress = driver.tasks_total > 0 ? (driver.tasks_done / driver.tasks_total) * 100 : 0;
          const complianceStatus = complianceMap.get(driver.id);
          return (
            <GlassCard key={driver.id} className="hover:border-glass-border-bright transition-colors cursor-pointer">
              <div className="flex items-start justify-between mb-3">
                <div className="flex items-center gap-3">
                  <div className="relative">
                    <div className="h-9 w-9 rounded-full bg-gradient-to-br from-cyan-neon/20 to-purple-plasma/20 flex items-center justify-center border border-glass-border">
                      <span className="text-sm font-bold text-white">{driver.name.split(" ").map(n => n[0]).join("").slice(0,2)}</span>
                    </div>
                    {cfg.isActive && (
                      <span className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full bg-green-signal border-2 border-canvas" />
                    )}
                  </div>
                  <div>
                    <div className="flex items-center gap-1.5">
                      <p className="text-sm font-semibold text-white">{driver.name}</p>
                      {complianceStatus === "compliant" && (
                        <span aria-label="Compliance: Compliant">
                          <ShieldCheck size={12} className="text-green-signal" />
                        </span>
                      )}
                      {(complianceStatus === "under_review" || complianceStatus === "expiring_soon") && (
                        <span aria-label={`Compliance: ${complianceStatus.replace("_", " ")}`}>
                          <ShieldAlert size={12} className="text-amber-signal" />
                        </span>
                      )}
                      {(complianceStatus === "pending_submission" || complianceStatus === "suspended") && (
                        <span aria-label={`Compliance: ${complianceStatus.replace("_", " ")}`}>
                          <ShieldX size={12} className="text-red-signal" />
                        </span>
                      )}
                    </div>
                    <p className="text-2xs font-mono text-white/40">{driver.vehicle} · {driver.plate}</p>
                    {isGigWorker(driver) && (
                      <p className="text-2xs font-mono">
                        {(!complianceStatus || complianceStatus === "pending_submission") ? (
                          <span className="text-amber-signal">Pending — awaiting compliance docs</span>
                        ) : complianceStatus === "under_review" ? (
                          <span className="text-amber-signal">Under review — offer claim blocked</span>
                        ) : (
                          <span className="text-green-signal">
                            ₱{(driver.per_delivery_rate_cents / 100).toFixed(0)}/delivery · eligible for offers
                          </span>
                        )}
                      </p>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  {isGigWorker(driver) && (
                    <NeonBadge variant="purple">
                      <Zap size={9} className="mr-0.5 inline" />Gig
                    </NeonBadge>
                  )}
                  <span className={`text-lg font-bold font-heading ${GRADE_COLOR[driver.grade]}`}>{driver.grade}</span>
                  <NeonBadge variant={cfg.variant} dot={cfg.dot}>{cfg.label}</NeonBadge>
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

              {/* Cross-portal deep link — partner-portal owns driver commission/SLA.
                  Plain <a> so the /partner basePath is preserved across the jump. */}
              <div className="mt-2.5 flex items-center justify-between border-t border-glass-border/40 pt-2">
                <a
                  href={`/partner/drivers?focus=${encodeURIComponent(driver.id)}`}
                  onClick={(e) => e.stopPropagation()}
                  className="inline-flex items-center gap-1 rounded-lg border border-glass-border bg-glass-100 px-2 py-1 text-2xs text-white/50 transition-all hover:border-purple-plasma/40 hover:text-purple-plasma"
                >
                  <Briefcase size={10} />
                  Manage in Partner Portal
                </a>

                {/* Delete — only shown for offline drivers with manage permission */}
                {canManageDrivers && driver.status === "offline" && (
                  confirmDeleteId === driver.id ? (
                    <div className="flex items-center gap-1.5">
                      <span className="text-2xs text-white/40 font-mono">Confirm?</span>
                      <button
                        onClick={(e) => { e.stopPropagation(); handleDeleteDriver(driver.id); }}
                        disabled={deletingId === driver.id}
                        className="rounded px-2 py-0.5 text-2xs font-semibold text-red-400 border border-red-500/30 bg-red-500/10 hover:bg-red-500/20 transition-all disabled:opacity-50"
                      >
                        {deletingId === driver.id ? "Deleting…" : "Yes, delete"}
                      </button>
                      <button
                        onClick={(e) => { e.stopPropagation(); setConfirmDeleteId(null); }}
                        className="rounded px-2 py-0.5 text-2xs text-white/40 border border-glass-border hover:text-white transition-colors"
                      >
                        Cancel
                      </button>
                    </div>
                  ) : (
                    <button
                      onClick={(e) => { e.stopPropagation(); setConfirmDeleteId(driver.id); }}
                      className="inline-flex items-center gap-1 rounded-lg border border-red-500/20 bg-red-500/5 px-2 py-1 text-2xs text-red-400/60 transition-all hover:border-red-500/40 hover:text-red-400 hover:bg-red-500/10"
                    >
                      <Trash2 size={10} />
                      Remove
                    </button>
                  )
                )}
              </div>
            </GlassCard>
          );
        })}
      </motion.div>
    </motion.div>
  );
}
