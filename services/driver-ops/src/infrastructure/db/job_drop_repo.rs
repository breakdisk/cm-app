use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::repositories::{EarningAdjustment, JobDrop, JobDropRepository};

pub struct PgJobDropRepository {
    pool: PgPool,
}

impl PgJobDropRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn to_drop(r: &sqlx::postgres::PgRow) -> JobDrop {
    JobDrop {
        tenant_id:       r.get("tenant_id"),
        driver_id:       r.get("driver_id"),
        route_id:        r.get("route_id"),
        shipment_id:     r.get("shipment_id"),
        tracking_number: r.get("tracking_number"),
        reason_code:     r.get("reason_code"),
        note:            r.get("note"),
        lat:             r.get("lat"),
        lng:             r.get("lng"),
        payout_cents:    r.get("payout_cents"),
        fee_cents:       r.get("fee_cents"),
        dropped_at:      r.get("dropped_at"),
    }
}

#[async_trait]
impl JobDropRepository for PgJobDropRepository {
    async fn drop_job(&self, drop: &JobDrop, task_ids: &[Uuid]) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;

        // Every task must still be open. One that was completed or failed in
        // the meantime means the job is no longer the driver's to drop.
        let cancelled = sqlx::query(
            r#"UPDATE driver_ops.tasks
               SET status = 'cancelled'
               WHERE id = ANY($1)
                 AND driver_id = $2
                 AND status IN ('pending', 'in_progress')"#,
        )
        .bind(task_ids)
        .bind(drop.driver_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if cancelled != task_ids.len() as u64 {
            tx.rollback().await?;
            return Ok(false);
        }

        sqlx::query(
            r#"INSERT INTO driver_ops.job_drops
                   (tenant_id, driver_id, route_id, shipment_id, tracking_number,
                    reason_code, note, lat, lng, payout_cents, fee_cents, dropped_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        )
        .bind(drop.tenant_id)
        .bind(drop.driver_id)
        .bind(drop.route_id)
        .bind(drop.shipment_id)
        .bind(&drop.tracking_number)
        .bind(&drop.reason_code)
        .bind(&drop.note)
        .bind(drop.lat)
        .bind(drop.lng)
        .bind(drop.payout_cents)
        .bind(drop.fee_cents)
        .bind(drop.dropped_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(true)
    }

    async fn find(&self, driver_id: Uuid, shipment_id: Uuid, route_id: Uuid) -> anyhow::Result<Option<JobDrop>> {
        let row = sqlx::query(
            r#"SELECT tenant_id, driver_id, route_id, shipment_id, tracking_number,
                      reason_code, note, lat, lng, payout_cents, fee_cents, dropped_at
               FROM driver_ops.job_drops
               WHERE driver_id = $1 AND shipment_id = $2 AND route_id = $3"#,
        )
        .bind(driver_id)
        .bind(shipment_id)
        .bind(route_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.as_ref().map(to_drop))
    }

    async fn offer_drop_count(&self, driver_id: Uuid) -> anyhow::Result<i64> {
        // Cross-schema read of dispatch's offers, the precedent set by
        // `get_offer_stats`. Soft-fails to 0 where dispatch's schema is not
        // reachable; the offer stats are then absent too, so no rate is shown.
        let count: Result<(i64,), _> = sqlx::query_as(
            r#"SELECT COUNT(*)::bigint
               FROM driver_ops.job_drops jd
               WHERE jd.driver_id = $1
                 AND EXISTS (
                     SELECT 1 FROM dispatch.task_offers o
                     WHERE o.shipment_id = jd.shipment_id AND o.claimed_by = $1
                 )"#,
        )
        .bind(driver_id)
        .fetch_one(&self.pool)
        .await;
        match count {
            Ok((n,)) => Ok(n),
            Err(e) => {
                tracing::debug!(err = %e, "offer drop count unavailable (dispatch schema not reachable)");
                Ok(0)
            }
        }
    }

    async fn claimed_from_offer(&self, driver_id: Uuid, shipment_id: Uuid) -> anyhow::Result<bool> {
        let found: Result<(bool,), _> = sqlx::query_as(
            r#"SELECT EXISTS (
                   SELECT 1 FROM dispatch.task_offers
                   WHERE shipment_id = $1 AND claimed_by = $2
               )"#,
        )
        .bind(shipment_id)
        .bind(driver_id)
        .fetch_one(&self.pool)
        .await;
        match found {
            Ok((b,)) => Ok(b),
            Err(e) => {
                tracing::debug!(err = %e, "offer lookup unavailable (dispatch schema not reachable)");
                Ok(false)
            }
        }
    }

    async fn list_adjustments(&self, driver_id: Uuid, from: DateTime<Utc>, to: DateTime<Utc>) -> anyhow::Result<Vec<EarningAdjustment>> {
        let rows = sqlx::query(
            r#"SELECT 'waiting_fee' AS kind, t.id AS reference_id, t.tracking_number,
                      t.waiting_fee_cents AS amount_cents, t.completed_at AS at
               FROM driver_ops.tasks t
               WHERE t.driver_id = $1
                 AND t.waiting_fee_cents > 0
                 AND t.completed_at >= $2 AND t.completed_at < $3
               UNION ALL
               SELECT 'drop_fee', jd.id, jd.tracking_number, -jd.fee_cents, jd.dropped_at
               FROM driver_ops.job_drops jd
               WHERE jd.driver_id = $1
                 AND jd.fee_cents > 0
                 AND jd.dropped_at >= $2 AND jd.dropped_at < $3
               UNION ALL
               SELECT ec.kind, ec.reference_id, ec.tracking_number, ec.amount_cents, ec.credited_at
               FROM driver_ops.earning_credits ec
               WHERE ec.driver_id = $1
                 AND ec.credited_at >= $2 AND ec.credited_at < $3
               ORDER BY at DESC
               LIMIT 200"#,
        )
        .bind(driver_id)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| EarningAdjustment {
            kind:            r.get("kind"),
            reference_id:    r.get("reference_id"),
            tracking_number: r.get("tracking_number"),
            amount_cents:    r.get("amount_cents"),
            at:              r.get("at"),
        }).collect())
    }
}
