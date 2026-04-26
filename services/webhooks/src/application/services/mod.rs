use std::sync::Arc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use logisticos_errors::{AppError, AppResult};

use crate::domain::{
    entities::{Webhook, WebhookStatus},
    repositories::WebhookRepository,
};

pub struct WebhookService {
    repo: Arc<dyn WebhookRepository>,
}

impl WebhookService {
    pub fn new(repo: Arc<dyn WebhookRepository>) -> Self { Self { repo } }

    pub fn repo_ref(&self) -> Arc<dyn WebhookRepository> { Arc::clone(&self.repo) }

    pub async fn create(
        &self,
        tenant_id: Uuid,
        cmd: CreateWebhookCommand,
    ) -> AppResult<Webhook> {
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;
        validate_url(&cmd.url)?;
        validate_events(&cmd.events)?;

        let now = chrono::Utc::now();
        let webhook = Webhook {
            id:                Uuid::new_v4(),
            tenant_id,
            url:               cmd.url,
            events:            cmd.events,
            // 32 bytes of randomness, hex-encoded → 64-char secret. Ample for HMAC-SHA256.
            secret:            generate_secret(),
            status:            WebhookStatus::Active,
            description:       cmd.description,
            success_count:     0,
            fail_count:        0,
            last_delivery_at:  None,
            last_status_code:  None,
            created_at:        now,
            updated_at:        now,
        };
        self.repo.save(&webhook).await.map_err(AppError::internal)?;
        tracing::info!(webhook_id = %webhook.id, tenant_id = %tenant_id, url = %webhook.url, "Webhook created");
        Ok(webhook)
    }

    pub async fn get(&self, tenant_id: Uuid, id: Uuid) -> AppResult<Webhook> {
        let w = self.repo.find_by_id(id).await.map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Webhook", id: id.to_string() })?;
        // Tenant isolation: 404 not 403 so cross-tenant existence isn't leaked.
        if w.tenant_id != tenant_id {
            return Err(AppError::NotFound { resource: "Webhook", id: id.to_string() });
        }
        Ok(w)
    }

    pub async fn list(&self, tenant_id: Uuid) -> AppResult<Vec<Webhook>> {
        self.repo.list_by_tenant(tenant_id).await.map_err(AppError::internal)
    }

    pub async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        cmd: UpdateWebhookCommand,
    ) -> AppResult<Webhook> {
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;
        let mut w = self.get(tenant_id, id).await?;
        if let Some(url) = cmd.url {
            validate_url(&url)?;
            w.url = url;
        }
        if let Some(events) = cmd.events {
            validate_events(&events)?;
            w.events = events;
        }
        if let Some(status_str) = cmd.status {
            w.status = WebhookStatus::parse(&status_str)
                .ok_or_else(|| AppError::Validation(format!("Invalid status '{status_str}'")))?;
        }
        if let Some(desc) = cmd.description { w.description = Some(desc); }
        w.updated_at = chrono::Utc::now();
        self.repo.save(&w).await.map_err(AppError::internal)?;
        Ok(w)
    }

    pub async fn delete(&self, tenant_id: Uuid, id: Uuid) -> AppResult<()> {
        // Run get() first for the tenant guard — refuses cross-tenant deletes.
        self.get(tenant_id, id).await?;
        self.repo.delete(id).await.map_err(AppError::internal)?;
        tracing::info!(webhook_id = %id, tenant_id = %tenant_id, "Webhook deleted");
        Ok(())
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateWebhookCommand {
    #[validate(length(min = 8, max = 2048))]
    pub url:         String,
    pub events:      Vec<String>,
    #[validate(length(max = 500))]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateWebhookCommand {
    #[validate(length(min = 8, max = 2048))]
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub events: Option<Vec<String>>,
    /// "active" | "disabled"
    #[serde(default)]
    pub status: Option<String>,
    #[validate(length(max = 500))]
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateWebhookResult {
    pub webhook: Webhook,
    /// Plaintext signing secret returned exactly once at create time.
    /// The admin must persist this — subsequent reads omit it.
    pub secret: String,
}

fn validate_url(url: &str) -> AppResult<()> {
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(AppError::Validation("URL must be http:// or https://".into()));
    }
    Ok(())
}

fn validate_events(events: &[String]) -> AppResult<()> {
    if events.is_empty() {
        return Err(AppError::Validation("events array cannot be empty (use [\"*\"] for all)".into()));
    }
    for e in events {
        if e == "*" { continue; }
        // Format: source.event (e.g. "shipment.created").
        // Reject obvious garbage but stay permissive — the dispatcher does
        // exact-match against whatever event names producers actually emit.
        if e.is_empty() || e.len() > 100 || !e.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-') {
            return Err(AppError::Validation(format!("Invalid event name '{e}'")));
        }
    }
    Ok(())
}

fn generate_secret() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
