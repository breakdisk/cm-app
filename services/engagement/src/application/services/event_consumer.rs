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
//!   payments.cod.remitted      → "cod_remitted"          → Email                    (merchant wallet credited)
//!   payments.wallet.withdrawal_disbursed → "withdrawal_disbursed" → Email + Push    (carrier payout sent)
//!   payments.wallet.withdrawal_rejected  → "withdrawal_rejected"  → Email + Push    (carrier payout declined)
//!   tracking.receipt.email.requested → "shipment_confirmation" → Email   (customer-initiated re-send)
//!   identity.tenant.finalized        → "merchant_welcome"      → Email              (new merchant onboarded)
//!   identity.otp_requested (email)   → "otp_code"             → Email
//!   identity.otp_requested (phone)   → "otp_code"             → SMS

use tracing::{error, info, warn};
use crate::application::services::notification_service::NotificationService;
use crate::domain::entities::notification::NotificationPriority;
use crate::infrastructure::cache::SuppressionCache;
use crate::infrastructure::db::NotificationDb;
use logisticos_events::topics;

/// Minimal Kafka publisher interface used by the engagement service to emit
/// `CAMPAIGN_COMPLETED` after fan-out finishes.
#[async_trait::async_trait]
pub trait EngagementPublisher: Send + Sync {
    async fn publish(&self, topic: &str, key: &str, payload: &[u8]) -> anyhow::Result<()>;
}

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
        topics::COD_REMITTED => Some(EventNotificationMapping {
            template_id: "cod_remitted",
            priority: NotificationPriority::Normal,
            channels: &["email"],
        }),
        topics::WALLET_WITHDRAWAL_DISBURSED => Some(EventNotificationMapping {
            template_id: "withdrawal_disbursed",
            priority: NotificationPriority::High,
            channels: &["email", "push"],
        }),
        topics::WALLET_WITHDRAWAL_REJECTED => Some(EventNotificationMapping {
            template_id: "withdrawal_rejected",
            priority: NotificationPriority::High,
            channels: &["email", "push"],
        }),
        topics::RECEIPT_EMAIL_REQUESTED => Some(EventNotificationMapping {
            template_id: "shipment_confirmation",
            priority: NotificationPriority::Normal,
            channels: &["email"],
        }),
        // CAMPAIGN_TRIGGERED is handled by handle_campaign_triggered() — not routed here.
        // INVOICE_GENERATED channels/template differ by recipient_type.
        // Routing is handled in process_event(); return a placeholder here so
        // the event is not silently dropped by the None arm.
        topics::INVOICE_GENERATED => Some(EventNotificationMapping {
            template_id: "__invoice__",   // sentinel — overridden below
            priority: NotificationPriority::Normal,
            channels: &[],               // sentinel — overridden below
        }),
        // Merchant onboarding — send a welcome email when a draft tenant finalizes.
        topics::TENANT_FINALIZED => Some(EventNotificationMapping {
            template_id: "merchant_welcome",
            priority:    NotificationPriority::High,
            channels:    &["email"],
        }),
        // OTP delivery — email or SMS depending on which identifier was provided.
        // Dispatch loop skips whichever recipient field is empty.
        topics::OTP_REQUESTED => Some(EventNotificationMapping {
            template_id: "otp_code",
            priority:    NotificationPriority::High,
            channels:    &["email", "sms"],
        }),
        // Cross-border hub milestones — fanned out per recipient by
        // `handle_hub_milestone`, which synthesises a single-recipient payload and
        // re-enters `process_event` for each shipment in the container.
        topics::CONTAINER_ARRIVED_AT_PORT => Some(EventNotificationMapping {
            template_id: "shipment_at_port",
            priority: NotificationPriority::Normal,
            channels: &["whatsapp", "push"],
        }),
        topics::CONTAINER_CUSTOMS_HOLD => Some(EventNotificationMapping {
            template_id: "shipment_customs_hold",
            priority: NotificationPriority::Normal,
            channels: &["whatsapp", "push"],
        }),
        topics::CONTAINER_CUSTOMS_CLEARED => Some(EventNotificationMapping {
            template_id: "shipment_customs_cleared",
            priority: NotificationPriority::Normal,
            channels: &["whatsapp", "push"],
        }),
        _ => None,
    }
}

