//! Tenant-isolation tests for the handlers in `bootstrap.rs`.
//!
//! The real router (`build_router`, `require_auth` layer included) is driven
//! with `tower::ServiceExt::oneshot`. Storage is in-memory fakes that honour
//! the `tenant_id` they are handed, exactly as the Postgres impls do in SQL, so
//! what these tests prove is the handler/service half of the contract: the
//! tenant that reaches storage is the JWT's, a foreign id comes back 404, and
//! nothing is written. The SQL itself is covered by the statement scan at the
//! bottom (every hub_ops statement must mention tenant_id), not by execution:
//! there is no Postgres in unit tests.
//!
//! The pool is `connect_lazy` and never used: every route exercised here is
//! served by the fakes.

use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use logisticos_auth::claims::Claims;
use logisticos_types::{ContainerId, PalletId, TenantId, TransportMode};

use super::*;
use crate::application::services::hub_transfer_repositories::{
    HubInventoryRepository, HubLocationRepository, HubRoutingConfigRepository, HubScanRepository,
    HubTransferManifestRepository,
};
use crate::application::services::pallet_service::{ContainerRepository, PalletRepository};
use crate::domain::entities::{
    container::Container, pallet::Pallet, Hub, HubInventory, HubLocation, HubRoutingConfig,
    HubScan, HubTransferManifest, InventoryUnit, ParcelInduction,
};

const SECRET: &str = "tenant-isolation-test-secret";

// ── fakes ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Counters {
    writes: Mutex<Vec<String>>,
}
impl Counters {
    fn note(&self, what: &str) { self.writes.lock().unwrap().push(what.to_owned()); }
    fn count(&self) -> usize { self.writes.lock().unwrap().len() }
}

struct FakeHubs { hubs: Vec<Hub>, w: Arc<Counters> }
#[async_trait::async_trait]
impl HubRepository for FakeHubs {
    async fn find_by_id(&self, id: &crate::domain::entities::HubId, t: &TenantId) -> anyhow::Result<Option<Hub>> {
        Ok(self.hubs.iter().find(|h| h.id.inner() == id.inner() && h.tenant_id == *t).cloned())
    }
    async fn list(&self, t: &TenantId) -> anyhow::Result<Vec<Hub>> {
        Ok(self.hubs.iter().filter(|h| h.tenant_id == *t).cloned().collect())
    }
    async fn save(&self, _h: &Hub) -> anyhow::Result<()> { self.w.note("hub.save"); Ok(()) }
}

struct FakeInductions;
#[async_trait::async_trait]
impl InductionRepository for FakeInductions {
    async fn find_by_id(&self, _: &crate::domain::entities::InductionId, _: &TenantId) -> anyhow::Result<Option<ParcelInduction>> { Ok(None) }
    async fn find_by_shipment(&self, _: Uuid, _: &TenantId) -> anyhow::Result<Option<ParcelInduction>> { Ok(None) }
    async fn list_active(&self, _: &crate::domain::entities::HubId, _: &TenantId) -> anyhow::Result<Vec<ParcelInduction>> { Ok(vec![]) }
    async fn save(&self, _: &ParcelInduction) -> anyhow::Result<()> { Ok(()) }
}

struct FakePallets { items: Vec<Pallet>, w: Arc<Counters> }
#[async_trait::async_trait]
impl PalletRepository for FakePallets {
    async fn find_by_id(&self, id: &PalletId, t: &TenantId) -> anyhow::Result<Option<Pallet>> {
        Ok(self.items.iter().find(|p| p.id.inner() == id.inner() && p.tenant_id == *t).cloned())
    }
    async fn find_open_for_hub(
        &self,
        hub: &logisticos_types::HubId,
        _dest: Option<&logisticos_types::HubId>,
        t: &TenantId,
    ) -> anyhow::Result<Option<Pallet>> {
        Ok(self.items.iter().find(|p| p.origin_hub_id.inner() == hub.inner() && p.tenant_id == *t).cloned())
    }
    async fn save(&self, _p: &Pallet) -> anyhow::Result<()> { self.w.note("pallet.save"); Ok(()) }
}

