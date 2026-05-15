use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{delete, get, post, put},
    Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::application::services::{
    CreateListingCommand, OnboardCarrierCommand, RecordPickupInput,
    UpdateCarrierCommand, UpdateListingPatch,
};
use crate::domain::entities::SlaRecord;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Health check — no auth required
        .route("/health",                                      get(health))
        // Carrier CRUD
        .route("/v1/carriers",                                 get(list_carriers).post(onboard_carrier))
        .route("/v1/carriers/me",                              get(get_my_carrier))
        .route("/v1/carriers/rate-shop",                       get(rate_shop))
        .route("/v1/carriers/:id",                             get(get_carrier).put(update_carrier))
        .route("/v1/carriers/:id/activate",                    post(activate_carrier))
        .route("/v1/carriers/:id/suspend",                     post(suspend_carrier))
        .route("/v1/carriers/:id/api-key",                     post(generate_api_key))
        .route("/v1/carriers/:id/compliance-documents",        post(upload_compliance_document))
        // SLA reporting
        .route("/v1/carriers/:id/sla-summary",                 get(sla_summary))
        .route("/v1/carriers/:id/sla-history",                 get(sla_history))
        .route("/v1/carriers/:id/breach-reasons",              get(breach_reasons))
        // Marketplace — vehicle listings
        .route("/v1/marketplace/listings",                     get(list_listings).post(create_listing))
        .route("/v1/marketplace/listings/:listing_id",         put(update_listing).delete(delete_listing))
        // Marketplace — bookings
        .route("/v1/marketplace/bookings",                     get(list_bookings))
        .route("/v1/marketplace/bookings/:booking_id/accept",  post(accept_booking))
        .route("/v1/marketplace/bookings/:booking_id/reject",  post(reject_booking))
        .route("/v1/marketplace/bookings/:booking_id/pickup",  post(record_pickup))
        // Internal — called by dispatch when allocating a carrier to a shipment
        .route("/v1/internal/sla-records",                     post(create_sla_record))
        // Inbound webhook callbacks from 3PL carrier systems (API-key auth)
        .route("/v1/webhooks/carrier/:id/tracking",            post(carrier_tracking_webhook))
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"status": "ok", "service": "carrier"})))
}

#[derive(Debug, Deserialize)]
struct ListQuery { limit: Option<i64>, offset: Option<i64> }

async fn list_carriers(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carriers = state.carrier_svc.list(&tenant_id, q.limit.unwrap_or(50), q.offset.unwrap_or(0)).await?;
    let count = carriers.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"carriers": carriers, "count": count}))))
}

async fn onboard_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<OnboardCarrierCommand>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier = state.carrier_svc.onboard(&tenant_id, cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(carrier)))
}

/// GET /v1/carriers/me — returns the carrier whose contact_email matches
/// the authenticated user's JWT email. Used by the partner portal so it
/// doesn't need to know its own carrier UUID.
async fn get_my_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier = state.carrier_svc.get_by_email(&tenant_id, &claims.email).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(carrier)))
}

async fn get_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_READ)?;
    let carrier = state.carrier_svc.get(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(carrier)))
}

async fn update_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateCarrierCommand>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    // Tenant guard: refuse cross-tenant updates even if a leaked admin
    // token was used. Same shape as get_carrier returning 404 instead of
    // 403 to avoid leaking carrier existence to other tenants.
    let existing = state.carrier_svc.get(id).await?;
    let claim_tenant = TenantId::from_uuid(claims.tenant_id);
    if existing.tenant_id.inner() != claim_tenant.inner() {
        return Err(AppError::NotFound { resource: "Carrier", id: id.to_string() });
    }
    let carrier = state.carrier_svc.update(id, cmd).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(carrier)))
}

async fn activate_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let existing = state.carrier_svc.get(id).await?;
    let claim_tenant = TenantId::from_uuid(claims.tenant_id);
    if existing.tenant_id.inner() != claim_tenant.inner() {
        return Err(AppError::NotFound { resource: "Carrier", id: id.to_string() });
    }
    let carrier = state.carrier_svc.activate(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(carrier)))
}

