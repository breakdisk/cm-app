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
        .route(
            "/v1/omnideliv/vendors/me/storefront",
            get(my_storefront).patch(patch_my_storefront),
        )
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

/// What a vendor has claimed for their public storefront.
#[derive(Debug, serde::Serialize)]
pub struct StorefrontSettings {
    pub slug:           Option<String>,
    pub custom_domain:  Option<String>,
    pub tagline:        Option<String>,
    pub public_enabled: bool,
    /// The shareable link, assembled server-side from the same base the table
    /// QRs use. The portal must not build this: it does not know the public
    /// origin, and a second copy of it in a `NEXT_PUBLIC_*` would be compiled
    /// into the bundle and wrong for every tenant it was not built for.
    pub public_url:     Option<String>,
}

/// A partial update.
///
/// Absent leaves a field alone; `""` clears it. Empty-string-as-clear rather
/// than a tri-state `Option<Option<_>>`, because the latter needs `serde_with`
/// — and adding a crate here changes `Cargo.lock`, which rebuilds every service
/// image in CI. None of these three fields can legitimately be empty (a slug is
/// at least 3 characters, a domain at least 4), so the encoding is unambiguous.
#[derive(Debug, serde::Deserialize)]
pub struct StorefrontPatch {
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub custom_domain: Option<String>,
    #[serde(default)]
    pub tagline: Option<String>,
    #[serde(default)]
    pub public_enabled: Option<bool>,
}

fn settings_of(v: &crate::domain::entities::Vendor, base: &str) -> StorefrontSettings {
    // A custom domain wins when set: that is the whole point of pointing one at
    // us, and showing the platform link instead would be showing the vendor a
    // URL they did not ask their customers to use.
    let public_url = if !v.public_enabled {
        None
    } else if let Some(d) = &v.custom_domain {
        Some(format!("https://{d}"))
    } else {
        v.slug
            .as_ref()
            .map(|s| format!("{}/s/{s}", base.trim_end_matches('/')))
    };

    StorefrontSettings {
        slug:           v.slug.clone(),
        custom_domain:  v.custom_domain.clone(),
        tagline:        v.tagline.clone(),
        public_enabled: v.public_enabled,
        public_url,
    }
}

async fn my_storefront(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<StorefrontSettings>, (StatusCode, String)> {
    let vendor = st
        .vendors
        .find_by_user(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not load".to_string())
        })?
        .ok_or((StatusCode::NOT_FOUND, "you do not operate a store".to_string()))?;

    Ok(Json(settings_of(&vendor, &st.table_scan_base_url)))
}

/// `PATCH /v1/omnideliv/vendors/me/storefront`
///
/// A vendor claims their own public link. Deliberately `/me`-scoped rather than
/// operator-only: the slug is the vendor's public identity, and needing a
/// support ticket to change it is how nobody ever sets one.
async fn patch_my_storefront(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(p): Json<StorefrontPatch>,
) -> Result<Json<StorefrontSettings>, (StatusCode, String)> {
    let mut vendor = st
        .vendors
        .find_by_user(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not load".to_string())
        })?
        .ok_or((StatusCode::NOT_FOUND, "you do not operate a store".to_string()))?;

    // Validate before assigning anything, so a rejected change leaves the
    // storefront exactly as it was.
    if let Some(raw) = p.slug {
        vendor.slug = if raw.trim().is_empty() {
            None
        } else {
            Some(
                crate::domain::entities::check_slug(&raw)
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
            )
        };
    }
    if let Some(raw) = p.custom_domain {
        vendor.custom_domain = if raw.trim().is_empty() {
            None
        } else {
            Some(
                crate::domain::entities::check_custom_domain(&raw)
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
            )
        };
    }
    if let Some(raw) = p.tagline {
        let t: String = raw.trim().chars().take(160).collect();
        vendor.tagline = if t.is_empty() { None } else { Some(t) };
    }
    if let Some(on) = p.public_enabled {
        vendor.public_enabled = on;
    }

    // Publishing with no handle produces a storefront nobody can reach, which
    // reads to the vendor as the feature being broken.
    if vendor.public_enabled && vendor.slug.is_none() && vendor.custom_domain.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "choose a link name before publishing — otherwise the storefront has no address"
                .to_string(),
        ));
    }

    let saved = st
        .vendors
        .set_public_handle(
            claims.tenant_id,
            vendor.id,
            vendor.slug.as_deref(),
            vendor.custom_domain.as_deref(),
            vendor.tagline.as_deref(),
            vendor.public_enabled,
        )
        .await
        .map_err(|e| {
            // The unique indexes are the arbiter of who owns a name, and a
            // clash is a normal race between two vendors, not a fault.
            let taken = e
                .to_string()
                .contains("idx_vendor_slug")
                || e.to_string().contains("idx_vendor_custom_domain");
            if taken {
                return (
                    StatusCode::CONFLICT,
                    "that link name or domain is already taken".to_string(),
                );
            }
            tracing::error!(err = %e, "storefront settings save failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not save".to_string())
        })?;

    if !saved {
        return Err((StatusCode::NOT_FOUND, "you do not operate a store".to_string()));
    }

    Ok(Json(settings_of(&vendor, &st.table_scan_base_url)))
}
