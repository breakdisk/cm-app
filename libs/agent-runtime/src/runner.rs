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
