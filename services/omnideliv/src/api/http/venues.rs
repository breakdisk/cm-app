//! Setting a venue up: creating it, giving it tables, and saying which vendors
//! sell there.
//!
//! This is the half of QR table ordering that shipped missing. The schema, the
//! scan endpoint, the diner principal, the venue-scoped basket and the dine-in
//! order were all built and deployed — with no way to create a venue, a table,
//! or a vendor link. Every row had to be inserted by hand in SQL, so the
//! feature was unreachable in production despite being live.
//!
//! Deliberately separate from `tables`, which is about the two opposite-trust
//! audiences of an existing code. Setup is a third concern with a third shape:
//! everything here is operator-authenticated, tenant-scoped, and idempotent
//! where an operator might reasonably click twice.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions::VENDORS_MANAGE;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::domain::entities::{
    NotOrderable, OpeningWindow, Table, Venue, VenueKind,
};

#[derive(Debug, Deserialize)]
pub struct CreateVenueRequest {
    pub name: String,
    /// `"standalone"` (default) or `"foodcourt"`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Empty is allowed and means the venue is closed — see `Venue::new`. The
    /// response says so rather than the operator discovering it from codes
    /// that refuse every scan.
    #[serde(default)]
    pub hours: Vec<OpeningWindow>,
    /// **Required on purpose, with no default.** The column defaults to 480
    /// (PH), and silently handing a Dubai venue Manila's clock would make its
    /// opening hours four hours wrong with nothing to see. A missing field is
    /// a loud 422 instead.
    pub utc_offset_minutes: i32,
}

#[derive(Debug, Deserialize)]
pub struct CreateTablesRequest {
    /// One or more labels. Batch, because a restaurant sets up twenty tables
    /// at once and twenty round trips is not a setup flow.
    pub labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct LinkVendorRequest {
    pub vendor_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct VenueRow {
    pub venue_id:           Uuid,
    pub name:               String,
    pub kind:               String,
    pub status:             String,
    pub utc_offset_minutes: i32,
    pub hours:              Vec<OpeningWindow>,
    /// `null` when a printed code would scan right now; otherwise why it would
    /// not. **Operator-only.** The scan endpoint keeps collapsing every one of
    /// these into one indistinguishable 404 — this exists so the person who
    /// set the venue up can be told what the diner deliberately cannot.
    pub not_orderable:      Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NewTableRow {
    pub table_id: Uuid,
    pub label:    String,
    pub scan_url: String,
}

#[derive(Debug, Serialize)]
pub struct VendorRow {
    pub vendor_id: Uuid,
    pub name:      String,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/venues", get(list_venues).post(create_venue))
        .route("/v1/omnideliv/venues/:venue_id", get(get_venue))
        .route("/v1/omnideliv/venues/:venue_id/tables", post(create_tables))
        .route(
            "/v1/omnideliv/venues/:venue_id/vendors",
            get(list_vendors).post(link_vendor),
        )
        .route(
            "/v1/omnideliv/venues/:venue_id/vendors/:vendor_id",
            axum::routing::delete(unlink_vendor),
        )
}

/// Why a code at this venue would not scan, as a wire string.
fn hint_str(h: NotOrderable) -> &'static str {
    match h {
        NotOrderable::VenueNotActive => "venue_not_active",
        NotOrderable::TableClosed => "table_closed",
        NotOrderable::OutsideOpeningHours => "outside_opening_hours",
    }
}

fn row_of(v: &Venue, now: chrono::DateTime<Utc>) -> VenueRow {
    VenueRow {
        venue_id:           v.id,
        name:               v.name.clone(),
        kind:               v.kind.as_str().to_string(),
        status:             v.status.as_str().to_string(),
        utc_offset_minutes: v.utc_offset_minutes,
        hours:              v.hours.clone(),
        not_orderable:      v.orderable_now_hint(now).map(hint_str).map(str::to_string),
    }
}

async fn create_venue(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(req): Json<CreateVenueRequest>,
) -> Result<(StatusCode, Json<VenueRow>), (StatusCode, String)> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err((StatusCode::FORBIDDEN, "not permitted".into()));
    }

    let kind = match req.kind.as_deref() {
        None => VenueKind::Standalone,
        Some(k) => VenueKind::from_wire(k)
            .ok_or((StatusCode::BAD_REQUEST, format!("unknown venue kind: {k}")))?,
    };

