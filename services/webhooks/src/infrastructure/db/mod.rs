use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::{
    entities::{DeliveryAttempt, Webhook, WebhookStatus},
    repositories::{DeliveryRepository, WebhookRepository},
};

pub struct PgWebhookRepository { pool: PgPool }
impl PgWebhookRepository { pub fn new(pool: PgPool) -> Self { Self { pool } } }

fn map_webhook(row: &sqlx::postgres::PgRow) -> Webhook {
    Webhook {
        id:                row.get("id"),
        tenant_id:         row.get("tenant_id"),
        url:               row.get("url"),
        events:            row.get::<Vec<String>, _>("events"),
        secret:            row.get("secret"),
        status:            WebhookStatus::parse(row.get::<&str, _>("status")).unwrap_or(WebhookStatus::Disabled),
        description:       row.get("description"),
        success_count:     row.get("success_count"),
        fail_count:        row.get("fail_count"),
        last_delivery_at:  row.get("last_delivery_at"),
        last_status_code:  row.get("last_status_code"),
        created_at:        row.get("created_at"),
        updated_at:        row.get("updated_at"),
    }
}

#[async_trait]
impl WebhookRepository for PgWebhookRepository {
    async fn save(&self, w: &Webhook) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO webhooks.webhooks
                (id, tenant_id, url, events, secret, status, description,
                 success_count, fail_count, last_delivery_at, last_status_code,
                 created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (id) DO UPDATE SET
                url               = EXCLUDED.url,
                events            = EXCLUDED.events,
                status            = EXCLUDED.status,
                description       = EXCLUDED.description,
                updated_at        = EXCLUDED.updated_at
            "#,
        )
        .bind(w.id)
        .bind(w.tenant_id)
        .bind(&w.url)
        .bind(&w.events)
        .bind(&w.secret)
        .bind(w.status.as_str())
        .bind(&w.description)
        .bind(w.success_count)
        .bind(w.fail_count)
        .bind(w.last_delivery_at)
        .bind(w.last_status_code)
        .bind(w.created_at)
        .bind(w.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<Webhook>> {
        let row = sqlx::query(
            r#"SELECT id, tenant_id, url, events, secret, status, description,
                      success_count, fail_count, last_delivery_at, last_status_code,
                      created_at, updated_at
               FROM webhooks.webhooks WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| map_webhook(&r)))
    }

    async fn list_by_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Webhook>> {
        let rows = sqlx::query(
            r#"SELECT id, tenant_id, url, events, secret, status, description,
                      success_count, fail_count, last_delivery_at, last_status_code,
                      created_at, updated_at
               FROM webhooks.webhooks
               WHERE tenant_id = $1
               ORDER BY created_at DESC"#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(map_webhook).collect())
    }

    async fn delete(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM webhooks.webhooks WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn find_subscribers(
        &self,
        tenant_id: Uuid,
        event_type: &str,
    ) -> anyhow::Result<Vec<Webhook>> {
        // `events && ARRAY[event_type, '*']` uses the GIN index for both
        // exact-match and wildcard subscribers in a single scan.
        let rows = sqlx::query(
            r#"SELECT id, tenant_id, url, events, secret, status, description,
                      success_count, fail_count, last_delivery_at, last_status_code,
                      created_at, updated_at
               FROM webhooks.webhooks
               WHERE tenant_id = $1
                 AND status = 'active'
                 AND events && ARRAY[$2::text, '*'::text]"#,
        )
        .bind(tenant_id)
        .bind(event_type)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(map_webhook).collect())
    }

    async fn record_attempt(
        &self,
        webhook_id: Uuid,
        success: bool,
        status_code: i32,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"UPDATE webhooks.webhooks SET
                success_count    = success_count + CASE WHEN $2 THEN 1 ELSE 0 END,
                fail_count       = fail_count    + CASE WHEN $2 THEN 0 ELSE 1 END,
                last_delivery_at = NOW(),
                last_status_code = $3,
                updated_at       = NOW()
               WHERE id = $1"#,
        )
        .bind(webhook_id)
        .bind(success)
        .bind(status_code)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

pub struct PgDeliveryRepository { pool: PgPool }
impl PgDeliveryRepository { pub fn new(pool: PgPool) -> Self { Self { pool } } }

fn map_delivery(row: &sqlx::postgres::PgRow) -> DeliveryAttempt {
    DeliveryAttempt {
        id:            row.get("id"),
        webhook_id:    row.get("webhook_id"),
        tenant_id:     row.get("tenant_id"),
        event_type:    row.get("event_type"),
        payload:       row.get("payload"),
        attempt:       row.get("attempt"),
        status_code:   row.get("status_code"),
        response_body: row.get("response_body"),
        duration_ms:   row.get("duration_ms"),
        delivered_at:  row.get("delivered_at"),
    }
}

#[async_trait]
impl DeliveryRepository for PgDeliveryRepository {
    async fn save(&self, a: &DeliveryAttempt) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO webhooks.deliveries
                (id, webhook_id, tenant_id, event_type, payload, attempt,
                 status_code, response_body, duration_ms, delivered_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(a.id)
        .bind(a.webhook_id)
        .bind(a.tenant_id)
        .bind(&a.event_type)
        .bind(&a.payload)
        .bind(a.attempt)
        .bind(a.status_code)
        .bind(&a.response_body)
        .bind(a.duration_ms)
        .bind(a.delivered_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_by_webhook(
        &self,
        webhook_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<DeliveryAttempt>> {
        let rows = sqlx::query(
            r#"SELECT id, webhook_id, tenant_id, event_type, payload, attempt,
                      status_code, response_body, duration_ms, delivered_at
               FROM webhooks.deliveries
               WHERE webhook_id = $1
               ORDER BY delivered_at DESC
               LIMIT $2"#,
        )
        .bind(webhook_id)
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(map_delivery).collect())
    }
}
