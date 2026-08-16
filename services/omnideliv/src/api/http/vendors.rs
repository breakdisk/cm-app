//! Vendor read/write surface.
//!
//! `/me` resolves the vendor from the caller's claims — a vendor id in the path
//! would let any signed-in vendor read or edit another's store.

use std::sync::Arc;

use axum::{extract::{Path, Query, State}, http::StatusCode, routing::{get, post}, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::domain::entities::{current_period, LedgerStatus};
use crate::domain::entities::Vertical;
use logisticos_auth::rbac::permissions::VENDORS_MANAGE;

/// A vendor as the operator review queue sees it.
#[derive(Debug, Serialize)]
pub struct VendorAdminRow {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub vertical: String,
    pub status: String,
    pub has_owner: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NearQuery {
    pub vertical: String,
    pub lat: f64,
    pub lng: f64,
    #[serde(default = "default_radius")]
    pub radius_km: f64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_radius() -> f64 { 5.0 }
fn default_limit() -> i64 { 20 }

#[derive(Debug, Serialize)]
pub struct VendorSummary {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub prep_time_minutes: i32,
}

#[derive(Debug, Serialize)]
pub struct VendorProfile {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub prep_time_minutes: i32,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ProfilePatch {
    pub prep_time_minutes: Option<i32>,
    pub status: Option<String>,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/vendors", get(list_near))
        .route("/v1/omnideliv/vendors/me", get(me).patch(patch_me))
        .route("/v1/omnideliv/vendors/me/earnings", get(my_earnings))
        .route("/v1/omnideliv/vendors/apply", post(apply))
        .route("/v1/omnideliv/admin/vendors", get(list_all))
        .route("/v1/omnideliv/admin/vendors/:id/approve", post(approve))
}

async fn list_near(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Query(q): Query<NearQuery>,
) -> Result<Json<Vec<VendorSummary>>, StatusCode> {
    let vertical = match q.vertical.as_str() {
        "restaurant" => Vertical::Restaurant,
        "grocery"    => Vertical::Grocery,
        "pharmacy"   => Vertical::Pharmacy,
        "florist"    => Vertical::Florist,
        "retail"     => Vertical::Retail,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let vendors = st
        .catalog
        .vendors_near(claims.tenant_id, vertical, q.lat, q.lng, q.radius_km, q.limit)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(
        vendors.into_iter()
            .map(|v| VendorSummary {
                id: v.id,
                name: v.name,
                address: v.address,
                prep_time_minutes: v.prep_time_minutes,
            })
            .collect(),
    ))
}

async fn me(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<VendorProfile>, StatusCode> {
    let vendor = st
        .catalog
        .vendor_for_user(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        // 404 rather than 403: a customer hitting this runs no store, which is
        // an absence rather than a permission failure.
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(VendorProfile {
        id: vendor.id,
        name: vendor.name,
        address: vendor.address,
        prep_time_minutes: vendor.prep_time_minutes,
        status: vendor.status.as_str().to_string(),
    }))
}

/// `POST /v1/omnideliv/vendors/apply` — a store applies to sell.
///
/// Creates it in `onboarding`, which `is_orderable()` excludes — so it cannot
/// be searched, proposed by an agent, or ordered from until someone approves
/// it. Until now the only way in was an INSERT by hand.
async fn apply(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(req): Json<ApplyRequest>,
) -> Result<Json<VendorProfile>, StatusCode> {
    let vertical = parse_vertical(&req.vertical).ok_or(StatusCode::BAD_REQUEST)?;
    if req.name.trim().is_empty() || req.address.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let v = st
        .catalog
        .apply_as_vendor(
            claims.tenant_id, claims.user_id, vertical,
            req.name.trim().to_owned(), req.address.trim().to_owned(),
            req.lat, req.lng,
        )
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor application failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(VendorProfile {
        id: v.id, name: v.name, address: v.address,
        prep_time_minutes: v.prep_time_minutes,
        status: v.status.as_str().to_string(),
    }))
}

/// `POST /v1/omnideliv/admin/vendors/:id/approve` — operator approval.
///
/// Separate from applying on purpose: a store that could list itself would mean
/// anyone with a login can put food in front of customers.
///
/// NOTE: gated only by `require_auth` today — same open per-role RBAC question
/// as the payout run, and this route belongs in the same first batch.
/// Every vendor in the tenant with its status — the operator review queue.
/// `list_near` cannot serve this: it returns only active stores, which are by
/// definition the ones already past review.
async fn list_all(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<Vec<VendorAdminRow>>, StatusCode> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err(StatusCode::FORBIDDEN);
    }
    let vendors = st.catalog.list_vendors(claims.tenant_id).await.map_err(|e| {
        tracing::error!(err = %e, "vendor list failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(vendors.into_iter().map(|v| VendorAdminRow {
        id: v.id,
        name: v.name,
        address: v.address,
        vertical: v.vertical.as_str().to_string(),
        status: v.status.as_str().to_string(),
        // Null means the store is unreachable by any login: nobody can manage
        // its catalog, because /vendors/me resolves by user_id. Seeded rows
        // land this way, so the operator needs to see it.
        has_owner: v.user_id.is_some(),
        created_at: v.created_at.to_rfc3339(),
    }).collect()))
}

async fn approve(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    // This route used to take `_claims` and check nothing, so any signed-in
    // user could approve any vendor -- including the application they had just
    // submitted themselves. That erases the entire point of Onboarding being a
    // separate state from Active.
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err(StatusCode::FORBIDDEN);
    }
    let ok = st.catalog.approve_vendor(claims.tenant_id, id).await.map_err(|e| {
        tracing::error!(err = %e, "vendor approval failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if ok { Ok(StatusCode::NO_CONTENT) } else { Err(StatusCode::NOT_FOUND) }
}

#[derive(Debug, Deserialize)]
pub struct ApplyRequest {
    pub vertical: String,
    pub name:     String,
    pub address:  String,
    pub lat:      f64,
    pub lng:      f64,
}

fn parse_vertical(s: &str) -> Option<Vertical> {
    Some(match s {
        "restaurant" => Vertical::Restaurant,
        "grocery"    => Vertical::Grocery,
        "pharmacy"   => Vertical::Pharmacy,
        "florist"    => Vertical::Florist,
        "retail"     => Vertical::Retail,
        _ => return None,
    })
}

/// `GET /v1/omnideliv/vendors/me/earnings` — this period's payouts.
///
/// The vendor comes from the token, never a parameter: an id a caller could
/// name would let one store read another's takings. Until now there was no read
/// path at all — the only way to see whether a collected order had actually
/// been credited was psql.
async fn my_earnings(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<EarningsResponse>, StatusCode> {
    let vendor = st
        .catalog
        .vendor_for_user(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let period = current_period();
    let ledger = st
        .ledgers
        .find_open(claims.tenant_id, vendor.id, &period)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor ledger lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // No ledger yet is zero, not 404. A store that is open and has sold nothing
    // this week has earnings, and they are nil — a 404 would read as "your
    // store does not exist".
    // Closed and settled periods, so the card can say what is owed and what has
    // already been paid. `find_open` alone can never show either: the moment a
    // period closes it stops returning it, and the vendor's view of everything
    // they had earned went to zero.
    let recent = st
        .ledgers
        .list_recent(claims.tenant_id, vendor.id, 8)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor ledger history lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let awaiting_payout_cents = recent
        .iter()
        .filter(|p| p.status == LedgerStatus::Closed)
        .map(|p| p.balance_cents)
        .sum();
    let paid_cents = recent
        .iter()
        .filter(|p| p.status == LedgerStatus::Settled)
        .map(|p| p.balance_cents)
        .sum();
    let periods: Vec<PeriodSummary> = recent
        .iter()
        .map(|p| PeriodSummary {
            period:        p.period.clone(),
            status:        p.status.as_str().to_string(),
            balance_cents: p.balance_cents,
            updated_at:    p.updated_at,
        })
        .collect();

    let Some(l) = ledger else {
        return Ok(Json(EarningsResponse {
            period,
            balance_cents: 0,
            awaiting_payout_cents,
            paid_cents,
            periods,
            entries: Vec::new(),
        }));
    };

    Ok(Json(EarningsResponse {
        period:        l.period.clone(),
        balance_cents: l.balance_cents,
        awaiting_payout_cents,
        paid_cents,
        periods,
        entries: l
            .entries
            .iter()
            .map(|e| EarningEntry {
                kind:         format!("{:?}", e.kind).to_lowercase(),
                amount_cents: e.amount_cents,
                order_id:     e.order_id,
                at:           e.created_at,
            })
            .collect(),
    }))
}

#[derive(Debug, Serialize)]
struct EarningEntry {
    kind:         String,
    /// Signed as stored — credits positive, payouts negative — so a client
    /// summing the list gets the balance and cannot disagree with it.
    amount_cents: i64,
    order_id:     Option<Uuid>,
    at:           chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct PeriodSummary {
    period:        String,
    /// `open` | `closed` | `settled`. The distinction the console needs: an open
    /// figure is still moving, a closed one is owed, a settled one is history.
    status:        String,
    balance_cents: i64,
    updated_at:    chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct EarningsResponse {
    /// The period still accruing, and its running total. Not payable yet.
    period:        String,
    balance_cents: i64,
    /// Closed but not yet settled — what the vendor is actually owed.
    awaiting_payout_cents: i64,
    /// Already settled. Shown so "where did my money go" has an answer on the
    /// same card as the question.
    paid_cents:    i64,
    periods:       Vec<PeriodSummary>,
    entries:       Vec<EarningEntry>,
}

async fn patch_me(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(p): Json<ProfilePatch>,
) -> Result<StatusCode, (StatusCode, String)> {
    // A vendor may pause or resume itself. It may not offboard itself, or mark
    // itself active while still onboarding — those are Partner decisions.
    if let Some(s) = p.status.as_deref() {
        if !matches!(s, "active" | "paused") {
            return Err((StatusCode::FORBIDDEN, "that status is not yours to set".into()));
        }
    }
    if let Some(m) = p.prep_time_minutes {
        // A negative prep time would sort this vendor first in every
        // consolidation plan; three hours is already generous for a kitchen.
        if !(0..=180).contains(&m) {
            return Err((StatusCode::BAD_REQUEST, "prep time must be 0-180 minutes".into()));
        }
    }

    let updated = st
        .catalog
        .update_own_vendor(claims.tenant_id, claims.user_id, p.prep_time_minutes, p.status)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor profile update failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not save".into())
        })?;

    if !updated {
        return Err((StatusCode::NOT_FOUND, "you do not operate a store".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}
