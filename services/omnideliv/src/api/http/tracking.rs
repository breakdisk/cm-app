//! Screen D's data. Poll rather than stream: after checkout the interesting
//! events are minutes apart, so an SSE connection held open across a delivery
//! costs a socket per in-flight order to save a request every few seconds.

use std::sync::Arc;

use axum::{extract::{Path, State}, http::StatusCode, routing::get, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::Serialize;
use uuid::Uuid;

use crate::api::http::AppState;

#[derive(Debug, Serialize)]
pub struct TimelineEntry {
    pub event_type: String,
    /// The device clock where there was one, so the timeline shows when things
    /// happened rather than when the server heard about them.
    pub at:         chrono::DateTime<chrono::Utc>,
    pub payload:    serde_json::Value,
}

/// One pickup on the way, for the map.
#[derive(Debug, Serialize)]
pub struct StopView {
    pub vendor_name: String,
    pub lat:         f64,
    pub lng:         f64,
    pub picked_up:   bool,
}

#[derive(Debug, Serialize)]
pub struct TrackResponse {
    pub order_id:          Uuid,
    pub status:            String,
    /// The breakdown a receipt needs. Already columns on `omnideliv.orders` —
    /// the list simply never selected them, so a customer could see what they
    /// owed and never what for.
    pub goods_total_cents:  i64,
    pub delivery_fee_cents: i64,
    pub tip_cents:          i64,
    pub grand_total_cents: i64,
    pub stops_total:       usize,
    pub stops_collected:   usize,
    pub delivered_at:      Option<chrono::DateTime<chrono::Utc>>,
    pub timeline:          Vec<TimelineEntry>,
    /// `None` unless the order is in motion and the fix is fresh. Never a
    /// last-known point presented as live — a frozen dot a customer believes is
    /// moving is worse than an honest gap.
    pub courier:     Option<crate::application::services::CourierFix>,
    pub eta:         Option<crate::domain::entities::eta::EtaEstimate>,
    /// `None` for orders placed before migration 0013, which carry no
    /// destination. Those degrade to a timeline rather than a guessed point.
    pub destination: Option<crate::domain::entities::eta::LatLng>,
    pub stops:       Vec<StopView>,
}

/// Whether a live courier position may be disclosed for an order in this state.
///
/// Not `placed` or `awaiting_courier` (nobody is carrying it yet), and
/// deliberately not after `delivered` or `cancelled` — a courier's live
/// location must not stay readable from a finished order for the rest of
/// their shift.
fn discloses_position(status: crate::domain::entities::OrderStatus) -> bool {
    use crate::domain::entities::OrderStatus::*;
    matches!(status, Collecting | Delivering)
}

/// The position to actually put on the wire, given the order's state and
/// whatever field-ops last heard.
///
/// Both rules live here rather than inline in the handler so they are testable
/// *together*. Separately they were each correct and the pair was not: the
/// status gate was tested, `is_fresh` was tested, and nothing asserted that a
/// stale fix is dropped — so a courier whose phone died kept a pulsing dot on
/// the customer's screen while the ETA silently disappeared.
fn disclosable_position(
    status: crate::domain::entities::OrderStatus,
    fix: Option<crate::application::services::CourierFix>,
) -> Option<crate::application::services::CourierFix> {
    if !discloses_position(status) {
        return None;
    }
    fix.filter(crate::domain::entities::eta::is_fresh)
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/orders", get(my_orders))
        .route("/v1/omnideliv/orders/:id/track", get(track))
}

async fn track(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(order_id): Path<Uuid>,
) -> Result<Json<TrackResponse>, StatusCode> {
    let order = st
        .orders
        .find_by_id(claims.tenant_id, order_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "order lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        // Tenant-scoped in the query, so another tenant's order reads as absent
        // rather than leaking that it exists.
        .ok_or(StatusCode::NOT_FOUND)?;

    // ...and scoped to the customer too. Tenant alone let any signed-in user
    // track any order in the tenant by id — its total, its progress and its
    // whole timeline. NotFound rather than Forbidden, so the response does not
    // confirm the id belongs to a real order.
    if order.customer_id != claims.user_id {
        return Err(StatusCode::NOT_FOUND);
    }

    // A missing timeline is not a missing order. Tracking still answers with
    // the order's state — degrading the detail rather than the whole screen.
    let timeline = st
        .telemetry
        .timeline(claims.tenant_id, order_id)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(err = %e, %order_id, "timeline lookup failed; returning state only");
            Vec::new()
        });

    let stops_collected = order
        .legs
        .iter()
        .filter(|l| l.status == crate::domain::entities::LegStatus::PickedUp)
        .count();

    // Checked before the outbound call, so it is one gate rather than a rule
    // spread across two services, and it saves the call.
    let reported = match (discloses_position(order.status), order.courier_task_id) {
        (true, Some(assignment_id)) => st
            .courier_telemetry
            .position(claims.tenant_id, assignment_id)
            .await
            // A telemetry failure degrades the map and never fails the screen,
            // exactly as a missing timeline degrades rather than 404s above.
            .unwrap_or_else(|e| {
                tracing::error!(err = %e, %order_id, "courier position lookup failed");
                None
            }),
        _ => None,
    };

    // Both rules, applied together. A stale fix is not a position: field-ops
    // answers with the newest row it holds however old that is, so freshness is
    // this layer's to enforce and it must be enforced on the dot, not only on
    // the ETA.
    let courier = disclosable_position(order.status, reported);

    let vendor_ids: Vec<uuid::Uuid> = order.legs.iter().map(|l| l.vendor_id).collect();
    let vendors = st
        .vendors
        .find_by_ids(claims.tenant_id, &vendor_ids)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(err = %e, %order_id, "vendor lookup failed; map will omit stops");
            Vec::new()
        });

    let stops: Vec<StopView> = order
        .legs
        .iter()
        .filter_map(|l| {
            let v = vendors.iter().find(|v| v.id == l.vendor_id)?;
            Some(StopView {
                vendor_name: v.name.clone(),
                lat:         v.lat,
                lng:         v.lng,
                picked_up:   l.status == crate::domain::entities::LegStatus::PickedUp,
            })
        })
        .collect();

    let destination = match (order.delivery_lat, order.delivery_lng) {
        (Some(lat), Some(lng)) => Some(crate::domain::entities::eta::LatLng { lat, lng }),
        _ => None,
    };

    // Only stops still to collect count towards the remaining journey.
    let remaining: Vec<crate::domain::entities::eta::LatLng> = stops
        .iter()
        .filter(|s| !s.picked_up)
        .map(|s| crate::domain::entities::eta::LatLng { lat: s.lat, lng: s.lng })
        .collect();

    let eta = match (&courier, destination) {
        (Some(fix), Some(dest)) => {
            crate::domain::entities::eta::estimate_eta(fix, &remaining, dest)
        }
        _ => None,
    };

    Ok(Json(TrackResponse {
        order_id:          order.id,
        status:            order.status.as_str().to_string(),
        goods_total_cents:  order.goods_total_cents,
        delivery_fee_cents: order.delivery_fee_cents,
        tip_cents:          order.tip_cents,
        grand_total_cents: order.grand_total_cents,
        stops_total:       order.legs.len(),
        stops_collected,
        delivered_at:      order.delivered_at,
        timeline: timeline
            .into_iter()
            .map(|e| TimelineEntry {
                at:         e.sla_timestamp(),
                event_type: e.event_type,
                payload:    e.payload,
            })
            .collect(),
        courier,
        eta,
        destination,
        stops,
    }))
}

