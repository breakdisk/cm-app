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
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound(format!("Tracking number '{}' not found", tracking_number)))
    }

    /// Authenticated merchant lookup by shipment id.
    pub async fn get_by_shipment_id(&self, shipment_id: Uuid) -> AppResult<TrackingRecord> {
        self.repo
            .find_by_shipment_id(shipment_id)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound("Shipment not found".into()))
    }

    /// List shipments for a tenant (authenticated, paginated).
    pub async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> AppResult<Vec<TrackingRecord>> {
        let limit  = limit.clamp(1, 200);
        let offset = offset.max(0);
        self.repo
            .list_by_tenant(tenant_id, limit, offset)
            .await
            .map_err(AppError::internal)
    }
}
