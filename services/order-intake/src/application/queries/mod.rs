use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::ShipmentId;

use crate::{
    application::services::shipment_service::{ShipmentListFilter, ShipmentRepository},
    domain::entities::shipment::Shipment,
};

#[derive(Debug, Deserialize)]
pub struct ListShipmentsQuery {
    pub status:      Option<String>,
    pub merchant_id: Option<Uuid>,
    pub page:        Option<i64>,
    pub per_page:    Option<i64>,
}

pub struct ShipmentQueryService {
    repo: Arc<dyn ShipmentRepository>,
}

impl ShipmentQueryService {
    pub fn new(repo: Arc<dyn ShipmentRepository>) -> Self {
        Self { repo }
    }

    pub async fn get_by_id(&self, id: Uuid) -> AppResult<Shipment> {
        self.repo
            .find_by_id(&ShipmentId::from_uuid(id))
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Shipment", id: id.to_string() })
    }

    pub async fn list(
        &self,
        tenant_id: Uuid,
        q: ListShipmentsQuery,
    ) -> AppResult<(Vec<Shipment>, i64)> {
        let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
        let page     = q.page.unwrap_or(1).max(1);
        let filter = ShipmentListFilter {
            tenant_id,
            merchant_id: q.merchant_id,
            status: q.status,
            limit:  per_page,
            offset: (page - 1) * per_page,
        };
        self.repo.list(&filter).await.map_err(AppError::Internal)
    }
}
