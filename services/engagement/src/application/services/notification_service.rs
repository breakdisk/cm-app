//! NotificationService — processes queued notifications and dispatches them
//! through the correct channel adapter.
//!
//! Triggered by:
//!   a) Direct API call (from business-logic rules engine)
//!   b) Kafka event consumer (reacts to shipment.created, delivery.completed, etc.)

use std::sync::Arc;
use chrono::Utc;
use logisticos_errors::{AppError, AppResult};
use sqlx::PgPool;
use crate::{
    domain::entities::{
        notification::{Notification, NotificationPriority, NotificationStatus},
        template::{NotificationChannel, NotificationTemplate},
    },
    infrastructure::{
        channels::ChannelAdapter,
        db::NotificationDb,
    },
};

pub struct NotificationService {
    /// Global fallback adapters — used when the tenant has no custom credentials.
    whatsapp: Arc<dyn ChannelAdapter>,
    sms:      Arc<dyn ChannelAdapter>,
    email:    Arc<dyn ChannelAdapter>,
    push:     Arc<dyn ChannelAdapter>,
    /// DB pool used to look up per-tenant channel config at dispatch time.
    db_pool:  Option<PgPool>,
}

impl NotificationService {
    pub fn new(
        whatsapp: Arc<dyn ChannelAdapter>,
        sms:      Arc<dyn ChannelAdapter>,
        email:    Arc<dyn ChannelAdapter>,
        push:     Arc<dyn ChannelAdapter>,
    ) -> Self {
        Self { whatsapp, sms, email, push, db_pool: None }
    }

    pub fn with_db(mut self, pool: PgPool) -> Self {
        self.db_pool = Some(pool);
        self
    }

    pub async fn dispatch(&self, notification: &mut Notification) -> AppResult<()> {
        // Check per-tenant channel config when DB is available.
        if let Some(pool) = &self.db_pool {
            let db = NotificationDb::new(pool.clone());
            if let Ok(Some(cfg)) = db.get_channel_config(notification.tenant_id).await {
                if !cfg.is_channel_enabled(notification.channel) {
                    return Err(AppError::BusinessRule(format!(
                        "{} channel is disabled for this tenant",
                        notification.channel.as_str()
                    )));
                }
                // Use per-tenant adapter if credentials are configured.
                if let Some(tenant_adapter) = cfg.build_adapter(notification.channel) {
                    return self.dispatch_with(notification, tenant_adapter.as_ref()).await;
                }
            }
        }

        // Fall back to global env-var-backed adapter.
        let adapter: &dyn ChannelAdapter = match notification.channel {
            NotificationChannel::WhatsApp => self.whatsapp.as_ref(),
            NotificationChannel::Sms      => self.sms.as_ref(),
            NotificationChannel::Email    => self.email.as_ref(),
            NotificationChannel::Push     => self.push.as_ref(),
        };

        self.dispatch_with(notification, adapter).await
    }

    async fn dispatch_with(
        &self,
        notification: &mut Notification,
        adapter: &dyn ChannelAdapter,
    ) -> AppResult<()> {
        match adapter.send(&notification.recipient, &notification.rendered_body, notification.subject.as_deref()).await {
            Ok(provider_id) => {
                notification.mark_sent(provider_id);
                tracing::info!(
                    notification_id = %notification.id,
                    channel = notification.channel.as_str(),
                    "Notification sent"
                );
            }
            Err(e) => {
                notification.mark_failed(e.clone());
                tracing::warn!(
                    notification_id = %notification.id,
                    channel = notification.channel.as_str(),
                    error = %e,
                    retry_count = notification.retry_count,
                    "Notification send failed"
                );
                if !notification.can_retry() {
                    return Err(AppError::ExternalService {
                        service: notification.channel.as_str().into(),
                        message: e,
                    });
                }
            }
        }
        Ok(())
    }

    /// Build a Notification from a template + variables.
    pub fn build_from_template(
        template: &NotificationTemplate,
        tenant_id: uuid::Uuid,
        customer_id: uuid::Uuid,
        recipient: String,
        vars: &serde_json::Value,
        priority: NotificationPriority,
    ) -> AppResult<Notification> {
        let rendered_body = template.render(vars)
            .map_err(|e| AppError::Validation(e))?;

        let rendered_subject = template.subject.as_ref()
            .map(|s| {
                let mut subj = s.clone();
                if let Some(obj) = vars.as_object() {
                    for (k, v) in obj {
                        if let Some(val) = v.as_str() {
                            subj = subj.replace(&format!("{{{{{}}}}}", k), val);
                        }
                    }
                }
                subj
            });

        Ok(Notification {
            id: uuid::Uuid::new_v4(),
            tenant_id,
            customer_id,
            channel: template.channel,
            recipient,
            template_id: template.template_id.clone(),
            rendered_body,
            subject: rendered_subject,
            status: NotificationStatus::Queued,
            priority,
            provider_message_id: None,
            error_message: None,
            queued_at: Utc::now(),
            sent_at: None,
            delivered_at: None,
            retry_count: 0,
        })
    }
}
