use async_trait::async_trait;
use sqlx::PgPool;
use crate::domain::{entities::PricingFeature, repositories::PricingFeatureRepository};

pub struct PgPricingFeatureRepository {
    pool: PgPool,
}

impl PgPricingFeatureRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct FeatureRow {
    feature_key:      String,
    feature_name:     String,
    feature_category: String,
    description:      Option<String>,
    enabled_tiers:    Vec<String>,
    is_system:        bool,
    sort_order:       i32,
    created_at:       chrono::DateTime<chrono::Utc>,
    updated_at:       chrono::DateTime<chrono::Utc>,
}

impl From<FeatureRow> for PricingFeature {
    fn from(r: FeatureRow) -> Self {
        PricingFeature {
            feature_key:      r.feature_key,
            feature_name:     r.feature_name,
            feature_category: r.feature_category,
            description:      r.description,
            enabled_tiers:    r.enabled_tiers,
            is_system:        r.is_system,
            sort_order:       r.sort_order,
            created_at:       r.created_at,
            updated_at:       r.updated_at,
        }
    }
}

#[async_trait]
impl PricingFeatureRepository for PgPricingFeatureRepository {
    async fn list_all(&self) -> anyhow::Result<Vec<PricingFeature>> {
        let rows = sqlx::query_as::<_, FeatureRow>(
            "SELECT feature_key, feature_name, feature_category, description, \
             enabled_tiers, is_system, sort_order, created_at, updated_at \
             FROM identity.pricing_features ORDER BY sort_order ASC"
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(PricingFeature::from).collect())
    }

    async fn list_for_tier(&self, tier: &str) -> anyhow::Result<Vec<PricingFeature>> {
        let rows = sqlx::query_as::<_, FeatureRow>(
            "SELECT feature_key, feature_name, feature_category, description, \
             enabled_tiers, is_system, sort_order, created_at, updated_at \
             FROM identity.pricing_features \
             WHERE $1 = ANY(enabled_tiers) \
             ORDER BY sort_order ASC"
        )
        .bind(tier)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(PricingFeature::from).collect())
    }

    async fn set_enabled_tiers(&self, feature_key: &str, tiers: &[String]) -> anyhow::Result<()> {
        let result = sqlx::query(
            "UPDATE identity.pricing_features \
             SET enabled_tiers = $1, updated_at = NOW() \
             WHERE feature_key = $2"
        )
        .bind(tiers)
        .bind(feature_key)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(anyhow::anyhow!("Feature '{}' not found", feature_key));
        }
        Ok(())
    }
}
