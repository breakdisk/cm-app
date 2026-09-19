//! Whole-home moving, the customer's side: the catalogue, the calendar, a
//! signed quote, and booking it.
//!
//! Every number here is the server's. The app sends the property, the rooms
//! and which catalogue items are in them, and two addresses; the volumes,
//! weights, distance and prices come from here, and the quote token carries
//! the whole normalised job so the booking cannot re-declare any of it.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use logisticos_auth::rbac::permissions;
use uuid::Uuid;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::api::http::AppState;
use crate::application::commands::{AddressInput, CreateShipmentCommand, HomeBooking, IntakeInput};
use crate::domain::value_objects::cancel_authority::is_tenant_wide;
use crate::domain::value_objects::home_move::{
    addendum_amount, validate_extras, SurveyExtra, check_schedule, lead_pay, local_today, move_slots, price, resolve, survey_pay, survey_slots, survey_window_open, volume_of,
    CatalogueItem, DayCapacity, DeclaredItem, HomePrice, PricedItem, Property, PropertyType, TruckPlan, SURVEY_LEAD_DAYS,
    surge_bps, surge_cents, window_free_bps, NO_SURGE_BPS,
};
use crate::domain::value_objects::quote_token::{self, QuoteTokenPayload};
use crate::infrastructure::db::home_catalogue::catalogue_for;
use crate::infrastructure::db::home_addenda::{self, NewAddendum};
use crate::infrastructure::db::home_waitlist;
use crate::infrastructure::db::home_moves::{self, HomeMoveRecord};

/// A home move takes longer to review than a parcel; the token lives longer.
const HOME_QUOTE_TTL_MINUTES: i64 = 30;

/// What a home-move quote token carries: the whole job, normalised, so the
/// booking can neither re-declare the inventory nor re-price it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeQuotePayload {
    /// Always `HOME_QUOTE_KIND`. The parcel token is signed with the same
    /// secret; this is what stops one being presented as the other.
    pub kind: String,
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub origin: AddressInput,
    pub destination: AddressInput,
    pub property: Property,
    pub items: Vec<PricedItem>,
    pub plan: TruckPlan,
    pub distance_centikm: i64,
    pub origin_text: String,
    pub destination_text: String,
    pub total_cents: i64,
    pub weight_kg: i64,
    pub currency: String,
    pub survey_required: bool,
    pub survey_cents: i64,
    pub trucks: i64,
    pub helpers: i64,
    pub crew_total: i64,
    pub large_estate: bool,
    pub international: bool,
    pub expires_at: DateTime<Utc>,
    #[serde(default)]
    pub distance_basis: Option<crate::infrastructure::external::DistanceBasis>,
    #[serde(default)]
    pub drive_minutes: Option<u32>,
}

pub const HOME_QUOTE_KIND: &str = "home_move";

fn not_offered() -> AppError {
    AppError::ServiceUnavailable("Whole-home moves aren't offered here yet".into())
}

#[derive(Debug, Deserialize)]
pub struct CatalogueQuery {
    pub property_type: PropertyType,
}

#[derive(Debug, Serialize)]
pub struct RoomView {
    pub key: &'static str,
    pub name: &'static str,
    pub items: Vec<CatalogueItem>,
}

/// `GET /v1/shipments/home/catalogue?property_type=apartment`
pub async fn catalogue(
    State(s): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<CatalogueQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rates = &s.svc.home_rates;
    if !rates.offered() {
        return Err(not_offered());
    }
    let all = catalogue_for(&s.pool, claims.tenant_id).await.map_err(AppError::Internal)?;
    let rooms: Vec<RoomView> = q
        .property_type
        .rooms()
        .iter()
        .map(|(key, name, group)| RoomView {
            key,
            name,
            items: all.iter().filter(|i| i.group == *group).cloned().collect(),
        })
        .collect();
    Ok(Json(serde_json::json!({ "data": {
        "property_type": q.property_type,
        "sizes": q.property_type.sizes(),
        "rooms": rooms,
        "truck_name": rates.truck_name,
    }})))
}