/// One row of the caller's order list.
#[derive(Debug, serde::Serialize)]
pub struct OrderListItem {
    pub order_id:          uuid::Uuid,
    pub status:            String,
    /// The breakdown a receipt needs. Already columns on `omnideliv.orders` —
    /// the list simply never selected them, so a customer could see what they
    /// owed and never what for.
    pub goods_total_cents:  i64,
    pub delivery_fee_cents: i64,
    pub tip_cents:          i64,
    pub grand_total_cents: i64,
    pub stops_total:       i64,
    pub vendor_names:      String,
    pub placed_at:         chrono::DateTime<chrono::Utc>,
    pub delivered_at:      Option<chrono::DateTime<chrono::Utc>>,
}

/// `GET /v1/omnideliv/orders` — the caller's own orders, newest first.
///
/// Without this an order is unreachable the moment the app is closed: checkout
/// hands you a tracking screen and nothing else ever links back to it. The
/// customer id is taken from the token and never from the query string, so
/// there is no parameter here that could name someone else's history.
async fn my_orders(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<Vec<OrderListItem>>, StatusCode> {
    const LIMIT: i64 = 50;

    let summaries = st
        .orders
        .list_summaries_for_customer(claims.tenant_id, claims.user_id, LIMIT)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "order list failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(
        summaries
            .into_iter()
            .map(|s| OrderListItem {
                order_id:          s.id,
                status:            s.status,
                goods_total_cents:  s.goods_total_cents,
                delivery_fee_cents: s.delivery_fee_cents,
                tip_cents:          s.tip_cents,
                grand_total_cents: s.grand_total_cents,
                stops_total:       s.stops_total,
                vendor_names:      s.vendor_names,
                placed_at:         s.placed_at,
                delivered_at:      s.delivered_at,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::OrderStatus::*;

    #[test]
    fn position_is_disclosed_only_while_the_order_is_in_motion() {
        assert!(discloses_position(Collecting));
        assert!(discloses_position(Delivering));
    }

    /// The one that matters: a courier's live location must not remain
    /// readable from an order that is already finished.
    #[test]
    fn a_finished_order_never_discloses_a_position() {
        assert!(!discloses_position(Delivered));
        assert!(!discloses_position(Cancelled));
    }

    #[test]
    fn an_order_with_no_courier_yet_discloses_nothing() {
        assert!(!discloses_position(Placed));
        assert!(!discloses_position(AwaitingCourier));
    }

    fn fix(age: i64) -> crate::application::services::CourierFix {
        crate::application::services::CourierFix {
            lat: 14.5995,
            lng: 120.9842,
            heading_deg: None,
            smoothed_speed_kph: Some(20.0),
            age_seconds: age,
        }
    }

    #[test]
    fn a_fresh_fix_on_a_moving_order_is_disclosed() {
        assert!(disclosable_position(Delivering, Some(fix(5))).is_some());
        assert!(disclosable_position(Collecting, Some(fix(5))).is_some());
    }

    /// The bug this function exists for. field-ops answers with the newest row
    /// it holds however old that is, so a courier whose phone died mid-route
    /// keeps reporting one position forever. Showing it is worse than showing
    /// nothing: the customer reads a stationary dot as a courier who has
    /// stopped, not as a signal that was lost.
    #[test]
    fn a_stale_fix_is_dropped_even_while_the_order_is_moving() {
        let stale = crate::domain::entities::eta::FIX_STALE_AFTER_SECS + 1;
        assert!(disclosable_position(Delivering, Some(fix(stale))).is_none());
        assert!(disclosable_position(Collecting, Some(fix(stale))).is_none());
    }

    /// Freshness must not resurrect a position the status gate refused.
    #[test]
    fn a_finished_order_discloses_nothing_however_fresh_the_fix() {
        assert!(disclosable_position(Delivered, Some(fix(0))).is_none());
        assert!(disclosable_position(Cancelled, Some(fix(0))).is_none());
        assert!(disclosable_position(Placed, Some(fix(0))).is_none());
    }

    /// The boundary itself, so the two consumers cannot drift apart: anything
    /// this function keeps, `estimate_eta` must also be willing to price.
    #[test]
    fn the_freshness_boundary_matches_the_one_the_eta_uses() {
        use crate::domain::entities::eta::{estimate_eta, LatLng, FIX_STALE_AFTER_SECS};
        let dest = LatLng { lat: 14.60, lng: 120.99 };

        let at_limit = fix(FIX_STALE_AFTER_SECS);
        assert!(disclosable_position(Delivering, Some(at_limit.clone())).is_some());
        assert!(estimate_eta(&at_limit, &[], dest).is_some());

        let past = fix(FIX_STALE_AFTER_SECS + 1);
        assert!(disclosable_position(Delivering, Some(past.clone())).is_none());
        assert!(estimate_eta(&past, &[], dest).is_none());
    }
}
