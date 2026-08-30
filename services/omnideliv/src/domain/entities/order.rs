//! Orders and three-leg settlement.
//!
//! All money is integer cents. No floats appear anywhere in this module — a
//! rounding error here is money created or destroyed, and `f64` cannot
//! represent a cent exactly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Placed,
    AwaitingCourier,
    Collecting,
    Delivering,
    Delivered,
    Cancelled,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Placed          => "placed",
            OrderStatus::AwaitingCourier => "awaiting_courier",
            OrderStatus::Collecting      => "collecting",
            OrderStatus::Delivering      => "delivering",
            OrderStatus::Delivered       => "delivered",
            OrderStatus::Cancelled       => "cancelled",
        }
    }
}

/// Where one vendor's half of an order stands.
///
/// `Rejected` is distinct from `Failed` on purpose: a store refusing an order
/// and a pickup going wrong are different events with different money
/// consequences, and collapsing them makes "why did this die" unanswerable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegStatus {
    Pending,
    Accepted,
    Preparing,
    Ready,
    PickedUp,
    Served,
    Rejected,
    Failed,
    Settled,
}

impl LegStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LegStatus::Pending   => "pending",
            LegStatus::Accepted  => "accepted",
            LegStatus::Preparing => "preparing",
            LegStatus::Ready     => "ready",
            LegStatus::PickedUp  => "picked_up",
            LegStatus::Served    => "served",
            LegStatus::Rejected  => "rejected",
            LegStatus::Failed    => "failed",
            LegStatus::Settled   => "settled",
        }
    }

    /// Parses the wire/database form. `None` for anything unrecognised, so a
    /// row written by a newer deploy fails loudly instead of silently
    /// decoding as `Pending` and re-offering work that is already underway.
    pub fn from_wire(s: &str) -> Option<Self> {
        Some(match s {
            "pending"   => LegStatus::Pending,
            "accepted"  => LegStatus::Accepted,
            "preparing" => LegStatus::Preparing,
            "ready"     => LegStatus::Ready,
            "picked_up" => LegStatus::PickedUp,
            "served"    => LegStatus::Served,
            "rejected"  => LegStatus::Rejected,
            "failed"    => LegStatus::Failed,
            "settled"   => LegStatus::Settled,
            _ => return None,
        })
    }

    /// Every variant. The repository derives its legal-predecessor list from
    /// this rather than hand-writing one per route, so `can_transition_to`
    /// stays the only statement of the graph.
    pub const ALL: [LegStatus; 9] = [
        LegStatus::Pending,  LegStatus::Accepted, LegStatus::Preparing,
        LegStatus::Ready,    LegStatus::PickedUp, LegStatus::Served,
        LegStatus::Rejected, LegStatus::Failed,   LegStatus::Settled,
    ];

    pub fn is_terminal(self) -> bool {
        matches!(self, LegStatus::Rejected | LegStatus::Failed | LegStatus::Settled)
    }

    /// Whether this leg has answered the acceptance question at all. Drives the
    /// acceptance barrier — see `Order::acceptance_state`.
    pub fn has_answered(self) -> bool {
        self != LegStatus::Pending
    }

    /// Whether this leg still owes the courier something.
    ///
    /// Not the same question as `has_answered`: a leg can have answered the
    /// vendor's accept/reject question and still be sitting on the counter.
    /// Before the acceptance states existed, "not pending" happened to mean
    /// "resolved" — it does not any more, and `all_legs_collected` is the
    /// caller that would otherwise advance an order whose goods never moved.
    pub fn blocks_collection(self) -> bool {
        matches!(
            self,
            LegStatus::Pending | LegStatus::Accepted | LegStatus::Preparing | LegStatus::Ready
        )
    }

    /// Whether this leg will not be fulfilled — refused by the store, or
    /// broken later. The acceptance barrier excludes exactly these from the
    /// amount it captures, so the rule lives here rather than in a closure
    /// that a later plan would have to re-derive.
    pub fn declined(self) -> bool {
        matches!(self, LegStatus::Rejected | LegStatus::Failed)
    }

    /// The legal transition graph. Enforced here rather than only in SQL so the
    /// rule is testable without a database and stated in exactly one place.
    pub fn can_transition_to(self, next: LegStatus) -> bool {
        use LegStatus::*;
        if self.is_terminal() {
            return false;
        }
        // An operator can fail any live leg; there is no single legal
        // predecessor for a pickup that went wrong.
        if next == Failed {
            return true;
        }
        matches!(
            (self, next),
            (Pending,   Accepted)  | (Pending,   Rejected)
          | (Accepted,  Preparing) | (Accepted,  Ready)
          | (Preparing, Ready)
          | (Ready,     PickedUp)  | (Ready,     Served)
          | (PickedUp,  Settled)   | (Served,    Settled)
        )
    }
}

/// How far an order has got through asking its vendors.
///
/// Deliberately separate from `OrderStatus`: that field is written by the
/// courier-event path, and a second writer on the same field is how two
/// sources of truth disagree about one order. This is derived on read and
/// stored nowhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum AcceptanceState {
    /// At least one vendor has not answered yet.
    Awaiting { outstanding: usize },
    /// Every leg has answered. `accepted_subtotal_cents` is the amount that may
    /// be captured; the rest of the authorization is voided.
    ///
    /// `accepted + rejected` does not necessarily equal the leg count: a
    /// `Failed` leg is in neither bucket, because it was not refused.
    Resolved { accepted: usize, rejected: usize, accepted_subtotal_cents: i64 },
}

/// How the customer pays. `Cod` is every order this service has ever placed;
/// `Online` is the prepaid-checkout foundation — see `PaymentStatus` and
/// `Order::cod_amount_cents`.
///
/// `Cod` is `#[default]` — every order this service has ever placed.
/// `CheckoutRequest::payment_method` defaults to this so a client that
/// predates this feature (the OmniDeliv mobile app included) keeps getting
/// exactly today's behavior without having to learn a new field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    #[default]
    Cod,
    Online,
}

impl PaymentMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            PaymentMethod::Cod    => "cod",
            PaymentMethod::Online => "online",
        }
    }
}

/// Where an `Online` order's authorization hold stands. Meaningless for `Cod`
/// orders — cash never touches a gateway, so this simply never leaves
/// `Pending` for one, and nothing reads it for a `Cod` order.
///
/// `Pending` -> `Authorized` -> `Captured`, or `Pending`/`Authorized` -> `Failed`
/// / `Voided`. See `Order::payment_authorized` et al. for the guarded
/// transitions — deliberately its own small state machine, independent of
/// `OrderStatus`: a courier being offered the job and money being ring-fenced
/// for it are two different facts about an order, discovered by two different,
/// asynchronous events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    #[default]
    Pending,
    Authorized,
    Captured,
    Voided,
    Failed,
}

impl PaymentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PaymentStatus::Pending    => "pending",
            PaymentStatus::Authorized => "authorized",
            PaymentStatus::Captured   => "captured",
            PaymentStatus::Voided     => "voided",
            PaymentStatus::Failed     => "failed",
        }
    }
}

