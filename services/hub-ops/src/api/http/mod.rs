//! HTTP API layer for the Hub Operations service.
//!
//! Exposes REST endpoints for hub CRUD, capacity queries, hub manifests, and
//! the parcel induction lifecycle (induct → sort → dispatch).
//!
//! Routes
//! ──────
//!   GET  /v1/hubs                        — list hubs for tenant
//!   POST /v1/hubs                        — create a new hub
//!   GET  /v1/hubs/:hub_id                — get hub detail
//!   GET  /v1/hubs/:hub_id/capacity       — current load vs. capacity summary
//!   GET  /v1/hubs/:hub_id/manifest       — active parcels currently in hub
//!   POST /v1/inductions                  — induct a parcel into a hub
//!   GET  /v1/inductions/:id              — get an induction record by UUID
//!   POST /v1/inductions/:id/sort         — sort parcel to zone/bay
//!   POST /v1/inductions/:id/dispatch     — dispatch parcel out of hub
//!   GET  /health | /ready | /metrics     — observability

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;
use logisticos_types::TenantId;

use crate::application::services::{
    CreateHubCommand, HubService, InductParcelCommand, SortParcelCommand,
};

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub hub_svc: Arc<HubService>,
}

// ---------------------------------------------------------------------------
// Hub handlers
// ---------------------------------------------------------------------------

/// `GET /v1/hubs` — list all active hubs for the authenticated tenant.
async fn list_hubs(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_READ)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hubs = state.hub_svc.list_hubs(&tenant_id).await?;

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "hubs":  hubs,
            "count": hubs.len(),
        })),
    ))
}

/// `POST /v1/hubs` — create a new hub facility.
async fn create_hub(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<CreateHubCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hub = state.hub_svc.create_hub(&tenant_id, cmd).await?;

    Ok::<_, AppError>((StatusCode::CREATED, Json(hub)))
}

/// `GET /v1/hubs/:hub_id` — fetch a single hub record.
async fn get_hub(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(hub_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_READ)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hubs = state.hub_svc.list_hubs(&tenant_id).await?;

    let hub = hubs
        .into_iter()
        .find(|h| h.id.inner() == hub_id)
        .ok_or_else(|| AppError::NotFound { resource: "hub", id: hub_id.to_string() })?;

    Ok::<_, AppError>((StatusCode::OK, Json(hub)))
}

/// `GET /v1/hubs/:hub_id/capacity`
///
/// Returns a lightweight capacity summary without the full hub document.
/// Includes current load, max capacity, and utilisation percentage.
async fn hub_capacity(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(hub_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_READ)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hubs = state.hub_svc.list_hubs(&tenant_id).await?;

    let hub = hubs
        .into_iter()
        .find(|h| h.id.inner() == hub_id)
        .ok_or_else(|| AppError::NotFound { resource: "hub", id: hub_id.to_string() })?;

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "hub_id":           hub_id,
            "hub_name":         hub.name,
            "capacity":         hub.capacity,
            "current_load":     hub.current_load,
            "capacity_pct":     hub.capacity_pct(),
            "is_over_capacity": hub.is_over_capacity(),
        })),
    ))
}

/// `GET /v1/hubs/:hub_id/manifest`
///
/// Returns all parcels currently active in the hub (status inducted or sorted),
/// after verifying the hub belongs to the caller's tenant.
async fn hub_manifest(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(hub_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_READ)?;

    // Verify hub belongs to this tenant before returning manifest data.
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hubs = state.hub_svc.list_hubs(&tenant_id).await?;

    if !hubs.iter().any(|h| h.id.inner() == hub_id) {
        return Err(AppError::NotFound { resource: "hub", id: hub_id.to_string() });
    }

    let parcels = state.hub_svc.hub_manifest(hub_id).await?;

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "hub_id":  hub_id,
            "parcels": parcels,
            "count":   parcels.len(),
        })),
    ))
}

// ---------------------------------------------------------------------------
// Induction handlers
// ---------------------------------------------------------------------------

