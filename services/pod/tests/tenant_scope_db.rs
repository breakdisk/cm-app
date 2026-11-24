//! A proof of delivery or pickup is evidence (photos, signature, GPS, the
//! recipient's name). It must be readable only inside the tenant that owns it:
//! asking for another tenant's id is "not found", exactly like an id that does
//! not exist, so ids cannot be probed.
//!
//! Needs Postgres with the pod migrations applied. Skipped locally when
//! DATABASE_URL is unset; fatal in CI, where a missing database means the
//! harness broke and a green pass over nothing would be worse than no test.

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use logisticos_errors::AppError;
use logisticos_events::producer::KafkaProducer;
use logisticos_pod::application::services::PodService;
use logisticos_pod::domain::entities::{ProofOfDelivery, ProofOfPickup};
use logisticos_pod::domain::repositories::{PickupRepository, PodRepository};
use logisticos_pod::domain::value_objects::delivery_pin::DeliveryPinPolicy;
use logisticos_pod::infrastructure::db::{
    PgOtpRepository, PgPickupRepository, PgPodRepository, PgTelemetryRepository,
};
use logisticos_pod::infrastructure::external::sms::NoOpSmsAdapter;
use logisticos_pod::infrastructure::external::storage::{PresignedUpload, StorageAdapter};
use logisticos_types::TenantId;

struct NoStorage;

#[async_trait]
impl StorageAdapter for NoStorage {
    async fn presign_upload(&self, key: &str, _ct: &str, _ttl: u32) -> anyhow::Result<PresignedUpload> {
        Ok(PresignedUpload { url: format!("https://s3.test/{key}"), headers: Default::default() })
    }
    async fn presign_download(&self, key: &str, _ttl: u32) -> anyhow::Result<String> {
        Ok(format!("https://s3.test/dl/{key}"))
    }
    async fn object_exists(&self, _key: &str) -> anyhow::Result<bool> { Ok(true) }
    async fn delete(&self, _key: &str) -> anyhow::Result<()> { Ok(()) }
}

async fn pool() -> Option<PgPool> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(PgPoolOptions::new().max_connections(4).connect(&url).await.expect("connect")),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set: Postgres provisioning failed. \
                 These tests guard cross-tenant reads of delivery evidence; they must not be skipped here."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

fn service(pool: &PgPool) -> PodService {
    PodService::new(
        Arc::new(PgPodRepository::new(pool.clone())),
        Arc::new(PgOtpRepository::new(pool.clone())),
        Arc::new(PgPickupRepository::new(pool.clone())),
        Arc::new(PgTelemetryRepository::new(pool.clone())),
        Arc::new(NoStorage),
        Arc::new(NoStorage),
        Arc::new(NoOpSmsAdapter),
        Arc::new(KafkaProducer::new("localhost:9092").expect("producer")),
        None,
        DeliveryPinPolicy::default(),
    )
}

fn pod_for(tenant: Uuid) -> ProofOfDelivery {
    ProofOfDelivery::new(
        tenant, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
        "Maria Santos".into(), 14.5995, 120.9842, true, false, None, true, true,
    )
}

fn pop_for(tenant: Uuid) -> ProofOfPickup {
    ProofOfPickup::new(
        tenant, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
        14.5995, 120.9842, true, false, None, "standard".into(), None, None,
    )
}

fn is_not_found(e: &AppError) -> bool {
    matches!(e, AppError::NotFound { .. })
}

#[tokio::test]
async fn pod_of_another_tenant_is_not_found() {
    let Some(pool) = pool().await else { return };
    let (mine, theirs) = (Uuid::new_v4(), Uuid::new_v4());
    let pod = pod_for(theirs);
    PgPodRepository::new(pool.clone()).save(&pod).await.expect("seed");
    let svc = service(&pool);

    let err = svc.get_by_id(&TenantId::from_uuid(mine), pod.id).await
        .expect_err("another tenant's POD must not be readable");
    assert!(is_not_found(&err), "must look exactly like a missing id: {err:?}");

    // Same answer through the repository, and by shipment.
    let repo = PgPodRepository::new(pool.clone());
    assert!(repo.find_by_id(mine, pod.id).await.expect("query").is_none());
    assert!(repo.find_by_shipment(mine, pod.shipment_id).await.expect("query").is_none());
    assert!(svc.get_by_shipment(&TenantId::from_uuid(mine), pod.shipment_id).await.expect("query").is_none());
}

#[tokio::test]
async fn own_pod_is_returned() {
    let Some(pool) = pool().await else { return };
    let mine = Uuid::new_v4();
    let pod = pod_for(mine);
    PgPodRepository::new(pool.clone()).save(&pod).await.expect("seed");
    let svc = service(&pool);

    let got = svc.get_by_id(&TenantId::from_uuid(mine), pod.id).await.expect("own POD");
    assert_eq!(got.id, pod.id);
    let by_ship = svc.get_by_shipment(&TenantId::from_uuid(mine), pod.shipment_id).await.expect("query")
        .expect("own POD by shipment");
    assert_eq!(by_ship.id, pod.id);
}

#[tokio::test]
async fn the_mutating_paths_are_scoped_too() {
    let Some(pool) = pool().await else { return };
    let (mine, theirs) = (Uuid::new_v4(), Uuid::new_v4());
    let pod = pod_for(theirs);
    PgPodRepository::new(pool.clone()).save(&pod).await.expect("seed");
    let svc = service(&pool);
    let me = TenantId::from_uuid(mine);

    let err = svc
        .get_upload_url(pod.id, &me, "image/jpeg")
        .await
        .expect_err("presigning an upload against another tenant's POD");
    assert!(is_not_found(&err), "{err:?}");
}

#[tokio::test]
async fn pop_of_another_tenant_is_not_found_and_own_is_returned() {
    let Some(pool) = pool().await else { return };
    let (mine, theirs) = (Uuid::new_v4(), Uuid::new_v4());
    let (their_pop, my_pop) = (pop_for(theirs), pop_for(mine));
    let repo = PgPickupRepository::new(pool.clone());
    repo.save(&their_pop).await.expect("seed theirs");
    repo.save(&my_pop).await.expect("seed mine");
    let svc = service(&pool);
    let me = TenantId::from_uuid(mine);

    let err = svc.get_pop_by_id(&me, their_pop.id).await.expect_err("another tenant's POP");
    assert!(is_not_found(&err), "{err:?}");
    assert!(svc.get_pop_by_shipment(&me, their_pop.shipment_id).await.expect("query").is_none());
    let err = svc
        .get_pop_upload_url(their_pop.id, &me, "image/jpeg")
        .await
        .expect_err("presigning an upload against another tenant's POP");
    assert!(is_not_found(&err), "{err:?}");

    assert_eq!(svc.get_pop_by_id(&me, my_pop.id).await.expect("own POP").id, my_pop.id);
    assert_eq!(
        svc.get_pop_by_shipment(&me, my_pop.shipment_id).await.expect("query").expect("own").id,
        my_pop.id
    );
}
