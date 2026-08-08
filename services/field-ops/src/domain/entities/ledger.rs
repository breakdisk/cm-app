//! Courier earnings ledger.
//!
//! Same append-only shape as OmniDeliv's vendor ledger and the platform's
//! DriverLedger: signed entries, a denormalised balance that is always their
//! sum, corrections by compensating entry.
//!
//! Platform tier, so it is deliberately product-agnostic: `external_ref` holds
//! whatever job id the crediting product uses and this module never interprets
//! it, exactly as `courier_assignments` does.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CourierLedgerStatus { Open, Closed, Settled }

impl CourierLedgerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CourierLedgerStatus::Open    => "open",
            CourierLedgerStatus::Closed  => "closed",
            CourierLedgerStatus::Settled => "settled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CourierEntryKind {
    TripEarning,
    Tip,
    Adjustment,
    Payout,
    /// Cash taken at the door. Recorded **negative**: the courier is holding
    /// the platform's money, so it reduces what the platform owes them.
    CodCollected,
    /// That cash handed back. Positive, clearing the debt.
    CodRemitted,
}

impl CourierEntryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CourierEntryKind::TripEarning => "trip_earning",
            CourierEntryKind::Tip         => "tip",
            CourierEntryKind::Adjustment  => "adjustment",
            CourierEntryKind::Payout      => "payout",
            CourierEntryKind::CodCollected => "cod_collected",
            CourierEntryKind::CodRemitted  => "cod_remitted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourierLedgerEntry {
    pub id:           Uuid,
    pub ledger_id:    Uuid,
    pub kind:         CourierEntryKind,
    /// Signed. Credits positive, payouts negative, so the balance is a plain
    /// sum and cannot disagree with the log.
    pub amount_cents: i64,
    pub external_ref: Option<Uuid>,
    pub reference:    Option<String>,
    pub created_at:   DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourierLedger {
    pub id:            Uuid,
    pub tenant_id:     Uuid,
    pub courier_id:    Uuid,
    pub period:        String,
    pub status:        CourierLedgerStatus,
    pub balance_cents: i64,
    pub version:       i64,
    pub entries:       Vec<CourierLedgerEntry>,
    pub created_at:    DateTime<Utc>,
    pub updated_at:    DateTime<Utc>,
}

