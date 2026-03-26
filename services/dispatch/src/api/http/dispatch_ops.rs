use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_types::TenantId;
use crate::{api::http::AppState, application::commands::QuickDispatchCommand};

pub async fn quick_dispatch(
    AuthClaims(claims): AuthClaims,
    Path(shipment_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_ASSIGN);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let preferred_driver_id = body.get("preferred_driver_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Uuid>().ok());

    let cmd = QuickDispatchCommand { shipment_id, preferred_driver_id };
    let assignment = state.dispatch_service.quick_dispatch(tenant_id, cmd).await?;

    Ok(Json(serde_json::json!({
        "data": {
            "assignment_id": assignment.id,
            "driver_id": assignment.driver_id.inner(),
            "status": "pending"
        }
    })))
}
