//! Selling, renewing and expiring a tenant's plan.
//!
//! One rule runs through all of it: **the tenant never names their own tier.**
//! `checkout` takes a tier and an interval and turns them into a price from the
//! catalogue and a hosted payment page; nothing grants anything. The only code
//! that raises a tenant's entitlement is `apply_captured_payment`, reached only
//! from a capture webhook, and the only code that lowers it is the sweep.
//!
//! Renewal is by notice, not by automatic charge. Charging on a schedule needs
//! a stored credential the platform can use without the cardholder present, and
//! the Network International adapter has no such capability — `create_session`
//! returns a one-shot hosted page and nothing stores a token. So a subscription
//! nearing its end publishes `subscription.renewal_due` for engagement to act
//! on, and the tenant pays the next period the way they paid the first. The
//! seam for stored credentials is `SubscriptionPlan::period_days` plus this
//! service's checkout path; adding auto-charge later means adding a gateway
//! capability, not restructuring this.

use std::sync::Arc;

use chrono::Utc;
use logisticos_events::{envelope::Event, producer::KafkaProducer, topics};
use uuid::Uuid;

use crate::domain::{
    entities::{
        subscription::{RENEWAL_NOTICE_DAYS, GRACE_PERIOD_DAYS},
        BillingInterval, Subscription, SubscriptionPlan, SubscriptionStatus,
    },
    repositories::SubscriptionRepository,
};
use crate::infrastructure::external::identity_client::TenantTierSync;

use super::payment_intent_service::{CreateIntentCommand, PaymentIntentService};
use crate::domain::repositories::payment_gateway::PaymentAction;

/// The tag stamped on every subscription intent, and what the capture consumer
/// filters on. Shares `payment.intent.*` with shipping fees, OmniDeliv orders
/// and marketplace bookings.
pub const SUBSCRIPTION_PURPOSE: &str = "subscription";

#[derive(Debug, thiserror::Error)]
pub enum SubscriptionError {
    /// Starter is free and Enterprise is quoted by hand. Neither is sellable
    /// through a self-serve checkout, and neither has a plan row.
    #[error("{0}")]
    NotSelfServe(String),
    #[error("no {interval} plan for tier {tier} in {currency}")]
    NoPlan { tier: String, interval: String, currency: String },
    #[error("online card payment is not configured for this deployment")]
    PaymentsUnavailable,
    #[error("no subscription to change")]
    NoSubscription,
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// A subscription and, when one is needed, where to go and pay for it.
#[derive(Debug)]
pub struct SubscriptionCheckout {
    pub subscription: Subscription,
    pub checkout_url: String,
}

pub struct SubscriptionService {
    repo:     Arc<dyn SubscriptionRepository>,
    intents:  Option<Arc<PaymentIntentService>>,
    tiers:    Option<Arc<dyn TenantTierSync>>,
    kafka:    Arc<KafkaProducer>,
    return_url_base: String,
}

impl SubscriptionService {
    pub fn new(
        repo: Arc<dyn SubscriptionRepository>,
        intents: Option<Arc<PaymentIntentService>>,
        tiers: Option<Arc<dyn TenantTierSync>>,
        kafka: Arc<KafkaProducer>,
        return_url_base: String,
    ) -> Self {
        Self { repo, intents, tiers, kafka, return_url_base }
    }

    pub async fn list_plans(&self, currency: &str) -> anyhow::Result<Vec<SubscriptionPlan>> {
        self.repo.list_plans(currency).await
    }

    pub async fn current(&self, tenant_id: Uuid) -> anyhow::Result<Option<Subscription>> {
        self.repo.find_live_for_tenant(tenant_id).await
    }

