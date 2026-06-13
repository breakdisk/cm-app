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
use logisticos_types::TenantId;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Public — no auth required
        .route("/track/:tracking_number",                          get(public_track))
        .route("/v1/tracking/public/:tracking_number",             get(public_track))
        .route("/track/:tracking_number/reschedule",               post(reschedule_delivery))
        .route("/v1/tracking/:tracking_number/reschedule",         post(reschedule_delivery))
        .route("/track/:tracking_number/feedback",                 post(submit_feedback))
        .route("/v1/tracking/:tracking_number/feedback",           post(submit_feedback))
        .route("/v1/tracking/:tracking_number/confirm-receipt",    post(confirm_receipt))
        .route("/v1/tracking/:tracking_number/send-receipt",       post(send_receipt))
        // Authenticated — merchant/ops
        .route("/v1/tracking/:shipment_id",                        get(get_by_shipment_id))
        .route("/v1/tracking",                                     get(list_shipments))
}

// ---------------------------------------------------------------------------
// GET /track/:tracking_number — public customer tracking page
// ---------------------------------------------------------------------------

async fn public_track(
    State(state): State<AppState>,
    Path(tracking_number): Path<String>,
) -> impl IntoResponse {
    use crate::domain::entities::TrackingStatus;

    match state.tracking_svc.get_public(&tracking_number).await {
        Ok(record) => {
            // Optional POP/POD photos from the pod service. Best-effort: a miss
            // degrades to absent evidence, never a 500.
            let evidence = state.pod_client.get_evidence(record.shipment_id).await;

            // Timeline: the customer tracking portal reads `events[]` with
            // { status, description, location, occurred_at } — the StatusEvent
            // shape maps 1:1.
            let events: Vec<serde_json::Value> = record.status_history.iter().map(|e| {
                serde_json::json!({
                    "status":      e.status,
                    "description": e.description,
                    "location":    e.location,
                    "occurred_at": e.occurred_at,
                })
            }).collect();

            // Driver block — surfaced only while the parcel is in motion.
            let show_driver = matches!(
                record.current_status,
                TrackingStatus::OutForDelivery
                    | TrackingStatus::OutForPickup
                    | TrackingStatus::AssignedToDriver
            );
            let driver = record.driver_name.as_ref().filter(|_| show_driver).map(|name| {
                serde_json::json!({
                    "name": name,
                    "lat":  record.driver_position.as_ref().map(|p| p.lat),
                    "lng":  record.driver_position.as_ref().map(|p| p.lng),
                })
            });

            // Pickup evidence photo (if the pod service returned one).
            let pop = evidence.as_ref()
                .map(|ev| ev.get("pop").cloned().unwrap_or(serde_json::Value::Null))
                .filter(|v| !v.is_null());

            // Delivery evidence — only once delivered. delivered_at + recipient
            // come from the authoritative tracking record; photos from pod.
            let pod = if record.current_status == TrackingStatus::Delivered {
                let photo_urls = evidence.as_ref()
                    .and_then(|ev| ev.get("pod").and_then(|p| p.get("photo_urls")).cloned())
                    .unwrap_or_else(|| serde_json::json!([]));
                Some(serde_json::json!({
                    "delivered_at":   record.delivered_at,
                    "recipient_name": record.recipient_name,
                    "photo_urls":     photo_urls,
                }))
            } else {
                None
            };

            // Driver position block kept for the React-Native customer app
            // (LiveDriverMap reads `driver_location`); same visibility rule.
            let driver_location = if show_driver {
                record.driver_position.as_ref().map(|p| serde_json::json!({"lat": p.lat, "lng": p.lng}))
            } else {
                None
            };

            // Backward-compatible superset. The customer **portal** (web) reads
            // the new keys (`origin_city`, `destination_city`, `events`, `driver`,
            // `eta`, `pop`, `pod`); the customer **app** (React Native) reads the
            // legacy keys (`origin`, `destination`, `history`, `driver_location`).
            // Both unwrap the `data` envelope. Emitting both keeps every client
            // working without a coordinated deploy.
            let data = serde_json::json!({
                "tracking_number":    record.tracking_number,
                "status":             record.current_status,
                "status_label":       record.current_status.display_label(),
                // New (web portal)
                "origin_city":        record.origin_address,
                "destination_city":   record.destination_address,
                "eta":                record.estimated_delivery,
                "events":             events,
                "driver":             driver,
                "pop":                pop,
                "pod":                pod,
                // Legacy (mobile app + previous response shape)
                "origin":             record.origin_address,
                "destination":        record.destination_address,
                "estimated_delivery": record.estimated_delivery,
                "delivered_at":       record.delivered_at,
                "attempt_number":     record.attempt_number,
                "next_attempt_at":    record.next_attempt_at,
                "reschedule_count":   record.reschedule_count,
                "recipient_name":     record.recipient_name,
                "history":            record.status_history,
                "driver_location":    driver_location,
            });
            (StatusCode::OK, Json(serde_json::json!({ "data": data }))).into_response()
        }
        Err(AppError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Tracking number not found"})),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /v1/tracking/:shipment_id — authenticated detailed view
// ---------------------------------------------------------------------------

async fn get_by_shipment_id(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(shipment_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;

    let record = state.tracking_svc.get_by_shipment_id(shipment_id).await?;

    // Tenants can only see their own shipments.
    if record.tenant_id != TenantId::from_uuid(claims.tenant_id) {
        return Err(AppError::Forbidden { resource: "shipment".to_owned() });
    }

    // Fetch POP + POD evidence from the pod service (non-blocking; optional).
    // Failure is logged by PodClient and surfaced as None so the tracking
    // response degrades gracefully instead of returning 500.
    let evidence = state.pod_client.get_evidence(record.shipment_id).await;

    // Build the enriched response: start with all TrackingRecord fields, then
    // inject the nested `pop` and `pod` evidence objects the merchant portal
    // expects. Also alias `status_history` → `events` for portal compatibility.
    let mut body = serde_json::to_value(&record)
        .map_err(|e| AppError::Internal(e.into()))?;

    if let Some(ev) = evidence {
        body["pop"] = ev["pop"].clone();
        body["pod"] = ev["pod"].clone();
    }
    // `events` is an alias for `status_history` used by merchant portal timeline.
    body["events"] = body["status_history"].clone();

    Ok::<_, AppError>((StatusCode::OK, Json(body)))
}

// ---------------------------------------------------------------------------
// GET /v1/tracking?limit=&offset=
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit:  Option<i64>,
    pub offset: Option<i64>,
}

async fn list_shipments(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;

    let records = state
        .tracking_svc
        .list(&TenantId::from_uuid(claims.tenant_id), q.limit.unwrap_or(50), q.offset.unwrap_or(0))
        .await?;

    let count = records.len();
    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({"shipments": records, "count": count})),
    ))
}

