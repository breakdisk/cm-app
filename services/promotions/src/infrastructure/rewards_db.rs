//! Loyalty, credit, referrals and corporate rates.
//!
//! The trait's methods default to "nothing here", so a test double only writes
//! what the test is about.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgRow, Row};
use uuid::Uuid;

use super::db::PgPromotionsStore;
use crate::domain::tier::{NewTier, Tier};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreditEntry {
    pub kind: String,
    pub amount_cents: i64,
    pub currency: String,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferralRow {
    pub id: Uuid,
    pub joined_at: DateTime<Utc>,
    pub rewarded_at: Option<DateTime<Utc>>,
    pub reward_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Corporate {
    pub id: Uuid,
    pub code: String,
    pub firm_name: String,
    pub percent_bps: i64,
    pub email_domain: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct NewCorporate {
    pub code: String,
    pub firm_name: String,
    pub percent_bps: i64,
    pub email_domain: Option<String>,
}

#[async_trait]
pub trait RewardsStore: Send + Sync {
    // ── Loyalty ──
    async fn tiers(&self, _tenant: Uuid) -> anyhow::Result<Vec<Tier>> { Ok(Vec::new()) }
    async fn replace_tiers(&self, _tenant: Uuid, _tiers: &[NewTier]) -> anyhow::Result<Vec<Tier>> { Ok(Vec::new()) }
    async fn completed_moves_since(&self, _tenant: Uuid, _account: Uuid, _since: DateTime<Utc>) -> anyhow::Result<i64> { Ok(0) }
    async fn booked_moves(&self, _tenant: Uuid, _account: Uuid) -> anyhow::Result<i64> { Ok(0) }
    async fn record_move_booked(&self, _tenant: Uuid, _account: Uuid, _shipment: Uuid, _at: DateTime<Utc>) -> anyhow::Result<()> { Ok(()) }
    /// The account whose move this was, when it had not completed before.
    async fn record_move_completed(&self, _shipment: Uuid, _at: DateTime<Utc>) -> anyhow::Result<Option<(Uuid, Uuid)>> { Ok(None) }

    // ── Credit ──
    async fn credit_balance(&self, _tenant: Uuid, _account: Uuid, _currency: &str) -> anyhow::Result<i64> { Ok(0) }
    async fn credit_entries(&self, _tenant: Uuid, _account: Uuid, _limit: i64) -> anyhow::Result<Vec<CreditEntry>> { Ok(Vec::new()) }
    async fn grant_credit(&self, _tenant: Uuid, _account: Uuid, _amount: i64, _currency: &str, _note: &str) -> anyhow::Result<()> { Ok(()) }

    // ── Referrals ──
    async fn referral_code(&self, _tenant: Uuid, _account: Uuid) -> anyhow::Result<Option<String>> { Ok(None) }
    /// False when the code is taken (try another).
    async fn insert_referral_code(&self, _tenant: Uuid, _account: Uuid, _code: &str) -> anyhow::Result<bool> { Ok(false) }
    async fn referrer_for_code(&self, _tenant: Uuid, _code: &str) -> anyhow::Result<Option<Uuid>> { Ok(None) }
    async fn is_referred(&self, _tenant: Uuid, _invitee: Uuid) -> anyhow::Result<bool> { Ok(false) }
    /// False when this invitee was referred meanwhile.
    async fn insert_referral(&self, _tenant: Uuid, _referrer: Uuid, _invitee: Uuid) -> anyhow::Result<bool> { Ok(false) }
    async fn referrals_by(&self, _tenant: Uuid, _referrer: Uuid) -> anyhow::Result<Vec<ReferralRow>> { Ok(Vec::new()) }
    /// The invitee's referral, when it has not paid out yet: (id, referrer).
    async fn unpaid_referral_for(&self, _tenant: Uuid, _invitee: Uuid) -> anyhow::Result<Option<(Uuid, Uuid)>> { Ok(None) }
    /// Pays a referral once. False when it had already paid.
    async fn pay_referral(&self, _referral: Uuid, _tenant: Uuid, _referrer: Uuid, _amount: i64, _currency: &str) -> anyhow::Result<bool> { Ok(false) }

    // ── Corporate ──
    async fn corporate_by_code(&self, _tenant: Uuid, _code: &str) -> anyhow::Result<Option<Corporate>> { Ok(None) }
    async fn corporate_by_domain(&self, _tenant: Uuid, _domain: &str) -> anyhow::Result<Option<Corporate>> { Ok(None) }
    async fn linked_corporate(&self, _tenant: Uuid, _account: Uuid) -> anyhow::Result<Option<Corporate>> { Ok(None) }
    async fn link_corporate(&self, _tenant: Uuid, _account: Uuid, _corporate: Uuid) -> anyhow::Result<()> { Ok(()) }
    async fn unlink_corporate(&self, _tenant: Uuid, _account: Uuid) -> anyhow::Result<bool> { Ok(false) }
    async fn list_corporates(&self, _tenant: Uuid) -> anyhow::Result<Vec<Corporate>> { Ok(Vec::new()) }
    async fn create_corporate(&self, _tenant: Uuid, _c: &NewCorporate) -> anyhow::Result<Option<Corporate>> { Ok(None) }
    /// False when there is no such company rate in this tenant.
    async fn set_corporate_active(&self, _tenant: Uuid, _id: Uuid, _active: bool) -> anyhow::Result<bool> { Ok(false) }
}

fn to_tier(r: &PgRow) -> Tier {
    Tier {
        id: r.get("id"),
        name: r.get("name"),
        min_moves: r.get("min_moves"),
        perk: r.get("perk"),
        accessorial_bps: r.get("accessorial_bps"),
        cap_cents: r.get("cap_cents"),
        referral_multiplier: r.get("referral_multiplier"),
    }
}

fn to_corporate(r: &PgRow) -> Corporate {
    Corporate {
        id: r.get("id"),
        code: r.get("code"),
        firm_name: r.get("firm_name"),
        percent_bps: r.get("percent_bps"),
        email_domain: r.get("email_domain"),
        active: r.get("active"),
    }
}

const TIER_COLUMNS: &str = "id, name, min_moves, perk, accessorial_bps, cap_cents, referral_multiplier";
const CORPORATE_COLUMNS: &str = "c.id, c.code, c.firm_name, c.percent_bps, c.email_domain, c.active";

#[async_trait]
impl RewardsStore for PgPromotionsStore {
    async fn tiers(&self, tenant: Uuid) -> anyhow::Result<Vec<Tier>> {
        let rows = sqlx::query(&format!(
            "SELECT {TIER_COLUMNS} FROM promotions.tiers WHERE tenant_id = $1 ORDER BY min_moves"
        ))
        .bind(tenant)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.iter().map(to_tier).collect())
    }

    async fn replace_tiers(&self, tenant: Uuid, tiers: &[NewTier]) -> anyhow::Result<Vec<Tier>> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM promotions.tiers WHERE tenant_id = $1")
            .bind(tenant)
            .execute(&mut *tx)
            .await?;
        for t in tiers {
            sqlx::query(
                "INSERT INTO promotions.tiers
                     (tenant_id, name, min_moves, perk, accessorial_bps, cap_cents, referral_multiplier)
                 VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(tenant)
            .bind(t.name.trim())
            .bind(t.min_moves)
            .bind(t.perk.trim())
            .bind(t.accessorial_bps)
            .bind(t.cap_cents)
            .bind(t.referral_multiplier)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.tiers(tenant).await
    }

    async fn completed_moves_since(&self, tenant: Uuid, account: Uuid, since: DateTime<Utc>) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM promotions.moves
             WHERE tenant_id = $1 AND account_id = $2 AND completed_at >= $3",
        )
        .bind(tenant)
        .bind(account)
        .bind(since)
        .fetch_one(self.pool())
        .await?;
        Ok(n)
    }

    async fn booked_moves(&self, tenant: Uuid, account: Uuid) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM promotions.moves WHERE tenant_id = $1 AND account_id = $2",
        )
        .bind(tenant)
        .bind(account)
        .fetch_one(self.pool())
        .await?;
        Ok(n)
    }

    async fn record_move_booked(&self, tenant: Uuid, account: Uuid, shipment: Uuid, at: DateTime<Utc>) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO promotions.moves (shipment_id, tenant_id, account_id, booked_at)
             VALUES ($1, $2, $3, $4) ON CONFLICT (shipment_id) DO NOTHING",
        )
        .bind(shipment)
        .bind(tenant)
        .bind(account)
        .bind(at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    async fn record_move_completed(&self, shipment: Uuid, at: DateTime<Utc>) -> anyhow::Result<Option<(Uuid, Uuid)>> {
        let row: Option<(Uuid, Uuid)> = sqlx::query_as(
            "UPDATE promotions.moves SET completed_at = $2
             WHERE shipment_id = $1 AND completed_at IS NULL
             RETURNING tenant_id, account_id",
        )
        .bind(shipment)
        .bind(at)
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }

    async fn credit_balance(&self, tenant: Uuid, account: Uuid, currency: &str) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(amount_cents), 0)::bigint FROM promotions.credit_entries
             WHERE tenant_id = $1 AND account_id = $2 AND currency = $3",
        )
        .bind(tenant)
        .bind(account)
        .bind(currency)
        .fetch_one(self.pool())
        .await?;
        Ok(n)
    }

    async fn credit_entries(&self, tenant: Uuid, account: Uuid, limit: i64) -> anyhow::Result<Vec<CreditEntry>> {
        let rows = sqlx::query(
            "SELECT kind, amount_cents, currency, note, created_at FROM promotions.credit_entries
             WHERE tenant_id = $1 AND account_id = $2 ORDER BY created_at DESC LIMIT $3",
        )
        .bind(tenant)
        .bind(account)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|r| CreditEntry {
                kind: r.get("kind"),
                amount_cents: r.get("amount_cents"),
                currency: r.get("currency"),
                note: r.get("note"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    async fn grant_credit(&self, tenant: Uuid, account: Uuid, amount: i64, currency: &str, note: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO promotions.credit_entries (tenant_id, account_id, amount_cents, currency, kind, note)
             VALUES ($1, $2, $3, $4, 'grant', $5)",
        )
        .bind(tenant)
        .bind(account)
        .bind(amount)
        .bind(currency)
        .bind(note)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    async fn referral_code(&self, tenant: Uuid, account: Uuid) -> anyhow::Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT code FROM promotions.referral_codes WHERE tenant_id = $1 AND account_id = $2",
        )
        .bind(tenant)
        .bind(account)
        .fetch_optional(self.pool())
        .await?)
    }

    async fn insert_referral_code(&self, tenant: Uuid, account: Uuid, code: &str) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "INSERT INTO promotions.referral_codes (tenant_id, account_id, code) VALUES ($1, $2, $3)
             ON CONFLICT DO NOTHING",
        )
        .bind(tenant)
        .bind(account)
        .bind(code)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    async fn referrer_for_code(&self, tenant: Uuid, code: &str) -> anyhow::Result<Option<Uuid>> {
        Ok(sqlx::query_scalar(
            "SELECT account_id FROM promotions.referral_codes WHERE tenant_id = $1 AND code = $2",
        )
        .bind(tenant)
        .bind(code)
        .fetch_optional(self.pool())
        .await?)
    }

    async fn is_referred(&self, tenant: Uuid, invitee: Uuid) -> anyhow::Result<bool> {
        let (b,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM promotions.referrals WHERE tenant_id = $1 AND invitee_account_id = $2)",
        )
        .bind(tenant)
        .bind(invitee)
        .fetch_one(self.pool())
        .await?;
        Ok(b)
    }

    async fn insert_referral(&self, tenant: Uuid, referrer: Uuid, invitee: Uuid) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "INSERT INTO promotions.referrals (tenant_id, referrer_account_id, invitee_account_id)
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(tenant)
        .bind(referrer)
        .bind(invitee)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    async fn referrals_by(&self, tenant: Uuid, referrer: Uuid) -> anyhow::Result<Vec<ReferralRow>> {
        let rows = sqlx::query(
            "SELECT id, created_at, rewarded_at, reward_cents FROM promotions.referrals
             WHERE tenant_id = $1 AND referrer_account_id = $2 ORDER BY created_at DESC LIMIT 100",
        )
        .bind(tenant)
        .bind(referrer)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|r| ReferralRow {
                id: r.get("id"),
                joined_at: r.get("created_at"),
                rewarded_at: r.get("rewarded_at"),
                reward_cents: r.get("reward_cents"),
            })
            .collect())
    }

    async fn unpaid_referral_for(&self, tenant: Uuid, invitee: Uuid) -> anyhow::Result<Option<(Uuid, Uuid)>> {
        Ok(sqlx::query_as(
            "SELECT id, referrer_account_id FROM promotions.referrals
             WHERE tenant_id = $1 AND invitee_account_id = $2 AND rewarded_at IS NULL",
        )
        .bind(tenant)
        .bind(invitee)
        .fetch_optional(self.pool())
        .await?)
    }

    async fn pay_referral(&self, referral: Uuid, tenant: Uuid, referrer: Uuid, amount: i64, currency: &str) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        let marked = sqlx::query(
            "UPDATE promotions.referrals SET rewarded_at = NOW(), reward_cents = $2
             WHERE id = $1 AND rewarded_at IS NULL",
        )
        .bind(referral)
        .bind(amount)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if marked == 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        if amount > 0 {
            sqlx::query(
                "INSERT INTO promotions.credit_entries
                     (tenant_id, account_id, amount_cents, currency, kind, referral_id, note)
                 VALUES ($1, $2, $3, $4, 'referral_reward', $5, 'A friend you referred made their first move')",
            )
            .bind(tenant)
            .bind(referrer)
            .bind(amount)
            .bind(currency)
            .bind(referral)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    async fn corporate_by_code(&self, tenant: Uuid, code: &str) -> anyhow::Result<Option<Corporate>> {
        let row = sqlx::query(&format!(
            "SELECT {CORPORATE_COLUMNS} FROM promotions.corporate_accounts c WHERE c.tenant_id = $1 AND c.code = $2"
        ))
        .bind(tenant)
        .bind(code)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.as_ref().map(to_corporate))
    }

    async fn corporate_by_domain(&self, tenant: Uuid, domain: &str) -> anyhow::Result<Option<Corporate>> {
        let row = sqlx::query(&format!(
            "SELECT {CORPORATE_COLUMNS} FROM promotions.corporate_accounts c
             WHERE c.tenant_id = $1 AND c.active AND lower(c.email_domain) = lower($2) LIMIT 1"
        ))
        .bind(tenant)
        .bind(domain)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.as_ref().map(to_corporate))
    }

    async fn linked_corporate(&self, tenant: Uuid, account: Uuid) -> anyhow::Result<Option<Corporate>> {
        let row = sqlx::query(&format!(
            "SELECT {CORPORATE_COLUMNS} FROM promotions.corporate_links l
             JOIN promotions.corporate_accounts c ON c.id = l.corporate_id
             WHERE l.tenant_id = $1 AND l.account_id = $2"
        ))
        .bind(tenant)
        .bind(account)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.as_ref().map(to_corporate))
    }

    async fn link_corporate(&self, tenant: Uuid, account: Uuid, corporate: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO promotions.corporate_links (tenant_id, account_id, corporate_id) VALUES ($1, $2, $3)
             ON CONFLICT (tenant_id, account_id) DO UPDATE SET corporate_id = EXCLUDED.corporate_id, linked_at = NOW()",
        )
        .bind(tenant)
        .bind(account)
        .bind(corporate)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    async fn unlink_corporate(&self, tenant: Uuid, account: Uuid) -> anyhow::Result<bool> {
        let done = sqlx::query("DELETE FROM promotions.corporate_links WHERE tenant_id = $1 AND account_id = $2")
            .bind(tenant)
            .bind(account)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    async fn list_corporates(&self, tenant: Uuid) -> anyhow::Result<Vec<Corporate>> {
        let rows = sqlx::query(&format!(
            "SELECT {CORPORATE_COLUMNS} FROM promotions.corporate_accounts c WHERE c.tenant_id = $1 ORDER BY c.firm_name"
        ))
        .bind(tenant)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.iter().map(to_corporate).collect())
    }

    async fn create_corporate(&self, tenant: Uuid, c: &NewCorporate) -> anyhow::Result<Option<Corporate>> {
        let row = sqlx::query(
            "INSERT INTO promotions.corporate_accounts AS c (tenant_id, code, firm_name, percent_bps, email_domain)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (tenant_id, code) DO NOTHING
             RETURNING c.id, c.code, c.firm_name, c.percent_bps, c.email_domain, c.active",
        )
        .bind(tenant)
        .bind(&c.code)
        .bind(&c.firm_name)
        .bind(c.percent_bps)
        .bind(&c.email_domain)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.as_ref().map(to_corporate))
    }

    async fn set_corporate_active(&self, tenant: Uuid, id: Uuid, active: bool) -> anyhow::Result<bool> {
        let done = sqlx::query("UPDATE promotions.corporate_accounts SET active = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(tenant)
            .bind(id)
            .bind(active)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() == 1)
    }
}
