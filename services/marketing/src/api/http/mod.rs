use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
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
use crate::domain::entities::CampaignStatus;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/campaigns",                  get(list_campaigns).post(create_campaign))
        .route("/v1/campaigns/weekly-stats",     get(weekly_stats_handler))
        .route("/v1/campaigns/:id",              get(get_campaign))
        .route("/v1/campaigns/:id/schedule",     post(schedule_campaign))
        .route("/v1/campaigns/:id/activate",     post(activate_campaign))
        .route("/v1/campaigns/:id/cancel",       post(cancel_campaign))
        // MCP server endpoint — consumed by the AI layer and API gateway registry
        .route("/mcp",                           post(mcp_handler))
}

// ---------------------------------------------------------------------------
// MCP handler — JSON-RPC style: { "method": "tools/list" | "tools/call", "params": {...} }
// ---------------------------------------------------------------------------

async fn mcp_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    use crate::mcp::{audit, auth, tools};
    use std::time::Instant;

    let method = body.get("method").and_then(|v| v.as_str()).unwrap_or("");

    match method {
        "tools/list" => {
            let payload = serde_json::json!({ "tools": tools::list() });
            (StatusCode::OK, Json(payload)).into_response()
        }
        "tools/call" => {
            let ctx = match auth::extract_context(&headers, &state.jwt) {
                Ok(c)  => c,
                Err(e) => {
                    let resp = serde_json::json!({ "error": e });
                    return (StatusCode::UNAUTHORIZED, Json(resp)).into_response();
                }
            };

            let params    = body.get("params").unwrap_or(&serde_json::Value::Null);
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args      = params.get("arguments").unwrap_or(&serde_json::Value::Null);
            let start     = Instant::now();

            let state_arc = std::sync::Arc::new(state.clone());
            match tools::dispatch(tool_name, args, &ctx, &state_arc).await {
                Ok(result) => {
                    audit::audit_tool_call(&ctx, tool_name, true, start);
                    let payload = serde_json::json!({
                        "content": [{ "type": "text", "text": result.to_string() }]
                    });
                    (StatusCode::OK, Json(payload)).into_response()
                }
                Err(e) => {
                    audit::audit_tool_call(&ctx, tool_name, false, start);
                    let payload = serde_json::json!({ "error": e });
                    (StatusCode::BAD_REQUEST, Json(payload)).into_response()
                }
            }
        }
        other => {
            let payload = serde_json::json!({ "error": format!("Unknown method: {other}") });
            (StatusCode::BAD_REQUEST, Json(payload)).into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    limit:  Option<i64>,
    offset: Option<i64>,
    status: Option<String>,
}

async fn list_campaigns(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let campaigns = if let Some(status_str) = q.status.as_deref() {
        let status: CampaignStatus = serde_json::from_value(serde_json::Value::String(status_str.to_owned()))
            .map_err(|_| AppError::Validation(format!("unknown campaign status: {status_str}")))?;
        state.campaign_svc.list_by_status(&tenant_id, &status).await?
    } else {
        state.campaign_svc.list(&tenant_id, q.limit.unwrap_or(50), q.offset.unwrap_or(0)).await?
    };

    let count = campaigns.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"campaigns": campaigns, "count": count}))))
}

async fn create_campaign(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<CreateCampaignCommand>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let campaign = state.campaign_svc.create(&tenant_id, claims.user_id, cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(campaign)))
}

async fn get_campaign(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let campaign = state.campaign_svc.get(id).await?;
    if campaign.tenant_id != TenantId::from_uuid(claims.tenant_id) {
        return Err(AppError::Forbidden { resource: "campaign".to_owned() });
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

    // CDP audience resolution: if the campaign targets by CLV/customer_ids but
    // has no explicit recipients, resolve them from the CDP before activating.
    if let Some(ref cdp) = state.cdp_client {
        let campaign = state.campaign_svc.get(id).await?;
        if campaign.targeting.recipients.is_empty()
            && (campaign.targeting.min_clv_score.is_some()
                || !campaign.targeting.customer_ids.is_empty())
        {
            let recipients = cdp
                .resolve_audience(claims.tenant_id, &campaign.targeting)
                .await
                .map_err(AppError::internal)?;

            if recipients.is_empty() {
                return Err(AppError::BusinessRule(
                    "CDP resolved 0 recipients — broaden your targeting before activating".to_owned()
                ));
            }

            state.campaign_svc
                .patch_recipients(id, recipients)
                .await?;
        }
    }

    // Reject activation if the campaign has no recipients and no targeting rule
    // that would produce recipients at send time.
    let pre_activate = state.campaign_svc.get(id).await?;
    if pre_activate.targeting.recipients.is_empty()
        && pre_activate.targeting.customer_ids.is_empty()
        && pre_activate.targeting.min_clv_score.is_none()
    {
        return Err(AppError::BusinessRule(
            "Campaign has no recipients — add recipients before activating".to_owned()
        ));
    }

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

async fn weekly_stats_handler(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let stats = state.campaign_svc.weekly_stats(&tenant_id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "stats": stats }))))
}
