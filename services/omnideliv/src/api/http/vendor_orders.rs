//! The vendor's own order queue.
//!
//! `/me` resolves the store from claims, never from the path — the same rule
//! `vendors.rs` states, for the same reason: a vendor id in the URL would let
//! any signed-in store read and act on another store's orders.
//!
//! **The queue endpoint is the record.** Every notification channel added later
//! is a hint that something is on it. A push that never arrives costs a poll
//! interval; it must never cost an order. This is the same discipline dispatch's
//! offer routes follow for a driver whose app restarted.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use logisticos_auth::middleware::AuthClaims;
use serde::Deserialize;
use uuid::Uuid;

use crate::api::http::AppState;
use crate::domain::entities::LegStatus;
use crate::domain::repositories::{LegTransition, TransitionResponse, VendorLegRow};
use crate::infrastructure::messaging::LegRef;

/// Bounds on what a store may promise. An unbounded value silently becomes an
/// SLA nobody agreed to; the database enforces the same range, because the API
/// is not the only thing that will ever write this column.
const READY_MIN: i32 = 1;
const READY_MAX: i32 = 240;

/// Longest accepted idempotency key. Long enough for a UUID or a ULID with room
/// to spare, short enough that a client cannot use the column as storage.
const MAX_IDEMPOTENCY_KEY: usize = 200;

#[derive(Debug, Deserialize)]
pub struct AcceptRequest {
    pub ready_in_minutes: i32,
}

#[derive(Debug, Deserialize)]
pub struct RejectRequest {
    pub reason: String,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/vendors/me/orders", get(queue))
        .route("/v1/omnideliv/vendors/me/legs/:leg_id/accept", post(accept))
        .route("/v1/omnideliv/vendors/me/legs/:leg_id/reject", post(reject))
        .route("/v1/omnideliv/vendors/me/legs/:leg_id/ready", post(ready))
        .route("/v1/omnideliv/vendors/me/legs/:leg_id/served", post(served))
}

