use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::{
    entities::{NewPartnerMembership, PartnerMembership},
    repositories::{PartnerMembershipRepository, UpsertOutcome},
};

/// The partial unique index that allows one active membership per user.
const ONE_ACTIVE_INDEX: &str = "uq_one_active_partner_membership";

pub struct PgPartnerMembershipRepository {
    pool: PgPool,
}

impl PgPartnerMembershipRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct Row {
    id:         Uuid,
    tenant_id:  Uuid,
    user_id:    Uuid,
    carrier_id: Uuid,
    role:       String,
    status:     String,
    created_at: DateTime<Utc>,
}

impl From<Row> for PartnerMembership {
    fn from(r: Row) -> Self {
        PartnerMembership {
            id: r.id, tenant_id: r.tenant_id, user_id: r.user_id,
            carrier_id: r.carrier_id, role: r.role, status: r.status, created_at: r.created_at,
        }
    }
}

const COLUMNS: &str = "id, tenant_id, user_id, carrier_id, role, status, created_at";

#[async_trait]
impl PartnerMembershipRepository for PgPartnerMembershipRepository {
    async fn find_active(&self, tenant_id: Uuid, user_id: Uuid) -> anyhow::Result<Option<PartnerMembership>> {
        let row = sqlx::query_as::<_, Row>(&format!(
            "SELECT {COLUMNS} FROM identity.partner_memberships \
             WHERE tenant_id = $1 AND user_id = $2 AND status = 'active'"
        ))
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn upsert_active(&self, new: &NewPartnerMembership) -> anyhow::Result<UpsertOutcome> {
        let inserted = sqlx::query(
            "INSERT INTO identity.partner_memberships (tenant_id, user_id, carrier_id, role, status) \
             VALUES ($1, $2, $3, $4, 'active') \
             ON CONFLICT (tenant_id, user_id, carrier_id) DO NOTHING",
        )
        .bind(new.tenant_id)
        .bind(new.user_id)
        .bind(new.carrier_id)
        .bind(&new.role)
        .execute(&self.pool)
        .await;

        if let Err(sqlx::Error::Database(e)) = &inserted {
            if e.constraint() == Some(ONE_ACTIVE_INDEX) {
                // Another carrier already holds this user. Report which one.
                let existing = self.find_active(new.tenant_id, new.user_id).await?;
                return match existing {
                    Some(m) => Ok(UpsertOutcome::ConflictsWith(m.carrier_id)),
                    // The holder was suspended between the insert and this read:
                    // surface it as an error so the caller retries, not as success.
                    None => Err(anyhow::anyhow!("active membership vanished during upsert; retry")),
                };
            }
        }
        inserted?;

        let row = sqlx::query_as::<_, Row>(&format!(
            "SELECT {COLUMNS} FROM identity.partner_memberships \
             WHERE tenant_id = $1 AND user_id = $2 AND carrier_id = $3"
        ))
        .bind(new.tenant_id)
        .bind(new.user_id)
        .bind(new.carrier_id)
        .fetch_one(&self.pool)
        .await?;

        let membership: PartnerMembership = row.into();
        Ok(if membership.status == "active" {
            UpsertOutcome::Active(membership)
        } else {
            UpsertOutcome::Suspended(membership)
        })
    }
}
