"use client";
/**
 * Admin Portal — Drivers Page
 * Live driver roster from driver-ops backend.
 * Real-time updates via RosterEvent WebSocket (status + GPS).
 */
import { useState, useEffect, useCallback, useMemo } from "react";
import { createDriversApi, Driver as ApiDriver, driverFullName } from "@/lib/api/drivers";
import { useRosterEvents } from "@/hooks/useRosterEvents";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { Search, MapPin, Phone, RefreshCw, Briefcase, Users } from "lucide-react";

// ── Local UI state type ────────────────────────────────────────────────────────

type DriverStatus =
  | "offline"
  | "available"
  | "en_route"
  | "delivering"
  | "returning"
  | "on_break";

interface Driver {
  id: string;
  name: string;       // first_name + last_name
  phone: string;
  vehicle_type: string;
  driver_type: string;
  zone: string | null;
  status: DriverStatus;
  is_online: boolean;
  lat: number | null;
  lng: number | null;
  last_location_at: string | null;
  is_active: boolean;
}

const STATUS_CONFIG: Record<
  DriverStatus,
  { label: string; variant: "green" | "cyan" | "amber" | "red" | "purple"; dot: boolean; isActive: boolean }
> = {
  offline:    { label: "Offline",    variant: "red",    dot: false, isActive: false },
  available:  { label: "Available",  variant: "green",  dot: true,  isActive: true  },
  en_route:   { label: "En Route",   variant: "cyan",   dot: true,  isActive: true  },
  delivering: { label: "Delivering", variant: "green",  dot: true,  isActive: true  },
  returning:  { label: "Returning",  variant: "purple", dot: false, isActive: true  },
  on_break:   { label: "On Break",   variant: "amber",  dot: false, isActive: false },
};

function normalizeStatus(s: string): DriverStatus {
  const valid: DriverStatus[] = ["offline", "available", "en_route", "delivering", "returning", "on_break"];
  return (valid.includes(s as DriverStatus) ? s : "offline") as DriverStatus;
}

function toUiDriver(d: ApiDriver): Driver {
  return {
    id:              d.id,
    name:            driverFullName(d),
    phone:           d.phone,
    vehicle_type:    d.vehicle_type,
    driver_type:     d.driver_type,
    zone:            d.zone,
    status:          normalizeStatus(d.status as string),
    is_online:       d.is_online,
    lat:             d.lat,
    lng:             d.lng,
    last_location_at: d.last_location_at,
    is_active:       d.is_active,
  };
}

function formatLastSeen(iso: string | null): string {
  if (!iso) return "—";
  const diff = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (diff < 30) return "Just now";
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  return `${Math.floor(diff / 3600)}h ago`;
}

// ── Page ───────────────────────────────────────────────────────────────────────

