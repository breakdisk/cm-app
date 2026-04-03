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
    routing::{delete, get, patch, post, put},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
        .route("/v1/rules",              get(list_rules).post(create_rule))
        .route("/v1/rules/reload",       post(reload_rules))
        .route("/v1/rules/:id",          get(get_rule).put(update_rule).delete(delete_rule))
        .route("/v1/rules/:id/toggle",   patch(toggle_rule))
        .route("/v1/rules/:id/executions", get(list_executions))
        .route("/health",                get(health))
        .route("/ready",                 get(health))
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
            "total_pages": (total + per_page - 1) / per_page,
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
