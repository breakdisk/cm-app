use std::sync::Arc;
use chrono::NaiveDate;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::domain::entities::{DailyBucket, DeliveryKpis, DriverPerformance};
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
}
