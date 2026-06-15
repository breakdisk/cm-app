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
        // Internal endpoint — called by the business-logic rules engine (no user auth required).
        // The caller provides tenant_id in the body; the service validates the campaign belongs
        // to that tenant before publishing.
        .route("/v1/internal/campaigns/:id/trigger-for-recipient", post(trigger_for_recipient))
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

    // CDP audience resolution — runs before activation so the fan-out Kafka
    // event always carries a fully-resolved recipient list.
    if let Some(ref cdp) = state.cdp_client {
        let campaign = state.campaign_svc.get(id).await?;

        if campaign.targeting.recipients.is_empty() {
            // 1. Resolve CLV-filter or explicit customer_ids audience.
            if campaign.targeting.min_clv_score.is_some()
                || !campaign.targeting.customer_ids.is_empty()
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
                state.campaign_svc.patch_recipients(id, recipients).await?;

            // 2. Resolve segment-based audience.
            } else if let Some(seg_id) = campaign.targeting.segment_id {
                let recipients = cdp
                    .resolve_segment_audience(claims.tenant_id, seg_id)
                    .await
                    .map_err(AppError::internal)?;

                if recipients.is_empty() {
                    return Err(AppError::BusinessRule(
                        "Segment has 0 members — add customers to the segment before activating".to_owned()
                    ));
                }
                state.campaign_svc.patch_recipients(id, recipients).await?;
            }
        }
    }

    // Reject activation if the campaign has no recipients and no targeting rule
    // that would produce recipients at send time.
    let pre_activate = state.campaign_svc.get(id).await?;
    if pre_activate.targeting.recipients.is_empty()
        && pre_activate.targeting.customer_ids.is_empty()
        && pre_activate.targeting.min_clv_score.is_none()
        && pre_activate.targeting.segment_id.is_none()
    {
        return Err(AppError::BusinessRule(
            "Campaign has no recipients — add recipients or select a segment before activating".to_owned()
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

// ---------------------------------------------------------------------------
// Internal: trigger a campaign for a single recipient from the rules engine
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct TriggerForRecipientBody {
    customer_id:  Uuid,
    tenant_id:    Uuid,
    rule_id:      Uuid,
    rule_name:    String,
    shipment_id:  Option<Uuid>,
}

/// `POST /v1/internal/campaigns/:id/trigger-for-recipient`
///
/// Called by the business-logic rules engine when a `TriggerCampaign` action
/// fires.  Loads the campaign's channel + template, resolves the customer's
/// contact details from the CDP, and publishes a single-recipient
/// `CAMPAIGN_TRIGGERED` event so the engagement service handles delivery.
///
/// This endpoint is internal (no Bearer token required) — it is not exposed
/// through the public API gateway.  Network-level isolation (K8s NetworkPolicy)
/// restricts callers to the business-logic service.
async fn trigger_for_recipient(
    State(state): State<AppState>,
    Path(campaign_id): Path<Uuid>,
    Json(body): Json<TriggerForRecipientBody>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;

    let tenant_id = TenantId::from_uuid(body.tenant_id);
    let campaign  = state.campaign_svc.get(campaign_id).await?;

    if campaign.tenant_id != tenant_id {
        return Err(AppError::Forbidden { resource: "campaign".to_owned() });
    }

    // Resolve the single customer's contact from CDP (if client is wired).
    let recipient = if let Some(ref cdp) = state.cdp_client {
        let targeting = crate::domain::entities::TargetingRule {
            customer_ids:     vec![body.customer_id],
            min_clv_score:    None,
            last_active_days: None,
            recipients:       vec![],
            estimated_reach:  1,
        };
        let mut resolved = cdp
            .resolve_audience(body.tenant_id, &targeting)
            .await
            .map_err(AppError::internal)?;
        if resolved.is_empty() {
            return Err(AppError::BusinessRule(format!(
                "CDP could not resolve customer {} — skipping campaign trigger",
                body.customer_id
            )));
        }
        resolved.remove(0)
    } else {
        // No CDP client — send to customer_id only (push channel will still work).
        crate::domain::entities::CampaignRecipient {
            customer_id: Some(body.customer_id),
            name:        None,
            email:       None,
            phone:       None,
            platform_id: None,
        }
    };

    // Build a single-recipient CAMPAIGN_TRIGGERED payload that mirrors the bulk
    // path so the engagement consumer can handle it identically.
    let payload = serde_json::json!({
        "campaign_id":             campaign.id.inner(),
        "tenant_id":               campaign.tenant_id.inner(),
        "name":                    campaign.name,
        "created_by":              campaign.created_by,
        "channel":                 campaign.channel,
        "template_id":             campaign.template.template_id,
        "subject":                 campaign.template.subject,
        "variables":               campaign.template.variables,
        "triggered_by_rule_id":    body.rule_id,
        "triggered_by_rule_name":  body.rule_name,
        "recipients": [recipient],
    });

    state.campaign_svc
        .publish_campaign_triggered(&payload)
        .await
        .map_err(AppError::internal)?;

    tracing::info!(
        campaign_id = %campaign_id,
        customer_id = %body.customer_id,
        rule_id     = %body.rule_id,
        rule_name   = %body.rule_name,
        "Campaign triggered by automation rule for single recipient"
    );

    Ok::<_, AppError>((StatusCode::ACCEPTED, Json(serde_json::json!({
        "status":      "accepted",
        "campaign_id": campaign_id,
        "customer_id": body.customer_id,
        "rule_id":     body.rule_id,
    }))))
}
