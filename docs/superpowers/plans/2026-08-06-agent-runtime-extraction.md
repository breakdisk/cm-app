# Agent Runtime Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the agent loop, session entities, RBAC gate and Claude client out of `services/ai-layer` into a shared `libs/agent-runtime` crate, so OmniDeliv's mesh and ai-layer's logistics agents run on one runtime with one audit shape and one RBAC implementation.

**Architecture:** The runtime becomes product-agnostic by inverting three dependencies. The logistics-specific `AgentType` enum is replaced *inside the runtime* by a resolved `AgentRole` struct — products keep their own enums and convert at session construction. The concrete `ClaudeClient` gains a `ClaudeApi` trait so the loop can be driven by a stub in tests. The concrete `ToolRegistry` becomes a `ToolBox` trait so the runtime never sees logistics tool implementations. Behaviour is preserved exactly; ai-layer's existing tests are the safety net.

**Tech Stack:** Rust 2021, Tokio, `async-trait`, `serde`, `thiserror`/`anyhow`, `reqwest` (Rust has no official Anthropic SDK — raw HTTP is correct here).

---

## Prerequisites

Read before starting:

- [services/ai-layer/src/application/agent/mod.rs](../../../services/ai-layer/src/application/agent/mod.rs) — the loop being moved (265 lines)
- [services/ai-layer/src/domain/entities/mod.rs](../../../services/ai-layer/src/domain/entities/mod.rs) — entities + the `allowed_tools` RBAC gate and its tests (381 lines)
- [services/ai-layer/src/infrastructure/claude/mod.rs](../../../services/ai-layer/src/infrastructure/claude/mod.rs) — the client (150 lines)

**Disk:** this workspace has a documented disk-pressure problem. Before starting, clear `C:\cargo-target-logisticos\debug\incremental` and export `CARGO_INCREMENTAL=0` for the whole session. `cargo check` skips linking and is the verification tool for the large crates; the new crate is small enough to link, so its tests run normally.

**Non-goal:** this plan does not change which Claude model is called. `claude-opus-4-6` is an active model and the code uses none of the parameters removed on newer models, so a bump to `claude-opus-5` would be nearly clean — but it is a model migration with its own re-tuning, not a refactor. Task 4 makes the model configurable so that migration becomes a config change later.

---

## File Structure

**New crate — `libs/agent-runtime/`:**

| File | Responsibility |
|---|---|
| `Cargo.toml` | Crate manifest; workspace member |
| `src/lib.rs` | Re-exports; module wiring only |
| `src/role.rs` | `AgentRole` — resolved, product-agnostic agent identity + RBAC allowlist |
| `src/session.rs` | `AgentSession`, `SessionStatus`, `AgentMessage`, `MessageRole`, `AgentAction` |
| `src/claude.rs` | `ClaudeApi` trait, `ClaudeClient` impl, `MessagesResponse`, `ContentBlock`, `Usage` |
| `src/tools.rs` | `ToolBox` trait, `ToolDefinition`, `ToolResult`, `ToolContext` |
| `src/store.rs` | `SessionStore` trait (save / find_by_id only) |
| `src/runner.rs` | `AgentRunner` — the loop, generic over the three traits |
| `src/testing.rs` | `StubClaude`, `StubToolBox`, `InMemoryStore` (behind `#[cfg(feature = "testing")]`) |

**Modified — `services/ai-layer/`:**

| File | Change |
|---|---|
| `Cargo.toml` | Add `logisticos-agent-runtime` dependency |
| `src/domain/entities/mod.rs` | Keep `AgentType` enum; delete moved entities; add `impl From<AgentType> for AgentRole` |
| `src/infrastructure/claude/mod.rs` | Delete — re-export from the crate |
| `src/infrastructure/tools/mod.rs` | Keep the 21 tools; `impl ToolBox for ToolRegistry` |
| `src/infrastructure/db/mod.rs` | Keep `SessionRepository` (dashboard queries); `impl SessionStore` too |
| `src/application/agent/mod.rs` | Delete — re-export `AgentRunner` from the crate |

**Split rationale:** the runtime crate owns *how an agent runs*; ai-layer keeps *what a logistics agent is*. Nothing in `libs/agent-runtime` may name a logistics concept — that invariant is what makes it reusable by OmniDeliv, and Task 10 adds a CI check enforcing it.

---

## Task 1: Create the crate skeleton

**Files:**
- Create: `libs/agent-runtime/Cargo.toml`
- Create: `libs/agent-runtime/src/lib.rs`
- Modify: `Cargo.toml` (workspace members list)

- [ ] **Step 1: Write the manifest**

```toml
# libs/agent-runtime/Cargo.toml
[package]
name        = "logisticos-agent-runtime"
description = "Product-agnostic Claude agent loop: session, RBAC gate, tool dispatch, audit"
version.workspace      = true
edition.workspace      = true
authors.workspace      = true
rust-version.workspace = true

[dependencies]
logisticos-errors.workspace = true
logisticos-types.workspace  = true
serde.workspace             = true
serde_json.workspace        = true
uuid.workspace              = true
chrono.workspace            = true
tracing.workspace           = true
reqwest.workspace           = true
async-trait.workspace       = true
anyhow.workspace            = true

[features]
testing = []

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 2: Write the lib root**

```rust
// libs/agent-runtime/src/lib.rs
//! Product-agnostic agent runtime.
//!
//! Nothing in this crate may name a concept owned by a specific product.
//! Products supply their own roles and tools; this crate owns the loop.
//! Enforced by `scripts/check-runtime-boundary.sh` in CI.

