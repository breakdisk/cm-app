import type { SlaRecord, ZoneSlaRow } from "@/lib/api/carriers";

/**
 * A partner's SLA numbers, derived from the partner's OWN carrier data.
 *
 * These used to come from the tenant-wide analytics endpoints, which count
 * every carrier's and merchant's shipments and are not a partner's to read.
 * `GET /v1/carriers/:id/sla-summary` and `/sla-history` are scoped to the
 * caller's carrier, so everything below is the partner's own.
 */

export interface TrendPoint {
  /** Calendar day, `YYYY-MM-DD` (UTC). */
  date: string;
  /** On-time share of that day's decided shipments, 0–100, one decimal. */
  rate: number;
}

/**
 * Daily on-time rate from a carrier's delivery history.
 *
 * Only shipments with a verdict count (`on_time` is true or false); one still
 * in transit has none yet and is left out, so it cannot drag a day down. A day
 * with no decided shipments is omitted rather than drawn as 0% or 100%. The
 * day is when the verdict was reached (`delivered_at`), falling back to when
 * the record was created for a failure that never got a delivery time.
 */
export function dailyOnTimeRate(records: SlaRecord[]): TrendPoint[] {
  const days = new Map<string, { onTime: number; decided: number }>();
  for (const r of records) {
    if (r.on_time === null) continue;
    const when = r.delivered_at ?? r.created_at;
    const day = when.slice(0, 10);
    const d = days.get(day) ?? { onTime: 0, decided: 0 };
    d.decided += 1;
    if (r.on_time) d.onTime += 1;
    days.set(day, d);
  }
  return [...days.entries()]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([date, d]) => ({ date, rate: Math.round((d.onTime / d.decided) * 1000) / 10 }));
}

export interface SlaTotals {
  /** Every shipment tracked in the window, including those still in transit. */
  total: number;
  onTime: number;
  failed: number;
  /** On-time share of decided shipments, 0–100. `null` when none are decided. */
  ratePct: number | null;
}

/** Roll the per-zone summary up to one set of figures. */
export function totalsFromZones(zones: ZoneSlaRow[]): SlaTotals {
  let total = 0;
  let onTime = 0;
  let failed = 0;
  for (const z of zones) {
    total += z.total;
    onTime += z.on_time;
    failed += z.failed;
  }
  const decided = onTime + failed;
  return { total, onTime, failed, ratePct: decided > 0 ? (onTime / decided) * 100 : null };
}
