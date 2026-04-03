use async_trait::async_trait;
use sqlx::{PgPool, Row};
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
        let row = sqlx::query_as::<_, UserRow>(
            r#"SELECT id, tenant_id, email, password_hash, first_name, last_name,
                      roles, is_active, email_verified, last_login_at, created_at, updated_at
               FROM identity.users WHERE id = $1"#
        )
        .bind(id.inner())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(User::from))
    }

    async fn find_by_email(&self, tenant_id: &TenantId, email: &str) -> anyhow::Result<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"SELECT id, tenant_id, email, password_hash, first_name, last_name,
                      roles, is_active, email_verified, last_login_at, created_at, updated_at
               FROM identity.users WHERE tenant_id = $1 AND email = $2"#
        )
        .bind(tenant_id.inner())
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(User::from))
    }

    async fn save(&self, user: &User) -> anyhow::Result<()> {
        sqlx::query(
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
                   updated_at     = EXCLUDED.updated_at"#
        )
        .bind(user.id.inner())
        .bind(user.tenant_id.inner())
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(&user.first_name)
        .bind(&user.last_name)
        .bind(&user.roles)
        .bind(user.is_active)
        .bind(user.email_verified)
        .bind(user.last_login_at)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<User>> {
        let rows = sqlx::query_as::<_, UserRow>(
            r#"SELECT id, tenant_id, email, password_hash, first_name, last_name,
                      roles, is_active, email_verified, last_login_at, created_at, updated_at
               FROM identity.users WHERE tenant_id = $1 ORDER BY created_at ASC"#
        )
        .bind(tenant_id.inner())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(User::from).collect())
    }
}

// ── Password reset token repository ──────────────────────────────────────────

pub struct PgPasswordResetTokenRepository {
    pool: PgPool,
}

impl PgPasswordResetTokenRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn create_reset_token(&self, user_id: uuid::Uuid, tenant_id: uuid::Uuid, token_hash: &str) -> anyhow::Result<()> {
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
        sqlx::query(
            r#"INSERT INTO identity.password_reset_tokens (user_id, tenant_id, token_hash, expires_at)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (token_hash) DO NOTHING"#
        )
        .bind(user_id).bind(tenant_id).bind(token_hash).bind(expires_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_valid_by_token(&self, token_hash: &str) -> anyhow::Result<Option<(uuid::Uuid, uuid::Uuid)>> {
        let row = sqlx::query(
            r#"SELECT user_id, tenant_id FROM identity.password_reset_tokens
               WHERE token_hash = $1 AND used = false AND expires_at > NOW()"#
        )
        .bind(token_hash).fetch_optional(&self.pool).await?;
        Ok(row.map(|r| (r.get::<uuid::Uuid, _>("user_id"), r.get::<uuid::Uuid, _>("tenant_id"))))
    }

    pub async fn mark_used(&self, token_hash: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE identity.password_reset_tokens SET used = true WHERE token_hash = $1")
            .bind(token_hash).execute(&self.pool).await?;
        Ok(())
    }
}

// ── Email verification token repository ──────────────────────────────────────

pub struct PgEmailVerificationTokenRepository {
    pool: PgPool,
}

impl PgEmailVerificationTokenRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn create(&self, user_id: uuid::Uuid, tenant_id: uuid::Uuid, token_hash: &str) -> anyhow::Result<()> {
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);
        sqlx::query(
            r#"INSERT INTO identity.email_verification_tokens (user_id, tenant_id, token_hash, expires_at)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (token_hash) DO NOTHING"#,
        )
        .bind(user_id).bind(tenant_id).bind(token_hash).bind(expires_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_valid(&self, token_hash: &str) -> anyhow::Result<Option<(uuid::Uuid, uuid::Uuid)>> {
        let row = sqlx::query(
            r#"SELECT user_id, tenant_id FROM identity.email_verification_tokens
               WHERE token_hash = $1 AND used = false AND expires_at > NOW()"#,
        )
        .bind(token_hash).fetch_optional(&self.pool).await?;
        Ok(row.map(|r| (r.get::<uuid::Uuid, _>("user_id"), r.get::<uuid::Uuid, _>("tenant_id"))))
    }

    pub async fn mark_used(&self, token_hash: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE identity.email_verification_tokens SET used = true WHERE token_hash = $1")
            .bind(token_hash).execute(&self.pool).await?;
        Ok(())
    }
}