pub mod claude;
pub mod role;
pub mod runner;
pub mod session;
pub mod store;
pub mod tools;

#[cfg(feature = "testing")]
pub mod testing;

pub use role::AgentRole;
pub use runner::AgentRunner;
pub use session::{AgentAction, AgentMessage, AgentSession, MessageRole, SessionStatus};
pub use store::SessionStore;
pub use tools::{ToolBox, ToolContext, ToolDefinition, ToolResult};
```

- [ ] **Step 3: Register the workspace member**

In the root `Cargo.toml`, add `"libs/agent-runtime",` to `members` directly after `"libs/ai-client",`.

- [ ] **Step 4: Verify it resolves**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-agent-runtime`
Expected: FAIL — `file not found for module 'claude'` and five similar errors. This confirms the manifest and workspace wiring are correct and the modules are what's missing.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml libs/agent-runtime/
git commit -m "feat(agent-runtime): scaffold shared agent runtime crate"
```

---

## Task 2: AgentRole — the product-agnostic identity

This is the load-bearing decision. `AgentType` is a logistics enum (`Dispatch`, `Recovery`, …) but the runner only ever asks it three questions. `AgentRole` answers those three questions as plain data, so the runtime never sees the enum and OmniDeliv can supply its own.

**Files:**
- Create: `libs/agent-runtime/src/role.rs`

- [ ] **Step 1: Write the failing test**

```rust
// libs/agent-runtime/src/role.rs
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-agent-runtime role::`
Expected: FAIL to compile — `cannot find type 'AgentRole' in this scope`.

- [ ] **Step 3: Write the implementation**

Put this *above* the `mod tests` block in the same file:

```rust
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
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-agent-runtime role::`
Expected: PASS — 3 passed.

- [ ] **Step 5: Commit**

```bash
git add libs/agent-runtime/src/role.rs
git commit -m "feat(agent-runtime): add AgentRole with RBAC allowlist gate"
```

---

## Task 3: Session entities

Straight move of `AgentSession`, `SessionStatus`, `AgentMessage`, `MessageRole`, `AgentAction` — with two changes: `agent_type: AgentType` becomes `role: AgentRole`, and the hardcoded `model_used` becomes a constructor parameter.

**Files:**
- Create: `libs/agent-runtime/src/session.rs`

- [ ] **Step 1: Write the failing test**

```rust
// libs/agent-runtime/src/session.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::TenantId;
    use uuid::Uuid;

    fn role() -> AgentRole {
        AgentRole::unrestricted("planner", "Planner Agent", "You plan work.")
    }

    fn session() -> AgentSession {
        AgentSession::new(
            TenantId::from_uuid(Uuid::new_v4()),
            role(),
            serde_json::json!({}),
            "claude-opus-4-6",
        )
    }

    #[test]
    fn new_session_starts_running_and_records_its_model() {
        let s = session();
        assert_eq!(s.status, SessionStatus::Running);
        assert_eq!(s.model_used, "claude-opus-4-6");
        assert!(s.completed_at.is_none());
    }

    /// `GET /v1/agents/chat/:id` reports `resolved_by_human` as
    /// `status == Completed && escalation_reason.is_some()`. That only
    /// distinguishes an operator-resolved case from an ordinary finished chat
    /// because `complete()` leaves `escalation_reason` in place.
    #[test]
    fn completing_an_escalation_keeps_the_escalation_reason() {
        let mut escalated = session();
        escalated.escalate("customer asked for a human".into());
        escalated.complete("Resolved by human (op-1): refunded".into(), 1.0);

        assert_eq!(escalated.status, SessionStatus::Completed);
        assert!(escalated.escalation_reason.is_some());

        let mut plain = session();
        plain.complete("Here's your tracking update.".into(), 0.9);
        assert_eq!(plain.status, SessionStatus::Completed);
        assert!(plain.escalation_reason.is_none());
    }

    #[test]
    fn reopen_clears_completion_for_the_next_turn() {
        let mut s = session();
        s.complete("done".into(), 0.9);
        s.reopen();
        assert_eq!(s.status, SessionStatus::Running);
        assert!(s.completed_at.is_none());
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-agent-runtime session::`
Expected: FAIL to compile — `cannot find type 'AgentSession' in this scope`.

- [ ] **Step 3: Write the implementation**

Above the tests block:

```rust
//! Agent session, messages and the append-only action audit log.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::role::AgentRole;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Completed,
    Failed,
    HumanEscalated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role:    MessageRole,
    /// String, or an array of Claude content blocks.
    pub content: serde_json::Value,
}

/// Immutable audit entry for one tool call. Never updated after `succeeded` is set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    pub id:          Uuid,
    pub session_id:  Uuid,
    pub tool_name:   String,
    pub tool_input:  serde_json::Value,
    pub tool_result: Option<serde_json::Value>,
    pub succeeded:   bool,
    pub executed_at: DateTime<Utc>,
}

