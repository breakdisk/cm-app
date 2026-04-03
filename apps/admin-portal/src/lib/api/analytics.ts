import { createApiClient, ApiResponse } from "./client";

export interface DeliveryMetrics {
  shipments_today: number;
  shipments_today_trend: number;
  delivery_rate: number;
  delivery_rate_trend: number;
  avg_delivery_days: number;
  avg_delivery_days_trend: number;
  revenue_mtd: number;
  revenue_mtd_trend: number;
}

export interface DailyVolume {
  day: string;       // "Mon", "Tue", etc.
  delivered: number;
  failed: number;
}

export interface SlaBreakdown {
  name: string;      // "On Time", "Slightly Late", etc.
  value: number;     // percentage
  fill: string;      // hex colour
}

export interface ZonePerformance {
  zone: string;
  deliveries: number;
  success_rate: number;
}

export interface AnalyticsDashboard {
  metrics: DeliveryMetrics;
  weekly_volume: DailyVolume[];
  sla_breakdown: SlaBreakdown[];
  zone_performance: ZonePerformance[];
}

export function createAnalyticsApi(token: string) {
  const client = createApiClient(token);

  return {
    getDashboard: (params?: { from?: string; to?: string }) =>
      client
        .get<ApiResponse<AnalyticsDashboard>>("/v1/analytics/dashboard", { params })
        .then((r) => r.data),
  };
}