/// Resolves the caller's store.
///
/// 404 rather than 403 for a caller who runs no store: that is an absence, not
/// a permission failure — the same choice `vendors::me` makes.
async fn my_vendor_id(st: &AppState, claims: &AuthClaims) -> Result<Uuid, StatusCode> {
    st.catalog
        .vendor_for_user(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(|v| v.id)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn queue(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<Vec<VendorLegRow>>, StatusCode> {
    let vendor_id = my_vendor_id(&st, &claims).await?;
    let rows = st.legs.list_open(claims.tenant_id, vendor_id).await.map_err(|e| {
        tracing::error!(err = %e, "vendor queue read failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows))
}

/// Reads the idempotency key, if the client sent a usable one.
///
/// Absent is fine — the guarded transition already makes a duplicate submission
/// safe on its own. The key exists for the side effects that hang off a
/// transition, which a no-op must not re-fire.
fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty() && s.len() <= MAX_IDEMPOTENCY_KEY)
}

/// What every action does: resolve the store, replay-check, attempt the guarded
/// move, publish, and record the answer.
#[allow(clippy::too_many_arguments)]
async fn act(
    st: &AppState,
    claims: &AuthClaims,
    headers: &HeaderMap,
    action: &str,
    leg_id: Uuid,
    to: LegStatus,
    ready_in_minutes: Option<i32>,
    rejected_reason: Option<&str>,
) -> Result<Json<TransitionResponse>, StatusCode> {
    let vendor_id = my_vendor_id(st, claims).await?;
    let key = idempotency_key(headers);

    // Replay check before any other work — the ordering order-intake uses.
    if let Some(k) = key.as_deref() {
        if let Some(stored) = st
            .legs
            .find_idempotent_response(claims.tenant_id, vendor_id, k)
            .await
            .map_err(|e| {
                tracing::error!(err = %e, "idempotency lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        {
            return Ok(Json(stored));
        }
    }

    let outcome = st
        .legs
        .transition(claims.tenant_id, vendor_id, leg_id, to, ready_in_minutes, rejected_reason)
        .await
        .map_err(|e| {
            // The repository bails when the leg is not this vendor's. 404
            // rather than 403 so a probing caller cannot confirm the id exists.
            tracing::warn!(err = %e, %leg_id, "leg transition rejected");
            StatusCode::NOT_FOUND
        })?;

    let response = match outcome {
        LegTransition::Applied { to, order_id, goods_subtotal_cents } => {
            // Published after the write has committed, and never inside it —
            // the same rule dispatch's claim transaction follows. Best-effort:
            // a broker outage must not fail a transition that already happened,
            // because the queue endpoint remains the record either way.
            let leg = LegRef {
                tenant_id: claims.tenant_id,
                vendor_id,
                order_id,
                leg_id,
                goods_subtotal_cents,
                status: to,
            };
            let published = match to {
                LegStatus::Accepted => {
                    st.vendor_events.leg_accepted(&leg, ready_in_minutes.unwrap_or(0)).await
                }
                LegStatus::Rejected => {
                    st.vendor_events.leg_rejected(&leg, rejected_reason.unwrap_or("")).await
                }
                // `ready` and `served` have no consumer yet. They are recorded
                // in the queue, which is what a courier and a customer read.
                _ => Ok(()),
            };
            if let Err(e) = published {
                tracing::warn!(err = %e, %leg_id, status = to.as_str(),
                    "vendor leg event not published — the queue is still correct");
            }

            TransitionResponse { leg_id, status: to.as_str().to_owned(), changed: true }
        }

        // Already where the caller wanted it. Not an error: this is a tablet
        // that retried, or a second member of staff a moment later. Telling
        // them the leg is accepted is telling them the truth.
        LegTransition::NoOp { current } if current == to => {
            TransitionResponse { leg_id, status: current.as_str().to_owned(), changed: false }
        }

        // Moved somewhere else entirely. Accepting a leg that is already
        // `ready` is a real conflict, not a duplicate submission.
        LegTransition::NoOp { .. } => return Err(StatusCode::CONFLICT),
    };

    if let Some(k) = key.as_deref() {
        // Best-effort: a store that already got its answer must not be handed a
        // 500 because the replay note failed to save.
        if let Err(e) = st
            .legs
            .record_idempotent_response(claims.tenant_id, vendor_id, k, leg_id, action, &response)
            .await
        {
            tracing::warn!(err = %e, "failed to record idempotency key");
        }
    }

    Ok(Json(response))
}

async fn accept(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    headers: HeaderMap,
    Path(leg_id): Path<Uuid>,
    Json(req): Json<AcceptRequest>,
) -> Result<Json<TransitionResponse>, StatusCode> {
    if !(READY_MIN..=READY_MAX).contains(&req.ready_in_minutes) {
        return Err(StatusCode::BAD_REQUEST);
    }
    act(&st, &claims, &headers, "accept", leg_id, LegStatus::Accepted,
        Some(req.ready_in_minutes), None).await
}

async fn reject(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    headers: HeaderMap,
    Path(leg_id): Path<Uuid>,
    Json(req): Json<RejectRequest>,
) -> Result<Json<TransitionResponse>, StatusCode> {
    let reason = req.reason.trim();
    if reason.is_empty() {
        // The substitution path reads this. A blank reason makes an order that
        // died unexplainable, so it is a 400 rather than a default string.
        return Err(StatusCode::BAD_REQUEST);
    }
    act(&st, &claims, &headers, "reject", leg_id, LegStatus::Rejected, None, Some(reason)).await
}

async fn ready(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    headers: HeaderMap,
    Path(leg_id): Path<Uuid>,
) -> Result<Json<TransitionResponse>, StatusCode> {
    act(&st, &claims, &headers, "ready", leg_id, LegStatus::Ready, None, None).await
}

async fn served(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    headers: HeaderMap,
    Path(leg_id): Path<Uuid>,
) -> Result<Json<TransitionResponse>, StatusCode> {
    act(&st, &claims, &headers, "served", leg_id, LegStatus::Served, None, None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with(key: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-idempotency-key", HeaderValue::from_str(key).unwrap());
        h
    }

    #[test]
    fn a_missing_key_is_simply_absent() {
        assert_eq!(idempotency_key(&HeaderMap::new()), None);
    }

    #[test]
    fn a_blank_key_is_treated_as_absent_rather_than_stored() {
        // Storing "" would make every keyless-but-header-sending client share
        // one replay slot and answer each other's requests.
        assert_eq!(idempotency_key(&headers_with("   ")), None);
    }

    #[test]
    fn an_overlong_key_is_rejected_rather_than_truncated() {
        // Truncating would map two distinct requests onto one key, which is
        // worse than ignoring the header.
        let long = "k".repeat(MAX_IDEMPOTENCY_KEY + 1);
        assert_eq!(idempotency_key(&headers_with(&long)), None);
        let ok = "k".repeat(MAX_IDEMPOTENCY_KEY);
        assert_eq!(idempotency_key(&headers_with(&ok)), Some(ok));
    }

    #[test]
    fn a_key_is_trimmed_so_whitespace_does_not_fork_the_replay_slot() {
        assert_eq!(idempotency_key(&headers_with("  abc  ")), Some("abc".to_owned()));
    }

    #[test]
    fn the_promised_ready_window_is_bounded_at_both_ends() {
        // Zero would mean "already ready" without the store ever saying so, and
        // a negative would put ready_at before accepted_at.
        assert!(!(READY_MIN..=READY_MAX).contains(&0));
        assert!(!(READY_MIN..=READY_MAX).contains(&-5));
        assert!(!(READY_MIN..=READY_MAX).contains(&(READY_MAX + 1)));
        assert!((READY_MIN..=READY_MAX).contains(&1));
        assert!((READY_MIN..=READY_MAX).contains(&20));
        assert!((READY_MIN..=READY_MAX).contains(&READY_MAX));
    }
}
