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
                let fallback = Event::new(
                    "dispatch",
                    topics::HUB_CARRIER_BOOKING_REQUESTED,
                    d.tenant_id,
                    HubCarrierBookingRequested {
                        shipment_id:      d.shipment_id,
                        tenant_id:        d.tenant_id,
                        hub_id:           d.hub_id,
                        destination_zone: d.destination_zone.clone(),
                        carrier_id:       None,
                        service_level:    d.service_level.clone(),
                        sla_hours:        d.sla_hours,
                        total_cost_cents: d.total_cost_cents,
                        requested_at:     chrono::Utc::now().to_rfc3339(),
                    },
                );
                kafka
                    .publish_event(topics::HUB_CARRIER_BOOKING_REQUESTED, &fallback)
                    .await?;
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
}
