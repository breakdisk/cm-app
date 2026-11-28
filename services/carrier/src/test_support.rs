//! In-memory doubles and an `AppState` builder shared by the HTTP and MCP
//! tests. Test-only: nothing here is compiled into the service binary.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;
use logisticos_types::TenantId;
use uuid::Uuid;

use crate::application::services::{CarrierService, MarketplaceService};
use crate::domain::entities::{
    Carrier, CarrierId, MarketplaceBooking, SlaRecord, VehicleListing, ZoneSlaRow,
};
use crate::domain::repositories::{
    BreachReasonRow, CarrierRepository, MarketplaceRepository, SlaRecordRepository,
};
use crate::infrastructure::adapters::AdapterRegistry;
use crate::infrastructure::{external::ExternalCarrierClient, messaging::CarrierPublisher};
use crate::infrastructure::db::{CarrierBookingRecord, CarrierBookingStore};
use crate::AppState;

#[derive(Default)]
pub struct Carriers(pub Mutex<Vec<Carrier>>);

#[async_trait]
impl CarrierRepository for Carriers {
    async fn find_by_id(&self, id: &CarrierId) -> anyhow::Result<Option<Carrier>> {
        Ok(self.0.lock().unwrap().iter().find(|c| c.id == *id).cloned())
    }
    async fn find_by_code(&self, _t: &TenantId, _c: &str) -> anyhow::Result<Option<Carrier>> { Ok(None) }
    async fn find_by_contact_email(&self, _t: &TenantId, _e: &str) -> anyhow::Result<Option<Carrier>> { Ok(None) }
    async fn list(&self, _t: &TenantId, _l: i64, _o: i64) -> anyhow::Result<Vec<Carrier>> { Ok(vec![]) }
    async fn list_active(&self, _t: &TenantId) -> anyhow::Result<Vec<Carrier>> { Ok(vec![]) }
    async fn save(&self, c: &Carrier) -> anyhow::Result<()> {
        self.0.lock().unwrap().push(c.clone());
        Ok(())
    }
}

pub struct NoSla;

