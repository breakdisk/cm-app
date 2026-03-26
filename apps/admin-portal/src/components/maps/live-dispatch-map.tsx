"use client";
import { useRef, useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { cn } from "@/lib/design-system/cn";
import { colors } from "@/lib/design-system/tokens";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface DriverPin {
  driver_id: string;
  driver_name: string;
  lat: number;
  lng: number;
  status: "idle" | "en_route" | "delivering" | "returning";
  deliveries_remaining: number;
}

export interface RouteGeoJson {
  driver_id: string;
  geojson: GeoJSON.FeatureCollection;
  color: string;
}

interface LiveDispatchMapProps {
  drivers?: DriverPin[];
  routes?: RouteGeoJson[];
  onDriverClick?: (driver: DriverPin) => void;
  className?: string;
}

const statusColor: Record<DriverPin["status"], string> = {
  idle:       colors.amber.signal,
  en_route:   colors.cyan.neon,
  delivering: colors.green.signal,
  returning:  colors.purple.plasma,
};

const statusLabel: Record<DriverPin["status"], string> = {
  idle:       "Idle",
  en_route:   "En Route",
  delivering: "Delivering",
  returning:  "Returning",
};

// ── Metro Manila approximate normalized positions [0..100] ───────────────────
// These positions simulate a top-down view of Metro Manila geography.
const MANILA_LAYOUT: Record<string, { x: number; y: number }> = {
  "Caloocan City": { x: 28, y: 18 },
  "Quezon City":   { x: 52, y: 22 },
  "Marikina":      { x: 70, y: 28 },
  "Pasig City":    { x: 65, y: 40 },
  "Mandaluyong":   { x: 46, y: 38 },
  "Makati CBD":    { x: 44, y: 50 },
  "BGC Taguig":    { x: 56, y: 62 },
  "Taguig City":   { x: 58, y: 67 },
  "Parañaque":     { x: 44, y: 75 },
  "Las Piñas":     { x: 36, y: 78 },
  "Muntinlupa":    { x: 42, y: 84 },
  "Manila City":   { x: 32, y: 46 },
  "Pasay City":    { x: 36, y: 58 },
  "Valenzuela":    { x: 24, y: 12 },
};

// Deterministic position from driver_id
function getDriverPosition(driver: DriverPin) {
  const keys   = Object.keys(MANILA_LAYOUT);
  const idx    = driver.driver_id.charCodeAt(driver.driver_id.length - 1) % keys.length;
  const base   = MANILA_LAYOUT[keys[idx]];
  // Add small spread per driver
  const seed   = driver.driver_id.charCodeAt(0) + driver.driver_id.charCodeAt(1);
  return {
    x: base.x + ((seed * 7) % 6) - 3,
    y: base.y + ((seed * 11) % 6) - 3,
  };
}

// ── Simulation map (no Mapbox token) ─────────────────────────────────────────

function SimulationMap({
  drivers,
  onDriverClick,
  className,
}: {
  drivers: DriverPin[];
  onDriverClick?: (d: DriverPin) => void;
  className?: string;
}) {
  const [selected, setSelected] = useState<string | null>(null);
  const [tick, setTick]         = useState(0);

  // Animate driver positions with slight jitter every 2.5s
  useEffect(() => {
    const id = setInterval(() => setTick((t) => t + 1), 2500);
    return () => clearInterval(id);
  }, []);

  const positions = drivers.map((d) => {
    const base  = getDriverPosition(d);
    const jitter = d.status !== "idle" ? ((tick + d.driver_id.charCodeAt(0)) % 3) - 1 : 0;
    return { ...base, x: base.x + jitter * 0.4, y: base.y + jitter * 0.4 };
  });

  function handleClick(driver: DriverPin) {
    setSelected((prev) => (prev === driver.driver_id ? null : driver.driver_id));
    onDriverClick?.(driver);
  }

  const sel = drivers.find((d) => d.driver_id === selected);

  return (
    <div className={cn("relative w-full h-full overflow-hidden rounded-2xl border border-glass-border bg-[#060c1a]", className)}>

      {/* ── Grid overlay ───────────────────────────────────────────────── */}
      <svg className="absolute inset-0 w-full h-full pointer-events-none opacity-[0.07]" preserveAspectRatio="none">
        {/* Horizontal lines */}
        {Array.from({ length: 10 }).map((_, i) => (
          <line key={`h${i}`} x1="0" y1={`${(i + 1) * 9}%`} x2="100%" y2={`${(i + 1) * 9}%`} stroke="white" strokeWidth="0.5" />
        ))}
        {/* Vertical lines */}
        {Array.from({ length: 7 }).map((_, i) => (
          <line key={`v${i}`} x1={`${(i + 1) * 13}%`} y1="0" x2={`${(i + 1) * 13}%`} y2="100%" stroke="white" strokeWidth="0.5" />
        ))}
      </svg>

      {/* ── Road network ───────────────────────────────────────────────── */}
      <svg className="absolute inset-0 w-full h-full pointer-events-none" preserveAspectRatio="none">
        {/* EDSA ring road */}
        <path
          d="M28,15 Q55,10 72,28 Q82,45 72,65 Q58,80 40,82 Q22,75 16,58 Q10,40 28,15"
          fill="none" stroke="rgba(255,255,255,0.08)" strokeWidth="3"
        />
        {/* C5 */}
        <path
          d="M65,22 Q72,40 70,60 Q65,74 58,80"
          fill="none" stroke="rgba(255,255,255,0.05)" strokeWidth="2"
        />
        {/* Radial roads */}
        <line x1="32" y1="46" x2="44" y2="50" stroke="rgba(255,255,255,0.06)" strokeWidth="1.5" />
        <line x1="44" y1="50" x2="56" y2="62" stroke="rgba(255,255,255,0.06)" strokeWidth="1.5" />
        <line x1="28" y1="15" x2="44" y2="50" stroke="rgba(255,255,255,0.04)" strokeWidth="1" />
        {/* Manila Bay coast */}
        <path
          d="M10,35 Q14,42 18,52 Q22,62 28,68"
          fill="none" stroke="rgba(0,229,255,0.06)" strokeWidth="4"
        />
        <text x="6" y="52" fontSize="7" fill="rgba(0,229,255,0.15)" fontFamily="monospace">Manila Bay</text>
      </svg>

      {/* ── Route lines between active drivers ─────────────────────────── */}
      <svg className="absolute inset-0 w-full h-full pointer-events-none">
        {drivers
          .filter((d) => d.status === "en_route" || d.status === "delivering")
          .map((d, i) => {
            const pos   = positions[drivers.indexOf(d)];
            const depot = { x: 50, y: 50 }; // central depot
            return (
              <motion.line
                key={d.driver_id}
                x1={`${pos.x}%`} y1={`${pos.y}%`}
                x2={`${depot.x}%`} y2={`${depot.y}%`}
                stroke={statusColor[d.status]}
                strokeWidth="1"
                strokeDasharray="4 3"
                strokeOpacity="0.25"
                animate={{ strokeOpacity: [0.15, 0.35, 0.15] }}
                transition={{ duration: 3, repeat: Infinity, delay: i * 0.4 }}
              />
            );
          })}
      </svg>

      {/* ── Zone labels ────────────────────────────────────────────────── */}
      {[
        { label: "Quezon City",  x: 50, y: 18, opacity: 0.2 },
        { label: "Makati",       x: 42, y: 53, opacity: 0.22 },
        { label: "BGC",          x: 55, y: 65, opacity: 0.2 },
        { label: "Pasig",        x: 65, y: 37, opacity: 0.18 },
        { label: "Caloocan",     x: 22, y: 20, opacity: 0.16 },
        { label: "Parañaque",    x: 40, y: 78, opacity: 0.15 },
      ].map((z) => (
        <div
          key={z.label}
          className="absolute pointer-events-none font-mono text-[9px] uppercase tracking-widest"
          style={{
            left:    `${z.x}%`,
            top:     `${z.y}%`,
            color:   `rgba(255,255,255,${z.opacity})`,
            transform: "translate(-50%,-50%)",
          }}
        >
          {z.label}
        </div>
      ))}

      {/* ── Depot marker ───────────────────────────────────────────────── */}
      <div
        className="absolute pointer-events-none"
        style={{ left: "50%", top: "50%", transform: "translate(-50%,-50%)" }}
      >
        <div className="relative flex items-center justify-center">
          <div className="h-3 w-3 rounded-full border-2 border-amber-signal bg-canvas-100" style={{ boxShadow: "0 0 8px #FFAB00" }} />
          <span className="absolute top-4 left-1/2 -translate-x-1/2 font-mono text-[8px] text-amber-signal/50 whitespace-nowrap">DEPOT</span>
        </div>
      </div>

      {/* ── Driver markers ──────────────────────────────────────────────── */}
      {drivers.map((driver, i) => {
        const pos   = positions[i];
        const color = statusColor[driver.status];
        const isSel = selected === driver.driver_id;

        return (
          <motion.div
            key={driver.driver_id}
            className="absolute cursor-pointer"
            style={{ left: `${pos.x}%`, top: `${pos.y}%`, zIndex: isSel ? 30 : 10 }}
            animate={{ x: `${pos.x}%`, y: `${pos.y}%` }}
            transition={{ duration: 2, ease: "easeInOut" }}
            onClick={() => handleClick(driver)}
          >
            <motion.div
              className="relative flex items-center justify-center"
              animate={{ scale: isSel ? 1.4 : 1 }}
              transition={{ type: "spring", stiffness: 300 }}
            >
              {/* Beacon ring */}
              {driver.status !== "idle" && (
                <motion.span
                  className="absolute rounded-full"
                  style={{
                    width: 24, height: 24,
                    background: color,
                    opacity: 0.25,
                  }}
                  animate={{ scale: [1, 2, 1], opacity: [0.25, 0, 0.25] }}
                  transition={{ duration: 2, repeat: Infinity, ease: "easeOut" }}
                />
              )}
              {/* Driver dot */}
              <div
                className="relative h-4 w-4 rounded-full border-2 border-canvas flex items-center justify-center"
                style={{ background: color, boxShadow: `0 0 8px ${color}88` }}
              >
                {driver.deliveries_remaining > 0 && (
                  <span
                    className="absolute -top-2.5 -right-2.5 h-3.5 w-3.5 rounded-full bg-canvas flex items-center justify-center text-[8px] font-mono font-bold border border-glass-border"
                    style={{ color }}
                  >
                    {driver.deliveries_remaining}
                  </span>
                )}
              </div>
            </motion.div>
          </motion.div>
        );
      })}

      {/* ── Driver tooltip ──────────────────────────────────────────────── */}
      <AnimatePresence>
        {sel && (() => {
          const idx = drivers.indexOf(sel);
          const pos = positions[idx];
          return (
            <motion.div
              key="tooltip"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 6 }}
              transition={{ duration: 0.2 }}
              className="absolute z-40 pointer-events-none"
              style={{
                left:      `${pos.x + 2}%`,
                top:       `${pos.y + 3}%`,
                minWidth:  160,
              }}
            >
              <div className="glass-sm rounded-xl p-3 border border-glass-border-bright">
                <p className="text-xs font-semibold text-white leading-tight mb-0.5">{sel.driver_name}</p>
                <p className="text-2xs font-mono" style={{ color: statusColor[sel.status] }}>
                  {statusLabel[sel.status]}
                </p>
                {sel.deliveries_remaining > 0 && (
                  <p className="text-2xs font-mono text-white/40 mt-1">
                    {sel.deliveries_remaining} stops remaining
                  </p>
                )}
              </div>
            </motion.div>
          );
        })()}
      </AnimatePresence>

      {/* ── Legend ──────────────────────────────────────────────────────── */}
      <div className="absolute bottom-4 left-4 glass-sm rounded-xl p-3 flex flex-col gap-1.5 z-20">
        {(Object.entries(statusColor) as [DriverPin["status"], string][]).map(([status, color]) => (
          <div key={status} className="flex items-center gap-2">
            <span className="h-2 w-2 rounded-full flex-shrink-0" style={{ background: color, boxShadow: `0 0 5px ${color}` }} />
            <span className="text-2xs font-mono text-white/50 capitalize">{statusLabel[status]}</span>
          </div>
        ))}
      </div>

      {/* ── Live badge ──────────────────────────────────────────────────── */}
      <div className="absolute top-4 right-4 z-20 flex flex-col items-end gap-2">
        <span className="inline-flex items-center gap-1.5 glass-sm px-2.5 py-1 rounded-full">
          <span className="relative flex h-1.5 w-1.5">
            <span className="absolute inline-flex h-full w-full rounded-full bg-green-signal opacity-75 animate-beacon" />
            <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-green-signal" />
          </span>
          <span className="text-2xs font-mono text-green-signal uppercase tracking-widest">Live</span>
        </span>
        <span className="text-2xs font-mono text-white/25 glass-sm px-2 py-1 rounded-lg">
          {drivers.filter(d => d.status !== "idle").length}/{drivers.length} active
        </span>
      </div>

      {/* ── Click hint ──────────────────────────────────────────────────── */}
      <div className="absolute bottom-4 right-4 z-20">
        <span className="text-2xs font-mono text-white/20">Click driver to inspect</span>
      </div>
    </div>
  );
}

