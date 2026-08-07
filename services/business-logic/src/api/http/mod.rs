//! HTTP API for the Business Logic & Automation Engine.
//!
//! Provides CRUD management of automation rules.
//! The rules engine itself is event-driven (Kafka consumer); this API
//! lets operators create, update, and monitor rules via the admin portal.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, patch, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::{
    application::services::RuleRepository,
    domain::entities::rule::{AutomationRule, RuleAction, RuleCondition, RuleTrigger},
    infrastructure::db::PgRuleRepository,
};

// ── AppState ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub rule_repo:    Arc<RuleRepository>,
    pub pg_repo:      Arc<PgRuleRepository>,
    pub jwt:          Arc<logisticos_auth::jwt::JwtService>,
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/rules",                    get(list_rules).post(create_rule))
        .route("/v1/rules/reload",             post(reload_rules))
        .route("/v1/rules/validate-expression", post(validate_expression))
        .route("/v1/rules/:id",                get(get_rule).put(update_rule).delete(delete_rule))
        .route("/v1/rules/:id/toggle",         patch(toggle_rule))
        .route("/v1/rules/:id/executions",     get(list_executions))
        // MCP endpoint — consumed by the AI layer for rule management
        .route("/mcp",                         post(mcp_handler))
        .route("/health",                      get(health))
        .route("/ready",                       get(health))
}

// ── GET /v1/rules ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListQuery {
    is_active: Option<bool>,
    page:      Option<u32>,
    per_page:  Option<u32>,
}

async fn list_rules(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    let page     = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);

    let mut rules = state.pg_repo
        .load_for_tenant(claims.tenant_id)
        .await
        .map_err(AppError::internal)?;

    if let Some(active) = q.is_active {
        rules.retain(|r| r.is_active == active);
    }

    let total = rules.len() as u32;
    let start = ((page - 1) * per_page) as usize;
    let page_rules: Vec<_> = rules.into_iter().skip(start).take(per_page as usize).collect();

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "data":        page_rules,
            "total":       total,
            "page":        page,
            "per_page":    per_page,
            "total_pages": total.div_ceil(per_page),
        })),
    ))
}

// ── POST /v1/rules ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateRuleBody {
    name:        String,
    description: Option<String>,
    trigger:     RuleTrigger,
    conditions:  Option<Vec<RuleCondition>>,
    actions:     Vec<RuleAction>,
    priority:    Option<u32>,
}

async fn create_rule(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<CreateRuleBody>,
) -> impl IntoResponse {
    if body.actions.is_empty() {
        return Err(AppError::Validation("rules must have at least one action".into()));
    }

    let rule = AutomationRule {
        id:          Uuid::new_v4(),
        tenant_id:   claims.tenant_id,
        name:        body.name,
        description: body.description.unwrap_or_default(),
        is_active:   true,
        trigger:     body.trigger,
        conditions:  body.conditions.unwrap_or_default(),
        actions:     body.actions,
        priority:    body.priority.unwrap_or(100),
        created_at:  Utc::now(),
    };

    state.pg_repo.create(&rule).await.map_err(AppError::internal)?;

    // Hot-reload the in-memory engine with the updated rule set.
    reload_from_db(&state).await?;

    Ok::<_, AppError>((StatusCode::CREATED, Json(serde_json::json!({"data": rule}))))
}

// ── GET /v1/rules/:id ─────────────────────────────────────────────────────────

async fn get_rule(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let rule = state.pg_repo
        .find_by_id(id)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Rule", id: id.to_string() })?;

    if rule.tenant_id != claims.tenant_id && rule.tenant_id != Uuid::nil() {
        return Err(AppError::Forbidden { resource: "rule".to_owned() });
    }

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"data": rule}))))
}

// ── PUT /v1/rules/:id ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct UpdateRuleBody {
    name:        String,
    description: Option<String>,
    trigger:     RuleTrigger,
    conditions:  Option<Vec<RuleCondition>>,
    actions:     Vec<RuleAction>,
    priority:    Option<u32>,
}

