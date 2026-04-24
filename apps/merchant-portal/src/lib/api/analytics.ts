import { createApiClient } from "./client";

// ── Types ──────────────────────────────────────────────────────────────────────
// Mirrors services/analytics/src/domain/entities/mod.rs.

export interface DashboardMetrics {
  shipments_today: number;
  shipments_today_trend: number;
  delivery_rate: number;
  delivery_rate_trend: number;
  avg_delivery_days: number;
  avg_delivery_days_trend: number;
  revenue_mtd: number;          // cents
  revenue_mtd_trend: number;
}

export interface WeeklyVolumeDay {
  day: string;
  delivered: number;
  failed: number;
}

export interface SlaBreakdown {
  name: string;
  value: number;
  fill: string;
}

export interface ZonePerformance {
  zone: string;
  deliveries: number;
  success_rate: number;
}

export interface DashboardData {
  metrics: DashboardMetrics;
  weekly_volume: WeeklyVolumeDay[];
  sla_breakdown: SlaBreakdown[];
  zone_performance: ZonePerformance[];
}

export interface DeliveryKpis {
  tenant_id: string;
  from: string;
  to: string;
  total_shipments: number;
  delivered: number;
  failed: number;
  cancelled: number;
  delivery_success_rate: number;
  on_time_rate: number;
  avg_delivery_hours: number;
  cod_shipments: number;
  cod_collected_cents: number;
  cod_collection_rate: number;
  computed_at: string;
}

export interface DailyBucket {
  date: string;
  shipments: number;
  delivered: number;
  failed: number;
  success_rate: number;
  cod_collected_cents: number;
}

export interface TimeseriesResponse {
  data: DailyBucket[];
  count: number;
}

// ── Client ─────────────────────────────────────────────────────────────────────

export const analyticsApi = {
  async dashboard(): Promise<DashboardData> {
    const { data } = await createApiClient().get<{ data: DashboardData }>("/v1/analytics/dashboard");
    return data.data;
  },

  async kpis(from: string, to: string): Promise<DeliveryKpis> {
    const { data } = await createApiClient().get<DeliveryKpis>("/v1/analytics/kpis", {
      params: { from, to },
    });
    return data;
  },

  async timeseries(from: string, to: string): Promise<DailyBucket[]> {
    const { data } = await createApiClient().get<TimeseriesResponse>("/v1/analytics/timeseries", {
      params: { from, to },
    });
    return data.data ?? [];
  },
};

// ── Helpers ────────────────────────────────────────────────────────────────────

/** ISO date N days before today, yyyy-mm-dd. */
export function daysAgo(n: number): string {
  const d = new Date();
  d.setDate(d.getDate() - n);
  return d.toISOString().slice(0, 10);
}

/** ISO date today, yyyy-mm-dd. */
export function today(): string {
  return new Date().toISOString().slice(0, 10);
}
