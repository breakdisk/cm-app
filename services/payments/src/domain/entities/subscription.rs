//! A tenant's paid plan.
//!
//! The rule that shapes everything here: **a tier change is a consequence of a
//! captured payment, never a request.** `PUT /v1/tenants/:id/tier` in identity
//! requires `tenants:manage`, which no role grants and which
//! `libs/auth/src/rbac.rs` has a test specifically to keep ungranted — the same
//! permission rewrites the platform-wide pricing matrix, so granting it to let
//! a tenant upgrade would hand them everyone's prices and a free jump to
//! Enterprise. Nothing in this module gives a tenant a way to name their own
//! tier; the only writer is `activate_from_payment`, and the only thing that
//! calls it is a capture webhook.
//!
//! Renewal is deliberately not an automatic charge. Taking money on a schedule
//! needs a stored credential — a card token the platform can charge without the
//! cardholder present — and the Network International adapter has no such
//! capability: `create_session` returns a one-shot hosted page and nothing
//! stores a token. Rather than pretend, a subscription nearing its end publishes
//! `subscription.renewal_due` and the tenant pays the next period the same way
//! they paid the first. The `past_due` grace window exists so that a tenant who
//! is a few days late does not lose their platform.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How long a subscription keeps its paid tier after its period ends without a
/// renewal. Short enough that "free forever" is not reachable by simply not
/// paying, long enough to survive a weekend and an unread email.
pub const GRACE_PERIOD_DAYS: i64 = 7;

/// How far ahead of the period end the renewal notice goes out.
pub const RENEWAL_NOTICE_DAYS: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingInterval {
    Monthly,
    Annual,
}

impl BillingInterval {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Monthly => "monthly",
            Self::Annual  => "annual",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "monthly" => Some(Self::Monthly),
            "annual"  => Some(Self::Annual),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    /// Created; nothing captured. The tenant is still on whatever tier they had.
    PendingPayment,
    /// Paid and inside its period.
    Active,
    /// The period ended with no renewal. Still on the paid tier — this is the
    /// grace window, not a downgrade.
    PastDue,
    /// The tenant asked to stop. Runs to period end, then lapses.
    Cancelled,
    /// Grace exhausted. Tier reverted to starter.
    Lapsed,
}

impl SubscriptionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingPayment => "pending_payment",
            Self::Active         => "active",
            Self::PastDue        => "past_due",
            Self::Cancelled      => "cancelled",
            Self::Lapsed         => "lapsed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending_payment" => Some(Self::PendingPayment),
            "active"          => Some(Self::Active),
            "past_due"        => Some(Self::PastDue),
            "cancelled"       => Some(Self::Cancelled),
            "lapsed"          => Some(Self::Lapsed),
            _ => None,
        }
    }

    /// Whether this subscription still entitles the tenant to its paid tier.
    ///
    /// `PastDue` counts, and that is the point of the grace window: a tenant a
    /// few days late keeps their drivers, their tracking pages and their API
    /// keys. `Cancelled` counts too — they paid for the period they are in.
    pub fn grants_tier(&self) -> bool {
        matches!(self, Self::Active | Self::PastDue | Self::Cancelled)
    }
}

