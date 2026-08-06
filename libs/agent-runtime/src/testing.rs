//! Test doubles. Enabled by the `testing` feature so downstream crates can
//! reuse them without shipping them in release builds.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;

use crate::claude::{ClaudeApi, ContentBlock, MessagesResponse, Usage};
use crate::session::{AgentMessage, AgentSession};
use crate::store::SessionStore;
use crate::tools::{ToolBox, ToolContext, ToolDefinition, ToolResult};

/// Returns queued responses in order. Errors if the loop asks for more than
/// were queued — an unbounded loop should fail the test loudly, not hang.
pub struct StubClaude {
    responses: Mutex<std::collections::VecDeque<MessagesResponse>>,
    pub calls: Mutex<usize>,
}

impl StubClaude {
    pub fn new(responses: Vec<MessagesResponse>) -> Self {
        Self { responses: Mutex::new(responses.into()), calls: Mutex::new(0) }
    }

    pub fn text(text: &str) -> MessagesResponse {
        MessagesResponse {
            id: "msg_stub".into(),
            stop_reason: "end_turn".into(),
            content: vec![ContentBlock::Text { text: text.into() }],
            usage: Usage { input_tokens: 1, output_tokens: 1 },
        }
    }

    pub fn tool_call(id: &str, name: &str, input: serde_json::Value) -> MessagesResponse {
        MessagesResponse {
            id: "msg_stub".into(),
            stop_reason: "tool_use".into(),
            content: vec![ContentBlock::ToolUse { id: id.into(), name: name.into(), input }],
            usage: Usage { input_tokens: 1, output_tokens: 1 },
        }
    }
}

#[async_trait]
impl ClaudeApi for StubClaude {
    fn model(&self) -> &str { "stub-model" }

    async fn send(
        &self,
        _system: &str,
        _messages: &[AgentMessage],
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<MessagesResponse> {
        *self.calls.lock().unwrap() += 1;
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("StubClaude ran out of queued responses"))
    }
}

/// Records every execution so a test can assert a forbidden tool never ran.
pub struct StubToolBox {
    defs:         Vec<ToolDefinition>,
    pub executed: Mutex<Vec<String>>,
}

