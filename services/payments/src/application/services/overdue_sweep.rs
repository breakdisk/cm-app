//! Moves unpaid invoices to `overdue` and announces each whole day they stay
//! overdue on `logisticos.payments.invoice.overdue`.
//!
//! Two steps per tick, each a bounded batch:
//! 1. **Transition** `issued` invoices whose `due_at` has passed to `overdue`.
//! 2. **Notify**: for each `overdue` invoice whose whole-days-overdue `d`
//!    exceeds `overdue_days_notified`, claim `prev -> d` with a compare-and-set,
//!    publish `InvoiceOverdue { days_overdue: d, previous_days_overdue: prev }`,
//!    and put the claim back (`d -> prev`) if the publish fails so the next tick
//!    retries. The published pair is exactly the claimed transition, which is
//!    what business-logic's `PaymentOverdue` rule keys on
//!    (`previous < days <= days_overdue`).
//!
//! A crash between claim and publish loses that one notice (at-most-once per
//! day-step); a failed publish never does.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use logisticos_events::{envelope::Event, payloads::InvoiceOverdue, producer::KafkaProducer, topics};
use uuid::Uuid;

use crate::domain::entities::Invoice;

const BATCH: i64 = 500;

/// Whole days between `due_at` and `now`, floored; 0 when not yet due.
pub fn days_overdue(due_at: DateTime<Utc>, now: DateTime<Utc>) -> i32 {
    let days = (now - due_at).num_days();
    i32::try_from(days.max(0)).unwrap_or(i32::MAX)
}

/// Whether a notice is owed: the invoice has been overdue for more whole days
/// than were last announced.
pub fn notice_owed(days: i32, previously_notified: i32) -> bool {
    days > previously_notified
}

