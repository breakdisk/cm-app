use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{entities::DutySession, repositories::DutySessionRepository};

pub struct PgDutySessionRepository {
    pool: PgPool,
}

impl PgDutySessionRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl DutySessionRepository for PgDutySessionRepository {
    async fn open(&self, tenant_id: Uuid, driver_id: Uuid, at: DateTime<Utc>) -> anyhow::Result<bool> {
        // `duty_sessions_one_open_per_driver` makes this idempotent: go-online is
        // repeated (the toggle, restore on reconnect) and must not stack sessions.
        let result = sqlx::query(
            r#"
            INSERT INTO driver_ops.duty_sessions (tenant_id, driver_id, started_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (driver_id) WHERE ended_at IS NULL DO NOTHING
            "#,
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn close_open(&self, tenant_id: Uuid, driver_id: Uuid, at: DateTime<Utc>) -> anyhow::Result<bool> {
        // GREATEST keeps the end-after-start check satisfied when this host's
        // clock is behind the one that opened the session.
        let result = sqlx::query(
            r#"
            UPDATE driver_ops.duty_sessions
               SET ended_at = GREATEST($3, started_at)
             WHERE tenant_id = $1 AND driver_id = $2 AND ended_at IS NULL
            "#,
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_overlapping(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        since: DateTime<Utc>,
    ) -> anyhow::Result<Vec<DutySession>> {
        let rows: Vec<(DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
            r#"
            SELECT started_at, ended_at
              FROM driver_ops.duty_sessions
             WHERE tenant_id = $1 AND driver_id = $2
               AND (ended_at IS NULL OR ended_at > $3)
             ORDER BY started_at
            "#,
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(since)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(started_at, ended_at)| DutySession { started_at, ended_at })
            .collect())
    }
}