/// Fan-out handler for container milestone events. A container covers many
/// shipments, so these events carry a `details` list of per-shipment recipients.
/// For each recipient we synthesise a single-recipient payload and re-enter
/// `process_event`, reusing the full template/channel/suppression machinery.
pub async fn handle_hub_milestone(
    event_type: &str,
    payload: &serde_json::Value,
    notification_service: &NotificationService,
    suppression_cache: &SuppressionCache,
) {
    let tenant_id = payload["tenant_id"].as_str().unwrap_or_default();
    let data = payload.get("data").unwrap_or(payload);
    let Some(details) = data["details"].as_array() else {
        warn!(event_type, "hub milestone event has no recipient details — no notifications sent");
        return;
    };
    if details.is_empty() {
        warn!(event_type, "hub milestone event has empty details — no notifications sent");
        return;
    }

    for detail in details {
        let synthetic = serde_json::json!({
            "tenant_id": tenant_id,
            "data": {
                "customer_id":     detail["customer_id"].clone(),
                "shipment_id":     detail["shipment_id"].clone(),
                "customer_phone":  detail["customer_phone"].clone(),
                "customer_email":  detail["customer_email"].clone(),
                "customer_name":   detail["customer_name"].clone(),
                "tracking_number": detail["tracking_number"].clone(),
            }
        });
        process_event(event_type, &synthetic, notification_service, suppression_cache).await;
    }
}

