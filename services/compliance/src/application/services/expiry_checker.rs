use std::sync::Arc;
use chrono::Utc;
use uuid::Uuid;
use crate::{
    domain::{
        entities::DocumentStatus,
        events::ExpiryWarningPayload,
        repositories::{DriverDocumentRepository, ComplianceProfileRepository},
    },
    infrastructure::messaging::ComplianceProducer,
    application::services::ComplianceService,
};

pub struct ExpiryCheckerService {
    compliance:  Arc<ComplianceService>,
    documents:   Arc<dyn DriverDocumentRepository>,
    profiles:    Arc<dyn ComplianceProfileRepository>,
    producer:    Arc<ComplianceProducer>,
}

impl ExpiryCheckerService {
    pub fn new(
        compliance: Arc<ComplianceService>,
        documents:  Arc<dyn DriverDocumentRepository>,
        profiles:   Arc<dyn ComplianceProfileRepository>,
        producer:   Arc<ComplianceProducer>,
    ) -> Self {
        Self { compliance, documents, profiles, producer }
    }

    /// Run once per day. Call from a `tokio::spawn` loop in bootstrap.
    pub async fn run_once(&self) -> anyhow::Result<()> {
        use std::collections::HashMap;

        let today = Utc::now().date_naive();

        // Cache document types to avoid per-document DB round-trips
        let mut dt_cache: HashMap<Uuid, crate::domain::entities::DocumentType> = HashMap::new();

        // Helper closure can't be async, so inline the cache logic directly in each loop.

        // 1. Warn about docs expiring within their per-type warn window.
        let expiring = self.documents.find_expiring(60).await?;
        for doc in &expiring {
            // Cache lookup
            let dt = if let Some(cached) = dt_cache.get(&doc.document_type_id) {
                cached.clone()
            } else {
                match self.compliance.doc_types.find_by_id(doc.document_type_id).await? {
                    Some(dt) => {
                        dt_cache.insert(doc.document_type_id, dt.clone());
                        dt
                    }
                    None => continue,
                }
            };

            let days_remaining = doc.expiry_date
                .map(|e| (e - today).num_days())
                .unwrap_or(0);

            if days_remaining > dt.warn_days_before as i64 {
                continue;
            }

            let profile = match self.profiles.find_by_id(doc.compliance_profile_id).await? {
                Some(p) => p,
                None    => continue,
            };

            self.producer.publish_expiry_warning(profile.tenant_id, ExpiryWarningPayload {
                tenant_id:     profile.tenant_id,
                entity_id:     profile.entity_id,
                document_type: dt.code,
                expiry_date:   doc.expiry_date.unwrap_or_default().to_string(),
                days_remaining: i32::try_from(days_remaining).unwrap_or(i32::MAX),
            }).await?;
        }

        // 2. Mark expired docs + check per-type grace period
        let expired = self.documents.find_expired().await?;
        for mut doc in expired {
            let Some(expiry) = doc.expiry_date else { continue; };
            let days_past = (today - expiry).num_days();

            let profile = match self.profiles.find_by_id(doc.compliance_profile_id).await? {
                Some(p) => p,
                None    => continue,
            };

            // Mark doc as expired
            if doc.status == DocumentStatus::Approved {
                doc.status = DocumentStatus::Expired;
                doc.updated_at = Utc::now();
                self.documents.save(&doc).await?;
            }

            // Recompute profile (will transition to Expired overall status)
            self.compliance.recompute_and_publish_public(&profile).await?;

            // Load per-type grace period (cached)
            let grace_days = if let Some(cached) = dt_cache.get(&doc.document_type_id) {
                cached.grace_period_days as i64
            } else {
                match self.compliance.doc_types.find_by_id(doc.document_type_id).await? {
                    Some(dt) => {
                        let g = dt.grace_period_days as i64;
                        dt_cache.insert(doc.document_type_id, dt);
                        g
                    }
                    None => {
                        tracing::warn!(
                            document_type_id = %doc.document_type_id,
                            "Document type config not found; using default grace period of 7 days for auto-suspend decision"
                        );
                        7i64
                    }
                }
            };

            if days_past > grace_days {
                self.compliance.suspend(
                    profile.id,
                    Uuid::nil(),
                    Some(format!("Auto-suspended: document expired {days_past} days ago (grace: {grace_days}d)")),
                ).await?;
            }
        }

        Ok(())
    }
}