async fn update_rule(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRuleBody>,
) -> impl IntoResponse {
    let mut rule = state.pg_repo
        .find_by_id(id)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Rule", id: id.to_string() })?;

    if rule.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "rule".to_owned() });
    }

    rule.name        = body.name;
    rule.description = body.description.unwrap_or_default();
    rule.trigger     = body.trigger;
    rule.conditions  = body.conditions.unwrap_or_default();
    rule.actions     = body.actions;
    rule.priority    = body.priority.unwrap_or(rule.priority);

    let updated = state.pg_repo.update(&rule).await.map_err(AppError::internal)?;
    if !updated {
        return Err(AppError::NotFound { resource: "Rule", id: id.to_string() });
    }

    reload_from_db(&state).await?;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"data": rule}))))
}

// ── DELETE /v1/rules/:id ──────────────────────────────────────────────────────

async fn delete_rule(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let deleted = state.pg_repo
        .delete(id, claims.tenant_id)
        .await
        .map_err(AppError::internal)?;

    if !deleted {
        return Err(AppError::NotFound { resource: "Rule", id: id.to_string() });
    }

    reload_from_db(&state).await?;

    Ok::<_, AppError>(StatusCode::NO_CONTENT)
}

// ── PATCH /v1/rules/:id/toggle ────────────────────────────────────────────────

#[derive(Deserialize)]
struct ToggleBody {
    is_active: bool,
}

async fn toggle_rule(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<ToggleBody>,
) -> impl IntoResponse {
    let updated = state.pg_repo
        .set_active(id, claims.tenant_id, body.is_active)
        .await
        .map_err(AppError::internal)?;

    if !updated {
        return Err(AppError::NotFound { resource: "Rule", id: id.to_string() });
    }

    reload_from_db(&state).await?;

    let rule = state.pg_repo
        .find_by_id(id)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Rule", id: id.to_string() })?;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"data": rule}))))
}

// ── POST /v1/rules/reload ─────────────────────────────────────────────────────

async fn reload_rules(
    State(state): State<AppState>,
    _claims: AuthClaims,
) -> impl IntoResponse {
    let rules = reload_from_db(&state).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"rules_loaded": rules}))))
}

// ── GET /v1/rules/:id/executions ──────────────────────────────────────────────

#[derive(Deserialize)]
struct ExecutionsQuery {
    cursor: Option<DateTime<Utc>>,
    limit:  Option<i64>,
}

async fn list_executions(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Query(q): Query<ExecutionsQuery>,
) -> impl IntoResponse {
    // Verify rule belongs to this tenant.
    let rule = state.pg_repo
        .find_by_id(id)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Rule", id: id.to_string() })?;

    if rule.tenant_id != claims.tenant_id && rule.tenant_id != Uuid::nil() {
        return Err(AppError::Forbidden { resource: "rule".to_owned() });
    }

    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let rows = state.pg_repo
        .list_executions(id, limit, q.cursor)
        .await
        .map_err(AppError::internal)?;

    let next_cursor = rows.last().map(|r| r.fired_at.to_rfc3339());

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "data":        rows,
        "next_cursor": next_cursor,
    }))))
}

// ── POST /v1/rules/validate-expression ───────────────────────────────────────
// Validates a custom DSL expression without persisting it.
// Returns { valid, error } so the admin portal can give inline feedback
// before the operator saves a rule with a broken condition.

#[derive(Deserialize)]
struct ValidateExpressionBody {
    expression: String,
}

