use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::{TenantId, RouteId};
use crate::{
    api::http::AppState,
    application::commands::CreateRouteCommand,
};

pub async fn list_routes(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_VIEW);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let routes = state.dispatch_service.list_routes(&tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": routes })))
}

pub async fn get_route(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_VIEW);
    let route_id = RouteId::from_uuid(id);
    let route = state.dispatch_service.get_route(&route_id).await?;

    // Tenant isolation — verify caller's tenant owns this route
    if route.tenant_id.inner() != claims.tenant_id {
        return Err(AppError::NotFound { resource: "Route", id: id.to_string() });
    }

    Ok(Json(serde_json::json!({ "data": route })))
}

/// PUT /v1/routes/:id/cancel
///
/// Cancels a `planned` or `in_progress` route owned by the caller's tenant.
/// A completed route cannot be cancelled. The driver is freed from the route
/// so auto-dispatch can assign them to a new job.
pub async fn cancel_route(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_ASSIGN);
    let route_id  = RouteId::from_uuid(id);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let cancelled = state.dispatch_service.cancel_route(&route_id, &tenant_id).await?;
    if !cancelled {
        return Err(AppError::NotFound { resource: "Route", id: id.to_string() });
    }

    Ok(Json(serde_json::json!({
        "data": { "route_id": id, "status": "cancelled" }
    })))
}

pub async fn create_route(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<CreateRouteCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_ASSIGN);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let route = state.dispatch_service.create_route(tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "route_id": route.id, "status": "planned" } })))
}
