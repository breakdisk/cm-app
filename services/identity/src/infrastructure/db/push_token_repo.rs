use sqlx::PgPool;

pub struct PgPushTokenRepository {
    pool: PgPool,
}

impl PgPushTokenRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    /// Upsert a push token for a user. If the same token already exists for the
    /// tenant, the platform/app/device_id fields are refreshed and updated_at ticks.
    pub async fn upsert(
        &self,
        tenant_id: uuid::Uuid,
        user_id: uuid::Uuid,
        token: &str,
        platform: &str,
        app: &str,
        device_id: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO identity.push_tokens (tenant_id, user_id, token, platform, app, device_id)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (tenant_id, token) DO UPDATE
                   SET user_id   = EXCLUDED.user_id,
                       platform  = EXCLUDED.platform,
                       app       = EXCLUDED.app,
                       device_id = EXCLUDED.device_id,
                       updated_at = NOW()"#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(token)
        .bind(platform)
        .bind(app)
        .bind(device_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// List all push tokens registered for a user + app combination.
    /// Used by engagement service (via internal HTTP) to dispatch push notifications.
    pub async fn list_by_user(
        &self,
        user_id: uuid::Uuid,
        app: &str,
    ) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT token FROM identity.push_tokens WHERE user_id = $1 AND app = $2"
        )
        .bind(user_id)
        .bind(app)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(t,)| t).collect())
    }

    /// Delete a push token (on logout / uninstall).
    pub async fn delete(&self, tenant_id: uuid::Uuid, token: &str) -> anyhow::Result<()> {
        sqlx::query(
            r#"DELETE FROM identity.push_tokens
               WHERE tenant_id = $1 AND token = $2"#,
        )
        .bind(tenant_id)
        .bind(token)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
