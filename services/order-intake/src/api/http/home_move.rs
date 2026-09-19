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
    check_schedule, move_slots, price, resolve, survey_slots, CatalogueItem, DeclaredItem, HomePrice, PricedItem,
    Property, PropertyType, Slot, TruckPlan, SURVEY_LEAD_DAYS,
};
use crate::domain::value_objects::quote_token::{self, QuoteTokenPayload};
use crate::infrastructure::db::home_catalogue::catalogue_for;
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

/// `GET /v1/shipments/home/slots` — the survey windows and move starts on
/// offer, and the rule that joins them. The server re-checks the pair at
/// booking; this is so the app never shows an illegal one.
pub async fn slots(State(s): State<AppState>, _claims: AuthClaims) -> Result<Json<serde_json::Value>, AppError> {
    let rates = &s.svc.home_rates;
    if !rates.offered() {
        return Err(not_offered());
    }
    let now = Utc::now();
    let survey: Vec<Slot> = survey_slots(now, rates.utc_offset_minutes);
    let moves: Vec<Slot> = move_slots(now, rates.utc_offset_minutes);
    Ok(Json(serde_json::json!({ "data": {
        "survey": survey,
        "move": moves,
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
    let distance_km = a.distance_km(&b);
    let distance_centikm = (distance_km * 100.0).round() as i64;

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
    };
    let quote_token = quote_token::sign_payload(payment.quote_token_secret.as_bytes(), &payload);

    Ok((
        StatusCode::OK,
        Json(HomeQuoteResponse {
            price: priced,
            currency,
            distance_km: distance_centikm as f64 / 100.0,
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
    let parcel = admit(&payload, claims.tenant_id, claims.user_id, req.survey_at, req.move_at, now, rates.utc_offset_minutes)?;

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
                survey_at: req.survey_at,
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
        total_cents: payload.total_cents,
        currency: payload.currency.clone(),
        trucks: i32::try_from(payload.trucks).unwrap_or(i32::MAX),
        helpers: i32::try_from(payload.helpers).unwrap_or(i32::MAX),
        crew_total: i32::try_from(payload.crew_total).unwrap_or(i32::MAX),
        large_estate: payload.large_estate,
        international: payload.international,
        survey_cents: payload.survey_cents,
        survey_submitted_at: None,
    };
    home_moves::insert(&s.pool, &record).await.map_err(AppError::Internal)?;
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

/// `GET /v1/shipments/:id/home` — the booked move's detail. The customer who
/// booked it, or staff who reach every shipment in the tenant; anyone else
/// reads it as missing.
pub async fn detail(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let not_found = || AppError::NotFound { resource: "Home move", id: id.to_string() };
    let record = home_moves::get(&s.pool, claims.tenant_id, id).await.map_err(AppError::Internal)?.ok_or_else(not_found)?;
    let tenant_wide = is_tenant_wide(
        claims.has_permission(permissions::SHIPMENT_CREATE),
        claims.has_permission(permissions::SHIPMENT_UPDATE),
    );
    if record.account_id != claims.user_id && !tenant_wide {
        return Err(not_found());
    }
    Ok(Json(serde_json::json!({ "data": record })))
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