// ── Mapbox map (with token) ───────────────────────────────────────────────────

function MapboxMap({
  drivers,
  routes = [],
  onDriverClick,
  className,
}: LiveDispatchMapProps & { drivers: DriverPin[] }) {
  const [Map, setMap]     = useState<any>(null);
  const [libs, setLibs]   = useState<any>(null);
  const mapRef            = useRef<any>(null);
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      import("react-map-gl"),
      import("mapbox-gl/dist/mapbox-gl.css" as any),
    ]).then(([mapgl]) => {
      setMap(() => mapgl.default);
      setLibs(mapgl);
    });
  }, []);

  const MAPBOX_TOKEN = process.env.NEXT_PUBLIC_MAPBOX_TOKEN ?? "";

  useEffect(() => {
    if (!drivers.length || !mapRef.current) return;
    const lngs = drivers.map((d) => d.lng);
    const lats  = drivers.map((d) => d.lat);
    mapRef.current.fitBounds(
      [[Math.min(...lngs) - 0.02, Math.min(...lats) - 0.02],
       [Math.max(...lngs) + 0.02, Math.max(...lats) + 0.02]],
      { padding: 60, duration: 1200 }
    );
  }, [drivers]);

  if (!Map || !libs) return null;
  const { Marker, Source, Layer } = libs;

  return (
    <div className={cn("relative rounded-2xl overflow-hidden border border-glass-border", className)}>
      <Map
        ref={mapRef}
        mapboxAccessToken={MAPBOX_TOKEN}
        mapStyle="mapbox://styles/mapbox/dark-v11"
        style={{ width: "100%", height: "100%" }}
        initialViewState={{ longitude: 121.774, latitude: 12.879, zoom: 6 }}
        attributionControl={false}
      >
        {routes.map((route) => (
          <Source key={route.driver_id} type="geojson" data={route.geojson}>
            <Layer id={`route-${route.driver_id}`} type="line" paint={{
              "line-color": route.color, "line-width": 2, "line-opacity": 0.7, "line-dasharray": [2, 1],
            }} />
          </Source>
        ))}
        {drivers.map((driver) => (
          <Marker key={driver.driver_id} longitude={driver.lng} latitude={driver.lat} anchor="center"
            onClick={() => { setSelected(driver.driver_id); onDriverClick?.(driver); }}
          >
            <motion.div initial={{ scale: 0 }} animate={{ scale: 1 }} whileHover={{ scale: 1.2 }} className="relative cursor-pointer">
              <span className="absolute inset-0 rounded-full animate-beacon" style={{ background: statusColor[driver.status] }} />
              <span className="relative flex h-4 w-4 rounded-full border-2 border-canvas" style={{ background: statusColor[driver.status] }} />
              {driver.deliveries_remaining > 0 && (
                <span className="absolute -top-2.5 -right-2.5 flex h-4 w-4 items-center justify-center rounded-full bg-canvas border border-glass-border text-2xs font-mono text-white/70">
                  {driver.deliveries_remaining}
                </span>
              )}
            </motion.div>
          </Marker>
        ))}
      </Map>
    </div>
  );
}

// ── Exported component ────────────────────────────────────────────────────────

export function LiveDispatchMap({
  drivers = [],
  routes  = [],
  onDriverClick,
  className,
}: LiveDispatchMapProps) {
  const MAPBOX_TOKEN = process.env.NEXT_PUBLIC_MAPBOX_TOKEN ?? "";

  if (!MAPBOX_TOKEN) {
    return (
      <SimulationMap
        drivers={drivers}
        onDriverClick={onDriverClick}
        className={className}
      />
    );
  }

  return (
    <MapboxMap
      drivers={drivers}
      routes={routes}
      onDriverClick={onDriverClick}
      className={className}
    />
  );
}
