pub mod activate_campaign;
pub mod create_campaign;
pub mod get_campaign_stats;
pub mod list_campaigns;

use serde_json::Value;
use std::sync::Arc;
use crate::AppState;
use crate::mcp::context::McpContext;

/// Route a `tools/call` request to the correct handler.
pub async fn dispatch(
    name:  &str,
    args:  &Value,
    ctx:   &McpContext,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    match name {
        "list_campaigns"    => list_campaigns::handle(args, ctx, state).await,
        "create_campaign"   => create_campaign::handle(args, ctx, state).await,
        "activate_campaign" => activate_campaign::handle(args, ctx, state).await,
        "get_campaign_stats"=> get_campaign_stats::handle(args, ctx, state).await,
        other => Err(format!("Unknown tool: {other}")),
    }
}

/// Return the `tools/list` schema array used by AI agent tool discovery.
pub fn list() -> Value {
    serde_json::json!([
        {
            "name": "list_campaigns",
            "description": "List campaigns for the tenant. Optionally paginate with limit/offset.",
            "inputSchema": list_campaigns::schema()
        },
        {
            "name": "create_campaign",
            "description": "Create a draft campaign. Provide an inline message body or a registered template_id. For CDP-based targeting, set min_clv_score; for direct sends, provide the recipients array.",
            "inputSchema": create_campaign::schema()
        },
        {
            "name": "activate_campaign",
            "description": "Activate a draft or scheduled campaign, starting the fan-out send. If the campaign has CDP targeting criteria (min_clv_score), the audience is resolved automatically before sending.",
            "inputSchema": activate_campaign::schema()
        },
        {
            "name": "get_campaign_stats",
            "description": "Retrieve a campaign's current delivery metrics: total sent, delivered, failed, and delivery rate.",
            "inputSchema": get_campaign_stats::schema()
        }
    ])
}
