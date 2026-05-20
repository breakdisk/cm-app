use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct McpContext {
    pub tenant_id:  Uuid,
    pub actor_uid:  Uuid,
    pub permissions: Vec<String>,
    pub trace_id:   String,
}

impl McpContext {
    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.contains(&perm.to_owned())
            || self.permissions.contains(&"*".to_owned())
    }
}
