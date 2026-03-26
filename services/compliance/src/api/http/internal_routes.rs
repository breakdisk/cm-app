use axum::{extract::{Path, Query, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use serde::Deserialize;
use logisticos_errors::AppError;
use crate::api::http::AppState;

// Security: This endpoint has no JWT authentication by design.
// It is protected at the network layer by Istio mTLS — only services
// within the mesh with a valid certificate (e.g., the dispatch service)
// can reach this route. The Istio AuthorizationPolicy for this service
// restricts callers to: dispatch, driver-ops, carrier.
// DO NOT expose this route on the external ingress / API gateway.

#[derive(Deserialize)]
pub struct StatusQuery { pub tenant_id: Uuid }

pub async fn get_status(
    Path((entity_type, entity_id)): Path<(String, Uuid)>,
    Query(query): Query<StatusQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let profile = state.compliance.profiles
        .find_by_entity(query.tenant_id, &entity_type, entity_id)
        .await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: entity_id.to_string() })?;

    Ok(Json(serde_json::json!({
        "data": {
            "entity_id": entity_id,
            "entity_type": entity_type,
            "status": profile.overall_status,
            "is_assignable": profile.overall_status.is_assignable(),
        }
    })))
}
