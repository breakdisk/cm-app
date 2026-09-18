use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{postgres::PgRow, PgPool, Row};
use uuid::Uuid;

use crate::domain::offer::{AccountHistory, Discount, Offer};

/// A redemption about to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRedemption {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub offer: Offer,
    pub shipment_id: Uuid,
    pub month: String,
    pub discount_cents: i64,
    pub currency: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedeemOutcome {
    Redeemed,
    /// This booking already redeemed; a retried create lands here.
    AlreadyForThisBooking,
    /// Another booking took this month's code first.
    MonthTaken,
    /// A once-per-account code this account already used.
    OfferTaken,
}

/// A new offer, as an admin states it.
#[derive(Debug, Clone)]
pub struct NewOffer {
    pub tenant_id: Uuid,
    pub code: String,
    pub title: String,
    pub body: String,
    pub discount: Discount,
    pub cap_cents: Option<i64>,
    pub windowed: bool,
    pub once_per_account: bool,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub budget_tag: String,
}

#[async_trait]
pub trait PromotionsStore: Send + Sync {
    async fn find_offer(&self, tenant_id: Uuid, code: &str) -> anyhow::Result<Option<Offer>>;
    /// Active offers inside their dates, newest first.
    async fn live_offers(&self, tenant_id: Uuid, now: DateTime<Utc>) -> anyhow::Result<Vec<Offer>>;
    async fn history(&self, tenant_id: Uuid, account_id: Uuid, month: &str) -> anyhow::Result<AccountHistory>;
    async fn redeem(&self, r: &NewRedemption) -> anyhow::Result<RedeemOutcome>;
    /// The booking was cancelled: its code goes back. True when one was released.
    async fn release(&self, shipment_id: Uuid, at: DateTime<Utc>) -> anyhow::Result<bool>;
    async fn list_offers(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Offer>>;
    async fn create_offer(&self, o: &NewOffer) -> anyhow::Result<Option<Offer>>;
    async fn set_active(&self, tenant_id: Uuid, offer_id: Uuid, active: bool) -> anyhow::Result<bool>;
}

pub struct PgPromotionsStore {
    pool: PgPool,
}

impl PgPromotionsStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

const OFFER_COLUMNS: &str = "id, tenant_id, code, title, body, discount_kind, percent_bps, flat_cents, \
     cap_cents, windowed, once_per_account, starts_at, ends_at, active, budget_tag";

fn to_offer(r: &PgRow) -> anyhow::Result<Offer> {
    let kind: String = r.get("discount_kind");
    let discount = match kind.as_str() {
        "percent_carriage" => Discount::PercentCarriage { bps: r.get("percent_bps") },
        "flat" => Discount::Flat { cents: r.get("flat_cents") },
        other => anyhow::bail!("offer has an unknown discount_kind {other:?}"),
    };
    Ok(Offer {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        code: r.get("code"),
        title: r.get("title"),
        body: r.get("body"),
        discount,
        cap_cents: r.get("cap_cents"),
        windowed: r.get("windowed"),
        once_per_account: r.get("once_per_account"),
        starts_at: r.get("starts_at"),
        ends_at: r.get("ends_at"),
        active: r.get("active"),
        budget_tag: r.get("budget_tag"),
    })
}

fn unique_violation_on(e: &sqlx::Error) -> Option<String> {
    let db = e.as_database_error()?;
    if db.code().as_deref() != Some("23505") {
        return None;
    }
    Some(db.constraint().unwrap_or_default().to_owned())
}

#[async_trait]
impl PromotionsStore for PgPromotionsStore {
    async fn find_offer(&self, tenant_id: Uuid, code: &str) -> anyhow::Result<Option<Offer>> {
        let row = sqlx::query(&format!(
            "SELECT {OFFER_COLUMNS} FROM promotions.offers WHERE tenant_id = $1 AND code = $2"
        ))
        .bind(tenant_id)
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(to_offer).transpose()
    }

