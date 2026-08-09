//! HTTP API layer for the Engagement service.
//!
//! Exposes REST endpoints for notification dispatch, template management, and
//! campaign read-access. All mutating routes require JWT authentication and
//! appropriate RBAC permissions.  Prometheus metrics are served at `/metrics`;
//! liveness and readiness probes at `/health` and `/ready`.

pub mod webhook;

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::{
    application::services::notification_service::NotificationService,
    domain::entities::{
        notification::NotificationPriority,
        template::{NotificationChannel, NotificationTemplate},
    },
    infrastructure::db::NotificationDb,
};

// ---------------------------------------------------------------------------
// Permission constants
// Engagement service permissions follow the `<resource>:<action>` convention.
// ---------------------------------------------------------------------------

const PERM_SEND:             &str = "engagement:send";
const PERM_READ:             &str = "engagement:read";
const PERM_TEMPLATES_WRITE:  &str = "engagement:templates:write";

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub notification_svc: Arc<NotificationService>,
    pub db: PgPool,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SendNotificationRequest {
    pub customer_id:   Uuid,
    /// "whatsapp" | "sms" | "email" | "push"
    pub channel:       String,
    pub template_id:   Uuid,
    pub template_vars: HashMap<String, String>,
    /// "high" | "normal" | "low"  (defaults to "normal")
    #[serde(default = "default_priority_str")]
    pub priority:      String,
}

fn default_priority_str() -> String { "normal".into() }

#[derive(Debug, Deserialize)]
pub struct CreateTemplateRequest {
    pub name:          String,
    pub channel:       String,
    pub subject:       Option<String>,
    /// Handlebars-style body template — variables delimited by `{{var}}`
    pub body_template: String,
    pub variables:     Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTemplateRequest {
    pub name:          Option<String>,
    pub subject:       Option<String>,
    pub body_template: Option<String>,
    pub variables:     Option<Vec<String>>,
    pub is_active:     Option<bool>,
}

/// Query parameters for `GET /v1/notifications`
#[derive(Debug, Deserialize)]
pub struct ListNotificationsQuery {
    pub customer_id: Option<Uuid>,
    pub status:      Option<String>,
    #[serde(default = "default_page")]
    pub page:        u64,
    #[serde(default = "default_limit")]
    pub limit:       u64,
}

fn default_page()  -> u64 { 1  }
fn default_limit() -> u64 { 20 }

impl ListNotificationsQuery {
    fn clamp_limit(&self) -> i64 { self.limit.clamp(1, 100) as i64 }
    fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.limit.clamp(1, 100)) as i64
    }
}

/// Query parameters for `GET /v1/campaigns`
#[derive(Debug, Deserialize)]
pub struct ListCampaignsQuery {
    #[serde(default = "default_page")]
    pub page:  u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
}

// ---------------------------------------------------------------------------
// Helpers: parse channel / priority strings to domain enums
// ---------------------------------------------------------------------------

fn parse_channel(s: &str) -> Result<NotificationChannel, AppError> {
    match s {
        "whatsapp"  => Ok(NotificationChannel::WhatsApp),
        "sms"       => Ok(NotificationChannel::Sms),
        "email"     => Ok(NotificationChannel::Email),
        "push"      => Ok(NotificationChannel::Push),
        "messenger" => Ok(NotificationChannel::Messenger),
        "telegram"  => Ok(NotificationChannel::Telegram),
        "x"         => Ok(NotificationChannel::X),
        "viber"     => Ok(NotificationChannel::Viber),
        "wechat"    => Ok(NotificationChannel::WeChat),
        "line"      => Ok(NotificationChannel::Line),
        "slack"     => Ok(NotificationChannel::Slack),
        other => Err(AppError::Validation(format!(
            "Unknown channel '{}': must be one of whatsapp, sms, email, push, messenger, telegram, x, viber, wechat, line, slack", other
        ))),
    }
}

fn parse_priority(s: &str) -> NotificationPriority {
    match s {
        "high" => NotificationPriority::High,
        "low"  => NotificationPriority::Low,
        _      => NotificationPriority::Normal,
    }
}

// ---------------------------------------------------------------------------
// Notification handlers
// ---------------------------------------------------------------------------

