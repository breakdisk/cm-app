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
            created_at: now,
            updated_at: now,
        }
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
