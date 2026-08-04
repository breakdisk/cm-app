/// The core agentic loop — runs an agent session until completion or escalation.
///
/// Loop:
///   1. Build system prompt from AgentType
///   2. Send messages + tools to Claude
///   3. If Claude calls tools → execute them, append results, go to 2
///   4. If Claude returns end_turn text → session complete
///   5. If `escalate_to_human` tool called → session escalated
///   6. Persist session after every turn (crash recovery)
use std::sync::Arc;
use serde_json::{json, Value};
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::domain::entities::{
    AgentAction, AgentMessage, AgentSession, AgentType, MessageRole, SessionStatus,
};
use crate::infrastructure::claude::ContentBlock;
use crate::infrastructure::{
    claude::{ClaudeClient, ContentBlock as ClaudeContent},
    db::SessionRepository,
    tools::{ToolContext, ToolRegistry, ToolResult},
};

const MAX_TURNS: usize = 20;  // Hard cap on agentic loop iterations

pub struct AgentRunner {
    claude:    Arc<ClaudeClient>,
    tools:     Arc<ToolRegistry>,
    repo:      Arc<dyn SessionRepository>,
}

impl AgentRunner {
    pub fn new(
        claude: Arc<ClaudeClient>,
        tools:  Arc<ToolRegistry>,
        repo:   Arc<dyn SessionRepository>,
    ) -> Self {
        Self { claude, tools, repo }
    }

    /// Run a full agent session from trigger to completion.
    /// Returns the session id for status polling.
    ///
    /// Autonomous entry point: no human caller, so tools run without a bearer
    /// token and can only reach internal endpoints.
    pub async fn run(
        &self,
        tenant_id: TenantId,
        agent_type: AgentType,
        trigger: Value,
        initial_user_message: String,
    ) -> AppResult<AgentSession> {
        self.run_with_context(tenant_id, agent_type, trigger, initial_user_message, ToolContext::default())
            .await
    }

    /// Same as `run`, but tools execute with the supplied caller context — used
    /// by request-scoped surfaces (customer chat) so downstream services apply
    /// the caller's own authorisation.
    pub async fn run_with_context(
        &self,
        tenant_id: TenantId,
        agent_type: AgentType,
        trigger: Value,
        initial_user_message: String,
        ctx: ToolContext,
    ) -> AppResult<AgentSession> {
        let mut session = AgentSession::new(tenant_id, agent_type, trigger);
        self.repo.save(&session).await.map_err(AppError::internal)?;

        // Seed the conversation with the triggering event.
        session.messages.push(AgentMessage {
            role:    MessageRole::User,
            content: Value::String(initial_user_message),
        });

        self.drive(session, ctx).await
    }

    /// Append a new user turn to an existing session and run the loop again.
    ///
    /// This is what makes a *conversation* out of the one-shot agent runtime:
    /// the full prior message history (including tool calls and their results)
    /// is already on the session, so the agent keeps its context across turns.
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

    /// The agentic loop itself, shared by the fresh-run and resume paths.
    async fn drive(&self, mut session: AgentSession, ctx: ToolContext) -> AppResult<AgentSession> {
        let system = format!(
            "{}\n\nTenant context: tenant_id = {}",
            session.agent_type.system_context(),
            session.tenant_id.inner()
        );

        // A restricted agent is never told the other tools exist, and the
        // allowlist is re-checked before every execution below — Claude naming a
        // tool it was not offered must not be enough to run it.
        let allowed = session.agent_type.allowed_tools();
        let tools = self.tools.definitions_allowed(allowed);
        let mut turns = 0;

        loop {
            turns += 1;
            if turns > MAX_TURNS {
                session.escalate(format!("Agent exceeded {} turns without completing", MAX_TURNS));
                self.repo.save(&session).await.ok();
                return Ok(session);
            }

            let response = match self.claude.send(&system, &session.messages, &tools).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(session_id = %session.id, err = %e, "Claude API error");
                    session.fail(format!("Claude API error: {}", e));
                    self.repo.save(&session).await.ok();
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

            // Append Claude's response to message history (as assistant message).
            session.messages.push(AgentMessage {
                role:    MessageRole::Assistant,
                content: serde_json::to_value(&response.content).unwrap_or_default(),
            });

            // No tool calls → agent is done.
            if response.stop_reason == "end_turn" {
                let final_text = ClaudeClient::extract_text(&response);
                // Extract confidence if agent mentioned it (heuristic: look for "confidence: X%").
                let confidence = extract_confidence_from_text(&final_text);
                session.complete(final_text, confidence);
                self.repo.save(&session).await.ok();
                return Ok(session);
            }

            // Tool calls → execute each and collect results.
            let tool_calls = ClaudeClient::extract_tool_calls(&response);
            if tool_calls.is_empty() {
                session.complete(ClaudeClient::extract_text(&response), 0.9);
                self.repo.save(&session).await.ok();
                return Ok(session);
            }

            let mut tool_results: Vec<Value> = Vec::new();

            for block in tool_calls {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    // Authorisation gate — refuse anything outside this agent
                    // type's allowlist without touching the registry.
                    if let Some(allow) = allowed {
                        if !allow.contains(&name.as_str()) {
                            tracing::warn!(
                                session_id = %session.id,
                                agent_type = ?session.agent_type,
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
                    }

                    tracing::info!(
                        session_id = %session.id,
                        tool = %name,
                        "Executing tool"
                    );

                    let mut action = AgentAction::new(session.id, name.clone(), input.clone());
                    let result = self.tools.execute(name, input.clone(), id.clone(), ctx.clone()).await;

                    // Check for escalation signal.
                    if !result.is_error {
                        if result.content.get("__escalate").and_then(|v| v.as_bool()).unwrap_or(false) {
                            let reason = result.content["reason"].as_str().unwrap_or("Agent requested escalation").to_owned();
                            action.tool_result = Some(result.content.clone());
                            action.succeeded = true;
                            session.actions.push(action);
                            // Keep whatever the agent said to the customer in this
                            // turn — an escalated chat still needs a reply to show
                            // in the bubble, not an empty outcome.
                            let handoff_text = ClaudeClient::extract_text(&response);
                            if !handoff_text.trim().is_empty() {
                                session.outcome = Some(handoff_text);
                            }
                            session.escalate(reason);
                            self.repo.save(&session).await.ok();
                            return Ok(session);
                        }
                    }

                    action.tool_result = Some(result.content.clone());
                    action.succeeded = !result.is_error;
                    session.actions.push(action);

                    // Build tool_result message content block.
                    tool_results.push(json!({
                        "type":        "tool_result",
                        "tool_use_id": result.tool_use_id,
                        "content":     result.content.to_string(),
                        "is_error":    result.is_error,
                    }));
                }
            }

            // Append tool results back to conversation.
            session.messages.push(AgentMessage {
                role:    MessageRole::User,
                content: Value::Array(tool_results),
            });

            // Persist mid-session for crash recovery.
            self.repo.save(&session).await.ok();
        }
    }
}

fn extract_confidence_from_text(text: &str) -> f32 {
    // Heuristic: if the agent's final message contains "confidence: XX%" parse it.
    let lower = text.to_lowercase();
    if let Some(idx) = lower.find("confidence:") {
        let after = &lower[idx + 11..];
        let pct_str: String = after.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if let Ok(pct) = pct_str.trim().parse::<f32>() {
            return (pct / 100.0).clamp(0.0, 1.0);
        }
    }
    0.85 // Default if not stated
}
