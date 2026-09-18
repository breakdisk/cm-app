//! ShipmentService — orchestrates order intake business logic.

use std::sync::Arc;
use chrono::Utc;
use logisticos_events::{Event, payloads::{AwbIssued, ShipmentCreated}, topics};
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{
    awb::ServiceCode,
    ShipmentId, MerchantId, CustomerId, Money, Currency, ShipmentStatus, TenantId,
};
use crate::domain::value_objects::cancel_authority::{may_act_on, ActingAs};
use crate::domain::value_objects::cancellation_policy::{quote_cancellation, CancelTier, CancellationPolicy};

/// Refuse a by-id action the caller has no authority over.
///
/// A refusal is a 404, not a 403, so a caller cannot probe which shipment ids
/// exist in another tenant. `Unset` is a programming error, not a caller error.
pub(crate) fn authorize_shipment(acting_as: &ActingAs, shipment: &Shipment) -> AppResult<()> {
    let permitted = match acting_as {
        ActingAs::Unset => {
            return Err(AppError::Internal(anyhow::anyhow!(
                "by-id action on shipment {} reached the service with no actor",
                shipment.id.inner()
            )))
        }
        ActingAs::System => true,
        ActingAs::User(actor) => {
            may_act_on(actor, shipment.tenant_id.inner(), shipment.merchant_id.inner())
        }
    };
    if permitted {
        Ok(())
    } else {
        Err(AppError::NotFound { resource: "Shipment", id: shipment.id.inner().to_string() })
    }
}

use crate::{
    application::commands::{
        CreateShipmentCommand, CancelShipmentCommand, RescheduleShipmentCommand,
        BulkCreateShipmentCommand, BulkCreateResult, BulkRowError,
    },
    domain::{
        entities::{
            shipment::{PaymentRequirement, Shipment},
            piece::Piece,
        },
        value_objects::{
            ServiceType, ShipmentWeight, ShipmentDimensions,
            AwbGenerator, generate_child_awbs,
        },
    },
};

pub struct ShipmentListFilter {
    pub tenant_id:   uuid::Uuid,
    pub merchant_id: Option<uuid::Uuid>,
    pub status:      Option<String>,
    /// Inclusive lower bound on `updated_at` — used by billing queries to
    /// window shipments delivered within a billing period.
    pub updated_from: Option<chrono::DateTime<chrono::Utc>>,
    /// Exclusive upper bound on `updated_at`.
    pub updated_to:   Option<chrono::DateTime<chrono::Utc>>,
    pub limit:       i64,
    pub offset:      i64,
}

/// An immutable timeline entry to append to `order_intake.shipment_events`.
/// Created at lifecycle milestones (created, confirmed, …) so the admin and
/// customer tracking timelines can show the date/time/location of each step.
pub struct NewShipmentEvent {
    pub shipment_id: ShipmentId,
    pub tenant_id:   uuid::Uuid,
    pub event_type:  String,
    pub from_status: Option<String>,
    pub to_status:   String,
    pub actor_type:  String,
    /// City-level location label stamped into the event metadata.
    pub location:    Option<String>,
}

/// The tenant an event about this shipment belongs to.
fn cmd_tenant_of(s: &Shipment) -> uuid::Uuid {
    s.tenant_id.inner()
}

pub trait ShipmentRepository: Send + Sync {
    fn find_by_id<'a>(
        &'a self,
        id: &'a ShipmentId,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<Shipment>>> + Send + 'a>>;

    fn save<'a>(
        &'a self,
        shipment: &'a Shipment,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>;

    /// Idempotent re-submission lookup, scoped per tenant.
    fn find_by_idempotency_key<'a>(
        &'a self,
        tenant_id: uuid::Uuid,
        idempotency_key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<Shipment>>> + Send + 'a>>;

    /// Shipments still `awaiting_payment` past the given cutoff — the sweep target.
    fn find_awaiting_payment_older_than<'a>(
        &'a self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<Shipment>>> + Send + 'a>>;

    /// Cancel a shipment *only if* it is still awaiting payment, in one
    /// statement. Returns whether it was cancelled.
    ///
    /// The expiry sweep cannot use the read-modify-write `cancel()` path: a
    /// payment capture landing between that read and its write would be
    /// overwritten by the sweep's stale copy, leaving a shipment cancelled
    /// with the customer's payment erased from this service's record. The
    /// condition therefore has to be evaluated by the database, at write time.
    fn cancel_if_awaiting_payment<'a>(
        &'a self,
        shipment_id: uuid::Uuid,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<bool>> + Send + 'a>>;

    fn save_pieces<'a>(
        &'a self,
        pieces: &'a [Piece],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>;

    // Hand-written boxed future for an object-safe async trait method — the
    // shape is the point; a type alias would only hide it.
    #[allow(clippy::type_complexity)]
    fn list<'a>(
        &'a self,
        filter: &'a ShipmentListFilter,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<(Vec<Shipment>, i64)>> + Send + 'a>>;

