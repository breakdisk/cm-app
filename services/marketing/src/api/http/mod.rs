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

/// Auth-protected routes (mounted with `require_auth` middleware in bootstrap).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/campaigns",                                        get(list_campaigns).post(create_campaign))
        .route("/v1/campaigns/weekly-stats",                           get(weekly_stats_handler))
        .route("/v1/campaigns/:id",                                    get(get_campaign))
        .route("/v1/campaigns/:id/schedule",                           post(schedule_campaign))
        .route("/v1/campaigns/:id/activate",                           post(activate_campaign))
        .route("/v1/campaigns/:id/cancel",                             post(cancel_campaign))
        // A/B Testing
        .route("/v1/campaigns/:id/ab-test",                            get(get_ab_test).post(create_ab_test))
        .route("/v1/campaigns/:id/ab-test/select-winner",             post(select_ab_winner))
        // Journey Builder
        .route("/v1/journeys",                                         get(list_journeys).post(create_journey))
        .route("/v1/journeys/:id",                                     get(get_journey).put(update_journey).delete(delete_journey))
        .route("/v1/journeys/:id/activate",                            post(activate_journey))
        .route("/v1/journeys/:id/pause",                               post(pause_journey))
        .route("/v1/journeys/:id/enroll",                              post(enroll_journey))
        .route("/v1/journeys/:id/enrollments",                         get(list_journey_enrollments))
        // MCP server endpoint — consumed by the AI layer and API gateway registry
        .route("/mcp",                                                 post(mcp_handler))
}

/// Internal routes — no user auth required. API gateway blocks /v1/internal/* from
/// public traffic; network-level isolation (K8s NetworkPolicy) restricts callers to
/// peer services within the cluster.
pub fn internal_router() -> Router<AppState> {
    Router::new()
        .route("/v1/internal/campaigns/:id/trigger-for-recipient", post(trigger_for_recipient))
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

    // A/B test fan-out: split the resolved audience by variant weight *before*
    // activating, so each variant's slice is published to Kafka under its own
    // template_id instead of everyone silently receiving the base template.
    let ab_test = state.ab_test_repo.find_by_campaign(id).await.ok().flatten();
    let recipients = pre_activate.targeting.recipients.clone();

    let variant_plan = ab_test.as_ref().and_then(|ab_test| {
        if recipients.is_empty() {
            return None;
        }
        let total = recipients.len();
        let mut offset = 0usize;
        let mut plan: Vec<(String, Vec<crate::domain::entities::CampaignRecipient>)> = Vec::new();
        for variant in &ab_test.variants {
            let count = (variant.weight_pct as usize * total).div_ceil(100).min(total - offset);
            if count == 0 { continue; }
            let slice = recipients[offset..offset + count].to_vec();
            offset += count;
            plan.push((variant.template_id.clone(), slice));
            if offset >= total { break; }
        }
        Some(plan)
    });

    let campaign = state.campaign_svc.activate(id, variant_plan.clone()).await?;

    // Record each recipient's variant assignment in send_log — populates the
    // stats queried by GET /v1/campaigns/:id/ab-test.
    if let Some(ab_test) = ab_test {
        if let Some(plan) = variant_plan {
            let tenant  = campaign.tenant_id.inner();
            let channel = serde_json::to_value(&campaign.channel)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| "whatsapp".to_owned());

            for (variant, (template_id, slice)) in ab_test.variants.iter().zip(plan.iter()) {
                let customer_ids: Vec<Uuid> = slice.iter().filter_map(|r| r.customer_id).collect();
                if customer_ids.is_empty() { continue; }

                if let Err(e) = state.ab_test_repo.log_variant_sends(
                    id, tenant, &channel, template_id, &variant.name, &customer_ids,
                ).await {
                    tracing::warn!(
                        err = %e, campaign_id = %id, variant = %variant.name,
                        "A/B fan-out: failed to log variant sends — stats will be incomplete"
                    );
                }
            }
        }
    }

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
/// `shipment_id` rides along for the template context that business-logic
/// sends; this handler routes on customer and rule only.
#[allow(dead_code)]
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
            segment_id:       None,
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

// ---------------------------------------------------------------------------
// A/B Testing handlers
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct CreateAbTestBody {
    name:     String,
    variants: Vec<crate::domain::entities::AbVariant>,
}

