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
    NotOrderable, OpeningWindow, Table, TableStatus, Venue, VenueKind, VenueStatus,
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

/// A partial update. Every field is optional; `None` leaves it alone.
#[derive(Debug, Deserialize)]
pub struct UpdateVenueRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub hours: Option<Vec<OpeningWindow>>,
    #[serde(default)]
    pub utc_offset_minutes: Option<i32>,
    /// `"active"` | `"paused"` | `"closed"`.
    ///
    /// **This is the kill switch.** `orderable_now` refuses every scan at this
    /// venue while it is not `active`, so pausing stops table ordering across
    /// the whole building at once.
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTableRequest {
    /// `"open"` | `"closed"`. Closing stops new scans at this table only.
    pub status: String,
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
        .route(
            "/v1/omnideliv/venues/:venue_id",
            get(get_venue).patch(update_venue).delete(delete_venue),
        )
        .route("/v1/omnideliv/venues/:venue_id/tables", post(create_tables))
        .route(
            "/v1/omnideliv/venues/tables/:table_id",
            axum::routing::patch(update_table).delete(delete_table),
        )
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

/// `PATCH /v1/omnideliv/venues/:venue_id` -- edit a venue, or stop it trading.
///
/// Before this existed a venue was immutable from the moment it was created: a
/// mistyped name was permanent, hours could never change, and above all there
/// was **no way to stop table ordering**. `VenueStatus::Paused` gates every
/// scan in `orderable_now` and nothing on the platform could set it, so the
/// only way to shut a leaked or overwhelmed venue down was rotating every
/// table's token one at a time -- N operations, each permanently killing a
/// printed sticker.
async fn update_venue(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(venue_id): Path<Uuid>,
    Json(req): Json<UpdateVenueRequest>,
) -> Result<Json<VenueRow>, (StatusCode, String)> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err((StatusCode::FORBIDDEN, "not permitted".into()));
    }

    let status = match req.status.as_deref() {
        None => None,
        Some(v) => Some(
            VenueStatus::from_wire(v)
                .ok_or((StatusCode::BAD_REQUEST, format!("unknown venue status: {v}")))?,
        ),
    };

    let mut venue = st
        .venues
        .find_venue(claims.tenant_id, venue_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "venue read failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "venue read failed".to_string())
        })?
        .ok_or((StatusCode::NOT_FOUND, "venue not found".to_string()))?;

    let now = Utc::now();
    let was = venue.status;
    venue
        .apply(req.name.as_deref(), req.hours, req.utc_offset_minutes, status, now)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    if !st.venues.update_venue(&venue).await.map_err(|e| {
        tracing::error!(err = %e, "venue update failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "venue update failed".to_string())
    })? {
        return Err((StatusCode::NOT_FOUND, "venue not found".to_string()));
    }

    if was != venue.status {
        // Logged at info because it is the answer to "why did every table here
        // stop working at 7pm", which is otherwise unanswerable: the scan
        // refusal a diner sees is the same indistinguishable 404 as everything
        // else.
        tracing::info!(
            %venue_id, tenant_id = %claims.tenant_id,
            from = was.as_str(), to = venue.status.as_str(),
            "venue trading status changed",
        );
    }

    Ok(Json(row_of(&venue, now)))
}

/// `DELETE /v1/omnideliv/venues/:venue_id`
///
/// **Refuses while the venue still has tables.** The schema cascades, so an
/// unguarded delete would take every table, vendor link and live session with
/// it -- silently destroying every printed code in the building. Making the
/// caller remove the tables first turns that into an itemised, deliberate act.
async fn delete_venue(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(venue_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err((StatusCode::FORBIDDEN, "not permitted".into()));
    }

    let tables = st
        .venues
        .count_tables(claims.tenant_id, venue_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "table count failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not delete the venue".to_string(),
            )
        })?;

    if tables > 0 {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "This venue still has {tables} table(s). Remove them first: deleting the venue would invalidate every printed code at once."
            ),
        ));
    }

    if !st
        .venues
        .delete_venue(claims.tenant_id, venue_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "venue delete failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not delete the venue".to_string(),
            )
        })?
    {
        return Err((StatusCode::NOT_FOUND, "venue not found".to_string()));
    }

    tracing::info!(%venue_id, tenant_id = %claims.tenant_id, "venue deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /v1/omnideliv/venues/tables/:table_id` -- open or close one table.
///
/// Closing stops new scans at that table and leaves sessions already open
/// alone. That split is the whole point: a table being cleared, repaired or
/// re-laid stops taking orders without cancelling the meal in progress on it.
/// The printed code stays valid, so reopening is one click and not a reprint.
async fn update_table(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(table_id): Path<Uuid>,
    Json(req): Json<UpdateTableRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err((StatusCode::FORBIDDEN, "not permitted".into()));
    }
    let status = TableStatus::from_wire(&req.status).ok_or((
        StatusCode::BAD_REQUEST,
        format!("unknown table status: {}", req.status),
    ))?;

    if !st
        .venues
        .set_table_status(claims.tenant_id, table_id, status)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "table status update failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not update the table".to_string(),
            )
        })?
    {
        return Err((StatusCode::NOT_FOUND, "table not found".to_string()));
    }

    tracing::info!(%table_id, tenant_id = %claims.tenant_id, status = status.as_str(),
                   "table trading status changed");
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /v1/omnideliv/venues/tables/:table_id`
///
/// **Refuses while a session is live at that table.** Someone is sitting there
/// mid-meal; deleting cascades their session away and their basket stops
/// resolving. Closing the table is the answer for "stop using this one" --
/// deleting is for a table that no longer exists.
async fn delete_table(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(table_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err((StatusCode::FORBIDDEN, "not permitted".into()));
    }

    let live = st
        .venues
        .count_live_sessions(table_id, Utc::now())
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "live session count failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not delete the table".to_string(),
            )
        })?;

    if live > 0 {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "{live} diner session(s) are open at this table. Close the table instead: that stops new scans and lets the people sitting there finish."
            ),
        ));
    }

    if !st
        .venues
        .delete_table(claims.tenant_id, table_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "table delete failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not delete the table".to_string(),
            )
        })?
    {
        return Err((StatusCode::NOT_FOUND, "table not found".to_string()));
    }

    tracing::info!(%table_id, tenant_id = %claims.tenant_id, "table deleted");
    Ok(StatusCode::NO_CONTENT)
}
