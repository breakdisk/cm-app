use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{TenantId, UserId};
use crate::domain::{entities::User, repositories::UserRepository};

pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id:             uuid::Uuid,
    tenant_id:      uuid::Uuid,
    email:          String,
    password_hash:  String,
    first_name:     String,
    last_name:      String,
    roles:          Vec<String>,
    is_active:      bool,
    email_verified: bool,
    last_login_at:  Option<chrono::DateTime<chrono::Utc>>,
    created_at:     chrono::DateTime<chrono::Utc>,
    updated_at:     chrono::DateTime<chrono::Utc>,
}

impl From<UserRow> for User {
    fn from(r: UserRow) -> Self {
        User {
            id:             UserId::from_uuid(r.id),
            tenant_id:      TenantId::from_uuid(r.tenant_id),
            email:          r.email,
            password_hash:  r.password_hash,
            first_name:     r.first_name,
            last_name:      r.last_name,
            roles:          r.roles,
            is_active:      r.is_active,
            email_verified: r.email_verified,
            last_login_at:  r.last_login_at,
            created_at:     r.created_at,
            updated_at:     r.updated_at,
        }
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn find_by_id(&self, id: &UserId) -> anyhow::Result<Option<User>> {
        let row = sqlx::query_as!(
            UserRow,
            r#"SELECT id, tenant_id, email, password_hash, first_name, last_name,
                      roles, is_active, email_verified, last_login_at, created_at, updated_at
               FROM identity.users WHERE id = $1"#,
            id.inner()
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(User::from))
    }

    async fn find_by_email(&self, tenant_id: &TenantId, email: &str) -> anyhow::Result<Option<User>> {
        let row = sqlx::query_as!(
            UserRow,
            r#"SELECT id, tenant_id, email, password_hash, first_name, last_name,
                      roles, is_active, email_verified, last_login_at, created_at, updated_at
               FROM identity.users WHERE tenant_id = $1 AND email = $2"#,
            tenant_id.inner(),
            email,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(User::from))
    }

    async fn save(&self, user: &User) -> anyhow::Result<()> {
        sqlx::query!(
            r#"INSERT INTO identity.users
                   (id, tenant_id, email, password_hash, first_name, last_name,
                    roles, is_active, email_verified, last_login_at, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
               ON CONFLICT (id) DO UPDATE SET
                   email          = EXCLUDED.email,
                   password_hash  = EXCLUDED.password_hash,
                   first_name     = EXCLUDED.first_name,
                   last_name      = EXCLUDED.last_name,
                   roles          = EXCLUDED.roles,
                   is_active      = EXCLUDED.is_active,
                   email_verified = EXCLUDED.email_verified,
                   last_login_at  = EXCLUDED.last_login_at,
                   updated_at     = EXCLUDED.updated_at"#,
            user.id.inner(),
            user.tenant_id.inner(),
            user.email,
            user.password_hash,
            user.first_name,
            user.last_name,
            &user.roles,
            user.is_active,
            user.email_verified,
            user.last_login_at,
            user.created_at,
            user.updated_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<User>> {
        let rows = sqlx::query_as!(
            UserRow,
            r#"SELECT id, tenant_id, email, password_hash, first_name, last_name,
                      roles, is_active, email_verified, last_login_at, created_at, updated_at
               FROM identity.users WHERE tenant_id = $1 ORDER BY created_at ASC"#,
            tenant_id.inner()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(User::from).collect())
    }
}
