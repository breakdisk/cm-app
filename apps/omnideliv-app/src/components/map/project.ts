/**
 * Lat/lng to screen coordinates inside a padded box.
 *
 * An equirectangular fit to the bounding box of the points, not a real map
 * projection — over a city-sized extent the distortion is invisible, and the
 * alternative pulls in a projection library for a picture with four dots on it.
 *
 * Pure and separate from the component so the geometry is testable without
 * rendering a tree.
 */
export interface LatLngLike {
  lat: number;
  lng: number;
}

export interface Box {
  width: number;
  height: number;
  padding: number;
}

export interface Point {
  x: number;
  y: number;
}

export function projectAll(points: LatLngLike[], box: Box): Point[] {
  if (points.length === 0) return [];

  const lats = points.map((p) => p.lat);
  const lngs = points.map((p) => p.lng);
  const minLat = Math.min(...lats);
  const maxLat = Math.max(...lats);
  const minLng = Math.min(...lngs);
  const maxLng = Math.max(...lngs);

  const usableW = box.width - box.padding * 2;
  const usableH = box.height - box.padding * 2;

  // A single point, or several at the same place, has zero span in one or both
  // axes. Scaling by that is a division by zero and renders NaN — which in
  // React Native is a silently invisible view, not an error anyone sees.
  const spanLat = maxLat - minLat;
  const spanLng = maxLng - minLng;

  return points.map((p) => ({
    x:
      spanLng === 0
        ? box.width / 2
        : box.padding + ((p.lng - minLng) / spanLng) * usableW,
    // Inverted: screen y grows downwards, latitude grows northwards.
    y:
      spanLat === 0
        ? box.height / 2
        : box.padding + ((maxLat - p.lat) / spanLat) * usableH,
  }));
}

/**
 * Which markers to draw, given what is present. Pure, so it is testable
 * without `@testing-library/react-native` — deliberately not a dependency of
 * this app (see CanvasPlot.tsx) — and without rendering a component tree.
 *
 * Mirrors the point order CanvasPlot builds: stops, then destination, then
 * (if present) the courier.
 */
export function markerIndices(stopCount: number, hasCourier: boolean) {
  return {
    stopCount,
    destIndex: stopCount,
    courierIndex: hasCourier ? stopCount + 1 : null,
  };
}
