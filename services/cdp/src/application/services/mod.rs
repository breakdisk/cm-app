use std::sync::Arc;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::domain::{
    entities::{BehavioralEvent, CustomerProfile, CustomerId, EventType},
    repositories::{CustomerProfileRepository, ProfileFilter},
};

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UpsertProfileCommand {
    pub external_customer_id: Uuid,
    pub name:                 Option<String>,
    pub email:                Option<String>,
    pub phone:                Option<String>,
}

#[derive(Debug)]
pub struct RecordEventCommand {
    pub tenant_id:            TenantId,
    pub external_customer_id: Uuid,
    pub event_type:           EventType,
    pub shipment_id:          Option<Uuid>,
    pub metadata:             serde_json::Value,
    pub occurred_at:          DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Read views
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ProfileSummary {
    pub id:                      Uuid,
    pub external_customer_id:    Uuid,
    pub name:                    Option<String>,
    pub email:                   Option<String>,
    pub phone:                   Option<String>,
    pub total_shipments:         u32,
    pub successful_deliveries:   u32,
    pub failed_deliveries:       u32,
    pub delivery_success_rate:   f32,
    pub total_cod_collected_cents: i64,
    pub clv_score:               f32,
    pub engagement_score:        f32,
    pub preferred_address:       Option<String>,
    pub last_shipment_at:        Option<DateTime<Utc>>,
}

impl From<&CustomerProfile> for ProfileSummary {
    fn from(p: &CustomerProfile) -> Self {
        Self {
            id:                         p.id.inner(),
            external_customer_id:       p.external_customer_id,
            name:                       p.name.clone(),
            email:                      p.email.clone(),
            phone:                      p.phone.clone(),
            total_shipments:            p.total_shipments,
            successful_deliveries:      p.successful_deliveries,
            failed_deliveries:          p.failed_deliveries,
            delivery_success_rate:      p.delivery_success_rate(),
            total_cod_collected_cents:  p.total_cod_collected_cents,
            clv_score:                  p.clv_score,
            engagement_score:           p.engagement_score,
            preferred_address:          p.preferred_address().map(str::to_owned),
            last_shipment_at:           p.last_shipment_at,
        }
    }
}

// ---------------------------------------------------------------------------
// ProfileService
// ---------------------------------------------------------------------------

pub struct ProfileService {
    repo: Arc<dyn CustomerProfileRepository>,
}

impl ProfileService {
    pub fn new(repo: Arc<dyn CustomerProfileRepository>) -> Self {
        Self { repo }
    }

    /// Upsert a customer profile. Creates if not found; merges identity fields if present.
    pub async fn upsert(
        &self,
        tenant_id: &TenantId,
        cmd: UpsertProfileCommand,
    ) -> AppResult<ProfileSummary> {
        let mut profile = match self
            .repo
            .find_by_external_id(tenant_id, cmd.external_customer_id)
            .await
            .map_err(AppError::internal)?
        {
            Some(p) => p,
            None => CustomerProfile::new(tenant_id.clone(), cmd.external_customer_id),
        };

        profile.enrich_identity(cmd.name, cmd.email, cmd.phone);
        self.repo.save(&profile).await.map_err(AppError::internal)?;
        Ok(ProfileSummary::from(&profile))
    }

    /// Record a behavioral event for a customer, auto-creating the profile if needed.
    pub async fn record_event(&self, cmd: RecordEventCommand) -> AppResult<()> {
        let mut profile = match self
            .repo
            .find_by_external_id(&cmd.tenant_id, cmd.external_customer_id)
            .await
            .map_err(AppError::internal)?
        {
            Some(p) => p,
            None => CustomerProfile::new(cmd.tenant_id.clone(), cmd.external_customer_id),
        };

        let event = BehavioralEvent::new(
            cmd.event_type,
            cmd.shipment_id,
            cmd.metadata,
            cmd.occurred_at,
        );
        profile.record_event(event);
        self.repo.save(&profile).await.map_err(AppError::internal)?;
        Ok(())
    }

    /// Get full profile by internal CDP id.
    pub async fn get_by_id(&self, id: Uuid) -> AppResult<CustomerProfile> {
        self.repo
            .find_by_id(&CustomerId::from_uuid(id))
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "CustomerProfile", id: id.to_string() })
    }

    /// Get profile by external customer id.
    pub async fn get_by_external_id(
        &self,
        tenant_id: &TenantId,
        external_id: Uuid,
    ) -> AppResult<CustomerProfile> {
        self.repo
            .find_by_external_id(tenant_id, external_id)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "CustomerProfile", id: external_id.to_string() })
    }

    /// List profiles for a tenant.
    pub async fn list(
        &self,
        tenant_id: &TenantId,
        filter: ProfileFilter,
    ) -> AppResult<Vec<ProfileSummary>> {
        let profiles = self
            .repo
            .list(tenant_id, &filter)
            .await
            .map_err(AppError::internal)?;
        Ok(profiles.iter().map(ProfileSummary::from).collect())
    }

    /// Top customers by CLV — for merchant dashboard.
    pub async fn top_by_clv(
        &self,
        tenant_id: &TenantId,
        limit: i64,
    ) -> AppResult<Vec<ProfileSummary>> {
        let limit = limit.clamp(1, 100);
        let profiles = self
            .repo
            .top_by_clv(tenant_id, limit)
            .await
            .map_err(AppError::internal)?;
        Ok(profiles.iter().map(ProfileSummary::from).collect())
    }
}