/// `POST /v1/campaigns/:id/ab-test` — create an A/B test for a campaign.
async fn create_ab_test(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(campaign_id): Path<Uuid>,
    Json(body): Json<CreateAbTestBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;

    let campaign = state.campaign_svc.get(campaign_id).await?;
    if campaign.tenant_id != logisticos_types::TenantId::from_uuid(claims.tenant_id) {
        return Err(AppError::Forbidden { resource: "campaign".to_owned() });
    }
    if body.variants.len() < 2 {
        return Err(AppError::Validation("A/B test requires at least 2 variants".to_owned()));
    }
    let total_weight: u32 = body.variants.iter().map(|v| v.weight_pct as u32).sum();
    if total_weight != 100 {
        return Err(AppError::Validation(format!(
            "A/B variant weights must sum to 100 (got {total_weight})"
        )));
    }

    use crate::domain::entities::AbTest;
    let test = AbTest {
        id:             Uuid::new_v4(),
        tenant_id:      claims.tenant_id,
        campaign_id,
        name:           body.name,
        variants:       body.variants,
        winner_variant: None,
        started_at:     chrono::Utc::now(),
        concluded_at:   None,
    };
    state.ab_test_repo.create(&test).await.map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(test)))
}

/// `GET /v1/campaigns/:id/ab-test` — get A/B test with variant performance stats.
async fn get_ab_test(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(campaign_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let campaign = state.campaign_svc.get(campaign_id).await?;
    if campaign.tenant_id != logisticos_types::TenantId::from_uuid(claims.tenant_id) {
        return Err(AppError::Forbidden { resource: "campaign".to_owned() });
    }
    let test = state.ab_test_repo.find_by_campaign(campaign_id).await.map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "AbTest", id: campaign_id.to_string() })?;
    let stats = state.ab_test_repo.get_stats(campaign_id).await.map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "ab_test": test, "stats": stats }))))
}

#[derive(serde::Deserialize)]
struct SelectWinnerBody { variant: String }

/// `POST /v1/campaigns/:id/ab-test/select-winner` — mark the winning variant.
async fn select_ab_winner(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(campaign_id): Path<Uuid>,
    Json(body): Json<SelectWinnerBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_SEND)?;
    let campaign = state.campaign_svc.get(campaign_id).await?;
    if campaign.tenant_id != logisticos_types::TenantId::from_uuid(claims.tenant_id) {
        return Err(AppError::Forbidden { resource: "campaign".to_owned() });
    }
    state.ab_test_repo.set_winner(campaign_id, &body.variant).await.map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "winner_variant": body.variant }))))
}

// ---------------------------------------------------------------------------
// Journey handlers
// ---------------------------------------------------------------------------

use crate::domain::entities::{Journey, JourneyEnrollment, JourneyStatus, JourneyStep};

#[derive(serde::Deserialize)]
struct CreateJourneyBody {
    name:        String,
    description: Option<String>,
    trigger:     serde_json::Value,
    steps:       Vec<CreateJourneyStepBody>,
}

#[derive(serde::Deserialize)]
struct CreateJourneyStepBody {
    step_order:            i32,
    step_type:             String,
    campaign_id:           Option<Uuid>,
    wait_days:             Option<i32>,
    condition_type:        Option<String>,
    condition_campaign_id: Option<Uuid>,
    yes_next_order:        Option<i32>,
    no_next_order:         Option<i32>,
}

fn build_journey(tenant_id: Uuid, body: CreateJourneyBody, existing_id: Option<Uuid>) -> Journey {
    let now = chrono::Utc::now();
    let journey_id = existing_id.unwrap_or_else(Uuid::new_v4);
    let steps = body.steps.into_iter().map(|s| JourneyStep {
        id:                    Uuid::new_v4(),
        journey_id,
        step_order:            s.step_order,
        step_type:             s.step_type,
        campaign_id:           s.campaign_id,
        wait_days:             s.wait_days,
        condition_type:        s.condition_type,
        condition_campaign_id: s.condition_campaign_id,
        yes_next_order:        s.yes_next_order,
        no_next_order:         s.no_next_order,
    }).collect();
    Journey {
        id: journey_id, tenant_id,
        name: body.name, description: body.description, trigger: body.trigger,
        status: JourneyStatus::Draft, steps,
        created_at: now, updated_at: now,
    }
}

/// `GET /v1/journeys` — list all journeys for the tenant.
async fn list_journeys(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let journeys = state.journey_repo.list(claims.tenant_id).await.map_err(AppError::internal)?;
    let count = journeys.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "journeys": journeys, "count": count }))))
}

