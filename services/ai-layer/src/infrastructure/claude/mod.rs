/// Claude API client — wraps the Anthropic Messages API with tool use support.
///
/// Uses claude-opus-4-6 for all agentic reasoning. The agent loop:
///   1. Send messages + tool definitions to Claude
///   2. Claude returns tool_use block → execute the tool
///   3. Append tool_result to messages
///   4. Repeat until Claude returns a final text response (no more tool calls)
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::entities::{AgentMessage, MessageRole, ToolDefinition};

const CLAUDE_MODEL: &str = "claude-opus-4-6";
const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const MAX_TOKENS: u32 = 4096;

// ---------------------------------------------------------------------------
// Request / Response shapes matching the Anthropic API
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct MessagesRequest<'a> {
    model:      &'static str,
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

#[derive(Debug, Deserialize)]
pub struct MessagesResponse {
    pub id:           String,
    pub stop_reason:  String,   // "end_turn" | "tool_use"
    pub content:      Vec<ContentBlock>,
    pub usage:        Usage,
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

// ---------------------------------------------------------------------------
// Claude client
// ---------------------------------------------------------------------------

pub struct ClaudeClient {
    http:    reqwest::Client,
    api_key: String,
}

impl ClaudeClient {
    pub fn new(api_key: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build Claude HTTP client");
        Self { http, api_key }
    }

    /// Send a single agentic turn to Claude.
    /// Returns the response including any tool_use blocks.
    pub async fn send(
        &self,
        system: &str,
        messages: &[AgentMessage],
        tools: &[ToolDefinition],
    ) -> anyhow::Result<MessagesResponse> {
        let claude_messages: Vec<ClaudeMessage> = messages
            .iter()
            .map(|m| ClaudeMessage {
                role:    match m.role { MessageRole::User => "user", MessageRole::Assistant => "assistant" }.into(),
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
            model:      CLAUDE_MODEL,
            max_tokens: MAX_TOKENS,
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

    /// Convenience: extract the final text response from Claude's content blocks.
    pub fn extract_text(response: &MessagesResponse) -> String {
        response.content
            .iter()
            .filter_map(|b| if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Extract all tool_use blocks from a response.
    pub fn extract_tool_calls(response: &MessagesResponse) -> Vec<&ContentBlock> {
        response.content
            .iter()
            .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
            .collect()
    }
}
