//! Storage for the job chat thread. Every read and write is tenant-scoped;
//! nothing here decides who may take part (see `job_participants`).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entities::job_message::{JobMessage, SenderRole};

pub struct JobChatDb {
    pool: PgPool,
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: Uuid,
    shipment_id: Uuid,
    sender_id: Uuid,
    sender_role: String,
    body: String,
    created_at: DateTime<Utc>,
}

impl MessageRow {
    fn into_message(self) -> JobMessage {
        JobMessage {
            id: self.id,
            shipment_id: self.shipment_id,
            sender_id: self.sender_id,
            // The column allows only the two roles, so this is unreachable for
            // any row written through this service.
            sender_role: SenderRole::parse(&self.sender_role).unwrap_or(SenderRole::Customer),
            body: self.body,
            created_at: self.created_at,
        }
    }
}

impl JobChatDb {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Stores one message. A repeated `client_message_id` returns the message
    /// already stored rather than a second copy — a phone that retries a send
    /// on a flaky connection must not post the same sentence twice. DO UPDATE
    /// (writing the body back to itself) rather than DO NOTHING, because
    /// DO NOTHING returns no row and the caller needs the message back.
    pub async fn insert(
        &self,
        tenant_id: Uuid,
        shipment_id: Uuid,
        sender_id: Uuid,
        sender_role: SenderRole,
        body: &str,
        client_message_id: Option<Uuid>,
    ) -> anyhow::Result<JobMessage> {
        let row = sqlx::query_as::<_, MessageRow>(
            r#"
            INSERT INTO engagement.job_messages
                (tenant_id, shipment_id, sender_id, sender_role, body, client_message_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (sender_id, client_message_id)
            DO UPDATE SET body = engagement.job_messages.body
            RETURNING id, shipment_id, sender_id, sender_role, body, created_at
            "#,
        )
        .bind(tenant_id)
        .bind(shipment_id)
        .bind(sender_id)
        .bind(sender_role.as_str())
        .bind(body)
        .bind(client_message_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into_message())
    }

    /// The thread, oldest first. `since` is what the open chat polls with.
    pub async fn list(
        &self,
        tenant_id: Uuid,
        shipment_id: Uuid,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> anyhow::Result<Vec<JobMessage>> {
        let rows = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT id, shipment_id, sender_id, sender_role, body, created_at
              FROM engagement.job_messages
             WHERE tenant_id = $1
               AND shipment_id = $2
               AND ($3::timestamptz IS NULL OR created_at > $3)
             ORDER BY created_at
             LIMIT $4
            "#,
        )
        .bind(tenant_id)
        .bind(shipment_id)
        .bind(since)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(MessageRow::into_message).collect())
    }

    /// Moves this reader's marker forward. GREATEST keeps it monotonic, so a
    /// slow request that finishes late cannot drag it backwards and resurrect
    /// messages the reader has already seen.
    pub async fn mark_read(
        &self,
        tenant_id: Uuid,
        shipment_id: Uuid,
        reader_id: Uuid,
        at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO engagement.job_message_reads (tenant_id, shipment_id, reader_id, last_read_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (tenant_id, shipment_id, reader_id)
            DO UPDATE SET last_read_at = GREATEST(engagement.job_message_reads.last_read_at, EXCLUDED.last_read_at)
            "#,
        )
        .bind(tenant_id)
        .bind(shipment_id)
        .bind(reader_id)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// What the other person has sent since this reader last read.
    pub async fn unread(&self, tenant_id: Uuid, shipment_id: Uuid, reader_id: Uuid) -> anyhow::Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT count(*)
              FROM engagement.job_messages m
              LEFT JOIN engagement.job_message_reads r
                     ON r.tenant_id = m.tenant_id
                    AND r.shipment_id = m.shipment_id
                    AND r.reader_id = $3
             WHERE m.tenant_id = $1
               AND m.shipment_id = $2
               AND m.sender_id <> $3
               AND (r.last_read_at IS NULL OR m.created_at > r.last_read_at)
            "#,
        )
        .bind(tenant_id)
        .bind(shipment_id)
        .bind(reader_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }
}
