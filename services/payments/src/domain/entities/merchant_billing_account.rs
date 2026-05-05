use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantBillingAccount {
    pub id:                          Uuid,
    pub tenant_id:                   Uuid,
    pub merchant_id:                 Uuid,
    pub base_rate_override_centavos: Option<i64>,
    pub payment_terms_days:          i16,
    pub credit_limit_centavos:       i64,
    pub tin:                         Option<String>,
    pub vat_registered:              bool,
    pub billing_email:               String,
    pub invoice_channel:             String,
    pub bank_name:                   Option<String>,
    pub bank_account_number:         Option<String>,
    pub bank_account_name:           Option<String>,
    pub created_at:                  DateTime<Utc>,
    pub updated_at:                  DateTime<Utc>,
}

impl MerchantBillingAccount {
    pub fn new(
        tenant_id:     Uuid,
        merchant_id:   Uuid,
        billing_email: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            merchant_id,
            base_rate_override_centavos: None,
            payment_terms_days: 30,
            credit_limit_centavos: 0,
            tin: None,
            vat_registered: false,
            billing_email,
            invoice_channel: "email".into(),
            bank_name: None,
            bank_account_number: None,
            bank_account_name: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn masked_bank_account(&self) -> Option<String> {
        self.bank_account_number.as_ref().map(|n| {
            let digits: String = n.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() <= 4 {
                "*".repeat(digits.len())
            } else {
                format!("****{}", &digits[digits.len() - 4..])
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masked_bank_account_shows_last_4() {
        let mut acct = MerchantBillingAccount::new(
            Uuid::new_v4(), Uuid::new_v4(), "m@example.com".into()
        );
        acct.bank_account_number = Some("1234567890".into());
        assert_eq!(acct.masked_bank_account(), Some("****7890".into()));
    }

    #[test]
    fn masked_bank_account_none_when_absent() {
        let acct = MerchantBillingAccount::new(
            Uuid::new_v4(), Uuid::new_v4(), "m@example.com".into()
        );
        assert_eq!(acct.masked_bank_account(), None);
    }
}