impl AgentAction {
    pub fn new(session_id: Uuid, tool_name: String, tool_input: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            tool_name,
            tool_input,
            tool_result: None,
            succeeded: false,
            executed_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id:                Uuid,
    pub tenant_id:         TenantId,
    pub role:              AgentRole,
    pub status:            SessionStatus,
    pub trigger:           serde_json::Value,
    pub messages:          Vec<AgentMessage>,
    pub actions:           Vec<AgentAction>,
    pub outcome:           Option<String>,
    pub escalation_reason: Option<String>,
    pub confidence_score:  Option<f32>,
    pub model_used:        String,
    pub started_at:        DateTime<Utc>,
    pub completed_at:      Option<DateTime<Utc>>,
}

impl AgentSession {
    /// `model` is recorded on the session for audit. It is supplied rather than
    /// hardcoded so the recorded model can never drift from the one actually called.
    pub fn new(
        tenant_id: TenantId,
        role: AgentRole,
        trigger: serde_json::Value,
        model: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            role,
            status: SessionStatus::Running,
            trigger,
            messages: Vec::new(),
            actions: Vec::new(),
            outcome: None,
            escalation_reason: None,
            confidence_score: None,
            model_used: model.into(),
            started_at: Utc::now(),
            completed_at: None,
        }
    }

    pub fn complete(&mut self, outcome: String, confidence: f32) {
        self.status = SessionStatus::Completed;
        self.outcome = Some(outcome);
        self.confidence_score = Some(confidence);
        self.completed_at = Some(Utc::now());
    }

    pub fn fail(&mut self, reason: String) {
        self.status = SessionStatus::Failed;
        self.escalation_reason = Some(reason);
        self.completed_at = Some(Utc::now());
    }

    pub fn escalate(&mut self, reason: String) {
        self.status = SessionStatus::HumanEscalated;
        self.escalation_reason = Some(reason);
        self.completed_at = Some(Utc::now());
    }

    /// Put a finished session back into `Running` so another user turn can be
    /// appended — one row and one audit trail per conversation, not per message.
    pub fn reopen(&mut self) {
        self.status = SessionStatus::Running;
        self.completed_at = None;
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-agent-runtime session::`
Expected: PASS — 3 passed.

- [ ] **Step 5: Commit**

```bash
git add libs/agent-runtime/src/session.rs
git commit -m "feat(agent-runtime): move session entities, make model_used explicit"
```

---

## Task 4: ClaudeApi trait + client

The current `AgentRunner` holds `Arc<ClaudeClient>` — a concrete struct. That is why the loop has never been unit-testable. Introducing the trait is the single highest-value change in this plan.

**Files:**
- Create: `libs/agent-runtime/src/claude.rs`

- [ ] **Step 1: Write the failing test**

```rust
// libs/agent-runtime/src/claude.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;

    fn text_response(text: &str) -> MessagesResponse {
        MessagesResponse {
            id: "msg_1".into(),
            stop_reason: "end_turn".into(),
            content: vec![ContentBlock::Text { text: text.into() }],
            usage: Usage { input_tokens: 1, output_tokens: 1 },
        }
    }

    #[test]
    fn extract_text_joins_all_text_blocks() {
        let mut r = text_response("first");
        r.content.push(ContentBlock::Text { text: "second".into() });
        assert_eq!(extract_text(&r), "first\nsecond");
    }

    #[test]
    fn extract_text_ignores_tool_use_blocks() {
        let mut r = text_response("visible");
        r.content.push(ContentBlock::ToolUse {
            id: "toolu_1".into(),
            name: "read_item".into(),
            input: serde_json::json!({}),
        });
        assert_eq!(extract_text(&r), "visible");
    }

    #[test]
    fn extract_tool_calls_returns_only_tool_use_blocks() {
        let mut r = text_response("preamble");
        r.content.push(ContentBlock::ToolUse {
            id: "toolu_1".into(),
            name: "read_item".into(),
            input: serde_json::json!({"id": "1"}),
        });
        let calls = extract_tool_calls(&r);
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0], ContentBlock::ToolUse { .. }));
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-agent-runtime claude::`
Expected: FAIL to compile — `cannot find function 'extract_text' in this scope`.

- [ ] **Step 3: Write the implementation**

Above the tests block. Note `extract_text`/`extract_tool_calls` become free functions — they were associated functions on the concrete client, which would have forced every `ClaudeApi` impl to reimplement them.

```rust
//! Claude Messages API client and the trait the runner depends on.
//!
//! Rust has no official Anthropic SDK, so this speaks raw HTTP. The trait
//! exists so the agent loop can be driven by a stub in tests — assert on
//! transitions, never on generated prose.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::session::{AgentMessage, MessageRole};
use crate::tools::ToolDefinition;

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";

#[derive(Debug, Deserialize)]
pub struct MessagesResponse {
    pub id:          String,
    /// "end_turn" | "tool_use"
    pub stop_reason: String,
    pub content:     Vec<ContentBlock>,
    pub usage:       Usage,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub input_tokens:  u32,
    pub output_tokens: u32,
}

