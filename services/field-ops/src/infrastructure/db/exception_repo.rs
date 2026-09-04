use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{AssignmentException, ExceptionReason};

#[async_trait::async_trait]
pub trait ExceptionRepository: Send + Sync {
    /// Returns false when this `(assignment_id, client_ref)` was already
    /// recorded — the offline queue replaying a tap, not a second failure.
    async fn record(&self, e: &AssignmentException) -> anyhow::Result<bool>;

    /// Open exceptions for a tenant, oldest first.
    async fn list_open(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<AssignmentException>>;
}

pub struct PgExceptionRepository {
    pool: PgPool,
}

impl PgExceptionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ExceptionRepository for PgExceptionRepository {
    async fn record(&self, e: &AssignmentException) -> anyhow::Result<bool> {
        // ON CONFLICT DO NOTHING against the (assignment_id, client_ref) index.
        // The app replays queued writes, so a duplicate is the expected case
        // and not an error: it returns false and the caller stays quiet.
        let done = sqlx::query(
            "INSERT INTO field_ops.assignment_exceptions
                 (id, tenant_id, assignment_id, courier_id, reason, note,
                  goods_disposition, capture_lat, capture_lng, client_ref,
                  device_timestamp, server_timestamp)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (assignment_id, client_ref) DO NOTHING",
        )
        .bind(e.id)
        .bind(e.tenant_id)
        .bind(e.assignment_id)
        .bind(e.courier_id)
        .bind(e.reason.as_str())
        .bind(e.note.as_deref())
        .bind(e.goods_disposition.as_deref())
        .bind(e.capture_lat)
        .bind(e.capture_lng)
        .bind(e.client_ref)
        .bind(e.device_timestamp)
        .bind(e.server_timestamp)
        .execute(&self.pool)
        .await?;

        Ok(done.rows_affected() == 1)
    }

    async fn list_open(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<AssignmentException>> {
        let rows = sqlx::query(
            "SELECT id, tenant_id, assignment_id, courier_id, reason, note,
                    goods_disposition, capture_lat, capture_lng, client_ref,
                    device_timestamp, server_timestamp,
                    resolved_at, resolved_by, resolution
               FROM field_ops.assignment_exceptions
              WHERE tenant_id = $1 AND resolved_at IS NULL
              ORDER BY server_timestamp ASC
              LIMIT $2",
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| {
                let reason: String = r.try_get("reason")?;
                Ok(AssignmentException {
                    id: r.try_get("id")?,
                    tenant_id: r.try_get("tenant_id")?,
                    assignment_id: r.try_get("assignment_id")?,
                    courier_id: r.try_get("courier_id")?,
                    // A row written by an older deploy can hold a reason this
                    // build no longer knows. Failing the whole ops queue over
                    // one unrecognised string is the wrong trade, so it lands
                    // as CourierBlocked — the value that means "a human has to
                    // look" — rather than dropping the row silently.
                    reason: ExceptionReason::parse(&reason)
                        .unwrap_or(ExceptionReason::CourierBlocked),
                    note: r.try_get("note")?,
                    goods_disposition: r.try_get("goods_disposition")?,
                    capture_lat: r.try_get("capture_lat")?,
                    capture_lng: r.try_get("capture_lng")?,
                    client_ref: r.try_get("client_ref")?,
                    device_timestamp: r.try_get("device_timestamp")?,
                    server_timestamp: r.try_get("server_timestamp")?,
                    resolved_at: r.try_get("resolved_at")?,
                    resolved_by: r.try_get("resolved_by")?,
                    resolution: r.try_get("resolution")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(Into::into)
    }
}