#[derive(Debug, Default, Deserialize)]
pub struct SlotsQuery {
    /// From the quote: a Large Estate needs a free multi-truck lead.
    #[serde(default)]
    pub large_estate: bool,
    /// From the quote: a move abroad needs an international lead.
    #[serde(default)]
    pub international: bool,
    /// Claiming a waitlist hold: that hold is not counted against the caller.
    #[serde(default)]
    pub waitlist_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlotView {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    /// A team is free for it. False greys it out.
    pub open: bool,
    /// The window's price multiplier, bps: 10000 is none. A move window
    /// whose teams are nearly all booked surges; a survey window never does.
    pub surge_bps: i64,
}

/// The calendar with each window's capacity: the teams driver-ops says the
/// date has, less the moves and surveys already booked and the windows held
/// for waitlisted customers (bar `claiming`, the hold being booked). Each
/// move window carries its surge. When capacity cannot be read, every window
/// is offered at no surge and `checked` says it was not counted.
pub(crate) async fn calendar(
    s: &AppState,
    tenant_id: Uuid,
    large_estate: bool,
    international: bool,
    claiming: Option<Uuid>,
) -> Result<(Vec<SlotView>, Vec<SlotView>, bool), AppError> {
    let rates = &s.svc.home_rates;
    let offset = rates.utc_offset_minutes;
    let now = Utc::now();
    let surveys = survey_slots(now, offset);
    let moves = move_slots(now, offset);
    let (Some(first), Some(last)) = (surveys.first().or(moves.first()), moves.last()) else {
        return Ok((Vec::new(), Vec::new(), false));
    };
    let from = local_today(first.starts_at, offset);
    let to = local_today(last.starts_at, offset);

    let capacity = match s.svc.home_teams.capacity(tenant_id, from, to).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(err = %e, "home-move capacity unreadable — offering every window unchecked");
            None
        }
    };
    let Some(capacity) = capacity else {
        let open = |sl: &crate::domain::value_objects::home_move::Slot| SlotView { starts_at: sl.starts_at, ends_at: sl.ends_at, open: true, surge_bps: NO_SURGE_BPS };
        return Ok((surveys.iter().map(open).collect(), moves.iter().map(open).collect(), false));
    };
    let by_date: std::collections::HashMap<chrono::NaiveDate, DayCapacity> =
        capacity.into_iter().map(|c| (c.date, c)).collect();
    let mut booked = home_moves::booked_between(&s.pool, tenant_id, first.starts_at, last.ends_at)
        .await
        .map_err(AppError::Internal)?;
    booked.extend(home_waitlist::holds(&s.pool, tenant_id, claiming).await.map_err(AppError::Internal)?);
    let none = DayCapacity::default();
    let cap_of = |at: DateTime<Utc>| by_date.get(&local_today(at, offset)).unwrap_or(&none);

    let survey_view = surveys
        .iter()
        .map(|sl| SlotView {
            starts_at: sl.starts_at,
            ends_at: sl.ends_at,
            open: survey_window_open(cap_of(sl.starts_at), &booked, sl.starts_at),
            surge_bps: NO_SURGE_BPS,
        })
        .collect();
    let move_view = moves
        .iter()
        .map(|sl| {
            let free = window_free_bps(cap_of(sl.starts_at), &booked, sl.starts_at, large_estate, international, offset);
            SlotView {
                starts_at: sl.starts_at,
                ends_at: sl.ends_at,
                open: free.is_some(),
                surge_bps: free.map_or(NO_SURGE_BPS, |f| surge_bps(rates, f)),
            }
        })
        .collect();
    Ok((survey_view, move_view, true))
}

/// `GET /v1/shipments/home/slots?large_estate=&international=` — the survey
/// windows and move starts, each open or full, the next open move, and the
/// rule that joins survey and move. The server re-checks at booking; this is
/// so the app never offers a full window or an illegal pair.
pub async fn slots(
    State(s): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<SlotsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rates = &s.svc.home_rates;
    if !rates.offered() {
        return Err(not_offered());
    }
    // The caller's own live hold is theirs to book, not a full window.
    let claiming = match q.waitlist_id {
        Some(id) => home_waitlist::get(&s.pool, claims.tenant_id, id)
            .await
            .map_err(AppError::Internal)?
            .filter(|e| e.account_id == claims.user_id && e.status == "offered" && e.hold_expires_at.is_some_and(|x| x > Utc::now()))
            .map(|e| e.id),
        None => None,
    };
    let (survey, moves, checked) = calendar(&s, claims.tenant_id, q.large_estate, q.international, claiming).await?;
    let next_open_move = moves.iter().find(|m| m.open).map(|m| m.starts_at);
    Ok(Json(serde_json::json!({ "data": {
        "survey": survey,
        "move": moves,
        "next_open_move": next_open_move,
        "capacity_checked": checked,
        "survey_lead_days": SURVEY_LEAD_DAYS,
        "utc_offset_minutes": rates.utc_offset_minutes,
    }})))
}

#[derive(Debug, Deserialize)]
pub struct HomeQuoteRequest {
    pub origin: AddressInput,
    pub destination: AddressInput,
    pub property: Property,
    pub items: Vec<DeclaredItem>,
    #[serde(default)]
    pub plan: TruckPlan,
}

#[derive(Debug, Serialize)]
pub struct HomeQuoteResponse {
    #[serde(flatten)]
    pub price: HomePrice,
    pub currency: String,
    pub distance_km: f64,
    /// `road`, or `direct` where no driving route exists (another island).
    pub distance_basis: crate::infrastructure::external::DistanceBasis,
    /// Driving time, where there is a road.
    pub drive_minutes: Option<u32>,
    pub origin_text: String,
    pub destination_text: String,
    pub plan: TruckPlan,
    /// Origin and destination in different countries: only an
    /// international-capable lead is offered it.
    pub international: bool,
    pub quote_token: String,
    pub expires_at: DateTime<Utc>,
}

