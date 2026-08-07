use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{AssignmentStatus, CourierAssignment, ProductKey};

/// Outcome of a claim attempt. `Lost` is an ordinary outcome, not an error —
/// two products racing is expected, and the loser needs to try another courier.
#[derive(Debug, PartialEq, Eq)]
pub enum ClaimOutcome {
    Won,
    Lost,
}

#[async_trait]
pub trait AssignmentRepository: Send + Sync {
    async fn save(&self, a: &CourierAssignment) -> anyhow::Result<()>;

    /// Atomically claim an offered assignment. Returns `Lost` when another
    /// assignment already holds this courier.
    async fn try_claim(&self, tenant_id: Uuid, assignment_id: Uuid) -> anyhow::Result<ClaimOutcome>;

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<CourierAssignment>>;
}

pub struct PgAssignmentRepository {
    pool: PgPool,
}

impl PgAssignmentRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl AssignmentRepository for PgAssignmentRepository {
    async fn save(&self, a: &CourierAssignment) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO field_ops.courier_assignments (
                id, tenant_id, courier_id, product, external_ref, status,
                offered_at, claimed_at, completed_at, heartbeat_at, created_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
            ON CONFLICT (id) DO UPDATE SET
                status       = EXCLUDED.status,
                claimed_at   = EXCLUDED.claimed_at,
                completed_at = EXCLUDED.completed_at,
                heartbeat_at = EXCLUDED.heartbeat_at
            "#,
        )
        .bind(a.id).bind(a.tenant_id).bind(a.courier_id)
        .bind(a.product.as_str()).bind(a.external_ref)
        .bind(a.status.as_str())
        .bind(a.offered_at).bind(a.claimed_at).bind(a.completed_at)
        .bind(a.heartbeat_at).bind(a.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<CourierAssignment>> {
        let Some(r) = sqlx::query(
            "SELECT * FROM field_ops.courier_assignments WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id).bind(id)
        .fetch_optional(&self.pool).await?
        else {
            return Ok(None);
        };

        let status: String = r.get("status");
        let product: String = r.get("product");

        Ok(Some(CourierAssignment {
            id:           r.get("id"),
            tenant_id:    r.get("tenant_id"),
            courier_id:   r.get("courier_id"),
            product:      ProductKey::new(product),
            external_ref: r.get("external_ref"),
            status: match status.as_str() {
                "offered"   => AssignmentStatus::Offered,
                "claimed"   => AssignmentStatus::Claimed,
                "completed" => AssignmentStatus::Completed,
                "released"  => AssignmentStatus::Released,
                "expired"   => AssignmentStatus::Expired,
                other => anyhow::bail!("unknown assignment status in database: {other}"),
            },
            offered_at:   r.get("offered_at"),
            claimed_at:   r.get("claimed_at"),
            completed_at: r.get("completed_at"),
            heartbeat_at: r.get("heartbeat_at"),
            created_at:   r.get("created_at"),
        }))
    }

    async fn try_claim(&self, tenant_id: Uuid, assignment_id: Uuid) -> anyhow::Result<ClaimOutcome> {
        // Compare-and-swap: the UPDATE only fires while the row is still
        // `offered`, and the partial unique index on (courier_id) WHERE
        // status='claimed' rejects it if another assignment already holds this
        // courier. Two racing products therefore produce exactly one winner —
        // one gets a row back, the other gets either zero rows (lost the CAS)
        // or a unique violation (lost the index race).
        let result = sqlx::query(
            r#"
            UPDATE field_ops.courier_assignments
               SET status = 'claimed', claimed_at = NOW(), heartbeat_at = NOW()
             WHERE id = $1 AND tenant_id = $2 AND status = 'offered'
            RETURNING id
            "#,
        )
        .bind(assignment_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await;

        match result {
            Ok(Some(_)) => Ok(ClaimOutcome::Won),
            Ok(None) => Ok(ClaimOutcome::Lost),
            // The unique index fired: another assignment claimed this courier
            // between our status check and our write. That is a lost race, not
            // a failure — surface it as such rather than a 500.
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(ClaimOutcome::Lost),
            Err(e) => Err(e.into()),
        }
    }
}
