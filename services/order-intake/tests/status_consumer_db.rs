//! The status consumer moves a shipment only for the tenant that owns it,
//! against a real database.
//!
//! Needs Postgres with order-intake's migrations applied. Skipped locally when
//! DATABASE_URL is unset; fatal in CI, where a missing database means the
//! harness broke and a green pass over nothing would be worse than no test.

use chrono::Utc;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use logisticos_events::{topics, Event};
use logisticos_order_intake::{
    application::services::shipment_service::ShipmentRepository,
    domain::{
        entities::shipment::{PaymentRequirement, Shipment},
        value_objects::{GeocodingRecord, ServiceType, ShipmentWeight},
    },
    infrastructure::{db::PgShipmentRepository, messaging::status_consumer::handle},
};
use logisticos_types::{
    awb::{Awb, ServiceCode, TenantCode},
    Address, CustomerId, MerchantId, ShipmentId, ShipmentStatus, TenantId,
};

async fn pool() -> Option<PgPool> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(PgPoolOptions::new().max_connections(2).connect(&url).await.expect("connect")),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set: Postgres provisioning failed. \
                 These tests guard which tenant may move a shipment; they must not be skipped here."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

fn shipment(tenant: Uuid) -> Shipment {
    let addr = Address {
        line1: "1 Seed Street".into(),
        line2: None,
        barangay: None,
        city: "Manila".into(),
        province: "Metro Manila".into(),
        postal_code: "1000".into(),
        country_code: "PH".into(),
        coordinates: None,
    };
    let now = Utc::now();
    let tenant_code = TenantCode::new("TST").expect("tenant code");
    Shipment {
        id: ShipmentId::new(),
        tenant_id: TenantId::from_uuid(tenant),
        merchant_id: MerchantId::from_uuid(Uuid::new_v4()),
        customer_id: CustomerId::new(),
        customer_name: "Test Customer".into(),
        customer_phone: "+639171234567".into(),
        customer_email: None,
        booked_by_customer: false,
        auto_dispatch: true,
        awb: Awb::generate(&tenant_code, ServiceCode::Standard, Uuid::new_v4().as_u128() as u32 % 9_000_000 + 1),
        piece_count: 1,
        status: ShipmentStatus::Pending,
        service_type: ServiceType::Standard,
        origin: addr.clone(),
        destination: addr,
        weight: ShipmentWeight::from_grams(1000),
        dimensions: None,
        declared_value: None,
        cod_amount: None,
        special_instructions: None,
        external_order_id: None,
        merchant_reference: None,
        source_platform: None,
        payment_intent_id: None,
        payment_status: PaymentRequirement::NotRequired,
        pending_dispatch_events: None,
        idempotency_key: None,
        created_at: now,
        updated_at: now,
        scheduled_pickup_at: None,
        sla_due_at: None,
        cancellation_policy_version: None,
        booking_amount_cents: None,
        booking_currency: None,
        dest_geocoding: GeocodingRecord::unknown(),
        dest_parts: Default::default(),
    }
}

fn driver_assigned(event_tenant: Uuid, shipment_id: Uuid) -> Vec<u8> {
    let data = serde_json::json!({ "shipment_id": shipment_id });
    serde_json::to_vec(&Event::new("dispatch", "driver.assigned", event_tenant, data)).expect("payload")
}

async fn status_of(pool: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM order_intake.shipments WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("status")
}

async fn timeline_rows(pool: &PgPool, id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM order_intake.shipment_events WHERE shipment_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("count")
}

#[tokio::test]
async fn an_event_from_another_tenant_does_not_move_the_shipment() {
    let Some(pool) = pool().await else { return };
    let repo = PgShipmentRepository { pool: pool.clone() };
    let owner = Uuid::new_v4();
    let s = shipment(owner);
    repo.save(&s).await.expect("save");
    let id = s.id.inner();

    let result = handle(&pool, topics::DRIVER_ASSIGNED, &driver_assigned(Uuid::new_v4(), id)).await;

    assert!(result.is_ok(), "skipped, not failed: the offset must still be committed");
    assert_eq!(status_of(&pool, id).await, "pending");
    assert_eq!(timeline_rows(&pool, id).await, 0, "no timeline row for a transition that did not happen");
}

#[tokio::test]
async fn an_event_with_a_nil_tenant_does_not_move_the_shipment() {
    let Some(pool) = pool().await else { return };
    let repo = PgShipmentRepository { pool: pool.clone() };
    let s = shipment(Uuid::new_v4());
    repo.save(&s).await.expect("save");
    let id = s.id.inner();

    handle(&pool, topics::DRIVER_ASSIGNED, &driver_assigned(Uuid::nil(), id)).await.expect("ok");
    assert_eq!(status_of(&pool, id).await, "pending");
}

#[tokio::test]
async fn an_event_from_the_owning_tenant_moves_the_shipment() {
    let Some(pool) = pool().await else { return };
    let repo = PgShipmentRepository { pool: pool.clone() };
    let owner = Uuid::new_v4();
    let s = shipment(owner);
    repo.save(&s).await.expect("save");
    let id = s.id.inner();

    handle(&pool, topics::DRIVER_ASSIGNED, &driver_assigned(owner, id)).await.expect("ok");

    assert_eq!(status_of(&pool, id).await, "pickup_assigned");
    assert_eq!(timeline_rows(&pool, id).await, 1);
}

#[tokio::test]
async fn a_container_event_from_another_tenant_does_not_move_a_shipment_by_awb() {
    let Some(pool) = pool().await else { return };
    let repo = PgShipmentRepository { pool: pool.clone() };
    let owner = Uuid::new_v4();
    let s = shipment(owner);
    repo.save(&s).await.expect("save");
    let id = s.id.inner();

    let data = serde_json::json!({ "master_awbs": [s.awb.as_str()] });
    let evt = |tenant| serde_json::to_vec(&Event::new("hub", "container.departed", tenant, data.clone())).expect("payload");

    handle(&pool, topics::CONTAINER_DEPARTED, &evt(Uuid::new_v4())).await.expect("ok");
    assert_eq!(status_of(&pool, id).await, "pending");

    handle(&pool, topics::CONTAINER_DEPARTED, &evt(owner)).await.expect("ok");
    assert_eq!(status_of(&pool, id).await, "in_transit");
}
