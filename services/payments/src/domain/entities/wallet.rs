use chrono::{DateTime, Utc};
use logisticos_types::{Money, TenantId, Currency};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Merchant wallet — holds earned COD remittances, credits, and platform fee deductions.
/// Merchants can withdraw to their registered bank account (PayMongo, GCash, bank wire).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub balance: Money,
    pub currency: Currency,
    pub version: i64,    // Optimistic concurrency — prevents double-credit on concurrent updates
    pub reserved_centavos: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Wallet {
    pub fn new(tenant_id: TenantId, currency: Currency) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            balance: Money::new(0, currency),
            currency,
            version: 0,
            reserved_centavos: 0,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn available_centavos(&self) -> i64 {
        self.balance.amount - self.reserved_centavos
    }

    pub fn reserve(&mut self, amount: i64) -> Result<(), &'static str> {
        if self.available_centavos() < amount {
            return Err("Insufficient available balance");
        }
        self.reserved_centavos += amount;
        self.version += 1;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn release_reservation(&mut self, amount: i64) {
        self.reserved_centavos = (self.reserved_centavos - amount).max(0);
        self.version += 1;
        self.updated_at = Utc::now();
    }

    /// Release a reservation and debit the balance in a single version bump.
    /// Used during withdrawal disbursal to avoid double-incrementing the optimistic lock version.
    pub fn complete_withdrawal(&mut self, amount_centavos: i64) -> Result<(), &'static str> {
        if self.reserved_centavos < amount_centavos {
            return Err("Reserved amount is less than withdrawal amount — invariant violated");
        }
        self.reserved_centavos -= amount_centavos;

        if self.balance.amount < amount_centavos {
            return Err("Insufficient balance for withdrawal");
        }
        self.balance = Money::new(self.balance.amount - amount_centavos, self.currency);
        self.version += 1;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Credit the wallet (COD remittance, refund, promotional credit).
    pub fn credit(&mut self, amount: Money) -> Result<(), &'static str> {
        if amount.currency != self.currency {
            return Err("Currency mismatch");
        }
        if amount.amount <= 0 {
            return Err("Credit amount must be positive");
        }
        self.balance = Money::new(self.balance.amount + amount.amount, self.currency);
        self.version += 1;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Debit the wallet (withdrawal, platform fees).
    pub fn debit(&mut self, amount: Money) -> Result<(), &'static str> {
        if amount.currency != self.currency {
            return Err("Currency mismatch");
        }
        if amount.amount <= 0 {
            return Err("Debit amount must be positive");
        }
        if self.balance.amount < amount.amount {
            return Err("Insufficient wallet balance");
        }
        self.balance = Money::new(self.balance.amount - amount.amount, self.currency);
        self.version += 1;
        self.updated_at = Utc::now();
        Ok(())
    }
}

/// Immutable ledger entry for every wallet movement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletTransaction {
    pub id: Uuid,
    pub wallet_id: Uuid,
    pub tenant_id: TenantId,
    pub transaction_type: TransactionType,
    pub amount: Money,
    pub reference_id: Uuid,     // COD collection ID, invoice ID, or withdrawal request ID
    pub description: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TransactionType {
    CodCredit,          // COD remittance received
    InvoiceDebit,       // Invoice payment deducted
    PlatformFeeDebit,   // Platform handling fee
    Withdrawal,         // Merchant bank transfer initiated
    RefundCredit,       // Customer refund credited back
    AdjustmentCredit,   // Manual credit adjustment by support
    AdjustmentDebit,    // Manual debit adjustment by support
}

impl WalletTransaction {
    pub fn cod_credit(wallet_id: Uuid, tenant_id: TenantId, amount: Money, cod_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            wallet_id,
            tenant_id,
            transaction_type: TransactionType::CodCredit,
            amount,
            reference_id: cod_id,
            description: format!("COD remittance: ₱{:.2}", amount.amount as f64 / 100.0),
            created_at: Utc::now(),
        }
    }

    pub fn platform_fee_debit(wallet_id: Uuid, tenant_id: TenantId, amount: Money, cod_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            wallet_id,
            tenant_id,
            transaction_type: TransactionType::PlatformFeeDebit,
            amount,
            reference_id: cod_id,
            description: format!("COD handling fee: ₱{:.2}", amount.amount as f64 / 100.0),
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_wallet() -> Wallet {
        let tenant_id = TenantId::from_uuid(Uuid::new_v4());
        let mut w = Wallet::new(tenant_id, Currency::PHP);
        w.credit(Money::new(100_000, Currency::PHP)).unwrap();
        w
    }

    #[test]
    fn reserve_reduces_available() {
        let mut w = make_wallet();
        w.reserve(30_000).unwrap();
        assert_eq!(w.available_centavos(), 70_000);
        assert_eq!(w.balance.amount, 100_000); // balance unchanged
    }

    #[test]
    fn reserve_fails_when_insufficient() {
        let mut w = make_wallet();
        assert!(w.reserve(200_000).is_err());
    }

    #[test]
    fn release_reservation_restores_available() {
        let mut w = make_wallet();
        w.reserve(30_000).unwrap();
        w.release_reservation(30_000);
        assert_eq!(w.available_centavos(), 100_000);
    }
}