/// What the courier still collects at the door.
///
/// A free function as well as a method because the order-list projection has
/// no `Order` to ask — and two subtractions written in two places is exactly
/// how a partly prepaid order ends up rendered as fully paid on one screen and
/// fully owed on another.
pub fn cod_amount_cents(grand_total_cents: i64, prepaid_amount_cents: i64) -> i64 {
    grand_total_cents - prepaid_amount_cents
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PaymentTransitionError {
    #[error("cannot go from payment status {from:?} to {to:?}")]
    Illegal { from: PaymentStatus, to: PaymentStatus },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorLeg {
    pub id:                   Uuid,
    pub order_id:             Uuid,
    pub tenant_id:            Uuid,
    pub vendor_id:            Uuid,
    pub goods_subtotal_cents: i64,
    /// Snapshotted at order time. The vendor's rate may change later; this
    /// order settles at the rate that applied when it was placed.
    pub commission_bps:       i32,
    pub commission_cents:     i64,
    pub payout_cents:         i64,
    pub status:               LegStatus,
    pub picked_up_at:         Option<DateTime<Utc>>,
    /// When the store accepted, and what it promised. Written only by the
    /// guarded transition in `leg_repo` — `OrderRepository::save` deliberately
    /// does not touch these, so a whole-order write can never clobber an
    /// acceptance a tablet made a moment earlier.
    pub accepted_at:          Option<DateTime<Utc>>,
    pub ready_at:             Option<DateTime<Utc>>,
    /// The store's own estimate, not `vendors.prep_time_minutes`. That is a
    /// static per-store default nothing reconciles against reality; this is
    /// what a person said about this order.
    pub ready_in_minutes:     Option<i32>,
    pub created_at:           DateTime<Utc>,
}

impl VendorLeg {
    /// Split a subtotal into commission and payout.
    ///
    /// `payout = subtotal - commission` rather than a second multiplication, so
    /// the two can never fail to sum to the subtotal regardless of rounding.
    pub fn settle(
        tenant_id: Uuid,
        vendor_id: Uuid,
        goods_subtotal_cents: i64,
        commission_bps: i32,
    ) -> Self {
        let commission_cents = goods_subtotal_cents * commission_bps as i64 / 10_000;
        Self {
            id: Uuid::new_v4(),
            order_id: Uuid::nil(), // set by Order::place
            tenant_id,
            vendor_id,
            goods_subtotal_cents,
            commission_bps,
            commission_cents,
            payout_cents: goods_subtotal_cents - commission_cents,
            status: LegStatus::Pending,
            picked_up_at: None,
            accepted_at: None,
            ready_at: None,
            ready_in_minutes: None,
            created_at: Utc::now(),
        }
    }

    pub fn mark_picked_up(&mut self) {
        self.status = LegStatus::PickedUp;
        self.picked_up_at = Some(Utc::now());
    }

    /// A vendor whose pickup failed is not paid. Per-leg status is what lets an
    /// order deliver what was collected and refund only the failed leg.
    pub fn mark_failed(&mut self) {
        self.status = LegStatus::Failed;
    }
}

/// Where the money funding a settlement actually came from.
///
/// The four `Settlement` legs sum to `grand_total_cents` either way — this
/// does not change the arithmetic, only which pool each leg is owed *from*.
/// A COD order's courier holds the customer's cash and remits it, netting
/// their own earnings out of what they hand back. A prepaid order's cash
/// never touches the courier at all: it landed in the NI merchant account,
/// so the courier is *owed* their earnings from that digital pool instead —
/// there is nothing for them to net against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FundingSource {
    /// Every cent is cash the courier collected at the door and must remit.
    CourierCollectedCash,
    /// Every cent already sat in the NI merchant account before the courier
    /// ever showed up. The courier collects nothing and is owed their
    /// earnings from this pool rather than netting against remitted cash.
    DigitalPool,
    /// A partially-prepaid order: `prepaid_amount_cents` came in through NI,
    /// the rest (`cod_amount_cents`) is cash the courier collects and remits
    /// exactly like a wholly-COD order — e.g. goods paid online, tip in cash.
    Mixed { cod_amount_cents: i64, prepaid_amount_cents: i64 },
}

/// The full three-leg split for one order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Settlement {
    pub vendor_payouts_cents:   i64,
    pub commissions_cents:      i64,
    pub courier_earnings_cents: i64,
    pub partner_margin_cents:   i64,
    /// See `FundingSource`. Does not participate in the balance identity —
    /// it says where the four amounts above came from, not a fifth amount.
    pub funding: FundingSource,
}

