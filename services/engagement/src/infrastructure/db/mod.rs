//! PostgreSQL repository for the Engagement service.
//!
//! `NotificationDb` wraps a `PgPool` and provides typed access to:
//!   - `engagement.notifications`
//!   - `engagement.notification_templates`
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

#[derive(sqlx::FromRow)]
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
            extra_data:          None,
        }
    }
}

#[derive(sqlx::FromRow)]
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
// Campaign row types for the read-only campaigns projection
// ---------------------------------------------------------------------------

/// Per-recipient send row returned by `list_campaign_sends`.
/// Defined at module scope so `sqlx::FromRow` proc-macro can resolve correctly
/// (inner-function struct derives work in stable Rust but are non-standard).
#[derive(sqlx::FromRow)]
struct SendRow {
    id:                  Uuid,
    campaign_id:         Uuid,
    customer_id:         Uuid,
    channel:             String,
    status:              String,
    provider_message_id: Option<String>,
    error_message:       Option<String>,
    queued_at:           chrono::DateTime<chrono::Utc>,
    sent_at:             Option<chrono::DateTime<chrono::Utc>>,
    failed_at:           Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(sqlx::FromRow)]
struct CampaignRow {
    id:              Uuid,
    name:            String,
    channel:         String,
    status:          String,
    scheduled_at:    Option<chrono::DateTime<chrono::Utc>>,
    total_sent:      i64,
    total_delivered: i64,
    total_failed:    i64,
    created_at:      chrono::DateTime<chrono::Utc>,
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
        sqlx::query(
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
            "#
        )
        .bind(n.id)
        .bind(n.tenant_id)
        .bind(n.customer_id)
        .bind(channel_str(&n.channel))
        .bind(&n.recipient)
        .bind(&n.template_id)
        .bind(&n.rendered_body)
        .bind(&n.subject)
        .bind(status_str(&n.status))
        .bind(priority_str(&n.priority))
        .bind(&n.provider_message_id)
        .bind(&n.error_message)
        .bind(n.queued_at)
        .bind(n.sent_at)
        .bind(n.delivered_at)
        .bind(n.retry_count as i32)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Look up a single notification by primary key.
    pub async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<Notification>> {
        let row = sqlx::query_as::<_, NotificationRow>(
            r#"
            SELECT id, tenant_id, customer_id, channel, recipient,
                   template_id, rendered_body, subject,
                   status, priority,
                   provider_message_id, error_message,
                   queued_at, sent_at, delivered_at, retry_count
            FROM engagement.notifications
            WHERE id = $1
            "#
        )
        .bind(id)
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
        let rows = sqlx::query_as::<_, NotificationRow>(
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
            "#
        )
        .bind(customer_id)
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
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
        let rows = sqlx::query_as::<_, NotificationRow>(
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
            "#
        )
        .bind(tenant_id)
        .bind(status)
        .bind(limit)
        .bind(offset)
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
        sqlx::query(
            r#"
            UPDATE engagement.notifications
               SET status              = $2,
                   provider_message_id = COALESCE($3, provider_message_id),
                   sent_at             = CASE WHEN $2 = 'sent' THEN NOW() ELSE sent_at END,
                   delivered_at        = CASE WHEN $2 = 'delivered' THEN NOW() ELSE delivered_at END
             WHERE id = $1
            "#
        )
        .bind(id)
        .bind(status_str(&status))
        .bind(provider_message_id)
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
        sqlx::query(
            r#"
            INSERT INTO engagement.notification_templates (
                id, tenant_id, template_id, channel, language,
                subject, body, variables, is_active
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9
            )
            "#
        )
        .bind(t.id)
        .bind(t.tenant_id)
        .bind(&t.template_id)
        .bind(channel_str(&t.channel))
        .bind(&t.language)
        .bind(&t.subject)
        .bind(&t.body)
        .bind(&t.variables)
        .bind(t.is_active)
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
        let row = sqlx::query_as::<_, TemplateRow>(
            r#"
            SELECT id, tenant_id, template_id, channel, language,
                   subject, body, variables, is_active
            FROM engagement.notification_templates
            WHERE id = $1
              AND (tenant_id = $2 OR tenant_id IS NULL)
            "#
        )
        .bind(id)
        .bind(tenant_id)
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
        let rows = sqlx::query_as::<_, TemplateRow>(
            r#"
            SELECT id, tenant_id, template_id, channel, language,
                   subject, body, variables, is_active
            FROM engagement.notification_templates
            WHERE tenant_id = $1 OR tenant_id IS NULL
            ORDER BY template_id ASC
            "#
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(TemplateRow::into_template).collect())
    }