/// A row of the plan catalogue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionPlan {
    pub id:           Uuid,
    /// `growth` or `business`. Starter is free and Enterprise is quoted by
    /// hand — neither is sellable through a self-serve checkout, so neither has
    /// a plan row. See migration 0019.
    pub tier:         String,
    pub interval:     BillingInterval,
    pub currency:     String,
    /// The whole charge for one period, not a monthly rate. An annual Growth
    /// plan is one charge of twelve discounted months.
    pub amount_cents: i64,
    pub period_days:  i64,
    pub is_active:    bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id:                     Uuid,
    pub tenant_id:              Uuid,
    pub plan_id:                Uuid,
    pub tier:                   String,
    pub status:                 SubscriptionStatus,
    pub currency:               String,
    pub amount_cents:           i64,
    pub current_period_start:   Option<DateTime<Utc>>,
    /// `None` until the first payment lands. An unpaid subscription has no
    /// period, and that `None` is what stops the renewal sweep treating a
    /// never-paid row as overdue.
    pub current_period_end:     Option<DateTime<Utc>>,
    /// The intent whose capture last extended this subscription. Kafka is
    /// at-least-once, and without this a redelivered capture is a free period.
    pub last_payment_intent_id: Option<Uuid>,
    pub renewal_notice_sent_at: Option<DateTime<Utc>>,
    /// When identity was last told about this tier. `None` means the money
    /// moved and the entitlement did not.
    pub tier_synced_at:         Option<DateTime<Utc>>,
    pub cancelled_at:           Option<DateTime<Utc>>,
    pub created_at:             DateTime<Utc>,
    pub updated_at:             DateTime<Utc>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SubscriptionError {
    #[error("a {0} subscription cannot be renewed")]
    NotRenewable(&'static str),
    #[error("this subscription is already {0}")]
    AlreadyInState(&'static str),
}

impl Subscription {
    /// A subscription awaiting its first payment. Grants nothing yet: `tier`
    /// records what the tenant is buying, not what they currently have.
    pub fn new(tenant_id: Uuid, plan: &SubscriptionPlan) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            plan_id: plan.id,
            tier: plan.tier.clone(),
            status: SubscriptionStatus::PendingPayment,
            currency: plan.currency.clone(),
            amount_cents: plan.amount_cents,
            current_period_start: None,
            current_period_end: None,
            last_payment_intent_id: None,
            renewal_notice_sent_at: None,
            tier_synced_at: None,
            cancelled_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Money landed — start or extend the paid period.
    ///
    /// Returns `false` when this exact intent has already been applied, which
    /// is the redelivery case and must not extend anything. Idempotency is
    /// keyed on the intent rather than on the status, because a legitimate
    /// renewal arrives while the subscription is already `Active`.
    ///
    /// A renewal that arrives before the current period ends extends from the
    /// period end, not from now: paying early must not forfeit the days already
    /// bought. A renewal that arrives after it starts from now, since the gap
    /// was unpaid.
    pub fn activate_from_payment(
        &mut self,
        intent_id: Uuid,
        period_days: i64,
        now: DateTime<Utc>,
    ) -> bool {
        if self.last_payment_intent_id == Some(intent_id) {
            return false;
        }

        let start = match self.current_period_end {
            Some(end) if end > now && self.status.grants_tier() => end,
            _ => now,
        };

        self.current_period_start   = Some(start);
        self.current_period_end     = Some(start + Duration::days(period_days));
        self.last_payment_intent_id = Some(intent_id);
        self.renewal_notice_sent_at = None;
        // The tenant may have been `lapsed` or `past_due`; paying restores them.
        // `Cancelled` deliberately does not: they asked to stop, and a renewal
        // payment against a cancelled subscription is not something any path
        // here creates.
        if self.status != SubscriptionStatus::Cancelled {
            self.status = SubscriptionStatus::Active;
        }
        // The tier now differs from whatever identity last heard.
        self.tier_synced_at = None;
        self.updated_at     = now;
        true
    }

    /// The tenant asked to stop. They keep the tier until the period they paid
    /// for runs out — a cancellation is not a refund.
    pub fn cancel(&mut self, now: DateTime<Utc>) -> Result<(), SubscriptionError> {
        match self.status {
            SubscriptionStatus::Active | SubscriptionStatus::PastDue
            | SubscriptionStatus::PendingPayment => {
                self.status = SubscriptionStatus::Cancelled;
                self.cancelled_at = Some(now);
                self.updated_at = now;
                Ok(())
            }
            SubscriptionStatus::Cancelled => Err(SubscriptionError::AlreadyInState("cancelled")),
            SubscriptionStatus::Lapsed    => Err(SubscriptionError::AlreadyInState("lapsed")),
        }
    }

    /// The period ended with nothing paid — enter the grace window. The tenant
    /// keeps their tier; only the clock changes.
    pub fn mark_past_due(&mut self, now: DateTime<Utc>) -> Result<(), SubscriptionError> {
        match self.status {
            SubscriptionStatus::Active => {
                self.status = SubscriptionStatus::PastDue;
                self.updated_at = now;
                Ok(())
            }
            SubscriptionStatus::PastDue => Err(SubscriptionError::AlreadyInState("past_due")),
            other => Err(SubscriptionError::NotRenewable(other.as_str())),
        }
    }

    /// Grace exhausted, or a cancelled subscription reached its period end.
    /// The tier goes back to starter — which is a `tier_synced_at` reset, not a
    /// deletion: the row stays as the billing record it is.
    pub fn lapse(&mut self, now: DateTime<Utc>) -> Result<(), SubscriptionError> {
        match self.status {
            SubscriptionStatus::Active
            | SubscriptionStatus::PastDue
            | SubscriptionStatus::Cancelled => {
                self.status = SubscriptionStatus::Lapsed;
                self.tier_synced_at = None;
                self.updated_at = now;
                Ok(())
            }
            other => Err(SubscriptionError::NotRenewable(other.as_str())),
        }
    }

    /// The tier identity should be holding for this tenant right now.
    ///
    /// `starter` for anything that is not currently entitled — a lapsed
    /// subscription must not leave the tenant on a tier they stopped paying
    /// for, and a never-paid one must not grant anything at all.
    pub fn effective_tier(&self) -> &str {
        if self.status.grants_tier() { &self.tier } else { "starter" }
    }

    /// Whether a renewal notice is due: inside the notice window, not already
    /// sent for this period, and actually renewable.
    pub fn renewal_notice_due(&self, now: DateTime<Utc>) -> bool {
        if self.renewal_notice_sent_at.is_some() {
            return false;
        }
        // A cancelled subscription is not being renewed, so nagging about it is
        // wrong; a lapsed or unpaid one has no period to renew.
        if !matches!(self.status, SubscriptionStatus::Active) {
            return false;
        }
        match self.current_period_end {
            Some(end) => now >= end - Duration::days(RENEWAL_NOTICE_DAYS) && now < end,
            None => false,
        }
    }

    /// Whether the paid period has run out.
    pub fn period_ended(&self, now: DateTime<Utc>) -> bool {
        self.current_period_end.map(|end| now >= end).unwrap_or(false)
    }

    /// Whether the grace window after the period end has run out too.
    pub fn grace_exhausted(&self, now: DateTime<Utc>) -> bool {
        self.current_period_end
            .map(|end| now >= end + Duration::days(GRACE_PERIOD_DAYS))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(days: i64) -> SubscriptionPlan {
        SubscriptionPlan {
            id: Uuid::new_v4(),
            tier: "growth".into(),
            interval: if days > 100 { BillingInterval::Annual } else { BillingInterval::Monthly },
            currency: "USD".into(),
            amount_cents: 14900,
            period_days: days,
            is_active: true,
        }
    }

    fn pending() -> Subscription {
        Subscription::new(Uuid::new_v4(), &plan(30))
    }

    /// The whole safety property of this module: buying is not the same as
    /// having. Until money lands the tenant is on starter, whatever they picked.
    #[test]
    fn an_unpaid_subscription_grants_nothing() {
        let s = pending();
        assert_eq!(s.status, SubscriptionStatus::PendingPayment);
        assert_eq!(s.effective_tier(), "starter");
        assert!(!s.status.grants_tier());
    }

    #[test]
    fn a_captured_payment_starts_the_period_and_grants_the_tier() {
        let mut s = pending();
        let now = Utc::now();
        assert!(s.activate_from_payment(Uuid::new_v4(), 30, now));
        assert_eq!(s.status, SubscriptionStatus::Active);
        assert_eq!(s.effective_tier(), "growth");
        assert_eq!(s.current_period_end, Some(now + Duration::days(30)));
        assert!(s.tier_synced_at.is_none(), "identity has not been told yet");
    }

    /// Kafka is at-least-once. Without the intent-id guard a redelivered
    /// capture is a free extra period, every time the partition replays.
    #[test]
    fn a_redelivered_capture_does_not_extend_the_period() {
        let mut s = pending();
        let intent = Uuid::new_v4();
        let now = Utc::now();
        assert!(s.activate_from_payment(intent, 30, now));
        let end = s.current_period_end;

        assert!(!s.activate_from_payment(intent, 30, now + Duration::minutes(5)));
        assert_eq!(s.current_period_end, end, "the period must not move");
    }

    /// Paying early must not forfeit the days already bought.
    #[test]
    fn an_early_renewal_extends_from_the_period_end_not_from_now() {
        let mut s = pending();
        let now = Utc::now();
        s.activate_from_payment(Uuid::new_v4(), 30, now);
        let first_end = s.current_period_end.unwrap();

        // Renew with 10 days still to run.
        let renewed_at = first_end - Duration::days(10);
        s.activate_from_payment(Uuid::new_v4(), 30, renewed_at);

        assert_eq!(s.current_period_end, Some(first_end + Duration::days(30)));
    }

    /// A late renewal starts now: the gap was unpaid, and back-dating would
    /// hand the tenant days they did not buy.
    #[test]
    fn a_late_renewal_starts_from_now() {
        let mut s = pending();
        let now = Utc::now();
        s.activate_from_payment(Uuid::new_v4(), 30, now);
        let first_end = s.current_period_end.unwrap();

        let late = first_end + Duration::days(4);
        s.mark_past_due(first_end).unwrap();
        s.activate_from_payment(Uuid::new_v4(), 30, late);

        assert_eq!(s.current_period_end, Some(late + Duration::days(30)));
        assert_eq!(s.status, SubscriptionStatus::Active, "paying restores a past-due tenant");
    }

    // ── The grace window ──────────────────────────────────────────────────

    /// The reason `past_due` is a status and not an immediate downgrade: a
    /// tenant a few days late keeps their drivers, tracking pages and API keys.
    #[test]
    fn a_past_due_tenant_keeps_their_tier() {
        let mut s = pending();
        let now = Utc::now();
        s.activate_from_payment(Uuid::new_v4(), 30, now);
        s.mark_past_due(now + Duration::days(30)).unwrap();

        assert!(s.status.grants_tier());
        assert_eq!(s.effective_tier(), "growth");
    }

    #[test]
    fn grace_runs_out_and_the_tier_reverts_to_starter() {
        let mut s = pending();
        let now = Utc::now();
        s.activate_from_payment(Uuid::new_v4(), 30, now);
        let end = s.current_period_end.unwrap();

        assert!(!s.grace_exhausted(end + Duration::days(GRACE_PERIOD_DAYS - 1)));
        assert!(s.grace_exhausted(end + Duration::days(GRACE_PERIOD_DAYS)));

        s.lapse(end + Duration::days(GRACE_PERIOD_DAYS)).unwrap();
        assert_eq!(s.effective_tier(), "starter");
        assert!(s.tier_synced_at.is_none(), "the downgrade must be synced too");
    }

    /// A cancellation is not a refund — the tenant keeps what they paid for.
    #[test]
    fn cancelling_keeps_the_tier_until_the_period_ends() {
        let mut s = pending();
        let now = Utc::now();
        s.activate_from_payment(Uuid::new_v4(), 30, now);
        s.cancel(now + Duration::days(2)).unwrap();

        assert_eq!(s.status, SubscriptionStatus::Cancelled);
        assert_eq!(s.effective_tier(), "growth");
        assert!(!s.period_ended(now + Duration::days(3)));
        assert!(s.period_ended(now + Duration::days(31)));
    }

    #[test]
    fn a_lapsed_subscription_cannot_be_cancelled_again() {
        let mut s = pending();
        let now = Utc::now();
        s.activate_from_payment(Uuid::new_v4(), 30, now);
        s.lapse(now + Duration::days(40)).unwrap();
        assert!(s.cancel(now + Duration::days(41)).is_err());
    }

    // ── Renewal notice ────────────────────────────────────────────────────

    #[test]
    fn a_renewal_notice_is_due_only_inside_the_window_and_only_once() {
        let mut s = pending();
        let now = Utc::now();
        s.activate_from_payment(Uuid::new_v4(), 30, now);
        let end = s.current_period_end.unwrap();

        assert!(!s.renewal_notice_due(end - Duration::days(RENEWAL_NOTICE_DAYS + 1)));
        assert!(s.renewal_notice_due(end - Duration::days(1)));

        s.renewal_notice_sent_at = Some(end - Duration::days(1));
        assert!(!s.renewal_notice_due(end - Duration::hours(1)), "one notice per period");
    }

    /// Nagging someone who already cancelled to renew is the wrong message.
    #[test]
    fn a_cancelled_subscription_is_never_nagged_to_renew() {
        let mut s = pending();
        let now = Utc::now();
        s.activate_from_payment(Uuid::new_v4(), 30, now);
        let end = s.current_period_end.unwrap();
        s.cancel(now).unwrap();
        assert!(!s.renewal_notice_due(end - Duration::days(1)));
    }

    /// Paying resets the notice, or the next period would go out unannounced.
    #[test]
    fn renewing_re_arms_the_notice_for_the_next_period() {
        let mut s = pending();
        let now = Utc::now();
        s.activate_from_payment(Uuid::new_v4(), 30, now);
        s.renewal_notice_sent_at = Some(now + Duration::days(27));

        s.activate_from_payment(Uuid::new_v4(), 30, now + Duration::days(28));
        assert!(s.renewal_notice_sent_at.is_none());
    }

    /// A never-paid subscription has no period, so nothing about it is overdue.
    /// Treating `None` as "already ended" would lapse every abandoned checkout
    /// and downgrade tenants who never subscribed in the first place.
    #[test]
    fn a_subscription_with_no_period_is_never_overdue() {
        let s = pending();
        let far_future = Utc::now() + Duration::days(3650);
        assert!(!s.period_ended(far_future));
        assert!(!s.grace_exhausted(far_future));
        assert!(!s.renewal_notice_due(far_future));
    }
}
