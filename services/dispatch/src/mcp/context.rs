use uuid::Uuid;

/// Derived from a verified JWT. Injected into every MCP tool handler.
/// All repo calls take `&McpContext` — the tenant_id is always JWT-derived,
/// never taken from tool arguments.
#[derive(Debug, Clone)]
pub struct McpContext {
    pub tenant_id: Uuid,
    pub actor_uid: Uuid,
    pub permissions: Vec<String>,
    pub trace_id: String,
}

impl McpContext {
    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.contains(&perm.to_owned())
            || self.permissions.contains(&"*".to_owned())
    }
}