    /// Overwrite a template's mutable fields. The `id` and `tenant_id` columns
    /// are immutable after creation.
    pub async fn update_template(&self, t: &NotificationTemplate) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE engagement.notification_templates
               SET template_id = $2,
                   channel     = $3,
                   language    = $4,
                   subject     = $5,
                   body        = $6,
                   variables   = $7,
                   is_active   = $8
             WHERE id = $1
            "#
        )
        .bind(t.id)
        .bind(&t.template_id)
        .bind(channel_str(&t.channel))
        .bind(&t.language)
        .bind(&t.subject)
        .bind(&t.body)
        .bind(&t.variables)
        .bind(t.is_active)
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
        let rows = sqlx::query_as::<_, CampaignRow>(
            r#"
            SELECT id, name, channel, status, scheduled_at,
                   sent_count      AS total_sent,
                   delivered_count AS total_delivered,
                   0::bigint       AS total_failed,
                   created_at
            FROM engagement.campaigns
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            LIMIT  $2
            OFFSET $3
            "#
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
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

    // -----------------------------------------------------------------------
    // Campaign send tracking
    // -----------------------------------------------------------------------

    /// Insert a per-recipient send record for a campaign fan-out.
    /// Returns the newly created row UUID, used to update status after dispatch.
    pub async fn insert_campaign_send(
        &self,
        campaign_id:            Uuid,
        customer_id:            Uuid,
        channel:                &str,
        triggered_by_rule_id:   Option<Uuid>,
        triggered_by_rule_name: Option<&str>,
    ) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO engagement.campaign_sends
                (id, campaign_id, customer_id, channel, status, queued_at,
                 triggered_by_rule_id, triggered_by_rule_name)
            VALUES ($1, $2, $3, $4, 'queued', NOW(), $5, $6)
            "#
        )
        .bind(id)
        .bind(campaign_id)
        .bind(customer_id)
        .bind(channel)
        .bind(triggered_by_rule_id)
        .bind(triggered_by_rule_name)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// Return send history for a single customer across all campaigns.
    /// Used by the communication history tab on the merchant portal customer detail page.
    pub async fn campaign_send_history_for_customer(
        &self,
        customer_id: Uuid,
        tenant_id:   Uuid,
        limit:       i64,
        offset:      i64,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            r#"
            SELECT
                cs.id,
                cs.campaign_id,
                COALESCE(ec.name, 'Campaign') AS campaign_name,
                cs.channel,
                cs.status,
                cs.triggered_by_rule_id,
                cs.triggered_by_rule_name,
                cs.queued_at,
                cs.sent_at,
                cs.delivered_at,
                cs.read_at,
                cs.failed_at,
                cs.opened_at,
                cs.clicked_at,
                cs.clicked_url
            FROM engagement.campaign_sends cs
            LEFT JOIN engagement.campaigns ec
                   ON ec.id = cs.campaign_id AND ec.tenant_id = $2
            WHERE cs.customer_id = $1
            ORDER BY cs.queued_at DESC
            LIMIT  $3
            OFFSET $4
            "#,
        )
        .bind(customer_id)
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        use sqlx::Row;
        Ok(rows.into_iter().map(|r| {
            serde_json::json!({
                "id":                      r.get::<Uuid, _>("id"),
                "campaign_id":             r.get::<Uuid, _>("campaign_id"),
                "campaign_name":           r.get::<String, _>("campaign_name"),
                "channel":                 r.get::<String, _>("channel"),
                "status":                  r.get::<String, _>("status"),
                "triggered_by_rule_id":    r.get::<Option<Uuid>, _>("triggered_by_rule_id"),
                "triggered_by_rule_name":  r.get::<Option<String>, _>("triggered_by_rule_name"),
                "queued_at":               r.get::<chrono::DateTime<chrono::Utc>, _>("queued_at"),
                "sent_at":                 r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("sent_at"),
                "delivered_at":            r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("delivered_at"),
                "read_at":                 r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("read_at"),
                "failed_at":               r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("failed_at"),
                "opened_at":               r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("opened_at"),
                "clicked_at":              r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("clicked_at"),
                "clicked_url":             r.get::<Option<String>, _>("clicked_url"),
            })
        }).collect())
    }

    /// Whether a customer has opened/clicked a specific campaign's send — used by
    /// the marketing service's journey "condition" step to branch yes/no. A
    /// customer can have multiple sends for the same campaign (multi-channel);
    /// any row with a receipt counts as engaged.
    pub async fn campaign_engagement_for_customer(
        &self,
        campaign_id: Uuid,
        customer_id: Uuid,
    ) -> anyhow::Result<Option<(bool, bool)>> {
        let row: Option<(bool, bool)> = sqlx::query_as(
            r#"
            SELECT
                bool_or(opened_at  IS NOT NULL) AS opened,
                bool_or(clicked_at IS NOT NULL) AS clicked
            FROM engagement.campaign_sends
            WHERE campaign_id = $1 AND customer_id = $2
            HAVING COUNT(*) > 0
            "#,
        )
        .bind(campaign_id)
        .bind(customer_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Update the delivery status of a campaign_sends row after dispatch.
    /// Sets the appropriate lifecycle timestamp (sent_at / failed_at) automatically.
    pub async fn update_campaign_send_status(
        &self,
        send_id:             Uuid,
        status:              &str,
        provider_message_id: Option<String>,
        error_message:       Option<String>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE engagement.campaign_sends
               SET status              = $2,
                   provider_message_id = COALESCE($3, provider_message_id),
                   error_message       = $4,
                   sent_at    = CASE WHEN $2 = 'sent'    THEN NOW() ELSE sent_at    END,
                   failed_at  = CASE WHEN $2 = 'failed'  THEN NOW() ELSE failed_at  END
             WHERE id = $1
            "#
        )
        .bind(send_id)
        .bind(status)
        .bind(provider_message_id)
        .bind(error_message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Upsert a campaign row into `engagement.campaigns` when a CAMPAIGN_TRIGGERED
    /// event is received.  This satisfies the schema's record-keeping requirement
    /// and enables the engagement read endpoints without a separate sync job.
    /// Status is set to 'running'; template_id is left NULL (inline templates).
    pub async fn upsert_campaign_from_trigger(
        &self,
        id:              Uuid,
        tenant_id:       Uuid,
        name:            &str,
        channel:         &str,
        created_by:      Uuid,
        recipient_count: i64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO engagement.campaigns
                (id, tenant_id, name, status, channel, template_id, audience_filter,
                 started_at, total_recipients, created_by_user_id)
            VALUES ($1, $2, $3, 'running', $4, NULL, '{}', NOW(), $5, $6)
            ON CONFLICT (id) DO UPDATE SET
                status           = 'running',
                started_at       = COALESCE(engagement.campaigns.started_at, NOW()),
                total_recipients = EXCLUDED.total_recipients
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(name)
        .bind(channel)
        .bind(recipient_count)
        .bind(created_by)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update engagement.campaigns after a fan-out completes.
    /// Sets status to 'completed' and records the final send counters.
    pub async fn complete_engagement_campaign(
        &self,
        id:              Uuid,
        sent_count:      i64,
        delivered_count: i64,
        failed_count:    i64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE engagement.campaigns
               SET status          = 'completed',
                   completed_at    = NOW(),
                   sent_count      = $2,
                   delivered_count = $3
             WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(sent_count)
        .bind(delivered_count)
        .execute(&self.pool)
        .await?;
        let _ = failed_count; // stored only in marketing service totals
        Ok(())
    }

    /// Count campaign_sends rows matching a given status for one campaign.
    /// Used to derive accurate delivery counters at fan-out completion time.
    pub async fn count_campaign_sends_by_status(
        &self,
        campaign_id: Uuid,
        status:      &str,
    ) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM engagement.campaign_sends WHERE campaign_id = $1 AND status = $2",
        )
        .bind(campaign_id)
        .bind(status)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Return a paginated list of individual send rows for a campaign.
    ///
    /// Used by `GET /v1/campaigns/:id/sends` to let the Merchant Portal drill
    /// into per-recipient delivery status without querying the marketing service.
    pub async fn list_campaign_sends(
        &self,
        campaign_id: Uuid,
        tenant_id:   Uuid,
        limit:       i64,
        offset:      i64,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        // Verify the campaign belongs to this tenant before returning rows.
        let campaign_exists: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM engagement.campaigns WHERE id = $1 AND tenant_id = $2",
        )
        .bind(campaign_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        if campaign_exists.is_none() {
            // Return empty rather than leaking tenant data on a 404.
            return Ok(vec![]);
        }

        let rows = sqlx::query_as::<_, SendRow>(
            r#"
            SELECT id, campaign_id, customer_id, channel, status,
                   provider_message_id, error_message,
                   queued_at, sent_at, failed_at
            FROM engagement.campaign_sends
            WHERE campaign_id = $1
            ORDER BY queued_at DESC
            LIMIT  $2
            OFFSET $3
            "#,
        )
        .bind(campaign_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "id":                  r.id,
                    "campaign_id":         r.campaign_id,
                    "customer_id":         r.customer_id,
                    "channel":             r.channel,
                    "status":              r.status,
                    "provider_message_id": r.provider_message_id,
                    "error_message":       r.error_message,
                    "queued_at":           r.queued_at,
                    "sent_at":             r.sent_at,
                    "failed_at":           r.failed_at,
                })
            })
            .collect())
    }

    /// Find a single campaign by UUID within a tenant's scope.
    pub async fn find_campaign_by_id(
        &self,
        id:        Uuid,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let row = sqlx::query_as::<_, CampaignRow>(
            r#"
            SELECT id, name, channel, status, scheduled_at,
                   sent_count      AS total_sent,
                   delivered_count AS total_delivered,
                   0::bigint       AS total_failed,
                   created_at
            FROM engagement.campaigns
            WHERE id = $1
              AND tenant_id = $2
            "#
        )
        .bind(id)
        .bind(tenant_id)
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

    // -------------------------------------------------------------------------
    // Provider receipt correlation
    // -------------------------------------------------------------------------

    /// Find a `campaign_sends` row by provider-assigned message ID.
    /// Used to correlate delivery receipt webhooks back to the original send.
    pub async fn find_send_id_by_provider_msg_id(
        &self,
        provider_message_id: &str,
    ) -> anyhow::Result<Option<Uuid>> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT id
            FROM engagement.campaign_sends
            WHERE provider_message_id = $1
            LIMIT 1
            "#,
        )
        .bind(provider_message_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id,)| id))
    }

    /// Apply a provider delivery receipt event to a `campaign_sends` row.
    ///
    /// `event` is one of: `"delivered"`, `"read"`, `"bounced"`, `"failed"`,
    /// `"opened"`, `"clicked"`.  Unknown events are silently ignored.
    ///
    /// For `"opened"`/`"clicked"`, returns `Some(ReceiptTransition)` describing
    /// whether this is the *first* time the row transitioned to opened/clicked
    /// (repeat receipts are idempotent no-ops) — callers use this to publish a
    /// `CAMPAIGN_OPENED`/`CAMPAIGN_CLICKED` Kafka event exactly once per customer
    /// per campaign, which drives journey auto-enrollment. Other events return `None`.
    pub async fn apply_receipt(
        &self,
        send_id:     Uuid,
        event:       &str,
        clicked_url: Option<&str>,
    ) -> anyhow::Result<Option<ReceiptTransition>> {
        if matches!(event, "opened" | "clicked") {
            // The tuple is the SELECT list, in order. Naming it as a struct
            // would decouple the two and let a column reorder go unnoticed.
            #[allow(clippy::type_complexity)]
            let prior: Option<(Uuid, Uuid, Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)> =
                sqlx::query_as(
                    r#"SELECT cs.campaign_id, cs.customer_id, ec.tenant_id, cs.opened_at, cs.clicked_at
                         FROM engagement.campaign_sends cs
                         LEFT JOIN engagement.campaigns ec ON ec.id = cs.campaign_id
                        WHERE cs.id = $1"#,
                )
                .bind(send_id)
                .fetch_optional(&self.pool)
                .await?;

            let Some((campaign_id, customer_id, tenant_id, prior_opened, prior_clicked)) = prior else {
                return Ok(None);
            };
            let Some(tenant_id) = tenant_id else {
                return Ok(None);
            };
            let fresh_open  = prior_opened.is_none();
            let fresh_click = event == "clicked" && prior_clicked.is_none();

            self.apply_receipt_write(send_id, event, clicked_url).await?;

            return Ok(Some(ReceiptTransition {
                campaign_id, customer_id, tenant_id, fresh_open, fresh_click,
            }));
        }

        self.apply_receipt_write(send_id, event, clicked_url).await?;
        Ok(None)
    }

    async fn apply_receipt_write(
        &self,
        send_id:     Uuid,
        event:       &str,
        clicked_url: Option<&str>,
    ) -> anyhow::Result<()> {
        match event {
            "delivered" => {
                sqlx::query(
                    r#"UPDATE engagement.campaign_sends
                          SET status = 'delivered', delivered_at = NOW()
                        WHERE id = $1 AND status NOT IN ('read','bounced')"#,
                )
                .bind(send_id)
                .execute(&self.pool)
                .await?;
            }
            "read" => {
                sqlx::query(
                    r#"UPDATE engagement.campaign_sends
                          SET status = 'read', read_at = NOW(),
                              delivered_at = COALESCE(delivered_at, NOW())
                        WHERE id = $1"#,
                )
                .bind(send_id)
                .execute(&self.pool)
                .await?;
            }
            "opened" => {
                sqlx::query(
                    r#"UPDATE engagement.campaign_sends
                          SET opened_at = COALESCE(opened_at, NOW()),
                              delivered_at = COALESCE(delivered_at, NOW())
                        WHERE id = $1"#,
                )
                .bind(send_id)
                .execute(&self.pool)
                .await?;
            }
            "clicked" => {
                sqlx::query(
                    r#"UPDATE engagement.campaign_sends
                          SET clicked_at  = COALESCE(clicked_at, NOW()),
                              clicked_url = COALESCE(clicked_url, $2),
                              opened_at   = COALESCE(opened_at, NOW()),
                              delivered_at = COALESCE(delivered_at, NOW())
                        WHERE id = $1"#,
                )
                .bind(send_id)
                .bind(clicked_url)
                .execute(&self.pool)
                .await?;
            }
            "bounced" | "failed" => {
                sqlx::query(
                    r#"UPDATE engagement.campaign_sends
                          SET status = $2, bounced_at = NOW()
                        WHERE id = $1"#,
                )
                .bind(send_id)
                .bind(event)
                .execute(&self.pool)
                .await?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Describes a fresh open/click transition on a `campaign_sends` row —
/// returned by `NotificationDb::apply_receipt` so callers can publish
/// `CAMPAIGN_OPENED`/`CAMPAIGN_CLICKED` exactly once per customer per campaign.
pub struct ReceiptTransition {
    pub campaign_id: Uuid,
    pub customer_id: Uuid,
    pub tenant_id:   Uuid,
    pub fresh_open:  bool,
    pub fresh_click: bool,
}