    /// Start paying for a plan, or change to a different one.
    ///
    /// Returns a hosted card page. Nothing about the tenant's entitlement moves
    /// here — see `apply_captured_payment`, which is the only thing that raises
    /// a tier and which only a capture webhook reaches.
    ///
    /// An existing live subscription is re-pointed at the new plan rather than
    /// duplicated: the unique index allows exactly one live row per tenant, and
    /// a tenant switching from Growth to Business is changing what they buy,
    /// not buying a second thing. The change takes effect when the payment
    /// lands, so a tenant who abandons the checkout keeps what they had —
    /// `tier` on a `pending_payment` row grants nothing.
    pub async fn checkout(
        &self,
        tenant_id: Uuid,
        tier: &str,
        interval: BillingInterval,
        currency: &str,
    ) -> Result<SubscriptionCheckout, SubscriptionError> {
        match tier {
            "starter" => return Err(SubscriptionError::NotSelfServe(
                "Starter is free — there is nothing to pay for. Cancel your current plan to \
                 return to it.".into(),
            )),
            "enterprise" => return Err(SubscriptionError::NotSelfServe(
                "Enterprise is priced per deployment — contact sales rather than checking out."
                    .into(),
            )),
            _ => {}
        }

        let plan = self
            .repo
            .find_plan(tier, interval, currency)
            .await?
            .ok_or_else(|| SubscriptionError::NoPlan {
                tier: tier.to_string(),
                interval: interval.as_str().to_string(),
                currency: currency.to_string(),
            })?;

        let intents = self.intents.as_ref().ok_or(SubscriptionError::PaymentsUnavailable)?;

        let mut sub = match self.repo.find_live_for_tenant(tenant_id).await? {
            Some(mut existing) => {
                existing.plan_id      = plan.id;
                existing.tier         = plan.tier.clone();
                existing.currency     = plan.currency.clone();
                existing.amount_cents = plan.amount_cents;
                existing.updated_at   = Utc::now();
                existing
            }
            None => Subscription::new(tenant_id, &plan),
        };

        // Persisted before the gateway call: `create_intent` stamps this
        // subscription's id as the intent's reference, and the capture webhook
        // can arrive before this function returns. A consumer looking up a row
        // that does not exist yet would drop a real payment on the floor.
        self.repo.save(&sub).await?;

        let created = intents
            .create_intent(CreateIntentCommand {
                tenant_id,
                purpose:        SUBSCRIPTION_PURPOSE.to_string(),
                reference_type: "subscription".to_string(),
                reference_id:   sub.id,
                amount_cents:   plan.amount_cents,
                currency:       plan.currency.clone(),
                return_url:     format!(
                    "{}?subscription_id={}",
                    self.return_url_base.trim_end_matches('/'),
                    sub.id,
                ),
                // Sale, not authorize. There is no third party to wait on — the
                // platform is the one providing the service, and it is providing
                // it the moment the payment lands. The authorize-then-capture
                // shape the other products use exists because a courier or a
                // carrier might say no.
                action: PaymentAction::Sale,
            })
            .await
            .map_err(|e| SubscriptionError::Other(anyhow::anyhow!("{e}")))?;

        sub.updated_at = Utc::now();
        self.repo.save(&sub).await?;

        Ok(SubscriptionCheckout { subscription: sub, checkout_url: created.checkout_url })
    }

    /// Stop at the end of the paid period. Not a refund: the tenant keeps the
    /// tier they already bought until it runs out, and the sweep lapses it then.
    pub async fn cancel(&self, tenant_id: Uuid) -> Result<Subscription, SubscriptionError> {
        let mut sub = self
            .repo
            .find_live_for_tenant(tenant_id)
            .await?
            .ok_or(SubscriptionError::NoSubscription)?;

        sub.cancel(Utc::now()).map_err(|e| SubscriptionError::Rejected(e.to_string()))?;
        self.repo.save(&sub).await?;
        Ok(sub)
    }

    /// Money landed for `subscription_id` — start or extend the paid period.
    ///
    /// Idempotent on the intent id, because Kafka is at-least-once and a
    /// redelivered capture would otherwise be a free period every replay.
    pub async fn apply_captured_payment(
        &self,
        subscription_id: Uuid,
        intent_id: Uuid,
    ) -> anyhow::Result<()> {
        let Some(mut sub) = self.repo.find_by_id(subscription_id).await? else {
            // Not an error: a purged row or an old partition replay. Logged so
            // a systematic mismatch is visible rather than silently dropped.
            tracing::warn!(%subscription_id, "payment captured for an unknown subscription");
            return Ok(());
        };

        let plan = self.repo.find_plan_by_id(sub.plan_id).await?;
        let period_days = plan.as_ref().map(|p| p.period_days).unwrap_or(30);

        if !sub.activate_from_payment(intent_id, period_days, Utc::now()) {
            return Ok(()); // already applied
        }
        self.repo.save(&sub).await?;

        // Best effort here, durable in the row. A failure leaves
        // `tier_synced_at` NULL and `sweep_tier_sync` retries — the tenant has
        // paid, so the entitlement is owed whether or not identity answered
        // this second.
        self.sync_tier(&sub).await;

        let ev = Event::new(
            "logisticos/payments", "subscription.activated", sub.tenant_id,
            serde_json::json!({
                "subscription_id":    sub.id,
                "tenant_id":          sub.tenant_id,
                "tier":               sub.tier,
                "current_period_end": sub.current_period_end,
                "amount_cents":       sub.amount_cents,
                "currency":           sub.currency,
            }),
        );
        if let Err(e) = self.kafka.publish_event(topics::SUBSCRIPTION_ACTIVATED, &ev).await {
            tracing::warn!(err = %e, subscription_id = %sub.id,
                "subscription.activated publish failed (the subscription is still active)");
        }
        Ok(())
    }

