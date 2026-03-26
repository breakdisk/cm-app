//! Kafka event consumer for the Engagement Engine.
//!
//! Listens to logistics events and triggers the appropriate notification.
//! This is the core of the automated communication system.
//!
//! Event → Template mapping:
//!   shipment.created        → "shipment_confirmation"   → WhatsApp + Email
//!   driver.assigned         → "pickup_scheduled"        → WhatsApp
//!   driver.pickup.completed → "shipment_picked_up"      → WhatsApp
//!   driver.delivery.completed → "delivery_confirmed"    → WhatsApp + Email
//!   driver.delivery.failed  → "delivery_failed_reschedule" → WhatsApp + SMS
//!   payments.cod.collected  → "cod_receipt"             → WhatsApp

use std::sync::Arc;
use tracing::{error, info, warn};
use crate::application::services::notification_service::NotificationService;
use crate::domain::entities::notification::NotificationPriority;
use logisticos_events::topics;

/// Maps a Kafka event type to the template ID and channel priority.
struct EventNotificationMapping {
    template_id: &'static str,
    priority: NotificationPriority,
    channels: &'static [&'static str],
}

fn get_mapping(event_type: &str) -> Option<EventNotificationMapping> {
    match event_type {
        "shipment.created" => Some(EventNotificationMapping {
            template_id: "shipment_confirmation",
            priority: NotificationPriority::Normal,
            channels: &["whatsapp", "email"],
        }),
        "dispatch.driver.assigned" => Some(EventNotificationMapping {
            template_id: "pickup_scheduled",
            priority: NotificationPriority::Normal,
            channels: &["whatsapp"],
        }),
        "driver.pickup.completed" => Some(EventNotificationMapping {
            template_id: "shipment_picked_up",
            priority: NotificationPriority::Normal,
            channels: &["whatsapp"],
        }),
        "driver.delivery.completed" => Some(EventNotificationMapping {
            template_id: "delivery_confirmed",
            priority: NotificationPriority::High,
            channels: &["whatsapp", "email"],
        }),
        "driver.delivery.failed" => Some(EventNotificationMapping {
            template_id: "delivery_failed_reschedule",
            priority: NotificationPriority::High,
            channels: &["whatsapp", "sms"],
        }),
        "payments.cod.collected" => Some(EventNotificationMapping {
            template_id: "cod_receipt",
            priority: NotificationPriority::High,
            channels: &["whatsapp"],
        }),
        _ => None,
    }
}

/// Processes a raw Kafka message payload and triggers the appropriate notification.
/// Called by the Kafka consumer loop in the service bootstrap.
pub async fn process_event(
    event_type: &str,
    payload: &serde_json::Value,
    notification_service: &NotificationService,
) {
    let Some(mapping) = get_mapping(event_type) else {
        // Not every event triggers a notification — this is expected
        return;
    };

    // Extract customer contact info from event payload
    let customer_id = payload["customer_id"].as_str()
        .and_then(|s| s.parse::<uuid::Uuid>().ok());
    let tenant_id = payload["tenant_id"].as_str()
        .and_then(|s| s.parse::<uuid::Uuid>().ok());

    let (Some(customer_id), Some(tenant_id)) = (customer_id, tenant_id) else {
        warn!(event_type, "Event missing customer_id or tenant_id — skipping notification");
        return;
    };

    // In production: look up customer contact preferences from CDP
    // For now: use phone/email from event payload
    let phone = payload["customer_phone"].as_str().unwrap_or("").to_owned();
    let email = payload["customer_email"].as_str().unwrap_or("").to_owned();

    // Build template variables from event payload
    let vars = serde_json::json!({
        "customer_name":     payload["customer_name"].as_str().unwrap_or("Customer"),
        "tracking_number":   payload["tracking_number"].as_str().unwrap_or(""),
        "shipment_id":       payload["shipment_id"].as_str().unwrap_or(""),
        "driver_name":       payload["driver_name"].as_str().unwrap_or(""),
        "estimated_arrival": payload["estimated_arrival"].as_str().unwrap_or(""),
        "failed_reason":     payload["reason"].as_str().unwrap_or(""),
        "tracking_url":      format!("https://track.logisticos.app/{}", payload["tracking_number"].as_str().unwrap_or("")),
    });

    info!(
        event_type,
        template_id = mapping.template_id,
        customer_id = %customer_id,
        "Triggering notification"
    );

    // In production: fetch template from template repository and call notification_service.build_from_template
    // This stub logs the intent; full implementation wires template_repo
    for channel in mapping.channels {
        let recipient = match *channel {
            "whatsapp" | "sms" => phone.clone(),
            "email"            => email.clone(),
            _                  => continue,
        };
        if recipient.is_empty() {
            warn!(event_type, channel, "No recipient for channel — skipping");
            continue;
        }
        info!(event_type, channel, recipient = %recipient, template = mapping.template_id, "Notification queued");
    }
}
