//! Pay a driver earns outside a delivered task: a home-move survey, a
//! support lead's share of a joint move, an emergency truck. Net of the
//! platform's commission. Earnings read these beside task payouts.

use std::sync::Mutex;

use async_trait::async_trait;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EarningCredit {
    pub tenant_id: Uuid,
    /// The driver's user id.
    pub driver_id: Uuid,
    /// snake_case, e.g. "survey_fee". Free text by design: a new kind of
    /// credited work is data, not a migration.
    pub kind: String,
    /// The shipment the work was for.
    pub reference_id: Uuid,
    #[serde(default)]
    pub tracking_number: Option<String>,
    pub gross_cents: i64,
    pub commission_cents: i64,
    pub amount_cents: i64,
}

impl EarningCredit {
    /// Why this credit would be refused, or None.
    pub fn problem(&self) -> Option<String> {
        let k = self.kind.as_bytes();
        let shaped = (2..=40).contains(&k.len())
            && k[0].is_ascii_lowercase()
            && k.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_');
        if !shaped {
            return Some(format!("\"{}\" is not a credit kind (snake_case, 2–40 characters)", self.kind));
        }
        if self.gross_cents < 0 || self.commission_cents < 0 {
            return Some("Amounts are never negative".into());
        }
        if self.amount_cents != self.gross_cents - self.commission_cents {
            return Some("The amount is the gross less the commission".into());
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credited {
    New,
    /// Already credited: nothing added.
    Already,
    /// No such driver in this tenant.
    UnknownDriver,
}

#[async_trait]
pub trait CreditStore: Send + Sync {
    async fn credit(&self, c: &EarningCredit) -> anyhow::Result<Credited>;
}

pub struct PgCreditStore {
    pool: PgPool,
}

impl PgCreditStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CreditStore for PgCreditStore {
    async fn credit(&self, c: &EarningCredit) -> anyhow::Result<Credited> {
        let known: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM driver_ops.drivers WHERE user_id = $1 AND tenant_id = $2)",
        )
        .bind(c.driver_id)
        .bind(c.tenant_id)
        .fetch_one(&self.pool)
        .await?;
        if !known {
            return Ok(Credited::UnknownDriver);
        }
        let inserted = sqlx::query(
            r#"INSERT INTO driver_ops.earning_credits
                   (tenant_id, driver_id, kind, reference_id, tracking_number, gross_cents, commission_cents, amount_cents)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (driver_id, kind, reference_id) DO NOTHING"#,
        )
        .bind(c.tenant_id)
        .bind(c.driver_id)
        .bind(&c.kind)
        .bind(c.reference_id)
        .bind(c.tracking_number.as_deref().filter(|t| !t.is_empty()))
        .bind(c.gross_cents)
        .bind(c.commission_cents)
        .bind(c.amount_cents)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(if inserted > 0 { Credited::New } else { Credited::Already })
    }
}

/// For tests: credits kept in memory, drivers named up front.
#[derive(Default)]
pub struct InMemoryCreditStore {
    pub drivers: Vec<(Uuid, Uuid)>,
    pub credits: Mutex<Vec<EarningCredit>>,
}

#[async_trait]
impl CreditStore for InMemoryCreditStore {
    async fn credit(&self, c: &EarningCredit) -> anyhow::Result<Credited> {
        if !self.drivers.contains(&(c.tenant_id, c.driver_id)) {
            return Ok(Credited::UnknownDriver);
        }
        let mut all = self.credits.lock().map_err(|_| anyhow::anyhow!("credit store poisoned"))?;
        if all.iter().any(|x| x.driver_id == c.driver_id && x.kind == c.kind && x.reference_id == c.reference_id) {
            return Ok(Credited::Already);
        }
        all.push(c.clone());
        Ok(Credited::New)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credit() -> EarningCredit {
        EarningCredit {
            tenant_id: Uuid::new_v4(),
            driver_id: Uuid::new_v4(),
            kind: "survey_fee".into(),
            reference_id: Uuid::new_v4(),
            tracking_number: None,
            gross_cents: 4_500,
            commission_cents: 900,
            amount_cents: 3_600,
        }
    }

    #[test]
    fn a_credit_is_net_of_commission_and_well_named() {
        assert_eq!(credit().problem(), None);
        assert!(EarningCredit { amount_cents: 4_500, ..credit() }.problem().is_some());
        assert!(EarningCredit { gross_cents: -1, commission_cents: 0, amount_cents: -1, ..credit() }.problem().is_some());
        assert!(EarningCredit { kind: "Survey Fee".into(), ..credit() }.problem().is_some());
        assert!(EarningCredit { kind: "x".into(), ..credit() }.problem().is_some());
        // A new kind needs no migration.
        assert_eq!(EarningCredit { kind: "loading_bonus_2".into(), ..credit() }.problem(), None);
    }

    #[tokio::test]
    async fn credited_once_and_only_to_a_driver_of_the_tenant() {
        let c = credit();
        let store = InMemoryCreditStore { drivers: vec![(c.tenant_id, c.driver_id)], ..Default::default() };
        assert_eq!(store.credit(&c).await.expect("credit"), Credited::New);
        assert_eq!(store.credit(&c).await.expect("credit"), Credited::Already);
        let stranger = EarningCredit { tenant_id: Uuid::new_v4(), ..c };
        assert_eq!(store.credit(&stranger).await.expect("credit"), Credited::UnknownDriver);
    }
}
