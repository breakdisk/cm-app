use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct DriverProfileRow {
    pub id:         Uuid,
    pub tenant_id:  Uuid,
    pub email:      String,
    pub first_name: String,
    pub last_name:  String,
}

#[async_trait]
pub trait DriverProfilesRepository: Send + Sync {
    async fn upsert(&self, row: &DriverProfileRow) -> anyhow::Result<()>;
    async fn list_by_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Vec<DriverProfileRow>>;
}

pub struct PgDriverProfilesRepository {
    pool: PgPool,
}

impl PgDriverProfilesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DriverProfilesRepository for PgDriverProfilesRepository {
    async fn upsert(&self, row: &DriverProfileRow) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO dispatch.driver_profiles (id, tenant_id, email, first_name, last_name)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (id) DO NOTHING"#,
        )
        .bind(row.id)
        .bind(row.tenant_id)
        .bind(&row.email)
        .bind(&row.first_name)
        .bind(&row.last_name)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_by_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Vec<DriverProfileRow>> {
        let rows = sqlx::query_as::<_, DriverProfileRow>(
            "SELECT id, tenant_id, email, first_name, last_name FROM dispatch.driver_profiles
             WHERE tenant_id = $1 AND is_active = true",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