struct FakeContainers { items: Vec<Container>, w: Arc<Counters> }
#[async_trait::async_trait]
impl ContainerRepository for FakeContainers {
    async fn find_by_id(&self, id: &ContainerId, t: &TenantId) -> anyhow::Result<Option<Container>> {
        Ok(self.items.iter().find(|c| c.id.inner() == id.inner() && c.tenant_id == *t).cloned())
    }
    async fn save(&self, _c: &Container) -> anyhow::Result<()> { self.w.note("container.save"); Ok(()) }
    async fn resolve_awbs_to_shipment_ids(&self, _: &[String], _: &TenantId) -> anyhow::Result<Vec<Uuid>> { Ok(vec![]) }
}

struct FakeScans { w: Arc<Counters> }
#[async_trait::async_trait]
impl HubScanRepository for FakeScans {
    async fn append(&self, _s: &HubScan) -> anyhow::Result<()> { self.w.note("scan.append"); Ok(()) }
    async fn list_for_shipment(&self, _: Uuid, _: Uuid) -> anyhow::Result<Vec<HubScan>> { Ok(vec![]) }
}

struct FakeLocations { items: Vec<HubLocation>, w: Arc<Counters> }
#[async_trait::async_trait]
impl HubLocationRepository for FakeLocations {
    async fn save(&self, _l: &HubLocation) -> anyhow::Result<()> { self.w.note("location.save"); Ok(()) }
    async fn find_by_id(&self, id: Uuid, t: Uuid) -> anyhow::Result<Option<HubLocation>> {
        Ok(self.items.iter().find(|l| l.id == id && l.tenant_id.inner() == t).cloned())
    }
    async fn list_for_hub(&self, hub: Uuid, t: Uuid, _z: Option<&str>) -> anyhow::Result<Vec<HubLocation>> {
        Ok(self.items.iter().filter(|l| l.hub_id.inner() == hub && l.tenant_id.inner() == t).cloned().collect())
    }
}

struct FakeInventory { items: Vec<HubInventory>, locations: Vec<HubLocation>, w: Arc<Counters> }
#[async_trait::async_trait]
impl HubInventoryRepository for FakeInventory {
    async fn upsert(&self, _i: &HubInventory) -> anyhow::Result<()> { self.w.note("inventory.upsert"); Ok(()) }
    async fn move_to(&self, inv: Uuid, to: Uuid, t: Uuid) -> anyhow::Result<u64> {
        let inv_ok = self.items.iter().any(|i| i.id == inv && i.tenant_id.inner() == t);
        let loc_ok = self.locations.iter().any(|l| l.id == to && l.tenant_id.inner() == t);
        if inv_ok && loc_ok { self.w.note("inventory.move"); Ok(1) } else { Ok(0) }
    }
    async fn list_at_location(&self, loc: Uuid, t: Uuid) -> anyhow::Result<Vec<HubInventory>> {
        Ok(self.items.iter().filter(|i| i.location_id == loc && i.tenant_id.inner() == t).cloned().collect())
    }
    async fn find_by_unit(&self, _: Uuid, _: &InventoryUnit) -> anyhow::Result<Option<HubInventory>> { Ok(None) }
}

struct FakeManifests { items: Vec<HubTransferManifest>, w: Arc<Counters> }
#[async_trait::async_trait]
impl HubTransferManifestRepository for FakeManifests {
    async fn save(&self, _m: &HubTransferManifest) -> anyhow::Result<()> { self.w.note("manifest.save"); Ok(()) }
    async fn find_by_container(&self, c: Uuid, t: Uuid) -> anyhow::Result<Option<HubTransferManifest>> {
        Ok(self.items.iter().find(|m| m.container_id.inner() == c && m.tenant_id.inner() == t).cloned())
    }
    async fn list_in_customs(&self, _: Uuid, _: Uuid) -> anyhow::Result<Vec<HubTransferManifest>> { Ok(vec![]) }
}

