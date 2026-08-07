//! Consumes `hub.shipment.carrier_booking_requested` from hub-ops (Carrier
//! routing) and from dispatch (Auto fallback). Creates a carrier SLA allocation
//! record for the shipment via `CarrierService::create_sla_record`, which emits
//! `CarrierAllocated` downstream.
//!
//! The event carries the routing-resolved `carrier_id` plus enrichment supplied
//! by the deconsolidate request (`service_level`, `sla_hours`, `total_cost_cents`).
//! When no `carrier_id` is configured for the zone we skip — a rate-shop requires
//! parcel weight/dimensions that this event does not carry.

use std::sync::Arc;

use logisticos_events::{envelope::Event, payloads::HubCarrierBookingRequested, topics};
use rdkafka::{
    config::ClientConfig,
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::Message,
};
use tokio::sync::watch;

use crate::application::services::CarrierService;
use crate::domain::entities::SlaRecord;

/// SLA window in hours, defaulting to 48 when the event omits it.
fn effective_sla_hours(sla_hours: i64) -> i64 {
    if sla_hours > 0 { sla_hours } else { 48 }
}

pub async fn start_hub_carrier_consumer(
    brokers: &str,
    group_id: &str,
    carrier_svc: Arc<CarrierService>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", format!("{}-hub-carrier", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::HUB_CARRIER_BOOKING_REQUESTED])?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Hub-carrier consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle(payload, &carrier_svc).await {
                                tracing::warn!(err = %e, "hub-carrier consumer: handler error (skipping)");
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "hub-carrier consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle(payload: &[u8], carrier_svc: &CarrierService) -> anyhow::Result<()> {
    let event: Event<HubCarrierBookingRequested> = serde_json::from_slice(payload)?;
    let d = event.data;

    let Some(carrier_id) = d.carrier_id else {
        tracing::warn!(
            shipment_id = %d.shipment_id,
            zone        = %d.destination_zone,
            "hub-carrier booking skipped: no carrier_id configured for zone (rate-shop needs parcel detail)"
        );
        return Ok(());
    };

    let service_level = if d.service_level.is_empty() {
        "standard".to_string()
    } else {
        d.service_level.clone()
    };
    let promised_by = chrono::Utc::now() + chrono::Duration::hours(effective_sla_hours(d.sla_hours));

    let record = SlaRecord::new(
        d.tenant_id,
        carrier_id,
        d.shipment_id,
        d.destination_zone.clone(),
        service_level,
        promised_by,
    );

    carrier_svc
        .create_sla_record(record, d.total_cost_cents, "hub_auto".to_string())
        .await
        .map_err(|e| anyhow::anyhow!("create_sla_record failed: {e}"))?;

    tracing::info!(
        shipment_id = %d.shipment_id,
        carrier_id  = %carrier_id,
        zone        = %d.destination_zone,
        "Hub carrier booking → SLA allocation created"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::effective_sla_hours;

    #[test]
    fn defaults_to_48h_when_unset_or_negative() {
        assert_eq!(effective_sla_hours(0), 48);
        assert_eq!(effective_sla_hours(-5), 48);
    }

    #[test]
    fn uses_provided_hours() {
        assert_eq!(effective_sla_hours(24), 24);
    }
}
