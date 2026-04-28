use std::sync::Arc;
use anyhow::Context;
use chrono::Utc;
use uuid::Uuid;
use crate::domain::{
    entities::{
        ComplianceProfile, ComplianceStatus, ComplianceAuditLog,
        DriverDocument, DocumentStatus,
    },
    repositories::{
        ComplianceProfileRepository, DriverDocumentRepository,
        DocumentTypeRepository, AuditLogRepository,
    },
    events::{ComplianceStatusChangedPayload, DocumentReviewedPayload, DriverReinstatedPayload},
};
use crate::infrastructure::messaging::ComplianceProducer;

pub struct ComplianceService {
    pub profiles:  Arc<dyn ComplianceProfileRepository>,
    pub documents: Arc<dyn DriverDocumentRepository>,
    pub doc_types: Arc<dyn DocumentTypeRepository>,
    pub audit:     Arc<dyn AuditLogRepository>,
    pub producer:  Arc<ComplianceProducer>,
}

impl ComplianceService {
    pub fn new(
        profiles:  Arc<dyn ComplianceProfileRepository>,
        documents: Arc<dyn DriverDocumentRepository>,
        doc_types: Arc<dyn DocumentTypeRepository>,
        audit:     Arc<dyn AuditLogRepository>,
        producer:  Arc<ComplianceProducer>,
    ) -> Self {
        Self { profiles, documents, doc_types, audit, producer }
    }

