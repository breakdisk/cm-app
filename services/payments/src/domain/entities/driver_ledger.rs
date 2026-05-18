//! DriverLedger — persistent cash-liability ledger for field drivers.
//!
//! ## Liability Model
//!
//! When a driver collects physical cash (COD at doorstep, Balikbayan deposit, final payment),
//! the platform records a **debit** against that driver's ledger — the driver *owes* the
//! platform those funds until they hand the cash in at the hub.
//!
//! ```text
//! Driver collects 150 PHP COD       → balance:    0  →  -15000 cents  (driver owes)
//! Driver hands cash to hub manager  → balance: -15000 →       0       (cleared)
//! Finance reconciles the shift      → reconciliation_status: reconciled
//! ```
//!
//! ## Concurrency
//!
//! `version` is an optimistic lock — every balance mutation must supply the current
//! version in a `WHERE version = $N` clause and increment it.  A conflict (0 rows updated)
//! means another POD event mutated the ledger concurrently; the caller retries.
//! This matches the `payments.wallets` pattern already in production.

use chrono::{DateTime, Utc};
use logisticos_types::{Money, TenantId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── LedgerEntryType ───────────────────────────────────────────────────────────

/// Classification of a single cash movement against a driver's ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerEntryType {
    // ── Debits (increase liability — driver owes platform) ────────────────────
    /// Track A Stage 1: driver collected box deposit cash (e.g., 50 AED).
    BalikbayanDepositCollected,
    /// Track A Stage 2: driver collected final ocean-freight balance in cash.
    BalikbayanFinalCollected,
    /// Track C: driver collected COD amount at doorstep.
    CodCollected,

    // ── Credits (reduce liability — platform received cash) ───────────────────
    /// Driver physically handed cash to a hub manager during shift close.
    CashRemittedToHub,
    /// Finance executed the formal `ReconcileCash` command confirming correct amounts.
    CashReconciled,

    // ── Adjustments ───────────────────────────────────────────────────────────
    /// Returned change or QR-wallet-initiated refund that reduces the driver's collected amount.
    OverageReversal,
    /// Admin corrective entry (e.g., miscounted cash found during audit).
    ManualAdjustment,
}

impl LedgerEntryType {
    /// True if this entry type *increases* the driver's cash liability.
    pub fn is_debit(&self) -> bool {
        matches!(
            self,
            Self::BalikbayanDepositCollected
                | Self::BalikbayanFinalCollected
                | Self::CodCollected
        )
    }
}

// ── LedgerEntry ───────────────────────────────────────────────────────────────

/// A single immutable line in the driver's cash ledger.
///
/// `amount_cents` is **signed**:
///   - Negative → increases liability (driver collected cash, owes platform).
///   - Positive → reduces liability (cash remitted or credited back).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub id:          Uuid,
    pub entry_type:  LedgerEntryType,
    /// Signed amount in the ledger's currency minor unit.
    pub amount_cents: i64,
    /// Causation references — for audit trail and idempotency checks.
    pub shipment_id: Option<Uuid>,
    pub invoice_id:  Option<Uuid>,
    pub pod_id:      Option<Uuid>,
    /// Human-readable audit memo (e.g., "COD AWB CM-UAE1-B0001234X, 150 AED").
    pub memo:        String,
    pub created_at:  DateTime<Utc>,
}

impl LedgerEntry {
    pub fn new(
        entry_type:  LedgerEntryType,
        amount_cents: i64,
        memo:        String,
    ) -> Self {
        Self {
            id:          Uuid::new_v4(),
            entry_type,
            amount_cents,
            shipment_id: None,
            invoice_id:  None,
            pod_id:      None,
            memo,
            created_at:  Utc::now(),
        }
    }

    pub fn with_shipment(mut self, shipment_id: Uuid) -> Self {
        self.shipment_id = Some(shipment_id);
        self
    }

    pub fn with_invoice(mut self, invoice_id: Uuid) -> Self {
        self.invoice_id = Some(invoice_id);
        self
    }

    pub fn with_pod(mut self, pod_id: Uuid) -> Self {
        self.pod_id = Some(pod_id);
        self
    }
}

// ── LedgerReconciliationStatus ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerReconciliationStatus {
    /// Cash is still with the driver — not yet handed to hub.
    Pending,
    /// Hub manager executed `ReconcileCash`; finance confirmed amounts match.
    Reconciled,
    /// Amount discrepancy found during reconciliation — under investigation.
    Disputed,
}

// ── DriverLedger ──────────────────────────────────────────────────────────────

