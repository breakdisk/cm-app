//! PostgreSQL repository for the Engagement service.
//!
//! `NotificationDb` wraps a `PgPool` and provides typed access to:
//!   - `engagement.notifications`
//!   - `engagement.templates`
//!   - `engagement.campaigns` (read-only projection for the HTTP layer)
//!
//! Every method returns `anyhow::Result` so callers can wrap the error with
//! `AppError::internal` using the standard `map_err(AppError::internal)`
//! pattern used across the codebase.

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entities::{
    notification::{Notification, NotificationPriority, NotificationStatus},
    template::{NotificationChannel, NotificationTemplate},
};

// ---------------------------------------------------------------------------
// Row types — flat structs that mirror the PostgreSQL columns exactly.
// Converted to domain types in `from_row` methods.
// ---------------------------------------------------------------------------

struct NotificationRow {
    id:                  Uuid,
    tenant_id:           Uuid,
    customer_id:         Uuid,
    channel:             String,
    recipient:           String,
    template_id:         String,
    rendered_body:       String,
    subject:             Option<String>,
    status:              String,
    priority:            String,
    provider_message_id: Option<String>,
    error_message:       Option<String>,
    queued_at:           chrono::DateTime<chrono::Utc>,
    sent_at:             Option<chrono::DateTime<chrono::Utc>>,
    delivered_at:        Option<chrono::DateTime<chrono::Utc>>,
    retry_count:         i32,
}

impl NotificationRow {
    fn into_notification(self) -> Notification {
        let channel = match self.channel.as_str() {
            "whatsapp" => NotificationChannel::WhatsApp,
            "sms"      => NotificationChannel::Sms,
            "email"    => NotificationChannel::Email,
            _          => NotificationChannel::Push,
        };
        let status = match self.status.as_str() {
            "sending"   => NotificationStatus::Sending,
            "sent"      => NotificationStatus::Sent,
            "delivered" => NotificationStatus::Delivered,
            "failed"    => NotificationStatus::Failed,
            "bounced"   => NotificationStatus::Bounced,
            _           => NotificationStatus::Queued,
        };
        let priority = match self.priority.as_str() {
            "high" => NotificationPriority::High,
            "low"  => NotificationPriority::Low,
            _      => NotificationPriority::Normal,
        };
        Notification {
            id:                  self.id,
            tenant_id:           self.tenant_id,
            customer_id:         self.customer_id,
            channel,
            recipient:           self.recipient,
            template_id:         self.template_id,
            rendered_body:       self.rendered_body,
            subject:             self.subject,
            status,
            priority,
            provider_message_id: self.provider_message_id,
            error_message:       self.error_message,
            queued_at:           self.queued_at,
            sent_at:             self.sent_at,
            delivered_at:        self.delivered_at,
            retry_count:         self.retry_count as u32,
        }
    }
}

struct TemplateRow {
    id:          Uuid,
    tenant_id:   Option<Uuid>,
    template_id: String,
    channel:     String,
    language:    String,
    subject:     Option<String>,
    body:        String,
    variables:   Vec<String>,
    is_active:   bool,
}

impl TemplateRow {
    fn into_template(self) -> NotificationTemplate {
        let channel = match self.channel.as_str() {
            "whatsapp" => NotificationChannel::WhatsApp,
            "sms"      => NotificationChannel::Sms,
            "email"    => NotificationChannel::Email,
            _          => NotificationChannel::Push,
        };
        NotificationTemplate {
            id:          self.id,
            tenant_id:   self.tenant_id,
            template_id: self.template_id,
            channel,
            language:    self.language,
            subject:     self.subject,
            body:        self.body,
            variables:   self.variables,
            is_active:   self.is_active,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions for status/priority/channel → &'static str
// ---------------------------------------------------------------------------

fn status_str(s: &NotificationStatus) -> &'static str {
    match s {
        NotificationStatus::Queued    => "queued",
        NotificationStatus::Sending   => "sending",
        NotificationStatus::Sent      => "sent",
        NotificationStatus::Delivered => "delivered",
        NotificationStatus::Failed    => "failed",
        NotificationStatus::Bounced   => "bounced",
    }
}

fn priority_str(p: &NotificationPriority) -> &'static str {
    match p {
        NotificationPriority::High   => "high",
        NotificationPriority::Normal => "normal",
        NotificationPriority::Low    => "low",
    }
}

fn channel_str(c: &NotificationChannel) -> &'static str {
    c.as_str()
}