/// `POST /v1/notifications/send`
///
/// Loads the named template from the database, renders it with the supplied
/// variables, dispatches the notification through the appropriate channel
/// adapter, and persists the result.
async fn send_notification(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<SendNotificationRequest>,
) -> impl IntoResponse {
    claims.require_permission(PERM_SEND)?;

    let tenant_uuid = claims.tenant_id;
    let db          = NotificationDb::new(state.db.clone());
    let channel     = parse_channel(&req.channel)?;
    let priority    = parse_priority(&req.priority);

    // Load template — tenant-scoped plus global templates (tenant_id IS NULL).
    let template = db
        .find_template_by_id(req.template_id, tenant_uuid)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound {
            resource: "template",
            id: req.template_id.to_string(),
        })?;

    if !template.is_active {
        return Err(AppError::BusinessRule(
            "Template is inactive and cannot be used for dispatch".into(),
        ));
    }

    // Validate that the template channel matches the requested channel.
    if template.channel != channel {
        return Err(AppError::Validation(format!(
            "Template channel '{}' does not match request channel '{}'",
            template.channel.as_str(),
            channel.as_str(),
        )));
    }

    // Build template variables as JSON for rendering.
    let vars_json = serde_json::to_value(&req.template_vars)
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // The `recipient` field (phone number or email address) must be provided
    // in template_vars.  Production will resolve this from the CDP.
    let recipient = req
        .template_vars
        .get("recipient")
        .cloned()
        .unwrap_or_default();

    if recipient.is_empty() {
        return Err(AppError::Validation(
            "template_vars must include 'recipient' (phone or email address)".into(),
        ));
    }

    let mut notification = NotificationService::build_from_template(
        &template,
        tenant_uuid,
        req.customer_id,
        recipient,
        &vars_json,
        priority,
    )?;

    // Persist before dispatch so the record exists even if dispatch fails.
    db.insert_notification(&notification)
        .await
        .map_err(AppError::Internal)?;

    state.notification_svc.dispatch(&mut notification).await?;

    // Update status after dispatch completes (sent or failed).
    db.update_status(
        notification.id,
        notification.status,
        notification.provider_message_id.clone(),
    )
    .await
    .map_err(AppError::Internal)?;

    Ok::<_, AppError>((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "id":     notification.id,
            "status": notification.status,
        })),
    ))
}

/// `GET /v1/notifications/:id`
async fn get_notification(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(PERM_READ)?;

    let db = NotificationDb::new(state.db.clone());
    let notification = db
        .find_by_id(id)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "notification", id: id.to_string() })?;

    // Tenant isolation — a notification from another tenant must not be visible.
    if notification.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "notification".into() });
    }

    Ok::<_, AppError>((StatusCode::OK, Json(notification)))
}

/// `GET /v1/notifications?customer_id=&status=&page=&limit=`
async fn list_notifications(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListNotificationsQuery>,
) -> impl IntoResponse {
    claims.require_permission(PERM_READ)?;

    let tenant_uuid = claims.tenant_id;
    let db          = NotificationDb::new(state.db.clone());

    let notifications = if let Some(customer_id) = q.customer_id {
        db.list_by_customer(customer_id, tenant_uuid, q.clamp_limit(), q.offset())
            .await
            .map_err(AppError::Internal)?
    } else {
        db.list_by_tenant(tenant_uuid, q.status.as_deref(), q.clamp_limit(), q.offset())
            .await
            .map_err(AppError::Internal)?
    };

    let count = notifications.len();
    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "notifications": notifications,
            "page":          q.page,
            "limit":         q.clamp_limit(),
            "count":         count,
        })),
    ))
}

// ---------------------------------------------------------------------------
// Template handlers
// ---------------------------------------------------------------------------

/// `POST /v1/templates`
async fn create_template(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<CreateTemplateRequest>,
) -> impl IntoResponse {
    claims.require_permission(PERM_TEMPLATES_WRITE)?;

    let channel = parse_channel(&req.channel)?;

    if req.body_template.trim().is_empty() {
        return Err(AppError::Validation("body_template must not be empty".into()));
    }

    let template = NotificationTemplate {
        id:          Uuid::new_v4(),
        tenant_id:   Some(claims.tenant_id),
        template_id: req.name.to_lowercase().replace(' ', "_"),
        channel,
        language:    "en".into(),
        subject:     req.subject,
        body:        req.body_template,
        variables:   req.variables,
        is_active:   true,
    };

    let db = NotificationDb::new(state.db.clone());
    db.insert_template(&template)
        .await
        .map_err(AppError::Internal)?;

    Ok::<_, AppError>((StatusCode::CREATED, Json(template)))
}