    /// Append an immutable timeline event. Default no-op so non-DB test doubles
    /// don't need to implement it; the Postgres repo overrides this.
    fn record_event<'a>(
        &'a self,
        _event: NewShipmentEvent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

pub trait EventPublisher: Send + Sync {
    fn publish<'a>(
        &'a self,
        topic: &'a str,
        key: &'a str,
        payload: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>;
}

pub trait AddressNormalizer: Send + Sync {
    fn normalize<'a>(
        &'a self,
        input: &'a crate::application::commands::AddressInput,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<logisticos_types::Address>> + Send + 'a>>;
}

/// Threshold above which an uncategorized shipment is treated as "heavy" for
/// driver-app iconography and dispatch vehicle matching (20 kg).
const HEAVY_WEIGHT_GRAMS: u32 = 20_000;

/// Resolve the delivery category for the driver task card / vehicle matching.
/// An explicit valid category from the booking wins; otherwise derive:
/// balikbayan → "large" (big shipment), ≥ 20 kg → "heavy", else "parcel".
fn resolve_delivery_category(
    requested: Option<&str>,
    service_type: &str,
    weight_grams: u32,
) -> Result<String, String> {
    if let Some(cat) = requested {
        return match cat {
            "food" | "parcel" | "grocery" | "medicine" | "heavy" | "large" => Ok(cat.to_string()),
            other => Err(format!("Unknown delivery category: {other}")),
        };
    }
    Ok(if service_type == "balikbayan" {
        "large".to_string()
    } else if weight_grams >= HEAVY_WEIGHT_GRAMS {
        "heavy".to_string()
    } else {
        "parcel".to_string()
    })
}

/// Human-readable SLA label for the estimated delivery field on `ShipmentCreated`.
fn sla_label(service_type: &str) -> &'static str {
    match service_type {
        "express"       => "Next business day",
        "same_day"      => "Today",
        "balikbayan"    => "21–45 business days",
        "international" => "7–14 business days",
        _               => "2–3 business days",  // standard (default)
    }
}

/// Bundles the collaborators for the optional "pay online at booking"
/// capability — `PaymentsClient`, the quote-token signing secret, and the
/// checkout return-URL base. Grouped into one struct, held as
/// `Option<PaymentCapability>` on `ShipmentService`, rather than three
/// separate `Option` fields: the three are only ever meaningful together
/// (`Config::payment_config()` in `crate::config` already enforces "all
/// three or none" at the config layer), and a single `Option` makes an
/// inconsistent half-enabled combination unrepresentable here too — there is
/// no way to construct a `ShipmentService` with, say, a signing secret but no
/// client.
pub struct PaymentCapability {
    pub client: Arc<crate::infrastructure::http::PaymentsClient>,
    /// HMAC-SHA256 signing secret for short-TTL quote tokens
    /// (`domain::value_objects::quote_token`) — used to re-verify a token
    /// presented on `create()` before trusting its amount to charge, and to
    /// sign new quotes in `api::http::quote::get_quote`.
    pub quote_token_secret: String,
    /// Base URL merchants/customers land on after completing (or abandoning)
    /// a hosted checkout — the payment gateway's `return_url`.
    pub shipment_return_url_base: String,
}

pub struct ShipmentService {
    pub repo:          Arc<dyn ShipmentRepository>,
    pub publisher:     Arc<dyn EventPublisher>,
    pub normalizer:    Arc<dyn AddressNormalizer>,
    pub awb_generator: Arc<dyn AwbGenerator>,
    /// `None` when the deployment has no payment config (see
    /// `Config::payment_config()`). A booking that carries a `quote_token`
    /// while this is `None` is rejected in `create()` rather than falling
    /// through to the cash/free path — see the check right after the
    /// idempotency replay, before any other work.
    pub payment: Option<PaymentCapability>,
    /// Mesh-internal carrier client, for pricing a consumer move off a rate
    /// card. `None` when `SERVICES__CARRIER_URL` is unset, in which case a move
    /// quote returns 503 rather than silently falling back to the parcel tariff.
    pub carrier: Option<Arc<crate::infrastructure::http::CarrierClient>>,
    /// Accessorial rate card. Empty by default, which simply means this
    /// deployment offers none -- each entry is independently optional.
    pub accessorials: crate::config::AccessorialsConfig,
    /// Cancellation fee policy. Rates default to 0 bps.
    pub cancellation_policy: CancellationPolicy,
    /// Mesh-internal promotions client: prices a promo code into a quote and
    /// spends it at booking. `None` when `SERVICES__PROMOTIONS_URL` is unset.
    pub promotions: Option<Arc<crate::infrastructure::http::PromotionsClient>>,
}

/// What a verified quote was discounted by, all spent at booking.
struct PricedDiscounts {
    code: Option<String>,
    lines: Vec<crate::domain::value_objects::quote_token::TokenDiscount>,
    account_id: uuid::Uuid,
}

/// Returned by `create()`: the persisted shipment, plus a checkout URL when
/// the booking required payment (`payment_status == AwaitingPayment`).
/// `checkout_url` is `None` for every other path, including an idempotent
/// replay of an already-paid-for booking.
pub struct CreateShipmentResult {
    pub shipment: Shipment,
    pub checkout_url: Option<String>,
}

