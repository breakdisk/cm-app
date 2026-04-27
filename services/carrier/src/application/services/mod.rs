use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::{
    domain::{
        entities::{Carrier, CarrierId, RateCard, SlaCommitment, SlaRecord, ZoneSlaRow},
        repositories::{CarrierRepository, SlaRecordRepository},
    },
    infrastructure::messaging::CarrierPublisher,
};

#[derive(Debug, Deserialize)]
pub struct OnboardCarrierCommand {
    pub name:          String,
    pub code:          String,
    pub contact_email: String,
    pub sla_target:    f64,
    pub max_delivery_days: u8,
}

/// Partial update applied to an existing carrier — fields left None are
/// preserved. Used by partner-portal Settings (profile fields) and Rates
/// (rate_cards). Per ADR-0013 draft a future partner role will scope this
/// to the partner's own carrier_id; today the gateway requires CARRIERS_MANAGE
/// which only the admin role holds, so partners log in with admin tokens.
#[derive(Debug, Deserialize)]
pub struct UpdateCarrierCommand {
    pub name:           Option<String>,
    pub contact_email:  Option<String>,
    pub contact_phone:  Option<String>,
    pub api_endpoint:   Option<String>,
    pub sla:            Option<SlaCommitment>,
    pub rate_cards:     Option<Vec<RateCard>>,
}

/// Rate shopping result — returned to the dispatch service to choose carrier for a shipment.
#[derive(Debug, serde::Serialize)]
pub struct CarrierQuote {
    pub carrier_id:           Uuid,
    pub carrier_name:         String,
    pub carrier_code:         String,
    pub service_type:         String,
    /// Total cost in cents (base_rate + per_kg * weight). Renamed from `total_cents`
    /// to match the frontend and dispatch service contract.
    pub total_cost_cents:     i64,
    pub on_time_rate:         f64,
    pub grade:                crate::domain::entities::PerformanceGrade,
    /// Always true for quotes returned from rate-shop; false would indicate a
    /// carrier with no matching rate card (filtered before this point).
    pub eligible:             bool,
    pub ineligibility_reason: Option<String>,
}

pub struct CarrierService {
    repo:      Arc<dyn CarrierRepository>,
    sla_repo:  Arc<dyn SlaRecordRepository>,
    publisher: Arc<CarrierPublisher>,
}

impl CarrierService {
    pub fn new(
        repo:      Arc<dyn CarrierRepository>,
        sla_repo:  Arc<dyn SlaRecordRepository>,
        publisher: Arc<CarrierPublisher>,
    ) -> Self {
        Self { repo, sla_repo, publisher }
    }

    pub async fn onboard(&self, tenant_id: &TenantId, cmd: OnboardCarrierCommand) -> AppResult<Carrier> {
        // Validate code uniqueness within tenant.
        if self.repo.find_by_code(tenant_id, &cmd.code).await.map_err(AppError::internal)?.is_some() {
            return Err(AppError::BusinessRule(format!("Carrier code '{}' already registered", cmd.code)));
        }
        let sla = SlaCommitment {
            on_time_target_pct: cmd.sla_target.clamp(0.0, 100.0),
            max_delivery_days: cmd.max_delivery_days,
            penalty_per_breach: 0,
        };
        let carrier = Carrier::new(tenant_id.clone(), cmd.name.clone(), cmd.code.clone(), cmd.contact_email.clone(), sla);
        self.repo.save(&carrier).await.map_err(AppError::internal)?;

        if let Err(e) = self.publisher.carrier_onboarded(
            carrier.id.inner(),
            carrier.tenant_id.inner(),
            carrier.name.clone(),
            carrier.code.clone(),
            carrier.contact_email.clone(),
        ).await {
            tracing::warn!("Failed to publish carrier_onboarded event: {e}");
        }
        Ok(carrier)
    }

    pub async fn activate(&self, id: Uuid) -> AppResult<Carrier> {
        let mut carrier = self.get(id).await?;
        let old_status = format!("{:?}", carrier.status).to_lowercase();
        carrier.activate().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save(&carrier).await.map_err(AppError::internal)?;

        if let Err(e) = self.publisher.carrier_status_changed(
            carrier.id.inner(), carrier.tenant_id.inner(),
            old_status, "active".into(), String::new(),
        ).await {
            tracing::warn!("Failed to publish carrier_status_changed (activate): {e}");
        }
        Ok(carrier)
    }

    pub async fn suspend(&self, id: Uuid, reason: String) -> AppResult<Carrier> {
        let mut carrier = self.get(id).await?;
        let old_status = format!("{:?}", carrier.status).to_lowercase();
        carrier.suspend(&reason);
        self.repo.save(&carrier).await.map_err(AppError::internal)?;

        if let Err(e) = self.publisher.carrier_status_changed(
            carrier.id.inner(), carrier.tenant_id.inner(),
            old_status, "suspended".into(), reason,
        ).await {
            tracing::warn!("Failed to publish carrier_status_changed (suspend): {e}");
        }
        Ok(carrier)
    }

