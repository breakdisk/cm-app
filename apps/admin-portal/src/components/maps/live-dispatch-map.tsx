"use client";
import { useRef, useEffect, useState, useCallback } from "react";
import { motion } from "framer-motion";
import { cn } from "@/lib/design-system/cn";
import { colors } from "@/lib/design-system/tokens";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface DriverPin {
  driver_id: string;
  driver_name: string;
  lat: number;
  lng: number;
  status: "available" | "en_route" | "delivering" | "returning" | "on_break" | "offline";
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
  available:  colors.amber.signal,
  en_route:   colors.cyan.neon,
  delivering: colors.green.signal,
  returning:  colors.purple.plasma,
  on_break:   "#6B7280",
  offline:    "#374151",
};

const statusLabel: Record<DriverPin["status"], string> = {
  available:  "Available",
  en_route:   "En Route",
  delivering: "Delivering",
  returning:  "Returning",
  on_break:   "On Break",
  offline:    "Offline",
};

// ── Canvas fallback — always visible, no external tile dependency ──────────────

function CanvasFallbackMap({
  drivers,
  onDriverClick,
  className,
  notice,
}: {
  drivers: DriverPin[];
  onDriverClick?: (d: DriverPin) => void;
  className?: string;
  notice?: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 800, h: 500 });
  const [hovered, setHovered] = useState<string | null>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      setSize({ w: entry.contentRect.width, h: entry.contentRect.height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Mercator-ish projection bounded to the driver cluster (or PH if no drivers)
  const PAD = 0.08;
  const lats = drivers.map((d) => d.lat);
  const lngs = drivers.map((d) => d.lng);
  const minLat = lats.length ? Math.min(...lats) : 5;
  const maxLat = lats.length ? Math.max(...lats) : 20;
  const minLng = lngs.length ? Math.min(...lngs) : 116;
  const maxLng = lngs.length ? Math.max(...lngs) : 127;
  const latSpan = (maxLat - minLat) || 2;
  const lngSpan = (maxLng - minLng) || 2;
  const padLat = latSpan * PAD;
  const padLng = lngSpan * PAD;

  const toXY = useCallback(
    (lat: number, lng: number) => {
      const x = ((lng - minLng + padLng) / (lngSpan + 2 * padLng)) * size.w;
      const y = (1 - (lat - minLat + padLat) / (latSpan + 2 * padLat)) * size.h;
      return { x, y };
    },
    [minLat, minLng, latSpan, lngSpan, padLat, padLng, size],
  );

  const GRID = 40;
  const cols = Math.ceil(size.w / GRID);
  const rows = Math.ceil(size.h / GRID);

  return (
    <div
      ref={containerRef}
      className={cn(
        "relative overflow-hidden rounded-2xl border border-glass-border bg-[#070d1a]",
        className,
      )}
    >
      {/* dot-grid background */}
      <svg
        className="absolute inset-0 w-full h-full pointer-events-none"
        aria-hidden
      >
        {Array.from({ length: rows + 1 }, (_, r) =>
          Array.from({ length: cols + 1 }, (_, c) => (
            <circle
              key={`${r}-${c}`}
              cx={c * GRID}
              cy={r * GRID}
              r={1}
              fill="rgba(0,229,255,0.08)"
            />
          )),
        )}
        {/* subtle grid lines */}
        {Array.from({ length: cols + 1 }, (_, c) => (
          <line
            key={`v${c}`}
            x1={c * GRID} y1={0} x2={c * GRID} y2={size.h}
            stroke="rgba(0,229,255,0.03)" strokeWidth={1}
          />
        ))}
        {Array.from({ length: rows + 1 }, (_, r) => (
          <line
            key={`h${r}`}
            x1={0} y1={r * GRID} x2={size.w} y2={r * GRID}
            stroke="rgba(0,229,255,0.03)" strokeWidth={1}
          />
        ))}
      </svg>

      {/* driver pins */}
      {drivers.map((driver) => {
        const { x, y } = toXY(driver.lat, driver.lng);
        const isHov = hovered === driver.driver_id;
        const col = statusColor[driver.status];
        return (
          <div
            key={driver.driver_id}
            className="absolute -translate-x-1/2 -translate-y-1/2 cursor-pointer z-10"
            style={{ left: x, top: y }}
            onClick={() => onDriverClick?.(driver)}
            onMouseEnter={() => setHovered(driver.driver_id)}
            onMouseLeave={() => setHovered(null)}
          >
            {/* beacon pulse for active statuses */}
            {driver.status !== "offline" && driver.status !== "on_break" && (
              <span
                className="absolute inset-0 rounded-full animate-ping opacity-40"
                style={{ background: col }}
              />
            )}
            <motion.span
              animate={{ scale: isHov ? 1.4 : 1 }}
              className="relative flex h-3.5 w-3.5 rounded-full border-2 border-[#070d1a]"
              style={{ background: col, boxShadow: `0 0 8px ${col}88` }}
            />
            {driver.deliveries_remaining > 0 && (
              <span
                className="absolute -top-2 -right-2 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-[#070d1a] border border-white/20 text-[8px] font-mono text-white/70"
              >
                {driver.deliveries_remaining}
              </span>
            )}
            {/* tooltip */}
            {isHov && (
              <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 z-20 whitespace-nowrap rounded-md border border-white/10 bg-[#0c1628]/90 backdrop-blur px-2 py-1 text-[10px] font-mono text-white/80 pointer-events-none">
                <p className="font-semibold text-white">{driver.driver_name}</p>
                <p style={{ color: col }}>{statusLabel[driver.status]}</p>
                <p className="text-white/40">{driver.lat.toFixed(4)}, {driver.lng.toFixed(4)}</p>
              </div>
            )}
          </div>
        );
      })}

      {/* empty state */}
      {drivers.length === 0 && (
        <div className="absolute inset-0 flex items-center justify-center">
          <p className="text-xs font-mono text-white/20">No drivers online</p>
        </div>
      )}

      {/* notice badge when tiles unavailable */}
      {notice && (
        <div className="absolute bottom-3 left-1/2 -translate-x-1/2 rounded-full border border-white/10 bg-[#070d1a]/80 backdrop-blur px-3 py-1 text-[9px] font-mono text-white/30">
          {notice}
        </div>
      )}

      {/* legend */}
      <div className="absolute top-3 right-3 flex flex-col gap-1 rounded-lg border border-white/10 bg-[#070d1a]/80 backdrop-blur px-2 py-1.5">
        {(["available", "en_route", "delivering", "on_break"] as const).map((s) => (
          <div key={s} className="flex items-center gap-1.5">
            <span className="h-2 w-2 rounded-full flex-shrink-0" style={{ background: statusColor[s] }} />
            <span className="text-[9px] font-mono text-white/40">{statusLabel[s]}</span>
          </div>
        ))}
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
  const [Map, setMap]         = useState<any>(null);
  const [libs, setLibs]       = useState<any>(null);
  const [tileError, setTileError] = useState(false);
  const mapRef                = useRef<any>(null);

  useEffect(() => {
    Promise.all([
      import("react-map-gl"),
      import("mapbox-gl/dist/mapbox-gl.css" as any),
    ]).then(([mapgl]) => {
      setMap(() => mapgl.default);
      setLibs(mapgl);
    }).catch(() => setTileError(true));
  }, []);

  const MAPBOX_TOKEN = process.env.NEXT_PUBLIC_MAPBOX_TOKEN ?? "";

  useEffect(() => {
    if (!drivers.length || !mapRef.current) return;
    const lngs = drivers.map((d) => d.lng);
    const lats  = drivers.map((d) => d.lat);
    mapRef.current.fitBounds(
      [[Math.min(...lngs) - 0.02, Math.min(...lats) - 0.02],
       [Math.max(...lngs) + 0.02, Math.max(...lats) + 0.02]],
      { padding: 60, duration: 1200 },
    );
  }, [drivers]);

  // Fall back to canvas map if library load or tile fetch fails
  if (tileError) {
    return (
      <CanvasFallbackMap
        drivers={drivers}
        onDriverClick={onDriverClick}
        className={className}
        notice="Map tiles unavailable — showing GPS plot"
      />
    );
  }

  if (!Map || !libs) {
    return (
      <CanvasFallbackMap
        drivers={drivers}
        onDriverClick={onDriverClick}
        className={className}
      />
    );
  }

  const { Marker, Source, Layer } = libs;

  return (
    <div
      className={cn("relative rounded-2xl overflow-hidden border border-glass-border", className)}
      style={{ filter: "brightness(1.9) contrast(1.1) saturate(1.2)" }}
    >
      <Map
        ref={mapRef}
        mapboxAccessToken={MAPBOX_TOKEN}
        mapStyle="mapbox://styles/mapbox/navigation-night-v1"
        style={{ width: "100%", height: "100%" }}
        initialViewState={{ longitude: 121.774, latitude: 12.879, zoom: 6 }}
        attributionControl={false}
        onError={() => setTileError(true)}
      >
        {routes.map((route) => (
          <Source key={route.driver_id} type="geojson" data={route.geojson}>
            <Layer
              id={`route-${route.driver_id}`}
              type="line"
              paint={{
                "line-color": route.color,
                "line-width": 2,
                "line-opacity": 0.7,
                "line-dasharray": [2, 1],
              }}
            />
          </Source>
        ))}
        {drivers.map((driver) => (
          <Marker
            key={driver.driver_id}
            longitude={driver.lng}
            latitude={driver.lat}
            anchor="center"
            onClick={() => { onDriverClick?.(driver); }}
          >
            <motion.div
              initial={{ scale: 0 }}
              animate={{ scale: 1 }}
              whileHover={{ scale: 1.2 }}
              className="relative cursor-pointer"
            >
              {driver.status !== "offline" && driver.status !== "on_break" && (
                <span
                  className="absolute inset-0 rounded-full animate-beacon"
                  style={{ background: statusColor[driver.status] }}
                />
              )}
              <span
                className="relative flex h-4 w-4 rounded-full border-2 border-canvas"
                style={{ background: statusColor[driver.status] }}
              />
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

  // No token at all — skip Mapbox entirely, go straight to canvas fallback
  if (!MAPBOX_TOKEN) {
    return (
      <CanvasFallbackMap
        drivers={drivers}
        onDriverClick={onDriverClick}
        className={className}
        notice="Mapbox token not configured — showing GPS plot"
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
