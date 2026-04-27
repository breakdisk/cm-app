use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, put},
    Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::application::services::{OnboardCarrierCommand, UpdateCarrierCommand};
use crate::domain::entities::SlaRecord;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Health check — no auth required
        .route("/health",                           get(health))
        // Carrier CRUD
        .route("/v1/carriers",                      get(list_carriers).post(onboard_carrier))
        .route("/v1/carriers/me",                   get(get_my_carrier))
        .route("/v1/carriers/rate-shop",            get(rate_shop))
        .route("/v1/carriers/:id",                  get(get_carrier).put(update_carrier))
        .route("/v1/carriers/:id/activate",         post(activate_carrier))
        .route("/v1/carriers/:id/suspend",          post(suspend_carrier))
        // SLA reporting
        .route("/v1/carriers/:id/sla-summary",      get(sla_summary))
        .route("/v1/carriers/:id/sla-history",      get(sla_history))
        // Internal — called by dispatch when allocating a carrier to a shipment
        .route("/v1/internal/sla-records",          post(create_sla_record))
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
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
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
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let carrier = state.carrier_svc.suspend(id, body.reason).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(carrier)))
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