/// `POST /v1/inductions`
///
/// Registers a parcel arriving at a hub. Idempotent: if the same
/// `(shipment_id, hub_id)` pair has already been inducted, the existing
/// record is returned unchanged.
async fn create_induction(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<InductParcelCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    // Guard: the target hub must belong to the caller's tenant.
    // HubService.induct_parcel enforces capacity rules but not tenant isolation.
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hubs = state.hub_svc.list_hubs(&tenant_id).await?;

    if !hubs.iter().any(|h| h.id.inner() == cmd.hub_id) {
        return Err(AppError::NotFound { resource: "hub", id: cmd.hub_id.to_string() });
    }

    let induction = state.hub_svc.induct_parcel(cmd).await?;

    Ok::<_, AppError>((StatusCode::CREATED, Json(induction)))
}

/// `GET /v1/inductions/:id` — fetch a single induction record by UUID.
///
/// Scans active manifests across all tenant hubs to locate the record.  A
/// production optimisation would add a `find_induction_by_id` query on the
/// service, but this approach keeps the API layer decoupled from repository
/// internals.
async fn get_induction(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hubs = state.hub_svc.list_hubs(&tenant_id).await?;

    for hub in &hubs {
        let parcels = state.hub_svc.hub_manifest(hub.id.inner()).await?;
        if let Some(induction) = parcels.into_iter().find(|p| p.id.inner() == id) {
            return Ok::<_, AppError>((StatusCode::OK, Json(induction)));
        }
    }

    Err(AppError::NotFound { resource: "induction", id: id.to_string() })
}

/// `POST /v1/inductions/:id/sort`
///
/// Assigns a delivery zone and physical bay slot to an inducted parcel.
#[derive(Debug, Deserialize)]
struct SortRequest {
    zone: String,
    bay:  String,
}

async fn sort_induction(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(req): Json<SortRequest>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    if req.zone.trim().is_empty() || req.bay.trim().is_empty() {
        return Err(AppError::Validation("zone and bay must not be empty".into()));
    }

    let cmd = SortParcelCommand {
        induction_id: id,
        zone: req.zone,
        bay:  req.bay,
    };

    let induction = state.hub_svc.sort_parcel(cmd).await?;

    Ok::<_, AppError>((StatusCode::OK, Json(induction)))
}

/// `POST /v1/inductions/:id/dispatch`
///
/// Marks a sorted parcel as dispatched on an outbound route, decrements the
/// hub's current load counter, and records the dispatch timestamp.
async fn dispatch_induction(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    let induction = state.hub_svc.dispatch_parcel(id).await?;

    Ok::<_, AppError>((StatusCode::OK, Json(induction)))
}

// ---------------------------------------------------------------------------
// Observability handlers
// ---------------------------------------------------------------------------

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "hub-ops" }))
}

async fn ready() -> Json<serde_json::Value> {
    // Production: verify DB pool and any critical external deps here.
    Json(serde_json::json!({ "status": "ready" }))
}

async fn metrics() -> &'static str {
    // Production: use `metrics-exporter-prometheus` crate, return `handle.render()`.
    "# HELP hub_ops_parcels_inducted_total Total parcels inducted\n\
     # TYPE hub_ops_parcels_inducted_total counter\n\
     hub_ops_parcels_inducted_total 0\n\
     # HELP hub_ops_parcels_dispatched_total Total parcels dispatched\n\
     # TYPE hub_ops_parcels_dispatched_total counter\n\
     hub_ops_parcels_dispatched_total 0\n"
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Builds the Axum router for the hub-ops service.
/// Pass the fully-constructed `AppState` during bootstrap.
pub fn router(state: AppState) -> Router {
    Router::new()
        // ── Hubs ────────────────────────────────────────────────────
        .route("/v1/hubs",                    get(list_hubs).post(create_hub))
        .route("/v1/hubs/:hub_id",            get(get_hub))
        .route("/v1/hubs/:hub_id/capacity",   get(hub_capacity))
        .route("/v1/hubs/:hub_id/manifest",   get(hub_manifest))
        // ── Inductions ──────────────────────────────────────────────
        // Note: the spec lists `/v1/inductionss` (double-s) which is a typo;
        // canonical path is `/v1/inductions`.
        .route("/v1/inductions",              post(create_induction))
        .route("/v1/inductions/:id",          get(get_induction))
        .route("/v1/inductions/:id/sort",     post(sort_induction))
        .route("/v1/inductions/:id/dispatch", post(dispatch_induction))
        // ── Observability ───────────────────────────────────────────
        .route("/health",                     get(health))
        .route("/ready",                      get(ready))
        .route("/metrics",                    get(metrics))
        .with_state(state)
}
