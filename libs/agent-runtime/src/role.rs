//! Resolved, product-agnostic agent identity.
//!
//! Products keep their own role enums and convert into this at session
//! construction. `key` is the stable persistence identity — it must match the
//! string the product's enum serialises to, or existing rows stop loading.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRole {
    key:            String,
    display_name:   String,
    system_context: String,
    /// `None` = full registry (trusted autonomous agents).
    /// `Some(list)` = allowlist, enforced in the loop *and* used to filter the
    /// tool definitions sent to Claude, so a restricted agent is never even
    /// told the other tools exist.
    allowed_tools:  Option<Vec<String>>,
}

impl AgentRole {
    pub fn unrestricted(
        key: impl Into<String>,
        display_name: impl Into<String>,
        system_context: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            display_name: display_name.into(),
            system_context: system_context.into(),
            allowed_tools: None,
        }
    }

    pub fn restricted<S: Into<String>>(
        key: impl Into<String>,
        display_name: impl Into<String>,
        system_context: impl Into<String>,
        allowed: impl IntoIterator<Item = S>,
    ) -> Self {
        Self {
            key: key.into(),
            display_name: display_name.into(),
            system_context: system_context.into(),
            allowed_tools: Some(allowed.into_iter().map(Into::into).collect()),
        }
    }

    pub fn key(&self) -> &str { &self.key }
    pub fn display_name(&self) -> &str { &self.display_name }
    pub fn system_context(&self) -> &str { &self.system_context }
    pub fn allowed_tools(&self) -> Option<&[String]> { self.allowed_tools.as_deref() }

    /// The authorisation gate. Checked before every tool execution — Claude
    /// naming a tool it was not offered must not be enough to run it.
    pub fn permits(&self, tool_name: &str) -> bool {
        match &self.allowed_tools {
            None => true,
            Some(allowed) => allowed.iter().any(|t| t == tool_name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures use deliberately product-neutral vocabulary — Task 10 adds a CI
    // check that fails if this crate names a product concept, tests included.

    #[test]
    fn unrestricted_role_permits_any_tool() {
        let role = AgentRole::unrestricted("planner", "Planner Agent", "You plan work.");
        assert!(role.permits("write_item"));
        assert!(role.permits("anything_at_all"));
        assert!(role.allowed_tools().is_none());
    }

    #[test]
    fn restricted_role_permits_only_its_allowlist() {
        let role = AgentRole::restricted(
            "reader",
            "Reader Agent",
            "You answer questions.",
            ["read_item", "escalate_to_human"],
        );
        assert!(role.permits("read_item"));
        assert!(!role.permits("write_item"));
        assert!(!role.permits("delete_item"));
    }

    /// The key round-trips as the persisted `agent_type` column value, so an
    /// existing row keeps deserialising after the extraction.
    #[test]
    fn key_is_the_persistence_identity() {
        let role = AgentRole::unrestricted("planner", "Planner Agent", "…");
        assert_eq!(role.key(), "planner");
    }
}
