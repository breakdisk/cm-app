use std::sync::Arc;

use tonic::{Request, Response, Status};
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::application::services::CarrierService;

// ── Generated proto types ────────────────────────────────────────────────────

pub mod proto {
    tonic::include_proto!("logisticos.carrier.v1");
}

use proto::{
    carrier_grpc_service_server::{CarrierGrpcService, CarrierGrpcServiceServer},
    CarrierQuote, CarrierSummary, GetCarrierRequest, GetCarrierResponse,
    ListActiveCarriersRequest, ListActiveCarriersResponse, RateShopRequest, RateShopResponse,
    RecordDeliveryOutcomeRequest, RecordDeliveryOutcomeResponse,
};

// ── Service implementation ────────────────────────────────────────────────────

pub struct CarrierGrpc {
    svc: Arc<CarrierService>,
}

impl CarrierGrpc {
    pub fn new(svc: Arc<CarrierService>) -> Self { Self { svc } }

    pub fn into_service(self) -> CarrierGrpcServiceServer<Self> {
        CarrierGrpcServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl CarrierGrpcService for CarrierGrpc {
    async fn get_carrier(
        &self,
        req: Request<GetCarrierRequest>,
    ) -> Result<Response<GetCarrierResponse>, Status> {
        let r = req.into_inner();
        let id = parse_uuid(&r.carrier_id, "carrier_id")?;

        let carrier = self
            .svc
            .get(id)
            .await
            .map_err(|e| Status::not_found(e.to_string()))?;

        Ok(Response::new(GetCarrierResponse {
            carrier: Some(carrier_to_summary(&carrier)),
        }))
    }

    async fn list_active_carriers(
        &self,
        req: Request<ListActiveCarriersRequest>,
    ) -> Result<Response<ListActiveCarriersResponse>, Status> {
        let r = req.into_inner();
        let tenant_id = parse_uuid(&r.tenant_id, "tenant_id")?;
        let tid = TenantId::from_uuid(tenant_id);

        let carriers = self
            .svc
            .list_active(&tid)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(ListActiveCarriersResponse {
            carriers: carriers.into_iter().map(|c| carrier_to_summary(&c)).collect(),
        }))
    }

    async fn rate_shop(
        &self,
        req: Request<RateShopRequest>,
    ) -> Result<Response<RateShopResponse>, Status> {
        let r = req.into_inner();
        let tenant_id = parse_uuid(&r.tenant_id, "tenant_id")?;
        let tid = TenantId::from_uuid(tenant_id);

        let quotes = self
            .svc
            .shop_rates(&tid, &r.service_type, r.weight_kg)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_quotes = quotes
            .into_iter()
            .map(|q| CarrierQuote {
                carrier_id:       q.carrier_id.to_string(),
                carrier_name:     q.carrier_name,
                carrier_code:     q.carrier_code,
                service_type:     q.service_type,
                total_cost_cents: q.total_cost_cents,
                on_time_rate:     q.on_time_rate,
                grade:            serde_json::to_value(&q.grade)
                                      .ok()
                                      .and_then(|v| v.as_str().map(|s| s.to_owned()))
                                      .unwrap_or_default(),
                eligible:         q.eligible,
            })
            .collect();

        Ok(Response::new(RateShopResponse { quotes: proto_quotes }))
    }

    async fn record_delivery_outcome(
        &self,
        req: Request<RecordDeliveryOutcomeRequest>,
    ) -> Result<Response<RecordDeliveryOutcomeResponse>, Status> {
        let r = req.into_inner();
        let shipment_id = parse_uuid(&r.shipment_id, "shipment_id")?;

        let delivered_at = if r.on_time && !r.delivered_at.is_empty() {
            r.delivered_at.parse::<chrono::DateTime<chrono::Utc>>().ok()
        } else {
            None
        };

        let cmd = crate::application::commands::RecordDeliveryOutcomeCommand {
            shipment_id,
            on_time:        r.on_time,
            delivered_at,
            failure_reason: if r.failure_reason.is_empty() { None } else { Some(r.failure_reason) },
        };

        let handler = crate::application::handlers::CarrierCommandHandler::new(
            Arc::clone(&self.svc),
        );

        match handler.handle_delivery_outcome(cmd).await {
            Ok(()) => Ok(Response::new(RecordDeliveryOutcomeResponse {
                success:      true,
                error_detail: String::new(),
            })),
            Err(e) => Ok(Response::new(RecordDeliveryOutcomeResponse {
                success:      false,
                error_detail: e.to_string(),
            })),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

// Error type is shared across the crate; boxing it here alone would not shrink it.
#[allow(clippy::result_large_err)]
fn parse_uuid(s: &str, field: &'static str) -> Result<Uuid, Status> {
    Uuid::parse_str(s)
        .map_err(|_| Status::invalid_argument(format!("{field}: not a valid UUID")))
}

fn carrier_to_summary(c: &crate::domain::entities::Carrier) -> CarrierSummary {
    CarrierSummary {
        id:                c.id.inner().to_string(),
        tenant_id:         c.tenant_id.inner().to_string(),
        name:              c.name.clone(),
        code:              c.code.clone(),
        contact_email:     c.contact_email.clone(),
        status:            serde_json::to_value(&c.status)
                               .ok()
                               .and_then(|v| v.as_str().map(String::from))
                               .unwrap_or_default(),
        compliance_status: serde_json::to_value(&c.compliance_status)
                               .ok()
                               .and_then(|v| v.as_str().map(String::from))
                               .unwrap_or_default(),
        on_time_rate:      c.on_time_rate(),
        performance_grade: serde_json::to_value(&c.performance_grade)
                               .ok()
                               .and_then(|v| v.as_str().map(String::from))
                               .unwrap_or_default(),
        has_api_key:       c.has_api_key,
    }
}
