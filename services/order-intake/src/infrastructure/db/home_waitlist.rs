//! The priority waitlist for fully booked home-move dates: first come, first
//! served, with a short hold on a window when one opens.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::value_objects::home_move::BookedMove;

/// How long an offered window is held for the customer it was offered to.
pub const HOLD_MINUTES: i64 = 5;

#[derive(Debug, Clone, Serialize)]
pub struct WaitlistEntry {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    /// The priced move, as the quote's payload.
    #[serde(skip_serializing)]
    pub quote: serde_json::Value,
    pub wanted_date: NaiveDate,
    pub large_estate: bool,
    pub international: bool,
    /// waiting | offered | booked | lapsed | withdrawn
    pub status: String,
    pub offered_move_at: Option<DateTime<Utc>>,
    pub hold_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Places ahead in the queue for the same date, while waiting.
    pub ahead: Option<i64>,
}

const COLUMNS: &str = "id, tenant_id, account_id, quote, wanted_date, large_estate, international, status,
                       offered_move_at, hold_expires_at, created_at";

fn entry(r: &sqlx::postgres::PgRow) -> WaitlistEntry {
    WaitlistEntry {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        account_id: r.get("account_id"),
        quote: r.get("quote"),
        wanted_date: r.get("wanted_date"),
        large_estate: r.get("large_estate"),
        international: r.get("international"),
        status: r.get("status"),
        offered_move_at: r.get("offered_move_at"),
        hold_expires_at: r.get("hold_expires_at"),
        created_at: r.get("created_at"),
        ahead: None,
    }
}

/// Queue for a date. None when this customer already holds a live place for it.
pub async fn join(
    pool: &PgPool,
    tenant_id: Uuid,
    account_id: Uuid,
    quote: &serde_json::Value,
    wanted_date: NaiveDate,
    large_estate: bool,
    international: bool,
) -> anyhow::Result<Option<WaitlistEntry>> {
    let row = sqlx::query(&format!(
        "INSERT INTO order_intake.home_waitlist (tenant_id, account_id, quote, wanted_date, large_estate, international)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (account_id, wanted_date) WHERE status IN ('waiting', 'offered') DO NOTHING
         RETURNING {COLUMNS}"
    ))
    .bind(tenant_id)
    .bind(account_id)
    .bind(quote)
    .bind(wanted_date)
    .bind(large_estate)
    .bind(international)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(entry))
}

/// A customer's live places, and any offer still in its hold, with their
/// place in the queue.
pub async fn mine(pool: &PgPool, tenant_id: Uuid, account_id: Uuid) -> anyhow::Result<Vec<WaitlistEntry>> {
    let rows = sqlx::query(&format!(
        "SELECT {COLUMNS},
                (SELECT COUNT(*) FROM order_intake.home_waitlist o
                  WHERE o.tenant_id = w.tenant_id AND o.wanted_date = w.wanted_date
                    AND o.status = 'waiting' AND o.created_at < w.created_at) AS ahead
           FROM order_intake.home_waitlist w
          WHERE w.tenant_id = $1 AND w.account_id = $2 AND w.status IN ('waiting', 'offered')
          ORDER BY w.wanted_date"
    ))
    .bind(tenant_id)
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| {
            let mut e = entry(r);
            e.ahead = (e.status == "waiting").then(|| r.get::<i64, _>("ahead"));
            e
        })
        .collect())
}

pub async fn get(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<WaitlistEntry>> {
    let row = sqlx::query(&format!("SELECT {COLUMNS} FROM order_intake.home_waitlist WHERE tenant_id = $1 AND id = $2"))
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.as_ref().map(entry))
}

pub async fn withdraw(pool: &PgPool, tenant_id: Uuid, account_id: Uuid, id: Uuid) -> anyhow::Result<bool> {
    Ok(sqlx::query(
        "UPDATE order_intake.home_waitlist SET status = 'withdrawn', updated_at = NOW()
          WHERE tenant_id = $1 AND account_id = $2 AND id = $3 AND status IN ('waiting', 'offered')",
    )
    .bind(tenant_id)
    .bind(account_id)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected()
        > 0)
}

/// Windows held for someone, as bookings everyone else's calendar counts —
/// except `except`, the hold the booking customer is claiming.
pub async fn holds(pool: &PgPool, tenant_id: Uuid, except: Option<Uuid>) -> anyhow::Result<Vec<BookedMove>> {
    let rows = sqlx::query(
        "SELECT offered_move_at, large_estate, international FROM order_intake.home_waitlist
          WHERE tenant_id = $1 AND status = 'offered' AND hold_expires_at > NOW()
            AND offered_move_at IS NOT NULL AND id IS DISTINCT FROM $2",
    )
    .bind(tenant_id)
    .bind(except)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| BookedMove {
            move_at: r.get("offered_move_at"),
            survey_at: None,
            large_estate: r.get("large_estate"),
            international: r.get("international"),
        })
        .collect())
}

/// Offer a waiting place a window, held until `expires`. False when it is no
/// longer waiting.
pub async fn offer(pool: &PgPool, id: Uuid, move_at: DateTime<Utc>, expires: DateTime<Utc>) -> anyhow::Result<bool> {
    Ok(sqlx::query(
        "UPDATE order_intake.home_waitlist
            SET status = 'offered', offered_move_at = $2, hold_expires_at = $3, updated_at = NOW()
          WHERE id = $1 AND status = 'waiting'",
    )
    .bind(id)
    .bind(move_at)
    .bind(expires)
    .execute(pool)
    .await?
    .rows_affected()
        > 0)
}

/// The hold was claimed: the move is booked.
pub async fn mark_booked(pool: &PgPool, id: Uuid) -> anyhow::Result<()> {
    sqlx::query("UPDATE order_intake.home_waitlist SET status = 'booked', updated_at = NOW() WHERE id = $1 AND status = 'offered'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Holds not claimed in time lapse; the next in line gets the window.
pub async fn lapse_expired(pool: &PgPool) -> anyhow::Result<u64> {
    Ok(sqlx::query(
        "UPDATE order_intake.home_waitlist SET status = 'lapsed', updated_at = NOW()
          WHERE status = 'offered' AND hold_expires_at <= NOW()",
    )
    .execute(pool)
    .await?
    .rows_affected())
}

/// Everyone waiting for a date from `from` on, first come first served.
pub async fn waiting(pool: &PgPool, from: NaiveDate) -> anyhow::Result<Vec<WaitlistEntry>> {
    let rows = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM order_intake.home_waitlist
          WHERE status = 'waiting' AND wanted_date >= $1
          ORDER BY tenant_id, wanted_date, created_at"
    ))
    .bind(from)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(entry).collect())
}

/// A date that has passed can no longer be offered.
pub async fn lapse_past(pool: &PgPool, today: NaiveDate) -> anyhow::Result<u64> {
    Ok(sqlx::query(
        "UPDATE order_intake.home_waitlist SET status = 'lapsed', updated_at = NOW()
          WHERE status = 'waiting' AND wanted_date < $1",
    )
    .bind(today)
    .execute(pool)
    .await?
    .rows_affected())
}
