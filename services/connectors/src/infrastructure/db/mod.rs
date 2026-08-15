use async_trait::async_trait;
use chrono::{DateTime, Utc};
use logisticos_errors::{AppError, AppResult};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::{
    entities::{ConnectorCredentials, Platform},
    repositories::CredentialsRepository,
};

pub struct PgCredentialsRepository {
    pool: PgPool,
}

impl PgCredentialsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_creds(r: &sqlx::postgres::PgRow) -> ConnectorCredentials {
    let platform_str: String = r.get("platform");
    ConnectorCredentials {
        id:             r.get("id"),
        tenant_id:      r.get("tenant_id"),
        merchant_id:    r.get("merchant_id"),
        tenant_slug:    r.get("tenant_slug"),
        platform:       Platform::from_str(&platform_str).unwrap_or(Platform::Shopify),
        webhook_secret: r.get("webhook_secret"),
        config:         r.get("config"),
        is_active:      r.get("is_active"),
        last_synced_at: r.get::<Option<DateTime<Utc>>, _>("last_synced_at"),
        sync_interval_mins: r.get("sync_interval_mins"),
        created_at:     r.get::<DateTime<Utc>, _>("created_at"),
    }
}

#[async_trait]
impl CredentialsRepository for PgCredentialsRepository {
    async fn find(
        &self,
        tenant_id: Uuid,
        platform: &str,
    ) -> AppResult<Option<ConnectorCredentials>> {
        let row = sqlx::query(
            r#"SELECT id, tenant_id, merchant_id, tenant_slug, platform,
                      webhook_secret, config, is_active, created_at
               FROM connectors.credentials
               WHERE tenant_id = $1 AND platform = $2 AND is_active = true
               LIMIT 1"#,
        )
        .bind(tenant_id)
        .bind(platform)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(row.as_ref().map(row_to_creds))
    }

    async fn claim_due_syncs(&self, limit: i64) -> AppResult<Vec<ConnectorCredentials>> {
        // One statement: pick the due rows, lock them, stamp them, return them.
        //
        // `FOR UPDATE SKIP LOCKED` is what makes a second replica's sweep step
        // over rows this one already holds rather than blocking on them or —
        // far worse — reading them before the stamp lands and syncing the same
        // vendor twice.
        //
        // The stamp goes on at *claim* time, not on completion. A sync that
        // hangs or panics must not leave its connector permanently due and
        // re-claimed on every tick; it waits out its interval like any other.
        let rows = sqlx::query(
            r#"
            UPDATE connectors.credentials c
               SET last_synced_at = NOW()
              FROM (
                    SELECT id
                      FROM connectors.credentials
                     WHERE is_active = true
                       AND sync_interval_mins IS NOT NULL
                       AND (last_synced_at IS NULL
                            OR last_synced_at < NOW()
                               - (sync_interval_mins || ' minutes')::interval)
                     ORDER BY last_synced_at NULLS FIRST
                     LIMIT $1
                       FOR UPDATE SKIP LOCKED
                   ) due
             WHERE c.id = due.id
         RETURNING c.id, c.tenant_id, c.merchant_id, c.tenant_slug, c.platform,
                   c.webhook_secret, c.config, c.is_active, c.created_at
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(rows.iter().map(row_to_creds).collect())
    }

    async fn record_sync_result(&self, id: Uuid, error: Option<&str>) -> AppResult<()> {
        sqlx::query(
            "UPDATE connectors.credentials
                SET last_sync_error = $2, updated_at = NOW()
              WHERE id = $1",
        )
        .bind(id)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    async fn upsert(&self, creds: &ConnectorCredentials) -> AppResult<()> {
        let now = Utc::now();
        sqlx::query(
            r#"INSERT INTO connectors.credentials
                   (id, tenant_id, merchant_id, tenant_slug, platform,
                    webhook_secret, config, is_active, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
               ON CONFLICT (tenant_id, platform) DO UPDATE SET
                   merchant_id    = EXCLUDED.merchant_id,
                   tenant_slug    = EXCLUDED.tenant_slug,
                   webhook_secret = EXCLUDED.webhook_secret,
                   config         = EXCLUDED.config,
                   is_active      = EXCLUDED.is_active,
                   updated_at     = EXCLUDED.updated_at"#,
        )
        .bind(creds.id)
        .bind(creds.tenant_id)
        .bind(creds.merchant_id)
        .bind(&creds.tenant_slug)
        .bind(creds.platform.as_str())
        .bind(&creds.webhook_secret)
        .bind(&creds.config)
        .bind(creds.is_active)
        .bind(creds.created_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    async fn delete(&self, tenant_id: Uuid, platform: &str) -> AppResult<()> {
        sqlx::query(
            "UPDATE connectors.credentials SET is_active = false, updated_at = NOW()
             WHERE tenant_id = $1 AND platform = $2",
        )
        .bind(tenant_id)
        .bind(platform)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    async fn list_for_tenant(&self, tenant_id: Uuid) -> AppResult<Vec<ConnectorCredentials>> {
        let rows = sqlx::query(
            r#"SELECT id, tenant_id, merchant_id, tenant_slug, platform,
                      webhook_secret, config, is_active, created_at
               FROM connectors.credentials
               WHERE tenant_id = $1 AND is_active = true
               ORDER BY platform"#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(rows.iter().map(row_to_creds).collect())
    }
}