/// Processes a raw Kafka message payload and triggers the appropriate notification.
/// Called by the Kafka consumer loop in the service bootstrap.
pub async fn process_event(
    event_type: &str,
    payload: &serde_json::Value,
    notification_service: &NotificationService,
    suppression_cache: &SuppressionCache,
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
    let is_invoice_event      = event_type == topics::INVOICE_GENERATED;
    let is_cod_remitted       = event_type == topics::COD_REMITTED;
    let is_withdrawal_event   = event_type == topics::WALLET_WITHDRAWAL_DISBURSED
                             || event_type == topics::WALLET_WITHDRAWAL_REJECTED;
    let is_tenant_finalized   = event_type == topics::TENANT_FINALIZED;
    let is_otp_event          = event_type == topics::OTP_REQUESTED;

    // For invoice events we resolve (template_id, channels) dynamically here
    // and override the sentinel values from get_mapping().
    let (resolved_template, resolved_channels): (&str, &[&str]) = if is_otp_event || is_withdrawal_event {
        // Template/channels already set correctly in get_mapping(); pass through.
        (mapping.template_id, mapping.channels)
    } else if is_invoice_event {
        let recipient_type = data["recipient_type"].as_str().unwrap_or("merchant");
        if recipient_type == "customer" {
            ("payment_receipt", &["whatsapp", "email", "push"])
        } else {
            ("invoice_issued", &["email"])
        }
    } else {
        (mapping.template_id, mapping.channels)
    };

    let (customer_id, phone, email, vars) = if is_tenant_finalized {
        // Welcome email to the merchant who just completed onboarding.
        // The event carries owner_email and the finalized business name.
        let owner_email   = data["owner_email"].as_str().unwrap_or("").to_owned();
        let business_name = data["name"].as_str().unwrap_or("Merchant").to_owned();

        if owner_email.is_empty() {
            warn!(event_type, "TENANT_FINALIZED missing owner_email — skipping welcome email");
            return;
        }

        let vars = serde_json::json!({
            "business_name":       business_name,
            "merchant_portal_url": "https://merchant.cargomarket.net",
        });

        // Use tenant_id as the notification audit UUID (no separate customer entity).
        (tenant_id, String::new(), owner_email, vars)
    } else if is_otp_event {
        // OTP delivery — email or SMS based on which identifier was provided.
        // No customer entity exists yet (this fires before otp_verify creates/resolves the user).
        let recipient_email = data["email"].as_str().unwrap_or("").to_owned();
        let phone_number    = data["phone_number"].as_str().unwrap_or("").to_owned();
        let otp_code        = data["otp_code"].as_str().unwrap_or("").to_owned();

        if otp_code.is_empty() || (recipient_email.is_empty() && phone_number.is_empty()) {
            warn!(event_type, "OTP_REQUESTED missing otp_code or both email and phone_number — skipping");
            return;
        }

        let vars = serde_json::json!({ "otp_code": otp_code });
        // Use nil UUID as customer_id — no user record exists at OTP-send time.
        // phone routes to SMS channel; email routes to Email channel.
        // The dispatch loop below skips whichever recipient is empty.
        (uuid::Uuid::nil(), phone_number, recipient_email, vars)
    } else if is_withdrawal_event {
        // Withdrawal disbursed/rejected — notify the carrier partner.
        // carrier_email is stored on the WithdrawalRequest (migration 0013) and forwarded
        // in the event payload. Push fires via tenant_id; email channel gracefully
        // no-ops when carrier_email is empty (carrier did not supply one at request time).
        let amount_cents   = data["amount_centavos"].as_i64().unwrap_or(0);
        let amount_php     = format!("{:.2}", amount_cents as f64 / 100.0);
        let review_note    = data["review_note"].as_str().unwrap_or("").to_owned();
        let withdrawal_id  = data["withdrawal_id"].as_str().unwrap_or("");
        let is_disbursed   = event_type == topics::WALLET_WITHDRAWAL_DISBURSED;

        let carrier_email = data["carrier_email"].as_str().unwrap_or("").to_owned();

        let vars = serde_json::json!({
            "withdrawal_id": withdrawal_id,
            "amount":        amount_php,
            "currency":      "PHP",
            "status":        if is_disbursed { "disbursed" } else { "rejected" },
            "review_note":   review_note,
        });

        // customer_id is tenant_id — sufficient for FCM push lookup.
        // carrier_email is stored on WithdrawalRequest since migration 0013.
        (tenant_id, String::new(), carrier_email, vars)
    } else if is_cod_remitted {
        // COD_REMITTED — notify the merchant that their wallet has been credited.
        // Uses merchant_id as the audit key; phone is empty (email-only).
        let merchant_id = data["merchant_id"].as_str()
            .and_then(|s| s.parse::<uuid::Uuid>().ok())
            .unwrap_or(uuid::Uuid::nil());
        let merchant_email = data["merchant_email"].as_str().unwrap_or("").to_owned();

        let net_cents  = data["net_credit_cents"].as_i64().unwrap_or(0);
        let gross_cents = data["gross_cents"].as_i64().unwrap_or(0);
        let fee_cents  = data["platform_fee_cents"].as_i64().unwrap_or(0);
        let cod_count  = data["cod_count"].as_i64().unwrap_or(0);
        let net_php    = format!("{:.2}", net_cents as f64 / 100.0);
        let gross_php  = format!("{:.2}", gross_cents as f64 / 100.0);
        let fee_php    = format!("{:.2}", fee_cents as f64 / 100.0);

        let vars = serde_json::json!({
            "batch_id":        data["batch_id"].as_str().unwrap_or(""),
            "cod_count":       cod_count,
            "gross_amount":    gross_php,
            "platform_fee":    fee_php,
            "net_credit":      net_php,
            "currency":        "PHP",
            "cutoff_date":     data["cutoff_date"].as_str().unwrap_or(""),
            "remitted_at":     data["remitted_at"].as_str().unwrap_or(""),
        });

        (merchant_id, String::new(), merchant_email, vars)
    } else if is_invoice_event {
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
        let is_receipt_resend = event_type == topics::RECEIPT_EMAIL_REQUESTED;

        // Transactional events may not carry customer_id — TaskCompleted/TaskAssigned
        // only carry contact fields. Fall back to shipment_id as the notification
        // audit key so the message is still delivered to the correct phone/email.
        let customer_id = data["customer_id"].as_str()
            .and_then(|s| s.parse::<uuid::Uuid>().ok())
            .or_else(|| data["shipment_id"].as_str().and_then(|s| s.parse::<uuid::Uuid>().ok()));
        let Some(customer_id) = customer_id else {
            warn!(event_type, "Event missing customer_id and shipment_id — skipping notification");
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
        // COD_COLLECTED events (CodReconciled payload) use "amount_cents".
        // All other events (TaskCompleted, ShipmentCreated) use "cod_amount_cents".
        let cod_cents = if event_type == topics::COD_COLLECTED {
            data["amount_cents"].as_i64().unwrap_or(0)
        } else {
            data["cod_amount_cents"].as_i64().unwrap_or(0)
        };
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
            "otp_code" => (
                Some("Your CargoMarket verification code".to_owned()),
                "Your one-time verification code is:\n\n\
                 {{otp_code}}\n\n\
                 This code expires in 5 minutes. Do not share it with anyone.\n\n\
                 — CargoMarket".to_owned(),
            ),
            "merchant_welcome" => (
                Some(format!("Welcome to CargoMarket, {}!", vars["business_name"].as_str().unwrap_or("Merchant"))),
                "Hi there,\n\n\
                 Your merchant account for {{business_name}} is now active on CargoMarket.\n\n\
                 You can start booking shipments, managing customers, and running campaigns from the \
                 Merchant Portal:\n\
                 {{merchant_portal_url}}\n\n\
                 If you have any questions, our support team is here to help.\n\n\
                 — The CargoMarket Team".to_owned(),
            ),
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
            "cod_remitted" => (
                Some(format!("COD Remittance — PHP {} credited to your wallet", vars["net_credit"].as_str().unwrap_or(""))),
                "Dear Merchant,\n\n\
                 Your Cash-on-Delivery remittance has been processed and your wallet has been credited.\n\n\
                 Batch Reference: {{batch_id}}\n\
                 Shipments:       {{cod_count}}\n\
                 Gross COD:       {{currency}} {{gross_amount}}\n\
                 Platform Fee:    {{currency}} {{platform_fee}}\n\
                 Net Credit:      {{currency}} {{net_credit}}\n\
                 Cutoff Date:     {{cutoff_date}}\n\
                 Credited At:     {{remitted_at}}\n\n\
                 You can view your wallet balance and transaction history in the CargoMarket Merchant Portal.\n\n\
                 — CargoMarket Billing".to_owned(),
            ),
            "withdrawal_disbursed" => (
                Some(format!("Withdrawal of PHP {} Disbursed", vars["amount"].as_str().unwrap_or(""))),
                "Your withdrawal request has been approved and the funds have been transferred.\n\n\
                 Amount:    {{currency}} {{amount}}\n\
                 Reference: {{withdrawal_id}}\n\n\
                 Please allow 1–3 business days for the transfer to appear in your bank account.\n\n\
                 You can view your full payout history in the CargoMarket Partner Portal.\n\n\
                 — CargoMarket Finance".to_owned(),
            ),
            "withdrawal_rejected" => (
                Some("Withdrawal Request Not Approved".to_owned()),
                "Your withdrawal request could not be approved at this time.\n\n\
                 Amount:    {{currency}} {{amount}}\n\
                 Reference: {{withdrawal_id}}\n\
                 Reason:    {{review_note}}\n\n\
                 If you have questions, please contact our partner support team.\n\
                 You can submit a new request from the CargoMarket Partner Portal.\n\n\
                 — CargoMarket Finance".to_owned(),
            ),
            "campaign_message" => (
                data.get("subject").and_then(|v| v.as_str()).map(|s| s.to_owned()),
                data.get("body").and_then(|v| v.as_str())
                    .unwrap_or("Hi {{customer_name}}, we have an update for you!")
                    .to_owned(),
            ),
            // ── Cross-border hub milestones (fanned out per recipient) ──
            "shipment_at_port" => (
                Some("Arrived at destination port".to_owned()),
                "Hi {{customer_name}},\n\n\
                 Good news — your shipment {{tracking_number}} has arrived at the destination port and is now awaiting customs processing.\n\n\
                 We'll let you know as soon as it clears. Track it here: {{tracking_url}}\n\n\
                 — CargoMarket".to_owned(),
            ),
            "shipment_customs_hold" => (
                Some("Shipment held at customs".to_owned()),
                "Hi {{customer_name}},\n\n\
                 Your shipment {{tracking_number}} is currently held by customs. This step can take a little longer than usual.\n\n\
                 No action is needed from you right now — we're handling it and will notify you the moment it clears.\n\
                 Track it: {{tracking_url}}\n\n\
                 — CargoMarket".to_owned(),
            ),
            "shipment_customs_cleared" => (
                Some("Cleared customs — on the way".to_owned()),
                "Hi {{customer_name}},\n\n\
                 Your shipment {{tracking_number}} has cleared customs and is now moving to final delivery.\n\n\
                 Track its journey: {{tracking_url}}\n\n\
                 — CargoMarket".to_owned(),
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

// ---------------------------------------------------------------------------
// Campaign fan-out handler
// ---------------------------------------------------------------------------

/// Handle a `CAMPAIGN_TRIGGERED` event by fanning out to all embedded recipients.
///
/// For each recipient:
///   1. Check suppression (open support ticket) — skip if suppressed.
///   2. Insert a `campaign_sends` row (status = queued).
///   3. Dispatch the notification on the campaign's channel.
///   4. Update the row to sent / failed.
///
/// After all sends, publish `CAMPAIGN_COMPLETED` so the marketing service can
/// flip the campaign status to Completed and record the final counters.
pub async fn handle_campaign_triggered(
    payload:              &serde_json::Value,
    db:                   &NotificationDb,
    notification_service: &NotificationService,
    suppression_cache:    &SuppressionCache,
    publisher:            &dyn EngagementPublisher,
) {
    use crate::domain::entities::template::{NotificationChannel, NotificationTemplate};

    // Payload is flat (no data: wrapper) — the marketing service publishes directly.
    let data = payload.get("data").unwrap_or(payload);

    let Some(campaign_id_str) = data["campaign_id"].as_str() else {
        warn!("CAMPAIGN_TRIGGERED missing campaign_id — skipping");
        return;
    };
    let Ok(campaign_id) = campaign_id_str.parse::<uuid::Uuid>() else {
        warn!(campaign_id = campaign_id_str, "CAMPAIGN_TRIGGERED invalid campaign_id UUID");
        return;
    };

    let tenant_id = data["tenant_id"].as_str()
        .or_else(|| payload["tenant_id"].as_str())
        .and_then(|s| s.parse::<uuid::Uuid>().ok());
    let Some(tenant_id) = tenant_id else {
        warn!(campaign_id = %campaign_id, "CAMPAIGN_TRIGGERED missing tenant_id — skipping");
        return;
    };

    // Campaign channel — send only on this channel, not all four.
    let channel_str = match data["channel"].as_str() {
        Some(c) => c,
        None => {
            warn!(campaign_id = %campaign_id, "CAMPAIGN_TRIGGERED missing channel field — skipping");
            return;
        }
    };
    let notif_channel = match channel_str {
        "whatsapp"  => NotificationChannel::WhatsApp,
        "sms"       => NotificationChannel::Sms,
        "email"     => NotificationChannel::Email,
        "push"      => NotificationChannel::Push,
        "messenger" => NotificationChannel::Messenger,
        "telegram"  => NotificationChannel::Telegram,
        "x"         => NotificationChannel::X,
        "viber"     => NotificationChannel::Viber,
        "wechat"    => NotificationChannel::WeChat,
        "line"      => NotificationChannel::Line,
        "slack"     => NotificationChannel::Slack,
        other => {
            warn!(campaign_id = %campaign_id, channel = other, "CAMPAIGN_TRIGGERED unknown channel");
            return;
        }
    };

    // Body / subject resolution — two-tier:
    //   1. Inline: variables.body is set, OR template_id starts with "inline_"
    //      → use variables.body directly (backwards-compat with legacy payloads).
    //   2. Registry: template_id is a valid UUID
    //      → look up engagement.templates; fallback to default string if not found.
    let template_id_str = data["template_id"].as_str().unwrap_or("");
    let inline_body = data["variables"]["body"].as_str()
        .or_else(|| data["body"].as_str());

    let (body, subject) = if inline_body.is_some() || template_id_str.starts_with("inline_") {
        // Inline path — merchant typed the body directly in the campaign builder.
        let b = inline_body
            .unwrap_or("Hi {{customer_name}}, we have an update for you!")
            .to_owned();
        let s = data["subject"].as_str()
            .or_else(|| data["variables"]["subject"].as_str())
            .map(str::to_owned);
        (b, s)
    } else if let Ok(tmpl_uuid) = template_id_str.parse::<uuid::Uuid>() {
        // Registry path — look up the template row from engagement.templates.
        match db.find_template_by_id(tmpl_uuid, tenant_id).await {
            Ok(Some(tmpl)) => {
                info!(
                    campaign_id = %campaign_id,
                    template_id = %tmpl_uuid,
                    "Campaign using registered template"
                );
                (tmpl.body.clone(), tmpl.subject.clone())
            }
            Ok(None) => {
                warn!(
                    campaign_id = %campaign_id,
                    template_id = %tmpl_uuid,
                    "Template not found in registry — using fallback body"
                );
                ("Hi {{customer_name}}, we have an update for you!".to_owned(), None)
            }
            Err(e) => {
                warn!(
                    campaign_id = %campaign_id,
                    template_id = %tmpl_uuid,
                    err = %e,
                    "Template registry lookup failed — using fallback body"
                );
                ("Hi {{customer_name}}, we have an update for you!".to_owned(), None)
            }
        }
    } else {
        // template_id is neither "inline_*" nor a UUID (e.g. a slug) — use fallback.
        warn!(
            campaign_id = %campaign_id,
            template_id = template_id_str,
            "Unrecognised template_id format — using fallback body"
        );
        ("Hi {{customer_name}}, we have an update for you!".to_owned(), None)
    };

    // Optional automation rule context — present when triggered by rules engine.
    let triggered_by_rule_id: Option<uuid::Uuid> = data["triggered_by_rule_id"]
        .as_str()
        .and_then(|s| s.parse().ok());
    let triggered_by_rule_name: Option<String> = data["triggered_by_rule_name"]
        .as_str()
        .map(str::to_owned);

    let Some(recipients) = data["recipients"].as_array() else {
        warn!(campaign_id = %campaign_id, "CAMPAIGN_TRIGGERED has no recipients array — nothing to send");
        return;
    };

    if recipients.is_empty() {
        warn!(campaign_id = %campaign_id, "CAMPAIGN_TRIGGERED recipients list is empty");
        return;
    }

    // Upsert a row into engagement.campaigns before inserting campaign_sends rows.
    // This satisfies the engagement read projection and avoids any residual FK issue.
    let campaign_name = data["name"].as_str().unwrap_or("Campaign");
    let created_by_str = data["created_by"].as_str()
        .and_then(|s| s.parse::<uuid::Uuid>().ok())
        .unwrap_or(uuid::Uuid::nil());
    if let Err(e) = db.upsert_campaign_from_trigger(
        campaign_id,
        tenant_id,
        campaign_name,
        channel_str,
        created_by_str,
        recipients.len() as i64,
    ).await {
        warn!(campaign_id = %campaign_id, err = %e, "Failed to upsert engagement.campaigns row — proceeding with fan-out");
    }

    info!(
        campaign_id = %campaign_id,
        channel = channel_str,
        recipient_count = recipients.len(),
        "Campaign fan-out starting"
    );

    let mut total_sent:   u64 = 0;
    let mut total_failed: u64 = 0;

    for r in recipients {
        let customer_id = r["customer_id"].as_str()
            .and_then(|s| s.parse::<uuid::Uuid>().ok())
            .unwrap_or(uuid::Uuid::nil());

        // Skip if the customer has an open support ticket.
        if customer_id != uuid::Uuid::nil()
            && suppression_cache.is_suppressed(customer_id).await
        {
            info!(
                campaign_id = %campaign_id,
                customer_id = %customer_id,
                "Campaign send suppressed — customer has open support ticket"
            );
            continue;
        }

        // Resolve the channel-specific address.
        let recipient = match channel_str {
            "whatsapp" | "sms" => r["phone"].as_str().unwrap_or("").to_owned(),
            "email"            => r["email"].as_str().unwrap_or("").to_owned(),
            "push"             => customer_id.to_string(),
            // Social channels use platform_id (Facebook PSID, Telegram chat_id,
            // Slack user_id, etc.). Falls back to customer_id for stub dispatch.
            "messenger" | "telegram" | "x" | "viber" | "wechat" | "line" | "slack" => {
                r["platform_id"].as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| customer_id.to_string())
            }
            _ => continue,
        };

        if recipient.is_empty() && channel_str != "push" {
            warn!(
                campaign_id = %campaign_id,
                channel = channel_str,
                "Recipient missing contact for channel — skipping"
            );
            continue;
        }

        // Persist a campaign_sends row before dispatching.
        let send_id = match db.insert_campaign_send(
            campaign_id,
            customer_id,
            channel_str,
            triggered_by_rule_id,
            triggered_by_rule_name.as_deref(),
        ).await {
            Ok(id) => id,
            Err(e) => {
                error!(campaign_id = %campaign_id, err = %e, "Failed to insert campaign_send row");
                total_failed += 1;
                continue;
            }
        };

        // Build template variables.
        // - {{customer_name}} and {{name}} are always available (fallback "Customer").
        //   Both keys map to the same value so merchants can use either placeholder.
        // - {{phone}} is injected for whatsapp/sms channels when the recipient has a
        //   non-empty phone string, letting templates include the customer's number.
        // The var_names list must match exactly the keys present in vars_map so that
        // render() — which errors on any declared-but-missing variable — stays safe.
        let customer_name = r["name"].as_str().unwrap_or("Customer");
        let mut vars_map = serde_json::json!({
            "customer_name": customer_name,
            "name":          customer_name,
        });
        let mut var_names = vec!["customer_name".to_owned(), "name".to_owned()];

        if (channel_str == "whatsapp" || channel_str == "sms") {
            if let Some(phone_str) = r["phone"].as_str().filter(|s| !s.is_empty()) {
                vars_map["phone"] = serde_json::Value::String(phone_str.to_owned());
                var_names.push("phone".to_owned());
            }
        }
        let vars = vars_map;

        let template = NotificationTemplate {
            id:          uuid::Uuid::new_v4(),
            tenant_id:   Some(tenant_id),
            template_id: "campaign_message".to_owned(),
            channel:     notif_channel,
            language:    "en".into(),
            subject:     subject.clone(),
            body:        body.clone(),
            variables:   var_names,
            is_active:   true,
        };

        let mut notification = match NotificationService::build_from_template(
            &template,
            tenant_id,
            customer_id,
            recipient.clone(),
            &vars,
            NotificationPriority::Low,
        ) {
            Ok(n)  => n,
            Err(e) => {
                error!(campaign_id = %campaign_id, err = %e, "Failed to build campaign notification");
                db.update_campaign_send_status(send_id, "failed", None, Some(e.to_string()))
                    .await.ok();
                total_failed += 1;
                continue;
            }
        };

        // For push campaigns, forward any deep_link (or other metadata) from the
        // campaign's template variables so the push adapter can attach it to the
        // Expo notification's `data` field and the customer app can deep-navigate.
        if channel_str == "push" {
            let campaign_vars = &data["variables"];
            if campaign_vars.as_object().is_some_and(|o| !o.is_empty()) {
                notification.extra_data = Some(campaign_vars.clone());
            }
        }

        match notification_service.dispatch(&mut notification).await {
            Ok(_) => {
                db.update_campaign_send_status(
                    send_id,
                    "sent",
                    notification.provider_message_id.clone(),
                    None,
                )
                .await.ok();
                total_sent += 1;
                info!(
                    campaign_id = %campaign_id,
                    channel = channel_str,
                    recipient = %recipient,
                    "Campaign send dispatched"
                );
            }
            Err(e) => {
                db.update_campaign_send_status(send_id, "failed", None, Some(e.to_string()))
                    .await.ok();
                total_failed += 1;
                error!(
                    campaign_id = %campaign_id,
                    channel = channel_str,
                    err = %e,
                    "Campaign send failed"
                );
            }
        }
    }

    // Count rows that reached 'sent' status as the best available proxy for
    // "delivered" — delivery receipt webhooks would update these later when wired.
    let total_delivered: u64 = db
        .count_campaign_sends_by_status(campaign_id, "sent")
        .await
        .unwrap_or_else(|e| {
            warn!(campaign_id = %campaign_id, err = %e, "Failed to count sent rows — falling back to total_sent for total_delivered");
            total_sent as i64
        }) as u64;

    // Update the engagement-side campaign record to 'completed'.
    db.complete_engagement_campaign(
        campaign_id,
        total_sent as i64,
        total_delivered as i64,
        total_failed as i64,
    )
    .await
    .ok();

    info!(
        campaign_id = %campaign_id,
        total_sent,
        total_delivered,
        total_failed,
        "Campaign fan-out complete — publishing CAMPAIGN_COMPLETED"
    );

    // Signal the marketing service to flip status → Completed and record counters.
    let completed_payload = serde_json::json!({
        "campaign_id":     campaign_id,
        "tenant_id":       tenant_id,
        "total_sent":      total_sent,
        "total_delivered": total_delivered,
        "total_failed":    total_failed,
    });
    if let Err(e) = publisher.publish(
        topics::CAMPAIGN_COMPLETED,
        &campaign_id.to_string(),
        &serde_json::to_vec(&completed_payload).unwrap_or_default(),
    ).await {
        error!(campaign_id = %campaign_id, err = %e, "Failed to publish CAMPAIGN_COMPLETED");
    }
}
