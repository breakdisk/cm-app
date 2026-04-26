pub mod health;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;

use crate::AppState;
use crate::application::services::{
    CreateWebhookCommand, CreateWebhookResult, UpdateWebhookCommand,
};
use crate::domain::entities::Webhook;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/webhooks",       get(list_webhooks).post(create_webhook))
        .route("/v1/webhooks/:id",   get(get_webhook).put(update_webhook).delete(delete_webhook))
}

/// Public DTO — same shape as the entity but `secret` is omitted. Returned
/// from every endpoint EXCEPT create-success (which surfaces it once via
/// CreateWebhookResult).
#[derive(serde::Serialize)]
struct WebhookDto<'a> {
    id:                Uuid,
    tenant_id:         Uuid,
    url:               &'a str,
    events:            &'a [String],
    status:            &'a str,
    description:       Option<&'a str>,
    success_count:     i64,
    fail_count:        i64,
    last_delivery_at:  Option<chrono::DateTime<chrono::Utc>>,
    last_status_code:  Option<i32>,
    created_at:        chrono::DateTime<chrono::Utc>,
    updated_at:        chrono::DateTime<chrono::Utc>,
}

impl<'a> From<&'a Webhook> for WebhookDto<'a> {
    fn from(w: &'a Webhook) -> Self {
        WebhookDto {
            id:               w.id,
            tenant_id:        w.tenant_id,
            url:              &w.url,
            events:           &w.events,
            status:           w.status.as_str(),
            description:      w.description.as_deref(),
            success_count:    w.success_count,
            fail_count:       w.fail_count,
            last_delivery_at: w.last_delivery_at,
            last_status_code: w.last_status_code,
            created_at:       w.created_at,
            updated_at:       w.updated_at,
        }
    }
}

async fn list_webhooks(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::WEBHOOKS_READ);
    let webhooks = state.webhook_svc.list(claims.tenant_id).await?;
    let dtos: Vec<WebhookDto<'_>> = webhooks.iter().map(WebhookDto::from).collect();
    Ok(Json(serde_json::json!({ "data": dtos, "count": dtos.len() })))
}

async fn create_webhook(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Json(cmd): Json<CreateWebhookCommand>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::WEBHOOKS_MANAGE);
    let webhook = state.webhook_svc.create(claims.tenant_id, cmd).await?;
    let dto = WebhookDto::from(&webhook);
    let result = CreateWebhookResult {
        webhook: webhook.clone(),
        secret:  webhook.secret.clone(),
    };
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "data": dto,
            // Returned exactly once. Admins must store it now — get_webhook
            // and list_webhooks omit it forever.
            "secret": result.secret,
        })),
    ))
}

async fn get_webhook(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::WEBHOOKS_READ);
    let w = state.webhook_svc.get(claims.tenant_id, id).await?;
    Ok(Json(serde_json::json!({ "data": WebhookDto::from(&w) })))
}

async fn update_webhook(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateWebhookCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::WEBHOOKS_MANAGE);
    let w = state.webhook_svc.update(claims.tenant_id, id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": WebhookDto::from(&w) })))
}

async fn delete_webhook(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    require_permission!(claims, logisticos_auth::rbac::permissions::WEBHOOKS_MANAGE);
    state.webhook_svc.delete(claims.tenant_id, id).await?;
    Ok::<_, AppError>(StatusCode::NO_CONTENT)
}