struct FakeRouting { w: Arc<Counters> }
#[async_trait::async_trait]
impl HubRoutingConfigRepository for FakeRouting {
    async fn save(&self, _c: &HubRoutingConfig) -> anyhow::Result<()> { self.w.note("routing.save"); Ok(()) }
    async fn find(&self, _: Uuid, _: Uuid, _: &str) -> anyhow::Result<Option<HubRoutingConfig>> { Ok(None) }
    async fn list_for_hub(&self, _: Uuid, _: Uuid) -> anyhow::Result<Vec<HubRoutingConfig>> { Ok(vec![]) }
}

/// Plans/specs are only reached after a hub check that the foreign-tenant
/// cases never pass; reaching them is a test failure.
struct NoSpecs;
#[async_trait::async_trait]
impl TruckSpecRepository for NoSpecs {
    async fn list(&self, _: Uuid) -> anyhow::Result<Vec<TruckSpec>> { unimplemented!("reached specs") }
    async fn find(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<TruckSpec>> { unimplemented!("reached specs") }
    async fn create(&self, _: &TruckSpec) -> anyhow::Result<TruckSpec> { unimplemented!("reached specs") }
    async fn update(&self, _: &TruckSpec) -> anyhow::Result<TruckSpec> { unimplemented!("reached specs") }
}
struct NoPlans;
#[async_trait::async_trait]
impl ConsolidationPlanRepository for NoPlans {
    async fn list(&self, _: Uuid, _: Uuid) -> anyhow::Result<Vec<ConsolidationPlan>> { Ok(vec![]) }
    async fn find(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<ConsolidationPlan>> { Ok(None) }
    async fn upsert(&self, _: &ConsolidationPlan) -> anyhow::Result<ConsolidationPlan> { unimplemented!("reached plans") }
    async fn update_placements(&self, _: Uuid, _: Uuid, _: Value) -> anyhow::Result<ConsolidationPlan> { unimplemented!("reached plans") }
    async fn confirm(&self, _: Uuid, _: Uuid, _: Uuid) -> anyhow::Result<ConsolidationPlan> { unimplemented!("reached plans") }
    async fn mark_loaded(&self, _: Uuid, _: Uuid) -> anyhow::Result<bool> { unimplemented!("reached plans") }
    async fn insert_loading(&self, _: Uuid, _: Uuid, _: &str, _: Option<Uuid>) -> anyhow::Result<bool> { unimplemented!("reached plans") }
    async fn loading_count(&self, _: Uuid, _: Uuid) -> anyhow::Result<i64> { unimplemented!("reached plans") }
}

/// The direct-SQL container surface used by the HTTP handlers.
struct FakeOps { rows: Vec<ContainerRow>, w: Arc<Counters> }
#[async_trait::async_trait]
impl ContainerOps for FakeOps {
    async fn find_by_id(&self, id: Uuid, t: Uuid) -> anyhow::Result<Option<ContainerRow>> {
        Ok(self.rows.iter().find(|r| r.id == id && r.tenant_id == t).map(|r| ContainerRow {
            id: r.id, tenant_id: r.tenant_id, status: r.status.clone(),
            departed_at: r.departed_at, estimated_arrival: r.estimated_arrival,
            master_awbs: r.master_awbs.clone(),
        }))
    }
    async fn shipment_ids_for_container(&self, _: Uuid, _: Uuid) -> anyhow::Result<Vec<Uuid>> { Ok(vec![]) }
    async fn mark_departed(&self, id: Uuid, t: Uuid, _: Option<chrono::DateTime<chrono::Utc>>) -> anyhow::Result<()> {
        if self.rows.iter().any(|r| r.id == id && r.tenant_id == t) { self.w.note("ops.depart"); Ok(()) }
        else { anyhow::bail!("not found") }
    }
    async fn create(&self, _: Uuid, _: Uuid, _: &str, _: Uuid, _: Uuid, _: Option<&str>) -> anyhow::Result<()> {
        self.w.note("ops.create"); Ok(())
    }
    async fn resolve_awbs_to_shipment_ids(&self, _: Uuid, _: &[String]) -> anyhow::Result<Vec<Uuid>> { Ok(vec![]) }
    async fn add_pallet(&self, _: Uuid, _: Uuid, _: Uuid, _: &[String]) -> anyhow::Result<()> { self.w.note("ops.add_pallet"); Ok(()) }
    async fn add_loose_piece(&self, _: Uuid, _: Uuid, _: &str, _: &str) -> anyhow::Result<()> { self.w.note("ops.add_loose"); Ok(()) }
    async fn finalise(&self, id: Uuid, t: Uuid) -> anyhow::Result<()> {
        if self.rows.iter().any(|r| r.id == id && r.tenant_id == t) { self.w.note("ops.finalise"); Ok(()) }
        else { anyhow::bail!("not found") }
    }
    async fn delete_planning(&self, _: Uuid, _: Uuid) -> anyhow::Result<()> { self.w.note("ops.delete"); Ok(()) }
}

// ── harness ──────────────────────────────────────────────────────────────────

struct World {
    router: Router,
    state: AppState,
    jwt: Arc<JwtService>,
    w: Arc<Counters>,
    /// Tenant that owns every seeded object.
    owner: Uuid,
    hub: Uuid,
    pallet: Uuid,
    container: Uuid,
    location: Uuid,
    inventory: Uuid,
}

fn world() -> World {
    let w = Arc::new(Counters::default());
    let owner = TenantId::new();
    let hub = Hub::new(owner.clone(), "H".into(), "a".into(), 0.0, 0.0, 10);
    let other_hub = logisticos_types::HubId::new();
    let pallet = Pallet::new(owner.clone(), logisticos_types::HubId::from_uuid(hub.id.inner()), None);
    let container = Container::new(
        owner.clone(), TransportMode::Road,
        logisticos_types::HubId::from_uuid(hub.id.inner()), other_hub,
    );
    let manifest = HubTransferManifest::new(
        owner.clone(), container.id.clone(), container.origin_hub_id.clone(),
        container.destination_hub.clone(), container.transport_mode,
    );
    let location = HubLocation::new(
        owner.clone(), logisticos_types::HubId::from_uuid(hub.id.inner()), "Z1", "A-1",
    );
    let inventory = HubInventory::check_in(
        owner.clone(), logisticos_types::HubId::from_uuid(hub.id.inner()), location.id,
        InventoryUnit::Consolidated(pallet.id.clone()), Uuid::new_v4(),
    );
    let row = ContainerRow {
        id: container.id.inner(), tenant_id: owner.inner(), status: "manifested".into(),
        departed_at: None, estimated_arrival: None, master_awbs: vec![],
    };

    let ids = (hub.id.inner(), pallet.id.inner(), container.id.inner(), location.id, inventory.id);
    let hub_repo: Arc<dyn HubRepository> = Arc::new(FakeHubs { hubs: vec![hub], w: w.clone() });
    let pallet_repo: Arc<dyn PalletRepository> = Arc::new(FakePallets { items: vec![pallet], w: w.clone() });
    let ctr_repo: Arc<dyn ContainerRepository> = Arc::new(FakeContainers { items: vec![container], w: w.clone() });
    let kafka = Arc::new(KafkaProducer::new("127.0.0.1:1").expect("producer construction is lazy"));
    let bc = HubBroadcaster::new();

    let state = AppState {
        svc: Arc::new(HubService::new(hub_repo.clone(), Arc::new(FakeInductions))),
        containers: Arc::new(FakeOps { rows: vec![row], w: w.clone() }),
        consolidation_svc: Arc::new(ConsolidationService::new(Arc::new(NoSpecs), Arc::new(NoPlans), bc.clone())),
        pallet_svc: Arc::new(PalletService::new(pallet_repo.clone(), ctr_repo.clone(), kafka.clone())),
        hub_transfer_svc: Arc::new(HubTransferService::new(
            hub_repo, pallet_repo, ctr_repo,
            Arc::new(FakeScans { w: w.clone() }),
            Arc::new(FakeLocations { items: vec![location.clone()], w: w.clone() }),
            Arc::new(FakeInventory { items: vec![inventory], locations: vec![location], w: w.clone() }),
            Arc::new(FakeManifests { items: vec![manifest], w: w.clone() }),
            Arc::new(FakeRouting { w: w.clone() }),
            kafka.clone(),
        )),
        hub_broadcaster: bc,
        kafka,
        payments_url: String::new(),
        pod_url: String::new(),
        order_intake_url: String::new(),
        pool: sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused@127.0.0.1:1/unused")
            .expect("lazy pool"),
    };
    let jwt = Arc::new(JwtService::new(SECRET, 3600, 86400));
    World {
        router: build_router(state.clone(), jwt.clone()),
        state, jwt, w, owner: owner.inner(),
        hub: ids.0, pallet: ids.1, container: ids.2, location: ids.3, inventory: ids.4,
    }
}

fn token(w: &World, tenant: Uuid) -> String {
    let claims = Claims::new(
        Uuid::new_v4(), tenant, "t".into(), "enterprise".into(), "u@test.local".into(),
        vec!["admin".into()],
        ["fleet:read", "fleet:manage", "shipments:read", "shipments:update", "shipments:create"]
            .iter().map(|s| s.to_string()).collect(),
        3600,
    );
    w.jwt.issue_access_token(claims).expect("token")
}

async fn call(w: &World, method: &str, uri: &str, tenant: Uuid, body: Option<Value>) -> StatusCode {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {}", token(w, tenant)));
    let body = match body {
        Some(b) => { req = req.header(header::CONTENT_TYPE, "application/json"); Body::from(b.to_string()) }
        None => Body::empty(),
    };
    w.router.clone().oneshot(req.body(body).expect("request")).await.expect("response").status()
}

fn stranger() -> Uuid { Uuid::new_v4() }

// ── cross-tenant reads: 404 ──────────────────────────────────────────────────

#[tokio::test]
async fn foreign_tenant_reads_are_404() {
    let w = world();
    let x = stranger();
    let cases = [
        format!("/v1/pallets/{}", w.pallet),
        format!("/v1/containers/{}/manifest", w.container),
        format!("/v1/hubs/{}/manifest", w.hub),
        format!("/v1/hubs/{}/customs-queue", w.hub),
        format!("/v1/hub-transfer/locations?hub_id={}", w.hub),
        format!("/v1/hub-transfer/inventory?location_id={}", w.location),
        format!("/v1/hub-transfer/routing-config?hub_id={}", w.hub),
        format!("/v1/hubs/{}/consolidation/plans", w.hub),
        format!("/v1/hubs/{}", w.hub),
    ];
    for uri in cases {
        assert_eq!(call(&w, "GET", &uri, x, None).await, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn owner_reads_still_work() {
    let w = world();
    for uri in [
        format!("/v1/pallets/{}", w.pallet),
        format!("/v1/containers/{}/manifest", w.container),
        format!("/v1/hubs/{}/manifest", w.hub),
        format!("/v1/hub-transfer/locations?hub_id={}", w.hub),
        format!("/v1/hub-transfer/inventory?location_id={}", w.location),
    ] {
        assert_eq!(call(&w, "GET", &uri, w.owner, None).await, StatusCode::OK, "{uri}");
    }
}

// ── cross-tenant writes: 404 and nothing written ─────────────────────────────

/// Every mutating hub-ops route with a body that deserialises, so the request
/// reaches the handler body (extraction failures would answer 422 first).
fn write_cases(w: &World) -> Vec<(&'static str, String, Option<Value>)> {
    let (hub, pallet, ctr, loc, inv) = (w.hub, w.pallet, w.container, w.location, w.inventory);
    let piece = json!({"piece_awb": "x", "master_awb": "y", "declared_weight_g": 1});
    let id = Uuid::new_v4();
    vec![
        ("POST", "/v1/hub-transfer/scans".into(), Some(json!({
            "hub_id": hub, "piece_awb": "a", "master_awb": "b", "shipment_id": Uuid::new_v4(),
            "scan_type": "inbound_receive", "device_timestamp": "2026-01-01T00:00:00Z"}))),
        ("POST", "/v1/hubs/sort".into(), Some(json!({"induction_id": id, "zone": "z", "bay": "b"}))),
        ("POST", format!("/v1/hubs/dispatch/{id}"), None),
        ("POST", "/v1/consolidation/specs".into(), Some(json!({
            "name": "n", "transport_mode": "road", "size_class": "s",
            "interior_length_cm": 1, "interior_width_cm": 1, "interior_height_cm": 1, "max_payload_kg": 1.0}))),
        ("PUT", format!("/v1/consolidation/specs/{id}"), Some(json!({}))),
        ("PUT", format!("/v1/consolidation/plans/{id}/placements"), Some(json!({"placements": []}))),
        ("POST", format!("/v1/consolidation/plans/{id}/confirm"), Some(json!({"destination_hub_id": hub, "transport_mode": "road"}))),
        ("POST", format!("/v1/consolidation/plans/{id}/scan"), Some(json!({"awb": "a"}))),
        ("POST", "/v1/hubs".into(), Some(json!({"name": "h", "address": "a", "lat": 0.0, "lng": 0.0, "capacity": 1, "serving_zones": []}))),
        ("PUT",  format!("/v1/containers/{ctr}/depart"), Some(json!({}))),
        ("POST", format!("/v1/containers/{ctr}/finalise"), None),
        ("POST", format!("/v1/containers/{ctr}/load-pallet"), Some(json!({"pallet_id": pallet, "master_awbs": []}))),
        ("POST", format!("/v1/containers/{ctr}/load-loose-piece"), Some(json!({"piece_awb": "a", "master_awb": "b"}))),
        ("POST", format!("/v1/pallets/{pallet}/seal"), None),
        ("POST", format!("/v1/pallets/{pallet}/load"), Some(json!({"container_id": ctr}))),
        ("POST", format!("/v1/containers/{ctr}/arrive"), None),
        ("POST", format!("/v1/containers/{ctr}/arrive-at-port"), Some(json!({}))),
        ("POST", format!("/v1/containers/{ctr}/enter-customs"), Some(json!({}))),
        ("POST", format!("/v1/containers/{ctr}/clear-customs"), Some(json!({}))),
        ("POST", format!("/v1/containers/{ctr}/release-domestic"), None),
        ("POST", format!("/v1/containers/{ctr}/deconsolidate"), Some(json!({"destination_zone": "z"}))),
        ("POST", "/v1/hub-transfer/inventory/move".into(), Some(json!({"inventory_id": inv, "to_location_id": loc}))),
        ("POST", format!("/v1/hubs/{hub}/pieces"), Some(piece)),
        ("POST", "/v1/containers".into(), Some(json!({"transport_mode": "road", "origin_hub_id": hub, "destination_hub_id": hub}))),
        ("POST", "/v1/hub-transfer/locations".into(), Some(json!({"hub_id": hub, "zone_id": "z", "location_tag": "t"}))),
        ("POST", "/v1/hub-transfer/inventory".into(), Some(json!({"hub_id": hub, "location_id": loc, "unit_kind": "consolidated", "unit_ref": pallet.to_string()}))),
        ("POST", "/v1/hub-transfer/routing-config".into(), Some(json!({"hub_id": hub, "destination_zone": "z", "routing_type": "own_driver"}))),
        ("POST", "/v1/hubs/induct".into(), Some(json!({"hub_id": hub, "shipment_id": Uuid::new_v4(), "tracking_number": "T"}))),
        ("POST", "/v1/consolidation/plans".into(), Some(json!({"hub_id": hub, "truck_spec_id": Uuid::new_v4(), "items": []}))),
    ]
}

/// Cases at the head of `write_cases` that are not hub-scoped (specs, hub
/// creation, ...) and so have no "foreign tenant" form.
const NON_TENANT_CASES: usize = 9;

#[tokio::test]
async fn foreign_tenant_writes_are_404_and_write_nothing() {
    let w = world();
    let x = stranger();
    for (m, uri, body) in write_cases(&w).into_iter().skip(NON_TENANT_CASES) {
        let st = call(&w, m, &uri, x, body).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "{m} {uri}");
    }
    assert_eq!(w.w.count(), 0, "a foreign tenant wrote: {:?}", w.w.writes.lock().unwrap());
}

/// A partner-bound token (carries a `pid`) is refused on every mutating route,
/// even with the permissions and the tenant of the data's real owner.
#[tokio::test]
async fn partner_bound_token_is_403_on_every_mutating_route_and_writes_nothing() {
    let w = world();
    let claims = Claims::new(
        Uuid::new_v4(), w.owner, "t".into(), "enterprise".into(), "p@test.local".into(),
        vec!["admin".into()],
        ["fleet:read", "fleet:manage", "shipments:read", "shipments:update", "shipments:create"]
            .iter().map(|s| s.to_string()).collect(),
        3600,
    ).with_partner(Some(Uuid::new_v4()));
    let tok = w.jwt.issue_access_token(claims).expect("token");
    let cases = write_cases(&w);
    assert!(cases.len() >= 29, "route list shrank: {}", cases.len());
    for (m, uri, body) in cases {
        let mut req = Request::builder().method(m).uri(&uri)
            .header(header::AUTHORIZATION, format!("Bearer {tok}"));
        let body = match body {
            Some(b) => { req = req.header(header::CONTENT_TYPE, "application/json"); Body::from(b.to_string()) }
            None => Body::empty(),
        };
        let st = w.router.clone().oneshot(req.body(body).expect("req")).await.expect("resp").status();
        assert_eq!(st, StatusCode::FORBIDDEN, "{m} {uri}");
    }
    assert_eq!(w.w.count(), 0, "a partner token wrote: {:?}", w.w.writes.lock().unwrap());
}

/// The invoice tenant code is derived from the JWT slug; a body value is ignored.
#[test]
fn clear_customs_tenant_code_comes_from_the_jwt_not_the_body() {
    let claims = |slug: &str| Claims::new(
        Uuid::new_v4(), Uuid::new_v4(), slug.into(), "enterprise".into(), "u@test.local".into(),
        vec!["admin".into()], vec![], 3600,
    );
    assert_eq!(tenant_code_from_claims(&claims("acme")), "ACM");
    assert_eq!(tenant_code_from_claims(&claims("oslo-ops")), "SLP");
    assert_eq!(tenant_code_from_claims(&claims("")), "");
    // A body that still sends tenant_code deserialises; its value is never read.
    let req: ClearCustomsRequest =
        serde_json::from_value(json!({"tenant_code": "EVL", "duties_total_cents": 5})).expect("body");
    assert_eq!(req.duties_total_cents, Some(5));
    let none: ClearCustomsRequest = serde_json::from_value(json!({})).expect("empty body");
    assert!(none.details.is_empty());
}

/// The body-supplied ids must be checked too: the container is the caller's
/// but the pallet is not, and the hubs on a new container are not.
#[tokio::test]
async fn foreign_ids_inside_an_own_request_are_404() {
    let w = world();
    let x = stranger();
    // x owns nothing, so seed x its own container via a second world is overkill:
    // use the owner's container id with the owner token and a foreign pallet id.
    let st = call(
        &w, "POST", &format!("/v1/containers/{}/load-pallet", w.container), w.owner,
        Some(json!({"pallet_id": Uuid::new_v4(), "master_awbs": []})),
    ).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let st = call(
        &w, "POST", "/v1/containers", w.owner,
        Some(json!({"transport_mode": "road", "origin_hub_id": w.hub, "destination_hub_id": Uuid::new_v4()})),
    ).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let st = call(
        &w, "POST", "/v1/hub-transfer/inventory/move", w.owner,
        Some(json!({"inventory_id": w.inventory, "to_location_id": Uuid::new_v4()})),
    ).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let _ = x;
    assert_eq!(w.w.count(), 0, "{:?}", w.w.writes.lock().unwrap());
}

#[tokio::test]
async fn owner_can_still_mutate_own_container() {
    let w = world();
    assert_eq!(call(&w, "POST", &format!("/v1/containers/{}/finalise", w.container), w.owner, None).await, StatusCode::OK);
    assert_eq!(w.w.writes.lock().unwrap().as_slice(), ["ops.finalise"]);
    let st = call(
        &w, "POST", "/v1/hub-transfer/inventory/move", w.owner,
        Some(json!({"inventory_id": w.inventory, "to_location_id": w.location})),
    ).await;
    assert_eq!(st, StatusCode::OK);
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let w = world();
    let req = Request::builder().uri(format!("/v1/pallets/{}", w.pallet)).body(Body::empty()).expect("req");
    assert_eq!(w.router.clone().oneshot(req).await.expect("resp").status(), StatusCode::UNAUTHORIZED);
}

// ── WebSocket subscription gate ──────────────────────────────────────────────

#[tokio::test]
async fn hub_websocket_refuses_foreign_tenant_hub() {
    let w = world();
    let bad = authorize_hub_ws(&w.state, &w.jwt, w.hub, &token(&w, stranger())).await;
    assert_eq!(bad.expect_err("foreign hub must be refused").status(), StatusCode::NOT_FOUND);
    let none = authorize_hub_ws(&w.state, &w.jwt, w.hub, "garbage").await;
    assert_eq!(none.expect_err("bad token").status(), StatusCode::UNAUTHORIZED);
    assert!(authorize_hub_ws(&w.state, &w.jwt, w.hub, &token(&w, w.owner)).await.is_ok());
}

// ── mutating routes require a permission ─────────────────────────────────────

#[tokio::test]
async fn mutating_routes_refuse_a_read_only_token() {
    let w = world();
    let claims = Claims::new(
        Uuid::new_v4(), w.owner, "t".into(), "enterprise".into(), "u@test.local".into(),
        vec!["viewer".into()], vec!["shipments:read".into(), "fleet:read".into()], 3600,
    );
    let tok = w.jwt.issue_access_token(claims).expect("token");
    for (m, uri) in [
        ("PUT", format!("/v1/containers/{}/depart", w.container)),
        ("POST", format!("/v1/containers/{}/finalise", w.container)),
        ("POST", format!("/v1/pallets/{}/seal", w.pallet)),
        ("POST", format!("/v1/containers/{}/clear-customs", w.container)),
        ("POST", "/v1/hub-transfer/inventory/move".to_string()),
    ] {
        let req = Request::builder().method(m).uri(&uri)
            .header(header::AUTHORIZATION, format!("Bearer {tok}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"inventory_id": w.inventory, "to_location_id": w.location}).to_string())).expect("req");
        let st = w.router.clone().oneshot(req).await.expect("resp").status();
        assert_eq!(st, StatusCode::FORBIDDEN, "{m} {uri}");
    }
    assert_eq!(w.w.count(), 0);
}

// ── SQL statement scan ───────────────────────────────────────────────────────

/// String literals that look like hub_ops SQL (`SELECT|UPDATE|DELETE|INSERT`
/// against `hub_ops.`) in the three files that hold it.
fn sql_literals(src: &str) -> Vec<String> {
    let cleaned: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let production = cleaned.split("#[cfg(test)]").next().unwrap_or("").to_owned();
    let b = production.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if production[i..].starts_with("r#\"") {
            let start = i + 3;
            if let Some(end) = production[start..].find("\"#") {
                out.push(production[start..start + end].to_owned());
                i = start + end + 2;
                continue;
            }
        } else if b[i] == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                if b[j] == b'\\' { j += 1; }
                j += 1;
            }
            out.push(production[i + 1..j.min(b.len())].to_owned());
            i = j + 1;
            continue;
        }
        i += 1;
    }
    out.into_iter()
        .filter(|l| {
            let t = l.trim_start().to_ascii_uppercase();
            l.contains("hub_ops.")
                && (t.starts_with("SELECT") || t.starts_with("UPDATE")
                    || t.starts_with("DELETE") || t.starts_with("INSERT"))
        })
        .collect()
}

#[test]
fn every_hub_ops_statement_mentions_tenant_id() {
    let files = [
        ("bootstrap.rs", include_str!("bootstrap.rs")),
        ("infrastructure/db/mod.rs", include_str!("infrastructure/db/mod.rs")),
        ("infrastructure/db/hub_transfer.rs", include_str!("infrastructure/db/hub_transfer.rs")),
    ];
    let mut total = 0;
    let mut missing = Vec::new();
    for (name, src) in files {
        for lit in sql_literals(src) {
            total += 1;
            if !lit.contains("tenant_id") {
                missing.push(format!("{name}: {}", lit.split_whitespace().collect::<Vec<_>>().join(" ")));
            }
        }
    }
    assert!(total > 40, "scanner found only {total} statements — it has stopped seeing the SQL");
    assert!(missing.is_empty(), "statements with no tenant_id:\n{}", missing.join("\n"));
}