    async fn live_offers(&self, tenant_id: Uuid, now: DateTime<Utc>) -> anyhow::Result<Vec<Offer>> {
        let rows = sqlx::query(&format!(
            "SELECT {OFFER_COLUMNS} FROM promotions.offers
             WHERE tenant_id = $1 AND active
               AND (starts_at IS NULL OR starts_at <= $2)
               AND (ends_at IS NULL OR ends_at > $2)
             ORDER BY created_at DESC
             LIMIT 50"
        ))
        .bind(tenant_id)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(to_offer).collect()
    }

    async fn history(&self, tenant_id: Uuid, account_id: Uuid, month: &str) -> anyhow::Result<AccountHistory> {
        let rows = sqlx::query(
            "SELECT offer_id, windowed, month FROM promotions.redemptions
             WHERE tenant_id = $1 AND account_id = $2 AND released_at IS NULL",
        )
        .bind(tenant_id)
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let mut h = AccountHistory::default();
        for r in &rows {
            let windowed: bool = r.get("windowed");
            let m: String = r.get("month");
            if windowed && m == month {
                h.windowed_used_this_month = true;
            }
            h.offers_used.push(r.get("offer_id"));
        }
        Ok(h)
    }

    async fn redeem(&self, r: &NewRedemption) -> anyhow::Result<RedeemOutcome> {
        let insert = sqlx::query(
            "INSERT INTO promotions.redemptions
                 (tenant_id, account_id, offer_id, code, shipment_id, month, windowed,
                  once_per_account, discount_cents, currency, budget_tag)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(r.tenant_id)
        .bind(r.account_id)
        .bind(r.offer.id)
        .bind(&r.offer.code)
        .bind(r.shipment_id)
        .bind(&r.month)
        .bind(r.offer.windowed)
        .bind(r.offer.once_per_account)
        .bind(r.discount_cents)
        .bind(&r.currency)
        .bind(&r.offer.budget_tag)
        .execute(&self.pool)
        .await;

        match insert {
            Ok(_) => Ok(RedeemOutcome::Redeemed),
            Err(e) => match unique_violation_on(&e).as_deref() {
                Some("redemptions_shipment_id_key") => Ok(RedeemOutcome::AlreadyForThisBooking),
                Some("redemptions_one_windowed_per_month") => Ok(RedeemOutcome::MonthTaken),
                Some("redemptions_once_per_account") => Ok(RedeemOutcome::OfferTaken),
                _ => Err(e.into()),
            },
        }
    }

    async fn release(&self, shipment_id: Uuid, at: DateTime<Utc>) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "UPDATE promotions.redemptions SET released_at = $2
             WHERE shipment_id = $1 AND released_at IS NULL",
        )
        .bind(shipment_id)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    async fn list_offers(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Offer>> {
        let rows = sqlx::query(&format!(
            "SELECT {OFFER_COLUMNS} FROM promotions.offers WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 200"
        ))
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(to_offer).collect()
    }

    async fn create_offer(&self, o: &NewOffer) -> anyhow::Result<Option<Offer>> {
        let (kind, bps, flat) = match o.discount {
            Discount::PercentCarriage { bps } => ("percent_carriage", bps, 0),
            Discount::Flat { cents } => ("flat", 0, cents),
        };
        let row = sqlx::query(&format!(
            "INSERT INTO promotions.offers
                 (tenant_id, code, title, body, discount_kind, percent_bps, flat_cents, cap_cents,
                  windowed, once_per_account, starts_at, ends_at, budget_tag)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT (tenant_id, code) DO NOTHING
             RETURNING {OFFER_COLUMNS}"
        ))
        .bind(o.tenant_id)
        .bind(&o.code)
        .bind(&o.title)
        .bind(&o.body)
        .bind(kind)
        .bind(bps)
        .bind(flat)
        .bind(o.cap_cents)
        .bind(o.windowed)
        .bind(o.once_per_account)
        .bind(o.starts_at)
        .bind(o.ends_at)
        .bind(&o.budget_tag)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(to_offer).transpose()
    }

    async fn set_active(&self, tenant_id: Uuid, offer_id: Uuid, active: bool) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "UPDATE promotions.offers SET active = $3, updated_at = NOW() WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(offer_id)
        .bind(active)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }
}