fn one_line(a: &logisticos_types::Address) -> String {
    [a.line1.as_str(), a.city.as_str()].iter().filter(|s| !s.trim().is_empty()).copied().collect::<Vec<_>>().join(", ")
}

/// `POST /v1/shipments/home/quote`
pub async fn quote(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<HomeQuoteRequest>,
) -> Result<(StatusCode, Json<HomeQuoteResponse>), AppError> {
    let rates = &s.svc.home_rates;
    if !rates.offered() {
        return Err(not_offered());
    }
    let payment = s.svc.payment.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable("Online payment is not configured for this deployment — no quote can be issued".into())
    })?;
    req.property.validate().map_err(AppError::Validation)?;

    let catalogue = catalogue_for(&s.pool, claims.tenant_id).await.map_err(AppError::Internal)?;
    let items = resolve(&req.property, &req.items, &catalogue).map_err(AppError::Validation)?;

    // The distance is ours: both ends geocoded here, never a km the app sent.
    let from = s.svc.normalizer.normalize(&req.origin).await.map_err(AppError::Internal)?;
    let to = s.svc.normalizer.normalize(&req.destination).await.map_err(AppError::Internal)?;
    let (Some(a), Some(b)) = (from.coordinates, to.coordinates) else {
        return Err(AppError::BusinessRule(
            "Could not locate one of the addresses — a home move is priced door to door and needs both".into(),
        ));
    };
    let drive = s.svc.router.drive(a, b).await.map_err(|e| {
        tracing::error!(err = %e, "road distance unavailable for a home-move quote");
        AppError::ServiceUnavailable("Can't measure the drive between the two homes right now — try again in a minute".into())
    })?;
    let distance_centikm = (drive.distance_km * 100.0).round() as i64;

    let priced = price(rates, &req.property, &items, distance_centikm, req.plan);
    let international = !req.origin.country_code.trim().eq_ignore_ascii_case(req.destination.country_code.trim());
    let currency = claims.currency.clone().unwrap_or_else(|| "PHP".into());
    let expires_at = Utc::now() + Duration::minutes(HOME_QUOTE_TTL_MINUTES);
    let payload = HomeQuotePayload {
        kind: HOME_QUOTE_KIND.into(),
        tenant_id: claims.tenant_id,
        account_id: claims.user_id,
        origin: req.origin.clone(),
        destination: req.destination.clone(),
        property: req.property.clone(),
        items,
        plan: req.plan,
        distance_centikm,
        origin_text: one_line(&from),
        destination_text: one_line(&to),
        total_cents: priced.total_cents,
        weight_kg: priced.weight_kg,
        currency: currency.clone(),
        survey_required: priced.survey_required,
        survey_cents: priced.survey_cents,
        trucks: priced.trucks,
        helpers: priced.helpers,
        crew_total: priced.crew_total,
        large_estate: priced.large_estate,
        international,
        expires_at,
        distance_basis: Some(drive.basis),
        drive_minutes: drive.minutes,
    };
    let quote_token = quote_token::sign_payload(payment.quote_token_secret.as_bytes(), &payload);

    Ok((
        StatusCode::OK,
        Json(HomeQuoteResponse {
            price: priced,
            currency,
            distance_km: distance_centikm as f64 / 100.0,
            distance_basis: drive.basis,
            drive_minutes: drive.minutes,
            origin_text: payload.origin_text,
            destination_text: payload.destination_text,
            plan: req.plan,
            international,
            quote_token,
            expires_at,
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct HomeBookRequest {
    pub quote_token: String,
    /// Required exactly when the quote says a survey is.
    #[serde(default)]
    pub survey_at: Option<DateTime<Utc>>,
    pub move_at: DateTime<Utc>,
    #[serde(default)]
    pub contact_name: Option<String>,
    #[serde(default)]
    pub contact_phone: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub intake: Option<IntakeInput>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// The surge the customer was shown for the window (bps). Booking is
    /// refused (`SURGE_CHANGED`) if the window has surged past it since.
    #[serde(default)]
    pub accepted_surge_bps: Option<i64>,
    /// Booking a window held for this customer off the waitlist.
    #[serde(default)]
    pub waitlist_id: Option<Uuid>,
}

fn tenant_code(slug: &str) -> String {
    slug.chars().filter(|c| c.is_ascii_alphanumeric() && *c != 'O' && *c != 'I').take(3).collect::<String>().to_uppercase()
}

/// What the booking path does with a verified home quote, apart from the
/// storage: the checks it must pass and the parcel-shaped token the shared
/// create path is handed. Pure, so it is tested without a database.
pub fn admit(
    payload: &HomeQuotePayload,
    tenant_id: Uuid,
    account_id: Uuid,
    survey_at: Option<DateTime<Utc>>,
    move_at: DateTime<Utc>,
    now: DateTime<Utc>,
    utc_offset_minutes: i32,
) -> Result<QuoteTokenPayload, AppError> {
    if payload.kind != HOME_QUOTE_KIND {
        return Err(AppError::Validation("That is not a home-move quote".into()));
    }
    if payload.expires_at < now {
        return Err(AppError::Validation("This quote has expired — price the move again".into()));
    }
    if payload.tenant_id != tenant_id {
        return Err(AppError::Validation("Quote token does not belong to this tenant".into()));
    }
    if payload.account_id != account_id {
        return Err(AppError::Validation("This quote was priced for a different account — price the move again".into()));
    }
    check_schedule(survey_at, move_at, payload.survey_required, now, utc_offset_minutes).map_err(AppError::Validation)?;

    let weight_grams = u32::try_from(payload.weight_kg.max(1).saturating_mul(1_000)).unwrap_or(u32::MAX);
    // Minted here, signed with the same secret, and good for minutes: the
    // shared create path verifies it exactly as it verifies any quote, so
    // payment, idempotency and the AWB work the one way they always do.
    Ok(QuoteTokenPayload {
        tenant_id,
        service_type: "home_move".into(),
        weight_grams,
        amount_cents: payload.total_cents,
        currency: payload.currency.clone(),
        expires_at: now + Duration::minutes(10),
        pricing_mode: Some("home_move".into()),
        billable_grams: Some(weight_grams),
        accessorial_paid_cents: Some(0),
        discount_cents: None,
        promo_code: None,
        account_id: None,
        discounts: None,
    })
}

/// `POST /v1/shipments/home` — book a priced home move for the chosen survey
/// and move times.
pub async fn book(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<HomeBookRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    claims.require_permission(permissions::SHIPMENT_CREATE)?;
    let rates = &s.svc.home_rates;
    if !rates.offered() {
        return Err(not_offered());
    }
    let payment = s.svc.payment.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable("Online payment is not configured for this deployment".into())
    })?;
    let secret = payment.quote_token_secret.as_bytes();
    let payload: HomeQuotePayload = quote_token::verify_payload(secret, &req.quote_token)
        .map_err(|e| AppError::Validation(format!("Invalid quote: {e}")))?;

    let now = Utc::now();
    let mut parcel = admit(&payload, claims.tenant_id, claims.user_id, req.survey_at, req.move_at, now, rates.utc_offset_minutes)?;

    // A window held off the waitlist is this customer's for its hold, and
    // only that window.
    let claiming = match req.waitlist_id {
        None => None,
        Some(id) => {
            let held = home_waitlist::get(&s.pool, claims.tenant_id, id).await.map_err(AppError::Internal)?;
            match held {
                Some(e) if e.account_id == claims.user_id
                    && e.status == "offered"
                    && e.offered_move_at == Some(req.move_at)
                    && e.hold_expires_at.is_some_and(|x| x > now) => Some(id),
                _ => {
                    return Err(AppError::Conflict(
                        "HOLD_LAPSED: that window is no longer held for you — pick an open one or rejoin the waitlist".into(),
                    ))
                }
            }
        }
    };

    // The windows may have filled since the calendar was drawn.
    let (surveys, moves, _) = calendar(&s, claims.tenant_id, payload.large_estate, payload.international, claiming).await?;
    let window = moves.iter().find(|m| m.starts_at == req.move_at && m.open);
    if window.is_none() {
        let next = moves.iter().find(|m| m.open).map(|m| m.starts_at.to_rfc3339()).unwrap_or_default();
        return Err(AppError::Conflict(format!(
            "SLOT_FULL: every verified moving team is booked for that move window{}",
            if next.is_empty() { String::new() } else { format!(" — the next open one is {next}") }
        )));
    }
    if let Some(survey_at) = req.survey_at {
        if !surveys.iter().any(|s| s.starts_at == survey_at && s.open) {
            return Err(AppError::Conflict("SLOT_FULL: every lead is surveying in that window — pick another".into()));
        }
    }
    // Surge on the fare (never the survey fee), at most what the customer saw.
    let surge = window.map_or(NO_SURGE_BPS, |w| w.surge_bps);
    if surge > req.accepted_surge_bps.unwrap_or(NO_SURGE_BPS) {
        return Err(AppError::Conflict(format!(
            "SURGE_CHANGED:{surge}: that window is now in high demand — look at the new price before booking"
        )));
    }
    let surge_added = surge_cents(payload.total_cents - payload.survey_cents, surge);
    parcel.amount_cents += surge_added;

    let phone = req
        .contact_phone
        .as_deref()
        .or(claims.phone.as_deref())
        .map(str::trim)
        .filter(|p| p.len() >= 7)
        .ok_or_else(|| AppError::Validation("A contact phone is needed for the crew".into()))?
        .to_owned();
    let email = Some(claims.email.trim().to_owned()).filter(|e| e.contains('@'));
    let name = req.contact_name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Customer").to_owned();
    let is_customer = claims.roles.iter().any(|r| r == "customer");
    let item_count: u32 = payload.items.iter().map(|i| i.qty).sum();
    // The lead's pay, less commission. The survey deposit is its own pay; a
    // surge is passed to the lead whole, as a High-Demand Bonus.
    let pay = lead_pay(rates, volume_of(&payload.items), payload.distance_centikm, payload.total_cents - payload.survey_cents);
    let lead_payout = pay.net_cents + surge_added;

    let cmd = CreateShipmentCommand {
        tenant_id: claims.tenant_id,
        merchant_id: claims.user_id,
        tenant_code: tenant_code(&claims.tenant_slug),
        customer_name: name,
        customer_phone: phone,
        customer_email: email,
        sender_name: None,
        sender_phone: None,
        sender_email: None,
        origin: payload.origin.clone(),
        destination: payload.destination.clone(),
        service_type: "home_move".into(),
        weight_grams: parcel.weight_grams,
        length_cm: None,
        width_cm: None,
        height_cm: None,
        declared_value_cents: None,
        cod_amount_cents: None,
        special_instructions: req.notes.clone().filter(|n| !n.trim().is_empty()),
        merchant_reference: None,
        description: Some(format!("Whole-home move: {} {}, {item_count} items", payload.property.size, match payload.property.kind {
            PropertyType::Apartment => "apartment",
            PropertyType::Villa => "villa",
            PropertyType::Offices => "offices",
        })),
        source_platform: None,
        external_order_id: None,
        piece_count: None,
        pieces: None,
        booked_by_customer: is_customer,
        // A crew is assigned by ops for the booked day, not grabbed now for a
        // move that may be three weeks out.
        auto_dispatch: Some(false),
        merchant_name: None,
        delivery_category: Some("large".into()),
        quote_token: Some(quote_token::sign(secret, &parcel)),
        home_booking: Some(HomeBooking {
            move_at: req.move_at,
            requirement: logisticos_events::payloads::HomeMoveRequirement {
                trucks: payload.trucks,
                helpers: payload.helpers,
                crew_total: payload.crew_total,
                large_estate: payload.large_estate,
                international: payload.international,
                move_at: req.move_at,
                move_date: Some((req.move_at + Duration::minutes(i64::from(rates.utc_offset_minutes))).date_naive()),
                survey_at: req.survey_at,
                lead_payout_cents: Some(lead_payout).filter(|p| *p > 0),
            },
        }),
        intake: req.intake,
        idempotency_key: req.idempotency_key,
    };
    let result = s.svc.create(cmd).await?;

    // After the shipment, so the row always has one; a retry of the same
    // booking reaches the same shipment and writes nothing new.
    let record = HomeMoveRecord {
        shipment_id: result.shipment.id.inner(),
        tenant_id: claims.tenant_id,
        account_id: claims.user_id,
        property: serde_json::to_value(&payload.property).map_err(|e| AppError::Internal(e.into()))?,
        items: serde_json::to_value(&payload.items).map_err(|e| AppError::Internal(e.into()))?,
        plan: match payload.plan { TruckPlan::Trucks => "trucks", TruckPlan::Trips => "trips" }.into(),
        distance_centikm: payload.distance_centikm,
        survey_required: payload.survey_required,
        survey_at: req.survey_at,
        move_at: req.move_at,
        total_cents: payload.total_cents + surge_added,
        currency: payload.currency.clone(),
        trucks: i32::try_from(payload.trucks).unwrap_or(i32::MAX),
        helpers: i32::try_from(payload.helpers).unwrap_or(i32::MAX),
        crew_total: i32::try_from(payload.crew_total).unwrap_or(i32::MAX),
        large_estate: payload.large_estate,
        international: payload.international,
        survey_cents: payload.survey_cents,
        survey_submitted_at: None,
        lead_gross_cents: pay.gross_cents,
        lead_commission_cents: pay.commission_cents,
        lead_payout_cents: lead_payout,
        surge_bps: i32::try_from(surge).unwrap_or(i32::MAX),
        surge_cents: surge_added,
        lead_bonus_cents: surge_added,
    };
    home_moves::insert(&s.pool, &record).await.map_err(AppError::Internal)?;
    if let Some(id) = claiming {
        home_waitlist::mark_booked(&s.pool, id).await.map_err(AppError::Internal)?;
    }
    tracing::info!(
        tenant_id = %claims.tenant_id, shipment_id = %record.shipment_id, total_cents = record.total_cents,
        survey = record.survey_required, "home move booked"
    );

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "shipment": result.shipment,
            "checkout_url": result.checkout_url,
            "home": record,
        })),
    ))
}

