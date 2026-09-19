//! The survey's addendum: what the surveyor found beyond the booking, and
//! the customer's answer. One open at a time per move.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct AddendumRecord {
    pub id: Uuid,
    pub shipment_id: Uuid,
    pub tenant_id: Uuid,
    pub submitted_by: Uuid,
    pub items: serde_json::Value,
    pub extras: serde_json::Value,
    pub note: String,
    pub items_cents: i64,
    pub extras_cents: i64,
    pub total_cents: i64,
    pub currency: String,
    pub trucks: i32,
    pub helpers: i32,
    pub crew_total: i32,
    pub large_estate: bool,
    pub status: String,
    pub checkout_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    /// The move's trucks when this was paid: more after it means a major
    /// overflow, and an extra truck.
    pub trucks_before: Option<i32>,
    /// The one-time code the customer gives the lead to approve on the lead's
    /// phone. Never serialised: the handler adds it for the customer only.
    #[serde(skip_serializing)]
    pub approval_code: Option<String>,
    #[serde(skip_serializing)]
    pub approval_attempts: i32,
}

/// Wrong on-site codes before it locks.
pub const APPROVAL_CODE_TRIES: i32 = 5;

/// A fresh six-digit on-site approval code.
pub fn new_approval_code() -> String {
    use rand::Rng;
    format!("{:06}", rand::thread_rng().gen_range(0..1_000_000))
}

/// Whether `given` is the code, in constant time.
pub fn code_matches(expected: &str, given: &str) -> bool {
    use subtle::ConstantTimeEq;
    let given = given.trim();
    expected.len() == given.len() && bool::from(expected.as_bytes().ct_eq(given.as_bytes()))
}

/// Take one of the code's tries, before comparing: parallel guesses each
/// take one or find none left. The tries used, or None when locked or no
/// longer pending.
pub async fn claim_code_try(pool: &PgPool, id: Uuid) -> anyhow::Result<Option<i32>> {
    Ok(sqlx::query_scalar(
        "UPDATE order_intake.home_addenda SET approval_attempts = approval_attempts + 1
          WHERE id = $1 AND status = 'pending' AND approval_attempts < $2
          RETURNING approval_attempts",
    )
    .bind(id)
    .bind(APPROVAL_CODE_TRIES)
    .fetch_optional(pool)
    .await?)
}

pub struct NewAddendum {
    pub shipment_id: Uuid,
    pub tenant_id: Uuid,
    pub submitted_by: Uuid,
    pub items: serde_json::Value,
    pub extras: serde_json::Value,
    pub note: String,
    pub items_cents: i64,
    pub extras_cents: i64,
    pub currency: String,
    pub trucks: i32,
    pub helpers: i32,
    pub crew_total: i32,
    pub large_estate: bool,
    pub approval_code: String,
}

const COLUMNS: &str = "id, shipment_id, tenant_id, submitted_by, items, extras, note, items_cents, extras_cents,
    total_cents, currency, trucks, helpers, crew_total, large_estate, status, checkout_url, created_at, decided_at, paid_at,
    trucks_before, approval_code, approval_attempts";

fn to_record(r: &sqlx::postgres::PgRow) -> AddendumRecord {
    AddendumRecord {
        id: r.get("id"),
        shipment_id: r.get("shipment_id"),
        tenant_id: r.get("tenant_id"),
        submitted_by: r.get("submitted_by"),
        items: r.get("items"),
        extras: r.get("extras"),
        note: r.get("note"),
        items_cents: r.get("items_cents"),
        extras_cents: r.get("extras_cents"),
        total_cents: r.get("total_cents"),
        currency: r.get("currency"),
        trucks: r.get("trucks"),
        helpers: r.get("helpers"),
        crew_total: r.get("crew_total"),
        large_estate: r.get("large_estate"),
        status: r.get("status"),
        checkout_url: r.get("checkout_url"),
        created_at: r.get("created_at"),
        decided_at: r.get("decided_at"),
        paid_at: r.get("paid_at"),
        trucks_before: r.get("trucks_before"),
        approval_code: r.get("approval_code"),
        approval_attempts: r.get("approval_attempts"),
    }
}

/// A move's paid addenda, in the order they were paid.
pub async fn paid_for(pool: &PgPool, shipment_id: Uuid) -> anyhow::Result<Vec<AddendumRecord>> {
    let rows = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM order_intake.home_addenda
          WHERE shipment_id = $1 AND status = 'paid' ORDER BY paid_at"
    ))
    .bind(shipment_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(to_record).collect())
}

/// None when one is already open for this move.
pub async fn insert(pool: &PgPool, a: &NewAddendum) -> anyhow::Result<Option<AddendumRecord>> {
    let row = sqlx::query(&format!(
        "INSERT INTO order_intake.home_addenda
                (shipment_id, tenant_id, submitted_by, items, extras, note, items_cents, extras_cents,
                 total_cents, currency, trucks, helpers, crew_total, large_estate, approval_code)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $7 + $8, $9, $10, $11, $12, $13, $14)
         ON CONFLICT DO NOTHING
         RETURNING {COLUMNS}"
    ))
    .bind(a.shipment_id)
    .bind(a.tenant_id)
    .bind(a.submitted_by)
    .bind(&a.items)
    .bind(&a.extras)
    .bind(&a.note)
    .bind(a.items_cents)
    .bind(a.extras_cents)
    .bind(&a.currency)
    .bind(a.trucks)
    .bind(a.helpers)
    .bind(a.crew_total)
    .bind(a.large_estate)
    .bind(&a.approval_code)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(to_record))
}

pub async fn latest(pool: &PgPool, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<Option<AddendumRecord>> {
    let row = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM order_intake.home_addenda
          WHERE tenant_id = $1 AND shipment_id = $2 ORDER BY created_at DESC LIMIT 1"
    ))
    .bind(tenant_id)
    .bind(shipment_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(to_record))
}

pub async fn get(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<AddendumRecord>> {
    let row = sqlx::query(&format!("SELECT {COLUMNS} FROM order_intake.home_addenda WHERE tenant_id = $1 AND id = $2"))
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.as_ref().map(to_record))
}

/// Pending → approved, with the payment it now waits on. False when it was
/// not pending (decided already, or someone else's).
pub async fn approve(pool: &PgPool, id: Uuid, intent_id: Uuid, checkout_url: &str) -> anyhow::Result<bool> {
    let done = sqlx::query(
        "UPDATE order_intake.home_addenda
            SET status = 'approved', decided_at = NOW(), payment_intent_id = $2, checkout_url = $3
          WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(intent_id)
    .bind(checkout_url)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn decline(pool: &PgPool, id: Uuid) -> anyhow::Result<bool> {
    let done = sqlx::query(
        "UPDATE order_intake.home_addenda SET status = 'declined', decided_at = NOW()
          WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The survey is done: from now the deposit is kept on a cancellation.
pub async fn mark_survey_submitted(pool: &PgPool, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE order_intake.home_moves SET survey_submitted_at = COALESCE(survey_submitted_at, NOW())
          WHERE tenant_id = $1 AND shipment_id = $2",
    )
    .bind(tenant_id)
    .bind(shipment_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_is_six_digits_and_matches_only_itself() {
        let c = new_approval_code();
        assert_eq!(c.len(), 6);
        assert!(c.chars().all(|d| d.is_ascii_digit()));
        assert!(code_matches(&c, &c));
        assert!(code_matches("004210", " 004210 "));
        assert!(!code_matches("004210", "4210"));
        assert!(!code_matches("004210", "004211"));
        assert!(!code_matches("004210", ""));
    }
}