/// The active cash-liability position for a single driver, scoped to a shift.
///
/// One ledger per `(tenant_id, driver_id, shift_id)` — `shift_id` is `None`
/// on tenants that have not yet enabled shift management (pre-shift era).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverLedger {
    pub id:                     Uuid,
    pub tenant_id:              TenantId,
    pub driver_id:              Uuid,
    /// Shift this ledger is scoped to. None = lifetime ledger.
    pub shift_id:               Option<Uuid>,

    /// Running net balance in the tenant's base currency minor unit (cents/centavos).
    /// Negative = driver owes platform; zero = no liability; positive = platform owes driver.
    pub balance_cents:          i64,
    pub currency:               logisticos_types::Currency,

    /// Chronological list of all entries. In-memory only — persisted to
    /// `driver_ledger_entries` as individual rows.
    pub entries:                Vec<LedgerEntry>,

    pub reconciliation_status:  LedgerReconciliationStatus,
    pub last_reconciliation_at: Option<DateTime<Utc>>,
    /// UUID of the hub manager who last reconciled this ledger.
    pub reconciled_by:          Option<Uuid>,

    /// Optimistic lock — increment on every balance mutation.
    pub version:                i32,
    pub created_at:             DateTime<Utc>,
    pub updated_at:             DateTime<Utc>,
}

// ── DriverLedgerError ─────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DriverLedgerError {
    #[error("Cannot record cash on an already-reconciled ledger")]
    AlreadyReconciled,

    #[error("Cannot reconcile a ledger with non-zero balance ({0} cents outstanding)")]
    BalanceNotZero(i64),

    #[error("Optimistic lock conflict — ledger was mutated concurrently (version mismatch)")]
    VersionConflict,

    #[error("Disputed ledger cannot be reconciled without admin resolution")]
    LedgerDisputed,
}

// ── impl DriverLedger ─────────────────────────────────────────────────────────

impl DriverLedger {
    /// Create a fresh, empty ledger for a driver at the start of a shift.
    pub fn open(
        tenant_id: TenantId,
        driver_id: Uuid,
        shift_id:  Option<Uuid>,
        currency:  logisticos_types::Currency,
    ) -> Self {
        let now = Utc::now();
        Self {
            id:                     Uuid::new_v4(),
            tenant_id,
            driver_id,
            shift_id,
            balance_cents:          0,
            currency,
            entries:                Vec::new(),
            reconciliation_status:  LedgerReconciliationStatus::Pending,
            last_reconciliation_at: None,
            reconciled_by:          None,
            version:                1,
            created_at:             now,
            updated_at:             now,
        }
    }

    // ── Cash debit operations ─────────────────────────────────────────────────

    /// Record cash collected at doorstep (COD, Balikbayan deposit, or final payment).
    ///
    /// `amount_cents` is always positive — the sign convention is applied internally
    /// (balance decreases, increasing the liability).
    pub fn record_cash_collected(
        &mut self,
        entry_type:  LedgerEntryType,
        amount_cents: i64,
        memo:        String,
        shipment_id: Uuid,
        invoice_id:  Option<Uuid>,
        pod_id:      Option<Uuid>,
    ) -> Result<&LedgerEntry, DriverLedgerError> {
        if self.reconciliation_status == LedgerReconciliationStatus::Reconciled {
            return Err(DriverLedgerError::AlreadyReconciled);
        }
        debug_assert!(
            entry_type.is_debit(),
            "record_cash_collected called with non-debit entry type {:?}",
            entry_type
        );

        // Negative amount = driver owes this amount to the platform.
        let signed = -amount_cents.abs();

        let entry = LedgerEntry::new(entry_type, signed, memo)
            .with_shipment(shipment_id)
            .with_invoice(invoice_id.unwrap_or(Uuid::nil()))
            .with_pod(pod_id.unwrap_or(Uuid::nil()));

        self.balance_cents += signed;
        self.entries.push(entry);
        self.version += 1;
        self.updated_at = Utc::now();

        Ok(self.entries.last().unwrap())
    }

    // ── Cash credit operations ────────────────────────────────────────────────

    /// Record cash handed in to the hub (partial or full shift close).
    ///
    /// `amount_cents` is the amount physically counted and accepted by the hub manager.
    pub fn record_cash_remitted(
        &mut self,
        amount_cents: i64,
        memo:        String,
    ) -> Result<&LedgerEntry, DriverLedgerError> {
        if self.reconciliation_status == LedgerReconciliationStatus::Reconciled {
            return Err(DriverLedgerError::AlreadyReconciled);
        }

        let entry = LedgerEntry::new(LedgerEntryType::CashRemittedToHub, amount_cents.abs(), memo);
        self.balance_cents += amount_cents.abs();
        self.entries.push(entry);
        self.version += 1;
        self.updated_at = Utc::now();

        Ok(self.entries.last().unwrap())
    }

    // ── Reconciliation ────────────────────────────────────────────────────────

