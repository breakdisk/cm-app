use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::TenantId;
use crate::domain::{entities::Branding, repositories::BrandingRepository};

pub struct PgBrandingRepository {
    pool: PgPool,
}

impl PgBrandingRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

/// Database row shape — maps directly from sqlx query.
#[derive(sqlx::FromRow)]
struct BrandingRow {
    tenant_id:       uuid::Uuid,
    display_name:    String,
    app_tagline:     Option<String>,
    logo_url:        Option<String>,
    logo_dark_url:   Option<String>,
    favicon_url:     Option<String>,
    primary_color:   Option<String>,
    secondary_color: Option<String>,
    accent_color:    Option<String>,
    support_email:   Option<String>,
    support_phone:   Option<String>,
    legal_text:      serde_json::Value,
    custom_domain:   Option<String>,
    created_at:      chrono::DateTime<chrono::Utc>,
    updated_at:      chrono::DateTime<chrono::Utc>,
}

impl From<BrandingRow> for Branding {
    fn from(r: BrandingRow) -> Self {
        Branding {
            tenant_id:       TenantId::from_uuid(r.tenant_id),
            display_name:    r.display_name,
            app_tagline:     r.app_tagline,
            logo_url:        r.logo_url,
            logo_dark_url:   r.logo_dark_url,
            favicon_url:     r.favicon_url,
            primary_color:   r.primary_color,
            secondary_color: r.secondary_color,
            accent_color:    r.accent_color,
            support_email:   r.support_email,
            support_phone:   r.support_phone,
            legal_text:      r.legal_text,
            custom_domain:   r.custom_domain,
            created_at:      r.created_at,
            updated_at:      r.updated_at,
        }
    }
}

const SELECT_COLS: &str = "tenant_id, display_name, app_tagline, logo_url, logo_dark_url, \
    favicon_url, primary_color, secondary_color, accent_color, support_email, support_phone, \
    legal_text, custom_domain, created_at, updated_at";

#[async_trait]
impl BrandingRepository for PgBrandingRepository {
    async fn find_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Option<Branding>> {
        let sql = format!(
            "SELECT {SELECT_COLS} FROM identity.tenant_branding WHERE tenant_id = $1"
        );
        let row = sqlx::query_as::<_, BrandingRow>(&sql)
            .bind(tenant_id.inner())
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Branding::from))
    }

    async fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Branding>> {
        let sql = format!(
            "SELECT {} FROM identity.tenant_branding b \
             JOIN identity.tenants t ON t.id = b.tenant_id \
             WHERE t.slug = $1",
            SELECT_COLS.replace("tenant_id", "b.tenant_id")
        );
        let row = sqlx::query_as::<_, BrandingRow>(&sql)
            .bind(slug)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Branding::from))
    }

    async fn find_by_custom_domain(&self, host: &str) -> anyhow::Result<Option<Branding>> {
        let sql = format!(
            "SELECT {SELECT_COLS} FROM identity.tenant_branding WHERE custom_domain = $1"
        );
        let row = sqlx::query_as::<_, BrandingRow>(&sql)
            .bind(host)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Branding::from))
    }

    async fn upsert(&self, b: &Branding) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO identity.tenant_branding
                   (tenant_id, display_name, app_tagline, logo_url, logo_dark_url,
                    favicon_url, primary_color, secondary_color, accent_color,
                    support_email, support_phone, legal_text, custom_domain, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
               ON CONFLICT (tenant_id) DO UPDATE SET
                   display_name    = EXCLUDED.display_name,
                   app_tagline     = EXCLUDED.app_tagline,
                   logo_url        = EXCLUDED.logo_url,
                   logo_dark_url   = EXCLUDED.logo_dark_url,
                   favicon_url     = EXCLUDED.favicon_url,
                   primary_color   = EXCLUDED.primary_color,
                   secondary_color = EXCLUDED.secondary_color,
                   accent_color    = EXCLUDED.accent_color,
                   support_email   = EXCLUDED.support_email,
                   support_phone   = EXCLUDED.support_phone,
                   legal_text      = EXCLUDED.legal_text,
                   custom_domain   = EXCLUDED.custom_domain,
                   updated_at      = EXCLUDED.updated_at"#
        )
        .bind(b.tenant_id.inner())
        .bind(&b.display_name)
        .bind(&b.app_tagline)
        .bind(&b.logo_url)
        .bind(&b.logo_dark_url)
        .bind(&b.favicon_url)
        .bind(&b.primary_color)
        .bind(&b.secondary_color)
        .bind(&b.accent_color)
        .bind(&b.support_email)
        .bind(&b.support_phone)
        .bind(&b.legal_text)
        .bind(&b.custom_domain)
        .bind(b.created_at)
        .bind(b.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
