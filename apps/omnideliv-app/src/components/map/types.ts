/**
 * The map seam's contract.
 *
 * One interface, so a tile-backed implementation can replace the canvas plot
 * without the data path changing. This mirrors the admin portal, which renders
 * Mapbox when a token is configured and a canvas GPS plot when it is not — the
 * difference being that here the fallback ships first and the tiles come later.
 *
 * Alone in a leaf module on purpose: both the chooser and every implementation
 * import it, and nothing here imports either of them.
 */
import type { CourierFix, LatLng, StopView } from "@/api/tracking";

export interface MapSurfaceProps {
  /** Null when there is no fresh fix. Implementations must draw nothing. */
  courier: CourierFix | null;
  stops: StopView[];
  destination: LatLng | null;
}
