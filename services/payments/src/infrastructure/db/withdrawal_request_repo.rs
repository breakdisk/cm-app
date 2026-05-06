use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::entities::{WithdrawalRequest, WithdrawalStatus};

pub struct PgWithdrawalRequestRepository { pool: PgPool }
impl PgWithdrawalRequestRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct RequestRow {
    id:              Uuid,
    tenant_id:       Uuid,
    wallet_id:       Uuid,
    amount_centavos: i64,
    currency:        String,
    status:          String,
    requested_by:    Uuid,
    reviewed_by:     Option<Uuid>,
    review_note:     Option<String>,
    reviewed_at:     Option<chrono::DateTime<chrono::Utc>>,
    created_at:      chrono::DateTime<chrono::Utc>,
    updated_at:      chrono::DateTime<chrono::Utc>,
}

fn parse_status(s: &str) -> anyhow::Result<WithdrawalStatus> {
    match s {
        "pending"   => Ok(WithdrawalStatus::Pending),
        "approved"  => Ok(WithdrawalStatus::Approved),
        "disbursed" => Ok(WithdrawalStatus::Disbursed),
        "rejected"  => Ok(WithdrawalStatus::Rejected),
        other       => Err(anyhow::anyhow!("Unknown withdrawal status: {other}")),
    }
}

fn try_from_row(r: RequestRow) -> anyhow::Result<WithdrawalRequest> {
    Ok(WithdrawalRequest {
        id: r.id, tenant_id: r.tenant_id, wallet_id: r.wallet_id,
        amount_centavos: r.amount_centavos, currency: r.currency,
        status: parse_status(&r.status)?,
        requested_by: r.requested_by,
        reviewed_by: r.reviewed_by, review_note: r.review_note,
        reviewed_at: r.reviewed_at, created_at: r.created_at, updated_at: r.updated_at,
    })
}

fn status_str(s: WithdrawalStatus) -> &'static str {
    match s {
        WithdrawalStatus::Pending   => "pending",
        WithdrawalStatus::Approved  => "approved",
        WithdrawalStatus::Disbursed => "disbursed",
        WithdrawalStatus::Rejected  => "rejected",
    }
}


const SELECT: &str = "SELECT id, tenant_id, wallet_id, amount_centavos, currency, status,
    requested_by, reviewed_by, review_note, reviewed_at, created_at, updated_at
    FROM payments.withdrawal_requests";

impl PgWithdrawalRequestRepository {
    pub async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<WithdrawalRequest>> {
        let row = sqlx::query_as::<_, RequestRow>(&format!("{SELECT} WHERE id = $1"))
            .bind(id).fetch_optional(&self.pool).await?;
        row.map(try_from_row).transpose()
    }

    pub async fn list_by_status(&self, tenant_id: Uuid, status: WithdrawalStatus) -> anyhow::Result<Vec<WithdrawalRequest>> {
        let rows = sqlx::query_as::<_, RequestRow>(
            &format!("{SELECT} WHERE tenant_id = $1 AND status = $2 ORDER BY created_at DESC")
        ).bind(tenant_id).bind(status_str(status)).fetch_all(&self.pool).await?;
        rows.into_iter().map(try_from_row).collect()
    }

    pub async fn insert(&self, r: &WithdrawalRequest) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO payments.withdrawal_requests
             (id, tenant_id, wallet_id, amount_centavos, currency, status,
              requested_by, reviewed_by, review_note, reviewed_at, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"
        )
        .bind(r.id).bind(r.tenant_id).bind(r.wallet_id).bind(r.amount_centavos)
        .bind(&r.currency).bind(status_str(r.status)).bind(r.requested_by)
        .bind(r.reviewed_by).bind(r.review_note.as_deref())
        .bind(r.reviewed_at).bind(r.created_at).bind(r.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn update(&self, r: &WithdrawalRequest) -> anyhow::Result<()> {
        let result = sqlx::query(
            "UPDATE payments.withdrawal_requests SET
             status = $2, reviewed_by = $3, review_note = $4, reviewed_at = $5, updated_at = $6
             WHERE id = $1 AND tenant_id = $7"
        )
        .bind(r.id).bind(status_str(r.status)).bind(r.reviewed_by)
        .bind(r.review_note.as_deref()).bind(r.reviewed_at).bind(r.updated_at)
        .bind(r.tenant_id)
        .execute(&self.pool).await?;

        if result.rows_affected() == 0 {
            anyhow::bail!("WithdrawalRequest update affected 0 rows — id={}", r.id);
        }
        Ok(())
    }
}
