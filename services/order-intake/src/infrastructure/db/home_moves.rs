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
    /// The lead's pay, before and after the platform's commission. Never
    /// sent with the record: the customer reads this record.
    #[serde(skip_serializing)]
    pub lead_gross_cents: i64,
    #[serde(skip_serializing)]
    pub lead_commission_cents: i64,
    #[serde(skip_serializing)]
    pub lead_payout_cents: i64,
    /// The window's surge when booked (bps; 10000 is none) and what it added
    /// to the fare. Shown to the customer.
    pub surge_bps: i32,
    pub surge_cents: i64,
    /// The surge, passed to the lead whole; inside `lead_payout_cents`.
    #[serde(skip_serializing)]
    pub lead_bonus_cents: i64,
}

/// Once per shipment: a retried booking (same idempotency key, same
/// shipment) writes nothing new.
pub async fn insert(pool: &PgPool, r: &HomeMoveRecord) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO order_intake.home_moves
               (shipment_id, tenant_id, account_id, property, items, plan, distance_centikm,
                survey_required, survey_at, move_at, total_cents, currency,
                trucks, helpers, crew_total, large_estate, international, survey_cents,
                lead_gross_cents, lead_commission_cents, lead_payout_cents, surge_bps, surge_cents, lead_bonus_cents)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)
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
    .bind(r.lead_gross_cents)
    .bind(r.lead_commission_cents)
    .bind(r.lead_payout_cents)
    .bind(r.surge_bps)
    .bind(r.surge_cents)
    .bind(r.lead_bonus_cents)
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

/// The move by its shipment alone — for a consumer that has no tenant in hand.
pub async fn by_shipment(pool: &PgPool, shipment_id: Uuid) -> anyhow::Result<Option<HomeMoveRecord>> {
    let tenant: Option<Uuid> = sqlx::query_scalar("SELECT tenant_id FROM order_intake.home_moves WHERE shipment_id = $1")
        .bind(shipment_id)
        .fetch_optional(pool)
        .await?;
    match tenant {
        Some(t) => get(pool, t, shipment_id).await,
        None => Ok(None),
    }
}

pub async fn get(pool: &PgPool, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<Option<HomeMoveRecord>> {
    let row = sqlx::query(
        r#"SELECT shipment_id, tenant_id, account_id, property, items, plan, distance_centikm,
                  survey_required, survey_at, move_at, total_cents, currency,
                  trucks, helpers, crew_total, large_estate, international, survey_cents, survey_submitted_at,
                  lead_gross_cents, lead_commission_cents, lead_payout_cents, surge_bps, surge_cents, lead_bonus_cents
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
        lead_gross_cents: r.get("lead_gross_cents"),
        lead_commission_cents: r.get("lead_commission_cents"),
        lead_payout_cents: r.get("lead_payout_cents"),
        surge_bps: r.get("surge_bps"),
        surge_cents: r.get("surge_cents"),
        lead_bonus_cents: r.get("lead_bonus_cents"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_leads_pay_is_never_sent_with_the_record() {
        // The customer reads this record; what the lead is paid is not theirs.
        let r = HomeMoveRecord {
            shipment_id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            account_id: Uuid::nil(),
            property: serde_json::json!({}),
            items: serde_json::json!([]),
            plan: "trucks".into(),
            distance_centikm: 1_840,
            survey_required: true,
            survey_at: None,
            move_at: Utc::now(),
            total_cents: 150_000,
            currency: "PHP".into(),
            trucks: 1,
            helpers: 4,
            crew_total: 5,
            large_estate: false,
            international: false,
            survey_cents: 4_500,
            survey_submitted_at: None,
            lead_gross_cents: 145_500,
            lead_commission_cents: 29_100,
            lead_payout_cents: 116_400,
            surge_bps: 10_000,
            surge_cents: 0,
            lead_bonus_cents: 0,
        };
        let json = serde_json::to_value(&r).expect("serializes");
        for hidden in ["lead_gross_cents", "lead_commission_cents", "lead_payout_cents", "lead_bonus_cents"] {
            assert!(json.get(hidden).is_none(), "{hidden} leaked");
        }
        assert_eq!(json["total_cents"], 150_000);
    }
}