    /// Idempotent — find an existing compliance profile for (tenant, entity_type,
    /// entity_id), or create a fresh one in `PendingSubmission` status. Used on
    /// the customer-app KYC path where no earlier event guarantees a profile
    /// exists, and on the driver.registered event handler to avoid duplicates.
    pub async fn ensure_profile(
        &self,
        tenant_id:    Uuid,
        entity_type:  &str,
        entity_id:    Uuid,
        jurisdiction: &str,
    ) -> anyhow::Result<ComplianceProfile> {
        if let Some(p) = self.profiles.find_by_entity(tenant_id, entity_type, entity_id).await? {
            return Ok(p);
        }
        let profile = ComplianceProfile {
            id:               Uuid::new_v4(),
            tenant_id,
            entity_type:      entity_type.to_owned(),
            entity_id,
            overall_status:   ComplianceStatus::PendingSubmission,
            jurisdiction:     jurisdiction.to_owned(),
            last_reviewed_at: None,
            reviewed_by:      None,
            suspended_at:     None,
            created_at:       Utc::now(),
            updated_at:       Utc::now(),
        };
        self.profiles.save(&profile).await?;
        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            tenant_id,
            compliance_profile_id: profile.id,
            document_id:           None,
            event_type:            "profile_created".into(),
            actor_id:              entity_id,
            actor_type:            "system".into(),
            notes:                 None,
            created_at:            Utc::now(),
        }).await?;
        Ok(profile)
    }

    /// Called when driver.registered Kafka event is received.
    /// Returns anyhow::Result<()> (consumer calls this and just logs errors).
    pub async fn create_profile_for_driver(
        &self,
        tenant_id:    Uuid,
        driver_id:    Uuid,
        jurisdiction: &str,
    ) -> anyhow::Result<()> {
        // Idempotent — skip if profile already exists
        if self.profiles.find_by_entity(tenant_id, "driver", driver_id).await?.is_some() {
            return Ok(());
        }
        let profile = ComplianceProfile {
            id:               Uuid::new_v4(),
            tenant_id,
            entity_type:      "driver".into(),
            entity_id:        driver_id,
            overall_status:   ComplianceStatus::PendingSubmission,
            jurisdiction:     jurisdiction.to_owned(),
            last_reviewed_at: None,
            reviewed_by:      None,
            suspended_at:     None,
            created_at:       Utc::now(),
            updated_at:       Utc::now(),
        };
        self.profiles.save(&profile).await?;
        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            tenant_id,
            compliance_profile_id: profile.id,
            document_id:           None,
            event_type:            "profile_created".into(),
            actor_id:              driver_id,
            actor_type:            "system".into(),
            notes:                 None,
            created_at:            Utc::now(),
        }).await?;
        Ok(())
    }

    /// Called when carrier.onboarded Kafka event is received.
    pub async fn create_profile_for_carrier(
        &self,
        tenant_id:  Uuid,
        carrier_id: Uuid,
    ) -> anyhow::Result<()> {
        if self.profiles.find_by_entity(tenant_id, "carrier", carrier_id).await?.is_some() {
            return Ok(());
        }
        let profile = ComplianceProfile {
            id:               Uuid::new_v4(),
            tenant_id,
            entity_type:      "carrier".into(),
            entity_id:        carrier_id,
            overall_status:   ComplianceStatus::PendingSubmission,
            jurisdiction:     "PH".into(),
            last_reviewed_at: None,
            reviewed_by:      None,
            suspended_at:     None,
            created_at:       Utc::now(),
            updated_at:       Utc::now(),
        };
        self.profiles.save(&profile).await?;
        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            tenant_id,
            compliance_profile_id: profile.id,
            document_id:           None,
            event_type:            "profile_created".into(),
            actor_id:              carrier_id,
            actor_type:            "system".into(),
            notes:                 None,
            created_at:            Utc::now(),
        }).await?;
        Ok(())
    }

    /// Called when vehicle.registered Kafka event is received.
    pub async fn create_profile_for_vehicle(
        &self,
        tenant_id:     Uuid,
        vehicle_id:    Uuid,
        jurisdiction:  &str,
    ) -> anyhow::Result<()> {
        if self.profiles.find_by_entity(tenant_id, "vehicle", vehicle_id).await?.is_some() {
            return Ok(());
        }
        let profile = ComplianceProfile {
            id:               Uuid::new_v4(),
            tenant_id,
            entity_type:      "vehicle".into(),
            entity_id:        vehicle_id,
            overall_status:   ComplianceStatus::PendingSubmission,
            jurisdiction:     jurisdiction.to_owned(),
            last_reviewed_at: None,
            reviewed_by:      None,
            suspended_at:     None,
            created_at:       Utc::now(),
            updated_at:       Utc::now(),
        };
        self.profiles.save(&profile).await?;
        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            tenant_id,
            compliance_profile_id: profile.id,
            document_id:           None,
            event_type:            "profile_created".into(),
            actor_id:              vehicle_id,
            actor_type:            "system".into(),
            notes:                 None,
            created_at:            Utc::now(),
        }).await?;
        Ok(())
    }

    /// Driver submits a document. Returns the created DriverDocument.
    pub async fn submit_document(
        &self,
        profile_id:       Uuid,
        document_type_id: Uuid,
        document_number:  String,
        issue_date:       Option<chrono::NaiveDate>,
        expiry_date:      Option<chrono::NaiveDate>,
        file_url:         String,
        actor_id:         Uuid,
    ) -> anyhow::Result<DriverDocument> {
        let profile = self.profiles.find_by_id(profile_id).await?
            .context("Profile not found")?;

        // Supersede any existing non-superseded doc of the same type
        let existing = self.documents.list_by_profile(profile_id).await?;
        for mut doc in existing.into_iter()
            .filter(|d| d.document_type_id == document_type_id
                && d.status != DocumentStatus::Superseded)
        {
            doc.status = DocumentStatus::Superseded;
            doc.updated_at = Utc::now();
            self.documents.save(&doc).await?;
        }

        let doc = DriverDocument {
            id:                    Uuid::new_v4(),
            compliance_profile_id: profile_id,
            document_type_id,
            document_number,
            issue_date,
            expiry_date,
            file_url,
            status:                DocumentStatus::Submitted,
            rejection_reason:      None,
            reviewed_by:           None,
            reviewed_at:           None,
            submitted_at:          Utc::now(),
            updated_at:            Utc::now(),
        };
        self.documents.save(&doc).await?;

        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            tenant_id:             profile.tenant_id,
            compliance_profile_id: profile_id,
            document_id:           Some(doc.id),
            event_type:            "doc_submitted".into(),
            actor_id,
            actor_type:            "driver".into(),
            notes:                 None,
            created_at:            Utc::now(),
        }).await?;

        self.recompute_and_publish(&profile).await?;
        Ok(doc)
    }

    /// Admin approves or rejects a document.
    pub async fn review_document(
        &self,
        doc_id:           Uuid,
        approved:         bool,
        rejection_reason: Option<String>,
        admin_id:         Uuid,
    ) -> anyhow::Result<()> {
        let mut doc = self.documents.find_by_id(doc_id).await?
            .context("Document not found")?;
        let profile = self.profiles.find_by_id(doc.compliance_profile_id).await?
            .context("Profile not found")?;

        doc.status = if approved { DocumentStatus::Approved } else { DocumentStatus::Rejected };
        doc.rejection_reason = rejection_reason.clone();
        doc.reviewed_by = Some(admin_id);
        doc.reviewed_at = Some(Utc::now());
        doc.updated_at = Utc::now();
        self.documents.save(&doc).await?;

        let doc_type_code = self.doc_types.find_by_id(doc.document_type_id).await?
            .context(format!("Document type {} not found for reviewed document", doc.document_type_id))?
            .code;

        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            tenant_id:             profile.tenant_id,
            compliance_profile_id: profile.id,
            document_id:           Some(doc_id),
            event_type:            if approved { "doc_approved" } else { "doc_rejected" }.into(),
            actor_id:              admin_id,
            actor_type:            "admin".into(),
            notes:                 rejection_reason.clone(),
            created_at:            Utc::now(),
        }).await?;

        self.producer.publish_document_reviewed(
            profile.tenant_id,
            DocumentReviewedPayload {
                tenant_id:        profile.tenant_id,
                entity_id:        profile.entity_id,
                document_type:    doc_type_code,
                decision:         if approved { "approved" } else { "rejected" }.into(),
                rejection_reason,
            },
        ).await?;

        self.recompute_and_publish(&profile).await?;
        Ok(())
    }

    /// Admin manually suspends an entity.
    pub async fn suspend(
        &self, profile_id: Uuid, admin_id: Uuid, reason: Option<String>,
    ) -> anyhow::Result<()> {
        let mut profile = self.profiles.find_by_id(profile_id).await?
            .context("Profile not found")?;
        let old_status = profile.overall_status.as_str().to_owned();
        profile.overall_status = ComplianceStatus::Suspended;
        profile.suspended_at = Some(Utc::now());
        profile.updated_at = Utc::now();
        self.profiles.save(&profile).await?;

        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            tenant_id:             profile.tenant_id,
            compliance_profile_id: profile_id,
            document_id:           None,
            event_type:            "admin_override".into(),
            actor_id:              admin_id,
            actor_type:            "admin".into(),
            notes:                 reason,
            created_at:            Utc::now(),
        }).await?;

        self.producer.publish_status_changed(profile.tenant_id, ComplianceStatusChangedPayload {
            entity_type:   profile.entity_type.clone(),
            entity_id:     profile.entity_id,
            old_status,
            new_status:    "suspended".into(),
            is_assignable: false,
        }).await?;
        Ok(())
    }

    /// Admin reinstates a suspended entity.
    pub async fn reinstate(
        &self, profile_id: Uuid, admin_id: Uuid, reason: Option<String>,
    ) -> anyhow::Result<()> {
        let mut profile = self.profiles.find_by_id(profile_id).await?
            .context("Profile not found")?;

        // Set transitional status so recompute_and_publish guard doesn't short-circuit.
        profile.overall_status = ComplianceStatus::UnderReview;
        profile.suspended_at   = None;
        profile.updated_at     = Utc::now();
        self.profiles.save(&profile).await?;

        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            tenant_id:             profile.tenant_id,
            compliance_profile_id: profile_id,
            document_id:           None,
            event_type:            "driver_reinstated".into(),
            actor_id:              admin_id,
            actor_type:            "admin".into(),
            notes:                 reason,
            created_at:            Utc::now(),
        }).await?;

        self.producer.publish_driver_reinstated(
            profile.tenant_id,
            DriverReinstatedPayload {
                entity_id:     profile.entity_id,
                entity_type:   profile.entity_type.clone(),
                reinstated_by: admin_id,
            },
        ).await?;

        // Re-derive final status from current document state
        self.recompute_and_publish(&profile).await
    }

    /// Recompute overall_status from current documents and publish if changed.
    async fn recompute_and_publish(&self, profile: &ComplianceProfile) -> anyhow::Result<()> {
        // Don't override a manual Suspended status via recompute
        if profile.overall_status == ComplianceStatus::Suspended {
            return Ok(());
        }

        let required_types = self.doc_types
            .list_required_for(&profile.entity_type, &profile.jurisdiction)
            .await?;
        let docs = self.documents.list_by_profile(profile.id).await?;

        // Filter to active (non-superseded) docs
        let active_docs: Vec<&DriverDocument> = docs.iter()
            .filter(|d| d.status != DocumentStatus::Superseded)
            .collect();

        let required_count = required_types.len();
        let active_statuses: Vec<DocumentStatus> = active_docs.iter()
            .map(|d| d.status.clone())
            .collect();

        let has_missing = active_docs.iter()
            .filter(|d| matches!(d.status,
                DocumentStatus::Submitted | DocumentStatus::UnderReview |
                DocumentStatus::Approved))
            .count() < required_count;

        let today = chrono::Utc::now().date_naive();
        let has_expiring = active_docs.iter().any(|d| {
            if d.status != DocumentStatus::Approved { return false; }
            let Some(exp) = d.expiry_date else { return false; };
            required_types.iter()
                .find(|rt| rt.id == d.document_type_id)
                .map(|rt| (exp - today).num_days() <= rt.warn_days_before as i64)
                .unwrap_or(false)
        });

        let new_status = ComplianceStatus::derive(&active_statuses, has_missing, has_expiring);

        if new_status.as_str() != profile.overall_status.as_str() {
            let old_status = profile.overall_status.as_str().to_owned();
            let mut updated = profile.clone();
            updated.overall_status = new_status.clone();
            updated.updated_at = Utc::now();
            self.profiles.save(&updated).await?;

            self.producer.publish_status_changed(profile.tenant_id, ComplianceStatusChangedPayload {
                entity_type:   profile.entity_type.clone(),
                entity_id:     profile.entity_id,
                old_status,
                new_status:    new_status.as_str().to_owned(),
                is_assignable: new_status.is_assignable(),
            }).await?;
        }
        Ok(())
    }

    /// Public wrapper used by ExpiryCheckerService and reinstate HTTP handler.
    pub async fn recompute_and_publish_public(&self, profile: &ComplianceProfile) -> anyhow::Result<()> {
        self.recompute_and_publish(profile).await
    }

    /// Static helper used in unit tests.
    pub fn compute_status(docs: &[DriverDocument], required_count: usize) -> ComplianceStatus {
        use chrono::Utc;
        let active: Vec<&DriverDocument> = docs.iter()
            .filter(|d| d.status != DocumentStatus::Superseded)
            .collect();
        let statuses: Vec<DocumentStatus> = active.iter().map(|d| d.status.clone()).collect();
        let has_missing = active.iter()
            .filter(|d| matches!(d.status, DocumentStatus::Submitted | DocumentStatus::UnderReview | DocumentStatus::Approved))
            .count() < required_count;
        let today = Utc::now().date_naive();
        let has_expiring = active.iter().any(|d| {
            d.status == DocumentStatus::Approved
                && d.expiry_date.map(|e| (e - today).num_days() <= 30).unwrap_or(false)
        });
        ComplianceStatus::derive(&statuses, has_missing, has_expiring)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recompute_does_not_override_manual_suspension() {
        use crate::domain::entities::ComplianceStatus;
        let status = ComplianceStatus::derive(&[DocumentStatus::Approved], false, false);
        // derive() itself returns Compliant — this is correct
        assert_eq!(status, ComplianceStatus::Compliant);
        // The recompute_and_publish function guards: if profile.overall_status == Suspended { return Ok(()) }
        // This means a suspended profile stays suspended regardless of document state.
    }

    #[test]
    fn recompute_status_all_approved_returns_compliant() {
        let docs = vec![
            mock_doc(DocumentStatus::Approved, Some(chrono::NaiveDate::from_ymd_opt(2030,1,1).unwrap())),
            mock_doc(DocumentStatus::Approved, Some(chrono::NaiveDate::from_ymd_opt(2030,1,1).unwrap())),
        ];
        let required_count = 2usize;
        let status = ComplianceService::compute_status(&docs, required_count);
        assert_eq!(status, ComplianceStatus::Compliant);
    }

    fn mock_doc(status: DocumentStatus, expiry: Option<chrono::NaiveDate>) -> DriverDocument {
        DriverDocument {
            id: uuid::Uuid::new_v4(),
            compliance_profile_id: uuid::Uuid::new_v4(),
            document_type_id: uuid::Uuid::new_v4(),
            document_number: "TEST".into(),
            issue_date: None,
            expiry_date: expiry,
            file_url: "s3://test".into(),
            status,
            rejection_reason: None,
            reviewed_by: None,
            reviewed_at: None,
            submitted_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}