// ---------------------------------------------------------------------------
// Repository struct
// ---------------------------------------------------------------------------

/// Thin database access layer for the engagement service.
/// Construct via `NotificationDb::new(pool)`.
pub struct NotificationDb {
    pool: PgPool,
}

impl NotificationDb {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // -----------------------------------------------------------------------
    // Notification operations
    // -----------------------------------------------------------------------

    /// Persist a new notification record.  ON CONFLICT DO NOTHING ensures that
    /// idempotent retries from the Kafka consumer do not create duplicates when
    /// the same message is re-delivered.
    pub async fn insert_notification(&self, n: &Notification) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO engagement.notifications (
                id, tenant_id, customer_id, channel, recipient,
                template_id, rendered_body, subject,
                status, priority,
                provider_message_id, error_message,
                queued_at, sent_at, delivered_at, retry_count
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8,
                $9, $10,
                $11, $12,
                $13, $14, $15, $16
            )
            ON CONFLICT (id) DO NOTHING
            "#,
            n.id,
            n.tenant_id,
            n.customer_id,
            channel_str(&n.channel),
            n.recipient,
            n.template_id,
            n.rendered_body,
            n.subject,
            status_str(&n.status),
            priority_str(&n.priority),
            n.provider_message_id,
            n.error_message,
            n.queued_at,
            n.sent_at,
            n.delivered_at,
            n.retry_count as i32,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Look up a single notification by primary key.
    pub async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<Notification>> {
        let row = sqlx::query_as!(
            NotificationRow,
            r#"
            SELECT id, tenant_id, customer_id, channel, recipient,
                   template_id, rendered_body, subject,
                   status, priority,
                   provider_message_id, error_message,
                   queued_at, sent_at, delivered_at, retry_count
            FROM engagement.notifications
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(NotificationRow::into_notification))
    }

    /// Return a paginated list of notifications for a specific customer, scoped
    /// to the caller's tenant.
    pub async fn list_by_customer(
        &self,
        customer_id: Uuid,
        tenant_id:   Uuid,
        limit:       i64,
        offset:      i64,
    ) -> anyhow::Result<Vec<Notification>> {
        let rows = sqlx::query_as!(
            NotificationRow,
            r#"
            SELECT id, tenant_id, customer_id, channel, recipient,
                   template_id, rendered_body, subject,
                   status, priority,
                   provider_message_id, error_message,
                   queued_at, sent_at, delivered_at, retry_count
            FROM engagement.notifications
            WHERE customer_id = $1
              AND tenant_id   = $2
            ORDER BY queued_at DESC
            LIMIT  $3
            OFFSET $4
            "#,
            customer_id,
            tenant_id,
            limit,
            offset,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(NotificationRow::into_notification).collect())
    }

