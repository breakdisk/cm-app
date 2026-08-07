use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{CourierAssignment, CourierLocation, ProductKey};
use crate::domain::repositories::CourierRepository;
use crate::infrastructure::db::{AssignmentRepository, ClaimOutcome, LocationRepository};

pub struct DispatchService {
    couriers:    Arc<dyn CourierRepository>,
    assignments: Arc<dyn AssignmentRepository>,
    locations:   Arc<dyn LocationRepository>,
}

impl DispatchService {
    pub fn new(
        couriers: Arc<dyn CourierRepository>,
        assignments: Arc<dyn AssignmentRepository>,
        locations: Arc<dyn LocationRepository>,
    ) -> Self {
        Self { couriers, assignments, locations }
    }

    /// Offer a job to the nearest dispatchable couriers. Offering is not
    /// claiming — several couriers may hold an offer for the same job; exactly
    /// one will win the claim.
    #[allow(clippy::too_many_arguments)]
    pub async fn offer_to_nearest(
        &self,
        tenant_id: Uuid,
        product: ProductKey,
        external_ref: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
        fanout: i64,
    ) -> anyhow::Result<Vec<CourierAssignment>> {
        let candidates = self
            .couriers
            .find_available_near(tenant_id, lat, lng, radius_km, fanout)
            .await?;

        let mut offers = Vec::with_capacity(candidates.len());
        for c in candidates {
            // `product` is cloned per offer rather than copied: ProductKey owns
            // a String precisely so the set of products is not fixed at compile
            // time, and that ownership is worth one small allocation per offer
            // in a fan-out that is already doing a database write each turn.
            let a = CourierAssignment::offer(tenant_id, c.id, product.clone(), external_ref);
            self.assignments.save(&a).await?;
            offers.push(a);
        }
        Ok(offers)
    }

    /// A courier accepts an offer. Returns `false` when another courier got
    /// there first — the caller should show "already taken", not an error.
    pub async fn claim(&self, tenant_id: Uuid, assignment_id: Uuid) -> anyhow::Result<bool> {
        match self.assignments.try_claim(tenant_id, assignment_id).await? {
            ClaimOutcome::Won  => Ok(true),
            ClaimOutcome::Lost => Ok(false),
        }
    }

    pub async fn record_position(
        &self,
        tenant_id: Uuid,
        courier_id: Uuid,
        lat: f64,
        lng: f64,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<()> {
        // The breadcrumb is the authoritative write; it must land first, and a
        // failure here fails the call. What follows is a cache refresh.
        let fix = CourierLocation::new(tenant_id, courier_id, lat, lng, device_timestamp);
        self.locations.record(&fix).await?;

        // Refresh the render cache on the courier row. This is NOT what supply
        // lookup reads — `find_available_near` joins courier_latest_locations,
        // because only the GiST index there can serve ST_DWithin. These columns
        // exist so a courier list renders without touching the time-series
        // table, and nothing dispatch-critical may depend on them.
        if let Some(mut c) = self.couriers.find_by_id(tenant_id, courier_id).await? {
            c.record_position(lat, lng);
            self.couriers.save(&c).await?;
        }
        Ok(())
    }
}