async fn validate_expression(
    _claims: AuthClaims,
    Json(body): Json<ValidateExpressionBody>,
) -> impl IntoResponse {
    use evalexpr::{ContextWithMutableVariables, HashMapContext, Value, eval_boolean_with_context};

    // Populate the same variables the rules engine exposes at runtime.
    let mut ctx = HashMapContext::new();
    let defaults: &[(&str, Value)] = &[
        ("service_type",         Value::String("standard".into())),
        ("zone",                 Value::String("zone-1".into())),
        ("current_day",          Value::String("Monday".into())),
        ("event_type",           Value::String("shipment.created".into())),
        ("current_hour",         Value::Int(10)),
        ("attempt_count",        Value::Int(1)),
        ("shipment_value_cents", Value::Int(50000)),
        ("has_customer",         Value::Boolean(true)),
        ("has_shipment",         Value::Boolean(true)),
        ("has_driver",           Value::Boolean(false)),
        ("has_merchant",         Value::Boolean(true)),
    ];
    for (k, v) in defaults {
        ctx.set_value((*k).into(), v.clone()).ok();
    }

    match eval_boolean_with_context(&body.expression, &ctx) {
        Ok(_) => Ok::<_, AppError>((
            StatusCode::OK,
            Json(serde_json::json!({ "valid": true })),
        )),
        Err(e) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "valid": false, "error": e.to_string() })),
        )),
    }
}

// ── POST /mcp ─────────────────────────────────────────────────────────────────
// JSON-RPC style: { "method": "tools/list" | "tools/call", "params": {...} }
// Consumed by the AI Intelligence Layer for rule management.

