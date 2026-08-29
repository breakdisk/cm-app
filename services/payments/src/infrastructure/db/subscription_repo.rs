use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::{
    entities::{BillingInterval, Subscription, SubscriptionPlan, SubscriptionStatus},
    repositories::SubscriptionRepository,
};

pub struct PgSubscriptionRepository { pool: PgPool }
impl PgSubscriptionRepository { pub fn new(pool: PgPool) -> Self { Self { pool } } }

const PLAN_COLS: &str =
    "id, tier, interval, currency, amount_cents, period_days, is_active";

const SUB_COLS: &str =
    "id, tenant_id, plan_id, tier, status, currency, amount_cents, \
     current_period_start, current_period_end, last_payment_intent_id, \
     renewal_notice_sent_at, tier_synced_at, cancelled_at, created_at, updated_at";

fn plan_from_row(r: &sqlx::postgres::PgRow) -> anyhow::Result<SubscriptionPlan> {
    let interval: String = r.get("interval");
    Ok(SubscriptionPlan {
        id:           r.get("id"),
        tier:         r.get("tier"),
        interval:     BillingInterval::parse(&interval)
            .ok_or_else(|| anyhow::anyhow!("unknown billing interval {interval:?}"))?,
        currency:     r.get("currency"),
        amount_cents: r.get("amount_cents"),
        period_days:  r.get::<i32, _>("period_days") as i64,
        is_active:    r.get("is_active"),
    })
}

fn sub_from_row(r: &sqlx::postgres::PgRow) -> anyhow::Result<Subscription> {
    let status: String = r.get("status");
    Ok(Subscription {
        id:                     r.get("id"),
        tenant_id:              r.get("tenant_id"),
        plan_id:                r.get("plan_id"),
        tier:                   r.get("tier"),
        status:                 SubscriptionStatus::parse(&status)
            .ok_or_else(|| anyhow::anyhow!("unknown subscription status {status:?}"))?,
        currency:               r.get("currency"),
        amount_cents:           r.get("amount_cents"),
        current_period_start:   r.get("current_period_start"),
        current_period_end:     r.get("current_period_end"),
        last_payment_intent_id: r.get("last_payment_intent_id"),
        renewal_notice_sent_at: r.get("renewal_notice_sent_at"),
        tier_synced_at:         r.get("tier_synced_at"),
        cancelled_at:           r.get("cancelled_at"),
        created_at:             r.get("created_at"),
        updated_at:             r.get("updated_at"),
    })
}

#[async_trait]
impl SubscriptionRepository for PgSubscriptionRepository {
    async fn find_plan(
        &self,
        tier: &str,
        interval: BillingInterval,
        currency: &str,
    ) -> anyhow::Result<Option<SubscriptionPlan>> {
        let row = sqlx::query(&format!(
            "SELECT {PLAN_COLS} FROM payments.subscription_plans \
             WHERE tier = $1 AND interval = $2 AND currency = $3 AND is_active"
        ))
        .bind(tier).bind(interval.as_str()).bind(currency)
        .fetch_optional(&self.pool).await?;
        row.as_ref().map(plan_from_row).transpose()
    }

    async fn find_plan_by_id(&self, id: Uuid) -> anyhow::Result<Option<SubscriptionPlan>> {
        let row = sqlx::query(&format!(
            "SELECT {PLAN_COLS} FROM payments.subscription_plans WHERE id = $1"
        ))
        .bind(id).fetch_optional(&self.pool).await?;
        row.as_ref().map(plan_from_row).transpose()
    }

    async fn list_plans(&self, currency: &str) -> anyhow::Result<Vec<SubscriptionPlan>> {
        let rows = sqlx::query(&format!(
            "SELECT {PLAN_COLS} FROM payments.subscription_plans \
             WHERE currency = $1 AND is_active ORDER BY amount_cents ASC"
        ))
        .bind(currency).fetch_all(&self.pool).await?;
        rows.iter().map(plan_from_row).collect()
    }

