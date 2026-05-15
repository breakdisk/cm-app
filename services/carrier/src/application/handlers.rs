use std::sync::Arc;

use logisticos_errors::AppResult;

use crate::{
    application::{
        commands::{
            ActivateCarrierCommand, CreateSlaRecordCommand, GenerateApiKeyCommand,
            RecordDeliveryOutcomeCommand, SuspendCarrierCommand,
        },
        queries::{
            GetCarrierByEmailQuery, GetCarrierByIdQuery, ListCarriersQuery, RateShopQuery,
            SlaHistoryQuery, SlaZoneSummaryQuery,
        },
        services::{CarrierService, OnboardCarrierCommand, UpdateCarrierCommand},
    },
    domain::entities::{Carrier, SlaRecord, ZoneSlaRow},
};

// ── Command handler ───────────────────────────────────────────────────────────

/// Routes carrier write commands to `CarrierService`.
///
/// This thin dispatcher layer exists so future additions (audit logging,
/// event sourcing, rate limiting per-command) have a single injection point
/// without touching the HTTP or Kafka layers.
pub struct CarrierCommandHandler {
    svc: Arc<CarrierService>,
}

impl CarrierCommandHandler {
    pub fn new(svc: Arc<CarrierService>) -> Self { Self { svc } }

    pub async fn handle_onboard(&self, tenant_id: &logisticos_types::TenantId, cmd: OnboardCarrierCommand) -> AppResult<Carrier> {
        self.svc.onboard(tenant_id, cmd).await
    }

    pub async fn handle_activate(&self, cmd: ActivateCarrierCommand) -> AppResult<Carrier> {
        self.svc.activate(cmd.carrier_id).await
    }

    pub async fn handle_suspend(&self, cmd: SuspendCarrierCommand) -> AppResult<Carrier> {
        self.svc.suspend(cmd.carrier_id, cmd.reason).await
    }

    pub async fn handle_update(&self, carrier_id: uuid::Uuid, cmd: UpdateCarrierCommand) -> AppResult<Carrier> {
        self.svc.update(carrier_id, cmd).await
    }

    pub async fn handle_generate_api_key(&self, cmd: GenerateApiKeyCommand) -> AppResult<String> {
        self.svc.generate_api_key(cmd.carrier_id).await
    }

    pub async fn handle_create_sla_record(&self, cmd: CreateSlaRecordCommand) -> AppResult<SlaRecord> {
        let record = SlaRecord::new(
            cmd.tenant_id,
            cmd.carrier_id,
            cmd.shipment_id,
            cmd.zone,
            cmd.service_level,
            cmd.promised_by,
        );
        self.svc.create_sla_record(record, cmd.total_cost_cents, cmd.method).await
    }

    pub async fn handle_delivery_outcome(&self, cmd: RecordDeliveryOutcomeCommand) -> AppResult<()> {
        use crate::domain::repositories::{CarrierRepository, SlaRecordRepository};

        // This mirrors `handle_delivery_completed/failed` in messaging.rs but is
        // exposed here for callers that bypass Kafka (e.g. synchronous test flows
        // or future REST-based fallback from driver-ops).
        let sla_repo = self.svc.sla_repo();
        let Some(mut sla) = sla_repo.find_by_shipment(cmd.shipment_id).await.map_err(logisticos_errors::AppError::internal)? else {
            tracing::warn!(shipment_id = %cmd.shipment_id, "No SLA record found for delivery outcome");
            return Ok(());
        };

        if cmd.on_time {
            let delivered_at = cmd.delivered_at.unwrap_or_else(chrono::Utc::now);
            sla.mark_delivered(delivered_at);
        } else {
            sla.mark_failed(cmd.failure_reason.as_deref().unwrap_or("failed"));
        }
        sla_repo.save_outcome(&sla).await.map_err(logisticos_errors::AppError::internal)?;

        let carrier_repo = self.svc.carrier_repo();
        let Some(mut carrier) = carrier_repo
            .find_by_id(&crate::domain::entities::CarrierId::from_uuid(sla.carrier_id))
            .await
            .map_err(logisticos_errors::AppError::internal)?
        else {
            tracing::warn!(carrier_id = %sla.carrier_id, "Carrier not found when recording delivery outcome");
            return Ok(());
        };
        carrier.record_delivery(cmd.on_time);
        carrier_repo.save(&carrier).await.map_err(logisticos_errors::AppError::internal)?;
        Ok(())
    }
}

// ── Query handler ─────────────────────────────────────────────────────────────

/// Routes carrier read queries to `CarrierService`.
pub struct CarrierQueryHandler {
    svc: Arc<CarrierService>,
}

impl CarrierQueryHandler {
    pub fn new(svc: Arc<CarrierService>) -> Self { Self { svc } }

    pub async fn handle_get_by_id(&self, q: GetCarrierByIdQuery) -> AppResult<Carrier> {
        self.svc.get(q.carrier_id).await
    }

    pub async fn handle_get_by_email(&self, q: GetCarrierByEmailQuery) -> AppResult<Carrier> {
        self.svc.get_by_email(&q.tenant_id, &q.email).await
    }

    pub async fn handle_list(&self, q: ListCarriersQuery) -> AppResult<Vec<Carrier>> {
        self.svc.list(&q.tenant_id, q.limit, q.offset).await
    }

    pub async fn handle_rate_shop(&self, q: RateShopQuery) -> AppResult<Vec<crate::application::services::CarrierQuote>> {
        self.svc.shop_rates(&q.tenant_id, &q.service_type, q.weight_kg).await
    }

    pub async fn handle_sla_zone_summary(&self, q: SlaZoneSummaryQuery) -> AppResult<Vec<ZoneSlaRow>> {
        self.svc.sla_zone_summary(q.carrier_id, q.from, q.to).await
    }

    pub async fn handle_sla_history(&self, q: SlaHistoryQuery) -> AppResult<Vec<SlaRecord>> {
        self.svc.sla_history(q.carrier_id, q.limit, q.offset).await
    }
}
