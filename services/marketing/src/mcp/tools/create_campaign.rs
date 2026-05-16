use serde_json::{json, Value};
use std::sync::Arc;
use logisticos_auth::rbac::permissions;
use logisticos_types::TenantId;
use crate::AppState;
use crate::application::services::CreateCampaignCommand;
use crate::mcp::context::McpContext;

pub async fn handle(args: &Value, ctx: &McpContext, state: &Arc<AppState>) -> Result<Value, String> {
    if !ctx.has_permission(permissions::CAMPAIGNS_CREATE) {
        return Err("Insufficient permissions: campaigns:create required".to_string());
    }

    let tenant_id = TenantId::from_uuid(ctx.tenant_id);

    let cmd: CreateCampaignCommand = serde_json::from_value(args.clone())
        .map_err(|e| format!("Invalid campaign payload: {e}"))?;

    let campaign = state.campaign_svc
        .create(&tenant_id, ctx.actor_uid, cmd)
        .await
        .map_err(|e| format!("Failed to create campaign: {e}"))?;

    Ok(json!({ "campaign": campaign }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name":        { "type": "string", "description": "Human-readable campaign name" },
            "description": { "type": "string", "description": "Optional trigger or purpose description" },
            "channel": {
                "type": "string",
                "enum": ["whatsapp", "sms", "email", "push"],
                "description": "Delivery channel"
            },
            "template": {
                "type": "object",
                "description": "Message template — use template_id for registry templates or inline_* with variables.body for ad-hoc messages",
                "properties": {
                    "template_id": { "type": "string" },
                    "subject":     { "type": "string", "description": "Email subject (email channel only)" },
                    "variables":   { "type": "object" }
                },
                "required": ["template_id", "variables"]
            },
            "targeting": {
                "type": "object",
                "description": "Audience targeting — provide recipients for direct sends or min_clv_score for CDP-resolved sends",
                "properties": {
                    "min_clv_score":    { "type": "number", "description": "Minimum CLV score (0–100); triggers CDP resolution at activation" },
                    "last_active_days": { "type": "integer", "description": "Only include customers active within this many days" },
                    "customer_ids":     { "type": "array", "items": { "type": "string" } },
                    "recipients": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "customer_id": { "type": "string" },
                                "phone":       { "type": "string" },
                                "email":       { "type": "string" },
                                "name":        { "type": "string" }
                            }
                        }
                    },
                    "estimated_reach": { "type": "integer" }
                },
                "required": ["customer_ids", "estimated_reach"]
            }
        },
        "required": ["name", "channel", "template", "targeting"]
    })
}
