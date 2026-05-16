use serde_json::{json, Value};
use std::sync::Arc;
use logisticos_auth::rbac::permissions;
use logisticos_types::TenantId;
use crate::AppState;
use crate::mcp::context::McpContext;

pub async fn handle(args: &Value, ctx: &McpContext, state: &Arc<AppState>) -> Result<Value, String> {
    if !ctx.has_permission(permissions::CAMPAIGNS_CREATE) {
        return Err("Insufficient permissions: campaigns:create required".to_string());
    }

    let tenant_id = TenantId::from_uuid(ctx.tenant_id);
    let limit  = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).clamp(1, 50);
    let offset = args.get("offset").and_then(|v| v.as_i64()).unwrap_or(0).max(0);

    let campaigns = state.campaign_svc
        .list(&tenant_id, limit, offset)
        .await
        .map_err(|e| format!("Failed to list campaigns: {e}"))?;

    let count = campaigns.len();
    Ok(json!({
        "campaigns": campaigns,
        "count": count,
    }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "limit":  { "type": "integer", "description": "Max results to return (1–50, default 20)" },
            "offset": { "type": "integer", "description": "Pagination offset (default 0)" }
        },
        "required": []
    })
}