impl Settlement {
    /// Everything the Partner keeps, by both routes.
    ///
    /// Provided as a method rather than a field on `Settlement` because the
    /// struct's four money fields must each name a disjoint slice of the
    /// grand total — a fifth money field overlapping two of them would break
    /// the balance identity the moment anyone summed the struct naively.
    pub fn partner_revenue_cents(&self) -> i64 {
        self.commissions_cents + self.partner_margin_cents
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransitionError {
    #[error("cannot go from {from:?} to {to:?}")]
    Illegal { from: OrderStatus, to: OrderStatus },
    #[error("{0} leg(s) still pending collection")]
    LegsPending(usize),
    #[error("no leg was collected — nothing to deliver")]
    NothingCollected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id:                 Uuid,
    pub tenant_id:          Uuid,
    pub customer_id:        Uuid,
    pub basket_id:          Uuid,
    pub plan_id:            Uuid,
    pub status:             OrderStatus,
    pub goods_total_cents:  i64,
    pub delivery_fee_cents: i64,
    pub tip_cents:          i64,
    pub grand_total_cents:  i64,
    pub courier_trip_cents: i64,
    pub courier_task_id:    Option<Uuid>,
    /// Where this order is going. `None` only for orders placed before
    /// migration 0013 — the recovery sweep escalates those rather than
    /// re-offering to a guessed point.
    pub delivery_lat:       Option<f64>,
    pub delivery_lng:       Option<f64>,
    /// Which identity user is carrying this order.
    ///
    /// `courier_task_id` is a field-ops *assignment* id and field-ops'
    /// `courier_id` is its own key for the person; neither can be compared
    /// against the `user_id` in a courier's JWT. The driver manifest
    /// authorizes on exactly that comparison, and resolving it per request
    /// would put a polled endpoint on another service's availability.
    ///
    /// `None` for orders claimed before migration 0020.
    pub courier_user_id:    Option<Uuid>,
    /// Who to hand it to, snapshotted at checkout. `None` for orders placed
    /// before migration 0019, and for any path that does not know a contact
    /// — the manifest renders a dropoff without a name rather than
    /// refusing to load.
    pub customer_name:      Option<String>,
    /// Snapshotted rather than resolved on read: the manifest is polled, and a
    /// cross-service identity lookup per refresh would put a courier's screen
    /// on identity's availability.
    pub customer_phone:     Option<String>,
    /// The customer's instruction to the courier — "unit 12B, gate code 4417".
    ///
    /// An order has no street address, so this is the only place anyone can say
    /// where the door actually is. Cleaned by `clean_delivery_note` before it
    /// gets here; never trusted raw from the request body.
    pub delivery_note:      Option<String>,
    pub legs:               Vec<VendorLeg>,
    pub placed_at:          DateTime<Utc>,
    pub delivered_at:       Option<DateTime<Utc>>,
    /// `Cod` for every order this service has ever placed. `Default::default()`
    /// so `Order::place`'s ten-argument signature is unchanged and every
    /// existing call site — production and test alike — keeps constructing a
    /// byte-identical COD order. Set via `with_payment` for the `Online` path.
    pub payment_method:        PaymentMethod,
    /// Meaningless for `Cod` — see `PaymentStatus`.
    pub payment_status:        PaymentStatus,
    /// The `payments` service's `payment_intents.id` for this order's
    /// authorization hold. `None` for every `Cod` order and for an `Online`
    /// order before `authorize()` returns.
    pub payment_intent_id:     Option<Uuid>,
    /// How much of `grand_total_cents` was (or will be) taken online rather
    /// than left for the courier to collect at the door. `0` for `Cod`. Not
    /// necessarily equal to `grand_total_cents` for `Online` either — see
    /// `cod_amount_cents`.
    pub prepaid_amount_cents:  i64,
    /// When `payment_authorized` last ran. `None` until then. This, not
    /// `placed_at`, is the clock the no-courier void timeout counts from —
    /// `placed_at` predates authorization by however long the customer spent
    /// on the hosted checkout page (up to the intent's own TTL).
    pub payment_authorized_at: Option<DateTime<Utc>>,
    /// The exact offer card `build_offer_card` produced at checkout time, held
    /// here so the `payment.intent.authorized` consumer can offer the job to
    /// couriers with the identical card a COD order would have shown
    /// immediately — rather than trying to reconstruct one later from less
    /// information. `None` for `Cod` orders, which never defer the offer.
    pub pending_offer_card:    Option<serde_json::Value>,
    /// The NI hosted-checkout page this order's authorization was opened
    /// against. `None` for `Cod`.
    ///
    /// Kept because the URL used to exist only in the checkout *response*:
    /// anything that took the customer off the page before they paid — a phone
    /// call, backgrounding the app, tapping Back — left an order that could
    /// never be paid for and no way back to it. Read through
    /// `resumable_checkout_url`, never directly, so the one rule that makes it
    /// safe to disclose lives in one place.
    pub payment_checkout_url:  Option<String>,
}

/// Namespaces identity mints from a phone number for OTP-only sign-in.
///
/// Nothing can be delivered to these addresses. They exist because the platform
/// keys accounts on an email while the thing actually verified was a phone.
///
/// A literal list rather than a suffix pattern, so a future
/// `@partner.logisticos.app` cannot silently start yielding phone numbers.
const PHONE_DERIVED_DOMAINS: &[&str] =
    &["@customer.logisticos.app", "@driver.logisticos.app"];

/// The phone behind a login address, or `None` if that address is a real
/// mailbox somebody chose.
///
/// Never a plain `split('@')`: that would put the local part of
/// `maria.reyes@gmail.com` on a courier's screen as a number to dial.
pub fn phone_from_login(email: &str) -> Option<String> {
    let local = PHONE_DERIVED_DOMAINS
        .iter()
        .find_map(|d| email.strip_suffix(*d))?;
    // The minted namespace is digits by construction. Anything else in it did
    // not come from the OTP path and must not be trusted as a number.
    if local.is_empty() || !local.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(local.to_string())
}


/// The number a courier can actually call, from whatever identity gave us.
///
/// Two sources, in order of trust:
///
/// 1. `claims.phone` — identity's own `users.phone_number` column, carried on
///    the token. Authoritative when present.
/// 2. The minted OTP login `<digits>@customer.logisticos.app`, decoded by
///    [`phone_from_login`]. The only thing available before identity put the
///    number on the token, and still the fallback for tokens issued then.
///
/// Why both. The original design assumed the login *was* the phone, because the
/// OTP path mints that address. Production disagreed: on 2026-08-23 all 34
/// orders had a null `customer_phone`, and the newest was placed by
/// `testdriverone@gmail.com` — a real mailbox — whose identity row holds
/// `+971553604321` all along. `phone_from_login` was behaving correctly; it was
/// simply never asked the one place that knew.
///
/// Never a plain `split('@')` on a real mailbox: that puts the local part of
/// `maria.reyes@gmail.com` on a courier's screen as a number to dial.
pub fn contact_phone(claims_phone: Option<&str>, login: &str) -> Option<String> {
    let from_claims = claims_phone
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_owned);

    from_claims.or_else(|| phone_from_login(login))
}

#[cfg(test)]
mod contact_phone_tests {
    use super::{contact_phone, phone_from_login};

    #[test]
    fn the_token_phone_wins_when_identity_has_one() {
        assert_eq!(
            contact_phone(Some("+971553604321"), "testdriverone@gmail.com"),
            Some("+971553604321".to_string()),
        );
    }

    /// The case that made this necessary. A real mailbox yields nothing from
    /// the login, and before the token carried a number there was nothing else
    /// to fall back to — so the courier got a blank where the phone should be.
    #[test]
    fn a_real_mailbox_alone_yields_nothing() {
        assert_eq!(contact_phone(None, "testdriverone@gmail.com"), None);
        assert_eq!(phone_from_login("testdriverone@gmail.com"), None);
    }

    /// Tokens minted before identity carried the phone still work.
    #[test]
    fn the_minted_login_is_still_decoded_when_the_token_is_silent() {
        assert_eq!(
            contact_phone(None, "639170000123@customer.logisticos.app"),
            Some("639170000123".to_string()),
        );
    }

    /// An empty or whitespace claim is not a phone number. Treating it as one
    /// would suppress the fallback and put a blank on the manifest.
    #[test]
    fn a_blank_claim_falls_through_to_the_login() {
        assert_eq!(
            contact_phone(Some(""), "639170000123@customer.logisticos.app"),
            Some("639170000123".to_string()),
        );
        assert_eq!(
            contact_phone(Some("   "), "639170000123@customer.logisticos.app"),
            Some("639170000123".to_string()),
        );
    }

    #[test]
    fn nothing_anywhere_is_still_nothing() {
        assert_eq!(contact_phone(None, "merchant@demo.com"), None);
    }
}


/// The longest delivery note a customer may leave.
///
/// This lands on a courier's phone, on a screen read one-handed at a door. It
/// is "unit 12B, gate code 4417, ring twice", not an essay — and it is the one
/// free-text field a client controls that a courier is asked to act on, so it
/// is bounded at the boundary rather than trusted and truncated later.
pub const MAX_DELIVERY_NOTE_CHARS: usize = 280;

/// Clean a customer's delivery note, or decide there isn't one.
///
/// Unlike the phone, this genuinely does come from the request body — the
/// customer is the only one who knows their gate code. That makes bounding it
/// the caller's job, not a formality.
///
/// Characters, not bytes: `chars().count()` so a note in Tagalog or with emoji
/// is measured the way a person would measure it, and a multi-byte character
/// near the limit cannot be cut in half.
pub fn clean_delivery_note(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(MAX_DELIVERY_NOTE_CHARS).collect())
}

#[cfg(test)]
mod delivery_note_tests {
    use super::{clean_delivery_note, MAX_DELIVERY_NOTE_CHARS};

    #[test]
    fn a_real_note_survives() {
        assert_eq!(
            clean_delivery_note(Some("Unit 12B, gate code 4417")),
            Some("Unit 12B, gate code 4417".to_string()),
        );
    }

    /// A blank note is no note. Storing `""` would render an empty line on the
    /// manifest that looks like a rendering fault.
    #[test]
    fn blank_and_whitespace_are_no_note_at_all() {
        assert_eq!(clean_delivery_note(None), None);
        assert_eq!(clean_delivery_note(Some("")), None);
        assert_eq!(clean_delivery_note(Some("   \n\t ")), None);
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        assert_eq!(clean_delivery_note(Some("  ring twice  ")), Some("ring twice".to_string()));
    }

