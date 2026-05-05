use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{TenantId, UserId};
use uuid::Uuid;

use crate::domain::{entities::PickupAddress, repositories::PickupAddressRepository};

pub struct PgPickupAddressRepository {
    pool: PgPool,
}

impl PgPickupAddressRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct PickupAddressRow {
    id:          Uuid,
    user_id:     Uuid,
    tenant_id:   Uuid,
    label:       String,
    address:     String,
    city:        String,
    province:    String,
    postal_code: String,
    country:     String,
    lat:         Option<f64>,
    lng:         Option<f64>,
    is_default:  bool,
    created_at:  chrono::DateTime<chrono::Utc>,
    updated_at:  chrono::DateTime<chrono::Utc>,
}

impl From<PickupAddressRow> for PickupAddress {
    fn from(r: PickupAddressRow) -> Self {
        Self {
            id:          r.id,
            user_id:     UserId::from_uuid(r.user_id),
            tenant_id:   TenantId::from_uuid(r.tenant_id),
            label:       r.label,
            address:     r.address,
            city:        r.city,
            province:    r.province,
            postal_code: r.postal_code,
            country:     r.country,
            lat:         r.lat,
            lng:         r.lng,
            is_default:  r.is_default,
            created_at:  r.created_at,
            updated_at:  r.updated_at,
        }
    }
}

#[async_trait]
impl PickupAddressRepository for PgPickupAddressRepository {
    async fn list_by_user(&self, user_id: &UserId) -> anyhow::Result<Vec<PickupAddress>> {
        let rows = sqlx::query_as::<_, PickupAddressRow>(
            r#"SELECT id, user_id, tenant_id, label, address, city, province,
                      postal_code, country, lat, lng, is_default, created_at, updated_at
               FROM identity.pickup_addresses
               WHERE user_id = $1
               ORDER BY is_default DESC, created_at ASC"#,
        )
        .bind(user_id.inner())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(PickupAddress::from).collect())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<PickupAddress>> {
        let row = sqlx::query_as::<_, PickupAddressRow>(
            r#"SELECT id, user_id, tenant_id, label, address, city, province,
                      postal_code, country, lat, lng, is_default, created_at, updated_at
               FROM identity.pickup_addresses WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(PickupAddress::from))
    }

    async fn insert(&self, addr: &PickupAddress) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO identity.pickup_addresses
                   (id, user_id, tenant_id, label, address, city, province,
                    postal_code, country, lat, lng, is_default, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)"#,
        )
        .bind(addr.id)
        .bind(addr.user_id.inner())
        .bind(addr.tenant_id.inner())
        .bind(&addr.label)
        .bind(&addr.address)
        .bind(&addr.city)
        .bind(&addr.province)
        .bind(&addr.postal_code)
        .bind(&addr.country)
        .bind(addr.lat)
        .bind(addr.lng)
        .bind(addr.is_default)
        .bind(addr.created_at)
        .bind(addr.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: Uuid, user_id: &UserId) -> anyhow::Result<()> {
        sqlx::query(
            "DELETE FROM identity.pickup_addresses WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user_id.inner())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_default(&self, id: Uuid, user_id: &UserId) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE identity.pickup_addresses SET is_default = false, updated_at = NOW() WHERE user_id = $1",
        )
        .bind(user_id.inner())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE identity.pickup_addresses SET is_default = true, updated_at = NOW() WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user_id.inner())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