    /// One pass of renewal notices and expiry. Returns how many subscriptions
    /// changed state, so the log line means something.
    ///
    /// One bad row is logged and skipped rather than aborting the batch: a
    /// sweep that stops at the first failure leaves every later tenant either
    /// un-notified or entitled to a tier they stopped paying for.
    pub async fn sweep(&self) -> anyhow::Result<usize> {
        const BATCH: i64 = 500;
        let now = Utc::now();
        let mut changed = 0;

        for mut sub in self.repo.list_due_for_sweep(BATCH).await? {
            let id = sub.id;

            if sub.renewal_notice_due(now) {
                self.publish_renewal_due(&sub).await;
                sub.renewal_notice_sent_at = Some(now);
                sub.updated_at = now;
                if let Err(e) = self.repo.save(&sub).await {
                    tracing::error!(err = %e, subscription_id = %id, "renewal notice save failed");
                }
                changed += 1;
                continue;
            }

            // Grace exhausted, or a cancelled subscription reached its end.
            // Checked before `period_ended` so a subscription that has been
            // past-due for longer than the window is lapsed rather than
            // re-marked past-due forever.
            let cancelled_and_over =
                sub.status == SubscriptionStatus::Cancelled && sub.period_ended(now);
            if sub.grace_exhausted(now) || cancelled_and_over {
                if sub.lapse(now).is_err() {
                    continue;
                }
                if let Err(e) = self.repo.save(&sub).await {
                    tracing::error!(err = %e, subscription_id = %id, "lapse save failed");
                    continue;
                }
                // Downgrade the tier the same way an upgrade is granted, and
                // leave the same durable retry marker if it fails.
                self.sync_tier(&sub).await;
                self.publish_lapsed(&sub).await;
                changed += 1;
                continue;
            }

            if sub.status == SubscriptionStatus::Active && sub.period_ended(now) {
                if sub.mark_past_due(now).is_err() {
                    continue;
                }
                if let Err(e) = self.repo.save(&sub).await {
                    tracing::error!(err = %e, subscription_id = %id, "past-due save failed");
                    continue;
                }
                // No tier change: this is the grace window, and the tenant keeps
                // what they have. Only the notice changes.
                self.publish_past_due(&sub).await;
                changed += 1;
            }
        }
        Ok(changed)
    }

    /// Retries tier grants identity never received. Without this a tenant can
    /// pay and be left on the tier they had, with the money taken and nothing
    /// in the system that would ever notice.
    pub async fn sweep_tier_sync(&self) -> anyhow::Result<usize> {
        const BATCH: i64 = 200;
        let mut synced = 0;
        for sub in self.repo.list_unsynced_tiers(BATCH).await? {
            if self.sync_tier(&sub).await {
                synced += 1;
            }
        }
        Ok(synced)
    }

    /// Tells identity the tier this subscription currently entitles the tenant
    /// to, and records that it landed. Returns whether it did.
    async fn sync_tier(&self, sub: &Subscription) -> bool {
        let Some(tiers) = self.tiers.as_ref() else {
            tracing::error!(subscription_id = %sub.id,
                "no identity client configured — the tenant has paid and the tier cannot be granted");
            return false;
        };
        match tiers.set_tier(sub.tenant_id, sub.effective_tier()).await {
            Ok(()) => {
                if let Err(e) = self.repo.mark_tier_synced(sub.id, Utc::now()).await {
                    // The grant landed; only the record of it failed. Retrying
                    // is harmless — setting the same tier twice is a no-op.
                    tracing::error!(err = %e, subscription_id = %sub.id,
                        "tier granted but the sync marker failed to save; will retry");
                    return false;
                }
                true
            }
            Err(e) => {
                tracing::error!(err = %e, subscription_id = %sub.id, tenant_id = %sub.tenant_id,
                    tier = %sub.effective_tier(),
                    "identity tier grant failed — the tenant is paid up and not entitled; will retry");
                false
            }
        }
    }

