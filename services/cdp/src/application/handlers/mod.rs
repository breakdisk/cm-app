/// Kafka event handlers — consume domain events and project them into the CDP profile store.
///
/// Subscriptions:
///   logisticos.order.shipment.created       → ShipmentCreated      → upsert profile, record event
///   logisticos.driver.delivery.completed    → DeliveryCompleted    → record event, update counters
///   logisticos.driver.delivery.failed       → DeliveryFailed       → record event, update counters
///   logisticos.payments.cod.collected       → CodCollected         → record event, update COD total
///   logisticos.support.ticket.opened        → SupportTicketOpened  → record event on profile
///   logisticos.support.ticket.closed        → SupportTicketClosed  → record event on profile
use std::sync::Arc;
use chrono::{DateTime, Utc};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::BorrowedMessage,
    Message,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_events::topics;
use logisticos_types::TenantId;

use crate::application::services::{ProfileService, RecordEventCommand, UpsertProfileCommand};
use crate::domain::entities::{EventType, ProfileType};
use crate::domain::repositories::ProfileFilter;

// ---------------------------------------------------------------------------
// Inbound payload shapes (mirrors libs/events/src/payloads.rs)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ShipmentCreatedPayload {
    merchant_id:          Uuid,
    customer_id:          Uuid,
    customer_name:        String,
    customer_phone:       String,
    #[serde(default)]
    customer_email:       String,
    destination_address:  String,
    // Sender identity — populated when the merchant portal sends sender fields.
    #[serde(default)]
    sender_name:          Option<String>,
    #[serde(default)]
    sender_phone:         Option<String>,
    #[serde(default)]
    sender_email:         Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeliveryCompletedPayload {
    shipment_id:  Uuid,
    #[serde(default)]
    customer_id:  Option<Uuid>,
    driver_id:    Uuid,
    delivered_at: String,
}

#[derive(Debug, Deserialize)]
struct DeliveryFailedPayload {
    shipment_id:    Uuid,
    #[serde(default)]
    customer_id:    Option<Uuid>,
    reason:         String,
    attempted_at:   String,
    attempt_number: u32,
}

#[derive(Debug, Deserialize)]
struct CodCollectedPayload {
    shipment_id:    Uuid,
    #[serde(default)]
    customer_id:    Option<Uuid>,
    amount_cents:   i64,
    collected_at:   String,
}

#[derive(Debug, Deserialize)]
struct SupportTicketPayload {
    ticket_id:   Uuid,
    customer_id: Uuid,
    #[serde(default)]
    opened_at:   Option<String>,
    #[serde(default)]
    closed_at:   Option<String>,
}

// ---------------------------------------------------------------------------
// Handler entry point — runs as a long-lived Tokio task.
// ---------------------------------------------------------------------------

