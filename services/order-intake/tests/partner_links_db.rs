//! Which partner handles each shipment, projected from events, against a real
//! database.
//!
//! Partner scoping needs something to filter on, and order-intake has no carrier
//! column. These links are it, so what matters here is that the link appears for
//! a carrier's work, never for a platform driver's, and never twice.
//!
//! Needs Postgres with order-intake's migrations applied. Skipped locally when
//! DATABASE_URL is unset; fatal in CI, where a missing database means the
//! harness broke and a green pass over nothing would be worse than no test.

use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use logisticos_events::{topics, Event};
use logisticos_order_intake::infrastructure::messaging::partner_link_consumer::handle_partner_link;

fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(url),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set: Postgres provisioning failed. \
                 These tests guard which partner may see a shipment; they must not be skipped here."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

async fn pool() -> Option<PgPool> {
    let url = database_url()?;
    Some(PgPoolOptions::new().max_connections(4).connect(&url).await.expect("connect"))
}

async fn links_for(pool: &PgPool, shipment: Uuid) -> Vec<(Uuid, String)> {
    sqlx::query_as("SELECT carrier_id, source FROM order_intake.shipment_partner_links WHERE shipment_id = $1")
        .bind(shipment)
        .fetch_all(pool)
        .await
        .unwrap()
}

fn carrier_allocated(tenant: Uuid, shipment: Uuid, carrier: Uuid) -> Vec<u8> {
    let data = serde_json::json!({
        "carrier_id": carrier, "tenant_id": tenant, "shipment_id": shipment,
        "zone": "AUH", "service_level": "standard", "total_cost_cents": 1500,
        "promised_by": "2026-12-01T10:00:00Z", "method": "rate_shop",
    });
    serde_json::to_vec(&Event::new("carrier", "carrier.allocated", tenant, data)).unwrap()
}

fn task_assigned(tenant: Uuid, shipment: Uuid, carrier: Option<Uuid>) -> Vec<u8> {
    let mut data = serde_json::json!({
        "task_id": Uuid::new_v4(), "assignment_id": Uuid::new_v4(), "shipment_id": shipment,
        "route_id": Uuid::new_v4(), "driver_id": Uuid::new_v4(), "tenant_id": tenant,
        "sequence": 1, "address_line1": "1 Test St", "address_city": "Abu Dhabi",
        "address_province": "AD", "address_postal_code": "00000",
        "customer_name": "C", "customer_phone": "+971500000000",
    });
    if let Some(c) = carrier {
        data["driver_carrier_id"] = serde_json::json!(c);
    }
    serde_json::to_vec(&Event::new("dispatch", "task.assigned", tenant, data)).unwrap()
}

#[tokio::test]
async fn carrier_allocated_links_the_shipment() {
    let Some(pool) = pool().await else { return };
    let (t, s, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    handle_partner_link(&pool, topics::CARRIER_ALLOCATED, &carrier_allocated(t, s, c)).await.unwrap();
    assert_eq!(links_for(&pool, s).await, vec![(c, "carrier_allocated".to_string())]);
}

#[tokio::test]
async fn task_for_a_carrier_driver_links_the_shipment() {
    let Some(pool) = pool().await else { return };
    let (t, s, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    handle_partner_link(&pool, topics::TASK_ASSIGNED, &task_assigned(t, s, Some(c))).await.unwrap();
    assert_eq!(links_for(&pool, s).await, vec![(c, "task_assigned".to_string())]);
}

#[tokio::test]
async fn task_for_a_platform_driver_links_nothing() {
    let Some(pool) = pool().await else { return };
    let (t, s) = (Uuid::new_v4(), Uuid::new_v4());
    handle_partner_link(&pool, topics::TASK_ASSIGNED, &task_assigned(t, s, None)).await.unwrap();
    assert!(links_for(&pool, s).await.is_empty());
}

#[tokio::test]
async fn replayed_event_adds_no_row() {
    let Some(pool) = pool().await else { return };
    let (t, s, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let msg = task_assigned(t, s, Some(c));
    handle_partner_link(&pool, topics::TASK_ASSIGNED, &msg).await.unwrap();
    handle_partner_link(&pool, topics::TASK_ASSIGNED, &msg).await.unwrap();
    assert_eq!(links_for(&pool, s).await.len(), 1);
}

/// The pickup leg and the delivery leg arrive as two events for one shipment.
/// Still one link.
#[tokio::test]
async fn two_legs_for_the_same_carrier_are_one_link() {
    let Some(pool) = pool().await else { return };
    let (t, s, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    handle_partner_link(&pool, topics::TASK_ASSIGNED, &task_assigned(t, s, Some(c))).await.unwrap();
    handle_partner_link(&pool, topics::TASK_ASSIGNED, &task_assigned(t, s, Some(c))).await.unwrap();
    handle_partner_link(&pool, topics::CARRIER_ALLOCATED, &carrier_allocated(t, s, c)).await.unwrap();
    assert_eq!(links_for(&pool, s).await.len(), 1);
}

/// A shipment handed from one carrier to another keeps both links, so neither
/// loses sight of work it did.
#[tokio::test]
async fn a_second_carrier_adds_a_second_link() {
    let Some(pool) = pool().await else { return };
    let (t, s) = (Uuid::new_v4(), Uuid::new_v4());
    let (c1, c2) = (Uuid::new_v4(), Uuid::new_v4());
    handle_partner_link(&pool, topics::TASK_ASSIGNED, &task_assigned(t, s, Some(c1))).await.unwrap();
    handle_partner_link(&pool, topics::CARRIER_ALLOCATED, &carrier_allocated(t, s, c2)).await.unwrap();
    assert_eq!(links_for(&pool, s).await.len(), 2);
}

#[tokio::test]
async fn is_linked_is_true_only_for_that_tenant_shipment_and_carrier() {
    use logisticos_order_intake::application::services::shipment_service::ShipmentRepository;
    use logisticos_order_intake::infrastructure::db::PgShipmentRepository;

    let Some(pool) = pool().await else { return };
    let (t, s, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    handle_partner_link(&pool, topics::TASK_ASSIGNED, &task_assigned(t, s, Some(c))).await.unwrap();

    let repo = PgShipmentRepository { pool };
    assert!(repo.is_linked(t, s, c).await.unwrap());
    assert!(!repo.is_linked(t, s, Uuid::new_v4()).await.unwrap(), "another carrier");
    assert!(!repo.is_linked(t, Uuid::new_v4(), c).await.unwrap(), "another shipment");
    assert!(!repo.is_linked(Uuid::new_v4(), s, c).await.unwrap(), "another tenant");
}

#[tokio::test]
async fn an_unrelated_topic_is_ignored() {
    let Some(pool) = pool().await else { return };
    handle_partner_link(&pool, "logisticos.something.else", b"{}").await.unwrap();
}

/// A malformed message is an error the caller logs and skips; it must not link
/// anything or panic.
#[tokio::test]
async fn a_malformed_payload_is_an_error() {
    let Some(pool) = pool().await else { return };
    assert!(handle_partner_link(&pool, topics::TASK_ASSIGNED, b"not json").await.is_err());
}
