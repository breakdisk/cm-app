use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    Submitted,
    UnderReview,
    Approved,
    Rejected,
    Expired,
    Superseded,
}

impl DocumentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Submitted    => "submitted",
            Self::UnderReview  => "under_review",
            Self::Approved     => "approved",
            Self::Rejected     => "rejected",
            Self::Expired      => "expired",
            Self::Superseded   => "superseded",
        }
    }

    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "submitted"    => Ok(Self::Submitted),
            "under_review" => Ok(Self::UnderReview),
            "approved"     => Ok(Self::Approved),
            "rejected"     => Ok(Self::Rejected),
            "expired"      => Ok(Self::Expired),
            "superseded"   => Ok(Self::Superseded),
            _              => Err(anyhow::anyhow!("unknown document status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverDocument {
    pub id:                    Uuid,
    pub compliance_profile_id: Uuid,
    pub document_type_id:      Uuid,
    pub document_number:       String,
    pub issue_date:            Option<NaiveDate>,
    pub expiry_date:           Option<NaiveDate>,
    pub file_url:              String,
    pub status:                DocumentStatus,
    pub rejection_reason:      Option<String>,
    pub reviewed_by:           Option<Uuid>,
    pub reviewed_at:           Option<DateTime<Utc>>,
    pub submitted_at:          DateTime<Utc>,
    pub updated_at:            DateTime<Utc>,
}