/// Who may act on a booked home move.
enum Party {
    Customer,
    Lead,
    Staff,
}

/// The customer who booked it, the lead who reserved it, or staff who reach
/// every shipment in the tenant. Anyone else reads the move as missing.
async fn party_of(s: &AppState, claims: &AuthClaims, record: &HomeMoveRecord) -> Option<Party> {
    if record.account_id == claims.user_id {
        return Some(Party::Customer);
    }
    if is_tenant_wide(
        claims.has_permission(permissions::SHIPMENT_CREATE),
        claims.has_permission(permissions::SHIPMENT_UPDATE),
    ) {
        return Some(Party::Staff);
    }
    match s.svc.home_teams.reservation(record.shipment_id).await {
        Ok(Some(r)) if r.driver_id == claims.user_id => Some(Party::Lead),
        _ => None,
    }
}

async fn record_for(s: &AppState, claims: &AuthClaims, id: Uuid) -> Result<(HomeMoveRecord, Party), AppError> {
    let not_found = || AppError::NotFound { resource: "Home move", id: id.to_string() };
    let record = home_moves::get(&s.pool, claims.tenant_id, id).await.map_err(AppError::Internal)?.ok_or_else(not_found)?;
    let party = party_of(s, claims, &record).await.ok_or_else(not_found)?;
    Ok((record, party))
}