    /// Bounded at the boundary. The client is not trusted to have limited it.
    #[test]
    fn an_overlong_note_is_cut_to_the_limit() {
        let long = "x".repeat(MAX_DELIVERY_NOTE_CHARS + 50);
        let cleaned = clean_delivery_note(Some(&long)).unwrap();
        assert_eq!(cleaned.chars().count(), MAX_DELIVERY_NOTE_CHARS);
    }

    /// Characters, not bytes. Cutting at a byte offset would split a multi-byte
    /// character and produce a note that is not valid text at all.
    #[test]
    fn a_multibyte_note_is_measured_in_characters() {
        let note = "ñ".repeat(MAX_DELIVERY_NOTE_CHARS + 10);
        let cleaned = clean_delivery_note(Some(&note)).unwrap();
        assert_eq!(cleaned.chars().count(), MAX_DELIVERY_NOTE_CHARS);
        // Still valid UTF-8 by construction — this would panic on a byte slice.
        assert!(cleaned.ends_with('ñ'));
    }
}

impl Order {
    #[allow(clippy::too_many_arguments)]
    pub fn place(
        tenant_id: Uuid,
        customer_id: Uuid,
        basket_id: Uuid,
        plan_id: Uuid,
        mut legs: Vec<VendorLeg>,
        delivery_fee_cents: i64,
        tip_cents: i64,
        courier_trip_cents: i64,
        delivery_lat: f64,
        delivery_lng: f64,
    ) -> Self {
        let id = Uuid::new_v4();
        for l in &mut legs {
            l.order_id = id;
        }

        // Derived, never passed in — a caller-supplied total is a place for the
        // arithmetic to disagree with itself.
        let goods_total_cents: i64 = legs.iter().map(|l| l.goods_subtotal_cents).sum();

        Self {
            id,
            tenant_id,
            customer_id,
            basket_id,
            plan_id,
            status: OrderStatus::Placed,
            goods_total_cents,
            delivery_fee_cents,
            tip_cents,
            grand_total_cents: goods_total_cents + delivery_fee_cents + tip_cents,
            courier_trip_cents,
            courier_task_id: None,
            // Taken as plain f64 and stored as Some: a newly placed order always
            // knows its destination. The Option exists for history, not for
            // callers to leave empty.
            delivery_lat: Some(delivery_lat),
            delivery_lng: Some(delivery_lng),
            // Set by `with_customer_contact` rather than taken here. This
            // constructor already carries ten arguments; two more that only one
            // of its six call sites can supply would be noise at the other five.
            courier_user_id: None,
            customer_name: None,
            customer_phone: None,
            delivery_note: None,
            legs,
            placed_at: Utc::now(),
            delivered_at: None,
            // Defaulted, not taken as parameters — see the field doc comment.
            // Set via `with_payment` for the one call site (checkout) that
            // knows a payment method; every other caller (recovery, replay,
            // tests) gets exactly today's COD order.
            payment_method: PaymentMethod::default(),
            payment_status: PaymentStatus::default(),
            payment_intent_id: None,
            prepaid_amount_cents: 0,
            payment_authorized_at: None,
            pending_offer_card: None,
            payment_checkout_url: None,
        }
    }

    /// Record who the courier is delivering to.
    ///
    /// Separate from `place` because only checkout knows it — every other path
    /// that builds an order (recovery, replay, tests) has no authenticated
    /// caller to take it from, and would otherwise have to pass `None, None`.
    pub fn with_customer_contact(
        mut self,
        name: Option<String>,
        phone: Option<String>,
    ) -> Self {
        self.customer_name = name;
        self.customer_phone = phone;
        self
    }

    /// Attach the customer's note for the courier. Chainable, and separate from
    /// the contact pair because it comes from a different place: the contact is
    /// taken from the validated token, this is the one field the customer types.
    pub fn with_delivery_note(mut self, note: Option<String>) -> Self {
        self.delivery_note = note;
        self
    }

    /// Switch this order onto the `Online` prepaid rail (or explicitly confirm
    /// `Cod`). Chainable and separate from `place`'s ten arguments for the same
    /// reason `with_customer_contact` is: only checkout's `place` call site
    /// knows a payment method, and every other caller keeps getting the
    /// `Cod` / `0` default without having to pass it.
    pub fn with_payment(mut self, method: PaymentMethod, prepaid_amount_cents: i64) -> Self {
        self.payment_method = method;
        self.prepaid_amount_cents = prepaid_amount_cents;
        self
    }

    /// Hold the exact offer card built at checkout for later use by the
    /// `payment.intent.authorized` consumer. See the field doc comment.
    pub fn with_pending_offer_card(mut self, card: Option<serde_json::Value>) -> Self {
        self.pending_offer_card = card;
        self
    }

    /// What the courier collects in cash at the door.
    ///
    /// Not a binary switch on `payment_method` — a partially-prepaid order
    /// (goods paid online, tip left in cash) is representable today by giving
    /// `prepaid_amount_cents` any value strictly between `0` and
    /// `grand_total_cents`, and this formula is the one and only place that
    /// has to know how to turn that into "what does the courier collect."
    /// Every dispatch call site (checkout, the authorized-payment consumer,
    /// and the stuck-order recovery sweep) reads this rather than
    /// `grand_total_cents` directly.
    /// The hosted-checkout page to send the customer back to, or `None`.
    ///
    /// Gated on `Pending` rather than on the URL merely being present. Once a
    /// hold is `Authorized` the page is spent, and re-opening it invites a
    /// second authorization against the same order — one the capture path
    /// would never capture and the void path would never release, because both
    /// only ever know about `payment_intent_id`. A `Captured`, `Voided` or
    /// `Failed` order must obviously never offer a way to pay again either.
    pub fn resumable_checkout_url(&self) -> Option<&str> {
        if self.payment_method != PaymentMethod::Online {
            return None;
        }
        if self.payment_status != PaymentStatus::Pending {
            return None;
        }
        // A cancelled order is not payable, whatever its payment status says.
        if matches!(self.status, OrderStatus::Cancelled | OrderStatus::Delivered) {
            return None;
        }
        self.payment_checkout_url.as_deref()
    }

    pub fn cod_amount_cents(&self) -> i64 {
        cod_amount_cents(self.grand_total_cents, self.prepaid_amount_cents)
    }

    fn advance_payment(
        &mut self,
        to: PaymentStatus,
        from: &[PaymentStatus],
    ) -> Result<(), PaymentTransitionError> {
        // Kafka (and the recovery sweep) can redeliver/re-run the same
        // transition — a repeat of the current status is a no-op, mirroring
        // `advance` on `OrderStatus` above.
        if self.payment_status == to {
            return Ok(());
        }
        if !from.contains(&self.payment_status) {
            return Err(PaymentTransitionError::Illegal { from: self.payment_status, to });
        }
        self.payment_status = to;
        Ok(())
    }