/// `GET /v1/templates`
async fn list_templates(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    claims.require_permission(PERM_READ)?;

    let db = NotificationDb::new(state.db.clone());
    let templates = db
        .list_templates(claims.tenant_id)
        .await
        .map_err(AppError::Internal)?;

    let count = templates.len();
    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "templates": templates,
            "count":     count,
        })),
    ))
}

/// `GET /v1/templates/:id`
async fn get_template(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(PERM_READ)?;

    let db = NotificationDb::new(state.db.clone());
    let template = db
        .find_template_by_id(id, claims.tenant_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "template", id: id.to_string() })?;

    Ok::<_, AppError>((StatusCode::OK, Json(template)))
}

/// `PUT /v1/templates/:id`
async fn update_template(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTemplateRequest>,
) -> impl IntoResponse {
    claims.require_permission(PERM_TEMPLATES_WRITE)?;

    let db = NotificationDb::new(state.db.clone());
    let mut template = db
        .find_template_by_id(id, claims.tenant_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "template", id: id.to_string() })?;

    // Only tenant-owned templates may be mutated; global templates (tenant_id
    // IS NULL) are read-only from the API layer.
    if template.tenant_id != Some(claims.tenant_id) {
        return Err(AppError::Forbidden { resource: "template".into() });
    }

    // Apply partial updates — None fields leave the existing value untouched.
    if let Some(name) = req.name {
        template.template_id = name.to_lowercase().replace(' ', "_");
    }
    if let Some(subject) = req.subject {
        template.subject = Some(subject);
    }
    if let Some(body) = req.body_template {
        if body.trim().is_empty() {
            return Err(AppError::Validation("body_template must not be empty".into()));
        }
        template.body = body;
    }
    if let Some(vars) = req.variables {
        template.variables = vars;
    }
    if let Some(active) = req.is_active {
        template.is_active = active;
    }

    db.update_template(&template)
        .await
        .map_err(AppError::Internal)?;

    Ok::<_, AppError>((StatusCode::OK, Json(template)))
}

// ---------------------------------------------------------------------------
// Campaign handlers  (read-only — write ops live in the marketing service)
// ---------------------------------------------------------------------------

/// `GET /v1/campaigns`
///
/// Returns a lightweight campaign projection from the `engagement.campaigns`
/// view, letting the engagement frontend display send status alongside
/// notification activity without calling the marketing service directly.
async fn list_campaigns(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListCampaignsQuery>,
) -> impl IntoResponse {
    claims.require_permission(PERM_READ)?;

    let limit  = q.limit.clamp(1, 100) as i64;
    let offset = ((q.page.saturating_sub(1)) * q.limit.clamp(1, 100)) as i64;

    let db = NotificationDb::new(state.db.clone());
    let campaigns = db
        .list_campaigns(claims.tenant_id, limit, offset)
        .await
        .map_err(AppError::Internal)?;

    let count = campaigns.len();
    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "campaigns": campaigns,
            "page":      q.page,
            "limit":     limit,
            "count":     count,
        })),
    ))
}

/// `GET /v1/campaigns/:id`
async fn get_campaign(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(PERM_READ)?;

    let db = NotificationDb::new(state.db.clone());
    let campaign = db
        .find_campaign_by_id(id, claims.tenant_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "campaign", id: id.to_string() })?;

    Ok::<_, AppError>((StatusCode::OK, Json(campaign)))
}

/// `GET /v1/campaigns/:id/sends?page=&limit=`
///
/// Returns per-recipient send rows for one campaign.  Useful for Merchant Portal
/// drill-down: see exactly which recipients were sent to, which failed, and any
/// provider error messages — without requiring a separate query to the marketing
/// service.
async fn list_campaign_sends(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Query(q): Query<ListCampaignsQuery>,
) -> impl IntoResponse {
    claims.require_permission(PERM_READ)?;

    let limit  = q.limit.clamp(1, 200) as i64;
    let offset = ((q.page.saturating_sub(1)) * q.limit.clamp(1, 200)) as i64;

    let db = NotificationDb::new(state.db.clone());

    // Verify the campaign exists and belongs to this tenant before listing sends.
    let campaign = db
        .find_campaign_by_id(id, claims.tenant_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "campaign", id: id.to_string() })?;

    let sends = db
        .list_campaign_sends(id, claims.tenant_id, limit, offset)
        .await
        .map_err(AppError::Internal)?;

    let count = sends.len();
    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "campaign": campaign,
            "sends":    sends,
            "page":     q.page,
            "limit":    limit,
            "count":    count,
        })),
    ))
}