impl CourierLedger {
    pub fn open(tenant_id: Uuid, courier_id: Uuid, period: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            courier_id,
            period,
            status: CourierLedgerStatus::Open,
            balance_cents: 0,
            version: 0,
            entries: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// The only way an entry enters the log, and the only place the balance
    /// moves — so no future method can post without moving the balance, or move
    /// the balance without leaving a record of why.
    fn append(&mut self, kind: CourierEntryKind, amount_cents: i64,
              external_ref: Option<Uuid>, reference: Option<String>) {
        self.entries.push(CourierLedgerEntry {
            id: Uuid::new_v4(),
            ledger_id: self.id,
            kind,
            amount_cents,
            external_ref,
            reference,
            created_at: Utc::now(),
        });
        self.balance_cents += amount_cents;
        self.version += 1;
        self.updated_at = Utc::now();
    }

    /// Credit one consolidated trip.
    ///
    /// `stops` is recorded for reporting only — the earning is per trip. Paying
    /// per stop would make every additional pickup a cost, which would remove
    /// the margin that makes consolidation worth doing.
    pub fn credit_trip(&mut self, amount_cents: i64, stops: usize, external_ref: Uuid) {
        self.append(
            CourierEntryKind::TripEarning,
            amount_cents,
            Some(external_ref),
            Some(format!("{stops} stops")),
        );
    }

    /// The tip reaches the courier in full — never a Partner revenue line.
    pub fn credit_tip(&mut self, amount_cents: i64, external_ref: Uuid) {
        self.append(CourierEntryKind::Tip, amount_cents, Some(external_ref), None);
    }

    /// Cash collected at the door.
    ///
    /// Negative on purpose. The ledger balance answers one question — *what
    /// does the platform owe this courier right now* — and a courier holding
    /// ₱389 of our cash while having earned ₱35 is ₱354 in our debt, not ₱424
    /// in credit. Signing it the other way would make the balance the sum of
    /// two unrelated things and quietly overpay at payout time.
    pub fn record_cod_collected(&mut self, amount_cents: i64, external_ref: Uuid) {
        self.append(CourierEntryKind::CodCollected, -amount_cents, Some(external_ref), None);
    }

    /// Cash handed back to the platform, clearing that debt.
    pub fn record_cod_remitted(&mut self, amount_cents: i64, reference: Option<String>) {
        self.append(CourierEntryKind::CodRemitted, amount_cents, None, reference);
    }

    /// What the courier still owes, or 0 when they are in credit.
    ///
    /// Derived from the log rather than stored: a second column tracking "cash
    /// held" would be a copy of the same fact that can drift from it.
    pub fn cash_held_cents(&self) -> i64 {
        let net: i64 = self
            .entries
            .iter()
            .filter(|e| matches!(e.kind, CourierEntryKind::CodCollected | CourierEntryKind::CodRemitted))
            .map(|e| e.amount_cents)
            .sum();
        // net is <= 0 while cash is outstanding; report it as a positive debt.
        if net < 0 { -net } else { 0 }
    }

    pub fn record_payout(&mut self, amount_cents: i64, batch: Option<String>) {
        self.append(CourierEntryKind::Payout, -amount_cents, None, batch);
    }

    pub fn adjust(&mut self, amount_cents: i64, reason: String) {
        self.append(CourierEntryKind::Adjustment, amount_cents, None, Some(reason));
    }

    pub fn is_open(&self) -> bool { self.status == CourierLedgerStatus::Open }

    /// Recompute the balance from the log. Any divergence from
    /// `balance_cents` means an entry was written by something other than
    /// `append`, which is the failure this shape exists to prevent.
    pub fn recomputed_balance_cents(&self) -> i64 {
        self.entries.iter().map(|e| e.amount_cents).sum()
    }
}

#[cfg(test)]
mod cod_accounting {
    use super::*;

    fn ledger() -> CourierLedger {
        CourierLedger::open(Uuid::new_v4(), Uuid::new_v4(), "2026-W32".into())
    }

    /// The whole point of the sign convention. A courier who earned ₱35 and is
    /// holding ₱389 of our cash is ₱354 in our debt — not ₱424 in credit. Get
    /// this backwards and a payout run hands them the customer's money again.
    #[test]
    fn collecting_cash_puts_the_courier_in_debt_not_credit() {
        let job = Uuid::new_v4();
        let mut l = ledger();

        l.credit_trip(3_500, 1, job);
        l.record_cod_collected(38_900, job);

        assert_eq!(l.balance_cents, 3_500 - 38_900);
        assert!(l.balance_cents < 0, "holding our cash cannot read as credit");
        assert_eq!(l.cash_held_cents(), 38_900);
    }

    #[test]
    fn remitting_clears_the_debt_and_leaves_the_earning() {
        let job = Uuid::new_v4();
        let mut l = ledger();

        l.credit_trip(3_500, 1, job);
        l.record_cod_collected(38_900, job);
        l.record_cod_remitted(38_900, Some("slip-001".into()));

        assert_eq!(l.cash_held_cents(), 0);
        assert_eq!(l.balance_cents, 3_500, "what is left is what we owe them");
    }

    #[test]
    fn a_partial_remittance_leaves_the_remainder_outstanding() {
        let job = Uuid::new_v4();
        let mut l = ledger();

        l.record_cod_collected(38_900, job);
        l.record_cod_remitted(20_000, None);

        assert_eq!(l.cash_held_cents(), 18_900);
    }

    /// Over-remitting is allowed and must not read as negative cash held —
    /// it happens (an over-payment, or cash handed over before a delivery
    /// settles), and a negative "held" would subtract from the next debt.
    #[test]
    fn over_remitting_reports_no_cash_held_rather_than_a_negative() {
        let mut l = ledger();
        l.record_cod_collected(10_000, Uuid::new_v4());
        l.record_cod_remitted(15_000, None);

        assert_eq!(l.cash_held_cents(), 0);
        assert_eq!(l.balance_cents, 5_000);
    }

