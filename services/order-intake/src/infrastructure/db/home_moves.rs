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
}

/// Once per shipment: a retried booking (same idempotency key, same
/// shipment) writes nothing new.
pub async fn insert(pool: &PgPool, r: &HomeMoveRecord) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO order_intake.home_moves
               (shipment_id, tenant_id, account_id, property, items, plan, distance_centikm,
                survey_required, survey_at, move_at, total_cents, currency)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
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
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get(pool: &PgPool, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<Option<HomeMoveRecord>> {
    let row = sqlx::query(
        r#"SELECT shipment_id, tenant_id, account_id, property, items, plan, distance_centikm,
                  survey_required, survey_at, move_at, total_cents, currency
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
    }))
}