impl StubToolBox {
    pub fn with_tools(names: &[&str]) -> Self {
        Self {
            defs: names
                .iter()
                .map(|n| ToolDefinition {
                    name: (*n).into(),
                    description: format!("stub {n}"),
                    input_schema: serde_json::json!({"type": "object"}),
                })
                .collect(),
            executed: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ToolBox for StubToolBox {
    fn definitions(&self) -> &[ToolDefinition] { &self.defs }

    async fn execute(
        &self,
        name: String,
        _input: serde_json::Value,
        tool_use_id: String,
        _ctx: ToolContext,
    ) -> ToolResult {
        self.executed.lock().unwrap().push(name);
        ToolResult { tool_use_id, content: serde_json::json!({"ok": true}), is_error: false }
    }
}

#[derive(Default)]
pub struct InMemoryStore {
    pub saved: Mutex<Vec<AgentSession>>,
}

#[async_trait]
impl SessionStore for InMemoryStore {
    async fn save(&self, session: &AgentSession) -> anyhow::Result<()> {
        self.saved.lock().unwrap().push(session.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<AgentSession>> {
        Ok(self.saved.lock().unwrap().iter().rev().find(|s| s.id == id).cloned())
    }
}

pub fn arc<T: 'static>(t: T) -> Arc<T> { Arc::new(t) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::role::AgentRole;
    use crate::runner::AgentRunner;
    use crate::session::SessionStatus;
    use logisticos_types::TenantId;

    fn runner(
        claude: StubClaude,
        tools: Arc<StubToolBox>,
        store: Arc<InMemoryStore>,
    ) -> AgentRunner {
        AgentRunner::new(Arc::new(claude), tools, store)
    }

    #[tokio::test]
    async fn a_plain_text_turn_completes_the_session() {
        let store = Arc::new(InMemoryStore::default());
        let tools = Arc::new(StubToolBox::with_tools(&["read_item"]));
        let r = runner(StubClaude::new(vec![StubClaude::text("All set.")]), tools, store.clone());

        let session = r
            .run(
                TenantId::from_uuid(Uuid::new_v4()),
                AgentRole::unrestricted("test", "Test Agent", "You are a test."),
                serde_json::json!({}),
                "hello".into(),
            )
            .await
            .expect("run should succeed");

        assert_eq!(session.status, SessionStatus::Completed);
        assert_eq!(session.outcome.as_deref(), Some("All set."));
        assert_eq!(session.model_used, "stub-model");
    }

    /// The RBAC gate is a security control: a tool outside the allowlist must
    /// not execute even when Claude explicitly asks for it.
    #[tokio::test]
    async fn a_forbidden_tool_is_refused_and_never_executed() {
        let store = Arc::new(InMemoryStore::default());
        let tools = Arc::new(StubToolBox::with_tools(&["read_item", "write_item"]));
        let r = runner(
            StubClaude::new(vec![
                StubClaude::tool_call("toolu_1", "write_item", serde_json::json!({})),
                StubClaude::text("I can't do that."),
            ]),
            tools.clone(),
            store.clone(),
        );

        let session = r
            .run(
                TenantId::from_uuid(Uuid::new_v4()),
                AgentRole::restricted("narrow", "Narrow Agent", "You are narrow.", ["read_item"]),
                serde_json::json!({}),
                "write a new item".into(),
            )
            .await
            .expect("run should succeed");

        assert!(
            tools.executed.lock().unwrap().is_empty(),
            "forbidden tool must never reach the tool box"
        );
        let refused = session.actions.iter().find(|a| a.tool_name == "write_item").unwrap();
        assert!(!refused.succeeded);
        assert_eq!(session.status, SessionStatus::Completed);
    }

    #[tokio::test]
    async fn an_allowed_tool_executes_and_the_loop_continues() {
        let store = Arc::new(InMemoryStore::default());
        let tools = Arc::new(StubToolBox::with_tools(&["read_item"]));
        let r = runner(
            StubClaude::new(vec![
                StubClaude::tool_call("toolu_1", "read_item", serde_json::json!({"id": "1"})),
                StubClaude::text("Here is the item you asked for."),
            ]),
            tools.clone(),
            store.clone(),
        );

        let session = r
            .run(
                TenantId::from_uuid(Uuid::new_v4()),
                AgentRole::restricted("narrow", "Narrow Agent", "You are narrow.", ["read_item"]),
                serde_json::json!({}),
                "read item 1".into(),
            )
            .await
            .expect("run should succeed");

        assert_eq!(tools.executed.lock().unwrap().as_slice(), ["read_item"]);
        assert_eq!(session.status, SessionStatus::Completed);
        assert_eq!(session.actions.len(), 1);
        assert!(session.actions[0].succeeded);
    }

    /// Every turn is persisted so a crash mid-session is recoverable.
    #[tokio::test]
    async fn the_session_is_persisted_on_every_turn() {
        let store = Arc::new(InMemoryStore::default());
        let tools = Arc::new(StubToolBox::with_tools(&["read_item"]));
        let r = runner(
            StubClaude::new(vec![
                StubClaude::tool_call("toolu_1", "read_item", serde_json::json!({})),
                StubClaude::text("done"),
            ]),
            tools,
            store.clone(),
        );

        r.run(
            TenantId::from_uuid(Uuid::new_v4()),
            AgentRole::unrestricted("test", "Test Agent", "You are a test."),
            serde_json::json!({}),
            "go".into(),
        )
        .await
        .unwrap();

        // initial save + mid-loop save + final save
        assert!(store.saved.lock().unwrap().len() >= 3);
    }
}
