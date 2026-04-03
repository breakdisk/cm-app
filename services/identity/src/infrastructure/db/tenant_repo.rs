use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{TenantId, SubscriptionTier};
use crate::domain::{entities::Tenant, repositories::TenantRepository};

pub struct PgTenantRepository {
    pool: PgPool,
}

impl PgTenantRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

/// Database row shape — maps directly from sqlx query
#[derive(sqlx::FromRow)]
struct TenantRow {
    id:                uuid::Uuid,
    name:              String,
    slug:              String,
    subscription_tier: String,
    is_active:         bool,
    owner_email:       String,
    created_at:        chrono::DateTime<chrono::Utc>,
    updated_at:        chrono::DateTime<chrono::Utc>,
}

impl From<TenantRow> for Tenant {
    fn from(r: TenantRow) -> Self {
        Tenant {
            id: TenantId::from_uuid(r.id),
            name: r.name,
            slug: r.slug,
            subscription_tier: match r.subscription_tier.as_str() {
                "growth"     => SubscriptionTier::Growth,
                "business"   => SubscriptionTier::Business,
                "enterprise" => SubscriptionTier::Enterprise,
                _            => SubscriptionTier::Starter,
            },
            is_active: r.is_active,
            owner_email: r.owner_email,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[async_trait]
impl TenantRepository for PgTenantRepository {
    async fn find_by_id(&self, id: &TenantId) -> anyhow::Result<Option<Tenant>> {
        let row = sqlx::query_as::<_, TenantRow>(
            "SELECT id, name, slug, subscription_tier, is_active, owner_email, created_at, updated_at
             FROM identity.tenants WHERE id = $1"
        )
        .bind(id.inner())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Tenant::from))
    }

    async fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Tenant>> {
        let row = sqlx::query_as::<_, TenantRow>(
            "SELECT id, name, slug, subscription_tier, is_active, owner_email, created_at, updated_at
             FROM identity.tenants WHERE slug = $1"
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Tenant::from))
    }

    async fn save(&self, tenant: &Tenant) -> anyhow::Result<()> {
        let tier = format!("{:?}", tenant.subscription_tier).to_lowercase();
        sqlx::query(
            r#"INSERT INTO identity.tenants (id, name, slug, subscription_tier, is_active, owner_email, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (id) DO UPDATE SET
                   name              = EXCLUDED.name,
                   slug              = EXCLUDED.slug,
                   subscription_tier = EXCLUDED.subscription_tier,
                   is_active         = EXCLUDED.is_active,
                   updated_at        = EXCLUDED.updated_at"#
        )
        .bind(tenant.id.inner())
        .bind(&tenant.name)
        .bind(&tenant.slug)
        .bind(tier)
        .bind(tenant.is_active)
        .bind(&tenant.owner_email)
        .bind(tenant.created_at)
        .bind(tenant.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn slug_exists(&self, slug: &str) -> anyhow::Result<bool> {
        let row = sqlx::query("SELECT 1 AS exists FROM identity.tenants WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }
}