    /// The tenant's live subscription, if any.
    ///
    /// "Live" is the same set the unique index enforces, so this can never see
    /// two — and it deliberately excludes `lapsed`, which is history rather
    /// than a subscription a tenant currently has.
    async fn find_live_for_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Option<Subscription>> {
        let row = sqlx::query(&format!(
            "SELECT {SUB_COLS} FROM payments.subscriptions \
             WHERE tenant_id = $1 \
               AND status IN ('pending_payment', 'active', 'past_due', 'cancelled') \
             LIMIT 1"
        ))
        .bind(tenant_id).fetch_optional(&self.pool).await?;
        row.as_ref().map(sub_from_row).transpose()
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<Subscription>> {
        let row = sqlx::query(&format!(
            "SELECT {SUB_COLS} FROM payments.subscriptions WHERE id = $1"
        ))
        .bind(id).fetch_optional(&self.pool).await?;
        row.as_ref().map(sub_from_row).transpose()
    }

    /// Subscriptions the renewal / dunning sweep may need to act on.
    ///
    /// `current_period_end IS NOT NULL` is load-bearing: a never-paid row has no
    /// period, and pulling it in here would have the sweep lapse every abandoned
    /// checkout and downgrade tenants who never subscribed.
    async fn list_due_for_sweep(&self, limit: i64) -> anyhow::Result<Vec<Subscription>> {
        let rows = sqlx::query(&format!(
            "SELECT {SUB_COLS} FROM payments.subscriptions \
             WHERE status IN ('active', 'past_due', 'cancelled') \
               AND current_period_end IS NOT NULL \
             ORDER BY current_period_end ASC LIMIT $1"
        ))
        .bind(limit).fetch_all(&self.pool).await?;
        rows.iter().map(sub_from_row).collect()
    }

    /// Paid subscriptions identity has not been told about — the money moved
    /// and the entitlement did not.
    async fn list_unsynced_tiers(&self, limit: i64) -> anyhow::Result<Vec<Subscription>> {
        let rows = sqlx::query(&format!(
            "SELECT {SUB_COLS} FROM payments.subscriptions \
             WHERE tier_synced_at IS NULL AND status IN ('active', 'past_due', 'cancelled', 'lapsed') \
             ORDER BY updated_at ASC LIMIT $1"
        ))
        .bind(limit).fetch_all(&self.pool).await?;
        rows.iter().map(sub_from_row).collect()
    }

    async fn save(&self, s: &Subscription) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO payments.subscriptions
                   (id, tenant_id, plan_id, tier, status, currency, amount_cents,
                    current_period_start, current_period_end, last_payment_intent_id,
                    renewal_notice_sent_at, tier_synced_at, cancelled_at, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
               ON CONFLICT (id) DO UPDATE SET
                   plan_id                = EXCLUDED.plan_id,
                   tier                   = EXCLUDED.tier,
                   status                 = EXCLUDED.status,
                   currency               = EXCLUDED.currency,
                   amount_cents           = EXCLUDED.amount_cents,
                   current_period_start   = EXCLUDED.current_period_start,
                   current_period_end     = EXCLUDED.current_period_end,
                   last_payment_intent_id = EXCLUDED.last_payment_intent_id,
                   renewal_notice_sent_at = EXCLUDED.renewal_notice_sent_at,
                   tier_synced_at         = EXCLUDED.tier_synced_at,
                   cancelled_at           = EXCLUDED.cancelled_at,
                   updated_at             = EXCLUDED.updated_at"#,
        )
        .bind(s.id).bind(s.tenant_id).bind(s.plan_id).bind(&s.tier)
        .bind(s.status.as_str()).bind(&s.currency).bind(s.amount_cents)
        .bind(s.current_period_start).bind(s.current_period_end)
        .bind(s.last_payment_intent_id).bind(s.renewal_notice_sent_at)
        .bind(s.tier_synced_at).bind(s.cancelled_at)
        .bind(s.created_at).bind(s.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    /// Marks the tier as delivered to identity.
    ///
    /// A targeted `UPDATE` rather than a full `save`, because the caller is the
    /// sync sweep holding a snapshot it read some time ago: writing the whole
    /// row back would overwrite a renewal that landed in between with stale
    /// period dates.
    async fn mark_tier_synced(&self, id: Uuid, at: DateTime<Utc>) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE payments.subscriptions SET tier_synced_at = $1 WHERE id = $2",
        )
        .bind(at).bind(id).execute(&self.pool).await?;
        Ok(())
    }
}
