use std::sync::Arc;

use chrono::DateTime;
use logisticos_events::{
    envelope::Event,
    payloads::{
        CarrierAllocated, CarrierOnboarded, CarrierStatusChanged,
        CarrierTrackingEvent, DeliveryCompleted, DeliveryFailed,
    },
    producer::KafkaProducer,
    topics::{
        CARRIER_ALLOCATED, CARRIER_ONBOARDED, CARRIER_STATUS_CHANGED,
        CARRIER_TRACKING_EVENT, DELIVERY_COMPLETED, DELIVERY_FAILED,
    },
};
use rdkafka::{
    config::ClientConfig,
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::Message,
};
use tokio::sync::watch;
use uuid::Uuid;

use crate::{
    domain::{
        entities::CarrierId,
        repositories::{CarrierRepository, SlaRecordRepository},
    },
    infrastructure::{
        adapters::{AdapterRegistry, Address, BookingRequest},
        db::{CarrierBookingRecord, PgCarrierBookingRepository},
        storage::StorageAdapter,
    },
};

// ── Publisher ─────────────────────────────────────────────────────────────────

pub struct CarrierPublisher {
    kafka: Arc<KafkaProducer>,
}

impl CarrierPublisher {
    pub fn new(kafka: Arc<KafkaProducer>) -> Self { Self { kafka } }

    pub async fn carrier_onboarded(
        &self,
        carrier_id: Uuid,
        tenant_id: Uuid,
        name: String,
        code: String,
        contact_email: String,
    ) -> anyhow::Result<()> {
        let payload = CarrierOnboarded { carrier_id, tenant_id, name, code, contact_email };
        let event = Event::new("logisticos/carrier", "carrier.onboarded", tenant_id, payload);
        self.kafka.publish_event(CARRIER_ONBOARDED, &event).await
    }

    pub async fn carrier_status_changed(
        &self,
        carrier_id: Uuid,
        tenant_id: Uuid,
        old_status: String,
        new_status: String,
        reason: String,
    ) -> anyhow::Result<()> {
        let payload = CarrierStatusChanged { carrier_id, tenant_id, old_status, new_status, reason };
        let event = Event::new("logisticos/carrier", "carrier.status_changed", tenant_id, payload);
        self.kafka.publish_event(CARRIER_STATUS_CHANGED, &event).await
    }

    pub async fn carrier_allocated(
        &self,
        tenant_id: Uuid,
        payload: CarrierAllocated,
    ) -> anyhow::Result<()> {
        let event = Event::new("logisticos/carrier", "carrier.allocated", tenant_id, payload);
        self.kafka.publish_event(CARRIER_ALLOCATED, &event).await
    }

    pub async fn carrier_tracking_event(
        &self,
        tenant_id: Uuid,
        payload: CarrierTrackingEvent,
    ) -> anyhow::Result<()> {
        let event = Event::new("logisticos/carrier", "carrier.tracking.event", tenant_id, payload);
        self.kafka.publish_event(CARRIER_TRACKING_EVENT, &event).await
    }
}

// ── Delivery outcome consumer ─────────────────────────────────────────────────

