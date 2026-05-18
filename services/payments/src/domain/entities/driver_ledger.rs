//! Driver cash-flow ledger — tracks funds a driver holds on behalf of the tenant.
//!
//! # Two debit paths
//!
//! | Track | Trigger | Debit entry |
//! |-------|---------|-------------|
//! | Track A (Balikbayan) | `pickup.captured` with `service_code == "balikbayan"` | `declared_value_cents` |
//! | COD | `pod.captured` with `cod_amount_cents > 0` | `cod_amount_cents` |
//!
//! Remittance (driver hands cash to hub) creates a corresponding credit entry
//! and, when all entries are settled, flips the ledger to `Reconciled`.
//!
//! # Optimistic locking
//!
//! `version` is incremented on every `save()`. The repository implementation
//! must enforce `WHERE version = $expected_version` to prevent lost updates
//! when two events arrive for the same driver simultaneously (e.g. two Kafka
//! partitions both trigger a Track A debit for sequential pickups).

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Lifecycle state of a driver's open ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerStatus {
    /// Ledger is open — driver has unremitted funds.
    Open,
    /// All entries settled — ledger closed after end-of-shift remittance.
    Reconciled,
}

/// Type of movement recorded on the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// Driver received funds at pickup (Track A — Balikbayan declared value).
    PickupDebit,
    /// Driver received COD at delivery.
    CodDebit,
    /// Driver remitted cash to the hub or finance team.
    Remittance,
}

/// A single movement on the driver's ledger.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LedgerEntry {
    pub id:          Uuid,
    pub ledger_id:   Uuid,
    pub kind:        EntryKind,
    /// Positive = debit (driver received funds).
    /// Negative = credit (driver remitted funds).
    pub amount_cents: i64,
    pub shipment_id:  Option<Uuid>,
    pub reference:    Option<String>,  // pop_id / pod_id / batch_id etc.
    pub created_at:   DateTime<Utc>,
}

impl LedgerEntry {
    pub fn debit(ledger_id: Uuid, kind: EntryKind, amount_cents: i64, shipment_id: Option<Uuid>, reference: Option<String>) -> Self {
        assert!(amount_cents >= 0, "debit amount must be non-negative");
        Self {
            id: Uuid::new_v4(),
            ledger_id,
            kind,
            amount_cents,
            shipment_id,
            reference,
            created_at: Utc::now(),
        }
    }

    pub fn credit(ledger_id: Uuid, kind: EntryKind, amount_cents: i64, shipment_id: Option<Uuid>, reference: Option<String>) -> Self {
        assert!(amount_cents >= 0, "credit base amount must be non-negative");
        Self {
            id: Uuid::new_v4(),
            ledger_id,
            kind,
            amount_cents: -amount_cents,   // stored as negative
            shipment_id,
            reference,
            created_at: Utc::now(),
        }
    }
}

/// A driver's open or reconciled cash-flow ledger for a shift.
///
/// One ledger covers one shift (keyed by `shift_id`). When the driver
/// logs in for a new shift the repository creates a fresh ledger — the
/// prior one stays as an audit trail.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DriverLedger {
    pub id:           Uuid,
    pub tenant_id:    Uuid,
    pub driver_id:    Uuid,
    pub shift_id:     Option<Uuid>,
    pub status:       LedgerStatus,

    /// Running balance in cents (positive = driver owes this amount to the tenant).
    /// Recomputed from entries on load; not authoritative — entries are the source.
    pub balance_cents: i64,

    /// Optimistic-lock counter. `repository::save` must enforce
    /// `WHERE version = self.version` and increment before writing.
    pub version: i64,

    pub entries:      Vec<LedgerEntry>,
    pub created_at:   DateTime<Utc>,
    pub updated_at:   DateTime<Utc>,
}

impl DriverLedger {
    /// Create a new open ledger for a driver.
    pub fn new(tenant_id: Uuid, driver_id: Uuid, shift_id: Option<Uuid>) -> Self {
        let now = Utc::now();
        Self {
            id:            Uuid::new_v4(),
            tenant_id,
            driver_id,
            shift_id,
            status:        LedgerStatus::Open,
            balance_cents: 0,
            version:       0,
            entries:       Vec::new(),
            created_at:    now,
            updated_at:    now,
        }
    }

    /// Record a Track A (Balikbayan declared value) debit at pickup.
    pub fn debit_pickup(&mut self, amount_cents: i64, shipment_id: Uuid, pop_id: Uuid) {
        let entry = LedgerEntry::debit(
            self.id,
            EntryKind::PickupDebit,
            amount_cents,
            Some(shipment_id),
            Some(pop_id.to_string()),
        );
        self.balance_cents += amount_cents;
        self.entries.push(entry);
        self.updated_at = Utc::now();
    }

    /// Record a COD debit at delivery.
    pub fn debit_cod(&mut self, amount_cents: i64, shipment_id: Uuid, pod_id: Uuid) {
        let entry = LedgerEntry::debit(
            self.id,
            EntryKind::CodDebit,
            amount_cents,
            Some(shipment_id),
            Some(pod_id.to_string()),
        );
        self.balance_cents += amount_cents;
        self.entries.push(entry);
        self.updated_at = Utc::now();
    }

    /// Record a remittance (driver hands cash to hub).
    /// `amount_cents` is the cash handed over (positive); stored as negative entry.
    pub fn record_remittance(&mut self, amount_cents: i64, batch_id: Option<Uuid>) {
        let entry = LedgerEntry::credit(
            self.id,
            EntryKind::Remittance,
            amount_cents,
            None,
            batch_id.map(|id| id.to_string()),
        );
        self.balance_cents -= amount_cents;
        self.entries.push(entry);
        self.updated_at = Utc::now();
    }

    /// Close the ledger when balance reaches zero after remittance.
    pub fn reconcile(&mut self) -> Result<(), String> {
        if self.balance_cents != 0 {
            return Err(format!(
                "Cannot reconcile ledger with non-zero balance: {} cents outstanding",
                self.balance_cents
            ));
        }
        self.status     = LedgerStatus::Reconciled;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// True when the ledger can accept new debit entries.
    pub fn is_open(&self) -> bool { self.status == LedgerStatus::Open }
}
