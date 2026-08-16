/**
 * The only map name a screen imports.
 *
 * Swapping in a tile-backed surface is a change to this file alone — the track
 * screen and the data path both stay as they are.
 */
import { CanvasPlot } from "./CanvasPlot";
import type { MapSurfaceProps } from "./types";

export type { MapSurfaceProps };

export function MapSurface(props: MapSurfaceProps) {
  return <CanvasPlot {...props} />;
}