    async fn publish_renewal_due(&self, sub: &Subscription) {
        let ev = Event::new(
            "logisticos/payments", "subscription.renewal_due", sub.tenant_id,
            serde_json::json!({
                "subscription_id":    sub.id,
                "tenant_id":          sub.tenant_id,
                "tier":               sub.tier,
                "amount_cents":       sub.amount_cents,
                "currency":           sub.currency,
                "current_period_end": sub.current_period_end,
                "days_remaining":     RENEWAL_NOTICE_DAYS,
            }),
        );
        if let Err(e) = self.kafka.publish_event(topics::SUBSCRIPTION_RENEWAL_DUE, &ev).await {
            tracing::warn!(err = %e, subscription_id = %sub.id, "renewal-due publish failed");
        }
    }

    async fn publish_past_due(&self, sub: &Subscription) {
        let ev = Event::new(
            "logisticos/payments", "subscription.past_due", sub.tenant_id,
            serde_json::json!({
                "subscription_id":  sub.id,
                "tenant_id":        sub.tenant_id,
                "tier":             sub.tier,
                "grace_days_left":  GRACE_PERIOD_DAYS,
            }),
        );
        if let Err(e) = self.kafka.publish_event(topics::SUBSCRIPTION_PAST_DUE, &ev).await {
            tracing::warn!(err = %e, subscription_id = %sub.id, "past-due publish failed");
        }
    }

    async fn publish_lapsed(&self, sub: &Subscription) {
        let ev = Event::new(
            "logisticos/payments", "subscription.lapsed", sub.tenant_id,
            serde_json::json!({
                "subscription_id": sub.id,
                "tenant_id":       sub.tenant_id,
                "previous_tier":   sub.tier,
                "tier":            sub.effective_tier(),
            }),
        );
        if let Err(e) = self.kafka.publish_event(topics::SUBSCRIPTION_LAPSED, &ev).await {
            tracing::warn!(err = %e, subscription_id = %sub.id, "lapsed publish failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::subscription::GRACE_PERIOD_DAYS;
    use chrono::{DateTime, Duration};
    use std::sync::Mutex;

    // ── Fakes ─────────────────────────────────────────────────────────────

    struct FakeRepo {
        plans:    Vec<SubscriptionPlan>,
        subs:     Mutex<Vec<Subscription>>,
        synced:   Mutex<Vec<(Uuid, DateTime<Utc>)>>,
    }

    impl FakeRepo {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                plans: vec![
                    SubscriptionPlan {
                        id: Uuid::new_v4(), tier: "growth".into(),
                        interval: BillingInterval::Monthly, currency: "USD".into(),
                        amount_cents: 14900, period_days: 30, is_active: true,
                    },
                ],
                subs: Mutex::new(Vec::new()),
                synced: Mutex::new(Vec::new()),
            })
        }
        fn seed(self: &Arc<Self>, s: Subscription) -> Uuid {
            let id = s.id;
            self.subs.lock().unwrap().push(s);
            id
        }
        fn get(&self, id: Uuid) -> Subscription {
            self.subs.lock().unwrap().iter().find(|s| s.id == id).cloned().unwrap()
        }
        fn plan(&self) -> &SubscriptionPlan { &self.plans[0] }
    }