    let now = Utc::now();
    // Tenant from the validated token, never the body — the same rule the
    // checkout path follows, and for the same reason.
    let venue = Venue::new(
        claims.tenant_id,
        &req.name,
        kind,
        req.hours,
        req.utc_offset_minutes,
        now,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    st.venues.create_venue(&venue).await.map_err(|e| {
        tracing::error!(err = %e, "venue create failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "venue create failed".into())
    })?;

    Ok((StatusCode::CREATED, Json(row_of(&venue, now))))
}

async fn list_venues(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<Vec<VenueRow>>, (StatusCode, String)> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err((StatusCode::FORBIDDEN, "not permitted".into()));
    }
    let now = Utc::now();
    let venues = st.venues.list_venues(claims.tenant_id).await.map_err(|e| {
        tracing::error!(err = %e, "venue list failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "venue list failed".into())
    })?;
    Ok(Json(venues.iter().map(|v| row_of(v, now)).collect()))
}

async fn get_venue(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(venue_id): Path<Uuid>,
) -> Result<Json<VenueRow>, (StatusCode, String)> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err((StatusCode::FORBIDDEN, "not permitted".into()));
    }
    let now = Utc::now();
    let venue = st
        .venues
        .find_venue(claims.tenant_id, venue_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "venue read failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "venue read failed".into())
        })?
        .ok_or((StatusCode::NOT_FOUND, "venue not found".into()))?;
    Ok(Json(row_of(&venue, now)))
}

async fn create_tables(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(venue_id): Path<Uuid>,
    Json(req): Json<CreateTablesRequest>,
) -> Result<(StatusCode, Json<Vec<NewTableRow>>), (StatusCode, String)> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err((StatusCode::FORBIDDEN, "not permitted".into()));
    }
    if req.labels.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "at least one label is required".into()));
    }
    if req.labels.len() > 200 {
        return Err((StatusCode::BAD_REQUEST, "at most 200 tables per request".into()));
    }

    // The venue must be this tenant's before anything is written. `tables` has
    // a foreign key to `venues` but no tenant check of its own, so without this
    // a caller could hang tables off another tenant's venue.
    if st
        .venues
        .find_venue(claims.tenant_id, venue_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "venue read failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "venue read failed".into())
        })?
        .is_none()
    {
        return Err((StatusCode::NOT_FOUND, "venue not found".into()));
    }

    let now = Utc::now();
    // Built and validated in full before a single row is written, so a bad
    // label halfway down a list of twenty does not leave nineteen tables
    // created and the operator unsure which.
    let tables = req
        .labels
        .iter()
        .map(|l| Table::new(venue_id, claims.tenant_id, l, now))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // All or none — see `create_tables`. A label that already exists at this
    // venue is a 409 the operator can act on, not a 500: `UNIQUE (venue_id,
    // label)` is a rule they broke, not a fault on our side.
    st.venues.create_tables(&tables).await.map_err(|e| {
        if let Some(sqlx::Error::Database(db)) = e.downcast_ref::<sqlx::Error>() {
            if db.is_unique_violation() {
                return (
                    StatusCode::CONFLICT,
                    "one of those labels is already used at this venue".to_string(),
                );
            }
        }
        tracing::error!(err = %e, venue_id = %venue_id, "table create failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "table create failed".to_string())
    })?;

    let base = st.table_scan_base_url.trim_end_matches('/');
    let out = tables
        .iter()
        .map(|t| NewTableRow {
            table_id: t.id,
            label:    t.label.clone(),
            scan_url: format!("{base}/t/{}", t.token),
        })
        .collect();

    Ok((StatusCode::CREATED, Json(out)))
}

async fn link_vendor(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(venue_id): Path<Uuid>,
    Json(req): Json<LinkVendorRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err((StatusCode::FORBIDDEN, "not permitted".into()));
    }
    let linked = st
        .venues
        .link_vendor(claims.tenant_id, venue_id, req.vendor_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor link failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "vendor link failed".into())
        })?;

    if !linked {
        // One 404 for "no such venue" and "no such vendor" alike: the caller is
        // authenticated, but there is no reason to help them enumerate which of
        // the two ids belongs to somebody else.
        return Err((StatusCode::NOT_FOUND, "venue or vendor not found".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn unlink_vendor(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path((venue_id, vendor_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err((StatusCode::FORBIDDEN, "not permitted".into()));
    }
    let removed = st
        .venues
        .unlink_vendor(claims.tenant_id, venue_id, vendor_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor unlink failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "vendor unlink failed".into())
        })?;

    if !removed {
        return Err((StatusCode::NOT_FOUND, "not linked".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_vendors(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(venue_id): Path<Uuid>,
) -> Result<Json<Vec<VendorRow>>, (StatusCode, String)> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err((StatusCode::FORBIDDEN, "not permitted".into()));
    }
    let rows = st
        .venues
        .list_venue_vendors(claims.tenant_id, venue_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "venue vendor list failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "vendor list failed".into())
        })?;
    Ok(Json(
        rows.into_iter()
            .map(|(vendor_id, name)| VendorRow { vendor_id, name })
            .collect(),
    ))
}