#[async_trait]
pub trait OverdueStore: Send + Sync {
    /// `issued` and `due_at < now` -> `overdue`. Returns rows moved.
    async fn mark_issued_past_due(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<u64>;
    /// Overdue invoices that may owe a notice, with their `overdue_days_notified`.
    async fn list_overdue_awaiting_notice(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> anyhow::Result<Vec<(Invoice, i32)>>;
    /// `UPDATE .. SET overdue_days_notified = new WHERE id AND = expected`.
    /// True when this caller's update applied.
    async fn set_notified_days(&self, id: Uuid, expected: i32, new: i32) -> anyhow::Result<bool>;
}

#[async_trait]
pub trait OverdueNotifier: Send + Sync {
    async fn publish(&self, tenant_id: Uuid, payload: InvoiceOverdue) -> anyhow::Result<()>;
}

pub struct KafkaOverdueNotifier(pub Arc<KafkaProducer>);

#[async_trait]
impl OverdueNotifier for KafkaOverdueNotifier {
    async fn publish(&self, tenant_id: Uuid, payload: InvoiceOverdue) -> anyhow::Result<()> {
        let ev = Event::new("logisticos/payments", "invoice.overdue", tenant_id, payload);
        self.0.publish_event(topics::INVOICE_OVERDUE, &ev).await
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SweepOutcome {
    pub moved_to_overdue: u64,
    pub notified:         usize,
    pub publish_failed:   usize,
}

pub struct OverdueSweep {
    store:    Arc<dyn OverdueStore>,
    notifier: Arc<dyn OverdueNotifier>,
}

impl OverdueSweep {
    pub fn new(store: Arc<dyn OverdueStore>, notifier: Arc<dyn OverdueNotifier>) -> Self {
        Self { store, notifier }
    }

    pub async fn run_once(&self, now: DateTime<Utc>) -> anyhow::Result<SweepOutcome> {
        let mut out = SweepOutcome {
            moved_to_overdue: self.store.mark_issued_past_due(now, BATCH).await?,
            ..Default::default()
        };

        for (inv, prev) in self.store.list_overdue_awaiting_notice(now, BATCH).await? {
            let id = inv.id.inner();
            let d = days_overdue(inv.due_at, now);
            if !notice_owed(d, prev) {
                continue;
            }
            // Lost the race to another sweeper (or a rollback): not ours to send.
            match self.store.set_notified_days(id, prev, d).await {
                Ok(true) => {}
                Ok(false) => continue,
                Err(e) => {
                    tracing::error!(err = %e, invoice_id = %id, "overdue claim failed");
                    continue;
                }
            }
            let payload = InvoiceOverdue {
                invoice_id:            id,
                merchant_id:           inv.merchant_id.inner(),
                amount_cents:          inv.total_due().amount,
                currency:              inv.currency.to_string(),
                due_at:                inv.due_at,
                days_overdue:          d,
                previous_days_overdue: prev,
            };
            match self.notifier.publish(inv.tenant_id.inner(), payload).await {
                Ok(()) => out.notified += 1,
                Err(e) => {
                    out.publish_failed += 1;
                    tracing::error!(err = %e, invoice_id = %id, days = d,
                        "invoice.overdue publish failed - releasing claim for retry");
                    if let Err(e2) = self.store.set_notified_days(id, d, prev).await {
                        tracing::error!(err = %e2, invoice_id = %id,
                            "could not release overdue claim - this notice is lost");
                    }
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use logisticos_types::{
        awb::{Awb, ServiceCode, TenantCode},
        invoice::{ChargeType, InvoiceNumber, InvoiceType},
        Currency, MerchantId, Money, TenantId,
    };
    use std::sync::Mutex;

    use crate::domain::entities::{BillingPeriod, InvoiceLineItem, InvoiceStatus};

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn day_computation_boundaries() {
        let due = t0();
        assert_eq!(days_overdue(due, due), 0);
        assert_eq!(days_overdue(due, due + Duration::hours(23) + Duration::minutes(59)), 0);
        assert_eq!(days_overdue(due, due + Duration::hours(24)), 1);
        assert_eq!(days_overdue(due, due + Duration::hours(47) + Duration::minutes(59)), 1);
        assert_eq!(days_overdue(due, due + Duration::hours(48)), 2);
        assert_eq!(days_overdue(due, due - Duration::days(3)), 0, "not yet due never goes negative");
        assert_eq!(days_overdue(due, due + Duration::days(100_000)), 100_000);
    }

    #[test]
    fn notice_owed_only_when_days_advance() {
        assert!(notice_owed(0, -1));
        assert!(notice_owed(1, 0));
        assert!(notice_owed(5, 2));
        assert!(!notice_owed(1, 1));
        assert!(!notice_owed(1, 2));
    }

    fn invoice(due_at: DateTime<Utc>, status: InvoiceStatus) -> Invoice {
        let period = chrono::NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let num = InvoiceNumber::generate(InvoiceType::ShipmentCharges, "PH1", period, 1).unwrap();
        let mut inv = Invoice::new(
            num,
            InvoiceType::ShipmentCharges,
            TenantId::new(),
            MerchantId::new(),
            None,
            BillingPeriod::monthly(2026, 4),
            Currency::PHP,
        );
        let awb = Awb::generate(&TenantCode::new("PH1").unwrap(), ServiceCode::Standard, 1);
        inv.add_line_item(InvoiceLineItem::for_awb(
            ChargeType::BaseFreight, awb, "Freight".into(), 1, Money::new(10_000, Currency::PHP),
        ))
        .unwrap();
        inv.issue().unwrap();
        inv.due_at = due_at;
        inv.status = status;
        inv
    }

    /// The SQL rule (`status = 'issued' AND due_at < now`) and the domain rule
    /// must agree on the boundary, evaluated here with the same `now`.
    #[test]
    fn sql_rule_agrees_with_domain_rule_at_the_boundary() {
        let sql_rule =
            |status: &str, due: DateTime<Utc>, now: DateTime<Utc>| status == "issued" && due < now;
        let now = t0();
        for (due, expect) in [
            (now, false),
            (now + Duration::seconds(1), false),
            (now - Duration::seconds(1), true),
        ] {
            let inv = invoice(due, InvoiceStatus::Issued);
            assert_eq!(inv.is_past_due(now), expect, "domain, due={due}");
            assert_eq!(sql_rule("issued", due, now), expect, "sql, due={due}");
        }
        let paid = invoice(now - Duration::days(9), InvoiceStatus::Paid);
        assert!(!paid.is_past_due(now));
        assert!(!sql_rule("paid", paid.due_at, now));
    }

    struct FakeStore {
        rows: Mutex<Vec<(Invoice, i32)>>,
        /// Number of upcoming `set_notified_days` calls that lose the race.
        lose_next_claims: Mutex<usize>,
    }
    impl FakeStore {
        fn with(rows: Vec<(Invoice, i32)>) -> Arc<Self> {
            Arc::new(Self { rows: Mutex::new(rows), lose_next_claims: Mutex::new(0) })
        }
        fn notified(&self, i: usize) -> i32 {
            self.rows.lock().unwrap()[i].1
        }
    }
    #[async_trait]
    impl OverdueStore for FakeStore {
        async fn mark_issued_past_due(&self, now: DateTime<Utc>, _l: i64) -> anyhow::Result<u64> {
            let mut n = 0;
            for (inv, _) in self.rows.lock().unwrap().iter_mut() {
                if inv.is_past_due(now) {
                    inv.status = InvoiceStatus::Overdue;
                    n += 1;
                }
            }
            Ok(n)
        }
        async fn list_overdue_awaiting_notice(
            &self,
            now: DateTime<Utc>,
            _l: i64,
        ) -> anyhow::Result<Vec<(Invoice, i32)>> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .filter(|(i, p)| i.status == InvoiceStatus::Overdue && *p < days_overdue(i.due_at, now))
                .cloned()
                .collect())
        }
        async fn set_notified_days(&self, id: Uuid, expected: i32, new: i32) -> anyhow::Result<bool> {
            {
                let mut lose = self.lose_next_claims.lock().unwrap();
                if *lose > 0 {
                    *lose -= 1;
                    return Ok(false);
                }
            }
            let mut rows = self.rows.lock().unwrap();
            for (inv, p) in rows.iter_mut() {
                if inv.id.inner() == id && *p == expected {
                    *p = new;
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }

    #[derive(Default)]
    struct FakeNotifier {
        sent: Mutex<Vec<(Uuid, InvoiceOverdue)>>,
        fail: Mutex<bool>,
    }
    #[async_trait]
    impl OverdueNotifier for FakeNotifier {
        async fn publish(&self, tenant: Uuid, p: InvoiceOverdue) -> anyhow::Result<()> {
            if *self.fail.lock().unwrap() {
                anyhow::bail!("broker down");
            }
            self.sent.lock().unwrap().push((tenant, p));
            Ok(())
        }
    }

    #[tokio::test]
    async fn issued_past_due_becomes_overdue_and_notice_is_the_claimed_transition() {
        let now = t0();
        let inv = invoice(now - Duration::hours(50), InvoiceStatus::Issued);
        let tenant = inv.tenant_id.inner();
        let store = FakeStore::with(vec![(inv, -1)]);
        let notifier = Arc::new(FakeNotifier::default());
        let sweep = OverdueSweep::new(store.clone(), notifier.clone());

        let out = sweep.run_once(now).await.unwrap();
        assert_eq!(out, SweepOutcome { moved_to_overdue: 1, notified: 1, publish_failed: 0 });
        {
            let sent = notifier.sent.lock().unwrap();
            assert_eq!(sent[0].0, tenant, "envelope tenant is the invoice tenant");
            assert_eq!((sent[0].1.previous_days_overdue, sent[0].1.days_overdue), (-1, 2));
            assert_eq!(sent[0].1.amount_cents, 11_200, "10000 + 12% VAT");
            assert_eq!(sent[0].1.currency, "PHP");
        }
        assert_eq!(store.notified(0), 2);

        // Same day again: nothing owed.
        let out = sweep.run_once(now + Duration::hours(1)).await.unwrap();
        assert_eq!((out.moved_to_overdue, out.notified), (0, 0));
        // Next whole day: 2 -> 3.
        let out = sweep.run_once(now + Duration::hours(24)).await.unwrap();
        assert_eq!(out.notified, 1);
        let sent = notifier.sent.lock().unwrap();
        assert_eq!((sent[1].1.previous_days_overdue, sent[1].1.days_overdue), (2, 3));
    }

    #[tokio::test]
    async fn publish_failure_puts_the_claim_back_and_next_tick_retries() {
        let now = t0();
        let store = FakeStore::with(vec![(invoice(now - Duration::days(1), InvoiceStatus::Overdue), -1)]);
        let notifier = Arc::new(FakeNotifier::default());
        let sweep = OverdueSweep::new(store.clone(), notifier.clone());

        *notifier.fail.lock().unwrap() = true;
        let out = sweep.run_once(now).await.unwrap();
        assert_eq!((out.notified, out.publish_failed), (0, 1));
        assert_eq!(store.notified(0), -1, "claim rolled back to prev");

        *notifier.fail.lock().unwrap() = false;
        let out = sweep.run_once(now).await.unwrap();
        assert_eq!(out.notified, 1);
        assert_eq!(store.notified(0), 1);
        let sent = notifier.sent.lock().unwrap();
        assert_eq!((sent[0].1.previous_days_overdue, sent[0].1.days_overdue), (-1, 1));
    }

    #[tokio::test]
    async fn losing_the_claim_publishes_nothing() {
        let now = t0();
        let store = FakeStore::with(vec![(invoice(now - Duration::days(2), InvoiceStatus::Overdue), -1)]);
        *store.lose_next_claims.lock().unwrap() = 1;
        let notifier = Arc::new(FakeNotifier::default());
        let out = OverdueSweep::new(store, notifier.clone()).run_once(now).await.unwrap();
        assert_eq!(out.notified, 0);
        assert!(notifier.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn paid_invoice_is_never_touched() {
        let now = t0();
        let store = FakeStore::with(vec![(invoice(now - Duration::days(5), InvoiceStatus::Paid), -1)]);
        let notifier = Arc::new(FakeNotifier::default());
        let out = OverdueSweep::new(store, notifier.clone()).run_once(now).await.unwrap();
        assert_eq!(out, SweepOutcome::default());
        assert!(notifier.sent.lock().unwrap().is_empty());
    }
}