#[derive(Debug, Deserialize)]
struct SuspendBody { reason: String }

async fn suspend_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<SuspendBody>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let existing = state.carrier_svc.get(id).await?;
    let claim_tenant = TenantId::from_uuid(claims.tenant_id);
    if existing.tenant_id.inner() != claim_tenant.inner() {
        return Err(AppError::NotFound { resource: "Carrier", id: id.to_string() });
    }
    let carrier = state.carrier_svc.suspend(id, body.reason).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(carrier)))
}

/// POST /v1/carriers/:id/api-key
///
/// Generates (or rotates) the carrier's integration API key. Returns the
/// plaintext key **exactly once** — it is not stored and cannot be retrieved
/// again. The partner must copy it immediately.
///
/// Only `carriers:manage` callers can generate keys (admin or tenant_admin).
/// Tenant isolation is enforced the same way as `update_carrier`.
async fn generate_api_key(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    // Tenant guard — same pattern as update_carrier.
    let existing = state.carrier_svc.get(id).await?;
    let claim_tenant = TenantId::from_uuid(claims.tenant_id);
    if existing.tenant_id.inner() != claim_tenant.inner() {
        return Err(AppError::NotFound { resource: "Carrier", id: id.to_string() });
    }
    let raw_key = state.carrier_svc.generate_api_key(id).await?;
    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "api_key":     raw_key,
            "has_api_key": true,
        })),
    ))
}

#[derive(Debug, Deserialize)]
struct RateShopQuery {
    service_type: String,
    weight_kg: f32,
}

async fn rate_shop(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<RateShopQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let quotes = state.carrier_svc.shop_rates(&tenant_id, &q.service_type, q.weight_kg).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"quotes": quotes}))))
}

// ── SLA endpoints ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SlaSummaryQuery {
    /// ISO-8601 datetime, e.g. "2026-01-01T00:00:00Z"
    from: DateTime<Utc>,
    to:   DateTime<Utc>,
}

/// GET /v1/carriers/:id/sla-summary?from=&to=
/// Returns zone-level SLA aggregate for the carrier over the given window.
async fn sla_summary(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Query(q): Query<SlaSummaryQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_READ)?;
    let rows = state.carrier_svc.sla_zone_summary(id, q.from, q.to).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"zones": rows}))))
}

#[derive(Debug, Deserialize)]
struct SlaHistoryQuery { limit: Option<i64>, offset: Option<i64> }

/// GET /v1/carriers/:id/sla-history
/// Paginated delivery history for a carrier (partner portal detail view).
async fn sla_history(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Query(q): Query<SlaHistoryQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_READ)?;
    let records = state.carrier_svc
        .sla_history(id, q.limit.unwrap_or(50), q.offset.unwrap_or(0))
        .await?;
    let count = records.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"records": records, "count": count}))))
}

/// GET /v1/carriers/:id/breach-reasons?from=&to=
/// Returns SLA breach reasons aggregated by count for the partner portal SLA chart.
async fn breach_reasons(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Query(q): Query<SlaSummaryQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_READ)?;
    let rows = state.carrier_svc.breach_reasons(id, q.from, q.to).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"data": rows}))))
}

// ── Internal endpoints ────────────────────────────────────────────────────────

/// Body for `POST /v1/internal/sla-records` — sent by dispatch when it allocates
/// a carrier to a shipment. Not authenticated via JWT; trusted internal call
/// (mTLS in production, same cluster in dev).
#[derive(Debug, Deserialize)]
struct CreateSlaRecordBody {
    tenant_id:        Uuid,
    carrier_id:       Uuid,
    shipment_id:      Uuid,
    zone:             String,
    service_level:    String,
    promised_by:      DateTime<Utc>,
    total_cost_cents: i64,
    /// "rate_shop" | "manual"
    #[serde(default = "default_allocation_method")]
    method:           String,
}

fn default_allocation_method() -> String { "rate_shop".into() }

