use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
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
    // Derive 3-char AWB tenant code from the tenant slug (first 3 alphanumeric chars, uppercased)
    if cmd.tenant_code.is_empty() {
        cmd.tenant_code = claims.tenant_slug
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() && *c != 'O' && *c != 'I')
            .take(3)
            .collect::<String>()
            .to_uppercase();
    }
    // Mark as customer-booked when the JWT role is "customer" (customer app self-booking).
    if claims.roles.contains(&"customer".to_string()) {
        cmd.booked_by_customer = true;
    }
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
        // Internal service-to-service endpoints — no JWT auth required (Istio mTLS enforces caller identity).
        .nest("/v1/internal", Router::new()
            .route("/shipments/:id/billing", get(get_shipment_billing))
        )
        .with_state(state)
}

async fn get_shipment_billing(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let shipment = s.query.get_by_id(id).await?;

    let base_freight   = shipment.compute_base_fee();
    let fuel_surcharge = (base_freight.amount as f64 * 0.05).round() as i64; // 5% fuel levy
    let insurance      = shipment.declared_value
        .map(|v| (v.amount as f64 * 0.005).round() as i64) // 0.5% of declared value
        .unwrap_or(0);
    let total = base_freight.amount + fuel_surcharge + insurance;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "shipment_id":          id,
        "awb":                  shipment.awb.as_str(),
        "merchant_id":          shipment.merchant_id.inner(),
        "currency":             format!("{:?}", base_freight.currency),
        "base_freight":         base_freight.amount,
        "fuel_surcharge":       fuel_surcharge,
        "insurance":            insurance,
        "total":                total,
    }))))
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
