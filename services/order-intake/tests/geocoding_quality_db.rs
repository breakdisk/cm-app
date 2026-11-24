//! A destination's geocoding quality survives a save and a load, against a real
//! database, and a value this build does not know reads as UNKNOWN.
//!
//! Needs Postgres with order-intake's migrations applied. Skipped locally when
//! DATABASE_URL is unset; fatal in CI, where a missing database means the
//! harness broke and a green pass over nothing would be worse than no test.

use chrono::Utc;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use logisticos_order_intake::{
    application::services::shipment_service::ShipmentRepository,
    domain::{
        entities::shipment::{PaymentRequirement, Shipment},
        value_objects::{GeocodingRecord, ServiceType, ShipmentWeight},
    },
    infrastructure::db::PgShipmentRepository,
};
use logisticos_types::{
    awb::{Awb, ServiceCode, TenantCode},
    Address, CustomerId, GeocodeQuality, MerchantId, ShipmentId, ShipmentStatus, TenantId,
};

async fn pool() -> Option<PgPool> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(PgPoolOptions::new().max_connections(2).connect(&url).await.expect("connect")),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set: Postgres provisioning failed."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

fn shipment(dest_geocoding: GeocodingRecord) -> Shipment {
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
        tenant_id: TenantId::from_uuid(Uuid::new_v4()),
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
        cancellation_policy_version: None,
        booking_amount_cents: None,
        booking_currency: None,
        dest_geocoding,
    }
}

#[tokio::test]
async fn geocoding_record_round_trips() {
    let Some(pool) = pool().await else { return };
    let repo = PgShipmentRepository { pool };
    let record = GeocodingRecord {
        quality: GeocodeQuality::Parcel,
        source: Some("provider".into()),
        provider: Some("mapbox".into()),
        place_id: None,
        confidence: None,
        geocoded_at: Some(Utc::now()),
    };
    let s = shipment(record.clone());
    repo.save(&s).await.expect("save");

    let back = repo.find_by_id(&s.id).await.expect("find").expect("row");
    assert_eq!(back.dest_geocoding.quality, GeocodeQuality::Parcel);
    assert_eq!(back.dest_geocoding.source, record.source);
    assert_eq!(back.dest_geocoding.provider, record.provider);
    assert_eq!(back.dest_geocoding.place_id, None);
    assert_eq!(back.dest_geocoding.confidence, None);
    assert!(back.dest_geocoding.geocoded_at.is_some());
}

#[tokio::test]
async fn a_row_with_no_geocoding_reads_as_unknown_and_an_unrecognized_value_too() {
    let Some(pool) = pool().await else { return };
    let repo = PgShipmentRepository { pool: pool.clone() };
    let s = shipment(GeocodingRecord::unknown());
    repo.save(&s).await.expect("save");

    let back = repo.find_by_id(&s.id).await.expect("find").expect("row");
    assert_eq!(back.dest_geocoding.quality, GeocodeQuality::Unknown);
    assert_eq!(back.dest_geocoding.source, None);

    // A level a newer build wrote, or a hand edit: fails closed.
    sqlx::query("UPDATE order_intake.shipments SET dest_geocoding_quality = 'ROOFTOP_PLUS' WHERE id = $1")
        .bind(s.id.inner())
        .execute(&pool)
        .await
        .expect("update");
    let back = repo.find_by_id(&s.id).await.expect("find").expect("row");
    assert_eq!(back.dest_geocoding.quality, GeocodeQuality::Unknown);
}
