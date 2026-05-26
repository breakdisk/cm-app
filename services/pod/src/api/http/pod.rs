use axum::{extract::{Path, Query, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_types::{DriverId, TenantId};
use crate::{
    api::http::AppState,
    application::commands::*,
    domain::entities::PopStatus,
};

#[derive(serde::Deserialize)]
pub struct PopStatusQuery {
    pub shipment_ids: Vec<Uuid>,
}

#[derive(serde::Deserialize)]
pub struct PodQuery {
    pub shipment_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
pub struct PopQuery {
    pub shipment_id: Option<Uuid>,
}

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

/// GET /v1/pods?shipment_id=<uuid> — fetch POD for the admin portal panel.
/// Returns the most recent POD for the shipment with presigned photo/signature URLs.
pub async fn list_pods(
    AuthClaims(_claims): AuthClaims,
    Query(q): Query<PodQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let shipment_id = q.shipment_id
        .ok_or_else(|| AppError::Validation("shipment_id query param required".into()))?;

    match state.pod_service.get_by_shipment(shipment_id).await? {
        Some(pod) => {
            let view = state.pod_service.pod_to_view(&pod).await;
            Ok(Json(serde_json::json!({ "data": [view] })))
        }
        None => Ok(Json(serde_json::json!({ "data": [] }))),
    }
}

pub async fn get_pod(
    AuthClaims(_claims): AuthClaims,
    Path(pod_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pod = state.pod_service.get_by_id(pod_id).await?;
    let view = state.pod_service.pod_to_view(&pod).await;
    Ok(Json(serde_json::json!({ "data": view })))
}

pub async fn generate_otp(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<GenerateOtpCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let (otp_id, code) = state.pod_service.generate_and_send_otp(&tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "otp_id": otp_id, "code": code } })))
}

pub async fn verify_otp(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<VerifyOtpCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let otp_id = state.pod_service.verify_otp_standalone(claims.tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "otp_id": otp_id, "verified": true } })))
}

// ── Proof of Pickup (POP) handlers ────────────────────────────────────────────

/// POST /v1/pops — driver initiates a Proof of Pickup at the merchant/hub.
///
/// Body includes both the capture GPS (`capture_lat`, `capture_lng`) and the
/// pickup address GPS (`pickup_lat`, `pickup_lng`). The service computes the
/// geofence and OUT_OF_BOUNDS_HANDOVER flag from the distance between them.
pub async fn initiate_pickup(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<InitiatePickupCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let pop = state.pod_service
        .initiate_pickup(&driver_id, &tenant_id, cmd)
        .await?;

    Ok(Json(serde_json::json!({
        "data": {
            "pop_id":                pop.id,
            "geofence_verified":     pop.geofence_verified,
            "out_of_bounds_handover": pop.out_of_bounds_handover,
            "status":                "draft"
        }
    })))
}

/// PUT /v1/pops/:id/submit — driver finalises a Proof of Pickup.
///
/// Validates the barcode scan, records actual weight (Track B), registers
/// any uploaded parcel photo, and publishes the `pickup.captured` Kafka event.
pub async fn submit_pickup(
    AuthClaims(claims): AuthClaims,
    Path(pop_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<SubmitPickupCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let driver_id   = DriverId::from_uuid(claims.user_id);
    let tenant_id   = TenantId::from_uuid(claims.tenant_id);
    let tenant_code = cmd.tenant_code.clone();
    let cmd = SubmitPickupCommand { pop_id, ..cmd };

    let pop_id = state.pod_service
        .submit_pickup(&driver_id, &tenant_id, cmd, tenant_code)
        .await?;

    Ok(Json(serde_json::json!({
        "data": { "pop_id": pop_id, "status": "submitted" }
    })))
}

/// POST /v1/pops/:id/upload-url — driver requests a presigned R2 PUT URL for
/// a parcel photo captured at pickup. The s3_key returned here is passed in
/// `SubmitPickupCommand.photo_s3_key` — no separate attach step needed.
pub async fn get_pop_upload_url(
    AuthClaims(claims): AuthClaims,
    Path(pop_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let content_type = body["content_type"].as_str()
        .ok_or_else(|| AppError::Validation("content_type required".into()))?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let result = state.pod_service.get_pop_upload_url(pop_id, &tenant_id, content_type).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}

/// GET /v1/pops/:id — fetch a single POP record (ops / admin portal).
/// Returns a presigned `photo_url` (1-hour TTL) instead of the raw S3 key.
pub async fn get_pop(
    AuthClaims(_claims): AuthClaims,
    Path(pop_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pop = state.pod_service.get_pop_by_id(pop_id).await?;
    let view = state.pod_service.pop_to_view(&pop).await;
    Ok(Json(serde_json::json!({ "data": view })))
}

/// GET /v1/pops?shipment_id=<uuid> — fetch POP for the admin portal panel.
/// Returns the most recent submitted POP for the shipment with a presigned photo URL.
pub async fn list_pops(
    AuthClaims(_claims): AuthClaims,
    Query(q): Query<PopQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let shipment_id = q.shipment_id
        .ok_or_else(|| AppError::Validation("shipment_id query parameter is required".into()))?;

    match state.pod_service.get_pop_by_shipment(shipment_id).await? {
        Some(pop) => {
            let view = state.pod_service.pop_to_view(&pop).await;
            Ok(Json(serde_json::json!({ "data": view })))
        }
        None => Ok(Json(serde_json::json!({ "data": [] }))),
    }
}

/// `GET /v1/internal/pop-status?shipment_ids=<uuid>&shipment_ids=<uuid>...`
///
/// Internal service-to-service endpoint (no JWT — protected by Istio mTLS).
/// Called by hub-ops before loading a pallet or loose piece into a container.
///
/// Returns per-shipment POP completion status. A POP is "completed" when its
/// status is `Submitted`.
pub async fn pop_status_internal(
    Query(q): Query<PopStatusQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut statuses = Vec::with_capacity(q.shipment_ids.len());
    let mut all_completed = true;

    for shipment_id in &q.shipment_ids {
        let pop = state.pod_service.get_pop_by_shipment(*shipment_id).await?;
        let has_completed_pop = pop.map_or(false, |p| p.status == PopStatus::Submitted);
        if !has_completed_pop {
            all_completed = false;
        }
        statuses.push(serde_json::json!({
            "shipment_id":       shipment_id,
            "has_completed_pop": has_completed_pop,
        }));
    }

    Ok(Json(serde_json::json!({
        "statuses":      statuses,
        "all_completed": all_completed,
    })))
}
