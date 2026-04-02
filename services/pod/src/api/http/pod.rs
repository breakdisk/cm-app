use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_types::{DriverId, TenantId};
use crate::{
    api::http::AppState,
    application::commands::*,
};

pub async fn initiate(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cmd: InitiatePodCommand = serde_json::from_value(body.clone())
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let delivery_lat = body["delivery_lat"].as_f64()
        .ok_or_else(|| AppError::Validation("delivery_lat required".into()))?;
    let delivery_lng = body["delivery_lng"].as_f64()
        .ok_or_else(|| AppError::Validation("delivery_lng required".into()))?;

    let driver_id = DriverId::from_uuid(claims.user_id);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let pod = state.pod_service
        .initiate(&driver_id, &tenant_id, cmd, delivery_lat, delivery_lng)
        .await?;

    Ok(Json(serde_json::json!({
        "data": {
            "pod_id": pod.id,
            "geofence_verified": pod.geofence_verified,
            "status": "draft"
        }
    })))
}

pub async fn attach_signature(
    AuthClaims(_claims): AuthClaims,
    Path(pod_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<axum::http::StatusCode, AppError> {
    let signature_data = body["signature_data"].as_str()
        .ok_or_else(|| AppError::Validation("signature_data required".into()))?
        .to_string();

    state.pod_service.attach_signature(AttachSignatureCommand { pod_id, signature_data }).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn get_upload_url(
    AuthClaims(claims): AuthClaims,
    Path(pod_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let content_type = body["content_type"].as_str()
        .ok_or_else(|| AppError::Validation("content_type required".into()))?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let result = state.pod_service.get_upload_url(pod_id, &tenant_id, content_type).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}

pub async fn attach_photo(
    AuthClaims(_claims): AuthClaims,
    Path(pod_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<AttachPhotoCommand>,
) -> Result<axum::http::StatusCode, AppError> {
    let cmd = AttachPhotoCommand { pod_id, ..cmd };
    state.pod_service.attach_photo(cmd).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn submit(
    AuthClaims(claims): AuthClaims,
    Path(pod_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<SubmitPodCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let cmd = SubmitPodCommand { pod_id, ..cmd };
    let pod_id = state.pod_service.submit(&driver_id, &tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "pod_id": pod_id, "status": "submitted" } })))
}

pub async fn get_pod(
    AuthClaims(_claims): AuthClaims,
    Path(pod_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pod = state.pod_service.get_by_id(pod_id).await?;
    Ok(Json(serde_json::json!({ "data": pod })))
}

pub async fn generate_otp(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<GenerateOtpCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let otp_id = state.pod_service.generate_and_send_otp(&tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "otp_id": otp_id } })))
}

pub async fn verify_otp(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<VerifyOtpCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let otp_id = state.pod_service.verify_otp_standalone(claims.tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "otp_id": otp_id, "verified": true } })))
}
