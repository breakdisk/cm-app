use std::sync::Arc;
use chrono::NaiveDate;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::domain::entities::{
    DailyBucket, DashboardData, DashboardMetrics, DeliveryKpis, DriverPerformance,
    SlaBreakdown, WeeklyVolumeDay, ZonePerformance,
};
use crate::infrastructure::db::AnalyticsDb;

pub struct QueryService {
    db: Arc<AnalyticsDb>,
}

impl QueryService {
    pub fn new(db: Arc<AnalyticsDb>) -> Self { Self { db } }

    /// Summary KPIs for a date range.
    pub async fn delivery_kpis(
        &self,
        tenant_id: &TenantId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> AppResult<DeliveryKpis> {
        if from > to {
            return Err(AppError::Validation("'from' must be before 'to'".into()));
        }
        let max_range = chrono::Duration::days(365);
        if to - from > max_range {
            return Err(AppError::Validation("Date range cannot exceed 365 days".into()));
        }
        self.db
            .delivery_kpis(tenant_id.inner(), from, to)
            .await
            .map_err(AppError::internal)
    }

    /// Day-by-day time series for charts.
    pub async fn daily_timeseries(
        &self,
        tenant_id: &TenantId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> AppResult<Vec<DailyBucket>> {
        if from > to {
            return Err(AppError::Validation("'from' must be before 'to'".into()));
        }
        self.db
            .daily_timeseries(tenant_id.inner(), from, to)
            .await
            .map_err(AppError::internal)
    }

    /// Driver performance leaderboard.
    pub async fn driver_performance(
        &self,
        tenant_id: &TenantId,
        from: NaiveDate,
        to: NaiveDate,
        limit: i64,
    ) -> AppResult<Vec<DriverPerformance>> {
        self.db
            .driver_performance(tenant_id.inner(), from, to, limit.clamp(1, 50))
            .await
            .map_err(AppError::internal)
    }

    /// Aggregated dashboard data: KPI metrics, weekly volume chart, SLA breakdown, zone performance.
    pub async fn dashboard(&self, tenant_id: &TenantId) -> AppResult<DashboardData> {
        use chrono::Datelike;

        let today     = chrono::Utc::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let month_start = today
            .with_day(1)
            .unwrap_or(today);
        let week_start = today - chrono::Duration::days(6);

        let kpi_today     = self.db
            .delivery_kpis(tenant_id.inner(), today, today)
            .await
            .map_err(|e| AppError::Internal(e))?;
        let kpi_yesterday = self.db
            .delivery_kpis(tenant_id.inner(), yesterday, yesterday)
            .await
            .map_err(|e| AppError::Internal(e))?;
        let kpi_mtd = self.db
            .delivery_kpis(tenant_id.inner(), month_start, today)
            .await
            .map_err(|e| AppError::Internal(e))?;

        let daily_buckets = self.db
            .daily_timeseries(tenant_id.inner(), week_start, today)
            .await
            .map_err(|e| AppError::Internal(e))?;

        let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let weekly_volume: Vec<WeeklyVolumeDay> = daily_buckets
            .iter()
            .map(|b| {
                let dow = b.date.weekday().num_days_from_sunday() as usize;
                WeeklyVolumeDay {
                    day:       day_names[dow].to_owned(),
                    delivered: b.delivered,
                    failed:    b.failed,
                }
            })
            .collect();

        let avg_delivery_days_today     = kpi_today.avg_delivery_hours / 24.0;
        let avg_delivery_days_yesterday = kpi_yesterday.avg_delivery_hours / 24.0;

        let metrics = DashboardMetrics {
            shipments_today:         kpi_today.total_shipments,
            shipments_today_trend:   pct_change(kpi_today.total_shipments as f64, kpi_yesterday.total_shipments as f64),
            delivery_rate:           kpi_today.delivery_success_rate,
            delivery_rate_trend:     pct_change(kpi_today.delivery_success_rate, kpi_yesterday.delivery_success_rate),
            avg_delivery_days:       avg_delivery_days_today,
            avg_delivery_days_trend: pct_change(avg_delivery_days_today, avg_delivery_days_yesterday),
            revenue_mtd:             kpi_mtd.cod_collected_cents,
            revenue_mtd_trend:       0.0, // prior-month comparison requires additional query; deferred
        };

        let sla_breakdown = vec![
            SlaBreakdown {
                name:  "On Time".to_owned(),
                value: kpi_today.delivery_success_rate,
                fill:  "#00FF88".to_owned(),
            },
            SlaBreakdown {
                name:  "Late".to_owned(),
                value: (100.0 - kpi_today.delivery_success_rate).max(0.0),
                fill:  "#FFAB00".to_owned(),
            },
            SlaBreakdown {
                name:  "Failed".to_owned(),
                value: 0.0,
                fill:  "#FF4444".to_owned(),
            },
        ];

        let zone_performance: Vec<ZonePerformance> = vec![];

        Ok(DashboardData {
            metrics,
            weekly_volume,
            sla_breakdown,
            zone_performance,
        })
    }
}

fn pct_change(new: f64, old: f64) -> f64 {
    if old == 0.0 { 0.0 } else { (new - old) / old * 100.0 }
}