#[derive(Debug, Deserialize)]
pub struct SurveyRequest {
    /// Catalogue items found beyond the booking. Additions only.
    #[serde(default)]
    pub items: Vec<DeclaredItem>,
    #[serde(default)]
    pub extras: Vec<SurveyExtra>,
    #[serde(default)]
    pub note: String,
}

/// `POST /v1/shipments/:id/home/survey` — the lead's survey. It marks the
/// survey done (the deposit is kept from now on), and when it found more
/// than was booked, opens an addendum for the customer to approve. It can
/// only add: the agreed price never goes down.
pub async fn submit_survey(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(req): Json<SurveyRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let (record, party) = record_for(&s, &claims, id).await?;
    if matches!(party, Party::Customer) {
        return Err(AppError::Forbidden { resource: "survey".into() });
    }
    validate_extras(&req.extras).map_err(AppError::Validation)?;
    let property: Property = serde_json::from_value(record.property.clone()).map_err(|e| AppError::Internal(e.into()))?;
    let booked: Vec<PricedItem> = serde_json::from_value(record.items.clone()).map_err(|e| AppError::Internal(e.into()))?;
    let plan = if record.plan == "trips" { TruckPlan::Trips } else { TruckPlan::Trucks };
    let added = if req.items.is_empty() {
        Vec::new()
    } else {
        let catalogue = catalogue_for(&s.pool, claims.tenant_id).await.map_err(AppError::Internal)?;
        resolve(&property, &req.items, &catalogue).map_err(AppError::Validation)?
    };
    let (repriced, items_cents, extras_cents) = addendum_amount(
        &s.svc.home_rates, &property, &booked, &added, &req.extras, record.distance_centikm, plan, record.total_cents,
    );
    home_addenda::mark_survey_submitted(&s.pool, claims.tenant_id, id).await.map_err(AppError::Internal)?;
    pay_survey(&s, &record).await;
    if items_cents + extras_cents == 0 {
        tracing::info!(shipment_id = %id, by = %claims.user_id, "home survey done — nothing added");
        return Ok((StatusCode::OK, Json(serde_json::json!({ "data": { "survey_submitted": true, "addendum": null } }))));
    }
    let addendum = home_addenda::insert(&s.pool, &NewAddendum {
        shipment_id: id,
        tenant_id: claims.tenant_id,
        submitted_by: claims.user_id,
        items: serde_json::to_value(&added).map_err(|e| AppError::Internal(e.into()))?,
        extras: serde_json::to_value(&req.extras).map_err(|e| AppError::Internal(e.into()))?,
        note: req.note.chars().take(500).collect(),
        items_cents,
        extras_cents,
        currency: record.currency.clone(),
        trucks: i32::try_from(repriced.trucks).unwrap_or(i32::MAX),
        helpers: i32::try_from(repriced.helpers).unwrap_or(i32::MAX),
        crew_total: i32::try_from(repriced.crew_total).unwrap_or(i32::MAX),
        large_estate: repriced.large_estate,
    })
    .await
    .map_err(AppError::Internal)?
    .ok_or_else(|| AppError::Conflict("An addendum is already waiting on the customer for this move".into()))?;
    tracing::info!(shipment_id = %id, addendum_id = %addendum.id, total = addendum.total_cents, "home survey addendum opened");
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": { "survey_submitted": true, "addendum": addendum } }))))
}