    /// The `payment.intent.authorized` webhook landed: funds are ring-fenced,
    /// not yet taken. This is what unblocks the courier offer for an `Online`
    /// order — see the consumer in `infrastructure/messaging`.
    ///
    /// The already-`Authorized` case is checked explicitly, before deferring
    /// to `advance_payment`'s own idempotent no-op: that guard only protects
    /// `payment_status` itself, and would otherwise still let a Kafka
    /// redelivery re-stamp `payment_authorized_at` with a fresh timestamp —
    /// which is exactly the clock the no-courier void timeout counts from.
    pub fn payment_authorized(&mut self, intent_id: Uuid) -> Result<(), PaymentTransitionError> {
        if self.payment_status == PaymentStatus::Authorized {
            return Ok(());
        }
        self.advance_payment(PaymentStatus::Authorized, &[PaymentStatus::Pending])?;
        self.payment_intent_id = Some(intent_id);
        self.payment_authorized_at = Some(Utc::now());
        Ok(())
    }

    /// A courier actually accepted the job — the "capture" half of
    /// authorize-then-capture-or-void. Called from `courier_claimed`'s caller,
    /// never from within `courier_claimed` itself: capturing money is a
    /// gateway call the pure state transition must not know how to make.
    pub fn payment_captured(&mut self) -> Result<(), PaymentTransitionError> {
        self.advance_payment(PaymentStatus::Captured, &[PaymentStatus::Authorized])
    }

    /// No courier accepted within the no-courier timeout — release the hold.
    /// The customer is never charged.
    pub fn payment_voided(&mut self) -> Result<(), PaymentTransitionError> {
        self.advance_payment(PaymentStatus::Voided, &[PaymentStatus::Authorized])
    }

    /// The gateway declined the charge, or the hosted-checkout session simply
    /// expired unused (`services/payments`' own sweep — both publish the same
    /// `payment.intent.failed` event, see its doc comment). Either way this
    /// order never had a courier offered to it, and never will.
    pub fn payment_failed(&mut self) -> Result<(), PaymentTransitionError> {
        self.advance_payment(PaymentStatus::Failed, &[PaymentStatus::Pending, PaymentStatus::Authorized])
    }

    /// Kafka is at-least-once, so a repeat of the transition we already made is
    /// a no-op rather than an error. Anything else is refused: silently
    /// accepting an out-of-order event is how an uncollected order gets marked
    /// delivered.
    fn advance(&mut self, to: OrderStatus, from: &[OrderStatus]) -> Result<(), TransitionError> {
        if self.status == to {
            return Ok(());
        }
        if !from.contains(&self.status) {
            return Err(TransitionError::Illegal { from: self.status, to });
        }
        self.status = to;
        Ok(())
    }

    pub fn courier_offered(&mut self) -> Result<(), TransitionError> {
        self.advance(OrderStatus::AwaitingCourier, &[OrderStatus::Placed])
    }

    /// A courier took the job.
    ///
    /// `courier_user_id` is `None` for events published before field-ops
    /// carried it. Those orders cannot authorize a manifest read, and the
    /// manifest refuses them rather than falling open — "we do not know who is
    /// carrying this" must never read as "anyone may look".
    pub fn courier_claimed(
        &mut self,
        assignment_id: Uuid,
        courier_user_id: Option<Uuid>,
    ) -> Result<(), TransitionError> {
        self.advance(OrderStatus::Collecting, &[OrderStatus::Placed, OrderStatus::AwaitingCourier])?;
        self.courier_task_id = Some(assignment_id);
        // Never overwrite a known courier with `None`: Kafka is at-least-once,
        // so a replayed pre-migration `Assigned` must not erase the identity a
        // later message established.
        if courier_user_id.is_some() {
            self.courier_user_id = courier_user_id;
        }
        Ok(())
    }

    /// Every leg has reached a terminal state and at least one was collected.
    ///
    /// A failed or rejected leg is resolved; a leg still being prepared is
    /// not.
    pub fn all_legs_collected(&mut self) -> Result<(), TransitionError> {
        // Was: a count of `Pending` legs. That was equivalent to "unresolved"
        // only while `Pending | PickedUp | Failed | Settled` were the only
        // states. A leg at `Ready` is accepted and cooked and still on the
        // counter — advancing here would deliver an order whose goods were
        // never handed over.
        let outstanding = self.legs.iter().filter(|l| l.status.blocks_collection()).count();
        if outstanding > 0 {
            return Err(TransitionError::LegsPending(outstanding));
        }
        if !self.legs.iter().any(|l| l.status == LegStatus::PickedUp) {
            return Err(TransitionError::NothingCollected);
        }
        self.advance(OrderStatus::Delivering, &[OrderStatus::Collecting])
    }

    pub fn delivered(&mut self) -> Result<(), TransitionError> {
        self.advance(OrderStatus::Delivered, &[OrderStatus::Delivering])?;
        self.delivered_at = Some(Utc::now());
        Ok(())
    }

    /// Cancel, from any state except delivered.
    ///
    /// The plan called this terminal from *any* state, which would let a
    /// delivered order be flipped to cancelled — quietly dropping a completed
    /// order out of settlement while the vendor and courier have already been
    /// credited. Undoing a delivery is a refund, which is a separate concern
    /// with its own money movement, so it is refused here rather than
    /// approximated.
    pub fn cancel(&mut self) -> Result<(), TransitionError> {
        if self.status == OrderStatus::Delivered {
            return Err(TransitionError::Illegal {
                from: OrderStatus::Delivered,
                to: OrderStatus::Cancelled,
            });
        }
        self.status = OrderStatus::Cancelled;
        Ok(())
    }

    /// How far this order has got through asking its vendors, derived from the
    /// legs and stored nowhere. See `AcceptanceState`.
    ///
    /// This is what the acceptance barrier reads to decide how much of the
    /// authorization to capture: once every leg has answered, capture
    /// `accepted_subtotal_cents` and void the rest.
    pub fn acceptance_state(&self) -> AcceptanceState {
        let outstanding = self.legs.iter().filter(|l| !l.status.has_answered()).count();
        if outstanding > 0 {
            return AcceptanceState::Awaiting { outstanding };
        }

        // "Accepted" here means the leg survived the ask — anything that is not
        // an outright refusal or failure. A leg already picked up or served is
        // emphatically accepted.
        let survived = |l: &&VendorLeg| !l.status.declined();

        AcceptanceState::Resolved {
            accepted: self.legs.iter().filter(survived).count(),
            rejected: self.legs.iter().filter(|l| l.status == LegStatus::Rejected).count(),
            accepted_subtotal_cents: self
                .legs
                .iter()
                .filter(survived)
                .map(|l| l.goods_subtotal_cents)
                .sum(),
        }
    }