/// POST /v1/internal/sla-records
/// Creates a per-shipment SLA commitment record and emits `carrier.allocated`.
/// Called by dispatch immediately after carrier selection.
async fn create_sla_record(
    State(state): State<AppState>,
    Json(body): Json<CreateSlaRecordBody>,
) -> impl IntoResponse {
    let record = SlaRecord::new(
        body.tenant_id,
        body.carrier_id,
        body.shipment_id,
        body.zone,
        body.service_level,
        body.promised_by,
    );
    let created = state.carrier_svc
        .create_sla_record(record, body.total_cost_cents, body.method)
        .await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(created)))
}

// ── Compliance document upload ────────────────────────────────────────────────

/// POST /v1/carriers/:id/compliance-documents
///
/// Accepts a multipart upload containing `doc_type` (text field) and `file`
/// (the document). Updates the carrier's compliance_status to `under_review`.
/// In production the file bytes would be streamed to object storage (S3/GCS);
/// currently only the metadata is persisted to keep the service storage-agnostic.
async fn upload_compliance_document(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    // Tenant guard
    let existing = state.carrier_svc.get(id).await?;
    let claim_tenant = TenantId::from_uuid(claims.tenant_id);
    if existing.tenant_id.inner() != claim_tenant.inner() {
        return Err(AppError::NotFound { resource: "Carrier", id: id.to_string() });
    }

    let mut doc_type  = String::new();
    let mut filename  = String::from("unknown");
    let mut file_size = 0usize;

    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::internal(e.into()))? {
        let name = field.name().unwrap_or("").to_owned();
        match name.as_str() {
            "doc_type" => {
                doc_type = field.text().await.map_err(|e| AppError::internal(e.into()))?;
            }
            "file" => {
                if let Some(fname) = field.file_name() { filename = fname.to_owned(); }
                let bytes = field.bytes().await.map_err(|e| AppError::internal(e.into()))?;
                file_size = bytes.len();
                // Production: stream bytes to object storage here.
            }
            _ => {}
        }
    }

    if doc_type.is_empty() {
        return Err(AppError::BusinessRule("doc_type field is required".into()));
    }

    state.carrier_svc.submit_compliance_documents(id, &doc_type, &filename).await?;

    Ok::<_, AppError>((StatusCode::CREATED, Json(serde_json::json!({
        "message":    "Document received — under review",
        "doc_type":   doc_type,
        "filename":   filename,
        "size_bytes": file_size,
    }))))
}

// ── Marketplace — listings ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct MarketplacePaginationQuery { limit: Option<i64>, offset: Option<i64> }

/// GET /v1/marketplace/listings
/// Lists all vehicle listings owned by the authenticated carrier.
async fn list_listings(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<MarketplacePaginationQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier   = state.carrier_svc.get_by_email(&tenant_id, &claims.email).await?;
    let listings  = state.marketplace_svc
        .list_listings(carrier.id.inner(), q.limit.unwrap_or(50), q.offset.unwrap_or(0))
        .await?;
    let count = listings.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"listings": listings, "count": count}))))
}

/// POST /v1/marketplace/listings
async fn create_listing(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<CreateListingCommand>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier   = state.carrier_svc.get_by_email(&tenant_id, &claims.email).await?;
    let listing   = state.marketplace_svc
        .create_listing(carrier.tenant_id.inner(), carrier.id.inner(), cmd)
        .await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(listing)))
}

/// PUT /v1/marketplace/listings/:listing_id
async fn update_listing(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(listing_id): Path<Uuid>,
    Json(patch): Json<UpdateListingPatch>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier   = state.carrier_svc.get_by_email(&tenant_id, &claims.email).await?;
    let listing   = state.marketplace_svc
        .update_listing(listing_id, carrier.id.inner(), patch)
        .await?;
    Ok::<_, AppError>((StatusCode::OK, Json(listing)))
}