/// `GET /v1/shipments/:id/home/addendum` — the latest addendum, if any.
pub async fn get_addendum(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    record_for(&s, &claims, id).await?;
    let latest = home_addenda::latest(&s.pool, claims.tenant_id, id).await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": latest })))
}

async fn customer_addendum(
    s: &AppState,
    claims: &AuthClaims,
    id: Uuid,
    addendum_id: Uuid,
) -> Result<(HomeMoveRecord, home_addenda::AddendumRecord), AppError> {
    let (record, party) = record_for(s, claims, id).await?;
    // Only the customer answers for the customer's money.
    if !matches!(party, Party::Customer) {
        return Err(AppError::Forbidden { resource: "addendum".into() });
    }
    let addendum = home_addenda::get(&s.pool, claims.tenant_id, addendum_id)
        .await
        .map_err(AppError::Internal)?
        .filter(|a| a.shipment_id == id)
        .ok_or_else(|| AppError::NotFound { resource: "Addendum", id: addendum_id.to_string() })?;
    Ok((record, addendum))
}

/// `POST /v1/shipments/:id/home/addendum/:addendum_id/approve` — the
/// customer agrees; the difference is charged as its own payment.
pub async fn approve_addendum(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path((id, addendum_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (_, addendum) = customer_addendum(&s, &claims, id, addendum_id).await?;
    if addendum.status != "pending" {
        return Err(AppError::Conflict(format!("This addendum is already {}", addendum.status)));
    }
    let payment = s.svc.payment.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable("Online payment is not configured for this deployment".into())
    })?;
    let return_url = format!(
        "{}/payment/return?shipment_id={id}&addendum_id={addendum_id}",
        payment.shipment_return_url_base.trim_end_matches('/'),
    );
    let intent = payment
        .client
        .create_home_addendum_intent(claims.tenant_id, addendum_id, addendum.total_cents, &addendum.currency, &return_url)
        .await
        .map_err(AppError::Internal)?;
    if !home_addenda::approve(&s.pool, addendum_id, intent.intent_id, &intent.checkout_url).await.map_err(AppError::Internal)? {
        return Err(AppError::Conflict("This addendum was answered meanwhile".into()));
    }
    Ok(Json(serde_json::json!({ "data": { "status": "approved", "checkout_url": intent.checkout_url } })))
}