    /// Where every cent the customer paid goes.
    ///
    /// The four legs sum to `grand_total_cents` by construction:
    ///   goods_total  = vendor_payouts + commissions        (per-leg invariant)
    ///   delivery_fee = courier_trip   + fee_margin         (by definition)
    ///   tip          → courier, in full
    ///
    /// so vendor_payouts + commissions + (courier_trip + tip) + fee_margin
    ///  = goods_total + delivery_fee + tip
    ///  = grand_total.
    pub fn settlement(&self) -> Settlement {
        let vendor_payouts_cents: i64 = self.legs.iter().map(|l| l.payout_cents).sum();
        let commissions_cents:    i64 = self.legs.iter().map(|l| l.commission_cents).sum();

        // Per trip, not per stop. This asymmetry is the business model: a second
        // pickup barely moves courier cost but adds a full commission leg.
        let courier_earnings_cents = self.courier_trip_cents + self.tip_cents;
        let fee_margin_cents       = self.delivery_fee_cents - self.courier_trip_cents;

        // Where the money came from — does not change any of the four amounts
        // above, only which pool each is owed from. See `FundingSource`.
        let funding = match self.payment_method {
            PaymentMethod::Cod => FundingSource::CourierCollectedCash,
            PaymentMethod::Online => {
                let cod = self.cod_amount_cents();
                if cod <= 0 {
                    FundingSource::DigitalPool
                } else {
                    FundingSource::Mixed {
                        cod_amount_cents: cod,
                        prepaid_amount_cents: self.prepaid_amount_cents,
                    }
                }
            }
        };

        Settlement {
            vendor_payouts_cents,
            commissions_cents,
            courier_earnings_cents,
            // Fee margin only — NOT `fee_margin + commissions`.
            //
            // Commission is already its own term above. Folding it in here too
            // would report the same money twice, and the balance test would
            // fail (correctly). Total Partner revenue is
            // `Settlement::partner_revenue_cents`, which adds the two back
            // together for reporting without breaking the identity.
            partner_margin_cents: fee_margin_cents,
            funding,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn leg(subtotal: i64, bps: i32) -> VendorLeg {
        VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), subtotal, bps)
    }

    fn order(legs: Vec<VendorLeg>, fee: i64, tip: i64, trip: i64) -> Order {
        Order::place(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), legs, fee, tip, trip,
                     14.5995, 120.9842)
    }

    #[test]
    fn a_leg_splits_its_subtotal_exactly() {
        let l = leg(34_000, 1500);
        assert_eq!(l.commission_cents, 5_100);
        assert_eq!(l.payout_cents, 28_900);
        assert_eq!(l.commission_cents + l.payout_cents, l.goods_subtotal_cents);
    }

    #[test]
    fn commission_truncates_in_the_vendors_favour() {
        // 999 * 15% = 149.85 → 149, so the vendor keeps the part-cent.
        let l = leg(999, 1500);
        assert_eq!(l.commission_cents, 149);
        assert_eq!(l.payout_cents, 850);
        assert_eq!(l.commission_cents + l.payout_cents, 999);
    }

    #[test]
    fn the_grand_total_is_goods_plus_fee_plus_tip() {
        let o = order(vec![leg(34_000, 1500), leg(28_000, 1200)], 7_900, 4_000, 5_800);
        assert_eq!(o.goods_total_cents, 62_000);
        assert_eq!(o.grand_total_cents, 62_000 + 7_900 + 4_000);
    }

    /// THE INVARIANT. What the customer pays must exactly equal what everyone
    /// else receives. If this can drift, money is being created or destroyed.
    #[test]
    fn settlement_balances_exactly() {
        let o = order(vec![leg(34_000, 1500), leg(28_000, 1200)], 7_900, 4_000, 5_800);
        let s = o.settlement();

        assert_eq!(
            o.grand_total_cents,
            s.vendor_payouts_cents + s.commissions_cents + s.courier_earnings_cents + s.partner_margin_cents,
            "customer_paid must equal vendor_payouts + commissions + courier_earnings + partner_margin"
        );
    }

    /// The courier is paid per trip, not per stop — which is exactly why adding
    /// a second pickup is nearly free while adding a full commission leg.
    #[test]
    fn courier_earnings_are_the_trip_plus_the_whole_tip() {
        let o = order(vec![leg(10_000, 1000)], 7_900, 4_000, 5_800);
        let s = o.settlement();
        assert_eq!(s.courier_earnings_cents, 5_800 + 4_000);
    }

    /// `partner_margin` is the fee margin only — commission is its own term.
    /// Each term must name a disjoint slice of the total or they cannot sum to
    /// it. Total Partner revenue is the two added together, asserted here so
    /// neither reading can drift from the other.
    #[test]
    fn partner_margin_is_the_fee_less_the_courier_trip() {
        let o = order(vec![leg(10_000, 1000)], 7_900, 0, 5_800);
        let s = o.settlement();

        assert_eq!(s.partner_margin_cents, 7_900 - 5_800, "fee margin only");
        assert_eq!(s.commissions_cents, 1_000);
        assert_eq!(
            s.partner_revenue_cents(),
            (7_900 - 5_800) + 1_000,
            "total Partner revenue is fee margin plus commission"
        );
    }

    /// The margin lever, expressed as a test: a second vendor adds a full
    /// commission leg while the fee — and therefore the courier cost — is flat.
    ///
    /// The comparison is on total Partner revenue, not `partner_margin_cents`.
    /// That field is fee-margin-only by design, so it is *identical* for both
    /// orders; asserting it grew would contradict the decision recorded on
    /// `settlement` and fail.
    #[test]
    fn a_second_vendor_adds_commission_without_adding_fee() {
        let one = order(vec![leg(30_000, 1500)], 7_900, 0, 5_800);
        let two = order(vec![leg(30_000, 1500), leg(30_000, 1500)], 7_900, 0, 5_800);

        assert_eq!(one.delivery_fee_cents, two.delivery_fee_cents, "the fee is flat");
        assert_eq!(
            one.settlement().partner_margin_cents,
            two.settlement().partner_margin_cents,
            "fee margin is unchanged — the second stop costs the Partner nothing",
        );
        assert!(
            two.settlement().partner_revenue_cents() > one.settlement().partner_revenue_cents(),
            "the second vendor's commission is pure additional revenue"
        );
    }

    #[test]
    fn a_zero_tip_order_still_balances() {
        let o = order(vec![leg(15_000, 2000)], 4_900, 0, 3_500);
        let s = o.settlement();
        assert_eq!(
            o.grand_total_cents,
            s.vendor_payouts_cents + s.commissions_cents + s.courier_earnings_cents + s.partner_margin_cents,
        );
    }

    /// A courier trip costing more than the fee is a loss-leading order, not an
    /// impossible one — short-distance pricing floors make it routine. The
    /// identity must still hold with a negative margin, or the ledger would
    /// silently absorb the loss.
    #[test]
    fn an_underwater_delivery_fee_still_balances() {
        let o = order(vec![leg(20_000, 1500)], 4_900, 0, 6_500);
        let s = o.settlement();

        assert_eq!(s.partner_margin_cents, -1_600, "the Partner eats the difference");
        assert_eq!(
            o.grand_total_cents,
            s.vendor_payouts_cents + s.commissions_cents + s.courier_earnings_cents + s.partner_margin_cents,
        );
    }

    /// The happy path, in order. Each transition is legal only from the state
    /// before it — a machine that accepts any transition from any state is not
    /// a machine, it is a mutable field.
    #[test]
    fn the_lifecycle_advances_in_order() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        assert_eq!(o.status, OrderStatus::Placed);

        assert!(o.courier_offered().is_ok());
        assert_eq!(o.status, OrderStatus::AwaitingCourier);

        assert!(o.courier_claimed(Uuid::new_v4(), None).is_ok());
        assert_eq!(o.status, OrderStatus::Collecting);

        o.legs[0].mark_picked_up();
        assert!(o.all_legs_collected().is_ok());
        assert_eq!(o.status, OrderStatus::Delivering);

        assert!(o.delivered().is_ok());
        assert_eq!(o.status, OrderStatus::Delivered);
        assert!(o.delivered_at.is_some());
    }

    /// Kafka delivers at least once, so the same event can arrive twice.
    /// A repeat of the current transition must be a no-op, not an error and
    /// not a double-advance.
    #[test]
    fn a_repeated_transition_is_idempotent() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        o.courier_offered().unwrap();

        assert!(o.courier_offered().is_ok(), "a duplicate event must not error");
        assert_eq!(o.status, OrderStatus::AwaitingCourier, "and must not advance");
    }

    /// Out-of-order delivery is also possible. Skipping ahead must be refused
    /// loudly rather than silently marking an uncollected order delivered.
    #[test]
    fn skipping_a_state_is_refused() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        assert!(o.delivered().is_err(), "a placed order cannot jump to delivered");
        assert_eq!(o.status, OrderStatus::Placed);
    }

    /// Delivering with a leg still pending would pay a vendor whose goods were
    /// never collected.
    #[test]
    fn collection_is_refused_while_a_leg_is_pending() {
        let mut o = order(vec![leg(10_000, 1000), leg(5_000, 1000)], 4_900, 0, 3_500);
        o.courier_offered().unwrap();
        o.courier_claimed(Uuid::new_v4(), None).unwrap();

        o.legs[0].mark_picked_up();
        assert!(o.all_legs_collected().is_err(), "one leg is still pending");

        o.legs[1].mark_picked_up();
        assert!(o.all_legs_collected().is_ok());
    }

    /// A failed leg does not block the trip — the courier delivers what was
    /// collected and the failed leg is refunded separately.
    #[test]
    fn a_failed_leg_does_not_block_collection() {
        let mut o = order(vec![leg(10_000, 1000), leg(5_000, 1000)], 4_900, 0, 3_500);
        o.courier_offered().unwrap();
        o.courier_claimed(Uuid::new_v4(), None).unwrap();

        o.legs[0].mark_picked_up();
        o.legs[1].mark_failed();

        assert!(o.all_legs_collected().is_ok(), "a failed leg is resolved, not pending");
    }

    /// Every leg failing means there is nothing to deliver.
    #[test]
    fn an_order_with_no_collected_legs_cannot_be_delivered() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        o.courier_offered().unwrap();
        o.courier_claimed(Uuid::new_v4(), None).unwrap();
        o.legs[0].mark_failed();

        assert!(o.all_legs_collected().is_err(), "nothing was collected");
    }

    #[test]
    fn a_cancelled_order_accepts_no_further_transitions() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        o.cancel().unwrap();
        assert!(o.courier_offered().is_err());
        assert!(o.delivered().is_err());
    }

    /// A delivered order cannot be cancelled. Flipping it would drop a completed
    /// order out of settlement while the vendor and courier have already been
    /// credited — undoing a delivery is a refund, with its own money movement.
    #[test]
    fn a_delivered_order_cannot_be_cancelled() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        o.courier_offered().unwrap();
        o.courier_claimed(Uuid::new_v4(), None).unwrap();
        o.legs[0].mark_picked_up();
        o.all_legs_collected().unwrap();
        o.delivered().unwrap();

        assert!(o.cancel().is_err());
        assert_eq!(o.status, OrderStatus::Delivered, "the delivery stands");
    }

    /// A courier claiming straight from Placed is legal: the offer event and
    /// the claim can arrive out of order, and refusing the claim would strand
    /// an order whose courier is already on the way.
    #[test]
    fn a_claim_without_a_preceding_offer_is_accepted() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        let assignment = Uuid::new_v4();

        assert!(o.courier_claimed(assignment, None).is_ok());
        assert_eq!(o.status, OrderStatus::Collecting);
        assert_eq!(o.courier_task_id, Some(assignment));
    }

    /// A COD order never touches `with_payment` — every existing call site of
    /// `Order::place` (production and every test file in this crate) still
    /// constructs exactly this order, byte-identical to before this feature.
    #[test]
    fn a_default_order_is_cod_and_fully_collectible_at_the_door() {
        let o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        assert_eq!(o.payment_method, PaymentMethod::Cod);
        assert_eq!(o.payment_status, PaymentStatus::Pending);
        assert_eq!(o.prepaid_amount_cents, 0);
        assert_eq!(o.payment_intent_id, None);
        assert_eq!(
            o.cod_amount_cents(), o.grand_total_cents,
            "with nothing prepaid, the courier collects the entire grand total \
             — exactly today's behavior",
        );
    }
}

