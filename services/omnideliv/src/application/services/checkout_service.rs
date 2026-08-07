//! Checkout — the commit path.
//!
//! Deliberately not reachable from any agent tool. The mesh proposes; a human
//! tap commits. Everything here moves money or dispatches a courier, which is
//! exactly the set of actions no `AgentRole` is permitted to reach.

use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{
    Basket, ConsolidationPlan, Order, PendingStop, TemperatureClass, VendorLeg,
};
use crate::domain::repositories::{BasketRepository, VendorRepository};

#[derive(Debug, thiserror::Error)]
pub enum CheckoutError {
    #[error("basket {0} not found")]
    BasketNotFound(Uuid),
    #[error("basket has {0} line(s) awaiting review — the customer must decide first")]
    AwaitingReview(usize),
    #[error("basket is empty")]
    EmptyBasket,
    #[error("vendor {0} is no longer orderable")]
    VendorUnavailable(Uuid),
    #[error("no courier available")]
    NoCourier,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Placing an order requires a courier. The trait keeps `services/omnideliv`
/// from depending on field-ops types directly — a product service calling a
/// platform service through an interface it owns, not the reverse.
#[async_trait::async_trait]
pub trait CourierDispatch: Send + Sync {
    /// Offer the job to nearby couriers. Returns the assignment ids offered.
    ///
    /// The earning travels with the offer: field-ops credits the courier on
    /// delivery from what we declare here, because pricing is ours and a
    /// platform tier that computed pay would need every product's tariff.
    #[allow(clippy::too_many_arguments)]
    async fn offer(
        &self,
        tenant_id: Uuid,
        order_id: Uuid,
        lat: f64,
        lng: f64,
        trip_cents: i64,
        tip_cents: i64,
    ) -> anyhow::Result<Vec<Uuid>>;
}

pub struct CheckoutService {
    baskets:  Arc<dyn BasketRepository>,
    vendors:  Arc<dyn VendorRepository>,
    dispatch: Arc<dyn CourierDispatch>,
}

impl CheckoutService {
    pub fn new(
        baskets: Arc<dyn BasketRepository>,
        vendors: Arc<dyn VendorRepository>,
        dispatch: Arc<dyn CourierDispatch>,
    ) -> Self {
        Self { baskets, vendors, dispatch }
    }

    /// Place an order from a reviewed basket.
    ///
    /// Order of operations matters. The basket is validated, vendors re-checked
    /// and legs computed *before* anything irreversible happens, so a failure
    /// here leaves no money moved and no courier dispatched.
    pub async fn place(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        tip_cents: i64,
        delivery_lat: f64,
        delivery_lng: f64,
    ) -> Result<Order, CheckoutError> {
        let basket: Basket = self
            .baskets
            .find_by_id(tenant_id, basket_id)
            .await
            .map_err(CheckoutError::Other)?
            .ok_or(CheckoutError::BasketNotFound(basket_id))?;

        // The customer must resolve every substitution first — Screen C exists
        // precisely so this cannot be silently skipped.
        let pending = basket.lines_awaiting_review().len();
        if pending > 0 {
            return Err(CheckoutError::AwaitingReview(pending));
        }

        // Sorted by vendor id: `subtotals_by_vendor` returns a HashMap, whose
        // iteration order varies between runs. Leaving it unsorted would give
        // an order's legs a different sequence on every placement, which makes
        // the persisted rows and any test over them nondeterministic for no
        // reason. It does not affect the money — the sums are order-independent.
        let mut subtotals: Vec<(Uuid, i64)> = basket.subtotals_by_vendor().into_iter().collect();
        subtotals.sort_by_key(|(vendor_id, _)| *vendor_id);

        if subtotals.is_empty() {
            return Err(CheckoutError::EmptyBasket);
        }

        // Re-check every vendor at commit time. A vendor that paused since the
        // basket was assembled must not receive a dispatched courier.
        let mut legs = Vec::with_capacity(subtotals.len());
        let mut stops = Vec::with_capacity(subtotals.len());

        for (vendor_id, subtotal) in &subtotals {
            let vendor = self
                .vendors
                .find_by_id(tenant_id, *vendor_id)
                .await
                .map_err(CheckoutError::Other)?
                .ok_or(CheckoutError::VendorUnavailable(*vendor_id))?;

            if !vendor.is_orderable() {
                return Err(CheckoutError::VendorUnavailable(*vendor_id));
            }

            legs.push(VendorLeg::settle(tenant_id, vendor.id, *subtotal, vendor.commission_bps));
            stops.push(PendingStop {
                vendor_id:         vendor.id,
                prep_time_minutes: vendor.prep_time_minutes,
                temperature_class: temperature_for(&vendor),
            });
        }

        // Placeholder pricing until a tariff service owns it. Visible and
        // testable here rather than hidden behind a stub.
        //
        // The fee rises less per extra stop than the courier cost does, which is
        // the consolidation margin working as intended — but note it does rise,
        // so `flat` means flat per order, not identical across baskets.
        let flat_fee_cents = 4_900 + (stops.len() as i64 - 1).max(0) * 1_000;
        let courier_trip_cents = 3_500 + (stops.len() as i64 - 1).max(0) * 700;

        let plan = ConsolidationPlan::sequence(tenant_id, basket.id, stops, 0, flat_fee_cents);

        let mut order = Order::place(
            tenant_id, basket.customer_id, basket.id, plan.id,
            legs, flat_fee_cents, tip_cents, courier_trip_cents,
        );

        // Only now does anything irreversible happen.
        let offered = self
            .dispatch
            .offer(tenant_id, order.id, delivery_lat, delivery_lng,
                   order.courier_trip_cents, order.tip_cents)
            .await
            .map_err(CheckoutError::Other)?;

        if offered.is_empty() {
            // No charge, no order. Better to tell the customer now than to take
            // payment for a delivery nobody can make.
            return Err(CheckoutError::NoCourier);
        }

        order.courier_task_id = offered.first().copied();
        Ok(order)
    }
}

/// A vendor's temperature class, from its vertical. Coarse but honest: a
/// per-item classification needs a `temperature_class` column on catalog_items,
/// which is a catalog change rather than a checkout one.
fn temperature_for(vendor: &crate::domain::entities::Vendor) -> TemperatureClass {
    use crate::domain::entities::Vertical::*;
    match vendor.vertical {
        Restaurant => TemperatureClass::Hot,
        Grocery | Florist => TemperatureClass::Chilled,
        Pharmacy | Retail => TemperatureClass::Ambient,
    }
}
