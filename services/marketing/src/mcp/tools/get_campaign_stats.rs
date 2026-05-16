use serde_json::{json, Value};
use std::sync::Arc;
use logisticos_auth::rbac::permissions;
use logisticos_types::TenantId;
use crate::AppState;
use crate::mcp::context::McpContext;

pub async fn handle(args: &Value, ctx: &McpContext, state: &Arc<AppState>) -> Result<Value, String> {
    // Stats are visible to anyone who can send campaigns (includes campaign
    // managers who don't have the create permission but need to monitor sends).
    if !ctx.has_permission(permissions::CAMPAIGNS_CREATE)
        && !ctx.has_permission(permissions::CAMPAIGNS_SEND)
    {
        return Err("Insufficient permissions: campaigns:create or campaigns:send required".to_string());
    }

    let id: uuid::Uuid = args
        .get("campaign_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required argument: campaign_id")?
        .parse()
        .map_err(|_| "campaign_id must be a valid UUID".to_string())?;

    let campaign = state.campaign_svc
        .get(id)
        .await
        .map_err(|e| format!("Campaign not found: {e}"))?;

    // Verify tenant isolation.
    if campaign.tenant_id != TenantId::from_uuid(ctx.tenant_id) {
        return Err("Campaign not found".to_string());
    }

    let delivery_rate = if campaign.total_sent > 0 {
        (campaign.total_delivered as f64 / campaign.total_sent as f64) * 100.0
    } else {
        0.0
    };

    Ok(json!({
        "campaign": campaign,
        "summary": {
            "delivery_rate_pct": delivery_rate,
            "total_sent":        campaign.total_sent,
            "total_delivered":   campaign.total_delivered,
            "total_failed":      campaign.total_failed,
            "estimated_reach":   campaign.targeting.estimated_reach,
        }
    }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "campaign_id": {
                "type": "string",
                "format": "uuid",
                "description": "UUID of the campaign to inspect"
            }
        },
        "required": ["campaign_id"]
    })
}