/// `POST /v1/journeys` — create a new journey.
async fn create_journey(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<CreateJourneyBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let journey = build_journey(claims.tenant_id, body, None);
    state.journey_repo.save(&journey).await.map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(journey)))
}

/// `GET /v1/journeys/:id`
async fn get_journey(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let journey = state.journey_repo.find_by_id(id).await.map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Journey", id: id.to_string() })?;
    if journey.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "journey".to_owned() });
    }
    Ok::<_, AppError>((StatusCode::OK, Json(journey)))
}

/// `PUT /v1/journeys/:id` — update journey name/description/trigger/steps.
async fn update_journey(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateJourneyBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let existing = state.journey_repo.find_by_id(id).await.map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Journey", id: id.to_string() })?;
    if existing.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "journey".to_owned() });
    }
    if existing.status == JourneyStatus::Active {
        return Err(AppError::BusinessRule("Cannot edit an active journey — pause it first".to_owned()));
    }
    let prior_status     = existing.status.clone();
    let prior_created_at = existing.created_at;
    let mut updated = build_journey(claims.tenant_id, body, Some(id));
    updated.status     = prior_status;
    updated.created_at = prior_created_at;
    state.journey_repo.save(&updated).await.map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::OK, Json(updated)))
}

/// `DELETE /v1/journeys/:id`
async fn delete_journey(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let journey = state.journey_repo.find_by_id(id).await.map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Journey", id: id.to_string() })?;
    if journey.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "journey".to_owned() });
    }
    state.journey_repo.delete(id).await.map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::NO_CONTENT, ""))
}

/// `POST /v1/journeys/:id/activate` — set status to Active.
async fn activate_journey(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_SEND)?;
    let mut journey = state.journey_repo.find_by_id(id).await.map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Journey", id: id.to_string() })?;
    if journey.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "journey".to_owned() });
    }
    if journey.steps.is_empty() {
        return Err(AppError::BusinessRule("Journey must have at least one step before activating".to_owned()));
    }
    journey.status     = JourneyStatus::Active;
    journey.updated_at = chrono::Utc::now();
    state.journey_repo.save(&journey).await.map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::OK, Json(journey)))
}

/// `POST /v1/journeys/:id/pause` — set status to Paused (only from Active).
async fn pause_journey(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_SEND)?;
    let mut journey = state.journey_repo.find_by_id(id).await.map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Journey", id: id.to_string() })?;
    if journey.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "journey".to_owned() });
    }
    if journey.status != JourneyStatus::Active {
        return Err(AppError::BusinessRule("Only active journeys can be paused".to_owned()));
    }
    journey.status     = JourneyStatus::Paused;
    journey.updated_at = chrono::Utc::now();
    state.journey_repo.save(&journey).await.map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::OK, Json(journey)))
}

#[derive(serde::Deserialize)]
struct EnrollBody { customer_ids: Vec<Uuid> }

/// `POST /v1/journeys/:id/enroll` — enroll customers in the journey.
async fn enroll_journey(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(journey_id): Path<Uuid>,
    Json(body): Json<EnrollBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_SEND)?;
    let journey = state.journey_repo.find_by_id(journey_id).await.map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Journey", id: journey_id.to_string() })?;
    if journey.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "journey".to_owned() });
    }
    if journey.status != JourneyStatus::Active {
        return Err(AppError::BusinessRule("Journey must be Active to enroll customers".to_owned()));
    }
    let now = chrono::Utc::now();
    let mut enrolled = 0usize;
    for customer_id in body.customer_ids {
        let enrollment = JourneyEnrollment {
            id: Uuid::new_v4(), journey_id, tenant_id: claims.tenant_id, customer_id,
            current_step_order: Some(1), status: "active".to_owned(),
            next_action_at: Some(now), enrolled_at: now,
        };
        state.journey_repo.save_enrollment(&enrollment).await.map_err(AppError::internal)?;
        enrolled += 1;
    }
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "enrolled": enrolled }))))
}

/// `GET /v1/journeys/:id/enrollments`
async fn list_journey_enrollments(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(journey_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let journey = state.journey_repo.find_by_id(journey_id).await.map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Journey", id: journey_id.to_string() })?;
    if journey.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "journey".to_owned() });
    }
    let enrollments = state.journey_repo.list_enrollments(journey_id).await.map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "enrollments": enrollments }))))
}
