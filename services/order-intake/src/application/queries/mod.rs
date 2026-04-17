use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Pagination, PaginatedResponse, ShipmentId, TenantId};

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

/// Internal billing query — used by payments' BillingAggregationService to
/// pull the set of delivered shipments a merchant owes for in a period.
#[derive(Debug, Deserialize)]
pub struct BillingShipmentsQuery {
    pub tenant_id:   Uuid,
    pub merchant_id: Uuid,
    /// Inclusive — typically the first instant of the billing period.
    pub from:        DateTime<Utc>,
    /// Exclusive — typically the first instant of the following period.
    pub to:          DateTime<Utc>,
}

/// Billing-ready breakdown for a single delivered shipment.
/// Mirrors `payments::ShipmentBillingDto`; kept here so order-intake owns the shape.
#[derive(Debug, Serialize)]
pub struct BillingShipmentDto {
    pub shipment_id:          Uuid,
    pub awb:                  String,
    pub merchant_id:          Uuid,
    pub currency:             String,
    pub base_freight_cents:   i64,
    pub fuel_surcharge_cents: i64,
    pub insurance_cents:      i64,
    pub total_cents:          i64,
    pub delivered_at:         DateTime<Utc>,
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
            updated_from: None,
            updated_to:   None,
            limit:  per_page,
            offset: (page - 1) * per_page,
        };
        self.repo.list(&filter).await.map_err(AppError::Internal)
    }

    /// Internal use: enumerate delivered shipments for a merchant in a time window,
    /// with fee breakdown computed per the same rules as `/internal/shipments/:id/billing`.
    ///
    /// Paginates in 500-row chunks to cap memory for large merchants; caller
    /// receives the full flattened list.
    pub async fn list_for_billing(
        &self,
        q: BillingShipmentsQuery,
    ) -> AppResult<Vec<BillingShipmentDto>> {
        const CHUNK: i64 = 500;
        let mut out   = Vec::new();
        let mut offset = 0i64;
        loop {
            let filter = ShipmentListFilter {
                tenant_id:    q.tenant_id,
                merchant_id:  Some(q.merchant_id),
                status:       Some("delivered".into()),
                updated_from: Some(q.from),
                updated_to:   Some(q.to),
                limit:        CHUNK,
                offset,
            };
            let (chunk, _total) = self.repo.list(&filter).await.map_err(AppError::Internal)?;
            if chunk.is_empty() { break; }
            let chunk_len = chunk.len() as i64;
            for s in chunk {
                let base_freight   = s.compute_base_fee();
                let fuel_surcharge = (base_freight.amount as f64 * 0.05).round() as i64;
                let insurance      = s.declared_value
                    .as_ref()
                    .map(|v| (v.amount as f64 * 0.005).round() as i64)
                    .unwrap_or(0);
                let total = base_freight.amount + fuel_surcharge + insurance;
                out.push(BillingShipmentDto {
                    shipment_id:          s.id.inner(),
                    awb:                  s.awb.as_str().to_string(),
                    merchant_id:          s.merchant_id.inner(),
                    currency:             format!("{:?}", base_freight.currency),
                    base_freight_cents:   base_freight.amount,
                    fuel_surcharge_cents: fuel_surcharge,
                    insurance_cents:      insurance,
                    total_cents:          total,
                    delivered_at:         s.updated_at,
                });
            }
            if chunk_len < CHUNK { break; }
            offset += CHUNK;
        }
        Ok(out)
    }
}
