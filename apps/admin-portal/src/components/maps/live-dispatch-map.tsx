"use client";
import { useRef, useEffect, useState } from "react";
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
  on_break:   "#6B7280",   // neutral gray
  offline:    "#374151",   // dark gray
};

const statusLabel: Record<DriverPin["status"], string> = {
  available:  "Available",
  en_route:   "En Route",
  delivering: "Delivering",
  returning:  "Returning",
  on_break:   "On Break",
  offline:    "Offline",
};

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
      <div className={cn("flex items-center justify-center h-full bg-[#050810] rounded-2xl border border-glass-border", className)}>
        <div className="glass-card p-6 text-center text-muted-foreground text-sm">
          Mapbox token not configured
        </div>
      </div>
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