// ---------------------------------------------------------------------------
// Customer communication history
// ---------------------------------------------------------------------------

/// `GET /v1/customers/:customer_id/sends?page=&limit=`
///
/// Returns all campaign sends addressed to a specific customer across all campaigns.
/// Powers the "Communication History" tab on the Merchant Portal customer detail page.
async fn customer_send_history(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(customer_id): Path<Uuid>,
    Query(q): Query<ListCampaignsQuery>,
) -> impl IntoResponse {
    claims.require_permission(PERM_READ)?;

    let limit  = q.limit.clamp(1, 200) as i64;
    let offset = ((q.page.saturating_sub(1)) * q.limit.clamp(1, 200)) as i64;

    let db   = NotificationDb::new(state.db.clone());
    let rows = db
        .campaign_send_history_for_customer(customer_id, claims.tenant_id, limit, offset)
        .await
        .map_err(AppError::Internal)?;

    let count = rows.len();
    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "customer_id": customer_id,
            "sends":       rows,
            "page":        q.page,
            "limit":       limit,
            "count":       count,
        })),
    ))
}

// ---------------------------------------------------------------------------
// Observability handlers
// ---------------------------------------------------------------------------

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "engagement" }))
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    // Light-weight connectivity check against the database pool.
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "ready", "database": "ok" })),
        ),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "degraded", "database": e.to_string() })),
        ),
    }
}

async fn metrics() -> &'static str {
    // Production: use `metrics-exporter-prometheus` and return `handle.render()`.
    "# HELP engagement_notifications_total Total notifications dispatched\n\
     # TYPE engagement_notifications_total counter\n\
     engagement_notifications_total 0\n\
     # HELP engagement_notifications_failed_total Total notification failures\n\
     # TYPE engagement_notifications_failed_total counter\n\
     engagement_notifications_failed_total 0\n"
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Builds the Axum router for the engagement service.
/// Pass the fully-constructed `AppState` during bootstrap.
/// Liveness/readiness/metrics, deliberately kept out of `router()`.
///
/// `router()` is mounted behind `require_auth` in bootstrap, and a probe cannot
/// present a JWT — so a health route in there answers 401 forever, which is
/// what this service had been doing.
/// Takes state because `/ready` is a real check — it runs `SELECT 1` against
/// the pool, which is what makes it a readiness probe rather than a liveness one.
pub fn observability_router(state: AppState) -> Router {
    Router::new()
        .route("/health",  get(health))
        .route("/ready",   get(ready))
        .route("/metrics", get(metrics))
        .with_state(state)
}

pub fn router(state: AppState) -> Router {
    Router::new()
        // ── Notifications ───────────────────────────────────────────
        .route("/v1/notifications/send", post(send_notification))
        .route("/v1/notifications/:id",  get(get_notification))
        .route("/v1/notifications",      get(list_notifications))
        // ── Templates ───────────────────────────────────────────────
        .route("/v1/templates",          post(create_template).get(list_templates))
        .route("/v1/templates/:id",      get(get_template).put(update_template))
        // ── Campaigns (read-only projection) ────────────────────────
        .route("/v1/campaigns",              get(list_campaigns))
        .route("/v1/campaigns/:id",          get(get_campaign))
        .route("/v1/campaigns/:id/sends",    get(list_campaign_sends))
        // ── Customer communication history ──────────────────────────
        .route("/v1/customers/:customer_id/sends", get(customer_send_history))
        .with_state(state)
}

/// Internal service-to-service routes — no JWT auth required (Istio mTLS enforces
/// caller identity), mirroring the `/v1/internal/*` convention used by other services.
pub fn internal_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/v1/internal/campaign-sends/:campaign_id/:customer_id",
            get(internal_campaign_engagement),
        )
        .with_state(state)
}

/// `GET /v1/internal/campaign-sends/:campaign_id/:customer_id`
/// Used by the marketing service's journey executor to evaluate "condition"
/// steps (has this customer opened/clicked a given campaign send?).
async fn internal_campaign_engagement(
    State(state): State<AppState>,
    Path((campaign_id, customer_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    let db = NotificationDb::new(state.db.clone());
    let (opened, clicked) = db
        .campaign_engagement_for_customer(campaign_id, customer_id)
        .await
        .map_err(AppError::Internal)?
        .unwrap_or((false, false));

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "campaign_id": campaign_id,
            "customer_id": customer_id,
            "opened":      opened,
            "clicked":     clicked,
        })),
    ))
}
