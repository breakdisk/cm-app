//! Checkout — the commit path.
//!
//! Deliberately not reachable from any agent tool. The mesh proposes; a human
//! tap commits. Everything here moves money or dispatches a courier, which is
//! exactly the set of actions no `AgentRole` is permitted to reach.

use std::sync::Arc;

use uuid::Uuid;

use crate::application::services::order_payments::OrderPayments;
use crate::domain::entities::{Fulfilment, 
    Basket, ConsolidationPlan, Order, PaymentMethod, PendingStop, TemperatureClass, VendorLeg,
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

/// What `place` hands back: the order it built, plus — for `PaymentMethod::Online`
/// only — the hosted-checkout URL the customer must complete before a courier
/// is ever offered the job. `None` for `Cod`, which never opens a gateway
/// session.
pub struct PlaceOutcome {
    pub order: Order,
    pub checkout_url: Option<String>,
}

pub struct CheckoutService {
    baskets:  Arc<dyn BasketRepository>,
    vendors:  Arc<dyn VendorRepository>,
    dispatch: Arc<dyn CourierDispatch>,
    payments: Arc<dyn OrderPayments>,
    /// The currency every `authorize` call is opened in. One value for the
    /// whole service rather than per-request: OmniDeliv has no multi-currency
    /// concept anywhere else in this crate (baskets, prices and fees are all
    /// bare cents with no currency tag), so introducing one only for the
    /// payments boundary would be a precision this feature does not need yet.
    currency: String,
    /// Base URL the customer's browser/WebView is redirected to after
    /// completing (or abandoning) the hosted checkout page — the gateway's
    /// `return_url`. `order.id` is appended as a query parameter so the
    /// landing page can look the order back up.
    payment_return_url_base: String,
}

impl CheckoutService {
    pub fn new(
        baskets: Arc<dyn BasketRepository>,
        vendors: Arc<dyn VendorRepository>,
        dispatch: Arc<dyn CourierDispatch>,
        payments: Arc<dyn OrderPayments>,
        currency: String,
        payment_return_url_base: String,
    ) -> Self {
        Self { baskets, vendors, dispatch, payments, currency, payment_return_url_base }
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
    ///
    /// `payment_method` decides everything after the order itself is built:
    /// `Cod` offers the job to couriers immediately, exactly as before this
    /// parameter existed. `Online` opens an authorization hold instead and
    /// defers the courier offer to the `payment.intent.authorized` consumer
    /// — see the module doc comment on `infrastructure::messaging::payment_consumer`
    /// for why that has to be a separate, asynchronous step rather than
    /// something this method can simply wait for.
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
        delivery_note: Option<&str>,
        payment_method: PaymentMethod,
        fulfilment: crate::domain::entities::Fulfilment,
    ) -> Result<PlaceOutcome, CheckoutError> {
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
        .with_customer_contact(None, customer_phone)
        // The one field the customer types. Cleaned and bounded here rather
        // than trusted: it reaches a courier's screen verbatim.
        .with_delivery_note(crate::domain::entities::order::clean_delivery_note(delivery_note));

        // Applied before anything irreversible: `for_dine_in` also strips the
        // delivery economics, and the database CHECK refuses a dine-in row that
        // still carries them. Doing it after the payment branch would fail the
        // save at the very end of checkout, with money already moved.
        if !fulfilment.needs_a_courier() {
            order = order.for_dine_in();
        }

        // Only now does anything irreversible happen.
        // The longest prep time is the earliest the courier can expect to leave
        // the last pickup, which is the only deadline signal available before a
        // routing service exists. Stated as a hint, and named one.
        let deadline_hint_mins =
            plan.stops.iter().map(|s| s.prep_time_minutes as i64).max().unwrap_or(0);
        let card = build_offer_card(&card_stops, plan.total_distance_m as i64, deadline_hint_mins);

        // A dine-in order has no courier leg at all: the food crosses a room.
        // Nothing is offered, so there is no NoCourier failure to handle and
        // the order simply stands placed, waiting on the kitchen.
        if !fulfilment.needs_a_courier() {
            return Ok(PlaceOutcome { order, checkout_url: None });
        }

        match payment_method {
            PaymentMethod::Cod => {
                // Byte-identical to every checkout before this feature existed:
                // `order.cod_amount_cents()` is `grand_total_cents - 0` here,
                // because `with_payment` is never called on this branch and
                // `prepaid_amount_cents` stays at `Order::place`'s default of 0.
                let offered = self
                    .dispatch
                    .offer(tenant_id, order.id, delivery_lat, delivery_lng, FIRST_OFFER_RADIUS_KM,
                           order.courier_trip_cents, order.tip_cents,
                           order.cod_amount_cents(),
                           Some(card))
                    .await
                    .map_err(CheckoutError::Other)?;

                if offered.is_empty() {
                    // No charge, no order. Better to tell the customer now than
                    // to take payment for a delivery nobody can make.
                    return Err(CheckoutError::NoCourier);
                }

                order.courier_task_id = offered.first().copied();

                // Offered, not yet claimed. Without this the order sits in
                // `Placed` until a courier accepts, and `AwaitingCourier` is a
                // state nothing ever enters — which would also hide the
                // distinction the recovery sweep needs between "we never
                // managed to offer it" and "we offered it and nobody took it".
                //
                // Infallible from `Placed`, and the sweep still catches an
                // order left in `Placed` anyway, so a failure here is logged
                // rather than failing a checkout whose courier is already
                // offered.
                if let Err(e) = order.courier_offered() {
                    tracing::error!(err = %e, order_id = %order.id,
                        "could not mark the order awaiting a courier");
                }

                Ok(PlaceOutcome { order, checkout_url: None })
            }

            PaymentMethod::Online => {
                // The whole order, prepaid — see the module-level design note
                // on partial prepay for why this call site is the only place
                // that currently ever passes anything other than 0 or the full
                // total to `with_payment`.
                let prepaid_amount_cents = order.grand_total_cents;
                order = order
                    .with_payment(PaymentMethod::Online, prepaid_amount_cents)
                    // Held for the `payment.intent.authorized` consumer, which
                    // offers the job with this exact card rather than trying to
                    // reconstruct one later from less information — see the
                    // field doc comment on `Order::pending_offer_card`.
                    .with_pending_offer_card(Some(card));

                let return_url = format!(
                    "{}?order_id={}",
                    self.payment_return_url_base.trim_end_matches('/'),
                    order.id,
                );

                // Ring-fence the funds; do NOT offer the job yet. The courier
                // offer is deferred to whenever `payment.intent.authorized`
                // actually lands — which may be seconds or minutes from now,
                // depending on how long the customer takes on the hosted
                // checkout page this call returns a URL for.
                let authorized = self
                    .payments
                    .authorize(tenant_id, order.id, order.grand_total_cents, &self.currency, &return_url)
                    .await
                    .map_err(CheckoutError::Other)?;

                order.payment_intent_id = Some(authorized.intent_id);
                // Persisted, not just returned. Returned-only, this URL existed
                // for exactly one HTTP response: a customer who left the hosted
                // page before paying — a call, a backgrounded app, a Back tap —
                // had an order that could never be paid for and no route back to
                // it. See `Order::resumable_checkout_url` for when it may be
                // handed out again.
                order.payment_checkout_url = Some(authorized.checkout_url.clone());

                Ok(PlaceOutcome { order, checkout_url: Some(authorized.checkout_url) })
            }
        }
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

#[cfg(test)]
mod place_tests {
    use super::*;
    use std::sync::Mutex;

    use crate::application::services::order_payments::AuthorizedIntent;
    use crate::domain::entities::{Basket, BasketLine, OrderStatus, PaymentStatus, Vendor, Vertical};

    fn tenant() -> Uuid { Uuid::from_u128(1) }

    fn a_vendor(tenant_id: Uuid) -> Vendor {
        let mut v = Vendor::new(
            tenant_id, Vertical::Restaurant, "Kuya's Lutong Bahay".into(),
            "123 Mabini St".into(), 14.5995, 120.9842,
        );
        v.activate();
        v
    }

    fn a_basket(tenant_id: Uuid, vendor: &Vendor) -> Basket {
        let mut b = Basket::new(tenant_id, Uuid::new_v4());
        let si = b.browse_sub_intent(Vertical::Restaurant);
        b.add_line(BasketLine::propose(b.id, si, tenant_id, vendor.id, Uuid::new_v4(), 1, 34_000, "browse"));
        b
    }

    struct FakeBaskets(Basket);
    #[async_trait::async_trait]
    impl BasketRepository for FakeBaskets {
        async fn find_by_id(&self, _t: Uuid, _id: Uuid) -> anyhow::Result<Option<Basket>> {
            Ok(Some(self.0.clone()))
        }
        async fn set_conflicts(
            &self, _t: Uuid, _id: Uuid, _c: &[crate::domain::entities::BasketConflict],
        ) -> anyhow::Result<()> { Ok(()) }
        async fn save(&self, _b: &Basket) -> anyhow::Result<()> { Ok(()) }
    }

    struct FakeVendors(Vendor);
    #[async_trait::async_trait]
    impl VendorRepository for FakeVendors {
        async fn find_by_id(&self, _t: Uuid, _id: Uuid) -> anyhow::Result<Option<Vendor>> {
            Ok(Some(self.0.clone()))
        }
        async fn save(&self, _v: &Vendor) -> anyhow::Result<()> { Ok(()) }
        async fn find_by_user(&self, _t: Uuid, _u: Uuid) -> anyhow::Result<Option<Vendor>> { Ok(None) }
        async fn list_for_tenant(&self, _t: Uuid) -> anyhow::Result<Vec<Vendor>> { Ok(vec![]) }
        async fn find_near(
            &self, _t: Uuid, _v: Vertical, _lat: f64, _lng: f64, _r: f64, _l: i64,
        ) -> anyhow::Result<Vec<Vendor>> { Ok(vec![]) }
        async fn find_by_ids(&self, _t: Uuid, _ids: &[Uuid]) -> anyhow::Result<Vec<Vendor>> {
            Ok(vec![self.0.clone()])
        }
    }

    #[derive(Default)]
    struct FakeDispatch {
        /// The `cod_amount_cents` passed on every `offer` call, in order.
        cod_offered: Mutex<Vec<i64>>,
        respond_empty: bool,
    }
    #[async_trait::async_trait]
    impl CourierDispatch for FakeDispatch {
        #[allow(clippy::too_many_arguments)]
        async fn offer(
            &self, _t: Uuid, _o: Uuid, _lat: f64, _lng: f64, _r: f64,
            _trip: i64, _tip: i64, cod_amount_cents: i64, _card: Option<serde_json::Value>,
        ) -> anyhow::Result<Vec<Uuid>> {
            self.cod_offered.lock().unwrap().push(cod_amount_cents);
            if self.respond_empty { Ok(vec![]) } else { Ok(vec![Uuid::new_v4()]) }
        }
    }

    #[derive(Default)]
    struct FakePayments {
        authorize_calls: Mutex<usize>,
    }
    #[async_trait::async_trait]
    impl OrderPayments for FakePayments {
        async fn authorize(
            &self, _t: Uuid, order_id: Uuid, amount_cents: i64, _currency: &str, _return_url: &str,
        ) -> anyhow::Result<AuthorizedIntent> {
            *self.authorize_calls.lock().unwrap() += 1;
            Ok(AuthorizedIntent {
                intent_id: Uuid::new_v4(),
                checkout_url: format!("https://pay.test/{order_id}?amount={amount_cents}"),
            })
        }
        async fn capture(&self, _intent_id: Uuid, _amount_cents: Option<i64>) -> anyhow::Result<()> { Ok(()) }
        async fn void(&self, _intent_id: Uuid) -> anyhow::Result<()> { Ok(()) }
    }

    fn service(
        dispatch: Arc<FakeDispatch>, payments: Arc<FakePayments>, vendor: Vendor, basket: Basket,
    ) -> CheckoutService {
        CheckoutService::new(
            Arc::new(FakeBaskets(basket)),
            Arc::new(FakeVendors(vendor)),
            dispatch,
            payments,
            "AED".to_string(),
            "https://app.omnideliv.test/payment/return".to_string(),
        )
    }

    /// COD checkout must remain byte-identical to today: the gateway is never
    /// touched, the courier is offered the full grand total immediately, and
    /// the order lands in the same state it always has.
    #[tokio::test]
    async fn cod_checkout_never_touches_the_gateway_and_offers_the_full_total_immediately() {
        let tenant_id = tenant();
        let v = a_vendor(tenant_id);
        let b = a_basket(tenant_id, &v);
        let dispatch = Arc::new(FakeDispatch::default());
        let payments = Arc::new(FakePayments::default());
        let svc = service(dispatch.clone(), payments.clone(), v, b.clone());

        let outcome = svc
            .place(tenant_id, b.id, 4_000, 14.5995, 120.9842, "customer@demo.com", None, None,
                   PaymentMethod::Cod, Fulfilment::Delivery)
            .await
            .expect("cod checkout succeeds");

        assert_eq!(*payments.authorize_calls.lock().unwrap(), 0, "COD must never call the gateway");
        assert_eq!(outcome.checkout_url, None);
        assert_eq!(outcome.order.payment_method, PaymentMethod::Cod);
        assert_eq!(
            outcome.order.cod_amount_cents(), outcome.order.grand_total_cents,
            "the courier collects the entire grand total, exactly as before this feature",
        );
        assert_eq!(
            dispatch.cod_offered.lock().unwrap().as_slice(), &[outcome.order.grand_total_cents],
            "the offer must declare the full grand total, unchanged",
        );
        assert_eq!(
            outcome.order.status, OrderStatus::AwaitingCourier,
            "COD offers the job to couriers immediately, reaching the same state as today",
        );
    }

    /// Online checkout must open an authorization hold and hand back a
    /// checkout URL — and, critically, must NOT offer the job to any courier
    /// yet. `dispatch.offer` returning who a job was *offered to* is not the
    /// same as a courier accepting it; here nobody has even been asked.
    #[tokio::test]
    async fn online_checkout_returns_a_checkout_url_and_does_not_offer_a_courier() {
        let tenant_id = tenant();
        let v = a_vendor(tenant_id);
        let b = a_basket(tenant_id, &v);
        let dispatch = Arc::new(FakeDispatch::default());
        let payments = Arc::new(FakePayments::default());
        let svc = service(dispatch.clone(), payments.clone(), v, b.clone());

        let outcome = svc
            .place(tenant_id, b.id, 4_000, 14.5995, 120.9842, "customer@demo.com", None, None,
                   PaymentMethod::Online, Fulfilment::Delivery)
            .await
            .expect("online checkout succeeds");

        assert!(outcome.checkout_url.is_some(), "the caller needs somewhere to send the customer");
        assert_eq!(*payments.authorize_calls.lock().unwrap(), 1);
        assert!(
            dispatch.cod_offered.lock().unwrap().is_empty(),
            "no courier may be offered the job before the authorization actually lands",
        );
        assert_eq!(outcome.order.payment_method, PaymentMethod::Online);
        assert_eq!(outcome.order.payment_status, PaymentStatus::Pending);
        assert_eq!(outcome.order.status, OrderStatus::Placed, "not yet AwaitingCourier — nothing was offered");
        assert_eq!(outcome.order.prepaid_amount_cents, outcome.order.grand_total_cents);
        assert!(outcome.order.payment_intent_id.is_some());
        assert!(
            outcome.order.pending_offer_card.is_some(),
            "the card must be held for the authorized-payment consumer to replay later",
        );
    }
}