    /// `cash_held_cents` must read only the COD entries. Counting earnings
    /// would make a well-paid courier look like they owed less cash.
    #[test]
    fn earnings_do_not_offset_cash_held() {
        let job = Uuid::new_v4();
        let mut l = ledger();

        l.credit_trip(50_000, 1, job);
        l.credit_tip(5_000, job);
        l.record_cod_collected(10_000, job);

        assert_eq!(l.cash_held_cents(), 10_000, "earnings are not a remittance");
        assert_eq!(l.balance_cents, 45_000);
    }

    /// The append-only guard still holds with the new kinds.
    #[test]
    fn the_balance_still_equals_the_log() {
        let job = Uuid::new_v4();
        let mut l = ledger();
        l.credit_trip(3_500, 2, job);
        l.record_cod_collected(38_900, job);
        l.record_cod_remitted(38_900, None);
        l.record_payout(3_500, Some("batch-1".into()));

        assert_eq!(l.balance_cents, l.recomputed_balance_cents());
        assert_eq!(l.balance_cents, 0, "paid out and remitted leaves nothing owed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ledger() -> CourierLedger {
        CourierLedger::open(Uuid::new_v4(), Uuid::new_v4(), "2026-08-06".into())
    }

    /// The courier is paid per consolidated trip, not per stop. A three-vendor
    /// route earns one trip entry — which is precisely why consolidation is
    /// profitable rather than merely cheaper for the customer.
    #[test]
    fn a_consolidated_trip_earns_one_entry_regardless_of_stops() {
        let mut l = ledger();
        l.credit_trip(5_800, 3, Uuid::new_v4());

        let trips = l.entries.iter().filter(|e| e.kind == CourierEntryKind::TripEarning).count();
        assert_eq!(trips, 1, "three stops, one trip earning");
        assert_eq!(l.balance_cents, 5_800);
    }

    /// The margin lever from the courier's side: a one-stop and a three-stop
    /// route at the same trip rate pay the same, which is what makes the extra
    /// commission leg pure margin rather than a cost transfer.
    #[test]
    fn stop_count_does_not_change_the_earning() {
        let mut one = ledger();
        one.credit_trip(5_800, 1, Uuid::new_v4());

        let mut three = ledger();
        three.credit_trip(5_800, 3, Uuid::new_v4());

        assert_eq!(one.balance_cents, three.balance_cents);
    }

    /// The tip goes to the courier in full — it is never a Partner revenue line.
    #[test]
    fn the_whole_tip_reaches_the_courier() {
        let mut l = ledger();
        l.credit_trip(5_800, 2, Uuid::new_v4());
        l.credit_tip(4_000, Uuid::new_v4());
        assert_eq!(l.balance_cents, 5_800 + 4_000);
    }

    #[test]
    fn the_balance_always_equals_the_sum_of_entries() {
        let mut l = ledger();
        l.credit_trip(5_800, 2, Uuid::new_v4());
        l.credit_tip(4_000, Uuid::new_v4());
        l.record_payout(6_000, None);

        assert_eq!(l.balance_cents, l.recomputed_balance_cents());
    }

    #[test]
    fn an_adjustment_appends_rather_than_editing_history() {
        let mut l = ledger();
        l.credit_trip(5_800, 1, Uuid::new_v4());
        let before = l.entries.len();
        l.adjust(-500, "route shortened".into());
        assert_eq!(l.entries.len(), before + 1);
        assert_eq!(l.balance_cents, 5_300);
    }

    /// The credited trip plus tip is exactly what the order's settlement said
    /// courier_earnings_cents was. If these diverge, the courier is paid
    /// something other than what the customer was charged for delivery.
    #[test]
    fn trip_plus_tip_matches_the_settlement_courier_leg() {
        let (trip, tip) = (5_800_i64, 4_000_i64);
        let mut l = ledger();
        let order = Uuid::new_v4();
        l.credit_trip(trip, 2, order);
        l.credit_tip(tip, order);

        // Mirrors Order::settlement's courier_earnings_cents = trip + tip.
        assert_eq!(l.balance_cents, trip + tip);
    }
}