/// Concatenate every text block in a response.
pub fn extract_text(response: &MessagesResponse) -> String {
    response
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every `tool_use` block in a response, in order.
pub fn extract_tool_calls(response: &MessagesResponse) -> Vec<&ContentBlock> {
    response
        .content
        .iter()
        .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
        .collect()
}

/// One agentic turn. Implemented by `ClaudeClient` in production and by
/// `testing::StubClaude` in tests.
#[async_trait]
pub trait ClaudeApi: Send + Sync {
    async fn send(
        &self,
        system: &str,
        messages: &[AgentMessage],
        tools: &[ToolDefinition],
    ) -> anyhow::Result<MessagesResponse>;

    /// The model id this client calls. Recorded on the session for audit, so
    /// the recorded value can never drift from the one actually used.
    fn model(&self) -> &str;
}

#[derive(Debug, Serialize)]
struct MessagesRequest<'a> {
    model:      &'a str,
    max_tokens: u32,
    system:     &'a str,
    messages:   Vec<ClaudeMessage>,
    tools:      Vec<ClaudeTool>,
}

#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role:    String,
    content: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ClaudeTool {
    name:         String,
    description:  String,
    input_schema: serde_json::Value,
}

pub struct ClaudeClient {
    http:       reqwest::Client,
    api_key:    String,
    model:      String,
    max_tokens: u32,
}

impl ClaudeClient {
    pub fn new(api_key: String, model: impl Into<String>, max_tokens: u32) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build Claude HTTP client");
        Self { http, api_key, model: model.into(), max_tokens }
    }
}

#[async_trait]
impl ClaudeApi for ClaudeClient {
    fn model(&self) -> &str { &self.model }

    async fn send(
        &self,
        system: &str,
        messages: &[AgentMessage],
        tools: &[ToolDefinition],
    ) -> anyhow::Result<MessagesResponse> {
        let claude_messages: Vec<ClaudeMessage> = messages
            .iter()
            .map(|m| ClaudeMessage {
                role: match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                }
                .into(),
                content: m.content.clone(),
            })
            .collect();