pub async fn run_consumer(consumer: Arc<StreamConsumer>, svc: Arc<ProfileService>) {
    consumer
        .subscribe(&[
            topics::SHIPMENT_CREATED,
            topics::DELIVERY_COMPLETED,
            topics::DELIVERY_FAILED,
            topics::COD_COLLECTED,
            topics::SUPPORT_TICKET_OPENED,
            topics::SUPPORT_TICKET_CLOSED,
        ])
        .expect("CDP consumer subscription failed");

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Err(e) = handle_message(&msg, &svc).await {
                    tracing::warn!(
                        topic = msg.topic(),
                        offset = msg.offset(),
                        err = %e,
                        "CDP event handler error"
                    );
                }
                // At-least-once: commit after processing (even on error to avoid poison pill).
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "Kafka recv error in CDP consumer");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle_message(
    msg: &BorrowedMessage<'_>,
    svc: &Arc<ProfileService>,
) -> anyhow::Result<()> {
    let payload = match msg.payload() {
        Some(p) => p,
        None => return Ok(()), // tombstone / null payload — skip
    };

    // Parse the CloudEvents-style envelope: { id, source, event_type, time, tenant_id, data: T }.
    // Publishers do NOT set rdkafka headers — tenant_id lives inside the JSON envelope.
    let raw: serde_json::Value = serde_json::from_slice(payload)?;
    let tenant_id = raw["tenant_id"]
        .as_str()
        .and_then(|s| s.parse::<Uuid>().ok())
        .map(TenantId::from_uuid)
        .ok_or_else(|| anyhow::anyhow!("Missing tenant_id in event envelope on topic {}", msg.topic()))?;
    // Inner payload is always under the "data" key; fall back to root for legacy events.
    let data_val = raw.get("data").unwrap_or(&raw);

    match msg.topic() {
        topics::SHIPMENT_CREATED => {
            let data: ShipmentCreatedPayload = serde_json::from_value(data_val.clone())?;

            // Dedup receiver by phone so the same consignee across multiple
            // bookings converges to one profile rather than creating a new
            // UUID-keyed profile per shipment.
            let receiver_external_id = if !data.customer_phone.is_empty() {
                let existing = svc
                    .list(
                        &tenant_id,
                        ProfileFilter {
                            phone: Some(data.customer_phone.clone()),
                            profile_type: Some("receiver".to_string()),
                            limit: 1,
                            ..Default::default()
                        },
                    )
                    .await?;
                existing
                    .first()
                    .map(|p| p.external_customer_id)
                    .unwrap_or(data.customer_id)
            } else {
                data.customer_id
            };

            // 1. Upsert receiver profile.
            svc.upsert(
                &tenant_id,
                UpsertProfileCommand {
                    external_customer_id: receiver_external_id,
                    name:  Some(data.customer_name.clone()).filter(|s| !s.is_empty()),
                    email: Some(data.customer_email.clone()).filter(|s| !s.is_empty()),
                    phone: Some(data.customer_phone.clone()).filter(|s| !s.is_empty()),
                    profile_type: Some(ProfileType::Receiver),
                },
            )
            .await?;

            // 2. Record the shipment booking event on the deduplicated receiver profile.
            svc.record_event(RecordEventCommand {
                tenant_id: tenant_id.clone(),
                external_customer_id: receiver_external_id,
                event_type: EventType::ShipmentCreated,
                shipment_id: None,
                metadata: serde_json::json!({
                    "merchant_id": data.merchant_id,
                    "destination_address": data.destination_address,
                }),
                occurred_at: Utc::now(),
            })
            .await?;

            // 3. Upsert sender profile when sender identity is present.
            //    Dedup by phone: reuse the existing Sender profile if one exists
            //    with the same phone under this tenant, otherwise create a new one.
            if let Some(ref sender_phone) = data.sender_phone {
                if !sender_phone.is_empty() {
                    let existing = svc
                        .list(
                            &tenant_id,
                            ProfileFilter {
                                phone: Some(sender_phone.clone()),
                                profile_type: Some("sender".to_string()),
                                limit: 1,
                                ..Default::default()
                            },
                        )
                        .await?;

                    let sender_external_id = existing
                        .first()
                        .map(|s| s.external_customer_id)
                        .unwrap_or_else(Uuid::new_v4);

                    svc.upsert(
                        &tenant_id,
                        UpsertProfileCommand {
                            external_customer_id: sender_external_id,
                            name:  data.sender_name.clone().filter(|s| !s.is_empty()),
                            email: data.sender_email.clone().filter(|s| !s.is_empty()),
                            phone: Some(sender_phone.clone()),
                            profile_type: Some(ProfileType::Sender),
                        },
                    )
                    .await?;
                }
            }
        }
        topics::DELIVERY_COMPLETED => {
            let data: DeliveryCompletedPayload = serde_json::from_value(data_val.clone())?;
            let occurred_at = data
                .delivered_at
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            let external_customer_id = data.customer_id.unwrap_or(data.shipment_id);
            svc.record_event(RecordEventCommand {
                tenant_id,
                external_customer_id,
                event_type: EventType::DeliveryCompleted,
                shipment_id: Some(data.shipment_id),
                metadata: serde_json::json!({
                    "driver_id": data.driver_id,
                    "delivered_at": data.delivered_at,
                }),
                occurred_at,
            })
            .await?;
        }
        topics::DELIVERY_FAILED => {
            let data: DeliveryFailedPayload = serde_json::from_value(data_val.clone())?;
            let occurred_at = data
                .attempted_at
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            let external_customer_id = data.customer_id.unwrap_or(data.shipment_id);
            svc.record_event(RecordEventCommand {
                tenant_id,
                external_customer_id,
                event_type: EventType::DeliveryFailed,
                shipment_id: Some(data.shipment_id),
                metadata: serde_json::json!({
                    "reason": data.reason,
                    "attempt_number": data.attempt_number,
                }),
                occurred_at,
            })
            .await?;
        }
        topics::COD_COLLECTED => {
            let data: CodCollectedPayload = serde_json::from_value(data_val.clone())?;
            let occurred_at = data
                .collected_at
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            let external_customer_id = data.customer_id.unwrap_or(data.shipment_id);
            svc.record_event(RecordEventCommand {
                tenant_id,
                external_customer_id,
                event_type: EventType::CodPaid,
                shipment_id: Some(data.shipment_id),
                metadata: serde_json::json!({
                    "amount_cents": data.amount_cents,
                }),
                occurred_at,
            })
            .await?;
        }
        topics::SUPPORT_TICKET_OPENED => {
            let data: SupportTicketPayload = serde_json::from_value(data_val.clone())?;
            let occurred_at = data.opened_at
                .as_deref()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                .unwrap_or_else(Utc::now);
            svc.record_event(RecordEventCommand {
                tenant_id,
                external_customer_id: data.customer_id,
                event_type: EventType::SupportTicketOpened,
                shipment_id: None,
                metadata: serde_json::json!({ "ticket_id": data.ticket_id }),
                occurred_at,
            })
            .await?;
        }
        topics::SUPPORT_TICKET_CLOSED => {
            let data: SupportTicketPayload = serde_json::from_value(data_val.clone())?;
            let occurred_at = data.closed_at
                .as_deref()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                .unwrap_or_else(Utc::now);
            svc.record_event(RecordEventCommand {
                tenant_id,
                external_customer_id: data.customer_id,
                event_type: EventType::SupportTicketClosed,
                shipment_id: None,
                metadata: serde_json::json!({ "ticket_id": data.ticket_id }),
                occurred_at,
            })
            .await?;
        }
        other => {
            tracing::debug!(topic = other, "CDP consumer: unhandled topic");
        }
    }

    Ok(())
}