/// DELETE /v1/marketplace/listings/:listing_id
async fn delete_listing(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(listing_id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier   = state.carrier_svc.get_by_email(&tenant_id, &claims.email).await?;
    state.marketplace_svc.delete_listing(listing_id, carrier.id.inner()).await?;
    Ok::<_, AppError>(StatusCode::NO_CONTENT)
}

// ── Marketplace — bookings ────────────────────────────────────────────────────

/// GET /v1/marketplace/bookings
/// Lists bookings for the authenticated carrier, with PII masked on pending rows.
async fn list_bookings(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<MarketplacePaginationQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier   = state.carrier_svc.get_by_email(&tenant_id, &claims.email).await?;
    let bookings: Vec<_> = state.marketplace_svc
        .list_bookings(carrier.id.inner(), q.limit.unwrap_or(50), q.offset.unwrap_or(0))
        .await?
        .into_iter()
        .map(crate::application::services::MarketplaceService::apply_pii_mask)
        .collect();
    let count = bookings.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"bookings": bookings, "count": count}))))
}

/// POST /v1/marketplace/bookings/:booking_id/accept
async fn accept_booking(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(booking_id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier   = state.carrier_svc.get_by_email(&tenant_id, &claims.email).await?;
    let booking   = state.marketplace_svc.accept_booking(booking_id, carrier.id.inner()).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(booking)))
}

/// POST /v1/marketplace/bookings/:booking_id/reject
async fn reject_booking(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(booking_id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier   = state.carrier_svc.get_by_email(&tenant_id, &claims.email).await?;
    let booking   = state.marketplace_svc.reject_booking(booking_id, carrier.id.inner()).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(booking)))
}

/// POST /v1/marketplace/bookings/:booking_id/pickup
async fn record_pickup(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(booking_id): Path<Uuid>,
    Json(body): Json<RecordPickupInput>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier   = state.carrier_svc.get_by_email(&tenant_id, &claims.email).await?;
    let booking   = state.marketplace_svc.record_pickup(booking_id, carrier.id.inner(), body).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(booking)))
}

// ── Inbound 3PL webhook ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TrackingWebhookBody {
    /// Carrier's own shipment/tracking reference.
    reference:  Option<String>,
    /// Canonical LogisticOS shipment UUID (preferred).
    shipment_id: Option<Uuid>,
    /// Event type — e.g. "picked_up", "in_transit", "delivered", "failed".
    event:      String,
    /// Human-readable status message from the 3PL.
    message:    Option<String>,
}

/// POST /v1/webhooks/carrier/:id/tracking
///
/// Receives inbound tracking events from 3PL carrier systems. Authentication
/// is via the `X-Carrier-Api-Key` header (SHA-256 hash checked against the
/// carrier record). Events are logged for audit; in production they would be
/// forwarded to the engagement engine and update the shipment's live status.
async fn carrier_tracking_webhook(
    State(state): State<AppState>,
    Path(carrier_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<TrackingWebhookBody>,
) -> impl IntoResponse {
    // Verify API key — same logic as the require_carrier_api_key middleware.
    let raw_key = match headers.get("X-Carrier-Api-Key").and_then(|v| v.to_str().ok()) {
        Some(k) => k.to_owned(),
        None => return Ok::<_, AppError>((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Missing X-Carrier-Api-Key header"})),
        )),
    };
    let carrier = state.carrier_svc.get(carrier_id).await?;
    let expected = match &carrier.api_key_hash {
        Some(h) => h.clone(),
        None => return Ok((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "No API key configured for this carrier"})),
        )),
    };
    let presented = {
        let mut h = Sha256::new();
        h.update(raw_key.as_bytes());
        format!("{:x}", h.finalize())
    };
    if presented != expected {
        return Ok((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Invalid API key"})),
        ));
    }

    tracing::info!(
        carrier_id = %carrier_id,
        event      = %body.event,
        reference  = ?body.reference,
        shipment_id = ?body.shipment_id,
        message    = ?body.message,
        "Inbound 3PL tracking event received",
    );

    // Future: forward event to engagement engine / update shipment status.
    Ok((StatusCode::OK, Json(serde_json::json!({"received": true, "event": body.event}))))
}