    /// Return a paginated list of notifications for a tenant with an optional
    /// status filter.
    pub async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        status:    Option<&str>,
        limit:     i64,
        offset:    i64,
    ) -> anyhow::Result<Vec<Notification>> {
        let rows = sqlx::query_as!(
            NotificationRow,
            r#"
            SELECT id, tenant_id, customer_id, channel, recipient,
                   template_id, rendered_body, subject,
                   status, priority,
                   provider_message_id, error_message,
                   queued_at, sent_at, delivered_at, retry_count
            FROM engagement.notifications
            WHERE tenant_id = $1
              AND ($2::text IS NULL OR status = $2)
            ORDER BY queued_at DESC
            LIMIT  $3
            OFFSET $4
            "#,
            tenant_id,
            status,
            limit,
            offset,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(NotificationRow::into_notification).collect())
    }

    /// Update the delivery status and optional provider message ID of a
    /// notification after dispatch completes.
    pub async fn update_status(
        &self,
        id:                  Uuid,
        status:              NotificationStatus,
        provider_message_id: Option<String>,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            UPDATE engagement.notifications
               SET status              = $2,
                   provider_message_id = COALESCE($3, provider_message_id),
                   sent_at             = CASE WHEN $2 = 'sent' THEN NOW() ELSE sent_at END,
                   delivered_at        = CASE WHEN $2 = 'delivered' THEN NOW() ELSE delivered_at END
             WHERE id = $1
            "#,
            id,
            status_str(&status),
            provider_message_id,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Template operations
    // -----------------------------------------------------------------------

    /// Insert a new notification template.  Conflict on `(tenant_id, template_id)`
    /// is treated as an error — callers should use `update_template` for updates.
    pub async fn insert_template(&self, t: &NotificationTemplate) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO engagement.templates (
                id, tenant_id, template_id, channel, language,
                subject, body, variables, is_active
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9
            )
            "#,
            t.id,
            t.tenant_id,
            t.template_id,
            channel_str(&t.channel),
            t.language,
            t.subject,
            t.body,
            &t.variables,
            t.is_active,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find a template by UUID, returning templates whose `tenant_id` matches
    /// OR is NULL (system-wide templates shared across all tenants).
    pub async fn find_template_by_id(
        &self,
        id:        Uuid,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<NotificationTemplate>> {
        let row = sqlx::query_as!(
            TemplateRow,
            r#"
            SELECT id, tenant_id, template_id, channel, language,
                   subject, body, variables, is_active
            FROM engagement.templates
            WHERE id = $1
              AND (tenant_id = $2 OR tenant_id IS NULL)
            "#,
            id,
            tenant_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(TemplateRow::into_template))
    }

    /// List all active templates for a tenant, including global templates
    /// (tenant_id IS NULL).  Ordered by template_id for predictable output.
    pub async fn list_templates(
        &self,
        tenant_id: Uuid,
    ) -> anyhow::Result<Vec<NotificationTemplate>> {
        let rows = sqlx::query_as!(
            TemplateRow,
            r#"
            SELECT id, tenant_id, template_id, channel, language,
                   subject, body, variables, is_active
            FROM engagement.templates
            WHERE tenant_id = $1 OR tenant_id IS NULL
            ORDER BY template_id ASC
            "#,
            tenant_id,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(TemplateRow::into_template).collect())
    }

    /// Overwrite a template's mutable fields. The `id` and `tenant_id` columns
    /// are immutable after creation.
    pub async fn update_template(&self, t: &NotificationTemplate) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            UPDATE engagement.templates
               SET template_id = $2,
                   channel     = $3,
                   language    = $4,
                   subject     = $5,
                   body        = $6,
                   variables   = $7,
                   is_active   = $8
             WHERE id = $1
            "#,
            t.id,
            t.template_id,
            channel_str(&t.channel),
            t.language,
            t.subject,
            t.body,
            &t.variables,
            t.is_active,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Campaign operations (read-only — campaigns are owned by the marketing
    // service; the engagement service reads a denormalised projection view).
    // -----------------------------------------------------------------------

    /// Return a paginated list of campaigns for a tenant from the shared
    /// `engagement.campaigns` view.
    pub async fn list_campaigns(
        &self,
        tenant_id: Uuid,
        limit:     i64,
        offset:    i64,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, name, channel, status, scheduled_at, total_sent, total_delivered, total_failed, created_at
            FROM engagement.campaigns
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            LIMIT  $2
            OFFSET $3
            "#,
            tenant_id,
            limit,
            offset,
        )
        .fetch_all(&self.pool)
        .await?;

        let campaigns = rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "id":              r.id,
                    "name":            r.name,
                    "channel":         r.channel,
                    "status":          r.status,
                    "scheduled_at":    r.scheduled_at,
                    "total_sent":      r.total_sent,
                    "total_delivered": r.total_delivered,
                    "total_failed":    r.total_failed,
                    "created_at":      r.created_at,
                })
            })
            .collect();

        Ok(campaigns)
    }

    /// Find a single campaign by UUID within a tenant's scope.
    pub async fn find_campaign_by_id(
        &self,
        id:        Uuid,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let row = sqlx::query!(
            r#"
            SELECT id, name, channel, status, scheduled_at, total_sent, total_delivered, total_failed, created_at
            FROM engagement.campaigns
            WHERE id = $1
              AND tenant_id = $2
            "#,
            id,
            tenant_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            serde_json::json!({
                "id":              r.id,
                "name":            r.name,
                "channel":         r.channel,
                "status":          r.status,
                "scheduled_at":    r.scheduled_at,
                "total_sent":      r.total_sent,
                "total_delivered": r.total_delivered,
                "total_failed":    r.total_failed,
                "created_at":      r.created_at,
            })
        }))
    }
}
