//! Shared post-assignment event emission — used by BOTH the 1:1 quick-dispatch
//! path and the gig offer claim path so the two can never drift.
//!
//! Emits TASK_ASSIGNED once per leg (pickup when origin data exists, then
//! delivery) plus DRIVER_ASSIGNED for the customer-facing engagement flow.
//! Always called AFTER the assignment transaction commits — Kafka latency
//! must never sit inside a claim's row-lock window.

use logisticos_events::{envelope::Event, payloads::TaskAssigned, producer::KafkaProducer, topics};
use uuid::Uuid;

use crate::domain::events::DriverAssigned;
use crate::infrastructure::db::DispatchQueueRow;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn emit_assignment_events(
    kafka: &KafkaProducer,
    queue_item: &DispatchQueueRow,
    tenant_id: Uuid,
    driver_id: Uuid,
    assignment_id: Uuid,
    route_id: Uuid,
    shipment_id: Uuid,
    payout_cents: Option<i64>,
) -> anyhow::Result<()> {
    // One event per leg — driver-ops creates one DriverTask row per event,
    // sequenced pickup → delivery. Both share assignment_id (same job). The
    // pickup leg is only emitted when origin data exists — legacy shipments
    // without structured origin fall through to a single delivery task.
    let has_origin = !queue_item.origin_address_line1.is_empty()
        || queue_item.origin_lat.is_some();

    let mut next_sequence: i32 = 1;
    if has_origin {
        let pickup_event = Event::new("dispatch", "task.assigned", tenant_id, TaskAssigned {
            task_id:              Uuid::new_v4(),
            assignment_id,
            shipment_id,
            route_id,
            driver_id,
            tenant_id,
            sequence:             next_sequence,
            task_type:            "pickup".into(),
            address_line1:        queue_item.origin_address_line1.clone(),
            address_city:         queue_item.origin_city.clone(),
            address_province:     queue_item.origin_province.clone(),
            address_postal_code:  queue_item.origin_postal_code.clone(),
            address_lat:          queue_item.origin_lat,
            address_lng:          queue_item.origin_lng,
            customer_name:        queue_item.customer_name.clone(),
            customer_phone:       queue_item.customer_phone.clone(),
            cod_amount_cents:     None,  // COD is collected on delivery, never on pickup
            special_instructions: queue_item.special_instructions.clone(),
            tracking_number:      queue_item.tracking_number.clone().unwrap_or_default(),
            customer_email:       queue_item.customer_email.clone().unwrap_or_default(),
            customer_id:          Some(queue_item.customer_id),
            merchant_name:        queue_item.merchant_name.clone(),
            delivery_category:    queue_item.delivery_category.clone(),
            weight_grams:         queue_item.weight_grams.max(0) as u32,
            pickup_lat:           queue_item.origin_lat,
            pickup_lng:           queue_item.origin_lng,
            delivery_lat:         queue_item.dest_lat,
            delivery_lng:         queue_item.dest_lng,
            payout_cents,
        });
        kafka.publish_event(topics::TASK_ASSIGNED, &pickup_event).await?;
        next_sequence += 1;
    }

    let delivery_event = Event::new("dispatch", "task.assigned", tenant_id, TaskAssigned {
        task_id:              Uuid::new_v4(),
        assignment_id,
        shipment_id,
        route_id,
        driver_id,
        tenant_id,
        sequence:             next_sequence,
        task_type:            "delivery".into(),
        address_line1:        queue_item.dest_address_line1.clone(),
        address_city:         queue_item.dest_city.clone(),
        address_province:     queue_item.dest_province.clone(),
        address_postal_code:  queue_item.dest_postal_code.clone(),
        address_lat:          queue_item.dest_lat,
        address_lng:          queue_item.dest_lng,
        customer_name:        queue_item.customer_name.clone(),
        customer_phone:       queue_item.customer_phone.clone(),
        cod_amount_cents:     queue_item.cod_amount_cents,
        special_instructions: queue_item.special_instructions.clone(),
        tracking_number:      queue_item.tracking_number.clone().unwrap_or_default(),
        customer_email:       queue_item.customer_email.clone().unwrap_or_default(),
        customer_id:          Some(queue_item.customer_id),
        merchant_name:        queue_item.merchant_name.clone(),
        delivery_category:    queue_item.delivery_category.clone(),
        weight_grams:         queue_item.weight_grams.max(0) as u32,
        pickup_lat:           queue_item.origin_lat,
        pickup_lng:           queue_item.origin_lng,
        delivery_lat:         queue_item.dest_lat,
        delivery_lng:         queue_item.dest_lng,
        payout_cents,
    });
    kafka.publish_event(topics::TASK_ASSIGNED, &delivery_event).await?;

    // DRIVER_ASSIGNED — engagement sends "pickup_scheduled" WhatsApp to the
    // customer. Contact fields denormalized so engagement needs no lookup.
    let driver_assigned_event = Event::new("dispatch", "driver.assigned", tenant_id, DriverAssigned {
        assignment_id,
        shipment_id,
        customer_id:           queue_item.customer_id,
        route_id,
        driver_id,
        tenant_id,
        customer_name:         queue_item.customer_name.clone(),
        customer_phone:        queue_item.customer_phone.clone(),
        customer_email:        queue_item.customer_email.clone().unwrap_or_default(),
        tracking_number:       queue_item.tracking_number.clone().unwrap_or_default(),
        estimated_pickup_time: None,  // ETA not computed at assignment time
    });
    kafka.publish_event(topics::DRIVER_ASSIGNED, &driver_assigned_event).await?;

    Ok(())
}
