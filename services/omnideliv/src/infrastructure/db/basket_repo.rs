use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{
    Basket, BasketLine, BasketStatus, LineState, SubIntent, SubIntentStatus,
};
use crate::domain::repositories::BasketRepository;
use crate::infrastructure::db::vendor_repo::parse_vertical;

pub struct PgBasketRepository { pool: PgPool }

impl PgBasketRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn basket_status(s: &str) -> anyhow::Result<BasketStatus> {
    Ok(match s {
        "draft"           => BasketStatus::Draft,
        "proposed"        => BasketStatus::Proposed,
        "awaiting_review" => BasketStatus::AwaitingReview,
        "confirmed"       => BasketStatus::Confirmed,
        "abandoned"       => BasketStatus::Abandoned,
        other => anyhow::bail!("unknown basket status in database: {other}"),
    })
}

fn line_state(s: &str) -> anyhow::Result<LineState> {
    Ok(match s {
        "proposed"    => LineState::Proposed,
        "accepted"    => LineState::Accepted,
        "substituted" => LineState::Substituted,
        "rejected"    => LineState::Rejected,
        other => anyhow::bail!("unknown line state in database: {other}"),
    })
}

fn line_state_str(s: LineState) -> &'static str {
    match s {
        LineState::Proposed    => "proposed",
        LineState::Accepted    => "accepted",
        LineState::Substituted => "substituted",
        LineState::Rejected    => "rejected",
    }
}

fn sub_intent_status(s: &str) -> anyhow::Result<SubIntentStatus> {
    Ok(match s {
        "pending"   => SubIntentStatus::Pending,
        "satisfied" => SubIntentStatus::Satisfied,
        "degraded"  => SubIntentStatus::Degraded,
        "failed"    => SubIntentStatus::Failed,
        other => anyhow::bail!("unknown sub-intent status in database: {other}"),
    })
}

fn sub_intent_status_str(s: SubIntentStatus) -> &'static str {
    match s {
        SubIntentStatus::Pending   => "pending",
        SubIntentStatus::Satisfied => "satisfied",
        SubIntentStatus::Degraded  => "degraded",
        SubIntentStatus::Failed    => "failed",
    }
}

#[async_trait]
impl BasketRepository for PgBasketRepository {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Basket>> {
        let Some(b) = sqlx::query("SELECT * FROM omnideliv.baskets WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id).bind(id)
            .fetch_optional(&self.pool).await?
        else {
            return Ok(None);
        };

        let status_str: String = b.get("status");

        let si_rows = sqlx::query("SELECT * FROM omnideliv.sub_intents WHERE basket_id = $1 ORDER BY created_at")
            .bind(id).fetch_all(&self.pool).await?;
        let mut sub_intents = Vec::with_capacity(si_rows.len());
        for r in &si_rows {
            let vertical_str: String = r.get("vertical");
            let st: String = r.get("status");
            sub_intents.push(SubIntent {
                id:          r.get("id"),
                basket_id:   r.get("basket_id"),
                tenant_id:   r.get("tenant_id"),
                vertical:    parse_vertical(&vertical_str)?,
                vendor_hint: r.get("vendor_hint"),
                raw_text:    r.get("raw_text"),
                constraints: r.get("constraints"),
                status:      sub_intent_status(&st)?,
                created_at:  r.get("created_at"),
            });
        }

        let line_rows = sqlx::query("SELECT * FROM omnideliv.basket_lines WHERE basket_id = $1 ORDER BY created_at")
            .bind(id).fetch_all(&self.pool).await?;
        let mut lines = Vec::with_capacity(line_rows.len());
        for r in &line_rows {
            let st: String = r.get("state");
            lines.push(BasketLine {
                id:                r.get("id"),
                basket_id:         r.get("basket_id"),
                sub_intent_id:     r.get("sub_intent_id"),
                tenant_id:         r.get("tenant_id"),
                vendor_id:         r.get("vendor_id"),
                item_id:           r.get("item_id"),
                qty:               r.get("qty"),
                unit_price_cents:  r.get("unit_price_cents"),
                state:             line_state(&st)?,
                substitution_for:  r.get("substitution_for"),
                proposed_by_agent: r.get("proposed_by_agent"),
                created_at:        r.get("created_at"),
            });
        }

        Ok(Some(Basket {
            id:              b.get("id"),
            tenant_id:       b.get("tenant_id"),
            customer_id:     b.get("customer_id"),
            status:          basket_status(&status_str)?,
            mesh_session_id: b.get("mesh_session_id"),
            sub_intents,
            lines,
            created_at:      b.get("created_at"),
            updated_at:      b.get("updated_at"),
        }))
    }

    async fn save(&self, basket: &Basket) -> anyhow::Result<()> {
        // One transaction for the whole aggregate. `apply` replaces a
        // sub-intent's lines in memory, so persistence must mirror that: delete
        // then re-insert, or a removed line survives in the database.
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO omnideliv.baskets (id, tenant_id, customer_id, status, mesh_session_id, created_at, updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7)
            ON CONFLICT (id) DO UPDATE SET
                status          = EXCLUDED.status,
                mesh_session_id = EXCLUDED.mesh_session_id,
                updated_at      = EXCLUDED.updated_at
            "#,
        )
        .bind(basket.id).bind(basket.tenant_id).bind(basket.customer_id)
        .bind(basket.status.as_str()).bind(basket.mesh_session_id)
        .bind(basket.created_at).bind(basket.updated_at)
        .execute(&mut *tx).await?;

        for si in &basket.sub_intents {
            sqlx::query(
                r#"
                INSERT INTO omnideliv.sub_intents (
                    id, basket_id, tenant_id, vertical, vendor_hint, raw_text,
                    constraints, status, created_at
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                ON CONFLICT (id) DO UPDATE SET status = EXCLUDED.status
                "#,
            )
            .bind(si.id).bind(si.basket_id).bind(si.tenant_id)
            .bind(si.vertical.as_str()).bind(&si.vendor_hint).bind(&si.raw_text)
            .bind(&si.constraints).bind(sub_intent_status_str(si.status)).bind(si.created_at)
            .execute(&mut *tx).await?;
        }

        // Lines are replaced wholesale to mirror `Basket::apply`.
        sqlx::query("DELETE FROM omnideliv.basket_lines WHERE basket_id = $1")
            .bind(basket.id)
            .execute(&mut *tx).await?;

        for l in &basket.lines {
            sqlx::query(
                r#"
                INSERT INTO omnideliv.basket_lines (
                    id, basket_id, sub_intent_id, tenant_id, vendor_id, item_id,
                    qty, unit_price_cents, state, substitution_for,
                    proposed_by_agent, created_at
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                "#,
            )
            .bind(l.id).bind(l.basket_id).bind(l.sub_intent_id).bind(l.tenant_id)
            .bind(l.vendor_id).bind(l.item_id).bind(l.qty).bind(l.unit_price_cents)
            .bind(line_state_str(l.state)).bind(l.substitution_for)
            .bind(&l.proposed_by_agent).bind(l.created_at)
            .execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
