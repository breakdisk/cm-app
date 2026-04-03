use std::sync::Arc;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::domain::{entities::TrackingRecord, repositories::TrackingRepository};

pub struct TrackingService {
    repo: Arc<dyn TrackingRepository>,
}

impl TrackingService {
    pub fn new(repo: Arc<dyn TrackingRepository>) -> Self {
        Self { repo }
    }

    /// Public lookup — no auth; used for customer-facing tracking page.
    /// Returns only fields safe for public display (no driver phone, no tenant internals).
    pub async fn get_public(&self, tracking_number: &str) -> AppResult<TrackingRecord> {
        self.repo
            .find_by_tracking_number(tracking_number)
            .await
            .map_err(|e| AppError::Internal(e))?
            .ok_or_else(|| AppError::NotFound {
                resource: "tracking",
                id: tracking_number.to_owned(),
            })
    }

    /// Authenticated merchant lookup by shipment id.
    pub async fn get_by_shipment_id(&self, shipment_id: Uuid) -> AppResult<TrackingRecord> {
        self.repo
            .find_by_shipment_id(shipment_id)
            .await
            .map_err(|e| AppError::Internal(e))?
            .ok_or_else(|| AppError::NotFound {
                resource: "shipment",
                id: shipment_id.to_string(),
            })
    }

    /// List shipments for a tenant (authenticated, paginated).
    pub async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> AppResult<Vec<TrackingRecord>> {
        let limit  = limit.clamp(1, 200);
        let offset = offset.max(0);
        self.repo
            .list_by_tenant(tenant_id, limit, offset)
            .await
            .map_err(|e| AppError::Internal(e))
    }

    /// Customer-initiated reschedule — verifies the tracking number exists, validates inputs, then delegates.
    pub async fn reschedule(
        &self,
        tracking_number: &str,
        preferred_date: chrono::NaiveDate,
        preferred_time_slot: Option<&str>,
        reason: &str,
    ) -> AppResult<()> {
        // Verify the tracking record exists first.
        self.repo
            .find_by_tracking_number(tracking_number)
            .await
            .map_err(|e| AppError::Internal(e))?
            .ok_or_else(|| AppError::NotFound {
                resource: "tracking",
                id: tracking_number.to_owned(),
            })?;

        // Validate preferred_date: must be >= today (UTC).
        let today = chrono::Utc::now().date_naive();
        if preferred_date < today {
            return Err(AppError::Validation(
                "Preferred date must be in the future".to_owned(),
            ));
        }

        // Validate preferred_date: must be <= today + 90 days.
        let max_date = today + chrono::Duration::days(90);
        if preferred_date > max_date {
            return Err(AppError::Validation(
                "Preferred date cannot be more than 90 days in the future".to_owned(),
            ));
        }

        // Validate reason length if provided.
        if reason.len() > 500 {
            return Err(AppError::Validation(
                "Reason cannot exceed 500 characters".to_owned(),
            ));
        }

        self.repo
            .reschedule(tracking_number, preferred_date, preferred_time_slot, reason)
            .await
            .map_err(|e| AppError::Internal(e))
    }

    pub async fn submit_feedback(
        &self,
        tracking_number: &str,
        rating: i16,
        tags: Vec<String>,
        comments: Option<String>,
    ) -> AppResult<()> {
        // Validate rating
        if !(1..=5).contains(&rating) {
            return Err(AppError::Validation("Rating must be between 1 and 5".to_owned()));
        }
        self.repo
            .save_feedback(tracking_number, rating, &tags, comments.as_deref())
            .await
            .map_err(|e| AppError::Internal(e))
    }
}
