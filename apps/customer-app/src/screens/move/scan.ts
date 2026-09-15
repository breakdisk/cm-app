/**
 * Item scan (Move app, the plan screen).
 *
 * The AR module measures one item's box — three edges, in centimetres. The plan
 * needs a single L × W × H for the whole load, because the server bills the
 * greater of scale weight and volumetric weight (L × W × H / 5000). Only the
 * volume reaches that price, so the load's box is built to hold the scanned
 * volume rather than to describe a real crate.
 */
import type { BoxDimensions } from '../../../modules/ar-measurement/src';

export interface ItemDims {
  lengthCm: number;
  widthCm: number;
  heightCm: number;
}

/**
 * Whole centimetres, never below 1 (quote and create take integers), longest
 * edge first — so "length" is always the longest side, as the design labels it.
 */
export function dimsFromScan(box: Pick<BoxDimensions, 'length' | 'width' | 'height'>): ItemDims {
  const [lengthCm, widthCm, heightCm] = [box.length, box.width, box.height]
    .map((v) => Math.max(1, Math.round(Number.isFinite(v) ? v : 1)))
    .sort((a, b) => b - a);
  return { lengthCm, widthCm, heightCm };
}

export function volumeCm3(d: ItemDims): number {
  return d.lengthCm * d.widthCm * d.heightCm;
}

export function formatDims(d: ItemDims): string {
  return `${d.lengthCm} × ${d.widthCm} × ${d.heightCm} cm`;
}

export function formatVolume(cm3: number): string {
  const m3 = cm3 / 1_000_000;
  return `${m3 >= 10 ? m3.toFixed(1) : m3.toFixed(2)} m³`;
}

/**
 * One box for the load that holds every scanned item times its quantity: the
 * longest length, the widest width, and whatever height makes up the volume,
 * rounded to the nearest centimetre (so it is off by at most half a
 * length × width slice either way). Null when nothing has been scanned.
 */
export function combineLoad(items: ReadonlyArray<{ qty: number; dims?: ItemDims }>): ItemDims | null {
  const scanned = items.filter((i) => i.dims && i.qty > 0) as ReadonlyArray<{ qty: number; dims: ItemDims }>;
  if (scanned.length === 0) return null;
  const total = scanned.reduce((sum, i) => sum + volumeCm3(i.dims) * i.qty, 0);
  const lengthCm = Math.max(...scanned.map((i) => i.dims.lengthCm));
  const widthCm = Math.max(...scanned.map((i) => i.dims.widthCm));
  const heightCm = Math.max(1, Math.round(total / (lengthCm * widthCm)));
  return { lengthCm, widthCm, heightCm };
}
