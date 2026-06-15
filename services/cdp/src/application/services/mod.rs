use std::sync::Arc;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::domain::{
    entities::{BehavioralEvent, CustomerProfile, CustomerId, EventType, ProfileType, Segment, SegmentFilter, SegmentMember},
    repositories::{CustomerProfileRepository, ProfileFilter, SegmentRepository},
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
    pub profile_type:         Option<ProfileType>,
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
    pub profile_type:            ProfileType,
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
            profile_type:               p.profile_type.clone(),
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
        if let Some(pt) = cmd.profile_type {
            profile.profile_type = pt;
        }
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

    /// List profiles for a tenant — returns full profiles so address_history is included.
    pub async fn list(
        &self,
        tenant_id: &TenantId,
        filter: ProfileFilter,
    ) -> AppResult<Vec<CustomerProfile>> {
        self.repo.list(tenant_id, &filter).await.map_err(AppError::internal)
    }

    /// Top customers by CLV — for merchant dashboard.
    pub async fn top_by_clv(
        &self,
        tenant_id: &TenantId,
        limit: i64,
    ) -> AppResult<Vec<CustomerProfile>> {
        let limit = limit.clamp(1, 100);
        self.repo.top_by_clv(tenant_id, limit).await.map_err(AppError::internal)
    }
}

// ---------------------------------------------------------------------------
// Commands for segments
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateSegmentCommand {
    pub name:        String,
    pub description: Option<String>,
    pub filter:      SegmentFilter,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSegmentCommand {
    pub name:        Option<String>,
    pub description: Option<String>,
    pub filter:      Option<SegmentFilter>,
}

// ---------------------------------------------------------------------------
// SegmentPreview — returned by preview endpoint
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SegmentPreview {
    pub estimated_count: i64,
    pub sample_members:  Vec<SegmentMember>,
}

// ---------------------------------------------------------------------------
// SegmentService
// ---------------------------------------------------------------------------

pub struct SegmentService {
    repo: Arc<dyn SegmentRepository>,
}

impl SegmentService {
    pub fn new(repo: Arc<dyn SegmentRepository>) -> Self { Self { repo } }

    pub async fn create(
        &self,
        tenant_id: &TenantId,
        cmd: CreateSegmentCommand,
    ) -> AppResult<Segment> {
        let segment = Segment::new(
            tenant_id.clone(),
            cmd.name,
            cmd.description.unwrap_or_default(),
            cmd.filter,
        );
        self.repo.create(&segment).await.map_err(AppError::internal)?;
        Ok(segment)
    }

    pub async fn get(&self, tenant_id: &TenantId, id: Uuid) -> AppResult<Segment> {
        self.repo
            .find_by_id(tenant_id, id)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Segment", id: id.to_string() })
    }

    pub async fn list(&self, tenant_id: &TenantId) -> AppResult<Vec<Segment>> {
        self.repo.list(tenant_id).await.map_err(AppError::internal)
    }

    pub async fn update(
        &self,
        tenant_id: &TenantId,
        id: Uuid,
        cmd: UpdateSegmentCommand,
    ) -> AppResult<Segment> {
        let mut segment = self.get(tenant_id, id).await?;
        if let Some(name) = cmd.name { segment.name = name; }
        if let Some(desc) = cmd.description { segment.description = desc; }
        if let Some(filter) = cmd.filter { segment.filter = filter; }
        segment.updated_at = Utc::now();
        self.repo.update(&segment).await.map_err(AppError::internal)?;
        Ok(segment)
    }

    pub async fn delete(&self, tenant_id: &TenantId, id: Uuid) -> AppResult<()> {
        self.repo.delete(tenant_id, id).await.map_err(AppError::internal)
    }

    /// Real-time preview: count + sample members (max 10).
    pub async fn preview(&self, tenant_id: &TenantId, id: Uuid) -> AppResult<SegmentPreview> {
        let segment = self.get(tenant_id, id).await?;
        self.preview_filter(tenant_id, &segment.filter).await
    }

    /// Preview an ad-hoc filter (used in the builder before saving).
    pub async fn preview_filter(
        &self,
        tenant_id: &TenantId,
        filter: &SegmentFilter,
    ) -> AppResult<SegmentPreview> {
        let count = self.repo.count_members(tenant_id, filter).await.map_err(AppError::internal)?;
        let sample = self.repo.query_members(tenant_id, filter, 10, 0).await.map_err(AppError::internal)?;
        Ok(SegmentPreview { estimated_count: count, sample_members: sample })
    }

    /// Paginated member listing.
    pub async fn members(
        &self,
        tenant_id: &TenantId,
        id: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<SegmentMember>> {
        let segment = self.get(tenant_id, id).await?;
        self.repo
            .query_members(tenant_id, &segment.filter, limit, offset)
            .await
            .map_err(AppError::internal)
    }

    /// Check which segments a customer belongs to (for automation rule context).
    pub async fn membership(
        &self,
        tenant_id: &TenantId,
        customer_id: Uuid,
    ) -> AppResult<Vec<Uuid>> {
        self.repo
            .segment_ids_for_customer(tenant_id, customer_id)
            .await
            .map_err(AppError::internal)
    }

    /// Seed default segments for a new tenant.
    pub async fn seed_defaults(&self, tenant_id: &TenantId) -> AppResult<Vec<Segment>> {
        let defaults: Vec<(&str, &str, SegmentFilter)> = vec![
            ("High CLV", "Customers with CLV score ≥ 60", SegmentFilter { min_clv: Some(60.0), ..Default::default() }),
            ("VIP",      "High CLV (≥80) and highly engaged (engagement ≥ 70)", SegmentFilter { min_clv: Some(80.0), min_engagement: Some(70.0), ..Default::default() }),
            ("At Risk",  "Customers with high churn tier", SegmentFilter { churn_tier: Some("high".to_owned()), ..Default::default() }),
            ("Inactive 30d", "No shipment in the last 30 days", SegmentFilter { inactive_days: Some(30), ..Default::default() }),
        ];

        let mut created = Vec::new();
        for (name, desc, filter) in defaults {
            let segment = Segment::new(tenant_id.clone(), name.to_owned(), desc.to_owned(), filter);
            self.repo.create(&segment).await.map_err(AppError::internal)?;
            created.push(segment);
        }
        Ok(created)
    }
}
