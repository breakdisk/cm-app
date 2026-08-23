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

/// How far the first offer reaches. The recovery sweep widens from here.
pub const FIRST_OFFER_RADIUS_KM: f64 = 5.0;

/// Read-only capacity, for deciding whether a delivery can be promised.
///
/// Separate from `CourierDispatch` because the callers are different: this is
/// what the Fleet agent asks while planning, and it must never be able to
/// dispatch as a side effect of asking.
#[async_trait::async_trait]
pub trait CourierSupply: Send + Sync {
    /// Couriers who could take a job at this point right now.
    async fn available_near(
        &self,
        tenant_id: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
    ) -> anyhow::Result<usize>;
}

/// One pickup, reduced to what a courier needs before they commit.
pub struct CardStop {
    pub vendor_name: String,
    pub vertical:    String,
    pub temperature: String,
}

/// The pre-claim summary handed to field-ops as an opaque blob.
///
/// Names businesses but never the customer. `offer_to_nearest` fans out to the
/// nearest N couriers, so every field here reaches people who will decline this
/// job: a vendor is a public storefront, a delivery address is not.
pub fn build_offer_card(
    stops: &[CardStop],
    distance_m: i64,
    deadline_hint_mins: i64,
) -> serde_json::Value {
    // Deduplicated, order-preserving. Two restaurants must not read as a more
    // varied run than it is, and the first vertical is the one the courier will
    // reach first.
    let mut verticals: Vec<&str> = Vec::new();
    let mut temperature: Vec<&str> = Vec::new();
    for s in stops {
        if !verticals.contains(&s.vertical.as_str()) {
            verticals.push(&s.vertical);
        }
        if !temperature.contains(&s.temperature.as_str()) {
            temperature.push(&s.temperature);
        }
    }

    serde_json::json!({
        // Bumped only on a breaking change. The app renders defensively on an
        // unknown version rather than failing to draw the offer at all.
        "v": 1,
        // Pickups plus the single dropoff. A courier counts doors, not legs.
        "stops": stops.len() + 1,
        "pickups": stops.len(),
        "distance_m": distance_m,
        "deadline_hint_mins": deadline_hint_mins,
        "vendors": stops.iter().map(|s| s.vendor_name.clone()).collect::<Vec<_>>(),
        "verticals": verticals,
        "temperature": temperature,
    })
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
    ///
    /// `radius_km` is explicit so a retry can widen the search. The first offer
    /// goes near; an order nobody took goes wider, which is the only lever
    /// available before giving up and calling a human.
    ///
    /// `cod_amount_cents` is the cash the courier collects at the door. Today
    /// it is the order's grand total, because every OmniDeliv order is
    /// cash-on-delivery. It becomes 0 for prepaid orders when that rail exists
    /// — an amount rather than a payment-method flag, so that change is a value
    /// change and not a new branch through dispatch.
    ///
    /// `offer_card` is the pre-claim summary a courier judges the job by. It is
    /// opaque to field-ops, which stores and returns it without reading it, and
    /// it is disclosed to every courier in the fan-out -- so it must never carry
    /// the customer's identity or any street address. `None` is legal: the card
    /// is an affordance, not a precondition for dispatch.
    async fn offer(
        &self,
        tenant_id: Uuid,
        order_id: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
        trip_cents: i64,
        tip_cents: i64,
        cod_amount_cents: i64,
        offer_card: Option<serde_json::Value>,
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
    ///
    /// `customer_login` and `customer_phone_claim` both come from the validated
    /// token and never from the request body — a client-supplied contact would
    /// let anyone put an arbitrary phone number on an order, and that is a
    /// number a courier would then call.
    ///
    /// The phone claim is preferred; the login is only decoded when the token
    /// predates identity carrying the number.
    #[allow(clippy::too_many_arguments)]
    pub async fn place(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        tip_cents: i64,
        delivery_lat: f64,
        delivery_lng: f64,
        customer_login: &str,
        customer_phone_claim: Option<&str>,
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
        // Built from the same vendor read, so the card cannot describe a
        // different set of stops from the one the order is placed with.
        let mut card_stops = Vec::with_capacity(subtotals.len());

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
            let temp = temperature_for(&vendor);
            card_stops.push(CardStop {
                vendor_name: vendor.name.clone(),
                vertical:    vendor.vertical.as_str().to_string(),
                temperature: temp.as_str().to_string(),
            });
            stops.push(PendingStop {
                vendor_id:         vendor.id,
                prep_time_minutes: vendor.prep_time_minutes,
                temperature_class: temp,
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

        // The courier's only way to reach the customer. `phone_from_login`
        // returns `None` for a real mailbox, so an account created any other
        // way leaves the manifest without a number rather than showing the
        // courier an email local-part to dial.
        let customer_phone = crate::domain::entities::order::contact_phone(
            customer_phone_claim,
            customer_login,
        );

        let mut order = Order::place(
            tenant_id, basket.customer_id, basket.id, plan.id,
            legs, flat_fee_cents, tip_cents, courier_trip_cents,
            delivery_lat, delivery_lng,
        )
        // No display name on the OTP path — identity has none to give, and a
        // fabricated one would be worse than an honest blank.
        .with_customer_contact(None, customer_phone);

        // Only now does anything irreversible happen.
        // The longest prep time is the earliest the courier can expect to leave
        // the last pickup, which is the only deadline signal available before a
        // routing service exists. Stated as a hint, and named one.
        let deadline_hint_mins =
            plan.stops.iter().map(|s| s.prep_time_minutes as i64).max().unwrap_or(0);
        let card = build_offer_card(&card_stops, plan.total_distance_m as i64, deadline_hint_mins);

        let offered = self
            .dispatch
            .offer(tenant_id, order.id, delivery_lat, delivery_lng, FIRST_OFFER_RADIUS_KM,
                   order.courier_trip_cents, order.tip_cents,
                   // COD: the customer pays the whole thing at the door.
                   order.grand_total_cents,
                   Some(card))
            .await
            .map_err(CheckoutError::Other)?;

        if offered.is_empty() {
            // No charge, no order. Better to tell the customer now than to take
            // payment for a delivery nobody can make.
            return Err(CheckoutError::NoCourier);
        }

        order.courier_task_id = offered.first().copied();

        // Offered, not yet claimed. Without this the order sits in `Placed`
        // until a courier accepts, and `AwaitingCourier` is a state nothing
        // ever enters — which would also hide the distinction the recovery
        // sweep needs between "we never managed to offer it" and "we offered
        // it and nobody took it".
        //
        // Infallible from `Placed`, and the sweep still catches an order left
        // in `Placed` anyway, so a failure here is logged rather than failing a
        // checkout whose courier is already offered.
        if let Err(e) = order.courier_offered() {
            tracing::error!(err = %e, order_id = %order.id, "could not mark the order awaiting a courier");
        }

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

#[cfg(test)]
mod offer_card {
    use super::*;

    fn stop(name: &str, vertical: &str, temp: &str) -> CardStop {
        CardStop {
            vendor_name: name.to_string(),
            vertical:    vertical.to_string(),
            temperature: temp.to_string(),
        }
    }

    /// Enough to judge the job: how much work, how far, what kind. The pay rides
    /// on the assignment itself, so it is deliberately not duplicated here.
    #[test]
    fn the_card_describes_the_shape_of_the_job() {
        let card = build_offer_card(
            &[
                stop("Kuya's Lutong Bahay", "restaurant", "hot"),
                stop("Mercury Drug", "pharmacy", "chilled"),
            ],
            4_200,
            38,
        );

        assert_eq!(card["v"], 1);
        assert_eq!(card["pickups"], 2);
        assert_eq!(card["stops"], 3, "two pickups plus the dropoff");
        assert_eq!(card["distance_m"], 4_200);
        assert_eq!(card["deadline_hint_mins"], 38);
        assert_eq!(card["verticals"], serde_json::json!(["restaurant", "pharmacy"]));
        assert_eq!(card["temperature"], serde_json::json!(["hot", "chilled"]));
        assert_eq!(
            card["vendors"],
            serde_json::json!(["Kuya's Lutong Bahay", "Mercury Drug"])
        );
    }

    /// The rule the fan-out forces, and the reason this function exists rather
    /// than the manifest being sent early. `offer_to_nearest` offers to N
    /// couriers; anything on the card is handed to everyone who was merely
    /// considered and declined. A vendor is a public storefront. A customer's
    /// address is not.
    #[test]
    fn the_card_discloses_nothing_about_the_customer() {
        let card = build_offer_card(
            &[stop("Kuya's Lutong Bahay", "restaurant", "hot")],
            1_200,
            15,
        );
        let text = serde_json::to_string(&card).unwrap().to_lowercase();

        for leaked in ["\"lat\"", "\"lng\"", "address", "customer", "phone", "notes"] {
            assert!(
                !text.contains(leaked),
                "the offer card must not carry `{leaked}` — it reaches couriers who decline"
            );
        }
    }

    /// Duplicates would tell a courier the run is more varied than it is: two
    /// restaurants is one kind of handling, not two.
    #[test]
    fn repeated_verticals_appear_once_but_pickups_still_count() {
        let card = build_offer_card(
            &[stop("Kuya's", "restaurant", "hot"), stop("Jollibee", "restaurant", "hot")],
            2_000,
            20,
        );
        assert_eq!(card["verticals"], serde_json::json!(["restaurant"]));
        assert_eq!(card["temperature"], serde_json::json!(["hot"]));
        assert_eq!(card["pickups"], 2, "deduplicating kinds must not lose stops");
    }

    /// A single-vendor order is the common case and must not read as zero work.
    #[test]
    fn one_pickup_is_still_two_stops() {
        let card = build_offer_card(&[stop("Kuya's", "restaurant", "hot")], 900, 12);
        assert_eq!(card["pickups"], 1);
        assert_eq!(card["stops"], 2);
    }
}
