//! Scanning a table, and printing the codes that get scanned.
//!
//! Two audiences with opposite trust levels in one file, because they are two
//! halves of one mechanism and splitting them hides that:
//!
//! - **The scan** is unauthenticated. It is the platform's only public write,
//!   reachable by anyone who can photograph a sticker.
//! - **The print sheet and rotation** are operator-authenticated, and are the
//!   answer when a code leaks.
//!
//! The routes are returned by two separate functions so the router mounts them
//! on opposite sides of the auth layer. They must never be merged.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use logisticos_auth::middleware::AuthClaims;
use serde::Serialize;
use uuid::Uuid;

use crate::api::http::AppState;
use crate::domain::entities::{new_table_token, orderable_now, TableSession};
use logisticos_auth::rbac::permissions::VENDORS_MANAGE;

/// What a scanner gets back.
///
/// Carries the venue it is now bound to, so the app can show a menu without a
/// second round trip, and nothing about any other table.
#[derive(Debug, Serialize)]
pub struct ScanResponse {
    pub session_id: Uuid,
    /// The narrow, short-lived diner principal. No roles, no permissions.
    pub access_token: String,
    pub expires_at: String,
    pub venue_id: Uuid,
    pub venue_name: String,
    pub table_label: String,
    /// Who sells at this venue.
    ///
    /// Without this a diner cannot reach a menu at all: `catalog/search`
    /// requires a `vendor_id`, and the only endpoint listing a venue's vendors
    /// is operator-gated AND closed to diners by the gateway allowlist. So the
    /// scan has to answer it, which is also what this response already claimed
    /// to do -- "so the app can show a menu without a second round trip".
    pub vendors: Vec<VendorBrief>,
}

#[derive(Debug, Serialize)]
pub struct VendorBrief {
    pub vendor_id: Uuid,
    pub name:      String,
}

#[derive(Debug, Serialize)]
pub struct TableRow {
    pub table_id: Uuid,
    pub label: String,
    pub status: String,
    /// What to encode in the printed QR.
    pub scan_url: String,
    pub printed_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RotateResponse {
    pub table_id: Uuid,
    pub scan_url: String,
}

/// Unauthenticated. Mounted BEFORE the auth layer, alongside `health` and
/// `catalog::public_routes` — a diner has no account and never will.
pub fn public_routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/omnideliv/tables/:token/session", post(scan))
}

/// Operator-authenticated. Mounted inside the auth layer.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/venues/:venue_id/tables", get(list_tables))
        .route("/v1/omnideliv/venues/tables/:table_id/rotate", post(rotate))
        .route("/v1/omnideliv/venues/tables/:table_id/printed", post(mark_printed))
}