/// Subscribes to `delivery.completed` and `delivery.failed` driver events.
/// On each message:
///   1. Looks up the SLA record by shipment_id to resolve the carrier.
///   2. Updates the SLA record (mark_delivered / mark_failed + save_outcome).
///   3. Calls carrier.record_delivery(on_time) and saves the updated carrier.
pub async fn start_delivery_consumer(
    brokers: &str,
    group_id: &str,
    carrier_repo: Arc<dyn CarrierRepository>,
    sla_repo: Arc<dyn SlaRecordRepository>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .create()?;

    consumer.subscribe(&[DELIVERY_COMPLETED, DELIVERY_FAILED])?;
    tracing::info!("Carrier delivery consumer subscribed to {} / {}", DELIVERY_COMPLETED, DELIVERY_FAILED);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("Carrier delivery consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Err(e) => tracing::warn!("Kafka recv error: {e}"),
                    Ok(msg) => {
                        let topic = msg.topic();
                        match msg.payload_view::<str>() {
                            None => tracing::warn!(topic, "Empty Kafka payload — skipping"),
                            Some(Err(e)) => tracing::warn!(topic, "Non-UTF-8 payload: {e}"),
                            Some(Ok(raw)) => {
                                if topic == DELIVERY_COMPLETED {
                                    if let Err(e) = handle_delivery_completed(
                                        raw, &*carrier_repo, &*sla_repo,
                                    ).await {
                                        tracing::error!("handle_delivery_completed error: {e}");
                                    }
                                } else if topic == DELIVERY_FAILED {
                                    if let Err(e) = handle_delivery_failed(
                                        raw, &*carrier_repo, &*sla_repo,
                                    ).await {
                                        tracing::error!("handle_delivery_failed error: {e}");
                                    }
                                }
                            }
                        }
                        if let Err(e) = consumer.commit_message(&msg, CommitMode::Async) {
                            tracing::error!("Kafka commit error: {e}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_delivery_completed(
    raw: &str,
    carrier_repo: &dyn CarrierRepository,
    sla_repo: &dyn SlaRecordRepository,
) -> anyhow::Result<()> {
    let event: Event<DeliveryCompleted> = serde_json::from_str(raw)?;
    let payload = &event.data;

    let delivered_at = DateTime::parse_from_rfc3339(&payload.delivered_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    let Some(mut sla) = sla_repo.find_by_shipment(payload.shipment_id).await? else {
        tracing::warn!(shipment_id = %payload.shipment_id, "No SLA record for delivered shipment — skipping");
        return Ok(());
    };
    sla.mark_delivered(delivered_at);
    sla_repo.save_outcome(&sla).await?;

    let on_time = sla.on_time.unwrap_or(false);
    update_carrier_metrics(carrier_repo, sla.carrier_id, on_time).await
}

async fn handle_delivery_failed(
    raw: &str,
    carrier_repo: &dyn CarrierRepository,
    sla_repo: &dyn SlaRecordRepository,
) -> anyhow::Result<()> {
    let event: Event<DeliveryFailed> = serde_json::from_str(raw)?;
    let payload = &event.data;

    // Only count as a final failure on the last attempt or explicit failure (no next attempt).
    if payload.next_attempt_scheduled.is_some() {
        tracing::debug!(shipment_id = %payload.shipment_id, "Delivery failed but has next attempt — deferring SLA verdict");
        return Ok(());
    }

    let Some(mut sla) = sla_repo.find_by_shipment(payload.shipment_id).await? else {
        tracing::warn!(shipment_id = %payload.shipment_id, "No SLA record for failed shipment — skipping");
        return Ok(());
    };
    sla.mark_failed(&payload.reason);
    sla_repo.save_outcome(&sla).await?;

    update_carrier_metrics(carrier_repo, sla.carrier_id, false).await
}

async fn update_carrier_metrics(
    carrier_repo: &dyn CarrierRepository,
    carrier_id: Uuid,
    on_time: bool,
) -> anyhow::Result<()> {
    let Some(mut carrier) = carrier_repo
        .find_by_id(&CarrierId::from_uuid(carrier_id))
        .await?
    else {
        tracing::warn!(carrier_id = %carrier_id, "Carrier not found when updating delivery metrics");
        return Ok(());
    };
    carrier.record_delivery(on_time);
    carrier_repo.save(&carrier).await?;
    tracing::debug!(
        carrier_id = %carrier_id,
        on_time,
        total_shipments = carrier.total_shipments,
        grade = ?carrier.performance_grade,
        "Carrier performance updated"
    );
    Ok(())
}

// ── Allocation booking worker (G4) ────────────────────────────────────────────

/// Subscribes to `CARRIER_ALLOCATED` events. When the allocated carrier has a
/// registered 3PL adapter (e.g. DHL), books the shipment via the adapter API,
/// uploads the label to R2/S3, and writes an audit row to `carrier_bookings`.
///
/// This is fire-and-forget relative to the dispatch/SLA-record path — failures
/// are logged and the event is committed so we don't re-process indefinitely.
/// A retry strategy (dead-letter queue) should be added when Kafka is hardened.
pub async fn start_allocation_booking_worker(
    brokers: String,
    group_id: String,
    carrier_repo: Arc<dyn CarrierRepository>,
    adapter_registry: Arc<AdapterRegistry>,
    booking_repo: Arc<PgCarrierBookingRepository>,
    storage: Option<Arc<dyn StorageAdapter>>,
    order_intake_url: String,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", format!("{group_id}-allocation-booking"))
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .create()?;

    consumer.subscribe(&[CARRIER_ALLOCATED])?;
    tracing::info!("Allocation booking worker subscribed to {}", CARRIER_ALLOCATED);

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("Allocation booking worker shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Err(e) => tracing::warn!("Allocation booking worker Kafka recv error: {e}"),
                    Ok(msg) => {
                        if let Some(Ok(raw)) = msg.payload_view::<str>() {
                            if let Err(e) = handle_carrier_allocated(
                                raw,
                                &*carrier_repo,
                                &adapter_registry,
                                &booking_repo,
                                storage.as_deref(),
                                &order_intake_url,
                                &http,
                            ).await {
                                tracing::error!("handle_carrier_allocated error: {e}");
                            }
                        }
                        if let Err(e) = consumer.commit_message(&msg, CommitMode::Async) {
                            tracing::error!("Allocation booking worker Kafka commit error: {e}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_carrier_allocated(
    raw: &str,
    carrier_repo: &dyn CarrierRepository,
    adapter_registry: &AdapterRegistry,
    booking_repo: &PgCarrierBookingRepository,
    storage: Option<&dyn StorageAdapter>,
    order_intake_url: &str,
    http: &reqwest::Client,
) -> anyhow::Result<()> {
    let event: Event<CarrierAllocated> = serde_json::from_str(raw)?;
    let alloc = &event.data;

    // Resolve carrier to get its code (DHL/FEDEX/UPS/…)
    let Some(carrier) = carrier_repo
        .find_by_id(&CarrierId::from_uuid(alloc.carrier_id))
        .await?
    else {
        tracing::warn!(carrier_id = %alloc.carrier_id, "Carrier not found for allocation — skipping booking");
        return Ok(());
    };

    // Only proceed if we have a matching 3PL adapter for this carrier code.
    let Some(adapter) = adapter_registry.get(&carrier.code) else {
        tracing::debug!(
            carrier_code = %carrier.code,
            shipment_id  = %alloc.shipment_id,
            "No 3PL adapter for carrier code — skipping auto-booking"
        );
        return Ok(());
    };

    // Fetch shipment details from order-intake to build the BookingRequest.
    let shipment_url = format!("{}/v1/internal/shipments/{}", order_intake_url, alloc.shipment_id);
    let shipment_resp = http.get(&shipment_url).send().await?;
    if !shipment_resp.status().is_success() {
        anyhow::bail!(
            "order-intake returned {} for shipment {}",
            shipment_resp.status(),
            alloc.shipment_id
        );
    }
    let shipment: serde_json::Value = shipment_resp.json().await?;

    let booking_req = build_booking_request(&shipment, &alloc.service_level)?;
    let req_payload = serde_json::to_value(&booking_req).unwrap_or_default();

    let mut confirmation = adapter.book_shipment(&booking_req).await
        .map_err(|e| anyhow::anyhow!("3PL booking failed for {}: {e}", carrier.code))?;

    tracing::info!(
        carrier_code    = %carrier.code,
        shipment_id     = %alloc.shipment_id,
        booking_ref     = %confirmation.booking_ref,
        tracking_number = %confirmation.tracking_number,
        "3PL booking successful via allocation worker"
    );

    // Upload label bytes to R2/S3 and generate a signed download URL.
    if let (Some(bytes), Some(storage)) = (confirmation.label_bytes.take(), storage) {
        let key = format!("labels/{}/{}.pdf", carrier.code, confirmation.booking_ref);
        if let Err(e) = storage.put_bytes(&key, bytes, "application/pdf").await {
            tracing::warn!(key, "Label upload failed: {e}");
        } else {
            match storage.presign_download(&key, 86400).await {
                Ok(url) => confirmation.label_url = Some(url),
                Err(e)  => tracing::warn!(key, "Label presign failed: {e}"),
            }
        }
    }

    let awb = shipment["tracking_number"].as_str().map(String::from);
    let resp_payload = serde_json::json!({
        "booking_ref":        confirmation.booking_ref,
        "tracking_number":    confirmation.tracking_number,
        "label_url":          confirmation.label_url,
        "estimated_delivery": confirmation.estimated_delivery,
    });

    booking_repo.insert(&CarrierBookingRecord {
        tenant_id:        alloc.tenant_id,
        carrier_code:     carrier.code.clone(),
        shipment_id:      Some(alloc.shipment_id),
        awb,
        service_code:     booking_req.service_code.clone(),
        booking_ref:      confirmation.booking_ref.clone(),
        tracking_number:  confirmation.tracking_number.clone(),
        label_url:        confirmation.label_url.clone(),
        request_payload:  req_payload,
        response_payload: resp_payload,
        booked_by_actor:  None, // system-generated
    }).await?;

    Ok(())
}

/// Map shipment JSON from order-intake into a `BookingRequest` for the adapter.
fn build_booking_request(
    shipment: &serde_json::Value,
    service_level: &str,
) -> anyhow::Result<BookingRequest> {
    let parse_addr = |key: &str| -> anyhow::Result<Address> {
        let a = &shipment[key];
        Ok(Address {
            name:        a["name"].as_str().unwrap_or("").into(),
            company:     a["company"].as_str().map(String::from),
            street1:     a["street1"].as_str().unwrap_or("").into(),
            street2:     a["street2"].as_str().map(String::from),
            city:        a["city"].as_str().unwrap_or("").into(),
            state:       a["state"].as_str().map(String::from),
            postal_code: a["postal_code"].as_str().unwrap_or("").into(),
            country:     a["country"].as_str().unwrap_or("PH").into(),
            phone:       a["phone"].as_str().map(String::from),
            email:       a["email"].as_str().map(String::from),
        })
    };

    Ok(BookingRequest {
        service_code:         service_level.into(),
        shipper:              parse_addr("origin")?,
        consignee:            parse_addr("destination")?,
        weight_kg:            shipment["weight_kg"].as_f64().unwrap_or(1.0),
        length_cm:            shipment["length_cm"].as_f64(),
        width_cm:             shipment["width_cm"].as_f64(),
        height_cm:            shipment["height_cm"].as_f64(),
        description:          shipment["description"].as_str().unwrap_or("General Cargo").into(),
        declared_value_cents: shipment["declared_value_cents"].as_i64().unwrap_or(0),
        currency:             shipment["currency"].as_str().unwrap_or("PHP").into(),
        reference:            shipment["tracking_number"].as_str().map(String::from),
    })
}