/// `POST /v1/shipments/:id/home/addendum/:addendum_id/decline` — the
/// customer declines; the move stands as booked.
pub async fn decline_addendum(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path((id, addendum_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    customer_addendum(&s, &claims, id, addendum_id).await?;
    if !home_addenda::decline(&s.pool, addendum_id).await.map_err(AppError::Internal)? {
        return Err(AppError::Conflict("This addendum was already answered".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/shipments/:id/home` — the booked move's detail. The customer who
/// booked it, the lead who reserved it (for the survey), or staff who reach
/// every shipment in the tenant; anyone else reads it as missing.
pub async fn detail(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (record, party) = record_for(&s, &claims, id).await?;
    // The lead who reserved it, once one has — "Team {lead}". A failure to
    // ask leaves it unnamed rather than failing the page.
    let lead = match s.svc.home_teams.reservation(id).await {
        Ok(r) => r.map(|r| r.lead_name).filter(|n| !n.is_empty()),
        Err(e) => {
            tracing::warn!(shipment_id = %id, err = %e, "reserved lead unreadable");
            None
        }
    };
    // What the lead is paid is theirs and staff's to see, not the customer's.
    let pay = (!matches!(party, Party::Customer)).then(|| {
        let survey = survey_pay(&s.svc.home_rates, record.survey_cents);
        serde_json::json!({
            "lead_gross_cents": record.lead_gross_cents,
            "commission_cents": record.lead_commission_cents,
            "lead_payout_cents": record.lead_payout_cents,
            "survey_payout_cents": survey.net_cents,
            "survey_commission_cents": survey.commission_cents,
        })
    });
    Ok(Json(serde_json::json!({ "data": record, "lead_name": lead, "pay": pay })))
}

/// The survey is signed off: pay the reserved lead the survey fee, less
/// commission, whether or not the move goes ahead. Safe to repeat — a later
/// survey retries a credit that failed, and driver-ops credits it once.
async fn pay_survey(s: &AppState, record: &HomeMoveRecord) {
    if record.survey_cents <= 0 {
        return;
    }
    let lead = match s.svc.home_teams.reservation(record.shipment_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            tracing::warn!(shipment_id = %record.shipment_id, "survey signed off with no reserved lead — survey fee not credited");
            return;
        }
        Err(e) => {
            tracing::error!(shipment_id = %record.shipment_id, err = %e, "survey fee not credited: reservation unreadable — the next survey retries");
            return;
        }
    };
    let pay = survey_pay(&s.svc.home_rates, record.survey_cents);
    let tracking = s
        .svc
        .repo
        .find_by_id(&logisticos_types::ShipmentId::from_uuid(record.shipment_id))
        .await
        .ok()
        .flatten()
        .map(|sh| sh.awb.to_string());
    let credit = crate::infrastructure::http::home_capacity_client::LeadCredit {
        tenant_id: record.tenant_id,
        driver_id: lead.driver_id,
        kind: "survey_fee",
        reference_id: record.shipment_id,
        tracking_number: tracking,
        gross_cents: pay.gross_cents,
        commission_cents: pay.commission_cents,
        amount_cents: pay.net_cents,
    };
    match s.svc.home_teams.credit(&credit).await {
        Ok(_) => tracing::info!(shipment_id = %record.shipment_id, lead = %lead.driver_id, amount_cents = pay.net_cents, "survey fee credited to the lead"),
        Err(e) => tracing::error!(shipment_id = %record.shipment_id, err = %e, "survey fee not credited — the next survey retries"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::value_objects::home_move::{move_slots, survey_slots};

    fn address() -> AddressInput {
        AddressInput {
            line1: "12 Acacia St".into(), line2: None, barangay: None, city: "Makati".into(),
            province: "Metro Manila".into(), postal_code: "1200".into(), country_code: "PH".into(),
        }
    }

    fn payload(tenant: Uuid, account: Uuid, now: DateTime<Utc>, survey_required: bool) -> HomeQuotePayload {
        HomeQuotePayload {
            kind: HOME_QUOTE_KIND.into(),
            tenant_id: tenant,
            account_id: account,
            origin: address(),
            destination: address(),
            property: Property {
                kind: PropertyType::Apartment, size: if survey_required { "3 bedroom" } else { "1 bedroom" }.into(),
                pickup_floor: 0, pickup_has_lift: true, dropoff_floor: 0, dropoff_has_lift: true, long_carry: false,
            },
            items: vec![],
            plan: TruckPlan::Trucks,
            distance_centikm: 1_840,
            origin_text: "12 Acacia St, Makati".into(),
            destination_text: "12 Acacia St, Makati".into(),
            total_cents: 150_000,
            weight_kg: 1_200,
            currency: "PHP".into(),
            survey_required,
            survey_cents: if survey_required { 4_500 } else { 0 },
            trucks: 1,
            helpers: 2,
            crew_total: 3,
            large_estate: false,
            international: false,
            expires_at: now + Duration::minutes(30),
            distance_basis: None,
            drive_minutes: None,
        }
    }

    #[test]
    fn a_verified_home_quote_becomes_a_parcel_token_for_the_whole_household() {
        let (t, a, now) = (Uuid::new_v4(), Uuid::new_v4(), Utc::now());
        let mv = move_slots(now, 480)[0].starts_at;
        let parcel = admit(&payload(t, a, now, false), t, a, None, mv, now, 480).unwrap();
        assert_eq!(parcel.service_type, "home_move");
        assert_eq!(parcel.amount_cents, 150_000);
        assert_eq!(parcel.weight_grams, 1_200_000, "tonnes, not the 70 kg parcel cap");
        assert!(parcel.discounts.is_none(), "no discount line rides on a home move yet");
    }

    #[test]
    fn someone_elses_quote_or_an_expired_one_is_refused() {
        let (t, a, now) = (Uuid::new_v4(), Uuid::new_v4(), Utc::now());
        let mv = move_slots(now, 480)[0].starts_at;
        assert!(admit(&payload(t, a, now, false), t, Uuid::new_v4(), None, mv, now, 480).is_err());
        assert!(admit(&payload(t, a, now, false), Uuid::new_v4(), a, None, mv, now, 480).is_err());
        let mut stale = payload(t, a, now, false);
        stale.expires_at = now - Duration::seconds(1);
        assert!(admit(&stale, t, a, None, mv, now, 480).is_err());
        let mut parcel_kind = payload(t, a, now, false);
        parcel_kind.kind = "parcel".into();
        assert!(admit(&parcel_kind, t, a, None, mv, now, 480).is_err());
    }

    #[test]
    fn a_surveyed_move_needs_its_survey_two_days_ahead() {
        let (t, a, now) = (Uuid::new_v4(), Uuid::new_v4(), Utc::now());
        let surveys = survey_slots(now, 480);
        let moves = move_slots(now, 480);
        let last_move = moves.last().unwrap().starts_at;
        assert!(admit(&payload(t, a, now, true), t, a, None, last_move, now, 480).is_err());
        assert!(admit(&payload(t, a, now, true), t, a, Some(surveys[0].starts_at), last_move, now, 480).is_ok());
    }

    #[test]
    fn a_home_token_does_not_verify_as_a_parcel_token() {
        let (t, a, now) = (Uuid::new_v4(), Uuid::new_v4(), Utc::now());
        let token = quote_token::sign_payload(b"secret", &payload(t, a, now, false));
        assert!(quote_token::verify(b"secret", &token).is_err());
    }
}

