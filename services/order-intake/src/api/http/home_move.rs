//! Whole-home moving, the customer's side: the catalogue, the calendar and a
//! signed quote. Booking one is `POST /v1/shipments/home`.
//!
//! Every number here is the server's. The app sends the property, the rooms
//! and which catalogue items are in them, and two addresses; the volumes,
//! weights, distance and prices come from here, and the quote token carries
//! the whole normalised job so the booking cannot re-declare any of it.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::api::http::AppState;
use crate::application::commands::AddressInput;
use crate::domain::value_objects::home_move::{
    move_slots, price, resolve, survey_slots, CatalogueItem, DeclaredItem, HomePrice, HomeQuotePayload, Property,
    PropertyType, Slot, TruckPlan, HOME_QUOTE_KIND, SURVEY_LEAD_DAYS,
};
use crate::domain::value_objects::quote_token;
use crate::infrastructure::db::home_catalogue::catalogue_for;

/// A home move takes longer to review than a parcel; the token lives longer.
const HOME_QUOTE_TTL_MINUTES: i64 = 30;

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
    let currency = claims.currency.clone().unwrap_or_else(|| "PHP".into());
    let expires_at = Utc::now() + Duration::minutes(HOME_QUOTE_TTL_MINUTES);
    let payload = HomeQuotePayload {
        kind: HOME_QUOTE_KIND.into(),
        tenant_id: claims.tenant_id,
        account_id: claims.user_id,
        property: req.property.clone(),
        items,
        plan: req.plan,
        distance_centikm,
        origin_text: one_line(&from),
        destination_text: one_line(&to),
        total_cents: priced.total_cents,
        currency: currency.clone(),
        survey_required: priced.survey_required,
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
            quote_token,
            expires_at,
        }),
    ))
}
