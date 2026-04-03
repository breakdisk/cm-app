use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use chrono::NaiveDate;
use serde::Deserialize;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;
use logisticos_types::TenantId;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/analytics/kpis",              get(delivery_kpis))
        .route("/v1/analytics/timeseries",        get(daily_timeseries))
        .route("/v1/analytics/driver-performance", get(driver_performance))
        .route("/v1/analytics/dashboard",          get(get_dashboard))
}

#[derive(Debug, Deserialize)]
struct DateRangeQuery {
    from:  NaiveDate,
    to:    NaiveDate,
    limit: Option<i64>,
}

async fn delivery_kpis(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<DateRangeQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::ANALYTICS_VIEW)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let kpis = state.query_svc.delivery_kpis(&tenant_id, q.from, q.to).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(kpis)))
}

async fn daily_timeseries(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<DateRangeQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::ANALYTICS_VIEW)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let buckets = state.query_svc.daily_timeseries(&tenant_id, q.from, q.to).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"data": buckets, "count": buckets.len()}))))
}

async fn driver_performance(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<DateRangeQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::ANALYTICS_VIEW)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let perf = state
        .query_svc
        .driver_performance(&tenant_id, q.from, q.to, q.limit.unwrap_or(20))
        .await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"drivers": perf, "count": perf.len()}))))
}

async fn get_dashboard(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    if let Err(e) = claims.require_permission(permissions::ANALYTICS_VIEW) {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": e.to_string()}))).into_response();
    }
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    match state.query_svc.dashboard(&tenant_id).await {
        Ok(data)  => (StatusCode::OK, Json(serde_json::json!({"data": data}))).into_response(),
        Err(e)    => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}
