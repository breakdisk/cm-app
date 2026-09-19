//! What only a booked home move has: the property, the inventory as priced,
//! the plan and the survey. Keyed by the shipment it belongs to.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct HomeMoveRecord {
    pub shipment_id: Uuid,
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub property: serde_json::Value,
    pub items: serde_json::Value,
    pub plan: String,
    pub distance_centikm: i64,
    pub survey_required: bool,
    pub survey_at: Option<DateTime<Utc>>,
    pub move_at: DateTime<Utc>,
    pub total_cents: i64,
    pub currency: String,
    pub trucks: i32,
    pub helpers: i32,
    pub crew_total: i32,
    pub large_estate: bool,
    pub international: bool,
    pub survey_cents: i64,
    pub survey_submitted_at: Option<DateTime<Utc>>,
}

/// Once per shipment: a retried booking (same idempotency key, same
/// shipment) writes nothing new.
pub async fn insert(pool: &PgPool, r: &HomeMoveRecord) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO order_intake.home_moves
               (shipment_id, tenant_id, account_id, property, items, plan, distance_centikm,
                survey_required, survey_at, move_at, total_cents, currency,
                trucks, helpers, crew_total, large_estate, international, survey_cents)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
           ON CONFLICT (shipment_id) DO NOTHING"#,
    )
    .bind(r.shipment_id)
    .bind(r.tenant_id)
    .bind(r.account_id)
    .bind(&r.property)
    .bind(&r.items)
    .bind(&r.plan)
    .bind(r.distance_centikm)
    .bind(r.survey_required)
    .bind(r.survey_at)
    .bind(r.move_at)
    .bind(r.total_cents)
    .bind(&r.currency)
    .bind(r.trucks)
    .bind(r.helpers)
    .bind(r.crew_total)
    .bind(r.large_estate)
    .bind(r.international)
    .bind(r.survey_cents)
    .execute(pool)
    .await?;
    Ok(())
}

/// Moves booked (and not cancelled) whose move or survey falls between the
/// two instants — what a date's teams are already spoken for by.
pub async fn booked_between(
    pool: &PgPool,
    tenant_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> anyhow::Result<Vec<crate::domain::value_objects::home_move::BookedMove>> {
    let rows = sqlx::query(
        r#"SELECT h.move_at, h.survey_at, h.large_estate, h.international
             FROM order_intake.home_moves h
             JOIN order_intake.shipments s ON s.id = h.shipment_id AND s.status <> 'cancelled'
            WHERE h.tenant_id = $1
              AND ((h.move_at BETWEEN $2 AND $3) OR (h.survey_at BETWEEN $2 AND $3))"#,
    )
    .bind(tenant_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| crate::domain::value_objects::home_move::BookedMove {
            move_at: r.get("move_at"),
            survey_at: r.get("survey_at"),
            large_estate: r.get("large_estate"),
            international: r.get("international"),
        })
        .collect())
}

pub async fn get(pool: &PgPool, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<Option<HomeMoveRecord>> {
    let row = sqlx::query(
        r#"SELECT shipment_id, tenant_id, account_id, property, items, plan, distance_centikm,
                  survey_required, survey_at, move_at, total_cents, currency,
                  trucks, helpers, crew_total, large_estate, international, survey_cents, survey_submitted_at
             FROM order_intake.home_moves
            WHERE tenant_id = $1 AND shipment_id = $2"#,
    )
    .bind(tenant_id)
    .bind(shipment_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| HomeMoveRecord {
        shipment_id: r.get("shipment_id"),
        tenant_id: r.get("tenant_id"),
        account_id: r.get("account_id"),
        property: r.get("property"),
        items: r.get("items"),
        plan: r.get("plan"),
        distance_centikm: r.get("distance_centikm"),
        survey_required: r.get("survey_required"),
        survey_at: r.get("survey_at"),
        move_at: r.get("move_at"),
        total_cents: r.get("total_cents"),
        currency: r.get("currency"),
        trucks: r.get("trucks"),
        helpers: r.get("helpers"),
        crew_total: r.get("crew_total"),
        large_estate: r.get("large_estate"),
        international: r.get("international"),
        survey_cents: r.get("survey_cents"),
        survey_submitted_at: r.get("survey_submitted_at"),
    }))
}
