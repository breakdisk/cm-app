use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use super::driver_document::DocumentStatus;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceStatus {
    PendingSubmission,
    UnderReview,
    Compliant,
    ExpiringSoon,
    Expired,
    Suspended,
    Rejected,
}

impl ComplianceStatus {
    /// Derive overall status from the current set of required document statuses.
    /// `has_missing` = true when fewer approved/submitted docs exist than required count.
    /// `has_expiring` = true when any approved doc is within its warn window.
    pub fn derive(
        doc_statuses: &[DocumentStatus],
        has_missing: bool,
        has_expiring: bool,
    ) -> Self {
        if has_missing {
            return Self::PendingSubmission;
        }
        if doc_statuses.iter().any(|s| *s == DocumentStatus::Rejected) {
            return Self::PendingSubmission;
        }
        if doc_statuses.iter().any(|s| *s == DocumentStatus::Expired) {
            return Self::Expired;
        }
        if doc_statuses.iter().any(|s| matches!(s, DocumentStatus::Submitted | DocumentStatus::UnderReview)) {
            return Self::UnderReview;
        }
        if has_expiring {
            return Self::ExpiringSoon;
        }
        Self::Compliant
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingSubmission => "pending_submission",
            Self::UnderReview       => "under_review",
            Self::Compliant         => "compliant",
            Self::ExpiringSoon      => "expiring_soon",
            Self::Expired           => "expired",
            Self::Suspended         => "suspended",
            Self::Rejected          => "rejected",
        }
    }

    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "pending_submission" => Ok(Self::PendingSubmission),
            "under_review"       => Ok(Self::UnderReview),
            "compliant"          => Ok(Self::Compliant),
            "expiring_soon"      => Ok(Self::ExpiringSoon),
            "expired"            => Ok(Self::Expired),
            "suspended"          => Ok(Self::Suspended),
            "rejected"           => Ok(Self::Rejected),
            _                    => Err(anyhow::anyhow!("unknown compliance status: {}", s)),
        }
    }

    /// Can a driver with this status be assigned tasks?
    pub fn is_assignable(&self) -> bool {
        matches!(self, Self::Compliant | Self::ExpiringSoon | Self::Expired)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceProfile {
    pub id:               Uuid,
    pub tenant_id:        Uuid,
    pub entity_type:      String,
    pub entity_id:        Uuid,
    pub overall_status:   ComplianceStatus,
    pub jurisdiction:     String,
    pub last_reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by:      Option<Uuid>,
    pub suspended_at:     Option<DateTime<Utc>>,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_approved_is_compliant() {
        let statuses = vec![DocumentStatus::Approved, DocumentStatus::Approved];
        assert_eq!(
            ComplianceStatus::derive(&statuses, false, false),
            ComplianceStatus::Compliant
        );
    }

    #[test]
    fn any_submitted_is_under_review() {
        let statuses = vec![DocumentStatus::Approved, DocumentStatus::Submitted];
        assert_eq!(
            ComplianceStatus::derive(&statuses, false, false),
            ComplianceStatus::UnderReview
        );
    }

    #[test]
    fn any_missing_is_pending() {
        let statuses = vec![DocumentStatus::Approved];
        assert_eq!(
            ComplianceStatus::derive(&statuses, true, false),
            ComplianceStatus::PendingSubmission
        );
    }

    #[test]
    fn any_expiring_soon_with_all_approved() {
        let statuses = vec![DocumentStatus::Approved, DocumentStatus::Approved];
        assert_eq!(
            ComplianceStatus::derive(&statuses, false, true),
            ComplianceStatus::ExpiringSoon
        );
    }

    #[test]
    fn any_expired_is_expired() {
        let statuses = vec![DocumentStatus::Approved, DocumentStatus::Expired];
        assert_eq!(
            ComplianceStatus::derive(&statuses, false, false),
            ComplianceStatus::Expired
        );
    }

    #[test]
    fn rejected_doc_returns_to_pending() {
        let statuses = vec![DocumentStatus::Approved, DocumentStatus::Rejected];
        assert_eq!(
            ComplianceStatus::derive(&statuses, false, false),
            ComplianceStatus::PendingSubmission
        );
    }
}
