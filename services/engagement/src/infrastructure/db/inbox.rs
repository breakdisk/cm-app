//! Storage for the customer's campaign inbox. The rows are `campaign_sends`;
//! every read and write here is pinned to one tenant and one customer.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

pub struct InboxDb {
    pool: PgPool,
}

/// Whose inbox: the app user, plus the addresses their login proves they
/// hold. A campaign reaches a customer as a CDP profile, not as a user id,
/// so a send is theirs by either.
pub struct Reader {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    /// Normalised with `inbox::normalise_address`.
    pub addresses: Vec<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InboxItem {
    pub id: Uuid,
    pub campaign_id: Uuid,
    pub channel: String,
    pub title: String,
    pub body: String,
    pub deep_link: Option<String>,
    pub sent_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

impl InboxDb {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Newest first. `before` pages back from the last item already shown.
    pub async fn list(&self, r: &Reader, before: Option<DateTime<Utc>>, limit: i64) -> anyhow::Result<Vec<InboxItem>> {
        let rows = sqlx::query_as::<_, InboxItem>(
            r#"
            SELECT id, campaign_id, channel,
                   inbox_title AS title, COALESCE(inbox_body, '') AS body,
                   inbox_deep_link AS deep_link, queued_at AS sent_at,
                   inbox_read_at AS read_at
              FROM engagement.campaign_sends
             WHERE tenant_id = $1
               AND (customer_id = $2 OR inbox_address = ANY($5))
               AND inbox_title IS NOT NULL
               AND ($3::timestamptz IS NULL OR queued_at < $3)
             ORDER BY queued_at DESC
             LIMIT $4
            "#,
        )
        .bind(r.tenant_id)
        .bind(r.user_id)
        .bind(before)
        .bind(limit)
        .bind(&r.addresses)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn unread(&self, r: &Reader) -> anyhow::Result<i64> {
        let n: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM engagement.campaign_sends
             WHERE tenant_id = $1 AND (customer_id = $2 OR inbox_address = ANY($3))
               AND inbox_title IS NOT NULL AND inbox_read_at IS NULL
            "#,
        )
        .bind(r.tenant_id)
        .bind(r.user_id)
        .bind(&r.addresses)
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }

    /// False when there is no such message for this customer — someone
    /// else's id reads exactly like a missing one.
    pub async fn mark_read(&self, r: &Reader, id: Uuid, at: DateTime<Utc>) -> anyhow::Result<bool> {
        let done = sqlx::query(
            r#"
            UPDATE engagement.campaign_sends
               SET inbox_read_at = COALESCE(inbox_read_at, $4)
             WHERE id = $3 AND tenant_id = $1 AND (customer_id = $2 OR inbox_address = ANY($5))
               AND inbox_title IS NOT NULL
            "#,
        )
        .bind(r.tenant_id)
        .bind(r.user_id)
        .bind(id)
        .bind(at)
        .bind(&r.addresses)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn mark_all_read(&self, r: &Reader, at: DateTime<Utc>) -> anyhow::Result<u64> {
        let done = sqlx::query(
            r#"
            UPDATE engagement.campaign_sends
               SET inbox_read_at = $3
             WHERE tenant_id = $1 AND (customer_id = $2 OR inbox_address = ANY($4))
               AND inbox_title IS NOT NULL AND inbox_read_at IS NULL
            "#,
        )
        .bind(r.tenant_id)
        .bind(r.user_id)
        .bind(at)
        .bind(&r.addresses)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected())
    }
}
