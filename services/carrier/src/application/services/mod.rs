use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::domain::{
    entities::{Carrier, CarrierId, RateCard, SlaCommitment},
    repositories::CarrierRepository,
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
    pub carrier_id:    Uuid,
    pub carrier_name:  String,
    pub carrier_code:  String,
    pub service_type:  String,
    pub total_cents:   i64,
    pub on_time_rate:  f64,
    pub grade:         crate::domain::entities::PerformanceGrade,
}

pub struct CarrierService {
    repo: Arc<dyn CarrierRepository>,
}

impl CarrierService {
    pub fn new(repo: Arc<dyn CarrierRepository>) -> Self { Self { repo } }

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
        let carrier = Carrier::new(tenant_id.clone(), cmd.name, cmd.code, cmd.contact_email, sla);
        self.repo.save(&carrier).await.map_err(AppError::internal)?;
        Ok(carrier)
    }

    pub async fn activate(&self, id: Uuid) -> AppResult<Carrier> {
        let mut carrier = self.get(id).await?;
        carrier.activate().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save(&carrier).await.map_err(AppError::internal)?;
        Ok(carrier)
    }

    pub async fn suspend(&self, id: Uuid, reason: String) -> AppResult<Carrier> {
        let mut carrier = self.get(id).await?;
        carrier.suspend(&reason);
        self.repo.save(&carrier).await.map_err(AppError::internal)?;
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
                    carrier_id:   c.id.inner(),
                    carrier_name: c.name.clone(),
                    carrier_code: c.code.clone(),
                    service_type: service_type.to_owned(),
                    total_cents:  total,
                    on_time_rate: c.on_time_rate(),
                    grade:        c.performance_grade.clone(),
                })
            })
            .collect();

        quotes.sort_by_key(|q| q.total_cents);
        Ok(quotes)
    }
}
