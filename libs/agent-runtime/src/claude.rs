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
