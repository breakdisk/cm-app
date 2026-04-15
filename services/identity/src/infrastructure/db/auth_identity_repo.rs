use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::UserId;
use uuid::Uuid;

use crate::domain::{
    entities::{AuthIdentity, AuthProvider},
    repositories::AuthIdentityRepository,
};

pub struct PgAuthIdentityRepository {
    pool: PgPool,
}

impl PgAuthIdentityRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct AuthIdentityRow {
    id:               Uuid,
    user_id:          Uuid,
    provider:         String,
    provider_subject: String,
    email_at_link:    String,
    linked_at:        chrono::DateTime<chrono::Utc>,
}

impl TryFrom<AuthIdentityRow> for AuthIdentity {
    type Error = anyhow::Error;

    fn try_from(r: AuthIdentityRow) -> anyhow::Result<Self> {
        let provider = AuthProvider::parse(&r.provider)
            .ok_or_else(|| anyhow::anyhow!("unknown auth provider in DB: {}", r.provider))?;
        Ok(AuthIdentity {
            id:               r.id,
            user_id:          UserId::from_uuid(r.user_id),
            provider,
            provider_subject: r.provider_subject,
            email_at_link:    r.email_at_link,
            linked_at:        r.linked_at,
        })
    }
}

#[async_trait]
impl AuthIdentityRepository for PgAuthIdentityRepository {
    async fn find_by_provider_subject(
        &self,
        provider: AuthProvider,
        subject: &str,
    ) -> anyhow::Result<Option<AuthIdentity>> {
        let row = sqlx::query_as::<_, AuthIdentityRow>(
            r#"SELECT id, user_id, provider, provider_subject, email_at_link, linked_at
               FROM identity.auth_identities
               WHERE provider = $1 AND provider_subject = $2"#,
        )
        .bind(provider.as_str())
        .bind(subject)
        .fetch_optional(&self.pool)
        .await?;

        row.map(AuthIdentity::try_from).transpose()
    }

    async fn list_for_user(&self, user_id: &UserId) -> anyhow::Result<Vec<AuthIdentity>> {
        let rows = sqlx::query_as::<_, AuthIdentityRow>(
            r#"SELECT id, user_id, provider, provider_subject, email_at_link, linked_at
               FROM identity.auth_identities
               WHERE user_id = $1
               ORDER BY linked_at ASC"#,
        )
        .bind(user_id.inner())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(AuthIdentity::try_from).collect()
    }

    async fn insert(&self, identity: &AuthIdentity) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO identity.auth_identities
                   (id, user_id, provider, provider_subject, email_at_link, linked_at)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(identity.id)
        .bind(identity.user_id.inner())
        .bind(identity.provider.as_str())
        .bind(&identity.provider_subject)
        .bind(&identity.email_at_link)
        .bind(identity.linked_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