#[cfg(test)]
mod prepaid_checkout {
    use super::*;

    fn leg(subtotal: i64, bps: i32) -> VendorLeg {
        VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), subtotal, bps)
    }

    fn cod_order() -> Order {
        Order::place(
            Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![leg(34_000, 1500)], 7_900, 4_000, 5_800, 14.5995, 120.9842,
        )
    }

    fn online_order(prepaid_amount_cents: i64) -> Order {
        cod_order().with_payment(PaymentMethod::Online, prepaid_amount_cents)
    }

    fn payable_online_order() -> Order {
        let mut o = online_order(0);
        o.payment_checkout_url = Some("https://ni.example/pay/abc".into());
        o
    }

    /// The reason the field exists: an order the customer walked away from
    /// mid-payment must be finishable, not silently dead until the expiry
    /// sweep cancels it.
    #[test]
    fn an_unpaid_online_order_offers_its_checkout_page_back() {
        assert_eq!(
            payable_online_order().resumable_checkout_url(),
            Some("https://ni.example/pay/abc"),
        );
    }

    /// The gate that matters. Once a hold exists, re-opening the page invites
    /// a second authorization against the same order — one nothing would ever
    /// capture and nothing would ever void, because capture and void both only
    /// know `payment_intent_id`.
    #[test]
    fn a_page_is_never_offered_again_once_the_payment_has_moved_on() {
        for status in [
            PaymentStatus::Authorized,
            PaymentStatus::Captured,
            PaymentStatus::Voided,
            PaymentStatus::Failed,
        ] {
            let mut o = payable_online_order();
            o.payment_status = status;
            assert_eq!(
                o.resumable_checkout_url(), None,
                "{status:?} must not hand back a way to pay again",
            );
        }
    }

    /// A cancelled order is not payable whatever its payment status says —
    /// the recovery sweep cancels an abandoned checkout before its own void,
    /// so this pair genuinely co-occurs.
    #[test]
    fn a_cancelled_order_is_not_payable() {
        let mut o = payable_online_order();
        o.status = OrderStatus::Cancelled;
        assert_eq!(o.resumable_checkout_url(), None);
    }

    /// COD has no page and must never appear to have one, even if a row
    /// somehow carried a URL.
    #[test]
    fn a_cod_order_never_offers_a_checkout_page() {
        let mut o = cod_order();
        o.payment_checkout_url = Some("https://ni.example/pay/abc".into());
        assert_eq!(o.resumable_checkout_url(), None);
    }

    /// The formula the whole feature rests on: not a binary switch between
    /// "collect everything" and "collect nothing."
    #[test]
    fn cod_amount_is_the_grand_total_minus_whatever_was_prepaid() {
        let o = online_order(0);
        assert_eq!(o.cod_amount_cents(), o.grand_total_cents, "nothing prepaid yet");

        let fully_prepaid = online_order(o.grand_total_cents);
        assert_eq!(fully_prepaid.cod_amount_cents(), 0, "a fully online order collects nothing");

        // The case the design exists for: goods paid online, tip left in cash.
        let partial = o.tip_cents; // whatever isn't prepaid
        let mixed = online_order(o.grand_total_cents - partial);
        assert_eq!(mixed.cod_amount_cents(), partial, "the remainder is exactly what wasn't prepaid");
        assert!(
            mixed.cod_amount_cents() > 0 && mixed.cod_amount_cents() < mixed.grand_total_cents,
            "a genuinely partial prepay, not one of the two extremes",
        );
    }

    #[test]
    fn payment_authorized_moves_pending_to_authorized_and_stamps_the_intent() {
        let mut o = online_order(o_total(&cod_order()));
        let intent = Uuid::new_v4();

        assert!(o.payment_authorized(intent).is_ok());
        assert_eq!(o.payment_status, PaymentStatus::Authorized);
        assert_eq!(o.payment_intent_id, Some(intent));
        assert!(o.payment_authorized_at.is_some());
    }

    fn o_total(o: &Order) -> i64 { o.grand_total_cents }

    /// Kafka redelivers `payment.intent.authorized` at least once — a repeat
    /// must not error and must not re-stamp `payment_authorized_at`.
    #[test]
    fn a_repeated_authorization_is_idempotent() {
        let mut o = online_order(1);
        o.payment_authorized(Uuid::new_v4()).unwrap();
        let first_stamp = o.payment_authorized_at;

        assert!(o.payment_authorized(Uuid::new_v4()).is_ok(), "a duplicate must not error");
        assert_eq!(o.payment_authorized_at, first_stamp, "and must not advance again");
    }

    #[test]
    fn capture_requires_an_authorized_intent() {
        let mut o = online_order(1);
        assert!(o.payment_captured().is_err(), "nothing was authorized yet");

        o.payment_authorized(Uuid::new_v4()).unwrap();
        assert!(o.payment_captured().is_ok());
        assert_eq!(o.payment_status, PaymentStatus::Captured);
    }

    #[test]
    fn void_requires_an_authorized_intent_and_the_customer_is_never_charged() {
        let mut o = online_order(1);
        assert!(o.payment_voided().is_err(), "nothing was authorized yet");

        o.payment_authorized(Uuid::new_v4()).unwrap();
        assert!(o.payment_voided().is_ok());
        assert_eq!(o.payment_status, PaymentStatus::Voided);
    }

    /// Once captured or voided, the payment status is terminal — a stray
    /// second capture or void attempt must be refused, not silently repeated
    /// against the gateway.
    #[test]
    fn a_terminal_payment_status_refuses_further_transitions() {
        let mut captured = online_order(1);
        captured.payment_authorized(Uuid::new_v4()).unwrap();
        captured.payment_captured().unwrap();
        assert!(captured.payment_voided().is_err());

        let mut voided = online_order(1);
        voided.payment_authorized(Uuid::new_v4()).unwrap();
        voided.payment_voided().unwrap();
        assert!(voided.payment_captured().is_err());
    }

    #[test]
    fn payment_failed_is_reachable_before_or_after_authorization() {
        let mut before = online_order(1);
        assert!(before.payment_failed().is_ok());
        assert_eq!(before.payment_status, PaymentStatus::Failed);

        let mut after = online_order(1);
        after.payment_authorized(Uuid::new_v4()).unwrap();
        assert!(after.payment_failed().is_ok());
        assert_eq!(after.payment_status, PaymentStatus::Failed);
    }

    /// A COD order's settlement is funded from cash the courier collected —
    /// unchanged from today.
    #[test]
    fn cod_settlement_is_funded_by_courier_collected_cash() {
        let o = cod_order();
        assert_eq!(o.settlement().funding, FundingSource::CourierCollectedCash);
    }

    /// A fully-prepaid order's courier collects nothing, so their earnings
    /// are owed from the digital pool, not netted against remitted cash.
    #[test]
    fn a_fully_prepaid_settlement_is_funded_from_the_digital_pool() {
        let o = online_order(o_total(&cod_order()));
        assert_eq!(o.settlement().funding, FundingSource::DigitalPool);
    }

    /// A partially-prepaid order's settlement names both pools and the exact
    /// split between them.
    #[test]
    fn a_partially_prepaid_settlement_names_the_split() {
        let full = o_total(&cod_order());
        let o = online_order(full - 4_000); // everything but the tip
        let funding = o.settlement().funding;
        assert_eq!(
            funding,
            FundingSource::Mixed { cod_amount_cents: 4_000, prepaid_amount_cents: full - 4_000 },
        );
    }

    /// Whatever the funding source, the four-leg balance identity — the
    /// invariant `settlement_invariant.rs` sweeps — must still hold exactly.
    /// Prepaying changes *where* the money came from, never *how much* is
    /// owed to whom.
    #[test]
    fn the_balance_identity_holds_regardless_of_funding_source() {
        for prepaid in [0, 20_000, o_total(&cod_order())] {
            let o = online_order(prepaid);
            let s = o.settlement();
            assert_eq!(
                o.grand_total_cents,
                s.vendor_payouts_cents + s.commissions_cents + s.courier_earnings_cents + s.partner_margin_cents,
                "prepaid={prepaid} must still balance",
            );
        }
    }
}

