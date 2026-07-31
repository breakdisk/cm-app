use serde_json::{json, Value};
use std::sync::Arc;
use logisticos_auth::rbac::permissions;
use crate::AppState;
use crate::mcp::context::McpContext;

pub async fn handle(args: &Value, ctx: &McpContext, state: &Arc<AppState>) -> Result<Value, String> {
    if !ctx.has_permission(permissions::CAMPAIGNS_SEND) {
        return Err("Insufficient permissions: campaigns:send required".to_string());
    }

    let id: uuid::Uuid = args
        .get("campaign_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required argument: campaign_id")?
        .parse()
        .map_err(|_| "campaign_id must be a valid UUID".to_string())?;

    // CDP resolution for MCP path: if campaign has CDP criteria and no recipients,
    // resolve via the CDP client before activating (mirrors the HTTP handler flow).
    if let Some(ref cdp) = state.cdp_client {
        let campaign = state.campaign_svc
            .get(id)
            .await
            .map_err(|e| format!("Failed to fetch campaign: {e}"))?;

        // Enforce tenant isolation: a token from tenant A must not trigger CDP
        // resolution on tenant B's campaign even if it knows the UUID.
        if campaign.tenant_id != logisticos_types::TenantId::from_uuid(ctx.tenant_id) {
            return Err("Campaign not found".to_string());
        }

        if campaign.targeting.recipients.is_empty()
            && (campaign.targeting.min_clv_score.is_some()
                || !campaign.targeting.customer_ids.is_empty())
        {
            let recipients = cdp
                .resolve_audience(ctx.tenant_id, &campaign.targeting)
                .await
                .map_err(|e| format!("CDP audience resolution failed: {e}"))?;

            state.campaign_svc
                .patch_recipients(id, recipients)
                .await
                .map_err(|e| format!("Failed to update campaign recipients: {e}"))?;
        }
    }

    let campaign = state.campaign_svc
        .activate(id, None)
        .await
        .map_err(|e| format!("Failed to activate campaign: {e}"))?;

    Ok(json!({ "campaign": campaign }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "campaign_id": {
                "type": "string",
                "format": "uuid",
                "description": "UUID of the campaign to activate"
            }
        },
        "required": ["campaign_id"]
    })
}