    #[async_trait::async_trait]
    impl SubscriptionRepository for FakeRepo {
        async fn find_plan(&self, tier: &str, interval: BillingInterval, currency: &str)
            -> anyhow::Result<Option<SubscriptionPlan>> {
            Ok(self.plans.iter().find(|p|
                p.tier == tier && p.interval == interval && p.currency == currency).cloned())
        }
        async fn find_plan_by_id(&self, id: Uuid) -> anyhow::Result<Option<SubscriptionPlan>> {
            Ok(self.plans.iter().find(|p| p.id == id).cloned())
        }
        async fn list_plans(&self, currency: &str) -> anyhow::Result<Vec<SubscriptionPlan>> {
            Ok(self.plans.iter().filter(|p| p.currency == currency).cloned().collect())
        }
        async fn find_live_for_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Option<Subscription>> {
            Ok(self.subs.lock().unwrap().iter()
                .find(|s| s.tenant_id == tenant_id && s.status != SubscriptionStatus::Lapsed)
                .cloned())
        }
        async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<Subscription>> {
            Ok(self.subs.lock().unwrap().iter().find(|s| s.id == id).cloned())
        }
        async fn list_due_for_sweep(&self, _limit: i64) -> anyhow::Result<Vec<Subscription>> {
            Ok(self.subs.lock().unwrap().iter()
                .filter(|s| s.current_period_end.is_some()
                    && matches!(s.status, SubscriptionStatus::Active
                                        | SubscriptionStatus::PastDue
                                        | SubscriptionStatus::Cancelled))
                .cloned().collect())
        }
        async fn list_unsynced_tiers(&self, _limit: i64) -> anyhow::Result<Vec<Subscription>> {
            Ok(self.subs.lock().unwrap().iter()
                .filter(|s| s.tier_synced_at.is_none()
                    && s.status != SubscriptionStatus::PendingPayment)
                .cloned().collect())
        }
        async fn save(&self, s: &Subscription) -> anyhow::Result<()> {
            let mut all = self.subs.lock().unwrap();
            match all.iter_mut().find(|x| x.id == s.id) {
                Some(e) => *e = s.clone(),
                None => all.push(s.clone()),
            }
            Ok(())
        }
        async fn mark_tier_synced(&self, id: Uuid, at: DateTime<Utc>) -> anyhow::Result<()> {
            self.synced.lock().unwrap().push((id, at));
            if let Some(e) = self.subs.lock().unwrap().iter_mut().find(|s| s.id == id) {
                e.tier_synced_at = Some(at);
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeIdentity {
        granted: Mutex<Vec<(Uuid, String)>>,
        fails:   bool,
    }

    #[async_trait::async_trait]
    impl TenantTierSync for FakeIdentity {
        async fn set_tier(&self, tenant_id: Uuid, tier: &str) -> anyhow::Result<()> {
            self.granted.lock().unwrap().push((tenant_id, tier.to_string()));
            if self.fails { anyhow::bail!("identity is down") }
            Ok(())
        }
    }

    fn noop_kafka() -> Arc<KafkaProducer> {
        let cluster = rdkafka::mocking::MockCluster::new(1).expect("mock kafka cluster");
        let brokers = cluster.bootstrap_servers();
        Box::leak(Box::new(cluster));
        Arc::new(KafkaProducer::new(&brokers).expect("noop kafka producer"))
    }

    fn service(repo: Arc<FakeRepo>, identity: Option<Arc<FakeIdentity>>) -> SubscriptionService {
        SubscriptionService::new(
            repo,
            None, // no gateway: every test here is past the checkout
            identity.map(|i| i as Arc<dyn TenantTierSync>),
            noop_kafka(),
            "https://example.invalid/return".into(),
        )
    }

    // ── Checkout refusals ─────────────────────────────────────────────────

    /// Starter is free. Selling it would take money for the thing a tenant
    /// already has, and the catalogue has no row for it to price against.
    #[tokio::test]
    async fn starter_cannot_be_bought() {
        let svc = service(FakeRepo::new(), None);
        let err = svc.checkout(Uuid::new_v4(), "starter", BillingInterval::Monthly, "USD")
            .await.unwrap_err();
        assert!(matches!(err, SubscriptionError::NotSelfServe(_)), "got {err:?}");
    }

    /// The pricing page says Enterprise is quoted per deployment. A self-serve
    /// checkout that invented a number would be selling something nobody
    /// agreed to.
    #[tokio::test]
    async fn enterprise_cannot_be_bought_without_sales() {
        let svc = service(FakeRepo::new(), None);
        let err = svc.checkout(Uuid::new_v4(), "enterprise", BillingInterval::Annual, "USD")
            .await.unwrap_err();
        assert!(matches!(err, SubscriptionError::NotSelfServe(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn a_tier_with_no_plan_row_is_refused_rather_than_priced_at_zero() {
        let svc = service(FakeRepo::new(), None);
        let err = svc.checkout(Uuid::new_v4(), "growth", BillingInterval::Monthly, "PHP")
            .await.unwrap_err();
        assert!(matches!(err, SubscriptionError::NoPlan { .. }), "got {err:?}");
    }

    // ── Capture -> period + tier ──────────────────────────────────────────

    async fn paid(repo: &Arc<FakeRepo>, svc: &SubscriptionService) -> (Uuid, Uuid) {
        let tenant = Uuid::new_v4();
        let sub = Subscription::new(tenant, repo.plan());
        let id = repo.seed(sub);
        svc.apply_captured_payment(id, Uuid::new_v4()).await.unwrap();
        (tenant, id)
    }

    #[tokio::test]
    async fn a_captured_payment_grants_the_tier_and_records_that_it_landed() {
        let repo = FakeRepo::new();
        let identity = Arc::new(FakeIdentity::default());
        let svc = service(repo.clone(), Some(identity.clone()));

        let (tenant, id) = paid(&repo, &svc).await;

        assert_eq!(identity.granted.lock().unwrap().as_slice(), &[(tenant, "growth".to_string())]);
        let s = repo.get(id);
        assert_eq!(s.status, SubscriptionStatus::Active);
        assert!(s.tier_synced_at.is_some());
    }

    /// The failure this whole design exists to make survivable: the money moved
    /// and the entitlement did not. It must leave a durable marker, not a log
    /// line.
    #[tokio::test]
    async fn a_failed_grant_leaves_the_subscription_active_and_unsynced() {
        let repo = FakeRepo::new();
        let identity = Arc::new(FakeIdentity { fails: true, ..Default::default() });
        let svc = service(repo.clone(), Some(identity));

        let (_, id) = paid(&repo, &svc).await;

        let s = repo.get(id);
        assert_eq!(s.status, SubscriptionStatus::Active, "the tenant has paid");
        assert!(s.tier_synced_at.is_none(), "and is owed an entitlement");
    }

    /// ...and the sweep is what pays that debt.
    #[tokio::test]
    async fn the_sync_sweep_recovers_a_tier_that_was_paid_for_but_never_granted() {
        let repo = FakeRepo::new();
        let failing = Arc::new(FakeIdentity { fails: true, ..Default::default() });
        let svc = service(repo.clone(), Some(failing));
        let (tenant, id) = paid(&repo, &svc).await;
        assert!(repo.get(id).tier_synced_at.is_none());

        // Identity comes back.
        let working = Arc::new(FakeIdentity::default());
        let svc = service(repo.clone(), Some(working.clone()));
        assert_eq!(svc.sweep_tier_sync().await.unwrap(), 1);

        assert_eq!(working.granted.lock().unwrap().as_slice(), &[(tenant, "growth".to_string())]);
        assert!(repo.get(id).tier_synced_at.is_some());
    }

    /// Kafka is at-least-once. Without the intent guard every partition replay
    /// is a free month.
    #[tokio::test]
    async fn a_redelivered_capture_grants_nothing_twice() {
        let repo = FakeRepo::new();
        let identity = Arc::new(FakeIdentity::default());
        let svc = service(repo.clone(), Some(identity.clone()));

        let tenant = Uuid::new_v4();
        let id = repo.seed(Subscription::new(tenant, repo.plan()));
        let intent = Uuid::new_v4();

        svc.apply_captured_payment(id, intent).await.unwrap();
        let first_end = repo.get(id).current_period_end;
        svc.apply_captured_payment(id, intent).await.unwrap();

        assert_eq!(repo.get(id).current_period_end, first_end);
        assert_eq!(identity.granted.lock().unwrap().len(), 1, "and does not re-grant");
    }

    #[tokio::test]
    async fn a_capture_for_an_unknown_subscription_is_not_an_error() {
        let repo = FakeRepo::new();
        let svc = service(repo, Some(Arc::new(FakeIdentity::default())));
        svc.apply_captured_payment(Uuid::new_v4(), Uuid::new_v4()).await
            .expect("an old replay must not crash-loop the consumer");
    }

    // ── The sweep ─────────────────────────────────────────────────────────

    fn aged(repo: &Arc<FakeRepo>, id: Uuid, period_end: DateTime<Utc>) {
        let mut s = repo.get(id);
        s.current_period_start = Some(period_end - Duration::days(30));
        s.current_period_end   = Some(period_end);
        if let Some(x) = repo.subs.lock().unwrap().iter_mut().find(|x| x.id == id) { *x = s; }
    }

    /// The grace window is the whole reason `past_due` exists: a tenant a few
    /// days late must not lose their drivers and tracking pages.
    #[tokio::test]
    async fn an_overdue_subscription_goes_past_due_without_losing_its_tier() {
        let repo = FakeRepo::new();
        let identity = Arc::new(FakeIdentity::default());
        let svc = service(repo.clone(), Some(identity.clone()));
        let (_, id) = paid(&repo, &svc).await;
        aged(&repo, id, Utc::now() - Duration::hours(1));

        assert_eq!(svc.sweep().await.unwrap(), 1);

        let s = repo.get(id);
        assert_eq!(s.status, SubscriptionStatus::PastDue);
        assert_eq!(s.effective_tier(), "growth", "still entitled during grace");
        // One grant, from the original payment. Going past due must not
        // re-grant and must not downgrade.
        assert_eq!(identity.granted.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_exhausted_grace_window_lapses_and_downgrades_to_starter() {
        let repo = FakeRepo::new();
        let identity = Arc::new(FakeIdentity::default());
        let svc = service(repo.clone(), Some(identity.clone()));
        let (tenant, id) = paid(&repo, &svc).await;
        aged(&repo, id, Utc::now() - Duration::days(GRACE_PERIOD_DAYS + 1));

        assert_eq!(svc.sweep().await.unwrap(), 1);

        let s = repo.get(id);
        assert_eq!(s.status, SubscriptionStatus::Lapsed);
        assert_eq!(
            identity.granted.lock().unwrap().last().cloned(),
            Some((tenant, "starter".to_string())),
            "the downgrade must reach identity too, or the tenant keeps a tier they stopped paying for",
        );
    }

    /// A never-paid subscription has no period. Treating that as "already
    /// ended" would lapse every abandoned checkout and downgrade tenants who
    /// never subscribed at all.
    #[tokio::test]
    async fn a_never_paid_subscription_is_untouched_by_the_sweep() {
        let repo = FakeRepo::new();
        let identity = Arc::new(FakeIdentity::default());
        let svc = service(repo.clone(), Some(identity.clone()));
        let id = repo.seed(Subscription::new(Uuid::new_v4(), repo.plan()));

        assert_eq!(svc.sweep().await.unwrap(), 0);
        assert_eq!(repo.get(id).status, SubscriptionStatus::PendingPayment);
        assert!(identity.granted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_renewal_notice_goes_out_once_inside_the_window() {
        let repo = FakeRepo::new();
        let svc = service(repo.clone(), Some(Arc::new(FakeIdentity::default())));
        let (_, id) = paid(&repo, &svc).await;
        aged(&repo, id, Utc::now() + Duration::days(1));

        assert_eq!(svc.sweep().await.unwrap(), 1);
        assert!(repo.get(id).renewal_notice_sent_at.is_some());

        assert_eq!(svc.sweep().await.unwrap(), 0, "one notice per period");
    }

    // ── Cancellation ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn cancelling_keeps_the_tier_until_the_period_ends_then_lapses_it() {
        let repo = FakeRepo::new();
        let identity = Arc::new(FakeIdentity::default());
        let svc = service(repo.clone(), Some(identity.clone()));
        let (tenant, id) = paid(&repo, &svc).await;

        let cancelled = svc.cancel(tenant).await.unwrap();
        assert_eq!(cancelled.status, SubscriptionStatus::Cancelled);
        assert_eq!(cancelled.effective_tier(), "growth", "no refund, no early cut-off");
        // Nothing downgraded yet.
        assert_eq!(identity.granted.lock().unwrap().len(), 1);

        // ...and when the period they paid for runs out, it lapses.
        aged(&repo, id, Utc::now() - Duration::minutes(1));
        assert_eq!(svc.sweep().await.unwrap(), 1);
        assert_eq!(repo.get(id).status, SubscriptionStatus::Lapsed);
        assert_eq!(
            identity.granted.lock().unwrap().last().cloned(),
            Some((tenant, "starter".to_string())),
        );
    }

    #[tokio::test]
    async fn cancelling_without_a_subscription_is_refused_not_silently_accepted() {
        let svc = service(FakeRepo::new(), Some(Arc::new(FakeIdentity::default())));
        let err = svc.cancel(Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(err, SubscriptionError::NoSubscription), "got {err:?}");
    }
}