// ---------------------------------------------------------------------------
// POST /track/:tracking_number/reschedule
// POST /v1/tracking/:tracking_number/reschedule
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct RescheduleBody {
    preferred_date: chrono::NaiveDate,
    #[serde(default)]
    preferred_time_slot: Option<String>, // "morning" | "afternoon" | "anytime"
    #[serde(default)]
    reason: Option<String>,
}

async fn reschedule_delivery(
    State(state): State<AppState>,
    Path(tracking_number): Path<String>,
    Json(body): Json<RescheduleBody>,
) -> impl IntoResponse {
    let reason_str = body.reason.as_deref().unwrap_or("Customer reschedule request");
    match state.tracking_svc.reschedule(
        &tracking_number,
        body.preferred_date,
        body.preferred_time_slot.as_deref(),
        reason_str,
    ).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "data": {
                    "rescheduled": true,
                    "tracking_number": tracking_number,
                }
            })),
        ).into_response(),
        Err(AppError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Tracking number not found"})),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /v1/tracking/:tracking_number/confirm-receipt
// Customer confirms they received the package (authenticated).
// Triggers a delivery.customer_confirmed Kafka event which downstream services
// (POD, analytics) can react to.
// ---------------------------------------------------------------------------

async fn confirm_receipt(
    State(state): State<AppState>,
    Path(tracking_number): Path<String>,
) -> impl IntoResponse {
    match state.tracking_svc.confirm_customer_receipt(&tracking_number).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "data": {
                    "confirmed": true,
                    "tracking_number": tracking_number,
                }
            })),
        ).into_response(),
        Err(AppError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Tracking number not found"})),
        ).into_response(),
        Err(AppError::BusinessRule(msg)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": msg})),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /track/:tracking_number/feedback
// POST /v1/tracking/:tracking_number/feedback
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct FeedbackBody {
    rating: i16,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    comments: Option<String>,
}

async fn submit_feedback(
    State(state): State<AppState>,
    Path(tracking_number): Path<String>,
    Json(body): Json<FeedbackBody>,
) -> impl IntoResponse {
    match state.tracking_svc.submit_feedback(
        &tracking_number,
        body.rating,
        body.tags,
        body.comments,
    ).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"data": {"submitted": true}})),
        ).into_response(),
        Err(AppError::Validation(msg)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": msg})),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /v1/tracking/:tracking_number/send-receipt
// Customer requests their receipt emailed to them.
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct SendReceiptBody {
    email: String,
}

async fn send_receipt(
    State(state): State<AppState>,
    Path(tracking_number): Path<String>,
    Json(body): Json<SendReceiptBody>,
) -> impl IntoResponse {
    match state.tracking_svc.send_receipt_email(&tracking_number, &body.email).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "data": {
                    "sent": true,
                    "email": body.email,
                    "tracking_number": tracking_number,
                }
            })),
        ).into_response(),
        Err(AppError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Tracking number not found"})),
        ).into_response(),
        Err(AppError::Validation(msg)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": msg})),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ).into_response(),
    }
}
