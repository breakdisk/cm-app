use uuid::Uuid;

/// The caller, as read from the validated JWT. Nothing here comes from tool
/// arguments: `tenant_id` in particular is never taken from the client.
#[derive(Debug, Clone)]
pub struct McpContext {
    pub tenant_id:   Uuid,
    pub actor_uid:   Uuid,
    pub permissions: Vec<String>,
    /// Set when the token is bound to a partner (a carrier's own operator).
    pub partner_id:  Option<Uuid>,
    pub trace_id:    String,
}

impl McpContext {
    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == perm || p == "*")
    }
}
