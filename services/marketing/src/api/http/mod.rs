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

use crate::application::services::{CreateCampaignCommand, ScheduleCampaignCommand};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/campaigns",                  get(list_campaigns).post(create_campaign))
        .route("/v1/campaigns/:id",              get(get_campaign))
        .route("/v1/campaigns/:id/schedule",     post(schedule_campaign))
        .route("/v1/campaigns/:id/activate",     post(activate_campaign))
        .route("/v1/campaigns/:id/cancel",       post(cancel_campaign))
}

#[derive(Debug, Deserialize)]
struct ListQuery { limit: Option<i64>, offset: Option<i64> }

async fn list_campaigns(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let campaigns = state.campaign_svc.list(&claims.tenant_id, q.limit.unwrap_or(50), q.offset.unwrap_or(0)).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"campaigns": campaigns, "count": campaigns.len()}))))
}

async fn create_campaign(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<CreateCampaignCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let campaign = state.campaign_svc.create(&claims.tenant_id, claims.user_id, cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(campaign)))
}

async fn get_campaign(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let campaign = state.campaign_svc.get(id).await?;
    if campaign.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden("Access denied".into()));
    }
    Ok::<_, AppError>((StatusCode::OK, Json(campaign)))
}

async fn schedule_campaign(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(cmd): Json<ScheduleCampaignCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_SEND)?;
    let campaign = state.campaign_svc.schedule(id, cmd).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(campaign)))
}

async fn activate_campaign(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_SEND)?;
    let campaign = state.campaign_svc.activate(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(campaign)))
}

async fn cancel_campaign(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_SEND)?;
    let campaign = state.campaign_svc.cancel(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(campaign)))
}
