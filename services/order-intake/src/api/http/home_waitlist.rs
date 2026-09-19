//! The priority waitlist for fully booked home-move dates.
//!
//! A customer whose date has no open window queues for it with their priced
//! move. Every minute the sweep lapses holds not claimed in time, then walks
//! each date's queue first come, first served: the first customer a window
//! can now serve is offered it, held for them for five minutes (the hold
//! counts against capacity for everyone else), and told by push. Claiming
//! re-prices the same move for that window; booking it spends the hold.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_events::{envelope::Event, payloads::HomeNotice, topics};
use serde::Deserialize;
use uuid::Uuid;

use super::home_move::{calendar, HomeQuotePayload, HomeQuoteResponse, SlotView, HOME_QUOTE_KIND};
use super::AppState;
use crate::domain::value_objects::home_move::{check_schedule, local_today, move_slots, price};
use crate::domain::value_objects::quote_token;
use crate::infrastructure::db::home_waitlist::{self, WaitlistEntry, HOLD_MINUTES};
use crate::infrastructure::external::DistanceBasis;

#[derive(Debug, Deserialize)]
pub struct JoinRequest {
    pub quote_token: String,
    /// The local date wanted.
    pub date: NaiveDate,
}

/// `POST /v1/shipments/home/waitlist` — queue for a fully booked date.
pub async fn join(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<JoinRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let rates = &s.svc.home_rates;
    let payment = s.svc.payment.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable("Online payment is not configured for this deployment".into())
    })?;
    let payload: HomeQuotePayload = quote_token::verify_payload(payment.quote_token_secret.as_bytes(), &req.quote_token)
        .map_err(|e| AppError::Validation(format!("Invalid quote: {e}")))?;
    let now = Utc::now();
    if payload.kind != HOME_QUOTE_KIND || payload.tenant_id != claims.tenant_id || payload.account_id != claims.user_id {
        return Err(AppError::Validation("That quote is not yours to queue with".into()));
    }
    if payload.expires_at < now {
        return Err(AppError::Validation("This quote has expired — price the move again".into()));
    }
    let offset = rates.utc_offset_minutes;
    if !move_slots(now, offset).iter().any(|m| local_today(m.starts_at, offset) == req.date) {
        return Err(AppError::Validation("That date is not one moves are booked for".into()));
    }
    let (_, moves, _) = calendar(&s, claims.tenant_id, payload.large_estate, payload.international, None).await?;
    if moves.iter().any(|m| m.open && local_today(m.starts_at, offset) == req.date) {
        return Err(AppError::Conflict("OPEN_WINDOW: that date has an open window — book it instead".into()));
    }
    let quote = serde_json::to_value(&payload).map_err(|e| AppError::Internal(e.into()))?;
    let entry = home_waitlist::join(&s.pool, claims.tenant_id, claims.user_id, &quote, req.date, payload.large_estate, payload.international)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::Conflict("You're already on the waitlist for that date".into()))?;
    tracing::info!(waitlist_id = %entry.id, date = %req.date, "home move waitlisted");
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": entry }))))
}

/// `GET /v1/shipments/home/waitlist` — this customer's places and holds.
pub async fn mine(State(s): State<AppState>, claims: AuthClaims) -> Result<Json<serde_json::Value>, AppError> {
    let entries = home_waitlist::mine(&s.pool, claims.tenant_id, claims.user_id).await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": entries })))
}

/// `DELETE /v1/shipments/home/waitlist/:id`
pub async fn withdraw(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    if home_waitlist::withdraw(&s.pool, claims.tenant_id, claims.user_id, id).await.map_err(AppError::Internal)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound { resource: "Waitlist place", id: id.to_string() })
    }
}

/// `POST /v1/shipments/home/waitlist/:id/claim` — a held window: the same
/// move priced again (today's rates) and signed for the hold, and the window.
/// Book it with `waitlist_id` to spend the hold.
pub async fn claim(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let now = Utc::now();
    let entry = home_waitlist::get(&s.pool, claims.tenant_id, id)
        .await
        .map_err(AppError::Internal)?
        .filter(|e| e.account_id == claims.user_id)
        .ok_or_else(|| AppError::NotFound { resource: "Waitlist place", id: id.to_string() })?;
    let (Some(move_at), Some(expires)) = (entry.offered_move_at, entry.hold_expires_at) else {
        return Err(AppError::Conflict("NOT_OFFERED: no window is held for you yet — we'll tell you when one opens".into()));
    };
    if entry.status != "offered" || expires <= now {
        return Err(AppError::Conflict("HOLD_LAPSED: that hold has ended — the window went to the next in line".into()));
    }
    let quote = requote(&s, &entry, expires)?;
    Ok(Json(serde_json::json!({ "data": {
        "waitlist_id": entry.id,
        "move_at": move_at,
        "hold_expires_at": expires,
        "quote": quote,
    }})))
}

/// The stored move priced again at today's rates, signed until `until`.
fn requote(s: &AppState, entry: &WaitlistEntry, until: DateTime<Utc>) -> Result<HomeQuoteResponse, AppError> {
    let rates = &s.svc.home_rates;
    let payment = s.svc.payment.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable("Online payment is not configured for this deployment".into())
    })?;
    let mut payload: HomeQuotePayload = serde_json::from_value(entry.quote.clone()).map_err(|e| AppError::Internal(e.into()))?;
    let priced = price(rates, &payload.property, &payload.items, payload.distance_centikm, payload.plan);
    payload.total_cents = priced.total_cents;
    payload.weight_kg = priced.weight_kg;
    payload.survey_required = priced.survey_required;
    payload.survey_cents = priced.survey_cents;
    payload.trucks = priced.trucks;
    payload.helpers = priced.helpers;
    payload.crew_total = priced.crew_total;
    payload.large_estate = priced.large_estate;
    payload.expires_at = until;
    let quote_token = quote_token::sign_payload(payment.quote_token_secret.as_bytes(), &payload);
    Ok(HomeQuoteResponse {
        price: priced,
        currency: payload.currency.clone(),
        distance_km: payload.distance_centikm as f64 / 100.0,
        distance_basis: payload.distance_basis.unwrap_or(DistanceBasis::Road),
        drive_minutes: payload.drive_minutes,
        origin_text: payload.origin_text.clone(),
        destination_text: payload.destination_text.clone(),
        plan: payload.plan,
        international: payload.international,
        quote_token,
        expires_at: until,
    })
}

