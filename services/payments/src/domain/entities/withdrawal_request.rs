use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WithdrawalStatus {
    Pending,
    Approved,
    Disbursed,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalRequest {
    pub id:              Uuid,
    pub tenant_id:       Uuid,
    pub wallet_id:       Uuid,
    pub amount_centavos: i64,
    pub currency:        String,
    pub status:          WithdrawalStatus,
    pub requested_by:    Uuid,
    pub reviewed_by:     Option<Uuid>,
    pub review_note:     Option<String>,
    pub reviewed_at:     Option<DateTime<Utc>>,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

impl WithdrawalRequest {
    pub fn new(tenant_id: Uuid, wallet_id: Uuid, amount_centavos: i64, requested_by: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            wallet_id,
            amount_centavos,
            currency: "PHP".into(),
            status: WithdrawalStatus::Pending,
            requested_by,
            reviewed_by: None,
            review_note: None,
            reviewed_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn approve(&mut self, reviewed_by: Uuid) -> Result<(), &'static str> {
        if self.status != WithdrawalStatus::Pending {
            return Err("Only pending requests can be approved");
        }
        self.status = WithdrawalStatus::Approved;
        self.reviewed_by = Some(reviewed_by);
        self.reviewed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn disburse(&mut self, reviewed_by: Uuid) -> Result<(), &'static str> {
        if self.status != WithdrawalStatus::Approved {
            return Err("Only approved requests can be disbursed");
        }
        self.status = WithdrawalStatus::Disbursed;
        self.reviewed_by = Some(reviewed_by);
        self.reviewed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn reject(&mut self, reviewed_by: Uuid, note: String) -> Result<(), &'static str> {
        if !matches!(self.status, WithdrawalStatus::Pending | WithdrawalStatus::Approved) {
            return Err("Only pending or approved requests can be rejected");
        }
        self.status = WithdrawalStatus::Rejected;
        self.reviewed_by = Some(reviewed_by);
        self.review_note = Some(note);
        self.reviewed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> WithdrawalRequest {
        WithdrawalRequest::new(Uuid::new_v4(), Uuid::new_v4(), 50_000, Uuid::new_v4())
    }

    #[test]
    fn approve_transitions_to_approved() {
        let mut r = req();
        r.approve(Uuid::new_v4()).unwrap();
        assert_eq!(r.status, WithdrawalStatus::Approved);
    }

    #[test]
    fn disburse_requires_approved() {
        let mut r = req();
        assert!(r.disburse(Uuid::new_v4()).is_err());
        r.approve(Uuid::new_v4()).unwrap();
        r.disburse(Uuid::new_v4()).unwrap();
        assert_eq!(r.status, WithdrawalStatus::Disbursed);
    }

    #[test]
    fn reject_from_pending() {
        let mut r = req();
        r.reject(Uuid::new_v4(), "Policy".into()).unwrap();
        assert_eq!(r.status, WithdrawalStatus::Rejected);
        assert_eq!(r.review_note.as_deref(), Some("Policy"));
    }
}