impl ShipmentService {
    // One argument per collaborator; a builder would only rename the list.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo:          Arc<dyn ShipmentRepository>,
        publisher:     Arc<dyn EventPublisher>,
        normalizer:    Arc<dyn AddressNormalizer>,
        awb_generator: Arc<dyn AwbGenerator>,
        payment: Option<PaymentCapability>,
        carrier: Option<Arc<crate::infrastructure::http::CarrierClient>>,
        accessorials: crate::config::AccessorialsConfig,
        cancellation_policy: CancellationPolicy,
    ) -> Self {
        Self { repo, publisher, normalizer, awb_generator, payment, carrier, accessorials, cancellation_policy, promotions: None }
    }

    #[must_use]
    pub fn with_promotions(mut self, promotions: Option<Arc<crate::infrastructure::http::PromotionsClient>>) -> Self {
        self.promotions = promotions;
        self
    }

    pub async fn create(&self, cmd: CreateShipmentCommand) -> AppResult<CreateShipmentResult> {
        tracing::info!(step = "enter", "ShipmentService::create");

        // ── Idempotent replay ──────────────────────────────────────────────────
        // A client retrying a request with the same key (e.g. after a timed-out
        // response whose success it never saw) gets back the shipment already
        // created for that key instead of creating a duplicate — and, on the
        // payment-aware path, instead of opening a second payment intent for
        // money already being collected. No checkout_url: a replay by definition
        // already went through the checkout flow (or never needed to) the first
        // time around.
        if let Some(key) = cmd.idempotency_key.as_deref() {
            if let Some(existing) = self.repo.find_by_idempotency_key(cmd.tenant_id, key).await
                .map_err(AppError::Internal)?
            {
                tracing::info!(shipment_id = %existing.id, "create: idempotent replay — returning existing shipment");
                return Ok(CreateShipmentResult { shipment: existing, checkout_url: None });
            }
        }

        // ── Reject a quote_token when online payment isn't configured ────────
        // Checked immediately after the idempotency replay and before any real
        // work (address normalization/geocoding, AWB sequence allocation) so a
        // deployment with payment disabled doesn't burn a Redis/Postgres AWB
        // number on a booking that's about to be rejected anyway. The part
        // that actually matters: a `quote_token` must never be allowed to fall
        // through to the free/cash path below — silently booking it for cash
        // would let a customer who believes they've already paid receive a
        // shipment that was never actually charged. So this is a hard reject,
        // not a "treat it as if there were no token" fallback.
        if cmd.quote_token.is_some() && self.payment.is_none() {
            return Err(AppError::ServiceUnavailable(
                "Online payment is not configured for this deployment — a booking \
                 carrying a quote_token cannot be processed".into(),
            ));
        }

        // ── Validate service type ────────────────────────────────────────────
        let service_type = ServiceType::parse(&cmd.service_type).map_err(AppError::Validation)?;
        tracing::info!(step = "service_type_ok", ?service_type, "create");

        let service_code = match service_type {
            ServiceType::Standard      => ServiceCode::Standard,
            ServiceType::Express       => ServiceCode::Express,
            ServiceType::SameDay       => ServiceCode::SameDay,
            ServiceType::Balikbayan    => ServiceCode::Balikbayan,
            ServiceType::International => ServiceCode::International,
        };

        // ── Business rule: same-day cutoff at 14:00 ──────────────────────────
        if service_type == ServiceType::SameDay {
            let hour = Utc::now().format("%H").to_string().parse::<u32>().unwrap_or(0);
            if hour >= 14 {
                return Err(AppError::BusinessRule(
                    "Same-day orders must be placed before 14:00 local time".into(),
                ));
            }
        }

        // ── Validate weight ──────────────────────────────────────────────────
        // For Balikbayan/International with per-piece declarations the aggregate
        // weight is derived from pieces; skip the single-parcel 70 kg cap.
        let is_per_piece = matches!(service_type, ServiceType::Balikbayan | ServiceType::International)
            && cmd.pieces.as_ref().map(|p| !p.is_empty()).unwrap_or(false);
        let weight = ShipmentWeight::from_grams(cmd.weight_grams);
        if !is_per_piece {
            weight.validate().map_err(|e| AppError::Validation(e.to_string()))?;
        }

        // ── Resolve delivery category (icon + vehicle matching downstream) ───
        let delivery_category = resolve_delivery_category(
            cmd.delivery_category.as_deref(),
            cmd.service_type.as_str(),
            cmd.weight_grams,
        ).map_err(AppError::Validation)?;

        // ── Business rule: COD must not exceed declared value ────────────────
        if let (Some(cod), Some(declared)) = (cmd.cod_amount_cents, cmd.declared_value_cents) {
            if cod > declared {
                return Err(AppError::BusinessRule(
                    "COD amount cannot exceed declared value".into(),
                ));
            }
        }

        // ── Normalize and geocode addresses ──────────────────────────────────
        let origin      = self.normalizer.normalize(&cmd.origin).await.map_err(AppError::Internal)?;
        let destination = self.normalizer.normalize(&cmd.destination).await.map_err(AppError::Internal)?;
        tracing::info!(step = "normalized", "create");

        // ── Dimensions (shipment-level, for Standard/Express/SameDay) ─────────
        let dimensions = match (cmd.length_cm, cmd.width_cm, cmd.height_cm) {
            (Some(l), Some(w), Some(h)) => Some(ShipmentDimensions { length_cm: l, width_cm: w, height_cm: h }),
            _ => None,
        };

        // ── Generate master AWB ───────────────────────────────────────────────
        let tenant_code = logisticos_types::awb::TenantCode::new(&cmd.tenant_code)
            .map_err(|e| AppError::Validation(e.to_string()))?;
        tracing::info!(step = "tenant_code_ok", tenant_code = %tenant_code.as_str(), "create");
        let master_awb = self
            .awb_generator
            .next_awb(&tenant_code, service_code)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        tracing::info!(step = "awb_generated", awb = %master_awb.as_str(), "create");

        let now = Utc::now();
        let shipment_id = ShipmentId::new();

        // ── Build piece records (track-specific) ──────────────────────────────
        let (piece_count, pieces, shipment_weight_grams) = match service_type {
            ServiceType::Balikbayan | ServiceType::International => {
                match cmd.pieces.as_ref().filter(|p| !p.is_empty()) {
                    Some(piece_inputs) => {
                        // Per-piece declaration path (Balikbayan standard flow).
                        let count = piece_inputs.len() as u16;
                        if count > 999 {
                            return Err(AppError::Validation("Shipment cannot exceed 999 pieces.".into()));
                        }
                        let aggregate_grams: u32 = piece_inputs.iter().map(|p| p.weight_grams).sum();
                        let child_awbs = generate_child_awbs(&master_awb, count)
                            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
                        let piece_vec: Vec<Piece> = piece_inputs
                            .iter()
                            .zip(child_awbs.iter())
                            .enumerate()
                            .map(|(i, (input, child))| {
                                let dims = match (input.length_cm, input.width_cm, input.height_cm) {
                                    (Some(l), Some(w), Some(h)) => Some(ShipmentDimensions { length_cm: l, width_cm: w, height_cm: h }),
                                    _ => None,
                                };
                                Piece {
                                    id:               uuid::Uuid::new_v4(),
                                    shipment_id:      shipment_id.clone(),
                                    piece_number:     (i + 1) as u16,
                                    piece_awb:        child.clone(),
                                    declared_weight:  ShipmentWeight::from_grams(input.weight_grams),
                                    actual_weight:    None,
                                    dimensions:       dims,
                                    description:      input.description.clone(),
                                    status:           logisticos_types::PieceStatus::Pending,
                                    last_hub_id:      None,
                                    last_scanned_at:  None,
                                    created_at:       now,
                                    updated_at:       now,
                                }
                            })
                            .collect();
                        (count, piece_vec, aggregate_grams)
                    }
                    None => {
                        // Fallback: uniform pieces from piece_count + aggregate weight.
                        let count = cmd.piece_count.unwrap_or(1).clamp(1, 999);
                        let child_awbs = generate_child_awbs(&master_awb, count)
                            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
                        let piece_weight = ShipmentWeight::from_grams(weight.grams / count as u32);
                        let piece_vec: Vec<Piece> = child_awbs.iter().map(|child| Piece {
                            id:              uuid::Uuid::new_v4(),
                            shipment_id:     shipment_id.clone(),
                            piece_number:    child.piece_number(),
                            piece_awb:       child.clone(),
                            declared_weight: piece_weight,
                            actual_weight:   None,
                            dimensions,
                            description:     cmd.description.clone(),
                            status:          logisticos_types::PieceStatus::Pending,
                            last_hub_id:     None,
                            last_scanned_at: None,
                            created_at:      now,
                            updated_at:      now,
                        }).collect();
                        (count, piece_vec, weight.grams)
                    }
                }
            }
            _ => {
                // Standard / Express / SameDay — pieces array must not be provided.
                if cmd.pieces.as_ref().map(|p| !p.is_empty()).unwrap_or(false) {
                    return Err(AppError::Validation(
                        "Per-piece declarations are only valid for Balikbayan and International shipments.".into()
                    ));
                }
                let count = cmd.piece_count.unwrap_or(1).clamp(1, 999);
                let child_awbs = generate_child_awbs(&master_awb, count)
                    .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
                let piece_weight = ShipmentWeight::from_grams(weight.grams / count as u32);
                let piece_vec: Vec<Piece> = child_awbs.iter().map(|child| Piece {
                    id:              uuid::Uuid::new_v4(),
                    shipment_id:     shipment_id.clone(),
                    piece_number:    child.piece_number(),
                    piece_awb:       child.clone(),
                    declared_weight: piece_weight,
                    actual_weight:   None,
                    dimensions,
                    description:     cmd.description.clone(),
                    status:          logisticos_types::PieceStatus::Pending,
                    last_hub_id:     None,
                    last_scanned_at: None,
                    created_at:      now,
                    updated_at:      now,
                }).collect();
                (count, piece_vec, weight.grams)
            }
        };

        // ── Billable weight at shipment level ─────────────────────────────────
        let billable_grams = dimensions
            .map(|d| d.volumetric_weight_grams().max(shipment_weight_grams))
            .unwrap_or(shipment_weight_grams);

        // ── Verify the quote token (if any) and decide whether payment gates
        //    dispatch ─────────────────────────────────────────────────────────
        // Verified before the shipment row is written so a bad/tampered/expired
        // token fails the request before anything is persisted.
        let mut promo: Option<PricedDiscounts> = None;
        let (payment_status, verified_amount_cents, verified_currency) = match &cmd.quote_token {
            None => (PaymentRequirement::NotRequired, None, None),
            Some(token) => {
                // `.expect` is safe: the guard immediately after the
                // idempotency check above already rejected any request that
                // reaches here with `self.payment` still `None`.
                let payment = self.payment.as_ref().expect(
                    "quote_token present implies self.payment is Some — enforced by \
                     the early guard in create()"
                );
                let payload = crate::domain::value_objects::quote_token::verify(
                    payment.quote_token_secret.as_bytes(), token,
                ).map_err(|e| AppError::Validation(format!("Invalid quote: {e}")))?;
                if payload.tenant_id != cmd.tenant_id {
                    return Err(AppError::Validation("Quote token does not belong to this tenant".into()));
                }
                if payload.service_type != cmd.service_type || payload.weight_grams != cmd.weight_grams {
                    return Err(AppError::Validation(
                        "Quote token does not match this booking's service type or weight".into(),
                    ));
                }
                let lines = payload.discount_lines();
                if !lines.is_empty() {
                    // A code, a tier and credit are all one account's, so the
                    // discount they bought books for that account only.
                    if payload.account_id != Some(cmd.merchant_id) {
                        return Err(AppError::Validation(
                            "This quote's discount was priced for a different account — get a new quote".into(),
                        ));
                    }
                    promo = Some(PricedDiscounts {
                        code: payload.promo_code.clone().filter(|_| lines.iter().any(|l| l.kind == "code")),
                        lines,
                        account_id: cmd.merchant_id,
                    });
                }
                (PaymentRequirement::AwaitingPayment, Some(payload.amount_cents), Some(payload.currency))
            }
        };

        // ── Build shipment record ─────────────────────────────────────────────
        let mut shipment = Shipment {
            id: shipment_id.clone(),
            tenant_id: TenantId::from_uuid(cmd.tenant_id),
            merchant_id: MerchantId::from_uuid(cmd.merchant_id),
            customer_id: CustomerId::new(),
            customer_name: cmd.customer_name.clone(),
            customer_phone: cmd.customer_phone.clone(),
            customer_email: cmd.customer_email.clone(),
            booked_by_customer: cmd.booked_by_customer,
            auto_dispatch: cmd.auto_dispatch.unwrap_or(true),
            awb: master_awb.clone(),
            piece_count,
            status: ShipmentStatus::Pending,
            service_type,
            origin,
            destination,
            weight: ShipmentWeight::from_grams(billable_grams),
            dimensions,
            declared_value: cmd.declared_value_cents.map(|v| Money::new(v, Currency::PHP)),
            cod_amount: cmd.cod_amount_cents.map(|v| Money::new(v, Currency::PHP)),
            special_instructions: cmd.special_instructions,
            merchant_reference: cmd.merchant_reference.clone(),
            source_platform: cmd.source_platform.clone(),
            external_order_id: cmd.external_order_id.clone(),
            payment_intent_id: None,
            payment_status,
            pending_dispatch_events: None,
            idempotency_key: cmd.idempotency_key.clone(),
            // Nothing books a slot yet; whole-home moving (part 2, Part A) will.
            scheduled_pickup_at: None,
            cancellation_policy_version: Some(self.cancellation_policy.version.clone()),
            booking_amount_cents: verified_amount_cents,
            booking_currency: verified_currency.clone(),
            created_at: now,
            updated_at: now,
        };

        // ── Build the lifecycle events (payload construction only — nothing is
        //    published yet; whether these fire now or wait for payment is
        //    decided below) ────────────────────────────────────────────────────
        let awb_event = Event::new(
            "logisticos/order-intake",
            "awb.issued",
            cmd.tenant_id,
            AwbIssued {
                awb:          master_awb.as_str().to_string(),
                tenant_id:    cmd.tenant_id,
                shipment_id:  shipment.id.inner(),
                merchant_id:  cmd.merchant_id,
                service_code: service_code.as_str().to_string(),
                sequence:     master_awb.sequence(),
                piece_count,
                issued_at:    now.to_rfc3339(),
            },
        );

        let total_fee_cents = shipment.compute_base_fee_with_pieces(&pieces).amount;
        let event = Event::new(
            "logisticos/order-intake",
            "shipment.created",
            cmd.tenant_id,
            ShipmentCreated {
                shipment_id:          shipment.id.inner(),
                merchant_id:          cmd.merchant_id,
                customer_id:          shipment.customer_id.inner(),
                customer_name:        cmd.customer_name.clone(),
                customer_phone:       cmd.customer_phone.clone(),
                customer_email:       cmd.customer_email.clone().unwrap_or_default(),
                origin_address:       format!("{}, {}", shipment.origin.city, shipment.origin.province),
                origin_city:          shipment.origin.city.clone(),
                origin_province:      shipment.origin.province.clone(),
                origin_postal_code:   shipment.origin.postal_code.clone(),
                origin_lat:           shipment.origin.coordinates.map(|c| c.lat),
                origin_lng:           shipment.origin.coordinates.map(|c| c.lng),
                destination_address:  format!("{}, {}", shipment.destination.city, shipment.destination.province),
                destination_city:         shipment.destination.city.clone(),
                destination_province:     shipment.destination.province.clone(),
                destination_postal_code:  shipment.destination.postal_code.clone(),
                destination_lat:      shipment.destination.coordinates.map(|c| c.lat),
                destination_lng:      shipment.destination.coordinates.map(|c| c.lng),
                service_type:         service_type.as_str().into(),
                cod_amount_cents:     shipment.cod_amount.map(|m| m.amount),
                tracking_number:      master_awb.as_str().to_string(),
                total_fee_cents,
                currency:             "PHP".into(),
                weight_grams:         billable_grams,
                estimated_delivery:   sla_label(service_type.as_str()).to_string(),
                booked_by_customer:   shipment.booked_by_customer,
                auto_dispatch:        shipment.auto_dispatch,
                special_instructions: shipment.special_instructions.clone(),
                sender_name:          cmd.sender_name.clone(),
                sender_phone:         cmd.sender_phone.clone(),
                sender_email:         cmd.sender_email.clone(),
                merchant_name:        cmd.merchant_name.clone().unwrap_or_default(),
                delivery_category:    delivery_category.clone(),
            },
        );

        // Booking is confirmed synchronously (validated + AWB issued). Advances
        // the customer-facing tracking read-model (delivery-experience already
        // consumes this topic) and fires merchant webhooks. order-intake's own
        // status_consumer does NOT subscribe to this topic, so the canonical
        // `shipments.status` stays Pending until dispatch — matching the
        // "initial status must be Pending" invariant.
        let confirmed_event = Event::new(
            "logisticos/order-intake",
            "shipment.confirmed",
            cmd.tenant_id,
            serde_json::json!({ "shipment_id": shipment.id.inner() }),
        );

        // ── Hold for payment, or fall through to publish after persisting ──────
        // AwaitingPayment: none of the three events above fire yet — dispatch,
        // engagement, and analytics must not see this shipment until
        // payment.intent.captured (Task 19) republishes them unchanged. Instead
        // a payment intent is opened here (before the row is written, so a
        // failed payments call leaves nothing behind) and its checkout URL is
        // returned to the caller.
        //
        // Every other shipment publishes below, *after* the row is persisted —
        // the ordering this method has always had. Publishing first would let a
        // failed save leave dispatch/engagement/analytics acting on a shipment
        // that does not exist.
        let awb_json = serde_json::to_value(&awb_event).map_err(|e| AppError::Internal(e.into()))?;
        let created_json = serde_json::to_value(&event).map_err(|e| AppError::Internal(e.into()))?;
        let confirmed_json = serde_json::to_value(&confirmed_event).map_err(|e| AppError::Internal(e.into()))?;

        let mut checkout_url: Option<String> = None;

        // ── Spend what the quote was discounted by ───────────────────────────
        // Every line together, before the payment intent: a code or credit
        // another booking spent since the quote stops this one before any
        // money moves. The ledger decides a race; this booking's own retry is
        // not a second spend.
        let spent_code = match (&promo, &self.promotions) {
            (None, _) => false,
            (Some(_), None) => {
                return Err(AppError::ServiceUnavailable(
                    "This quote carries a discount and promotions is not configured here — get a new quote".into(),
                ))
            }
            (Some(p), Some(client)) => {
                let currency = verified_currency.clone().unwrap_or_default();
                client
                    .redeem(cmd.tenant_id, p.account_id, shipment.id.inner(), &currency, p.code.as_deref(), &p.lines)
                    .await
                    .map_err(|e| match e {
                        crate::infrastructure::http::RedeemError::Refused(_) => AppError::Conflict(
                            "PROMO_ALREADY_USED: this code was spent on another booking since the quote — get a new quote".into(),
                        ),
                        crate::infrastructure::http::RedeemError::CreditChanged => AppError::Conflict(
                            "CREDIT_CHANGED: your account credit was used on another booking since the quote — get a new quote".into(),
                        ),
                        crate::infrastructure::http::RedeemError::Unavailable(m) => {
                            AppError::ServiceUnavailable(format!("Promotions is unavailable, so the discounted quote cannot be booked: {m}"))
                        }
                    })?;
                true
            }
        };

        // Everything from here to the saved row either succeeds or gives the
        // code and credit back: a booking that never exists must not cost the
        // customer their month's code or their balance.
        let persisted: AppResult<()> = async {
        if payment_status == PaymentRequirement::AwaitingPayment {
            shipment.pending_dispatch_events = Some(serde_json::json!({
                "awb_issued": awb_json,
                "shipment_created": created_json,
                "shipment_confirmed": confirmed_json,
            }));

            let amount_cents = verified_amount_cents
                .expect("AwaitingPayment always comes from a verified quote token carrying an amount");
            let currency = verified_currency
                .expect("AwaitingPayment always comes from a verified quote token carrying a currency");
            // Same invariant as above: AwaitingPayment only happens once the
            // quote_token branch confirmed `self.payment` is Some.
            let payment = self.payment.as_ref().expect(
                "payment_status AwaitingPayment implies self.payment is Some — enforced \
                 by the early guard in create()"
            );
            let return_url = format!(
                "{}/payment/return?shipment_id={}",
                payment.shipment_return_url_base.trim_end_matches('/'),
                shipment.id,
            );
            let intent = payment.client
                .create_shipping_fee_intent(cmd.tenant_id, shipment.id.inner(), amount_cents, &currency, &return_url)
                .await
                .map_err(AppError::Internal)?;
            shipment.payment_intent_id = Some(intent.intent_id);
            checkout_url = Some(intent.checkout_url);
        }

        // ── Persist ───────────────────────────────────────────────────────────
        // Only now, once quote verification and any payment intent creation
        // have succeeded, so nothing is ever saved half-built (e.g.
        // AwaitingPayment without a payment_intent_id). Still before the
        // publishes below, so a published event always implies a stored row.
        self.repo.save(&shipment).await.map_err(|e| {
            tracing::error!(error = ?e, "shipment_repo.save failed");
            AppError::Internal(e)
        })?;
        tracing::info!(step = "shipment_saved", "create");
        self.repo.save_pieces(&pieces).await.map_err(|e| {
            tracing::error!(error = ?e, "shipment_repo.save_pieces failed");
            AppError::Internal(e)
        })?;
        tracing::info!(step = "pieces_saved", "create");
        Ok(())
        }
        .await;
        if let Err(e) = persisted {
            if spent_code {
                if let Some(client) = &self.promotions {
                    client.release(shipment.id.inner()).await;
                }
            }
            return Err(e);
        }

        // ── Stamp the opening timeline milestones ─────────────────────────────
        // A successful booking is received (`created` → pending) and validated +
        // AWB-issued (`confirmed`) synchronously. Both are stamped now so the
        // admin and customer timelines show a date/time/location for the earliest
        // steps. The stored status stays Pending (booking awaiting dispatch);
        // `confirmed` is a timeline milestone, not a status change here.
        // Runs after `save()`, not before: `shipment_events.shipment_id` carries
        // a FK to `shipments(id)` (migration 0002), so stamping first would fail
        // the constraint on every booking. Best-effort regardless: a timeline
        // write must never fail shipment creation.
        let actor_type = if shipment.booked_by_customer { "customer" } else { "merchant" };
        let pickup_city = Some(shipment.origin.city.clone());
        if let Err(e) = self.repo.record_event(NewShipmentEvent {
            shipment_id: shipment.id.clone(),
            tenant_id:   cmd.tenant_id,
            event_type:  "created".into(),
            from_status: None,
            to_status:   "pending".into(),
            actor_type:  actor_type.into(),
            location:    pickup_city.clone(),
        }).await {
            tracing::warn!(error = %e, shipment_id = %shipment.id, "created timeline event failed (non-fatal)");
        }
        if let Err(e) = self.repo.record_event(NewShipmentEvent {
            shipment_id: shipment.id.clone(),
            tenant_id:   cmd.tenant_id,
            event_type:  "confirmed".into(),
            from_status: Some("pending".into()),
            to_status:   "confirmed".into(),
            actor_type:  "system".into(),
            location:    pickup_city,
        }).await {
            tracing::warn!(error = %e, shipment_id = %shipment.id, "confirmed timeline event failed (non-fatal)");
        }

        // ── Publish the lifecycle events ──────────────────────────────────────
        // Skipped entirely for AwaitingPayment: those three payloads are held in
        // `pending_dispatch_events` above and republished verbatim once payment
        // captures, so dispatch never sees an unpaid shipment.
        if payment_status != PaymentRequirement::AwaitingPayment {
            if let Ok(payload) = serde_json::to_string(&awb_event) {
                let _ = self.publisher
                    .publish(topics::AWB_ISSUED, master_awb.as_str(), &payload)
                    .await;
            }

            let payload = serde_json::to_string(&event).map_err(|e| AppError::Internal(e.into()))?;
            // Fire-and-forget — Kafka unavailability must not prevent shipment creation.
            if let Err(e) = self.publisher
                .publish(topics::SHIPMENT_CREATED, &shipment.id.to_string(), &payload)
                .await
            {
                tracing::warn!(error = %e, shipment_id = %shipment.id, "ShipmentCreated event publish failed (non-fatal)");
            }

            if let Ok(p) = serde_json::to_string(&confirmed_event) {
                if let Err(e) = self.publisher
                    .publish(topics::SHIPMENT_CONFIRMED, &shipment.id.to_string(), &p)
                    .await
                {
                    tracing::warn!(error = %e, shipment_id = %shipment.id, "ShipmentConfirmed event publish failed (non-fatal)");
                }
            }
        }

        tracing::info!(
            shipment_id  = %shipment.id,
            awb          = %master_awb,
            piece_count,
            service_type = service_type.as_str(),
            "Shipment created"
        );
        Ok(CreateShipmentResult { shipment, checkout_url })
    }

    pub async fn cancel(&self, cmd: CancelShipmentCommand) -> AppResult<()> {
        let id = ShipmentId::from_uuid(cmd.shipment_id);
        let mut shipment = self.repo.find_by_id(&id).await.map_err(AppError::Internal)?
            .ok_or(AppError::NotFound { resource: "Shipment", id: cmd.shipment_id.to_string() })?;
        // Before the status check, so a refusal cannot leak whether the id exists.
        authorize_shipment(&cmd.acting_as, &shipment)?;

        if !shipment.can_cancel() {
            return Err(AppError::BusinessRule(
                format!("Cannot cancel shipment in status {:?}", shipment.status),
            ));
        }

        // Every cancellation is priced, even when the price is nothing. An
        // unscheduled one carries no retention, which payments reads as a full
        // refund: exactly what every cancellation was before the policy existed.
        let quote = quote_cancellation(
            &self.cancellation_policy,
            shipment.scheduled_pickup_at,
            Utc::now(),
            shipment.booking_amount_cents,
        );
        let mut data = serde_json::json!({ "shipment_id": shipment.id.inner(), "reason": cmd.reason });
        if quote.tier != CancelTier::Unscheduled {
            data["retention_bps"] = serde_json::json!(quote.retention_bps);
            data["cancellation_tier"] = serde_json::json!(quote.tier.as_str());
            data["policy_version"] = serde_json::json!(shipment
                .cancellation_policy_version
                .as_deref()
                .unwrap_or(&self.cancellation_policy.version));
        }

        shipment.status     = ShipmentStatus::Cancelled;
        shipment.updated_at = Utc::now();
        self.repo.save(&shipment).await.map_err(AppError::Internal)?;

        let event = Event::new(
            "logisticos/order-intake",
            "shipment.cancelled",
            // Was Uuid::nil(), so every tenant-scoped consumer saw a cancellation
            // that belonged to no tenant.
            shipment.tenant_id.inner(),
            data,
        );
        let payload = serde_json::to_string(&event).map_err(|e| AppError::Internal(e.into()))?;
        self.publisher
            .publish(topics::SHIPMENT_CANCELLED, &shipment.id.to_string(), &payload)
            .await
            .map_err(AppError::Internal)?;

        Ok(())
    }

    /// Cancels every shipment still `awaiting_payment` past `ttl_minutes`.
    /// Called by the periodic sweep in `bootstrap.rs`.
    ///
    /// A shipment that captures payment concurrently with this running is not
    /// double-handled: `find_awaiting_payment_older_than` only selects rows
    /// still `payment_status = 'awaiting_payment'`, and the payment-captured
    /// consumer moves a paid shipment to `Paid` before this sweep could see
    /// it. The payments service's own intent sweep also runs on a shorter
    /// interval than this TTL, so an expired intent normally cancels the
    /// shipment via `payment.intent.failed` well before this backstop fires.
    ///
    /// Pre-checks `can_cancel()` per row, the same pattern
    /// `payment_consumer::handle_failed` uses, rather than relying solely on
    /// `cancel()`'s own check: a shipment can legitimately move out of a
    /// cancellable status between the query above and this loop reaching it
    /// (e.g. a merchant-initiated cancel, or the captured-payment consumer,
    /// racing this sweep). That is a benign, expected race, not a failure —
    /// so it is skipped quietly here rather than falling into the
    /// `continue`-on-`Err` branch below, which is reserved for genuine
    /// failures (repo errors, publish errors) worth logging at `error!`.
    pub async fn sweep_expired_payments(&self, ttl_minutes: i64) -> AppResult<usize> {
        let cutoff = Utc::now() - chrono::Duration::minutes(ttl_minutes);
        let stale = self.repo.find_awaiting_payment_older_than(cutoff).await.map_err(AppError::Internal)?;
        let mut cancelled = 0usize;

        for shipment in stale {
            // Conditional at the database, not here: a capture that lands
            // between the query above and this write must win, and the row
            // read a moment ago cannot tell us whether that happened.
            let did_cancel = match self.repo.cancel_if_awaiting_payment(shipment.id.inner()).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(shipment_id = %shipment.id, error = ?e, "sweep: failed to cancel expired-payment shipment");
                    continue;
                }
            };

            if !did_cancel {
                // Paid, already cancelled, or moved on — all benign races.
                tracing::info!(shipment_id = %shipment.id, "sweep: shipment no longer awaiting payment, skipping");
                continue;
            }

            let event = Event::new(
                "logisticos/order-intake",
                "shipment.cancelled",
                cmd_tenant_of(&shipment),
                serde_json::json!({ "shipment_id": shipment.id.inner(), "reason": "payment_expired" }),
            );
            match serde_json::to_string(&event) {
                Ok(payload) => {
                    if let Err(e) = self.publisher
                        .publish(topics::SHIPMENT_CANCELLED, &shipment.id.to_string(), &payload)
                        .await
                    {
                        // The cancellation is already durable; a lost
                        // notification must not undo it or stop the sweep.
                        tracing::error!(shipment_id = %shipment.id, error = %e, "sweep: cancelled but failed to publish shipment.cancelled");
                    }
                }
                Err(e) => tracing::error!(shipment_id = %shipment.id, error = %e, "sweep: could not serialize shipment.cancelled"),
            }

            cancelled += 1;
        }

        Ok(cancelled)
    }

    pub async fn reschedule(&self, cmd: RescheduleShipmentCommand) -> AppResult<()> {
        let id = ShipmentId::from_uuid(cmd.shipment_id);
        let mut shipment = self.repo.find_by_id(&id).await.map_err(AppError::Internal)?
            .ok_or(AppError::NotFound { resource: "Shipment", id: cmd.shipment_id.to_string() })?;
        authorize_shipment(&cmd.acting_as, &shipment)?;

        if !shipment.can_reschedule() {
            return Err(AppError::BusinessRule(
                format!("Cannot reschedule shipment in status {:?}", shipment.status),
            ));
        }

        // Reset status to Confirmed so dispatch can re-assign
        shipment.status     = ShipmentStatus::Confirmed;
        shipment.updated_at = Utc::now();
        self.repo.save(&shipment).await.map_err(AppError::Internal)?;

        let event = Event::new(
            "logisticos/order-intake",
            "shipment.rescheduled",
            shipment.tenant_id.inner(),
            serde_json::json!({
                "shipment_id":          shipment.id.inner(),
                "preferred_date":       cmd.preferred_date.to_string(),
                "preferred_time_slot":  cmd.preferred_time_slot,
                "reason":               cmd.reason,
            }),
        );
        let payload = serde_json::to_string(&event).map_err(|e| AppError::Internal(e.into()))?;
        // Fire-and-forget — Kafka unavailability must not prevent rescheduling.
        if let Err(e) = self.publisher
            .publish(topics::SHIPMENT_RESCHEDULED, &shipment.id.to_string(), &payload)
            .await
        {
            tracing::warn!(error = %e, shipment_id = %shipment.id, "ShipmentRescheduled event publish failed (non-fatal)");
        }

        Ok(())
    }

    pub async fn override_status(
        &self,
        shipment_id: uuid::Uuid,
        new_status: ShipmentStatus,
        actor: &str,
        acting_as: &ActingAs,
    ) -> AppResult<()> {
        let id = ShipmentId::from_uuid(shipment_id);
        let mut shipment = self.repo.find_by_id(&id).await.map_err(AppError::Internal)?
            .ok_or(AppError::NotFound { resource: "Shipment", id: shipment_id.to_string() })?;
        authorize_shipment(acting_as, &shipment)?;

        let old_status = shipment.status;
        shipment.status     = new_status;
        shipment.updated_at = Utc::now();
        self.repo.save(&shipment).await.map_err(AppError::Internal)?;

        tracing::info!(
            shipment_id = %shipment_id,
            old_status  = ?old_status,
            new_status  = ?new_status,
            actor,
            "Admin status override"
        );
        Ok(())
    }

    pub async fn bulk_create(&self, cmd: BulkCreateShipmentCommand) -> AppResult<BulkCreateResult> {
        let mut created = Vec::new();
        let mut failed  = Vec::new();

        for (i, row) in cmd.rows.into_iter().enumerate() {
            let reference = row.merchant_reference.clone();
            match self.create(row).await {
                Ok(result) => created.push(result.shipment.id.inner()),
                Err(e) => failed.push(BulkRowError {
                    row_index: i,
                    merchant_reference: reference,
                    error: e.to_string(),
                }),
            }
        }

        tracing::info!(created = created.len(), failed = failed.len(), "Bulk shipment creation complete");
        Ok(BulkCreateResult { created, failed })
    }
}
