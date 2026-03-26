//! NotificationService — processes queued notifications and dispatches them
//! through the correct channel adapter.
//!
//! Triggered by:
//!   a) Direct API call (from business-logic rules engine)
//!   b) Kafka event consumer (reacts to shipment.created, delivery.completed, etc.)

use std::sync::Arc;
use chrono::Utc;
use logisticos_errors::{AppError, AppResult};
use crate::{
    domain::entities::{
        notification::{Notification, NotificationPriority, NotificationStatus},
        template::{NotificationChannel, NotificationTemplate},
    },
    infrastructure::channels::ChannelAdapter,
};

pub struct NotificationService {
    whatsapp: Arc<dyn ChannelAdapter>,
    sms:      Arc<dyn ChannelAdapter>,
    email:    Arc<dyn ChannelAdapter>,
}

impl NotificationService {
    pub fn new(
        whatsapp: Arc<dyn ChannelAdapter>,
        sms:      Arc<dyn ChannelAdapter>,
        email:    Arc<dyn ChannelAdapter>,
    ) -> Self {
        Self { whatsapp, sms, email }
    }

    pub async fn dispatch(&self, notification: &mut Notification) -> AppResult<()> {
        let adapter: &dyn ChannelAdapter = match notification.channel {
            NotificationChannel::WhatsApp => self.whatsapp.as_ref(),
            NotificationChannel::Sms      => self.sms.as_ref(),
            NotificationChannel::Email    => self.email.as_ref(),
            NotificationChannel::Push     => {
                // Push notifications handled by FCM — separate adapter
                return Err(AppError::ExternalService {
                    service: "push".into(),
                    message: "Push not yet wired".into(),
                });
            }
        };

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
