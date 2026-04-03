use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{ApiKeyId, TenantId};
use crate::domain::{entities::ApiKey, repositories::ApiKeyRepository};

pub struct PgApiKeyRepository {
    pool: PgPool,
}

impl PgApiKeyRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id:           uuid::Uuid,
    tenant_id:    uuid::Uuid,
    name:         String,
    key_hash:     String,
    key_prefix:   String,
    scopes:       Vec<String>,
    is_active:    bool,
    expires_at:   Option<chrono::DateTime<chrono::Utc>>,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at:   chrono::DateTime<chrono::Utc>,
}

impl From<ApiKeyRow> for ApiKey {
    fn from(r: ApiKeyRow) -> Self {
        ApiKey {
            id:           ApiKeyId::from_uuid(r.id),
            tenant_id:    TenantId::from_uuid(r.tenant_id),
            name:         r.name,
            key_hash:     r.key_hash,
            key_prefix:   r.key_prefix,
            scopes:       r.scopes,
            is_active:    r.is_active,
            expires_at:   r.expires_at,
            last_used_at: r.last_used_at,
            created_at:   r.created_at,
        }
    }
}

#[async_trait]
impl ApiKeyRepository for PgApiKeyRepository {
    /// Look up an API key by its SHA-256 hash — the only way keys are authenticated.
    /// The raw key is never stored; callers hash before calling this.
    async fn find_by_hash(&self, key_hash: &str) -> anyhow::Result<Option<ApiKey>> {
        let row = sqlx::query_as::<_, ApiKeyRow>(
            r#"SELECT id, tenant_id, name, key_hash, key_prefix, scopes,
                      is_active, expires_at, last_used_at, created_at
               FROM identity.api_keys
               WHERE key_hash = $1 AND is_active = true"#
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(ApiKey::from))
    }

    async fn save(&self, key: &ApiKey) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO identity.api_keys
                   (id, tenant_id, name, key_hash, key_prefix, scopes,
                    is_active, expires_at, last_used_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
               ON CONFLICT (id) DO UPDATE SET
                   name         = EXCLUDED.name,
                   scopes       = EXCLUDED.scopes,
                   is_active    = EXCLUDED.is_active,
                   expires_at   = EXCLUDED.expires_at,
                   last_used_at = EXCLUDED.last_used_at"#
        )
        .bind(key.id.inner())
        .bind(key.tenant_id.inner())
        .bind(&key.name)
        .bind(&key.key_hash)
        .bind(&key.key_prefix)
        .bind(&key.scopes)
        .bind(key.is_active)
        .bind(key.expires_at)
        .bind(key.last_used_at)
        .bind(key.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<ApiKey>> {
        let rows = sqlx::query_as::<_, ApiKeyRow>(
            r#"SELECT id, tenant_id, name, key_hash, key_prefix, scopes,
                      is_active, expires_at, last_used_at, created_at
               FROM identity.api_keys
               WHERE tenant_id = $1
               ORDER BY created_at DESC"#
        )
        .bind(tenant_id.inner())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(ApiKey::from).collect())
    }

    /// Hard revoke: set is_active = false immediately.
    /// The key remains in the table for audit purposes.
    async fn revoke(&self, id: &ApiKeyId) -> anyhow::Result<()> {
        sqlx::query("UPDATE identity.api_keys SET is_active = false WHERE id = $1")
            .bind(id.inner())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
