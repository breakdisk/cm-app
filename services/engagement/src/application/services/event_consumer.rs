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
//!   payments.invoice.generated → "invoice_issued"        → Email                    (recipient_type=merchant)
//!   payments.invoice.generated → "payment_receipt"      → WhatsApp + Email + Push  (recipient_type=customer)
//!   tracking.receipt.email.requested → "shipment_confirmation" → Email   (customer-initiated re-send)

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
        topics::SHIPMENT_CREATED => Some(EventNotificationMapping {
            template_id: "shipment_confirmation",
            priority: NotificationPriority::Normal,
            channels: &["whatsapp", "email"],
        }),
        topics::DRIVER_ASSIGNED => Some(EventNotificationMapping {
            template_id: "pickup_scheduled",
            priority: NotificationPriority::Normal,
            channels: &["whatsapp"],
        }),
        topics::PICKUP_COMPLETED => Some(EventNotificationMapping {
            template_id: "shipment_picked_up",
            priority: NotificationPriority::Normal,
            channels: &["whatsapp"],
        }),
        topics::DELIVERY_COMPLETED => Some(EventNotificationMapping {
            template_id: "delivery_confirmed",
            priority: NotificationPriority::High,
            channels: &["whatsapp", "email"],
        }),
        topics::DELIVERY_FAILED => Some(EventNotificationMapping {
            template_id: "delivery_failed_reschedule",
            priority: NotificationPriority::High,
            channels: &["whatsapp", "sms"],
        }),
        topics::COD_COLLECTED => Some(EventNotificationMapping {
            template_id: "cod_receipt",
            priority: NotificationPriority::High,
            channels: &["whatsapp"],
        }),
        topics::RECEIPT_EMAIL_REQUESTED => Some(EventNotificationMapping {
            template_id: "shipment_confirmation",
            priority: NotificationPriority::Normal,
            channels: &["email"],
        }),
        // INVOICE_GENERATED channels/template differ by recipient_type.
        // Routing is handled in process_event(); return a placeholder here so
        // the event is not silently dropped by the None arm.
        topics::INVOICE_GENERATED => Some(EventNotificationMapping {
            template_id: "__invoice__",   // sentinel — overridden below
            priority: NotificationPriority::Normal,
            channels: &[],               // sentinel — overridden below
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

    // The event envelope wraps data: { ... }
    let data = payload.get("data").unwrap_or(payload);

    // tenant_id lives at the Event envelope level (payload["tenant_id"]).
    // Some legacy/embedded payloads also embed it inside data. Try envelope first.
    let tenant_id = payload["tenant_id"]
        .as_str()
        .or_else(|| data["tenant_id"].as_str())
        .and_then(|s| s.parse::<uuid::Uuid>().ok());

    let Some(tenant_id) = tenant_id else {
        warn!(event_type, "Event missing tenant_id in envelope and data — skipping notification");
        return;
    };

    // Invoice events branch on recipient_type ("merchant" | "customer").
    // All other events address the delivery customer directly.
    let is_invoice_event = event_type == topics::INVOICE_GENERATED;

    // For invoice events we resolve (template_id, channels) dynamically here
    // and override the sentinel values from get_mapping().
    let (resolved_template, resolved_channels): (&str, &[&str]) = if is_invoice_event {
        let recipient_type = data["recipient_type"].as_str().unwrap_or("merchant");
        if recipient_type == "customer" {
            ("payment_receipt", &["whatsapp", "email", "push"])
        } else {
            ("invoice_issued", &["email"])
        }
    } else {
        (mapping.template_id, mapping.channels)
    };

    let (customer_id, phone, email, vars) = if is_invoice_event {
        let recipient_type = data["recipient_type"].as_str().unwrap_or("merchant");
        let total_cents = data["total_cents"].as_i64().unwrap_or(0);
        let currency    = data["currency"].as_str().unwrap_or("PHP");
        let total       = format!("{:.2}", total_cents as f64 / 100.0);

        if recipient_type == "customer" {
            // B2C payment receipt — address the customer
            let customer_id = data["customer_id"].as_str()
                .and_then(|s| s.parse::<uuid::Uuid>().ok())
                .unwrap_or(uuid::Uuid::nil());
            let customer_phone = data["customer_phone"].as_str().unwrap_or("").to_owned();
            let customer_email = data["customer_email"].as_str().unwrap_or("").to_owned();

            let vars = serde_json::json!({
                "invoice_id":     data["invoice_id"].as_str().unwrap_or(""),
                "invoice_number": data["invoice_number"].as_str().unwrap_or(""),
                "customer_name":  data["customer_name"].as_str().unwrap_or("Customer"),
                "total_amount":   total,
                "currency":       currency,
                "paid_at":        data["paid_at"].as_str().unwrap_or(""),
                "tracking_number": data["tracking_number"].as_str().unwrap_or(""),
                "deep_link":      format!("logisticos://invoices/{}", data["invoice_id"].as_str().unwrap_or("")),
            });

            (customer_id, customer_phone, customer_email, vars)
        } else {
            // B2B tax invoice — address the merchant
            let merchant_id = data["merchant_id"].as_str()
                .and_then(|s| s.parse::<uuid::Uuid>().ok())
                .unwrap_or(uuid::Uuid::nil());
            let merchant_email = data["merchant_email"].as_str().unwrap_or("").to_owned();

            let vars = serde_json::json!({
                "invoice_id":     data["invoice_id"].as_str().unwrap_or(""),
                "merchant_name":  data["merchant_name"].as_str().unwrap_or("Merchant"),
                "total_amount":   total,
                "currency":       currency,
                "due_date":       data["due_at"].as_str().unwrap_or(""),
                "invoice_number": data["invoice_number"].as_str().unwrap_or(""),
            });

            (merchant_id, String::new(), merchant_email, vars)
        }
    } else {
        // RECEIPT_EMAIL_REQUESTED carries recipient_email (the address the
        // customer typed into the form) and may omit customer_id — the
        // tracking projection doesn't store it today. Fall back to shipment_id
        // as the notification's audit key; that's enough for the downstream
        // delivery log and keeps the common code path below unchanged.
        let is_receipt_resend = event_type == topics::RECEIPT_EMAIL_REQUESTED;

        let customer_id = data["customer_id"].as_str()
            .and_then(|s| s.parse::<uuid::Uuid>().ok())
            .or_else(|| if is_receipt_resend {
                data["shipment_id"].as_str().and_then(|s| s.parse::<uuid::Uuid>().ok())
            } else {
                None
            });
        let Some(customer_id) = customer_id else {
            warn!(event_type, "Event missing customer_id — skipping notification");
            return;
        };
        let phone = data["customer_phone"].as_str().unwrap_or("").to_owned();
        let email = if is_receipt_resend {
            data["recipient_email"].as_str().unwrap_or("").to_owned()
        } else {
            data["customer_email"].as_str().unwrap_or("").to_owned()
        };

        // Format fee amounts for receipt display
        let total_fee_cents = data["total_fee_cents"].as_i64().unwrap_or(0);
        let cod_cents = data["cod_amount_cents"].as_i64().unwrap_or(0);
        let currency = data["currency"].as_str().unwrap_or("PHP");
        let total_fee = format!("{} {:.2}", currency, total_fee_cents as f64 / 100.0);
        let cod_amount = if cod_cents > 0 {
            format!("{} {:.2}", currency, cod_cents as f64 / 100.0)
        } else {
            "N/A".to_owned()
        };
        let weight_grams = data["weight_grams"].as_u64().unwrap_or(0);
        let weight_display = if weight_grams > 0 {
            format!("{:.1} kg", weight_grams as f64 / 1000.0)
        } else {
            "—".to_owned()
        };

        let vars = serde_json::json!({
            "customer_name":     data["customer_name"].as_str().unwrap_or("Customer"),
            "tracking_number":   data["tracking_number"].as_str().unwrap_or(""),
            "shipment_id":       data["shipment_id"].as_str().unwrap_or(""),
            "driver_name":       data["driver_name"].as_str().unwrap_or(""),
            "estimated_arrival": data["estimated_arrival"].as_str().unwrap_or(""),
            "estimated_delivery": data["estimated_delivery"].as_str().unwrap_or(""),
            "failed_reason":     data["reason"].as_str().unwrap_or(""),
            "origin_address":    data["origin_address"].as_str().unwrap_or(""),
            "destination_address": data["destination_address"].as_str().unwrap_or(""),
            "service_type":      data["service_type"].as_str().unwrap_or("standard"),
            "total_fee":         total_fee,
            "cod_amount":        cod_amount,
            "weight":            weight_display,
            "completed_at":      data["completed_at"].as_str().unwrap_or(""),
            "tracking_url":      format!("https://track.logisticos.app/{}", data["tracking_number"].as_str().unwrap_or("")),
        });

        (customer_id, phone, email, vars)
    };

    info!(
        event_type,
        template_id = resolved_template,
        customer_id = %customer_id,
        "Triggering notification"
    );

    use crate::domain::entities::template::{NotificationChannel, NotificationTemplate};

    for channel in resolved_channels {
        let (recipient, notif_channel) = match *channel {
            "whatsapp" => (phone.clone(),  NotificationChannel::WhatsApp),
            "sms"      => (phone.clone(),  NotificationChannel::Sms),
            "email"    => (email.clone(),  NotificationChannel::Email),
            "push"     => (customer_id.to_string(), NotificationChannel::Push),
            _          => continue,
        };

        // Push uses customer_id as the device-token lookup key, so it's always
        // non-empty as long as we have a customer. For other channels, skip if blank.
        if recipient.is_empty() {
            warn!(event_type, channel, "No recipient for channel — skipping");
            continue;
        }

        // Build inline templates — these are the hardcoded receipt templates.
        // In production, templates come from the DB template registry.
        let (subject, body) = match resolved_template {
            "shipment_confirmation" => (
                Some(format!("Shipment Confirmed — {}", vars["tracking_number"].as_str().unwrap_or(""))),
                "Hi {{customer_name}},\n\n\
                 Your shipment has been confirmed and is being processed.\n\n\
                 Tracking Number: {{tracking_number}}\n\
                 From: {{origin_address}}\n\
                 To:   {{destination_address}}\n\
                 Service: {{service_type}}\n\
                 Weight:  {{weight}}\n\
                 Fee:     {{total_fee}}\n\
                 COD:     {{cod_amount}}\n\n\
                 Track your shipment: {{tracking_url}}\n\n\
                 Thank you for choosing CargoMarket!\n\
                 — CargoMarket Logistics".to_owned(),
            ),
            "pickup_scheduled" => (
                None,
                "Hi {{customer_name}},\n\n\
                 A rider has been assigned to pick up your shipment {{tracking_number}}.\n\
                 Track your pickup: {{tracking_url}}\n\n\
                 — CargoMarket".to_owned(),
            ),
            "shipment_picked_up" => (
                None,
                "Hi {{customer_name}},\n\n\
                 Your package {{tracking_number}} has been picked up and is on its way!\n\
                 Track it here: {{tracking_url}}\n\n\
                 — CargoMarket".to_owned(),
            ),
            "delivery_confirmed" => (
                Some(format!("Delivery Confirmed — {}", vars["tracking_number"].as_str().unwrap_or(""))),
                "Hi {{customer_name}},\n\n\
                 Your shipment {{tracking_number}} has been delivered!\n\n\
                 Delivered at: {{completed_at}}\n\n\
                 If you have any questions, contact us or track details at: {{tracking_url}}\n\n\
                 Thank you for shipping with CargoMarket!\n\
                 — CargoMarket Logistics".to_owned(),
            ),
            "delivery_failed_reschedule" => (
                None,
                "Hi {{customer_name}},\n\n\
                 We attempted delivery of {{tracking_number}} but couldn't complete it.\n\
                 Reason: {{failed_reason}}\n\n\
                 We'll retry delivery. To reschedule: {{tracking_url}}\n\n\
                 — CargoMarket".to_owned(),
            ),
            "cod_receipt" => (
                None,
                "Hi {{customer_name}},\n\n\
                 Cash on Delivery collected for {{tracking_number}}.\n\
                 Amount: {{cod_amount}}\n\n\
                 — CargoMarket".to_owned(),
            ),
            "payment_receipt" => (
                Some(format!("Payment Receipt {} — CargoMarket", vars["invoice_number"].as_str().unwrap_or(""))),
                "Dear {{customer_name}},\n\n\
                 Your payment has been received and a receipt has been issued.\n\n\
                 Receipt Number: {{invoice_number}}\n\
                 Amount Paid:    {{currency}} {{total_amount}}\n\
                 Date:           {{paid_at}}\n\
                 Tracking:       {{tracking_number}}\n\n\
                 View your receipt in the app: {{deep_link}}\n\n\
                 Thank you for shipping with CargoMarket!\n\
                 — CargoMarket".to_owned(),
            ),
            "invoice_issued" => (
                Some(format!("Invoice {} — CargoMarket", vars["invoice_number"].as_str().unwrap_or(""))),
                "Dear {{merchant_name}},\n\n\
                 Your invoice {{invoice_number}} has been issued.\n\n\
                 Amount Due: {{currency}} {{total_amount}}\n\
                 Due Date:   {{due_date}}\n\n\
                 You can view and pay your invoice in the CargoMarket Merchant Portal.\n\n\
                 — CargoMarket Billing".to_owned(),
            ),
            _ => (None, "{{body}}".to_owned()),
        };

        let template = NotificationTemplate {
            id:          uuid::Uuid::new_v4(),
            tenant_id:   Some(tenant_id),
            template_id: resolved_template.to_owned(),
            channel:     notif_channel,
            language:    "en".into(),
            subject,
            body,
            variables:   vars.as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default(),
            is_active:   true,
        };

        let mut notification = match NotificationService::build_from_template(
            &template,
            tenant_id,
            customer_id,
            recipient.clone(),
            &vars,
            mapping.priority,
        ) {
            Ok(n) => n,
            Err(e) => {
                error!(event_type, channel, err = %e, "Failed to build notification");
                continue;
            }
        };

        match notification_service.dispatch(&mut notification).await {
            Ok(_)  => info!(event_type, channel, recipient = %recipient, template = resolved_template, "Notification sent"),
            Err(e) => error!(event_type, channel, err = %e, "Notification dispatch failed"),
        }
    }
}
