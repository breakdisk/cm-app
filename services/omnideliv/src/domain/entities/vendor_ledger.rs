//! Vendor payout ledger.
//!
//! Modelled on the platform's existing `DriverLedger`: an append-only entry log
//! with a denormalised balance. Entries are never updated or deleted — a
//! correction is a new compensating entry, so the history stays auditable and
//! a vendor dispute can always be reconstructed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerStatus {
    Open,
    Closed,
    Settled,
}

impl LedgerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LedgerStatus::Open    => "open",
            LedgerStatus::Closed  => "closed",
            LedgerStatus::Settled => "settled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    GoodsCredit,
    CommissionDebit,
    Adjustment,
    Payout,
}

impl EntryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryKind::GoodsCredit     => "goods_credit",
            EntryKind::CommissionDebit => "commission_debit",
            EntryKind::Adjustment      => "adjustment",
            EntryKind::Payout          => "payout",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub id:           Uuid,
    pub ledger_id:    Uuid,
    pub kind:         EntryKind,
    /// Signed. Credits are positive, debits and payouts negative, so the
    /// balance is always a plain sum and cannot disagree with the log.
    pub amount_cents: i64,
    pub order_id:     Option<Uuid>,
    pub leg_id:       Option<Uuid>,
    pub reference:    Option<String>,
    pub created_at:   DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorLedger {
    pub id:            Uuid,
    pub tenant_id:     Uuid,
    pub vendor_id:     Uuid,
    pub period:        String,
    pub status:        LedgerStatus,
    pub balance_cents: i64,
    pub version:       i64,
    pub entries:       Vec<LedgerEntry>,
    pub created_at:    DateTime<Utc>,
    pub updated_at:    DateTime<Utc>,
}

