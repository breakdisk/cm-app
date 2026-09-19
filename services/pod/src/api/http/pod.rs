use axum::{extract::{Path, Query, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_types::{DriverId, TenantId};
use logisticos_types::awb::TenantCode;
use crate::{
    api::http::AppState,
    application::commands::*,
    domain::value_objects::delivery_pin::IssuedPin,
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

#[derive(Debug, serde::Deserialize)]
pub struct SurveyPhotoUploadRequest {
    pub shipment_id: Uuid,
    pub content_type: String,
}

/// `POST /v1/pod/survey-photos/upload-url` — a lead's whole-home survey photo.
/// The key names the tenant and shipment; order-intake records it against the
/// move only for a lead on that move.
pub async fn survey_photo_upload_url(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SurveyPhotoUploadRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let result = state.pod_service.survey_upload_url(&tenant_id, body.shipment_id, &body.content_type).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}

#[derive(Debug, serde::Deserialize)]
pub struct MediaUrlsRequest {
    pub keys: Vec<String>,
}

/// Internal: `POST /v1/internal/media/urls` — viewable URLs for survey photos.
pub async fn internal_media_urls(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MediaUrlsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let urls = state.pod_service.survey_media_urls(&body.keys).await?;
    Ok(Json(serde_json::json!({ "data": urls })))
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
    // Derive the invoice tenant code from the JWT when the client omits it.
    //
    // The driver app cannot supply this: it holds a tenant UUID and slug, never
    // the 3-char code. It therefore always arrived empty, and payments'
    // pod_consumer fell back to a hardcoded "PH1" — numbering every tenant's
    // customer-booked receipts under PH1's sequence. Deriving it here from the
    // same slug order-intake uses for the AWB keeps a tenant's tracking numbers
    // and its invoice numbers on the same code.
    let cmd = SubmitPodCommand {
        pod_id,
        tenant_code: if cmd.tenant_code.is_empty() {
            TenantCode::from_slug(&claims.tenant_slug)
                .map(|c| c.to_string())
                .unwrap_or_default()
        } else {
            cmd.tenant_code
        },
        ..cmd
    };
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

/// Whether the caller may see the OTP code in the response body.
///
/// A driver must not: the PIN exists precisely so that closing the job requires
/// the recipient. A driver may still call the endpoint to trigger a resend — they
/// just get `{ otp_id, sent }` with no code.
///
/// The driver check is evaluated first and wins outright. If it were merely one
/// entry in a deny-list weighed against the allow-list, granting a driver any
/// other role would hand them the code.
fn code_is_visible_to(roles: &[String]) -> bool {
    let driverish = roles.iter().any(|r| r == "driver" || r == "courier");
    if driverish {
        return false;
    }
    roles
        .iter()
        .any(|r| matches!(r.as_str(), "admin" | "tenant_admin" | "customer" | "support"))
}

/// `POST /v1/otps/generate` — issue (or re-issue) the recipient's delivery PIN.
///
/// Two things used to be wrong here, and together they made the PIN decorative:
/// the handler ran no permission or ownership check, and it took
/// `recipient_phone` from the request body then returned the code in the
/// response. A driver holding a tenant JWT could point the SMS at their own
/// handset, read the code out of the 200, and verify it — closing the job with
/// no recipient involved, which is exactly what the PIN is supposed to prevent.
///
/// The phone now comes from the shipment record and the body's value is ignored.
///
/// A live PIN is kept unless the body says `reissue: true`, and the response then
/// carries `active: true` with no code: the PIN is already with the recipient.
pub async fn generate_otp(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<GenerateOtpCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let recipient_phone = state
        .pod_service
        .recipient_phone_for_shipment(cmd.shipment_id)
        .await?;

    let issued = state
        .pod_service
        .generate_and_send_otp(
            &tenant_id,
            GenerateOtpCommand { shipment_id: cmd.shipment_id, recipient_phone, reissue: cmd.reissue },
        )
        .await?;

    let data = match issued {
        IssuedPin::KeptLive { otp_id } => serde_json::json!({ "otp_id": otp_id, "active": true, "sent": false }),
        IssuedPin::New { otp_id, code } if code_is_visible_to(&claims.roles) => {
            serde_json::json!({ "otp_id": otp_id, "code": code })
        }
        IssuedPin::New { otp_id, .. } => serde_json::json!({ "otp_id": otp_id, "sent": true }),
    };
    Ok(Json(serde_json::json!({ "data": data })))
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
    // Same derivation as `submit` — the driver app has no 3-char code to send, so
    // without this the Track A/B invoice raised from PickupCaptured is numbered
    // under whatever fallback the payments consumer applies.
    let tenant_code = if cmd.tenant_code.is_empty() {
        TenantCode::from_slug(&claims.tenant_slug)
            .map(|c| c.to_string())
            .unwrap_or_default()
    } else {
        cmd.tenant_code.clone()
    };
    let cmd = SubmitPickupCommand { pop_id, tenant_code: tenant_code.clone(), ..cmd };

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

/// `GET /v1/internal/pop-evidence/:shipment_id`
///
/// Internal service-to-service endpoint (no JWT — protected by Istio mTLS).
/// Called by the delivery-experience service to enrich tracking responses with
/// POP pickup evidence and POD delivery evidence for merchant/customer portals.
///
/// Returns:
/// ```json
/// {
///   "pop": { "photo_url": "https://...", "picked_up_at": "...", ... } | null,
///   "pod": { "photo_urls": [...], "signature_url": "...", "delivered_at": "...", ... } | null
/// }
/// ```
pub async fn get_evidence_internal(
    Path(shipment_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Fetch the submitted POP for this shipment (if any).
    let pop_view = match state.pod_service.get_completed_pop_by_shipment(shipment_id).await? {
        Some(ref pop) => Some(state.pod_service.pop_to_view(pop).await),
        None          => None,
    };

    // Fetch the POD for this shipment (if any). Use all-photos evidence view.
    let pod_view = match state.pod_service.get_by_shipment(shipment_id).await? {
        Some(ref pod) => Some(state.pod_service.pod_evidence_to_view(pod).await),
        None          => None,
    };

    Ok(Json(serde_json::json!({
        "pop": pop_view,
        "pod": pod_view,
    })))
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
        let pop = state.pod_service.get_completed_pop_by_shipment(*shipment_id).await?;
        let has_completed_pop = pop.is_some();
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

#[cfg(test)]
mod otp_authority_tests {
    use super::*;

    /// The whole point of the PIN is that the driver does not hold it. A driver
    /// asking for one must not receive the code in the response -- they may
    /// trigger a resend to the recipient, nothing more.
    #[test]
    fn a_driver_may_trigger_a_resend_but_never_read_the_code() {
        assert!(!code_is_visible_to(&["driver".to_string()]));
        assert!(!code_is_visible_to(&["courier".to_string()]));
    }

    /// A driver who also holds another role is still a driver standing at the
    /// door. The driverish check has to win over any grant, or the bypass is
    /// simply one role assignment away.
    #[test]
    fn a_driver_with_an_extra_role_still_cannot_read_the_code() {
        assert!(!code_is_visible_to(&["driver".to_string(), "admin".to_string()]));
    }

    /// Ops needs it for the phone-support case where the recipient never got
    /// the SMS.
    #[test]
    fn ops_may_read_the_code() {
        assert!(code_is_visible_to(&["admin".to_string()]));
        assert!(code_is_visible_to(&["tenant_admin".to_string()]));
    }

    /// The recipient's own app shows it on the tracking screen.
    #[test]
    fn the_customer_may_read_their_own_code() {
        assert!(code_is_visible_to(&["customer".to_string()]));
    }

    /// An unrecognised role is not trusted with it.
    #[test]
    fn an_unknown_role_cannot_read_the_code() {
        assert!(!code_is_visible_to(&["hub_scanner".to_string()]));
        assert!(!code_is_visible_to(&[]));
    }
}
