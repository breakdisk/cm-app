//! Tool contract. This crate defines the shape; products supply the tools.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name:         String,
    pub description:  String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content:     serde_json::Value,
    pub is_error:    bool,
}

/// Caller context threaded into tool execution. `bearer` is `None` for
/// autonomous agents (internal endpoints only) and `Some` for request-scoped
/// surfaces, so downstream services apply the caller's own authorisation.
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    pub bearer: Option<String>,
}

impl ToolContext {
    pub fn with_bearer(bearer: impl Into<String>) -> Self {
        Self { bearer: Some(bearer.into()) }
    }
}

/// Filter definitions by an allowlist. `None` means the full set.
pub fn filter_definitions(all: &[ToolDefinition], allowed: Option<&[String]>) -> Vec<ToolDefinition> {
    match allowed {
        None => all.to_vec(),
        Some(list) => all
            .iter()
            .filter(|d| list.iter().any(|a| a == &d.name))
            .cloned()
            .collect(),
    }
}

#[async_trait]
pub trait ToolBox: Send + Sync {
    fn definitions(&self) -> &[ToolDefinition];

    async fn execute(
        &self,
        name: String,
        input: serde_json::Value,
        tool_use_id: String,
        ctx: ToolContext,
    ) -> ToolResult;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> Vec<ToolDefinition> {
        ["read_item", "write_item", "escalate_to_human"]
            .into_iter()
            .map(|n| ToolDefinition {
                name: n.into(),
                description: format!("does {n}"),
                input_schema: serde_json::json!({"type": "object"}),
            })
            .collect()
    }

    #[test]
    fn no_allowlist_returns_every_definition() {
        assert_eq!(filter_definitions(&defs(), None).len(), 3);
    }

    /// A restricted agent is never told the other tools exist.
    #[test]
    fn allowlist_filters_definitions_sent_to_claude() {
        let allowed = ["read_item".to_string(), "escalate_to_human".to_string()];
        let filtered = filter_definitions(&defs(), Some(&allowed));
        let names: Vec<_> = filtered.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["read_item", "escalate_to_human"]);
        assert!(!names.contains(&"write_item"));
    }

    #[test]
    fn empty_allowlist_returns_nothing() {
        assert!(filter_definitions(&defs(), Some(&[])).is_empty());
    }
}
