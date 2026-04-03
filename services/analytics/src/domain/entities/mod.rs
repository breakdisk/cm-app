use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// KPI snapshot for a tenant over a date range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryKpis {
    pub tenant_id:             Uuid,
    pub from:                  NaiveDate,
    pub to:                    NaiveDate,

    pub total_shipments:       i64,
    pub delivered:             i64,
    pub failed:                i64,
    pub cancelled:             i64,

    /// Delivered / (Delivered + Failed), expressed as 0.0 – 100.0
    pub delivery_success_rate: f64,

    /// Deliveries completed before ETA / deliveries with ETA, 0.0 – 100.0
    pub on_time_rate:          f64,

    /// Average delivery time for completed shipments, in hours.
    pub avg_delivery_hours:    f64,

    /// COD metrics
    pub cod_shipments:         i64,
    pub cod_collected_cents:   i64,
    pub cod_collection_rate:   f64,   // collected / total_cod_shipments

    pub computed_at:           DateTime<Utc>,
}

/// Daily bucketed timeseries row — for chart rendering on the merchant dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBucket {
    pub date:              NaiveDate,
    pub shipments:         i64,
    pub delivered:         i64,
    pub failed:            i64,
    pub success_rate:      f64,
    pub cod_collected_cents: i64,
}

/// Driver performance summary — leaderboard for operations team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverPerformance {
    pub driver_id:         Uuid,
    pub driver_name:       Option<String>,
    pub total_deliveries:  i64,
    pub successful:        i64,
    pub failed:            i64,
    pub success_rate:      f64,
    pub avg_delivery_hours: f64,
    pub cod_collected_cents: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DashboardMetrics {
    pub shipments_today:         i64,
    pub shipments_today_trend:   f64,
    pub delivery_rate:           f64,
    pub delivery_rate_trend:     f64,
    pub avg_delivery_days:       f64,
    pub avg_delivery_days_trend: f64,
    pub revenue_mtd:             i64,
    pub revenue_mtd_trend:       f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WeeklyVolumeDay {
    pub day:       String,
    pub delivered: i64,
    pub failed:    i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SlaBreakdown {
    pub name:  String,
    pub value: f64,
    pub fill:  String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ZonePerformance {
    pub zone:         String,
    pub deliveries:   i64,
    pub success_rate: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DashboardData {
    pub metrics:          DashboardMetrics,
    pub weekly_volume:    Vec<WeeklyVolumeDay>,
    pub sla_breakdown:    Vec<SlaBreakdown>,
    pub zone_performance: Vec<ZonePerformance>,
}

/// Aggregate event row stored in the analytics schema (append-only).
/// Written by Kafka handlers on every relevant event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipmentEvent {
    pub id:              Uuid,
    pub tenant_id:       Uuid,
    pub shipment_id:     Uuid,
    pub event_type:      String,   // "created" | "delivered" | "failed" | "cancelled"
    pub driver_id:       Option<Uuid>,
    pub service_type:    Option<String>,
    pub cod_amount_cents: Option<i64>,
    pub on_time:         Option<bool>,
    pub delivery_hours:  Option<f64>,
    pub occurred_at:     DateTime<Utc>,
}