#[async_trait]
impl SlaRecordRepository for NoSla {
    async fn create(&self, _r: &SlaRecord) -> anyhow::Result<()> { Ok(()) }
    async fn find_by_shipment(&self, _s: Uuid) -> anyhow::Result<Option<SlaRecord>> { Ok(None) }
    async fn save_outcome(&self, _r: &SlaRecord) -> anyhow::Result<()> { Ok(()) }
    async fn list_by_carrier(&self, _c: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<SlaRecord>> { Ok(vec![]) }
    async fn zone_summary(&self, _c: Uuid, _f: chrono::DateTime<Utc>, _t: chrono::DateTime<Utc>) -> anyhow::Result<Vec<ZoneSlaRow>> { Ok(vec![]) }
    async fn breach_reasons(&self, _c: Uuid, _f: chrono::DateTime<Utc>, _t: chrono::DateTime<Utc>) -> anyhow::Result<Vec<BreachReasonRow>> { Ok(vec![]) }
}

/// Slice a vector the way SQL `LIMIT .. OFFSET ..` would.
fn page<T>(all: Vec<T>, limit: i64, offset: i64) -> Vec<T> {
    all.into_iter()
        .skip(usize::try_from(offset.max(0)).unwrap_or(0))
        .take(usize::try_from(limit.max(0)).unwrap_or(0))
        .collect()
}

#[derive(Default)]
pub struct Market {
    pub listings: Mutex<Vec<VehicleListing>>,
    pub bookings: Mutex<Vec<MarketplaceBooking>>,
}

#[async_trait]
impl MarketplaceRepository for Market {
    async fn create_listing(&self, l: &VehicleListing) -> anyhow::Result<()> {
        self.listings.lock().unwrap().push(l.clone());
        Ok(())
    }
    async fn find_listing_by_id(&self, _id: Uuid) -> anyhow::Result<Option<VehicleListing>> { Ok(None) }
    async fn list_listings_by_carrier(&self, _c: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<VehicleListing>> { Ok(vec![]) }
    async fn list_listings_by_tenant(&self, t: Uuid, l: i64, o: i64) -> anyhow::Result<Vec<VehicleListing>> {
        let all = self.listings.lock().unwrap().iter().filter(|x| x.tenant_id == t).cloned().collect();
        Ok(page(all, l, o))
    }
    async fn count_listings_by_tenant(&self, t: Uuid) -> anyhow::Result<i64> {
        let n = self.listings.lock().unwrap().iter().filter(|x| x.tenant_id == t).count();
        Ok(i64::try_from(n).unwrap_or(i64::MAX))
    }
    async fn update_listing(&self, _l: &VehicleListing) -> anyhow::Result<()> { Ok(()) }
    async fn delete_listing(&self, _id: Uuid) -> anyhow::Result<bool> { Ok(true) }
    async fn find_available_listings(
        &self, _t: Uuid, _w: f32, _s: Option<&str>, _at: chrono::DateTime<Utc>, _l: i64,
    ) -> anyhow::Result<Vec<VehicleListing>> { Ok(vec![]) }
    async fn create_booking(&self, b: &MarketplaceBooking) -> anyhow::Result<()> {
        self.bookings.lock().unwrap().push(b.clone());
        Ok(())
    }
    async fn save_booking_payment_reference(&self, _b: &MarketplaceBooking) -> anyhow::Result<()> { Ok(()) }
    async fn list_bookings_by_booker(&self, _t: Uuid, _u: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<MarketplaceBooking>> { Ok(vec![]) }
    async fn list_pending_bookings(&self, _l: i64) -> anyhow::Result<Vec<MarketplaceBooking>> { Ok(vec![]) }
    async fn find_booking_by_id(&self, _id: Uuid) -> anyhow::Result<Option<MarketplaceBooking>> { Ok(None) }
    async fn list_bookings_by_carrier(&self, _c: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<MarketplaceBooking>> { Ok(vec![]) }
    async fn list_bookings_by_tenant(&self, t: Uuid, l: i64, o: i64) -> anyhow::Result<Vec<MarketplaceBooking>> {
        let all = self.bookings.lock().unwrap().iter()
            .filter(|x| x.tenant_id == t && x.is_offered_to_carrier()).cloned().collect();
        Ok(page(all, l, o))
    }
    async fn count_bookings_by_tenant(&self, t: Uuid) -> anyhow::Result<i64> {
        let n = self.bookings.lock().unwrap().iter()
            .filter(|x| x.tenant_id == t && x.is_offered_to_carrier()).count();
        Ok(i64::try_from(n).unwrap_or(i64::MAX))
    }
    async fn save_booking(&self, _b: &MarketplaceBooking) -> anyhow::Result<()> { Ok(()) }
    async fn transition_booking(
        &self,
        b: &MarketplaceBooking,
        expected_status: &crate::domain::entities::BookingStatus,
        expected_payment: crate::domain::entities::BookingPaymentStatus,
    ) -> anyhow::Result<bool> {
        let mut all = self.bookings.lock().unwrap();
        match all.iter_mut().find(|x| {
            x.id == b.id && x.tenant_id == b.tenant_id
                && x.status == *expected_status && x.payment_status == expected_payment
        }) {
            Some(e) => { *e = b.clone(); Ok(true) }
            None => Ok(false),
        }
    }
}

/// In-memory booking log. Lookups are tenant-scoped exactly like the SQL.
#[derive(Default)]
pub struct Bookings(pub Mutex<Vec<(Uuid, String, String, String)>>);

impl Bookings {
    /// Record that `tenant` booked `booking_ref` / `tracking` with `carrier`.
    pub fn seed(&self, tenant: Uuid, carrier: &str, booking_ref: &str, tracking: &str) {
        self.0.lock().unwrap().push((tenant, carrier.into(), booking_ref.into(), tracking.into()));
    }
}

#[async_trait]
impl CarrierBookingStore for Bookings {
    async fn insert(&self, r: &CarrierBookingRecord) -> anyhow::Result<()> {
        self.seed(r.tenant_id, &r.carrier_code, &r.booking_ref, &r.tracking_number);
        Ok(())
    }
    async fn carrier_for_booking_ref(
        &self, t: Uuid, c: Option<&str>, r: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(self.0.lock().unwrap().iter()
            .find(|(bt, bc, br, _)| *bt == t && br == r && c.is_none_or(|c| c == bc))
            .map(|(_, bc, _, _)| bc.clone()))
    }
    async fn carrier_for_tracking_number(
        &self, t: Uuid, c: Option<&str>, n: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(self.0.lock().unwrap().iter()
            .find(|(bt, bc, _, bn)| *bt == t && bn == n && c.is_none_or(|c| c == bc))
            .map(|(_, bc, _, _)| bc.clone()))
    }
}

pub fn noop_kafka() -> Arc<KafkaProducer> {
    let cluster = rdkafka::mocking::MockCluster::new(1).expect("mock kafka cluster");
    let brokers = cluster.bootstrap_servers();
    Box::leak(Box::new(cluster));
    Arc::new(KafkaProducer::new(&brokers).expect("noop kafka producer"))
}

/// An `AppState` over in-memory repos, signing JWTs with `secret`.
pub fn app_state(
    carriers: Arc<Carriers>,
    market: Arc<Market>,
    adapters: AdapterRegistry,
    secret: &str,
    bookings: Option<Arc<Bookings>>,
) -> AppState {
    let kafka = noop_kafka();
    AppState {
        carrier_svc:      Arc::new(CarrierService::new(
            carriers,
            Arc::new(NoSla),
            Arc::new(CarrierPublisher::new(kafka.clone())),
            Arc::new(ExternalCarrierClient::new(None)),
        )),
        marketplace_svc:  Arc::new(MarketplaceService::new(market, kafka)),
        jwt:              Arc::new(JwtService::new(secret, 300, 600)),
        adapter_registry: Arc::new(adapters),
        storage:          None,
        booking_repo:     bookings.map(|b| b as Arc<dyn CarrierBookingStore>),
    }
}