    /// Apply a partial update — name/contact/sla/rate_cards. Tenant
    /// isolation is enforced by the caller (the HTTP handler asserts
    /// claims.tenant_id == carrier.tenant_id before invoking).
    pub async fn update(&self, id: Uuid, cmd: UpdateCarrierCommand) -> AppResult<Carrier> {
        let mut carrier = self.get(id).await?;
        if let Some(v) = cmd.name           { carrier.name = v; }
        if let Some(v) = cmd.contact_email  { carrier.contact_email = v; }
        if let Some(v) = cmd.contact_phone  { carrier.contact_phone = Some(v); }
        if let Some(v) = cmd.api_endpoint   { carrier.api_endpoint = Some(v); }
        if let Some(v) = cmd.sla {
            // Clamp + sanity so a slip in the UI can't post 110% targets.
            carrier.sla = SlaCommitment {
                on_time_target_pct: v.on_time_target_pct.clamp(0.0, 100.0),
                max_delivery_days:  v.max_delivery_days.max(1),
                penalty_per_breach: v.penalty_per_breach.max(0),
            };
        }
        if let Some(v) = cmd.rate_cards { carrier.rate_cards = v; }
        carrier.updated_at = chrono::Utc::now();
        self.repo.save(&carrier).await.map_err(AppError::internal)?;
        Ok(carrier)
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Carrier> {
        self.repo
            .find_by_id(&CarrierId::from_uuid(id))
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Carrier", id: id.to_string() })
    }

    /// Look up the carrier whose contact_email matches the authenticated user's
    /// email — used by the partner portal's GET /v1/carriers/me endpoint.
    pub async fn get_by_email(&self, tenant_id: &TenantId, email: &str) -> AppResult<Carrier> {
        self.repo
            .find_by_contact_email(tenant_id, email)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Carrier", id: email.to_string() })
    }

    pub async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> AppResult<Vec<Carrier>> {
        self.repo.list(tenant_id, limit.clamp(1, 100), offset.max(0)).await.map_err(AppError::internal)
    }

    /// Rate shop: return quotes from all active carriers for a given service type and weight.
    /// Results sorted by total cost ascending (cheapest first).
    pub async fn shop_rates(
        &self,
        tenant_id: &TenantId,
        service_type: &str,
        weight_kg: f32,
    ) -> AppResult<Vec<CarrierQuote>> {
        let carriers = self.repo.list_active(tenant_id).await.map_err(AppError::internal)?;

        let mut quotes: Vec<CarrierQuote> = carriers
            .into_iter()
            .filter_map(|c| {
                c.quote(service_type, weight_kg).map(|total| CarrierQuote {
                    carrier_id:           c.id.inner(),
                    carrier_name:         c.name.clone(),
                    carrier_code:         c.code.clone(),
                    service_type:         service_type.to_owned(),
                    total_cost_cents:     total,
                    on_time_rate:         c.on_time_rate(),
                    grade:                c.performance_grade.clone(),
                    eligible:             true,
                    ineligibility_reason: None,
                })
            })
            .collect();

        quotes.sort_by_key(|q| q.total_cost_cents);
        Ok(quotes)
    }

    /// Called by `POST /v1/internal/sla-records` when dispatch allocates a carrier
    /// to a shipment. Creates the SLA record and publishes a `carrier.allocated` event.
    pub async fn create_sla_record(
        &self,
        record: SlaRecord,
        total_cost_cents: i64,
        method: String,
    ) -> AppResult<SlaRecord> {
        self.sla_repo.create(&record).await.map_err(AppError::internal)?;

        let payload = logisticos_events::payloads::CarrierAllocated {
            carrier_id:       record.carrier_id,
            tenant_id:        record.tenant_id,
            shipment_id:      record.shipment_id,
            zone:             record.zone.clone(),
            service_level:    record.service_level.clone(),
            total_cost_cents,
            promised_by:      record.promised_by.to_rfc3339(),
            method,
        };
        if let Err(e) = self.publisher.carrier_allocated(record.tenant_id, payload).await {
            tracing::warn!("Failed to publish carrier_allocated event: {e}");
        }
        Ok(record)
    }

    /// Zone-level SLA summary for a carrier over a time window.
    pub async fn sla_zone_summary(
        &self,
        carrier_id: Uuid,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<Vec<ZoneSlaRow>> {
        self.sla_repo.zone_summary(carrier_id, from, to).await.map_err(AppError::internal)
    }

    /// Paginated SLA record history for a carrier (partner portal).
    pub async fn sla_history(
        &self,
        carrier_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<SlaRecord>> {
        self.sla_repo
            .list_by_carrier(carrier_id, limit.clamp(1, 100), offset.max(0))
            .await
            .map_err(AppError::internal)
    }
}
