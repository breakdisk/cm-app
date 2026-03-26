import { createApiClient, ApiResponse } from "./client";

export interface DeliveryMetrics {
  total_shipments: number;
  delivered: number;
  failed: number;
  in_transit: number;
  delivery_rate_pct: number;
  on_time_rate_pct: number;
  avg_delivery_hours: number;
  cod_collected_php: number;
  nps_score?: number;
}

export interface TimeSeriesPoint {
  timestamp: string;
  value: number;
}

export interface ZoneDemandForecast {
  zone_id: string;
  zone_name: string;
  forecast: Array<{
    date: string;
    predicted_volume: number;
    confidence: number;
  }>;
  generated_at: string;
}

export interface DriverPerformanceMetric {
  driver_id: string;
  driver_name: string;
  deliveries_completed: number;
  delivery_rate_pct: number;
  on_time_rate_pct: number;
  avg_stops_per_day: number;
  pod_compliance_pct: number;
}

export interface MerchantVolumeMetric {
  merchant_id: string;
  merchant_name: string;
  total_shipments: number;
  delivered: number;
  failed: number;
  cod_amount_php: number;
}

export interface AnalyticsQueryParams {
  from: string;
  to: string;
  zone_id?: string;
  merchant_id?: string;
  granularity?: "hour" | "day" | "week" | "month";
}

export const analyticsApi = {
  /** Get delivery KPIs for a date range */
  getDeliveryMetrics: (params: AnalyticsQueryParams, token: string) =>
    createApiClient(token)
      .get<ApiResponse<DeliveryMetrics>>("/v1/analytics/delivery-metrics", {
        params,
      })
      .then((r) => r.data.data),

  /** Get delivery volume time series for charting */
  getVolumeTimeSeries: (params: AnalyticsQueryParams, token: string) =>
    createApiClient(token)
      .get<ApiResponse<TimeSeriesPoint[]>>(
        "/v1/analytics/volume-time-series",
        { params }
      )
      .then((r) => r.data.data),

  /** Get zone demand forecast (AI-generated) */
  getZoneDemandForecast: (
    zoneId: string,
    days: number,
    token: string
  ) =>
    createApiClient(token)
      .get<ApiResponse<ZoneDemandForecast>>(
        `/v1/analytics/zones/${zoneId}/demand-forecast`,
        { params: { days } }
      )
      .then((r) => r.data.data),

  /** Get driver performance leaderboard */
  getDriverPerformance: (
    params: Pick<AnalyticsQueryParams, "from" | "to"> & {
      limit?: number;
      zone_id?: string;
    },
    token: string
  ) =>
    createApiClient(token)
      .get<ApiResponse<DriverPerformanceMetric[]>>(
        "/v1/analytics/driver-performance",
        { params }
      )
      .then((r) => r.data.data),

  /** Get top merchants by shipment volume */
  getMerchantVolume: (
    params: Pick<AnalyticsQueryParams, "from" | "to"> & { limit?: number },
    token: string
  ) =>
    createApiClient(token)
      .get<ApiResponse<MerchantVolumeMetric[]>>(
        "/v1/analytics/merchant-volume",
        { params }
      )
      .then((r) => r.data.data),

  /** Export analytics report as CSV */
  exportReport: (
    reportType: "delivery_summary" | "driver_performance" | "zone_heatmap",
    params: AnalyticsQueryParams,
    token: string
  ) =>
    createApiClient(token)
      .get<Blob>(`/v1/analytics/export/${reportType}`, {
        params,
        responseType: "blob",
      })
      .then((r) => r.data),
};