#[cfg(test)]
mod customer_contact {
    use super::*;

    /// Identity mints `<digits>@customer.logisticos.app` for OTP sign-ins, so
    /// the phone a courier needs is already in the caller's own token.
    #[test]
    fn a_phone_derived_address_yields_the_phone() {
        assert_eq!(
            phone_from_login("639170000123@customer.logisticos.app"),
            Some("639170000123".to_string())
        );
        assert_eq!(
            phone_from_login("639170000123@driver.logisticos.app"),
            Some("639170000123".to_string())
        );
    }

    /// The case that makes this a function rather than a `split('@')`. A real
    /// mailbox is not a phone, and "maria.reyes" on a courier's screen as a
    /// number to dial is worse than no number at all.
    #[test]
    fn a_real_address_yields_nothing() {
        assert_eq!(phone_from_login("maria.reyes@gmail.com"), None);
        assert_eq!(phone_from_login("merchant@demo.com"), None);
        assert_eq!(phone_from_login("admin@logisticos.app"), None);
    }

    /// The minted namespace is digits by construction. Anything else in it did
    /// not come from the OTP path.
    #[test]
    fn a_non_numeric_local_part_in_the_minted_namespace_yields_nothing() {
        assert_eq!(phone_from_login("admin@customer.logisticos.app"), None);
        assert_eq!(phone_from_login("@customer.logisticos.app"), None);
        assert_eq!(phone_from_login("6391-7000@customer.logisticos.app"), None);
    }

    fn an_order() -> Order {
        let leg = VendorLeg::settle(Uuid::from_u128(1), Uuid::new_v4(), 10_000, 1_500);
        Order::place(
            Uuid::from_u128(1), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![leg], 4_900, 0, 3_500, 14.5547, 121.0244,
        )
    }

    #[test]
    fn an_order_carries_the_contact_it_was_placed_with() {
        let o = an_order().with_customer_contact(
            Some("Maria Reyes".to_string()),
            Some("639170000123".to_string()),
        );
        assert_eq!(o.customer_name.as_deref(), Some("Maria Reyes"));
        assert_eq!(o.customer_phone.as_deref(), Some("639170000123"));
    }

    /// Orders placed before migration 0019 have no contact, and the manifest
    /// must render without one rather than refuse to load.
    #[test]
    fn an_order_without_a_contact_is_legal() {
        let o = an_order();
        assert!(o.customer_phone.is_none());
        assert!(o.customer_name.is_none());
    }
}