    /// Hub manager executes end-of-shift reconciliation.
    ///
    /// Business rule: balance must be zero (or within a configured tolerance in future).
    /// A non-zero balance indicates unremitted cash or a counting error — flag as Disputed.
    pub fn reconcile(
        &mut self,
        reconciled_by: Uuid,
        force_disputed: bool,
    ) -> Result<LedgerReconciliationStatus, DriverLedgerError> {
        if self.reconciliation_status == LedgerReconciliationStatus::Disputed
            && !force_disputed
        {
            return Err(DriverLedgerError::LedgerDisputed);
        }
        if self.balance_cents != 0 && !force_disputed {
            // Non-zero balance but no force flag → auto-flag as Disputed.
            self.reconciliation_status  = LedgerReconciliationStatus::Disputed;
            self.last_reconciliation_at = Some(Utc::now());
            self.reconciled_by          = Some(reconciled_by);
            self.version               += 1;
            self.updated_at             = Utc::now();
            return Ok(LedgerReconciliationStatus::Disputed);
        }

        self.reconciliation_status  = LedgerReconciliationStatus::Reconciled;
        self.last_reconciliation_at = Some(Utc::now());
        self.reconciled_by          = Some(reconciled_by);

        // Final reconciliation entry (zero-amount, audit trail only).
        let entry = LedgerEntry::new(
            LedgerEntryType::CashReconciled,
            0,
            format!(
                "Shift reconciled by {}; final balance {} cents",
                reconciled_by, self.balance_cents
            ),
        );
        self.entries.push(entry);
        self.version    += 1;
        self.updated_at  = Utc::now();

        Ok(LedgerReconciliationStatus::Reconciled)
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Total cash the driver has collected (sum of all debit entries, as positive).
    pub fn total_collected_cents(&self) -> i64 {
        self.entries
            .iter()
            .filter(|e| e.entry_type.is_debit())
            .map(|e| e.amount_cents.abs())
            .sum()
    }

    /// Total cash remitted back to hub (sum of all credit entries).
    pub fn total_remitted_cents(&self) -> i64 {
        self.entries
            .iter()
            .filter(|e| !e.entry_type.is_debit())
            .map(|e| e.amount_cents)
            .sum()
    }

    /// Returns the outstanding liability as a positive amount (easier to display).
    pub fn outstanding_cents(&self) -> i64 {
        self.balance_cents.abs().max(0)
    }

    pub fn is_clean(&self) -> bool {
        self.balance_cents == 0
    }

    pub fn currency_money(&self, cents: i64) -> Money {
        Money::new(cents, self.currency)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::{Currency, TenantId};

    fn make_ledger() -> DriverLedger {
        DriverLedger::open(
            TenantId::new(),
            Uuid::new_v4(),
            Some(Uuid::new_v4()),
            Currency::PHP,
        )
    }

    #[test]
    fn cod_collected_increases_liability() {
        let mut l = make_ledger();
        l.record_cash_collected(
            LedgerEntryType::CodCollected,
            15000,
            "COD CM-PH1-S0001234X".into(),
            Uuid::new_v4(), None, None,
        ).unwrap();
        assert_eq!(l.balance_cents, -15000);
        assert_eq!(l.total_collected_cents(), 15000);
        assert!(!l.is_clean());
    }

    #[test]
    fn remit_clears_balance() {
        let mut l = make_ledger();
        l.record_cash_collected(
            LedgerEntryType::CodCollected, 15000,
            "COD".into(), Uuid::new_v4(), None, None,
        ).unwrap();
        l.record_cash_remitted(15000, "Handed to hub manager".into()).unwrap();
        assert_eq!(l.balance_cents, 0);
        assert!(l.is_clean());
    }

    #[test]
    fn reconcile_clean_ledger_succeeds() {
        let mut l = make_ledger();
        l.record_cash_collected(
            LedgerEntryType::CodCollected, 15000,
            "COD".into(), Uuid::new_v4(), None, None,
        ).unwrap();
        l.record_cash_remitted(15000, "Hub close".into()).unwrap();
        let status = l.reconcile(Uuid::new_v4(), false).unwrap();
        assert_eq!(status, LedgerReconciliationStatus::Reconciled);
    }

    #[test]
    fn reconcile_nonzero_balance_flags_disputed() {
        let mut l = make_ledger();
        l.record_cash_collected(
            LedgerEntryType::CodCollected, 15000,
            "COD".into(), Uuid::new_v4(), None, None,
        ).unwrap();
        // Driver only remits 10000, still owes 5000
        l.record_cash_remitted(10000, "Partial".into()).unwrap();
        let status = l.reconcile(Uuid::new_v4(), false).unwrap();
        assert_eq!(status, LedgerReconciliationStatus::Disputed);
        assert_eq!(l.balance_cents, -5000);
    }

    #[test]
    fn balikbayan_two_stage_accumulates() {
        let mut l = make_ledger();
        let ship = Uuid::new_v4();
        l.record_cash_collected(
            LedgerEntryType::BalikbayanDepositCollected, 5000,
            "Empty box deposit".into(), ship, None, None,
        ).unwrap();
        l.record_cash_collected(
            LedgerEntryType::BalikbayanFinalCollected, 30000,
            "Final ocean freight balance".into(), ship, None, None,
        ).unwrap();
        assert_eq!(l.balance_cents, -35000);
        assert_eq!(l.total_collected_cents(), 35000);
    }

    #[test]
    fn cannot_collect_on_reconciled_ledger() {
        let mut l = make_ledger();
        l.reconcile(Uuid::new_v4(), false).unwrap(); // no entries, balance=0
        let err = l.record_cash_collected(
            LedgerEntryType::CodCollected, 5000,
            "Late COD".into(), Uuid::new_v4(), None, None,
        ).unwrap_err();
        assert_eq!(err, DriverLedgerError::AlreadyReconciled);
    }
}
