use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, put},
    Router,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::application::services::{ProfileService, UpsertProfileCommand};
use crate::domain::repositories::ProfileFilter;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/customers",                   get(list_profiles))
        .route("/v1/customers/top-clv",           get(top_by_clv))
        .route("/v1/customers/:external_id",       get(get_profile).put(upsert_profile))
        .route("/v1/customers/:external_id/events", get(get_events))
}

// ---------------------------------------------------------------------------
// GET /v1/customers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    name:    Option<String>,
    email:   Option<String>,
    phone:   Option<String>,
    min_clv: Option<f32>,
    limit:   Option<i64>,
    offset:  Option<i64>,
}

async fn list_profiles(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_VIEW)?;

    let filter = ProfileFilter {
        name_contains: q.name,
        email:         q.email,
        phone:         q.phone,
        min_clv:       q.min_clv,
        limit:         q.limit.unwrap_or(50).clamp(1, 200),
        offset:        q.offset.unwrap_or(0).max(0),
    };

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let profiles = state
        .profile_svc
        .list(&tenant_id, filter)
        .await?;
    let count = profiles.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"profiles": profiles, "count": count}))))
}

// ---------------------------------------------------------------------------
// GET /v1/customers/top-clv
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TopQuery {
    limit: Option<i64>,
}

async fn top_by_clv(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<TopQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_VIEW)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let profiles = state
        .profile_svc
        .top_by_clv(&tenant_id, q.limit.unwrap_or(20))
        .await?;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"profiles": profiles}))))
}

// ---------------------------------------------------------------------------
// GET /v1/customers/:external_id
// ---------------------------------------------------------------------------

async fn get_profile(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(external_id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_VIEW)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let profile = state
        .profile_svc
        .get_by_external_id(&tenant_id, external_id)
        .await?;

    Ok::<_, AppError>((StatusCode::OK, Json(profile)))
}

// ---------------------------------------------------------------------------
// PUT /v1/customers/:external_id — upsert identity fields
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UpsertBody {
    pub name:  Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
}

async fn upsert_profile(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(external_id): Path<Uuid>,
    Json(body): Json<UpsertBody>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_MANAGE)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let summary = state
        .profile_svc
        .upsert(
            &tenant_id,
            UpsertProfileCommand {
                external_customer_id: external_id,
                name:                 body.name,
                email:                body.email,
                phone:                body.phone,
            },
        )
        .await?;

    Ok::<_, AppError>((StatusCode::OK, Json(summary)))
}

// ---------------------------------------------------------------------------
// GET /v1/customers/:external_id/events — recent behavioral events
// ---------------------------------------------------------------------------

async fn get_events(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(external_id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CUSTOMERS_VIEW)?;

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let profile = state
        .profile_svc
        .get_by_external_id(&tenant_id, external_id)
        .await?;

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "external_customer_id": external_id,
            "events": profile.recent_events,
            "count": profile.recent_events.len(),
        })),
    ))
}