impl VendorLedger {
    pub fn open(tenant_id: Uuid, vendor_id: Uuid, period: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            vendor_id,
            period,
            status: LedgerStatus::Open,
            balance_cents: 0,
            version: 0,
            entries: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// The only way an entry enters the log, and the only place the balance
    /// moves. Private so no future method can post an entry without moving the
    /// balance, or move the balance without leaving a record of why.
    fn append(&mut self, kind: EntryKind, amount_cents: i64,
              order_id: Option<Uuid>, leg_id: Option<Uuid>, reference: Option<String>) {
        self.entries.push(LedgerEntry {
            id: Uuid::new_v4(),
            ledger_id: self.id,
            kind,
            amount_cents,
            order_id,
            leg_id,
            reference,
            created_at: Utc::now(),
        });
        self.balance_cents += amount_cents;
        self.version += 1;
        self.updated_at = Utc::now();
    }

    /// Credit a picked-up leg.
    ///
    /// Two entries, not one net figure: the vendor must be able to see the gross
    /// goods value and the commission separately, or a payout dispute has
    /// nothing to reconcile against.
    pub fn credit_leg(&mut self, goods_cents: i64, commission_cents: i64, order_id: Uuid, leg_id: Uuid) {
        self.append(EntryKind::GoodsCredit, goods_cents, Some(order_id), Some(leg_id), None);
        self.append(EntryKind::CommissionDebit, -commission_cents, Some(order_id), Some(leg_id), None);
    }

    pub fn record_payout(&mut self, amount_cents: i64, batch: Option<String>) {
        self.append(EntryKind::Payout, -amount_cents, None, None, batch);
    }

    /// A correction. Appends — never edits an existing entry.
    pub fn adjust(&mut self, amount_cents: i64, reason: String) {
        self.append(EntryKind::Adjustment, amount_cents, None, None, Some(reason));
    }

    pub fn is_open(&self) -> bool { self.status == LedgerStatus::Open }

    /// Recompute the balance from the log.
    ///
    /// The denormalised `balance_cents` is what queries read; this is what
    /// proves it. Any divergence means an entry was written by something other
    /// than `append`, which is the failure this ledger shape exists to prevent.
    pub fn recomputed_balance_cents(&self) -> i64 {
        self.entries.iter().map(|e| e.amount_cents).sum()
    }
}

/// The ledger period a credit lands in, and the one a reader is shown.
///
/// One definition so the write path and the read path cannot label the same
/// week differently — a mismatch there would show a vendor an empty ledger
/// while their money sat in a period nobody was asking for.
pub fn current_period() -> String {
    use chrono::Datelike;
    let iso = chrono::Utc::now().iso_week();
    format!("{}-W{:02}", iso.year(), iso.week())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ledger() -> VendorLedger {
        VendorLedger::open(Uuid::new_v4(), Uuid::new_v4(), "2026-W32".into())
    }

    #[test]
    fn a_new_ledger_is_open_and_flat() {
        let l = ledger();
        assert_eq!(l.status, LedgerStatus::Open);
        assert_eq!(l.balance_cents, 0);
        assert!(l.entries.is_empty());
        assert!(l.is_open());
    }

    /// Crediting a pickup records the goods credit and the commission debit as
    /// two entries, not one net figure — the vendor must be able to see what
    /// was deducted and why.
    #[test]
    fn crediting_a_leg_records_both_sides() {
        let mut l = ledger();
        l.credit_leg(34_000, 5_100, Uuid::new_v4(), Uuid::new_v4());

        assert_eq!(l.entries.len(), 2);
        assert_eq!(l.balance_cents, 28_900);
        assert!(l.entries.iter().any(|e| e.kind == EntryKind::GoodsCredit && e.amount_cents == 34_000));
        assert!(l.entries.iter().any(|e| e.kind == EntryKind::CommissionDebit && e.amount_cents == -5_100));
    }

    #[test]
    fn the_balance_always_equals_the_sum_of_entries() {
        let mut l = ledger();
        l.credit_leg(34_000, 5_100, Uuid::new_v4(), Uuid::new_v4());
        l.credit_leg(12_000, 1_800, Uuid::new_v4(), Uuid::new_v4());
        l.record_payout(20_000, Some("batch-1".into()));

        assert_eq!(
            l.balance_cents,
            l.recomputed_balance_cents(),
            "the denormalised balance must match the entry log",
        );
    }

    /// Append-only: a correction is a new compensating entry, never a mutation.
    #[test]
    fn an_adjustment_appends_rather_than_editing_history() {
        let mut l = ledger();
        l.credit_leg(34_000, 5_100, Uuid::new_v4(), Uuid::new_v4());
        let before = l.entries.len();

        l.adjust(-1_000, "overcharge correction".into());

        assert_eq!(l.entries.len(), before + 1, "history grows, never shrinks");
        assert_eq!(l.balance_cents, 27_900);
    }

    #[test]
    fn a_payout_reduces_the_balance() {
        let mut l = ledger();
        l.credit_leg(10_000, 1_000, Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(l.balance_cents, 9_000);
        l.record_payout(9_000, None);
        assert_eq!(l.balance_cents, 0);
    }

    /// Every entry carries its own version bump, so two concurrent pickups
    /// crediting the same vendor cannot both write from the same version and
    /// silently lose one — the same lost-update the basket lock prevents.
    #[test]
    fn every_entry_advances_the_version() {
        let mut l = ledger();
        assert_eq!(l.version, 0);

        l.credit_leg(10_000, 1_000, Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(l.version, 2, "a leg is two entries, so two bumps");

        l.record_payout(5_000, None);
        assert_eq!(l.version, 3);
    }

    /// A leg credit nets to exactly the payout the settlement computed. If these
    /// two ever disagree, the vendor is paid something other than what the order
    /// said they were owed.
    #[test]
    fn a_leg_credit_nets_to_the_settlement_payout() {
        use crate::domain::entities::VendorLeg;

        let leg = VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), 34_000, 1500);
        let mut l = ledger();
        l.credit_leg(leg.goods_subtotal_cents, leg.commission_cents, Uuid::new_v4(), leg.id);

        assert_eq!(l.balance_cents, leg.payout_cents);
    }
}