export default function DriversPage() {
  const [search, setSearch]             = useState("");
  const [statusFilter, setStatusFilter] = useState<DriverStatus | "all" | "online">("all");
  const [drivers, setDrivers]           = useState<Driver[]>([]);
  const [loading, setLoading]           = useState(true);
  const [error, setError]               = useState<string | null>(null);

  const fetchDrivers = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const api = createDriversApi();
      const res = await api.listDrivers({ per_page: 200 });
      setDrivers(res.data.map(toUiDriver));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load drivers");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { fetchDrivers(); }, [fetchDrivers]);

  // ── Live roster WS ──────────────────────────────────────────────────────────
  useRosterEvents((event) => {
    setDrivers((prev) => {
      const idx = prev.findIndex((d) => d.id === event.driver_id);
      if (idx === -1) return prev;
      const next = [...prev];
      if (event.type === "status_changed") {
        next[idx] = { ...next[idx], status: normalizeStatus(event.status), is_online: event.status !== "offline" };
      } else {
        next[idx] = { ...next[idx], lat: event.lat, lng: event.lng, last_location_at: new Date().toISOString() };
      }
      return next;
    });
  });

  // ── KPIs derived from live list ─────────────────────────────────────────────
  const kpi = useMemo(() => {
    const online   = drivers.filter((d) => d.is_online).length;
    const active   = drivers.filter((d) => STATUS_CONFIG[d.status].isActive).length;
    const onBreak  = drivers.filter((d) => d.status === "on_break").length;
    const offline  = drivers.filter((d) => d.status === "offline").length;
    return [
      { label: "Online",     value: online,          color: "green"  as const, format: "number" as const },
      { label: "Active",     value: active,          color: "cyan"   as const, format: "number" as const },
      { label: "On Break",   value: onBreak,         color: "amber"  as const, format: "number" as const },
      { label: "Offline",    value: offline,         color: "red"    as const, format: "number" as const },
    ];
  }, [drivers]);

  // ── Filtered roster ──────────────────────────────────────────────────────────
  const filtered = useMemo(() => {
    return drivers.filter((d) => {
      const cfg         = STATUS_CONFIG[d.status];
      const matchStatus =
        statusFilter === "all" ||
        (statusFilter === "online" && cfg.isActive) ||
        d.status === statusFilter;
      const q           = search.toLowerCase();
      const matchSearch = !q || d.name.toLowerCase().includes(q) || d.phone.includes(q);
      return matchStatus && matchSearch;
    });
  }, [drivers, statusFilter, search]);

  const onlineCount = useMemo(() => drivers.filter((d) => d.is_online).length, [drivers]);

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
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {onlineCount} online · {drivers.length} total roster
          </p>
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
            <LiveMetric label={m.label} value={m.value} trend={0} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

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
            <div className="ml-auto flex items-center gap-2 rounded-lg border border-glass-border bg-glass-100 px-3 py-2">
              <Search size={13} className="text-white/30" />
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Name or phone…"
                className="bg-transparent text-xs text-white placeholder:text-white/25 outline-none font-mono w-40"
              />
            </div>
          </div>
        </GlassCard>
      </motion.div>

      {/* Error state */}
      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="border-red-signal/30">
            <p className="text-sm text-red-signal/80 font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* Loading skeleton */}
      {loading && drivers.length === 0 && (
        <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 lg:grid-cols-2">
          {Array.from({ length: 6 }).map((_, i) => (
            <GlassCard key={i} className="animate-pulse">
              <div className="h-20 bg-glass-300 rounded" />
            </GlassCard>
          ))}
        </motion.div>
      )}

      {/* Empty state */}
      {!loading && !error && filtered.length === 0 && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="flex flex-col items-center py-12 gap-3">
            <Users size={32} className="text-white/20" />
            <p className="text-sm text-white/40">
              {drivers.length === 0 ? "No drivers registered yet." : "No drivers match the current filter."}
            </p>
          </GlassCard>
        </motion.div>
      )}

      {/* Driver grid */}
      {filtered.length > 0 && (
        <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 lg:grid-cols-2">
          {filtered.map((driver) => {
            const cfg     = STATUS_CONFIG[driver.status];
            const initials = driver.name.split(" ").map((n) => n[0]).join("").slice(0, 2).toUpperCase();
            const coords  = driver.lat != null && driver.lng != null
              ? `${driver.lat.toFixed(4)}, ${driver.lng.toFixed(4)}`
              : driver.zone ?? "Location unknown";
            return (
              <GlassCard
                key={driver.id}
                className="hover:border-glass-border-bright transition-colors cursor-pointer"
              >
                {/* Top row */}
                <div className="flex items-start justify-between mb-3">
                  <div className="flex items-center gap-3">
                    <div className="relative">
                      <div className="h-9 w-9 rounded-full bg-gradient-to-br from-cyan-neon/20 to-purple-plasma/20 flex items-center justify-center border border-glass-border">
                        <span className="text-sm font-bold text-white">{initials}</span>
                      </div>
                      {cfg.isActive && (
                        <span className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full bg-green-signal border-2 border-canvas" />
                      )}
                    </div>
                    <div>
                      <p className="text-sm font-semibold text-white">{driver.name}</p>
                      <p className="text-2xs font-mono text-white/40 capitalize">
                        {driver.vehicle_type} · {driver.driver_type}
                      </p>
                    </div>
                  </div>
                  <NeonBadge variant={cfg.variant} dot={cfg.dot}>{cfg.label}</NeonBadge>
                </div>

                {/* Phone + Location */}
                <div className="flex items-center justify-between text-2xs font-mono text-white/40">
                  <div className="flex items-center gap-1">
                    <Phone size={10} className="text-cyan-neon" />
                    {driver.phone}
                  </div>
                  <div className="flex items-center gap-1">
                    <MapPin size={10} className="text-cyan-neon" />
                    {coords}
                    {driver.last_location_at && (
                      <span className="text-white/25 ml-1">· {formatLastSeen(driver.last_location_at)}</span>
                    )}
                  </div>
                </div>

                {/* Partner portal deep link */}
                <div className="mt-2.5 flex items-center justify-end border-t border-glass-border/40 pt-2">
                  <a
                    href={`/partner/drivers?focus=${encodeURIComponent(driver.id)}`}
                    onClick={(e) => e.stopPropagation()}
                    className="inline-flex items-center gap-1 rounded-lg border border-glass-border bg-glass-100 px-2 py-1 text-2xs text-white/50 transition-all hover:border-purple-plasma/40 hover:text-purple-plasma"
                  >
                    <Briefcase size={10} />
                    Manage in Partner Portal
                  </a>
                </div>
              </GlassCard>
            );
          })}
        </motion.div>
      )}
    </motion.div>
  );
}
