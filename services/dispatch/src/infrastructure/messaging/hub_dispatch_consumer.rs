//! Consumes `hub.shipment.dispatch_requested` from hub-ops at deconsolidation.
//!
//! The shipment already has a `dispatch_queue` row from booking (SHIPMENT_CREATED).
//! At destination-hub break-bulk, hub-ops requests last-mile dispatch per shipment
//! based on the HubRoutingConfig (OwnDriver or Auto). We attempt an own-driver
//! assignment via `DriverAssignmentService::quick_dispatch`.
//!
//! Auto fallback: when the routing rule is Auto (`auto_fallback_window_mins > 0`)
//! and the own-driver assignment cannot be fulfilled, we escalate by publishing
//! `hub.shipment.carrier_booking_requested` so the carrier service books a 3PL.
//!
//! NOTE: the fallback currently fires on immediate assignment failure rather than
//! after a wall-clock timer. A delayed re-check (waiting the full window before
//! escalating) is a future enhancement; escalating on immediate failure preserves
//! the Auto intent without a scheduler.

use logisticos_events::{
    envelope::Event,
    payloads::{HubCarrierBookingRequested, HubDispatchRequested},
    producer::KafkaProducer,
    topics,
};
use logisticos_types::TenantId;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use std::sync::Arc;
use tokio::sync::watch;

use crate::application::commands::QuickDispatchCommand;
use crate::application::services::DriverAssignmentService;

/// Whether a failed own-driver dispatch should escalate to a carrier booking.
/// Auto routing (window > 0) escalates; OwnDriver (window == 0) does not.
fn should_fallback_to_carrier(auto_fallback_window_mins: i32) -> bool {
    auto_fallback_window_mins > 0
}

/// Build the carrier-booking escalation for a failed Auto dispatch.
/// Returns `None` when hub-ops supplied no configured carrier — nothing can be booked.
fn fallback_event(d: &HubDispatchRequested) -> Option<HubCarrierBookingRequested> {
    let carrier_id = d.fallback_carrier_id?;
    Some(HubCarrierBookingRequested {
        shipment_id:      d.shipment_id,
        tenant_id:        d.tenant_id,
        hub_id:           d.hub_id,
        destination_zone: d.destination_zone.clone(),
        carrier_id:       Some(carrier_id),
        service_level:    d.service_level.clone(),
        sla_hours:        d.sla_hours,
        total_cost_cents: d.total_cost_cents,
        requested_at:     chrono::Utc::now().to_rfc3339(),
    })
}

pub async fn start_hub_dispatch_consumer(
    brokers: &str,
    group_id: &str,
    dispatch_service: Arc<DriverAssignmentService>,
    kafka: Arc<KafkaProducer>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", format!("{}-hub-dispatch", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::HUB_DISPATCH_REQUESTED])?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Hub-dispatch consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle(payload, &dispatch_service, &kafka).await {
                                tracing::warn!(err = %e, "hub-dispatch consumer: handler error (skipping)");
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "hub-dispatch consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle(
    payload: &[u8],
    dispatch_service: &DriverAssignmentService,
    kafka: &KafkaProducer,
) -> anyhow::Result<()> {
    let event: Event<HubDispatchRequested> = serde_json::from_slice(payload)?;
    let d = event.data;

    let cmd = QuickDispatchCommand {
        shipment_id:         d.shipment_id,
        preferred_driver_id: None,
    };

    match dispatch_service.quick_dispatch(TenantId::from_uuid(d.tenant_id), cmd).await {
        Ok(assignment) => {
            tracing::info!(
                shipment_id = %d.shipment_id,
                driver_id   = %assignment.driver_id.inner(),
                zone        = %d.destination_zone,
                "Hub last-mile dispatched to own driver"
            );
        }
        Err(e) => {
            if should_fallback_to_carrier(d.auto_fallback_window_mins) {
                tracing::warn!(
                    shipment_id = %d.shipment_id,
                    err         = %e,
                    "Own-driver dispatch failed under Auto routing — escalating to carrier"
                );
                match fallback_event(&d) {
                    Some(payload) => {
                        let fallback = Event::new(
                            "dispatch",
                            topics::HUB_CARRIER_BOOKING_REQUESTED,
                            d.tenant_id,
                            payload,
                        );
                        kafka
                            .publish_event(topics::HUB_CARRIER_BOOKING_REQUESTED, &fallback)
                            .await?;
                    }
                    None => {
                        tracing::error!(
                            hub_id      = %d.hub_id,
                            shipment_id = %d.shipment_id,
                            zone        = %d.destination_zone,
                            "Auto routing has no carrier configured for this hub/zone — cannot fall back; shipment remains queued for manual dispatch"
                        );
                    }
                }
            } else {
                tracing::warn!(
                    shipment_id = %d.shipment_id,
                    err         = %e,
                    "Own-driver dispatch failed (OwnDriver routing) — shipment remains queued for manual dispatch"
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_routing_escalates_to_carrier() {
        assert!(should_fallback_to_carrier(30));
    }

    #[test]
    fn own_driver_routing_does_not_escalate() {
        assert!(!should_fallback_to_carrier(0));
    }

    fn request(fallback_carrier_id: Option<uuid::Uuid>) -> HubDispatchRequested {
        HubDispatchRequested {
            shipment_id:               uuid::Uuid::new_v4(),
            tenant_id:                 uuid::Uuid::new_v4(),
            hub_id:                    uuid::Uuid::new_v4(),
            destination_zone:          "Z1".into(),
            auto_fallback_window_mins: 30,
            service_level:             "standard".into(),
            sla_hours:                 48,
            total_cost_cents:          1500,
            fallback_carrier_id,
            requested_at:              "2026-11-23T00:00:00Z".into(),
        }
    }

    #[test]
    fn auto_fallback_carries_the_configured_carrier() {
        let carrier = uuid::Uuid::new_v4();
        let d = request(Some(carrier));
        let ev = fallback_event(&d).expect("carrier configured");
        assert_eq!(ev.carrier_id, Some(carrier));
        assert_eq!(ev.shipment_id, d.shipment_id);
        assert_eq!(ev.hub_id, d.hub_id);
        assert_eq!(ev.sla_hours, 48);
        assert_eq!(ev.total_cost_cents, 1500);
    }

    #[test]
    fn auto_without_a_carrier_stays_queued_and_logs() {
        assert!(fallback_event(&request(None)).is_none());
    }

    #[test]
    fn old_payload_without_the_field_decodes() {
        let json = r#"{
            "shipment_id":"00000000-0000-0000-0000-000000000001",
            "tenant_id":"00000000-0000-0000-0000-000000000002",
            "hub_id":"00000000-0000-0000-0000-000000000003",
            "destination_zone":"Z1",
            "auto_fallback_window_mins":30,
            "requested_at":"2026-11-23T00:00:00Z"
        }"#;
        let d: HubDispatchRequested = serde_json::from_str(json).expect("old payload decodes");
        assert_eq!(d.fallback_carrier_id, None);
    }
}