async fn mcp_handler(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let method = body.get("method").and_then(|v| v.as_str()).unwrap_or("");

    match method {
        "tools/list" => {
            let tools = serde_json::json!([
                {
                    "name": "list_automation_rules",
                    "description": "List automation rules for the tenant. Returns active/inactive rules with triggers, conditions, and actions.",
                    "inputSchema": { "type": "object", "properties": { "is_active": { "type": "boolean" } } }
                },
                {
                    "name": "create_automation_rule",
                    "description": "Create a new automation rule. Specify trigger (DeliveryFailed, DeliveryCompleted, ShipmentCreated, etc.), optional conditions, and one or more actions (NotifyCustomer, TriggerCampaign, RescheduleDelivery, etc.).",
                    "inputSchema": {
                        "type": "object",
                        "required": ["name", "trigger", "actions"],
                        "properties": {
                            "name":        { "type": "string" },
                            "description": { "type": "string" },
                            "trigger":     { "description": "e.g. 'DeliveryFailed' or {\"CustomerInactive\":{\"days\":30}}" },
                            "conditions":  { "type": "array" },
                            "actions":     { "type": "array", "description": "e.g. [{\"TriggerCampaign\":{\"campaign_id\":\"<uuid>\"}}]" },
                            "priority":    { "type": "integer" }
                        }
                    }
                },
                {
                    "name": "toggle_automation_rule",
                    "description": "Enable or disable an automation rule by ID.",
                    "inputSchema": {
                        "type": "object",
                        "required": ["rule_id", "is_active"],
                        "properties": {
                            "rule_id":   { "type": "string", "format": "uuid" },
                            "is_active": { "type": "boolean" }
                        }
                    }
                },
                {
                    "name": "get_rule_executions",
                    "description": "Retrieve recent execution history for an automation rule — outcome, topic, shipment, and any error.",
                    "inputSchema": {
                        "type": "object",
                        "required": ["rule_id"],
                        "properties": {
                            "rule_id": { "type": "string", "format": "uuid" },
                            "limit":   { "type": "integer", "default": 20 }
                        }
                    }
                }
            ]);
            (StatusCode::OK, Json(serde_json::json!({"tools": tools}))).into_response()
        }
        "tools/call" => {
            let params    = body.get("params").unwrap_or(&serde_json::Value::Null);
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args      = params.get("arguments").unwrap_or(&serde_json::Value::Null);

            let result: Result<serde_json::Value, String> = async {
                match tool_name {
                    "list_automation_rules" => {
                        let is_active = args["is_active"].as_bool();
                        let mut rules = state.pg_repo
                            .load_for_tenant(claims.tenant_id)
                            .await
                            .map_err(|e| format!("DB error: {e}"))?;
                        if let Some(active) = is_active {
                            rules.retain(|r| r.is_active == active);
                        }
                        serde_json::to_value(&rules).map_err(|e| e.to_string())
                    }
                    "create_automation_rule" => {
                        let name = args["name"].as_str()
                            .ok_or_else(|| "name required".to_owned())?
                            .to_owned();
                        let trigger: RuleTrigger = serde_json::from_value(args["trigger"].clone())
                            .map_err(|e| format!("invalid trigger: {e}"))?;
                        let actions: Vec<RuleAction> = serde_json::from_value(args["actions"].clone())
                            .map_err(|e| format!("invalid actions: {e}"))?;
                        if actions.is_empty() {
                            return Err("actions must not be empty".to_owned());
                        }
                        let conditions: Vec<RuleCondition> = args["conditions"]
                            .as_array()
                            .map(|arr| serde_json::from_value(serde_json::Value::Array(arr.clone())).unwrap_or_default())
                            .unwrap_or_default();
                        let rule = AutomationRule {
                            id:          Uuid::new_v4(),
                            tenant_id:   claims.tenant_id,
                            name,
                            description: args["description"].as_str().unwrap_or("").to_owned(),
                            is_active:   true,
                            trigger,
                            conditions,
                            actions,
                            priority:    args["priority"].as_u64().unwrap_or(100) as u32,
                            created_at:  Utc::now(),
                        };
                        state.pg_repo.create(&rule).await.map_err(|e| format!("DB error: {e}"))?;
                        reload_from_db(&state).await.map_err(|e| e.to_string())?;
                        serde_json::to_value(&rule).map_err(|e| e.to_string())
                    }
                    "toggle_automation_rule" => {
                        let rule_id = args["rule_id"].as_str()
                            .ok_or_else(|| "rule_id required".to_owned())?
                            .parse::<Uuid>()
                            .map_err(|_| "invalid rule_id UUID".to_owned())?;
                        let is_active = args["is_active"].as_bool()
                            .ok_or_else(|| "is_active required".to_owned())?;
                        let updated = state.pg_repo
                            .set_active(rule_id, claims.tenant_id, is_active)
                            .await
                            .map_err(|e| format!("DB error: {e}"))?;
                        if !updated {
                            return Err(format!("rule {rule_id} not found"));
                        }
                        reload_from_db(&state).await.map_err(|e| e.to_string())?;
                        Ok(serde_json::json!({"rule_id": rule_id, "is_active": is_active, "updated": true}))
                    }
                    "get_rule_executions" => {
                        let rule_id = args["rule_id"].as_str()
                            .ok_or_else(|| "rule_id required".to_owned())?
                            .parse::<Uuid>()
                            .map_err(|_| "invalid rule_id UUID".to_owned())?;
                        let limit = args["limit"].as_i64().unwrap_or(20).clamp(1, 100);
                        let rows = state.pg_repo
                            .list_executions(rule_id, limit, None)
                            .await
                            .map_err(|e| format!("DB error: {e}"))?;
                        serde_json::to_value(&rows).map_err(|e| e.to_string())
                    }
                    other => Err(format!("Unknown tool: {other}")),
                }
            }.await;

            match result {
                Ok(value) => (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "content": [{ "type": "text", "text": value.to_string() }]
                    })),
                ).into_response(),
                Err(e) => (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": e })),
                ).into_response(),
            }
        }
        other => {
            let payload = serde_json::json!({ "error": format!("Unknown method: {other}") });
            (StatusCode::BAD_REQUEST, Json(payload)).into_response()
        }
    }
}

// ── GET /health ───────────────────────────────────────────────────────────────

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Reload all rules from DB into the in-memory engine. Returns rule count.
async fn reload_from_db(state: &AppState) -> Result<usize, AppError> {
    let rules = state.pg_repo.load_all().await.map_err(AppError::internal)?;
    let count = rules.len();
    state.rule_repo.reload(rules).await;
    tracing::info!(count, "Rules engine hot-reloaded");
    Ok(count)
}
