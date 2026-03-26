"use client";
import { useRef, useCallback, useEffect, useState } from "react";
import Map, {
  Marker,
  Source,
  Layer,
  type MapRef,
  type LayerProps,
} from "react-map-gl";
import { motion, AnimatePresence } from "framer-motion";
import { cn } from "@/lib/design-system/cn";
import { colors } from "@/lib/design-system/tokens";
import "mapbox-gl/dist/mapbox-gl.css";

const MAPBOX_TOKEN = process.env.NEXT_PUBLIC_MAPBOX_TOKEN!;

// ── Dark neon Mapbox style ────────────────────────────────────
const MAP_STYLE = "mapbox://styles/mapbox/dark-v11";

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
  drivers: DriverPin[];
  routes?: RouteGeoJson[];
  onDriverClick?: (driver: DriverPin) => void;
  className?: string;
}

const statusToColor: Record<DriverPin["status"], string> = {
  idle:       colors.amber.signal,
  en_route:   colors.cyan.neon,
  delivering: colors.green.signal,
  returning:  colors.purple.plasma,
};

const routeLayer = (color: string): LayerProps => ({
  id:   "route",
  type: "line",
  paint: {
    "line-color": color,
    "line-width": 2,
    "line-opacity": 0.7,
    "line-dasharray": [2, 1],
  },
});

export function LiveDispatchMap({
  drivers,
  routes = [],
  onDriverClick,
  className,
}: LiveDispatchMapProps) {
  const mapRef = useRef<MapRef>(null);
  const [selected, setSelected] = useState<string | null>(null);

  // Fit map bounds to show all drivers
  useEffect(() => {
    if (!drivers.length || !mapRef.current) return;
    const lngs = drivers.map((d) => d.lng);
    const lats = drivers.map((d) => d.lat);
    mapRef.current.fitBounds(
      [[Math.min(...lngs) - 0.02, Math.min(...lats) - 0.02],
       [Math.max(...lngs) + 0.02, Math.max(...lats) + 0.02]],
      { padding: 60, duration: 1200 }
    );
  }, [drivers]);

  const handleDriverClick = useCallback(
    (driver: DriverPin) => {
      setSelected(driver.driver_id);
      onDriverClick?.(driver);
      mapRef.current?.flyTo({
        center: [driver.lng, driver.lat],
        zoom: 15,
        duration: 900,
      });
    },
    [onDriverClick]
  );

  return (
    <div className={cn("relative rounded-2xl overflow-hidden border border-glass-border", className)}>
      {/* Scan line overlay on the map */}
      <div className="scan-overlay absolute inset-0 z-10 pointer-events-none" />

      <Map
        ref={mapRef}
        mapboxAccessToken={MAPBOX_TOKEN}
        mapStyle={MAP_STYLE}
        style={{ width: "100%", height: "100%" }}
        initialViewState={{ longitude: 121.774, latitude: 12.879, zoom: 6 }}
        attributionControl={false}
      >
        {/* Route lines */}
        {routes.map((route) => (
          <Source key={route.driver_id} type="geojson" data={route.geojson}>
            <Layer {...routeLayer(route.color)} id={`route-${route.driver_id}`} />
          </Source>
        ))}

        {/* Driver markers */}
        {drivers.map((driver) => (
          <Marker
            key={driver.driver_id}
            longitude={driver.lng}
            latitude={driver.lat}
            anchor="center"
            onClick={() => handleDriverClick(driver)}
          >
            <motion.div
              initial={{ scale: 0 }}
              animate={{ scale: 1 }}
              whileHover={{ scale: 1.2 }}
              className="relative cursor-pointer"
            >
              {/* Beacon ring */}
              <span
                className="absolute inset-0 rounded-full animate-beacon"
                style={{ background: statusToColor[driver.status] }}
              />
              {/* Driver dot */}
              <span
                className="relative flex h-4 w-4 rounded-full border-2 border-canvas items-center justify-center"
                style={{ background: statusToColor[driver.status] }}
              />
              {/* Delivery count badge */}
              {driver.deliveries_remaining > 0 && (
                <span className="absolute -top-2.5 -right-2.5 flex h-4 w-4 items-center justify-center rounded-full bg-canvas border border-glass-border text-2xs font-mono text-white/70">
                  {driver.deliveries_remaining}
                </span>
              )}
            </motion.div>
          </Marker>
        ))}
      </Map>

      {/* Legend */}
      <div className="absolute bottom-4 left-4 z-20 glass-sm p-3 flex flex-col gap-1.5">
        {Object.entries(statusToColor).map(([status, color]) => (
          <div key={status} className="flex items-center gap-2">
            <span className="h-2 w-2 rounded-full flex-shrink-0" style={{ background: color, boxShadow: `0 0 6px ${color}` }} />
            <span className="text-2xs font-mono text-white/50 capitalize">{status.replace("_", " ")}</span>
          </div>
        ))}
      </div>

      {/* Live label */}
      <div className="absolute top-4 right-4 z-20">
        <span className="inline-flex items-center gap-1.5 glass-sm px-2.5 py-1">
          <span className="relative flex h-1.5 w-1.5">
            <span className="absolute inline-flex h-full w-full rounded-full bg-green-signal opacity-75 animate-beacon" />
            <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-green-signal" />
          </span>
          <span className="text-2xs font-mono text-green-signal uppercase tracking-widest">Live</span>
        </span>
      </div>
    </div>
  );
}