/// The first open move window on `date` this move can take — with a survey
/// window far enough ahead of it when the move needs one.
fn window_for(
    surveys: &[SlotView],
    moves: &[SlotView],
    date: NaiveDate,
    survey_required: bool,
    now: DateTime<Utc>,
    offset: i32,
) -> Option<DateTime<Utc>> {
    moves
        .iter()
        .filter(|m| m.open && local_today(m.starts_at, offset) == date)
        .find(|m| {
            !survey_required
                || surveys
                    .iter()
                    .any(|sv| sv.open && check_schedule(Some(sv.starts_at), m.starts_at, true, now, offset).is_ok())
        })
        .map(|m| m.starts_at)
}

/// Every minute: lapse holds not claimed and dates gone by, then offer each
/// date's opened windows to its queue, first come first served.
pub async fn sweep(s: &AppState) -> anyhow::Result<()> {
    let offset = s.svc.home_rates.utc_offset_minutes;
    let now = Utc::now();
    let lapsed = home_waitlist::lapse_expired(&s.pool).await?;
    if lapsed > 0 {
        tracing::info!(lapsed, "home waitlist: holds lapsed unclaimed");
    }
    home_waitlist::lapse_past(&s.pool, local_today(now, offset)).await?;

    for e in home_waitlist::waiting(&s.pool, local_today(now, offset)).await? {
        let (surveys, moves, checked) = match calendar(s, e.tenant_id, e.large_estate, e.international, None).await {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(waitlist_id = %e.id, err = %err, "home waitlist: calendar unreadable — next sweep");
                continue;
            }
        };
        // Never offer against capacity nobody counted.
        if !checked {
            continue;
        }
        let survey_required = e.quote.get("survey_required").and_then(serde_json::Value::as_bool).unwrap_or(false);
        let Some(move_at) = window_for(&surveys, &moves, e.wanted_date, survey_required, now, offset) else { continue };
        let expires = now + Duration::minutes(HOLD_MINUTES);
        if !home_waitlist::offer(&s.pool, e.id, move_at, expires).await? {
            continue;
        }
        tracing::info!(waitlist_id = %e.id, %move_at, "home waitlist: window offered");
        let window = (move_at + Duration::minutes(i64::from(offset))).format("%a %-d %b, %-I:%M %p").to_string();
        let notice = Event::new("logisticos/order-intake", "home.notice", e.tenant_id, HomeNotice {
            tenant_id: e.tenant_id,
            account_id: e.account_id,
            kind: "waitlist_offered".into(),
            reference_id: e.id,
            vars: serde_json::json!({
                "window": window,
                "hold_minutes": HOLD_MINUTES,
                "deep_link": format!("logisticos://move/home/waitlist/{}", e.id),
            }),
        });
        let payload = serde_json::to_string(&notice)?;
        // The hold stands whether or not the push goes: the app shows it too.
        if let Err(err) = s.svc.publisher.publish(topics::HOME_NOTICE, &e.id.to_string(), &payload).await {
            tracing::warn!(waitlist_id = %e.id, err = %err, "home waitlist: offer notice not published");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn slot(at: DateTime<Utc>, open: bool) -> SlotView {
        SlotView { starts_at: at, ends_at: at + Duration::hours(4), open, surge_bps: 10_000 }
    }

    #[test]
    fn the_first_open_window_that_day_is_offered() {
        let now = Utc.with_ymd_and_hms(2026, 9, 19, 2, 0, 0).single().expect("now");
        // Sat 26 Sep, 08:00 and 13:00 local (UTC+8).
        let morning = Utc.with_ymd_and_hms(2026, 9, 26, 0, 0, 0).single().expect("am");
        let afternoon = morning + Duration::hours(5);
        let date = NaiveDate::from_ymd_opt(2026, 9, 26).expect("date");
        let moves = [slot(morning, false), slot(afternoon, true), slot(afternoon + Duration::days(1), true)];
        assert_eq!(window_for(&[], &moves, date, false, now, 480), Some(afternoon));
        // Nothing open that day: nothing offered, even with the next day open.
        assert_eq!(window_for(&[], &moves[..1], date, false, now, 480), None);
    }

    #[test]
    fn a_surveyed_move_needs_a_survey_window_far_enough_ahead() {
        let now = Utc.with_ymd_and_hms(2026, 9, 19, 2, 0, 0).single().expect("now");
        let move_at = Utc.with_ymd_and_hms(2026, 9, 26, 0, 0, 0).single().expect("move");
        let date = NaiveDate::from_ymd_opt(2026, 9, 26).expect("date");
        let moves = [slot(move_at, true)];
        // A survey the day before the move is too late.
        let late = [slot(move_at - Duration::days(1), true)];
        assert_eq!(window_for(&late, &moves, date, true, now, 480), None);
        // Three days ahead will do.
        let early = [slot(move_at - Duration::days(3), true)];
        assert_eq!(window_for(&early, &moves, date, true, now, 480), Some(move_at));
    }
}