/// `POST /v1/omnideliv/tables/:token/session` — a diner scanned the code.
///
/// **Every refusal is the same 404.** An unknown token, a closed table, a
/// paused venue and a venue outside its hours are indistinguishable to the
/// caller. A scanner probing tokens must learn nothing from the response, and
/// "this table exists but is closed" is exactly the signal that would tell them
/// they had guessed a real one.
///
/// The reasons are logged, because an operator asking "why can nobody order at
/// table 12" needs the answer even though the diner must not get it.
async fn scan(
    State(st): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(token): Path<String>,
) -> Result<Json<ScanResponse>, StatusCode> {
    let now = Utc::now();

    // Before any database work: an unauthenticated endpoint must not let an
    // unbounded caller spend queries. The gateway's limiter cannot cover this
    // one — it keys on tenant and tier, and a scan carries neither.
    //
    // The client address comes from X-Forwarded-For, which the gateway appends
    // to. Absent (a direct call, or a misconfigured hop) the token key still
    // applies, so the limit degrades rather than disappearing.
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if !st.scan_limiter.check(&token, client_ip, now) {
        tracing::warn!(?client_ip, "scan rate limit exceeded");
        // 429, not the blanket 404 the other refusals use: a rate limit is a
        // "come back shortly", and telling a real diner's phone to retry is
        // strictly better than telling it the table does not exist. It leaks
        // nothing a prober did not already know by having been throttled.
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    let Some((table, venue)) = st.venues.find_table_by_token(&token).await.map_err(|e| {
        tracing::error!(err = %e, "table token lookup failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    else {
        // Deliberately not logged with the token: it is a credential, and a
        // scanner enumerating them would fill the log with the very values an
        // attacker would want read back.
        tracing::debug!("scan for an unknown table token");
        return Err(StatusCode::NOT_FOUND);
    };

    if let Err(reason) = orderable_now(&venue, &table, now) {
        tracing::info!(
            venue_id = %venue.id, table_id = %table.id, label = %table.label,
            ?reason,
            "scan refused — the table is not orderable right now",
        );
        return Err(StatusCode::NOT_FOUND);
    }

    // The cap is what stops one photographed code being an unbounded session
    // factory. Checked after `orderable_now` because a closed table should not
    // even be counted.
    let live = st
        .venues
        .count_live_sessions(table.id, now)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "live session count failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if live >= st.table_session_cap {
        tracing::warn!(
            venue_id = %venue.id, table_id = %table.id, live,
            cap = st.table_session_cap,
            "scan refused — this table already holds its cap of live sessions",
        );
        return Err(StatusCode::NOT_FOUND);
    }

    let session = TableSession {
        id: Uuid::new_v4(),
        table_id: table.id,
        venue_id: venue.id,
        tenant_id: venue.tenant_id,
        created_at: now,
        expires_at: now + Duration::minutes(st.table_session_mins),
        ended_at: None,
    };
    st.venues.create_session(&session).await.map_err(|e| {
        tracing::error!(err = %e, "table session create failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Minted only after the row exists. A token whose session was never
    // persisted would be a credential referring to nothing, and the cap that
    // bounds this endpoint counts rows.
    let claims = logisticos_auth::claims::Claims::for_table_session(
        session.id,
        venue.tenant_id,
        String::new(),
        String::new(),
        st.table_session_mins * 60,
    );
    let access_token = st.jwt.issue_access_token(claims).map_err(|e| {
        tracing::error!(err = %e, "table session token mint failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // A venue with no vendors linked is a working table with nothing to order
    // from. That is an operator mistake rather than a scan failure, so it is a
    // logged warning and an empty list -- refusing the scan would tell the
    // diner the table does not exist, which is worse and untrue.
    let vendors = st
        .venues
        .list_venue_vendors(venue.tenant_id, venue.id)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(err = %e, venue_id = %venue.id, "venue vendor list failed during scan");
            Vec::new()
        });
    if vendors.is_empty() {
        tracing::warn!(
            venue_id = %venue.id, table_id = %table.id,
            "scan succeeded but no vendors sell at this venue — nothing is orderable",
        );
    }

    Ok(Json(ScanResponse {
        session_id: session.id,
        access_token,
        expires_at: session.expires_at.to_rfc3339(),
        venue_id: venue.id,
        venue_name: venue.name,
        table_label: table.label,
        vendors: vendors
            .into_iter()
            .map(|(vendor_id, name)| VendorBrief { vendor_id, name })
            .collect(),
    }))
}

/// `GET /v1/omnideliv/venues/:venue_id/tables` — the print sheet.
///
/// Returns the scan URL for each table, which contains the live token. This is
/// the one place a token is ever handed out, and only to an operator who can
/// already manage the tenant's vendors.
async fn list_tables(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(venue_id): Path<Uuid>,
) -> Result<Json<Vec<TableRow>>, StatusCode> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err(StatusCode::FORBIDDEN);
    }
    let tables = st
        .venues
        .list_tables(claims.tenant_id, venue_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "table list failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(
        tables
            .into_iter()
            .map(|t| TableRow {
                scan_url: format!("{}/t/{}", st.table_scan_base_url.trim_end_matches('/'), t.token),
                table_id: t.id,
                label: t.label,
                status: t.status.as_str().to_string(),
                printed_at: t.printed_at.map(|d| d.to_rfc3339()),
            })
            .collect(),
    ))
}

/// `POST /v1/omnideliv/venues/tables/:table_id/rotate` — the answer to a leak.
///
/// Rotation is what makes a photographed code a five-minute problem instead of
/// an incident, so it is a button rather than a migration.
async fn rotate(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(table_id): Path<Uuid>,
) -> Result<Json<RotateResponse>, StatusCode> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err(StatusCode::FORBIDDEN);
    }
    let token = new_table_token();
    let ok = st
        .venues
        .rotate_token(claims.tenant_id, table_id, &token)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "table token rotation failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !ok {
        // Not this tenant's table, or no such table. 404 either way so a caller
        // cannot confirm a table id exists in someone else's venue.
        return Err(StatusCode::NOT_FOUND);
    }

    tracing::info!(%table_id, tenant_id = %claims.tenant_id, "table code rotated — the printed one is now dead");

    Ok(Json(RotateResponse {
        scan_url: format!("{}/t/{}", st.table_scan_base_url.trim_end_matches('/'), token),
        table_id,
    }))
}

/// `POST /v1/omnideliv/venues/tables/:table_id/printed` — the code is on paper.
///
/// The counterpart to rotation clearing `printed_at`. Together they answer the
/// one question the print sheet cannot otherwise settle: is the sticker on that
/// table the code the database will accept? An operator who rotates a leaked
/// code and forgets to reprint has a table that refuses every scan and no way
/// to see why.
async fn mark_printed(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(table_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    if !claims.has_permission(VENDORS_MANAGE) {
        return Err(StatusCode::FORBIDDEN);
    }
    let ok = st
        .venues
        .mark_printed(claims.tenant_id, table_id, Utc::now())
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "marking table printed failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !ok {
        // Same reasoning as rotate: 404 rather than confirming the id exists.
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