        let claude_tools: Vec<ClaudeTool> = tools
            .iter()
            .map(|t| ClaudeTool {
                name:         t.name.clone(),
                description:  t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect();

        let body = MessagesRequest {
            model:      &self.model,
            max_tokens: self.max_tokens,
            system,
            messages:   claude_messages,
            tools:      claude_tools,
        };

        let resp = self
            .http
            .post(CLAUDE_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Claude API error {}: {}", status, text);
        }

        Ok(resp.json::<MessagesResponse>().await?)
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-agent-runtime claude::`
Expected: PASS — 3 passed.

- [ ] **Step 5: Commit**

```bash
git add libs/agent-runtime/src/claude.rs
git commit -m "feat(agent-runtime): add ClaudeApi trait, make model and max_tokens configurable"
```

---

## Task 5: ToolBox trait

**Files:**
- Create: `libs/agent-runtime/src/tools.rs`

- [ ] **Step 1: Write the failing test**

```rust
// libs/agent-runtime/src/tools.rs — tests block
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-agent-runtime tools::`
Expected: FAIL to compile — `cannot find function 'filter_definitions' in this scope`.

- [ ] **Step 3: Write the implementation**

```rust
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
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-agent-runtime tools::`
Expected: PASS — 3 passed.

- [ ] **Step 5: Commit**

```bash
git add libs/agent-runtime/src/tools.rs
git commit -m "feat(agent-runtime): add ToolBox trait and definition filtering"
```

---

## Task 6: SessionStore trait

The existing `SessionRepository` mixes two concerns: crash-recovery persistence (`save`, `find_by_id`) and dashboard queries (`list_by_tenant`, `list_escalated`, `aggregate`). Only the first is runtime. The dashboard trait stays in ai-layer.

**Files:**
- Create: `libs/agent-runtime/src/store.rs`

- [ ] **Step 1: Write the implementation** (no test — this is a trait declaration with no behaviour; Task 8 exercises it through `InMemoryStore`)

```rust
//! Persistence contract for crash recovery.
//!
//! Deliberately narrow: the runner saves after every turn and reloads by id.
//! Listing, filtering and aggregation are product dashboard concerns and stay
//! with the product.

use async_trait::async_trait;
use uuid::Uuid;

use crate::session::AgentSession;

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session: &AgentSession) -> anyhow::Result<()>;
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<AgentSession>>;
}
```

- [ ] **Step 2: Verify it compiles**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-agent-runtime`
Expected: FAIL — `file not found for module 'runner'` and `'testing'`. Every other module now resolves; only Tasks 7 and 8 remain.

- [ ] **Step 3: Commit**

```bash
git add libs/agent-runtime/src/store.rs
git commit -m "feat(agent-runtime): add narrow SessionStore trait for crash recovery"
```

---

## Task 7: The agent loop

Move `AgentRunner` verbatim in behaviour, swapping three concrete types for the three traits and `session.agent_type` for `session.role`.

**Files:**
- Create: `libs/agent-runtime/src/runner.rs`

- [ ] **Step 1: Write the implementation**

The loop below is the existing one with the dependency swaps applied. Read the original at `services/ai-layer/src/application/agent/mod.rs` alongside this to confirm the behaviour matches — the escalation branch and the refusal branch are the two easy places to introduce a regression.

```rust
//! The core agentic loop — runs a session until completion or escalation.
//!
//! 1. Build system prompt from the role
//! 2. Send messages + allowed tools to Claude
//! 3. Tool calls → check the allowlist, execute, append results, go to 2
//! 4. `end_turn` → session complete
//! 5. `escalate_to_human` signal → session escalated
//! 6. Persist after every turn (crash recovery)

use std::sync::Arc;

use serde_json::{json, Value};

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::claude::{extract_text, extract_tool_calls, ClaudeApi, ContentBlock};
use crate::role::AgentRole;
use crate::session::{AgentAction, AgentMessage, AgentSession, MessageRole};
use crate::store::SessionStore;
use crate::tools::{filter_definitions, ToolBox, ToolContext};

const MAX_TURNS: usize = 20;

pub struct AgentRunner {
    claude: Arc<dyn ClaudeApi>,
    tools:  Arc<dyn ToolBox>,
    store:  Arc<dyn SessionStore>,
}

impl AgentRunner {
    pub fn new(
        claude: Arc<dyn ClaudeApi>,
        tools: Arc<dyn ToolBox>,
        store: Arc<dyn SessionStore>,
    ) -> Self {
        Self { claude, tools, store }
    }

    /// Autonomous entry point: no human caller, so tools run without a bearer
    /// token and can only reach internal endpoints.
    pub async fn run(
        &self,
        tenant_id: TenantId,
        role: AgentRole,
        trigger: Value,
        initial_user_message: String,
    ) -> AppResult<AgentSession> {
        self.run_with_context(tenant_id, role, trigger, initial_user_message, ToolContext::default())
            .await
    }

    /// Same as `run`, but tools execute with the supplied caller context.
    pub async fn run_with_context(
        &self,
        tenant_id: TenantId,
        role: AgentRole,
        trigger: Value,
        initial_user_message: String,
        ctx: ToolContext,
    ) -> AppResult<AgentSession> {
        let mut session = AgentSession::new(tenant_id, role, trigger, self.claude.model());
        self.store.save(&session).await.map_err(AppError::internal)?;

        session.messages.push(AgentMessage {
            role:    MessageRole::User,
            content: Value::String(initial_user_message),
        });

        self.drive(session, ctx).await
    }

    /// Append a user turn to an existing session and run the loop again. The
    /// full prior history (including tool calls and results) is already on the
    /// session, so the agent keeps its context across turns.
    pub async fn resume(
        &self,
        mut session: AgentSession,
        user_message: String,
        ctx: ToolContext,
    ) -> AppResult<AgentSession> {
        session.reopen();
        session.messages.push(AgentMessage {
            role:    MessageRole::User,
            content: Value::String(user_message),
        });
        self.drive(session, ctx).await
    }

    async fn drive(&self, mut session: AgentSession, ctx: ToolContext) -> AppResult<AgentSession> {
        let system = format!(
            "{}\n\nTenant context: tenant_id = {}",
            session.role.system_context(),
            session.tenant_id.inner()
        );

        // A restricted agent is never told the other tools exist, and the
        // allowlist is re-checked before every execution below — Claude naming
        // a tool it was not offered must not be enough to run it.
        let allowed = session.role.allowed_tools().map(|s| s.to_vec());
        let tools = filter_definitions(self.tools.definitions(), allowed.as_deref());
        let mut turns = 0;

        loop {
            turns += 1;
            if turns > MAX_TURNS {
                session.escalate(format!("Agent exceeded {} turns without completing", MAX_TURNS));
                self.store.save(&session).await.ok();
                return Ok(session);
            }

            let response = match self.claude.send(&system, &session.messages, &tools).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(session_id = %session.id, err = %e, "Claude API error");
                    session.fail(format!("Claude API error: {}", e));
                    self.store.save(&session).await.ok();
                    return Err(AppError::ExternalService {
                        service: "claude".into(),
                        message: e.to_string(),
                    });
                }
            };

            tracing::info!(
                session_id = %session.id,
                turn = turns,
                stop_reason = %response.stop_reason,
                input_tokens = response.usage.input_tokens,
                output_tokens = response.usage.output_tokens,
                "Agent turn"
            );

            session.messages.push(AgentMessage {
                role:    MessageRole::Assistant,
                content: serde_json::to_value(&response.content).unwrap_or_default(),
            });

            if response.stop_reason == "end_turn" {
                let final_text = extract_text(&response);
                let confidence = extract_confidence_from_text(&final_text);
                session.complete(final_text, confidence);
                self.store.save(&session).await.ok();
                return Ok(session);
            }

            let tool_calls = extract_tool_calls(&response);
            if tool_calls.is_empty() {
                session.complete(extract_text(&response), 0.9);
                self.store.save(&session).await.ok();
                return Ok(session);
            }

            let mut tool_results: Vec<Value> = Vec::new();

            for block in tool_calls {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    if !session.role.permits(name) {
                        tracing::warn!(
                            session_id = %session.id,
                            role = %session.role.key(),
                            tool = %name,
                            "Refused tool call outside agent allowlist"
                        );
                        let mut action = AgentAction::new(session.id, name.clone(), input.clone());
                        action.tool_result = Some(json!({"error": "tool not permitted for this agent"}));
                        action.succeeded = false;
                        session.actions.push(action);
                        tool_results.push(json!({
                            "type":        "tool_result",
                            "tool_use_id": id,
                            "content":     "This tool is not available to you. Do not try it again.",
                            "is_error":    true,
                        }));
                        continue;
                    }

                    tracing::info!(session_id = %session.id, tool = %name, "Executing tool");

                    let mut action = AgentAction::new(session.id, name.clone(), input.clone());
                    let result = self
                        .tools
                        .execute(name.clone(), input.clone(), id.clone(), ctx.clone())
                        .await;

                    if !result.is_error
                        && result.content.get("__escalate").and_then(|v| v.as_bool()).unwrap_or(false)
                    {
                        let reason = result.content["reason"]
                            .as_str()
                            .unwrap_or("Agent requested escalation")
                            .to_owned();
                        action.tool_result = Some(result.content.clone());
                        action.succeeded = true;
                        session.actions.push(action);
                        // Keep whatever the agent said to the customer this turn —
                        // an escalated chat still needs a reply in the bubble.
                        let handoff_text = extract_text(&response);
                        if !handoff_text.trim().is_empty() {
                            session.outcome = Some(handoff_text);
                        }
                        session.escalate(reason);
                        self.store.save(&session).await.ok();
                        return Ok(session);
                    }

                    action.tool_result = Some(result.content.clone());
                    action.succeeded = !result.is_error;
                    session.actions.push(action);

                    tool_results.push(json!({
                        "type":        "tool_result",
                        "tool_use_id": result.tool_use_id,
                        "content":     result.content.to_string(),
                        "is_error":    result.is_error,
                    }));
                }
            }

            session.messages.push(AgentMessage {
                role:    MessageRole::User,
                content: Value::Array(tool_results),
            });

            // Persist mid-session for crash recovery.
            self.store.save(&session).await.ok();
        }
    }
}

/// Heuristic: if the agent's final message contains "confidence: XX%", parse it.
fn extract_confidence_from_text(text: &str) -> f32 {
    let lower = text.to_lowercase();
    if let Some(idx) = lower.find("confidence:") {
        let after = &lower[idx + 11..];
        let pct_str: String = after.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if let Ok(pct) = pct_str.trim().parse::<f32>() {
            return (pct / 100.0).clamp(0.0, 1.0);
        }
    }
    0.85
}
```

- [ ] **Step 2: Verify it compiles**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-agent-runtime`
Expected: FAIL — `file not found for module 'testing'` only. All other modules resolve.

- [ ] **Step 3: Commit**

```bash
git add libs/agent-runtime/src/runner.rs
git commit -m "feat(agent-runtime): move AgentRunner onto the ClaudeApi/ToolBox/SessionStore traits"
```

---

## Task 8: Test doubles + the loop test that was impossible before

This is the payoff task. It proves the loop is now testable without a network call.

**Files:**
- Create: `libs/agent-runtime/src/testing.rs`

- [ ] **Step 1: Write the doubles**

```rust
//! Test doubles. Enabled by the `testing` feature so downstream crates can
//! reuse them without shipping them in release builds.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;

use crate::claude::{ClaudeApi, ContentBlock, MessagesResponse, Usage};
use crate::session::{AgentMessage, AgentSession};
use crate::store::SessionStore;
use crate::tools::{ToolBox, ToolContext, ToolDefinition, ToolResult};

/// Returns queued responses in order. Panics if the loop asks for more than
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
    defs:          Vec<ToolDefinition>,
    pub executed:  Mutex<Vec<String>>,
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
```

- [ ] **Step 2: Write the failing loop test**

Append to the same file:

```rust
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
```

- [ ] **Step 3: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-agent-runtime --features testing`
Expected: PASS — 4 new tests plus the 9 from Tasks 2–5, 13 total.

- [ ] **Step 4: Commit**

```bash
git add libs/agent-runtime/src/testing.rs
git commit -m "test(agent-runtime): stub Claude client makes the agent loop unit-testable"
```

---

## Task 9: Refactor ai-layer onto the crate

Behaviour-preserving. ai-layer's existing tests are the safety net — they must pass unchanged apart from the `AgentType` → `AgentRole` conversion.

**Files:**
- Modify: `services/ai-layer/Cargo.toml`
- Modify: `services/ai-layer/src/domain/entities/mod.rs`
- Delete: `services/ai-layer/src/infrastructure/claude/mod.rs`
- Delete: `services/ai-layer/src/application/agent/mod.rs`
- Modify: `services/ai-layer/src/infrastructure/tools/mod.rs`
- Modify: `services/ai-layer/src/infrastructure/db/mod.rs`
- Modify: `services/ai-layer/src/bootstrap.rs`

- [ ] **Step 1: Add the dependency**

In `services/ai-layer/Cargo.toml`, under `[dependencies]`:

```toml
logisticos-agent-runtime.workspace = true
```

Under `[dev-dependencies]`:

```toml
logisticos-agent-runtime = { workspace = true, features = ["testing"] }
```

Add to the root `Cargo.toml` `[workspace.dependencies]`:

```toml
logisticos-agent-runtime = { path = "libs/agent-runtime" }
```

- [ ] **Step 2: Write the failing conversion test**

Append to `services/ai-layer/src/domain/entities/mod.rs`'s existing `mod tests`:

```rust
    /// `AgentRole.key` is the persisted `agent_type` column value. If this
    /// drifts from the enum's serde representation, existing rows stop loading.
    #[test]
    fn agent_role_key_matches_the_persisted_enum_string() {
        for agent in [
            AgentType::Dispatch,
            AgentType::Recovery,
            AgentType::Reconciliation,
            AgentType::Anomaly,
            AgentType::MerchantSupport,
            AgentType::CustomerSupport,
            AgentType::OnDemand,
        ] {
            let serialised = serde_json::to_value(&agent).unwrap();
            let expected = serialised.as_str().expect("AgentType serialises to a string");
            let role: AgentRole = agent.clone().into();
            assert_eq!(role.key(), expected, "{agent:?} key must match its serde string");
        }
    }

    /// The customer-facing agent must stay the narrowest role on the platform
    /// after conversion — this is the same guarantee the pre-extraction test made.
    #[test]
    fn customer_support_role_stays_restricted_after_conversion() {
        let role: AgentRole = AgentType::CustomerSupport.into();
        for forbidden in [
            "assign_driver", "generate_invoice", "reconcile_cod", "send_driver_instruction",
            "get_cod_balance", "get_delivery_metrics", "get_driver_location", "get_churn_score",
            "schedule_dock", "get_available_drivers", "send_notification",
        ] {
            assert!(!role.permits(forbidden), "{forbidden} must not be reachable from customer chat");
        }
        for allowed in ["get_shipment", "reschedule_delivery", "escalate_to_human"] {
            assert!(role.permits(allowed), "{allowed} should be allowed");
        }
    }
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-ai-layer agent_role`
Expected: FAIL to compile — `cannot find type 'AgentRole' in this scope`.

- [ ] **Step 4: Write the conversion**

In `services/ai-layer/src/domain/entities/mod.rs`: keep the `AgentType` enum, its `display_name()`, `system_context()` and `allowed_tools()` exactly as they are. Delete the `AgentSession`, `SessionStatus`, `AgentMessage`, `MessageRole`, `AgentAction` and `ToolDefinition` definitions. Add at the top:

```rust
pub use logisticos_agent_runtime::{
    AgentAction, AgentMessage, AgentRole, AgentSession, MessageRole, SessionStatus, ToolDefinition,
};
```

And at the bottom, above `mod tests`:

```rust
impl From<AgentType> for AgentRole {
    fn from(t: AgentType) -> Self {
        // `key` must equal the enum's serde string — it is the persisted
        // `agent_type` column value. The test above enforces this.
        let key = serde_json::to_value(&t)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .expect("AgentType must serialise to a string");

        match t.allowed_tools() {
            None => AgentRole::unrestricted(key, t.display_name(), t.system_context()),
            Some(allowed) => {
                AgentRole::restricted(key, t.display_name(), t.system_context(), allowed.iter().copied())
            }
        }
    }
}
```

- [ ] **Step 5: Re-export the moved modules**

Replace the entire contents of `services/ai-layer/src/infrastructure/claude/mod.rs` with:

```rust
//! Moved to `logisticos-agent-runtime`. Re-exported so call sites keep working.
pub use logisticos_agent_runtime::claude::{
    extract_text, extract_tool_calls, ClaudeApi, ClaudeClient, ContentBlock, MessagesResponse, Usage,
};
```

Replace the entire contents of `services/ai-layer/src/application/agent/mod.rs` with:

```rust
//! Moved to `logisticos-agent-runtime`. Re-exported so call sites keep working.
pub use logisticos_agent_runtime::AgentRunner;
```

- [ ] **Step 6: Implement the traits on the concrete types**

In `services/ai-layer/src/infrastructure/tools/mod.rs`, keep `ToolRegistry` and its 21 tools; delete the local `ToolDefinition`, `ToolResult`, `ToolContext` definitions in favour of the crate's, and add:

```rust
use logisticos_agent_runtime::tools::{ToolBox, ToolContext, ToolDefinition, ToolResult};

#[async_trait::async_trait]
impl ToolBox for ToolRegistry {
    fn definitions(&self) -> &[ToolDefinition] {
        ToolRegistry::definitions(self)
    }

    async fn execute(
        &self,
        name: String,
        input: serde_json::Value,
        tool_use_id: String,
        ctx: ToolContext,
    ) -> ToolResult {
        ToolRegistry::execute(self, name, input, tool_use_id, ctx).await
    }
}
```

Delete `ToolRegistry::definitions_allowed` — the crate's `filter_definitions` replaces it.

In `services/ai-layer/src/infrastructure/db/mod.rs`, keep the `SessionRepository` trait and its dashboard methods, and add alongside it:

```rust
#[async_trait::async_trait]
impl logisticos_agent_runtime::SessionStore for PgSessionRepository {
    async fn save(&self, session: &AgentSession) -> anyhow::Result<()> {
        SessionRepository::save(self, session).await
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<AgentSession>> {
        SessionRepository::find_by_id(self, id).await
    }
}
```

In the row-mapping function, replace the `agent_type` deserialisation. The column still holds the same string; it now populates the role via the enum:

```rust
let agent_type_str: String = r.get("agent_type");
let agent_type: AgentType = serde_json::from_value(serde_json::Value::String(agent_type_str))?;
let role: AgentRole = agent_type.into();
```

and on write, `.bind(session.role.key())` replaces the serde round-trip.

- [ ] **Step 7: Update the bootstrap wiring**

In `services/ai-layer/src/bootstrap.rs`, `ClaudeClient::new` now takes three arguments. Read the model and token cap from config rather than hardcoding:

```rust
let claude = Arc::new(ClaudeClient::new(
    config.claude_api_key.clone(),
    config.claude_model.clone(),
    config.claude_max_tokens,
));
```

Add to `services/ai-layer/src/config.rs` alongside the existing Claude settings, with defaults preserving today's behaviour:

```rust
/// Model id. Defaults to the value the service used before this was configurable.
#[serde(default = "default_claude_model")]
pub claude_model: String,

/// Per-turn output cap. 4096 was the previous hardcoded value; raise it if
/// agents start truncating mid-answer.
#[serde(default = "default_claude_max_tokens")]
pub claude_max_tokens: u32,

fn default_claude_model() -> String { "claude-opus-4-6".to_string() }
fn default_claude_max_tokens() -> u32 { 4096 }
```

Every `AgentRunner` call site that passed an `AgentType` now passes `agent_type.into()`.

- [ ] **Step 8: Run the full ai-layer test suite**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-ai-layer`
Expected: PASS. The pre-existing RBAC tests (`customer_support_cannot_reach_operational_tools`, `customer_support_can_reach_its_three_tools`, `operational_agents_are_unrestricted`, `completing_an_escalation_keeps_the_escalation_reason`) must pass **unchanged** — they are the regression net for this refactor. Plus the 2 new conversion tests.

- [ ] **Step 9: Verify the whole workspace still type-checks**

Run: `CARGO_INCREMENTAL=0 cargo check --workspace`
Expected: PASS, no errors. If `link.exe` reports exit code 1318, that is disk exhaustion, not a code error — clear `C:\cargo-target-logisticos\debug\incremental` and re-run.

- [ ] **Step 10: Commit**

```bash
git add services/ai-layer/ Cargo.toml
git commit -m "refactor(ai-layer): consume logisticos-agent-runtime

AgentType stays as the logistics role enum and converts into the runtime's
AgentRole at session construction. The agent_type column value is unchanged —
AgentRole.key is asserted equal to the enum's serde string.

Model id and max_tokens move from two hardcoded constants into config; the
session's recorded model_used now comes from the client that was actually
called, so the audit trail cannot drift."
```

---

## Task 10: Enforce the boundary in CI

The crate is only reusable while nothing in it names a product concept. Make that mechanical.

**Files:**
- Create: `scripts/check-runtime-boundary.sh`
- Modify: `.github/workflows/ci-rust.yml`

- [ ] **Step 1: Write the check**

```bash
#!/usr/bin/env bash
# libs/agent-runtime must stay product-agnostic. If it names a product concept,
# it is no longer reusable by other products and the extraction has regressed.
set -euo pipefail

FORBIDDEN='shipment|driver|courier|vendor|merchant|dispatch|logistics|omnideliv|delivery|parcel'

# No exceptions — test fixtures use product-neutral vocabulary too
# (read_item / write_item / planner), so the whole of src/ is in scope.
if rg -i --type rust -n "$FORBIDDEN" libs/agent-runtime/src; then
  echo
  echo "ERROR: libs/agent-runtime references a product concept (matches above)."
  echo "The runtime must stay generic. Move product-specific code into the service."
  exit 1
fi

echo "OK: libs/agent-runtime is product-agnostic."
```

- [ ] **Step 2: Run it**

```bash
chmod +x scripts/check-runtime-boundary.sh && ./scripts/check-runtime-boundary.sh
```

Expected: `OK: libs/agent-runtime is product-agnostic.` If it fails, the offending line is printed — move that code into ai-layer rather than weakening the pattern.

- [ ] **Step 3: Wire into CI**

The Rust pipeline is `.github/workflows/ci-rust.yml`. Its `lint` job (line ~155, `name: Lint (fmt + clippy)`) is the right home — this is a lint, not a test, and the job needs no build cache to run it. Insert immediately after the `Checkout repository` step, before `Install Rust toolchain`, so the check fails fast without waiting on toolchain setup:

```yaml
      - name: Check agent-runtime boundary
        run: ./scripts/check-runtime-boundary.sh
```

`ripgrep` is preinstalled on `ubuntu-latest` GitHub runners, so no install step is needed.

- [ ] **Step 4: Verify the YAML still parses**

```bash
python -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci-rust.yml')); print('YAML OK')"
```

Expected: `YAML OK`

- [ ] **Step 5: Commit**

```bash
git add scripts/check-runtime-boundary.sh .github/workflows/ci-rust.yml
git commit -m "ci: fail the build if agent-runtime references a product concept"
```

---

## Definition of done

- [ ] `cargo test -p logisticos-agent-runtime --features testing` — 13 tests pass
- [ ] `cargo test -p logisticos-ai-layer` — all pre-existing tests pass unchanged, plus 2 new conversion tests
- [ ] `cargo check --workspace` — clean
- [ ] `./scripts/check-runtime-boundary.sh` — passes
- [ ] `services/ai-layer/src/application/agent/mod.rs` and `infrastructure/claude/mod.rs` contain only re-exports
- [ ] No occurrence of `"claude-opus-4-6"` outside `config.rs` (`rg -n 'claude-opus' --type rust` returns one hit)

## Follow-on work this unblocks

1. **Plan 2** — `parent_session_id` on `agent_sessions`, for mesh parent/child audit
2. **Plan 4** — the OmniDeliv mesh, which builds `MeshTransition` on top of `AgentRunner`
3. **Deferred, own decision:** bump `claude_model` to `claude-opus-5`. Now a config change plus prompt re-tuning — the code uses none of the parameters removed on newer models. Read `shared/model-migration.md` → Migrating to Claude Opus 5 before doing it; thinking is on by default there, which interacts with `claude_max_tokens`.
