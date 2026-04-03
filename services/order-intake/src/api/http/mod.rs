use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
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

use crate::application::{
    commands::{BulkCreateShipmentCommand, CancelShipmentCommand, CreateShipmentCommand},
    queries::ShipmentQueryService,
    services::shipment_service::ShipmentService,
};

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub svc:    Arc<ShipmentService>,
    pub query:  Arc<ShipmentQueryService>,
    pub jwt:    Arc<logisticos_auth::jwt::JwtService>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn create_shipment(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(mut cmd): Json<CreateShipmentCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_CREATE)?;
    cmd.tenant_id   = claims.tenant_id;
    cmd.merchant_id = claims.user_id; // merchant uses their user UUID as merchant_id
    let shipment = s.svc.create(cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(shipment)))
}

async fn bulk_create_shipments(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(mut cmd): Json<BulkCreateShipmentCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_CREATE)?;
    cmd.tenant_id   = claims.tenant_id;
    cmd.merchant_id = claims.user_id;
    let result = s.svc.bulk_create(cmd).await?;
    Ok::<_, AppError>((StatusCode::MULTI_STATUS, Json(result)))
}

async fn get_shipment(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let shipment = s.query.get_by_id(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(shipment)))
}

async fn cancel_shipment(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(mut cmd): Json<CancelShipmentCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    cmd.shipment_id = id;
    s.svc.cancel(cmd).await?;
    Ok::<_, AppError>((StatusCode::NO_CONTENT, ()))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router(state: AppState) -> Router {
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );
    Router::new()
        .route("/health", get(|| async { axum::Json(serde_json::json!({"status":"ok","service":"order-intake"})) }))
        .nest("/v1", Router::new()
            .route("/shipments",        post(create_shipment).get(list_shipments))
            .route("/shipments/bulk",   post(bulk_create_shipments))
            .route("/shipments/:id",    get(get_shipment))
            .route("/shipments/:id/cancel", post(cancel_shipment))
            .layer(auth_layer)
        )
        .with_state(state)
}

async fn list_shipments(
    State(s): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<crate::application::queries::ListShipmentsQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let (shipments, total) = s.query.list(claims.tenant_id, q).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "shipments": shipments,
        "total": total,
    }))))
}
